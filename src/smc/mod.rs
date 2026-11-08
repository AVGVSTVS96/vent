mod ffi;
mod key;
pub mod power;
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
const NOT_PRIVILEGED: i32 = 0xe000_02c1_u32.cast_signed();

#[derive(Debug)]
pub enum Error {
    Unavailable(&'static str),
    Kernel(i32),
    Smc { key: Key, code: u8 },
    NotFound(Key),
    PermissionDenied,
    Unsupported { key: Key, kind: Key },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(service) => write!(f, "{service} is unavailable"),
            Self::Kernel(code) => write!(f, "IOKit call failed ({code:#x})"),
            Self::Smc { key, code } => write!(f, "SMC rejected {key} ({code:#x})"),
            Self::NotFound(key) => write!(f, "SMC key {key} does not exist on this Mac"),
            Self::PermissionDenied => write!(f, "writing to the SMC needs root"),
            Self::Unsupported { key, kind } => {
                write!(
                    f,
                    "{key} holds a {kind:?} value, which vent can't read or write as a number"
                )
            }
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Copy)]
pub struct Info {
    pub size: u32,
    pub kind: Key,
}

pub fn is_laptop() -> bool {
    ffi::has_service(c"AppleSmartBattery")
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
                return Err(Error::Unavailable("AppleSMC"));
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
            NOT_PRIVILEGED => Err(Error::PermissionDenied),
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
            size: output.info.size,
            kind: Key::from(output.info.kind),
        })
    }

    pub fn read(&self, key: Key) -> Result<Value> {
        let info = self.info(key)?;
        let input = KeyData {
            key: key.into(),
            info: KeyInfo {
                size: info.size,
                ..KeyInfo::default()
            },
            command: READ,
            ..KeyData::default()
        };
        let output = self.checked(key, &input)?;
        let size = output.bytes.len().min(info.size as usize);
        Ok(Value::new(info.kind, &output.bytes[..size]))
    }

    pub fn number(&self, key: Key) -> Result<f64> {
        let value = self.read(key)?;
        value.number().ok_or(Error::Unsupported {
            key,
            kind: value.kind(),
        })
    }

    pub fn write(&self, key: Key, number: f64) -> Result<()> {
        let info = self.info(key)?;
        let bytes = Value::encode(info.kind, number)
            .filter(|bytes| bytes.len() == info.size as usize)
            .ok_or(Error::Unsupported {
                key,
                kind: info.kind,
            })?;
        let mut input = KeyData {
            key: key.into(),
            info: KeyInfo {
                size: info.size,
                ..KeyInfo::default()
            },
            command: WRITE,
            ..KeyData::default()
        };
        input.bytes[..bytes.len()].copy_from_slice(&bytes);
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

    /// The SMC indexes its keys in sorted order, so a prefix is one run found by binary search.
    pub fn keys(&self, prefix: &str) -> Result<Vec<Key>> {
        let count = self.key_count()?;
        let (mut low, mut high) = (0, count);
        while low < high {
            let middle = low + (high - low) / 2;
            if self.key_at(middle)?.as_str() < prefix {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        let mut keys = Vec::new();
        for index in low..count {
            let key = self.key_at(index)?;
            if !key.as_str().starts_with(prefix) {
                break;
            }
            keys.push(key);
        }
        Ok(keys)
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
