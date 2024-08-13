//! Android protocol specification:
//! https://android.googlesource.com/platform/system/core/+/master/fastboot/README.md

use std::io::{self, ErrorKind::TimedOut, Read, Result, Write};
use std::thread;
use std::time::{Duration, Instant};

use async_io::{block_on, Timer};
use futures_lite::FutureExt;
use nusb::{
    transfer::{Direction, EndpointType, RequestBuffer},
    DeviceInfo, Interface, Speed,
};

pub struct UsbDevice {
    i: Interface,
    bufsize: usize,
    e_in: u8,
    e_out: u8,
}

// this should be plenty
const POLL_DEV_TIMEOUT: Duration = Duration::from_secs(100);
// some devices only show up only briefly, so we have to be quick
const POLL_DEV_PERIOD: Duration = Duration::from_millis(1);

// TODO: VID/PID is tedious to figure out beforehand, and need not be unique.
// We may add another helper to scan for all available devices in fastboot mode.
// NOTE: The C fastboot CLI would just take the only fastboot device available,
// or ask to choose via its name.
pub fn poll_dev(vid: u16, pid: u16) -> std::result::Result<DeviceInfo, String> {
    let now = Instant::now();

    while Instant::now() <= now + POLL_DEV_TIMEOUT {
        match nusb::list_devices()
            .unwrap()
            .find(|d| d.vendor_id() == vid && d.product_id() == pid)
        {
            Some(di) => {
                return Ok(di);
            }
            None => {
                thread::sleep(POLL_DEV_PERIOD);
            }
        }
    }
    Err("timeout waiting for USB device".into())
}

// NOTE: Per spec, the max packet size (our read buffer size) must be
// - 64 bytes for full-speed
// - 512 bytes for high-speed
// - 1024 bytes for Super Speed USB.
impl UsbDevice {
    pub fn new(di: DeviceInfo) -> Self {
        // Just use the first interface - might need improvement
        let ii = di.interfaces().next().unwrap().interface_number();
        let d = di.open().unwrap();
        let i = d.claim_interface(ii).unwrap();

        let speed = di.speed().unwrap();
        let bufsize = match speed {
            Speed::Full | Speed::Low => 64,
            Speed::High => 512,
            Speed::Super | Speed::SuperPlus => 1024,
            _ => panic!("Unknown USB device speed {speed:?}"),
        };

        // Per spec, there must be two endpoints - bulk in and bulk out
        // TODO: Nice error messages when either is not found
        let c = d.configurations().next().unwrap();
        let s = c.interface_alt_settings().next().unwrap();
        let mut es = s
            .endpoints()
            .filter(|e| e.transfer_type() == EndpointType::Bulk);
        let e_in = es
            .find(|e| e.direction() == Direction::In)
            .unwrap()
            .address();
        let e_out = es
            .find(|e| e.direction() == Direction::Out)
            .unwrap()
            .address();

        UsbDevice {
            i,
            bufsize,
            e_in,
            e_out,
        }
    }
}

impl Read for UsbDevice {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        let timeout = Duration::from_secs(3);
        let fut = async {
            let b = RequestBuffer::new(self.bufsize);
            let comp = self.i.bulk_in(self.e_in, b).await;
            comp.status.map_err(io::Error::other)?;

            let n = comp.data.len();
            buf[..n].copy_from_slice(&comp.data);
            Ok(n)
        };

        block_on(fut.or(async {
            Timer::after(timeout).await;
            Err(TimedOut.into())
        }))
    }
}

impl Write for UsbDevice {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        let timeout = Duration::from_secs(3);
        let fut = async {
            let comp = self.i.bulk_out(self.e_out, buf.to_vec()).await;
            comp.status.map_err(io::Error::other)?;

            let n = comp.data.actual_length();
            Ok(n)
        };

        block_on(fut.or(async {
            Timer::after(timeout).await;
            Err(TimedOut.into())
        }))
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}
