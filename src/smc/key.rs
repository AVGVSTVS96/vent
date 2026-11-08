use std::fmt;
use std::str::FromStr;

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Key([u8; 4]);

impl Key {
    pub const fn new(code: [u8; 4]) -> Self {
        Self(code)
    }

    pub fn indexed(prefix: u8, index: u8, suffix: [u8; 2]) -> Self {
        Self([prefix, b'0' + index, suffix[0], suffix[1]])
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).unwrap_or("????")
    }
}

impl From<u32> for Key {
    fn from(code: u32) -> Self {
        Self(code.to_be_bytes())
    }
}

impl From<Key> for u32 {
    fn from(key: Key) -> Self {
        u32::from_be_bytes(key.0)
    }
}

impl FromStr for Key {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let bytes: [u8; 4] = text
            .as_bytes()
            .try_into()
            .map_err(|_| format!("an SMC key is 4 characters, got {text:?}"))?;
        Ok(Self(bytes))
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Key({self})")
    }
}
