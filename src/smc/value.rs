use std::fmt;

use super::Key;

#[derive(Clone, Debug)]
pub struct Value {
    kind: Key,
    bytes: Vec<u8>,
}

impl Value {
    pub fn new(kind: Key, bytes: &[u8]) -> Self {
        Self {
            kind,
            bytes: bytes.to_vec(),
        }
    }

    pub fn kind(&self) -> Key {
        self.kind
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn number(&self) -> Option<f64> {
        let b = self.bytes.as_slice();
        Some(match (self.kind.as_str(), b.len()) {
            ("flt ", 4) => f64::from(f32::from_le_bytes(b.try_into().ok()?)),
            ("ui8 " | "flag", 1) => f64::from(b[0]),
            ("si8 ", 1) => f64::from(i8::from_be_bytes([b[0]])),
            ("ui16", 2) => f64::from(u16::from_be_bytes(b.try_into().ok()?)),
            ("si16", 2) => f64::from(i16::from_be_bytes(b.try_into().ok()?)),
            ("ui32", 4) => f64::from(u32::from_be_bytes(b.try_into().ok()?)),
            ("sp78", 2) => f64::from(i16::from_be_bytes(b.try_into().ok()?)) / 256.0,
            ("fpe2", 2) => f64::from(u16::from_be_bytes(b.try_into().ok()?)) / 4.0,
            ("ioft", 8) => u64::from_le_bytes(b.try_into().ok()?) as f64 / 65536.0,
            _ => return None,
        })
    }

    pub fn encode(kind: Key, number: f64) -> Option<Vec<u8>> {
        Some(match kind.as_str() {
            #[allow(clippy::cast_possible_truncation)]
            "flt " => (number as f32).to_le_bytes().to_vec(),
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            "ui8 " | "flag" => vec![number as u8],
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            "ui16" => (number as u16).to_be_bytes().to_vec(),
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            "fpe2" => ((number * 4.0) as u16).to_be_bytes().to_vec(),
            _ => return None,
        })
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(number) = self.number() {
            return write!(f, "{number}");
        }
        for byte in &self.bytes {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}
