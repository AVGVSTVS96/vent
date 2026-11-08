use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;
use std::{fs, io};

use crate::fan::Speed;

pub const PATH: &str = "/Library/Application Support/vent/hold";

/// The speed vent keeps each fan at. A fan not in the hold belongs to the system.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Hold(BTreeMap<String, Speed>);

impl Hold {
    pub fn get(&self, fan: &str) -> Option<Speed> {
        self.0.get(fan).copied()
    }

    pub fn set(&mut self, fan: &str, speed: Speed) {
        if speed == Speed::Auto {
            self.0.remove(fan);
        } else {
            self.0.insert(fan.to_owned(), speed);
        }
    }

    pub fn fans(&self) -> impl Iterator<Item = (&str, Speed)> {
        self.0.iter().map(|(fan, speed)| (fan.as_str(), *speed))
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

impl FromStr for Hold {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let mut hold = Self::default();
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            let (fan, speed) = line
                .split_once(' ')
                .ok_or_else(|| format!("{line:?} is not a fan and a speed"))?;
            hold.set(fan, speed.parse()?);
        }
        Ok(hold)
    }
}

impl fmt::Display for Hold {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fans()
            .try_for_each(|(fan, speed)| writeln!(f, "{fan} {speed}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holds_round_trip_through_text() {
        let mut hold = Hold::default();
        hold.set("right", Speed::Max);
        hold.set("left", Speed::Rpm(1500.0));
        assert_eq!(hold.to_string(), "left 1500\nright max\n");
        assert_eq!(hold.to_string().parse::<Hold>(), Ok(hold));
    }

    #[test]
    fn auto_releases_a_fan() {
        let mut hold: Hold = "right max\n".parse().unwrap();
        hold.set("right", Speed::Auto);
        assert_eq!(hold, Hold::default());
    }
}
