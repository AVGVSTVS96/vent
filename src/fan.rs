use std::fmt;
use std::str::FromStr;

use crate::smc::{self, Key, Smc};

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
                "{text:?} is not a speed: use auto, min, max, an rpm like 3000, or a percent like 80%"
            )
        };
        let speed = match text.as_str() {
            "auto" => Self::Auto,
            "min" => Self::Min,
            "max" => Self::Max,
            _ => match text.strip_suffix('%') {
                Some(percent) => Self::Percent(percent.parse().map_err(|_| invalid())?),
                None => Self::Rpm(
                    text.trim_end_matches("rpm")
                        .trim()
                        .parse()
                        .map_err(|_| invalid())?,
                ),
            },
        };
        match speed {
            Self::Percent(p) if !(0.0..=100.0).contains(&p) => Err(invalid()),
            Self::Rpm(r) if !r.is_finite() || r < 0.0 => Err(invalid()),
            _ => Ok(speed),
        }
    }
}

impl fmt::Display for Speed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Auto => f.write_str("auto"),
            Self::Min => f.write_str("min"),
            Self::Max => f.write_str("max"),
            Self::Rpm(rpm) => write!(f, "{rpm:.0}"),
            Self::Percent(percent) => write!(f, "{percent}%"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Fan {
    pub index: u8,
    pub name: String,
    pub actual: f64,
    pub target: f64,
    pub min: f64,
    pub max: f64,
    pub manual: bool,
}

impl Fan {
    pub fn rpm_for(&self, speed: Speed) -> Option<f64> {
        let rpm = match speed {
            Speed::Auto => return None,
            Speed::Min => self.min,
            Speed::Max => self.max,
            Speed::Rpm(rpm) => rpm,
            Speed::Percent(percent) => self.min + (self.max - self.min) * percent / 100.0,
        };
        Some(rpm.max(self.min).min(self.max))
    }
}

/// Fans on a two-fan laptop are named for the side they sit on, as seen from the keyboard.
pub fn name(index: u8, count: u8, laptop: bool) -> String {
    match (laptop, count, index) {
        (true, 2, 0) => "left".into(),
        (true, 2, 1) => "right".into(),
        _ => format!("fan{index}"),
    }
}

pub struct Fans<'a> {
    smc: &'a Smc,
}

impl<'a> Fans<'a> {
    pub fn new(smc: &'a Smc) -> Self {
        Self { smc }
    }

    fn key(index: u8, suffix: [u8; 2]) -> Key {
        Key::indexed(b'F', index, suffix)
    }

    pub fn count(&self) -> smc::Result<u8> {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Ok(self.smc.number(Key::new(*b"FNum"))? as u8)
    }

    pub fn all(&self) -> smc::Result<Vec<Fan>> {
        let count = self.count()?;
        let laptop = smc::is_laptop();
        (0..count)
            .map(|index| {
                Ok(Fan {
                    index,
                    name: name(index, count, laptop),
                    actual: self.smc.number(Self::key(index, *b"Ac"))?,
                    target: self.smc.number(Self::key(index, *b"Tg"))?,
                    min: self.smc.number(Self::key(index, *b"Mn"))?,
                    max: self.smc.number(Self::key(index, *b"Mx"))?,
                    manual: self.smc.number(Self::key(index, *b"Md"))? >= 1.0,
                })
            })
            .collect()
    }

    pub fn set(&self, fan: &Fan, speed: Speed) -> smc::Result<()> {
        match fan.rpm_for(speed) {
            None => self.release(fan.index),
            Some(rpm) => {
                self.smc.write(Self::key(fan.index, *b"Md"), 1.0)?;
                self.smc.write(Self::key(fan.index, *b"Tg"), rpm)
            }
        }
    }

    /// Hands one fan back to the system, needing nothing read from it first.
    pub fn release(&self, index: u8) -> smc::Result<()> {
        self.smc.write(Self::key(index, *b"Md"), 0.0)
    }

    /// Hands every fan back, trying each even if one fails.
    pub fn release_all(&self) -> smc::Result<()> {
        (0..self.count()?)
            .map(|index| self.release(index))
            .fold(Ok(()), Result::and)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fan(manual: bool, target: f64) -> Fan {
        Fan {
            index: 1,
            name: "right".into(),
            actual: target,
            target,
            min: 1500.0,
            max: 5800.0,
            manual,
        }
    }

    #[test]
    fn speeds_parse_and_print_back() {
        for text in ["auto", "min", "max", "3000", "80%", "12.5%"] {
            assert_eq!(text.parse::<Speed>().unwrap().to_string(), text);
        }
        assert_eq!("3000 RPM".parse::<Speed>(), Ok(Speed::Rpm(3000.0)));
        for bad in ["", "fast", "101%", "-5", "nan"] {
            assert!(bad.parse::<Speed>().is_err(), "{bad} parsed");
        }
    }

    #[test]
    fn speeds_resolve_within_the_fan_range() {
        let fan = fan(false, 0.0);
        assert_eq!(fan.rpm_for(Speed::Auto), None);
        assert_eq!(fan.rpm_for(Speed::Max), Some(5800.0));
        assert_eq!(fan.rpm_for(Speed::Rpm(9000.0)), Some(5800.0));
        assert_eq!(fan.rpm_for(Speed::Rpm(100.0)), Some(1500.0));
        assert_eq!(fan.rpm_for(Speed::Percent(50.0)), Some(3650.0));
    }

    #[test]
    fn odd_fan_ranges_never_panic() {
        let backwards = Fan {
            min: 5000.0,
            max: 1000.0,
            ..fan(false, 0.0)
        };
        assert_eq!(backwards.rpm_for(Speed::Rpm(3000.0)), Some(1000.0));
        let unknown = Fan {
            min: f64::NAN,
            max: f64::NAN,
            ..fan(false, 0.0)
        };
        assert!(unknown.rpm_for(Speed::Max).is_some());
    }

    #[test]
    fn only_two_fan_laptops_get_sides() {
        assert_eq!(name(1, 2, true), "right");
        assert_eq!(name(1, 2, false), "fan1");
        assert_eq!(name(0, 1, true), "fan0");
    }
}
