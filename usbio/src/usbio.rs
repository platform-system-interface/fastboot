//! Android protocol specification:
//! https://android.googlesource.com/platform/system/core/+/master/fastboot/README.md

use std::io::{self, ErrorKind::TimedOut, Read, Result, Write};
use std::time::Duration;

use async_io::{block_on, Timer};
use futures_lite::FutureExt;
use nusb::{transfer::RequestBuffer, DeviceInfo, Interface, Speed};

pub struct UsbDevice {
    i: Interface,
    bufsize: usize,
}

// TODO: Is it always those two?
// Per spec, there must be two endpoints - bulk in and bulk out
const ENDPOINT_OUT: u8 = 0x02;
const ENDPOINT_IN: u8 = 0x81;

// NOTE: Per spec, the max packet size (our read buffer size) must be
// - 64 bytes for full-speed
// - 512 bytes for high-speed
// - 1024 bytes for Super Speed USB.
impl UsbDevice {
    pub fn new(di: DeviceInfo) -> Self {
        let speed = di.speed().unwrap();
        let bufsize = match speed {
            Speed::Full | Speed::Low => 64,
            Speed::High => 512,
            Speed::Super | Speed::SuperPlus => 1024,
            _ => panic!("Unknown USB device speed {speed:?}"),
        };
        let d = di.open().unwrap();
        // TODO: may need to use a different interface
        let i = d.claim_interface(0).unwrap();
        UsbDevice { i, bufsize }
    }
}

impl Read for UsbDevice {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        let endpoint = ENDPOINT_IN;
        let timeout = Duration::from_secs(3);

        let fut = async {
            let b = RequestBuffer::new(self.bufsize);
            let comp = self.i.bulk_in(endpoint, b).await;
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

        let endpoint = ENDPOINT_OUT;
        let timeout = Duration::from_secs(3);

        let fut = async {
            let comp = self.i.bulk_out(endpoint, buf.to_vec()).await;
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
