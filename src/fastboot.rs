//! Traits, helpers, and type definitions for Fastboot host functionality.

use std;
use std::io::{Read, Write};

/// Result wrapper that yields either a succesful result of a Fastboot operation
/// or an error [`String`].
pub type FbResult<T> = Result<T, String>;

const GETVAR_CMD: &[u8] = b"getvar:";
const DOWNLOAD_CMD: &[u8] = b"download:";
const FLASH_CMD: &[u8] = b"flash:";
const ERASE_CMD: &[u8] = b"erase:";
const CONTINUE_CMD: &[u8] = b"continue";
const REBOOT_CMD: &[u8] = b"reboot";
const REBOOT_BOOTLOADER_CMD: &[u8] = b"reboot-bootloader";

#[derive(Debug, Clone)]
enum Reply {
    Okay(String),
    Data(usize),
    Fail(String),
    Info(String),
}

impl<'s> From<&'s mut [u8]> for Reply {
    fn from(reply: &'s mut [u8]) -> Self {
        // Split a reply at OKAY/INFO/FAIL/DATA
        let (kind, data) = reply.split_at(4);
        let s = String::from_utf8_lossy(data);
        match kind {
            b"OKAY" => Reply::Okay(s.into_owned()),
            b"INFO" => Reply::Info(s.into_owned()),
            b"FAIL" => Reply::Fail(s.into_owned()),
            b"DATA" => {
                // Remove the null bytes that were in the buffer.
                // Parsing the number would otherwise fail.
                let d = s.trim_matches(char::from(0));
                match usize::from_str_radix(d, 16) {
                    Ok(size) => Reply::Data(size),
                    _ => Reply::Fail("DATA: Failed to decode size".to_owned()),
                }
            }
            _ => {
                eprintln!("Received: {kind:08x?}: {s}");
                Reply::Fail(s.into_owned())
            }
        }
    }
}

// NOTE: The real buf size is to be handled in the usbio crate, since it depends
// on the USB port's speed. This is the overall maximum. Might need rework.
const FB_MAX_REPLY_LEN: usize = 512;

// According to the spec and the U-Boot documentation, Fastboot is a synchronous
// protocol. Therefore we should always wait for a reply to our "request".
// This function will block until a reply or an error (except timeout) is
// received from the USB I/O implementation.
// See u-boot/doc/README.android-fastboot-protocol
fn fb_send<T: Fastboot>(io: &mut T, payload: &[u8]) -> FbResult<Reply> {
    io.write_all(payload).map_err(|err| err.to_string())?;
    loop {
        let mut buff = [0; FB_MAX_REPLY_LEN];
        match io.read(&mut buff) {
            Ok(received) => return Ok(Reply::from(&mut buff[..received])),
            Err(err) => {
                match err.kind() {
                    std::io::ErrorKind::TimedOut => {
                        // Trait can't possible now what is a timeout set by a
                        // particular Read/Write implementation so it will *not*
                        // consider TimedOut a fatal error. Instead it will just
                        // try again until a reply or another error is received.
                        continue;
                    }
                    _ => {
                        return Err(err.to_string());
                    }
                };
            }
        };
    }
}

/// The `Fastboot` trait provides Fastboot-protocol host-side interface.
///
/// There are no required methods. The only requirement is that an object,
/// implementing this trait implements also [`Read`], [`Write`] and [`Sized`] traits.
pub trait Fastboot: Read + Write + Sized {
    /// Gets a Fastboot variable.
    ///
    /// NOTE: Fastboot variables aren't U-Boot environment variables.
    fn getvar(&mut self, var: &str) -> FbResult<String> {
        let mut cmd = Vec::with_capacity(GETVAR_CMD.len() + var.len());
        cmd.extend_from_slice(GETVAR_CMD);
        cmd.extend_from_slice(var.as_bytes());
        let reply = fb_send(self, &cmd)?;
        match reply {
            Reply::Okay(variable) => Ok(variable),
            Reply::Fail(message) => Err(message),
            _ => Err("Unknown failure".to_owned()),
        }
    }

    /// Downloads provided data into a client.
    fn download(&mut self, data: &[u8]) -> FbResult<()> {
        // Wrapped in block to drop len as soon as possible
        let cmd = {
            let mut cmd = Vec::with_capacity(DOWNLOAD_CMD.len() + 8);
            let mut len = format!("{:08x}", data.len()).into_bytes();
            cmd.extend_from_slice(DOWNLOAD_CMD);
            cmd.append(&mut len);
            cmd
        };
        let reply = fb_send(self, &cmd)?;

        match reply {
            Reply::Data(size) if size == data.len() => {
                let reply = fb_send(self, data)?;
                match reply {
                    Reply::Okay(_) => Ok(()),
                    Reply::Fail(message) => Err(message),
                    Reply::Info(message) => {
                        println!("{message}");
                        Err(message)
                    }
                    _ => Err("Unknown failure".to_owned()),
                }
            }
            Reply::Fail(message) => Err(message),
            _ => Err("Unknown failure".to_owned()),
        }
    }

    /// Flashes downloaded data into a specified partition.
    fn flash(&mut self, partition: &str) -> FbResult<()> {
        let mut cmd = Vec::with_capacity(FLASH_CMD.len() + partition.len());
        cmd.extend_from_slice(FLASH_CMD);
        cmd.extend_from_slice(partition.as_bytes());
        let reply = fb_send(self, &cmd)?;
        match reply {
            Reply::Okay(_) => Ok(()),
            Reply::Fail(message) => Err(message),
            Reply::Info(message) => {
                println!("{message}");
                Err(message)
            }
            _ => Err("Unknown failure".to_owned()),
        }
    }

    /// Erases a specified partition.
    fn erase(&mut self, partition: &str) -> FbResult<()> {
        let mut cmd = Vec::with_capacity(ERASE_CMD.len() + partition.len());
        cmd.extend_from_slice(ERASE_CMD);
        cmd.extend_from_slice(partition.as_bytes());
        let reply = fb_send(self, &cmd)?;
        match reply {
            Reply::Okay(_) => Ok(()),
            Reply::Fail(message) => Err(message),
            _ => Err("Unknown failure".to_owned()),
        }
    }

    /// Continue booting as normal (if possible).
    /// NOTE: We cannot call this `continue` because of Rust syntax.
    fn continue_boot(&mut self) -> FbResult<()> {
        let reply = fb_send(self, CONTINUE_CMD)?;
        match reply {
            Reply::Okay(_) => Ok(()),
            Reply::Fail(message) => Err(message),
            _ => Err("Unknown failure".to_owned()),
        }
    }

    /// Reboots a client.
    fn reboot(&mut self) -> FbResult<()> {
        let reply = fb_send(self, REBOOT_CMD)?;
        match reply {
            Reply::Okay(_) => Ok(()),
            Reply::Fail(message) => Err(message),
            _ => Err("Unknown failure".to_owned()),
        }
    }

    /// Reboots a client.
    fn reboot_bootloader(&mut self) -> FbResult<()> {
        let reply = fb_send(self, REBOOT_BOOTLOADER_CMD)?;
        match reply {
            Reply::Okay(_) => Ok(()),
            Reply::Fail(message) => Err(message),
            _ => Err("Unknown failure".to_owned()),
        }
    }
}

// TODO: not sure if it's a right way to do things
// but I would like to avoid implementing a newtype
// workaround for every suitable type that wants to
// use this trait
impl<T: Read + Write + Sized> Fastboot for T {}
