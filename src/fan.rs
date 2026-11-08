use std::fmt;
use std::str::FromStr;

use crate::smc::{self, Key, Smc, Value};

const MANUAL: f64 = 1.0;
const AUTO: f64 = 0.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Speed {
    Auto,
    Min,
    Max,
    Rpm(f64),
    Percent(f64),
}

impl FromStr for Speed {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let text = text.trim().to_ascii_lowercase();
        let invalid = || {
            format!(
                "{text:?} is not a speed: use auto, min, max, an RPM like 3000, or a percent like 80%"
            )
        };
        match text.as_str() {
            "auto" => Ok(Self::Auto),
            "min" => Ok(Self::Min),
            "max" => Ok(Self::Max),
            _ => match text.strip_suffix('%') {
                Some(percent) => {
                    let percent: f64 = percent.parse().map_err(|_| invalid())?;
                    if (0.0..=100.0).contains(&percent) {
                        Ok(Self::Percent(percent))
                    } else {
                        Err(invalid())
                    }
                }
                None => text
                    .trim_end_matches("rpm")
                    .parse()
                    .map(Self::Rpm)
                    .map_err(|_| invalid()),
            },
        }
    }
}

impl fmt::Display for Speed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Auto => f.write_str("auto"),
            Self::Min => f.write_str("min"),
            Self::Max => f.write_str("max"),
            Self::Rpm(rpm) => write!(f, "{rpm:.0} rpm"),
            Self::Percent(percent) => write!(f, "{percent}%"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Fan {
    pub index: u8,
    pub name: &'static str,
    pub actual: f64,
    pub target: f64,
    pub min: f64,
    pub max: f64,
    pub manual: bool,
}

impl Fan {
    fn key(index: u8, suffix: &[u8; 2]) -> Key {
        Key::indexed(b'F', index, *suffix)
    }

    pub fn rpm_for(&self, speed: Speed) -> Option<f64> {
        match speed {
            Speed::Auto => None,
            Speed::Min => Some(self.min),
            Speed::Max => Some(self.max),
            Speed::Rpm(rpm) => Some(rpm.clamp(self.min, self.max)),
            Speed::Percent(percent) => Some(self.min + (self.max - self.min) * percent / 100.0),
        }
    }
}

pub struct Fans<'a> {
    smc: &'a Smc,
}

impl<'a> Fans<'a> {
    pub fn new(smc: &'a Smc) -> Self {
        Self { smc }
    }

    fn number(&self, key: Key) -> smc::Result<f64> {
        let value = self.smc.read(key)?;
        Ok(value.number().unwrap_or_default())
    }

    fn write(&self, key: Key, number: f64) -> smc::Result<()> {
        let kind = self.smc.info(key)?.kind;
        let bytes = Value::encode(kind, number).unwrap_or_default();
        self.smc.write(key, &bytes)
    }

    pub fn count(&self) -> smc::Result<u8> {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Ok(self.number(Key::new(*b"FNum"))? as u8)
    }

    pub fn get(&self, index: u8) -> smc::Result<Fan> {
        let count = self.count()?;
        Ok(Fan {
            index,
            name: name(index, count),
            actual: self.number(Fan::key(index, b"Ac"))?,
            target: self.number(Fan::key(index, b"Tg"))?,
            min: self.number(Fan::key(index, b"Mn"))?,
            max: self.number(Fan::key(index, b"Mx"))?,
            manual: self.number(Fan::key(index, b"Md"))? >= MANUAL,
        })
    }

    pub fn all(&self) -> smc::Result<Vec<Fan>> {
        (0..self.count()?).map(|index| self.get(index)).collect()
    }

    pub fn find(&self, name_or_index: &str) -> smc::Result<Option<Fan>> {
        let wanted = name_or_index.to_ascii_lowercase();
        Ok(self
            .all()?
            .into_iter()
            .find(|fan| fan.name == wanted || fan.index.to_string() == wanted))
    }

    pub fn set(&self, fan: &Fan, speed: Speed) -> smc::Result<()> {
        match fan.rpm_for(speed) {
            None => self.write(Fan::key(fan.index, b"Md"), AUTO),
            Some(rpm) => {
                self.write(Fan::key(fan.index, b"Md"), MANUAL)?;
                self.write(Fan::key(fan.index, b"Tg"), rpm)
            }
        }
    }
}

fn name(index: u8, count: u8) -> &'static str {
    match (count, index) {
        (2, 0) => "left",
        (2, 1) => "right",
        (_, 0) => "0",
        (_, 1) => "1",
        (_, 2) => "2",
        _ => "3",
    }
}
