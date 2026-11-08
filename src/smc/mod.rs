mod ffi;
mod key;
mod value;

use std::ffi::c_void;
use std::fmt;

pub use key::Key;
pub use value::Value;

use ffi::{KeyData, KeyInfo};

const SELECTOR: u32 = 2;
const READ: u8 = 5;
const WRITE: u8 = 6;
const KEY_AT_INDEX: u8 = 8;
const KEY_INFO: u8 = 9;
const KEY_NOT_FOUND: u8 = 0x84;

#[derive(Debug)]
pub enum Error {
    Unavailable,
    Kernel(i32),
    Smc {
        key: Key,
        code: u8,
    },
    NotFound(Key),
    PermissionDenied,
    Size {
        key: Key,
        expected: usize,
        got: usize,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => write!(f, "AppleSMC service not found"),
            Self::Kernel(code) => write!(f, "IOKit call failed ({code:#x})"),
            Self::Smc { key, code } => write!(f, "SMC rejected {key} ({code:#x})"),
            Self::NotFound(key) => write!(f, "SMC key {key} does not exist on this Mac"),
            Self::PermissionDenied => write!(f, "writing to the SMC needs root"),
            Self::Size { key, expected, got } => {
                write!(f, "{key} takes {expected} bytes, got {got}")
            }
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Copy)]
pub struct Info {
    pub size: usize,
    pub kind: Key,
}

pub struct Smc {
    connection: ffi::IoConnect,
}

impl Smc {
    pub fn open() -> Result<Self> {
        // SAFETY: plain IOKit calls; the matching dictionary is consumed by
        // IOServiceGetMatchingService and the service is released after opening.
        unsafe {
            let matching = ffi::IOServiceMatching(c"AppleSMC".as_ptr());
            let service = ffi::IOServiceGetMatchingService(ffi::MAIN_PORT_DEFAULT, matching);
            if service == 0 {
                return Err(Error::Unavailable);
            }
            let mut connection = 0;
            let status = ffi::IOServiceOpen(service, ffi::task_self(), 0, &raw mut connection);
            ffi::IOObjectRelease(service);
            if status != ffi::KERN_SUCCESS {
                return Err(Error::Kernel(status));
            }
            Ok(Self { connection })
        }
    }

    fn call(&self, input: &KeyData) -> Result<KeyData> {
        let mut output = KeyData::default();
        let mut size = size_of::<KeyData>();
        // SAFETY: both buffers are valid KeyData structs whose layout matches the
        // AppleSMC user client's 80-byte struct.
        let status = unsafe {
            ffi::IOConnectCallStructMethod(
                self.connection,
                SELECTOR,
                std::ptr::from_ref(input).cast::<c_void>(),
                size_of::<KeyData>(),
                (&raw mut output).cast::<c_void>(),
                &raw mut size,
            )
        };
        match status {
            ffi::KERN_SUCCESS => Ok(output),
            // kIOReturnNotPrivileged
            -536_870_207 => Err(Error::PermissionDenied),
            code => Err(Error::Kernel(code)),
        }
    }

    fn checked(&self, key: Key, input: &KeyData) -> Result<KeyData> {
        let output = self.call(input)?;
        match output.result {
            0 => Ok(output),
            KEY_NOT_FOUND => Err(Error::NotFound(key)),
            code => Err(Error::Smc { key, code }),
        }
    }

    pub fn info(&self, key: Key) -> Result<Info> {
        let input = KeyData {
            key: key.into(),
            command: KEY_INFO,
            ..KeyData::default()
        };
        let output = self.checked(key, &input)?;
        Ok(Info {
            size: output.info.size as usize,
            kind: Key::from(output.info.kind),
        })
    }

    pub fn read(&self, key: Key) -> Result<Value> {
        let info = self.info(key)?;
        let input = KeyData {
            key: key.into(),
            info: KeyInfo {
                size: u32::try_from(info.size).unwrap_or(32),
                ..KeyInfo::default()
            },
            command: READ,
            ..KeyData::default()
        };
        let output = self.checked(key, &input)?;
        Ok(Value::new(info.kind, &output.bytes[..info.size.min(32)]))
    }

    pub fn write(&self, key: Key, bytes: &[u8]) -> Result<()> {
        let info = self.info(key)?;
        if bytes.len() != info.size {
            return Err(Error::Size {
                key,
                expected: info.size,
                got: bytes.len(),
            });
        }
        let mut input = KeyData {
            key: key.into(),
            info: KeyInfo {
                size: u32::try_from(info.size).unwrap_or(32),
                ..KeyInfo::default()
            },
            command: WRITE,
            ..KeyData::default()
        };
        input.bytes[..bytes.len()].copy_from_slice(bytes);
        self.checked(key, &input).map(|_| ())
    }

    pub fn key_count(&self) -> Result<u32> {
        let value = self.read(Key::new(*b"#KEY"))?;
        Ok(u32::from_be_bytes(
            value.bytes().try_into().unwrap_or_default(),
        ))
    }

    pub fn key_at(&self, index: u32) -> Result<Key> {
        let input = KeyData {
            data32: index,
            command: KEY_AT_INDEX,
            ..KeyData::default()
        };
        let output = self.checked(Key::new(*b"#KEY"), &input)?;
        Ok(Key::from(output.key))
    }

    pub fn keys(&self) -> Result<Vec<Key>> {
        (0..self.key_count()?)
            .map(|index| self.key_at(index))
            .collect()
    }
}

impl Drop for Smc {
    fn drop(&mut self) {
        // SAFETY: the connection was opened by IOServiceOpen and is closed once.
        unsafe {
            ffi::IOServiceClose(self.connection);
        }
    }
}
