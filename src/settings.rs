use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;
use std::{fs, io};

use crate::curve::Curve;
use crate::fan::Speed;

pub const PATH: &str = "/Library/Application Support/vent/settings";

/// What vent does with each fan, and the curve auto fans follow. A fan not
/// listed is auto.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Settings {
    fans: BTreeMap<String, Speed>,
    pub curve: Curve,
}

impl Settings {
    pub fn speed(&self, fan: &str) -> Speed {
        self.fans.get(fan).copied().unwrap_or(Speed::Auto)
    }

    pub fn set(&mut self, fan: &str, speed: Speed) {
        if speed == Speed::Auto {
            self.fans.remove(fan);
        } else {
            self.fans.insert(fan.to_owned(), speed);
        }
    }

    pub fn all_auto(&mut self) {
        self.fans.clear();
    }

    pub fn load() -> io::Result<Self> {
        match fs::read_to_string(PATH) {
            Ok(text) => text
                .parse()
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error),
        }
    }

    pub fn save(&self) -> io::Result<()> {
        let path = std::path::Path::new(PATH);
        fs::create_dir_all(path.parent().unwrap_or(path))?;
        let draft = path.with_extension("new");
        fs::write(&draft, self.to_string())?;
        fs::rename(draft, path)
    }
}

impl FromStr for Settings {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let mut settings = Self::default();
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            match line.split_once(' ') {
                Some(("curve", curve)) => settings.curve = curve.parse()?,
                Some((fan, speed)) => settings.set(fan, speed.parse()?),
                None => return Err(format!("{line:?} is not a fan and a speed")),
            }
        }
        Ok(settings)
    }
}

impl fmt::Display for Settings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "curve {}", self.curve)?;
        self.fans
            .iter()
            .try_for_each(|(fan, speed)| writeln!(f, "{fan} {speed}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_through_text() {
        let mut settings = Settings {
            curve: Curve::new(55.0, 90.0).unwrap(),
            ..Settings::default()
        };
        settings.set("right", Speed::Max);
        settings.set("left", Speed::System);
        assert_eq!(
            settings.to_string(),
            "curve 55 90\nleft system\nright max\n"
        );
        assert_eq!(settings.to_string().parse::<Settings>(), Ok(settings));
    }

    #[test]
    fn unlisted_fans_are_auto() {
        let mut settings: Settings = "right max\n".parse().unwrap();
        assert_eq!(settings.curve, Curve::default());
        assert_eq!(settings.speed("left"), Speed::Auto);
        settings.set("right", Speed::Auto);
        assert_eq!(settings, Settings::default());
    }
}
