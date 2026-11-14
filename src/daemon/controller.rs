use std::collections::BTreeSet;

use crate::curve::Curve;
use crate::fan::{Fan, Fans, Speed};
use crate::protocol::{Request, Status};
use crate::settings::Settings;
use crate::smc::{self, Smc};
use crate::temps::{self, Sensors};

/// The share of each new reading the smoothed heat takes in, once a second: it
/// catches up with a change in about ten seconds, so a short burst doesn't spin the fans up.
const SMOOTHING: f64 = 0.1;
/// Auto fans go back to macOS only this many degrees below the curve's start, so
/// heat hovering at the start doesn't flip them back and forth.
const HYSTERESIS: f64 = 3.0;

pub struct Controller {
    smc: Smc,
    sensors: Sensors,
    settings: Settings,
    boost: Boost,
    /// Fans vent runs right now, to notice when something else takes one.
    running: BTreeSet<u8>,
    asleep: bool,
}

impl Controller {
    pub fn new(smc: Smc, sensors: Sensors, settings: Settings) -> Self {
        Self {
            smc,
            sensors,
            settings,
            boost: Boost::default(),
            running: BTreeSet::new(),
            asleep: false,
        }
    }

    /// Reads the heat and brings every fan to what its setting asks for now.
    pub fn tick(&mut self) -> smc::Result<()> {
        if self.asleep {
            return Ok(());
        }
        let heat = temps::heat(&self.sensors.read(&self.smc));
        if let Some(change) = self.boost.feed(heat, self.settings.curve) {
            eprintln!("vent daemon: {change}");
        }
        let fans = Fans::new(&self.smc);
        for fan in fans.all()? {
            let rpm = self.rpm_for(&fan);
            if self.running.contains(&fan.index) && !fan.manual {
                eprintln!(
                    "vent daemon: something else took {}; taking it back",
                    fan.name
                );
            }
            match rpm {
                None if fan.manual => fans.release(fan.index)?,
                Some(rpm) if !fan.manual || (fan.target - rpm).abs() >= 1.0 => {
                    fans.run_at(fan.index, rpm)?;
                }
                _ => {}
            }
            if rpm.is_some() {
                self.running.insert(fan.index);
            } else {
                self.running.remove(&fan.index);
            }
        }
        Ok(())
    }

    /// The rpm vent runs a fan at, or none while macOS has it.
    fn rpm_for(&self, fan: &Fan) -> Option<f64> {
        match self.settings.speed(&fan.name) {
            Speed::Auto => self
                .boost
                .level(self.settings.curve)
                .and_then(|level| fan.rpm_for(Speed::Percent(level * 100.0))),
            speed => fan.rpm_for(speed),
        }
    }

    /// Gives every fan back to the system until `wake`.
    pub fn sleep(&mut self) -> smc::Result<()> {
        self.asleep = true;
        self.running.clear();
        Fans::new(&self.smc).release_all()?;
        eprintln!("vent daemon: every fan to macOS");
        Ok(())
    }

    pub fn wake(&mut self) -> smc::Result<()> {
        self.asleep = false;
        self.boost = Boost::default();
        self.tick()
    }

    pub fn status(&self) -> Status {
        Status {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            heat: self.boost.heat,
            settings: self.settings.clone(),
        }
    }

    pub fn handle(&mut self, request: Request) -> Result<Status, String> {
        match request {
            Request::Get => return Ok(self.status()),
            Request::Set { fan, speed } => {
                let names: Vec<String> = Fans::new(&self.smc)
                    .all()
                    .map_err(|e| e.to_string())?
                    .into_iter()
                    .filter(|f| fan == "all" || f.name == fan || f.index.to_string() == fan)
                    .map(|f| f.name)
                    .collect();
                if names.is_empty() {
                    return Err(format!("no fan named {fan:?}"));
                }
                for name in &names {
                    self.settings.set(name, speed);
                }
            }
            Request::Auto => self.settings.all_auto(),
            Request::Curve(curve) => self.settings.curve = curve,
        }
        self.settings.save().map_err(|e| e.to_string())?;
        self.tick().map_err(|e| e.to_string())?;
        Ok(self.status())
    }
}

/// The chip's smoothed heat, and whether auto fans have passed the curve's start.
#[derive(Debug, Default)]
struct Boost {
    heat: Option<f64>,
    on: bool,
}

impl Boost {
    /// Takes in a new reading, and says so when auto fans change hands.
    fn feed(&mut self, reading: Option<f64>, curve: Curve) -> Option<String> {
        self.heat = reading.map(|reading| match self.heat {
            Some(heat) => heat + (reading - heat) * SMOOTHING,
            None => reading,
        });
        let on = self.heat.is_some_and(|heat| {
            heat >= curve.start || (self.on && heat > curve.start - HYSTERESIS)
        });
        let changed = on != self.on;
        self.on = on;
        changed.then(|| match (on, self.heat) {
            (true, Some(heat)) => format!("heat {heat:.0}°, vent runs the auto fans"),
            (_, Some(heat)) => format!("heat {heat:.0}°, macOS has the auto fans"),
            (_, None) => "no heat reading, macOS has the auto fans".to_owned(),
        })
    }

    /// Where auto fans sit on the curve, or none while macOS has them.
    fn level(&self, curve: Curve) -> Option<f64> {
        self.heat.filter(|_| self.on).map(|heat| curve.level(heat))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_bursts_barely_move_the_heat() {
        let curve = Curve::default();
        let mut boost = Boost::default();
        boost.feed(Some(50.0), curve);
        boost.feed(Some(95.0), curve);
        assert!(boost.heat.unwrap() < 55.0);
        assert_eq!(boost.level(curve), None);
    }

    #[test]
    fn auto_fans_change_hands_with_a_margin() {
        let curve = Curve::default();
        let mut boost = Boost::default();
        assert!(boost.feed(Some(curve.start + 1.0), curve).is_some());
        assert_eq!(boost.level(curve), Some(curve.level(curve.start + 1.0)));
        boost.heat = Some(curve.start - 1.0);
        assert!(boost.feed(Some(curve.start - 1.0), curve).is_none());
        assert_eq!(boost.level(curve), Some(0.0));
        boost.heat = Some(curve.start - HYSTERESIS - 1.0);
        assert!(
            boost
                .feed(Some(curve.start - HYSTERESIS - 1.0), curve)
                .is_some()
        );
        assert_eq!(boost.level(curve), None);
    }

    #[test]
    fn no_reading_leaves_fans_to_macos() {
        let curve = Curve::default();
        let mut boost = Boost {
            heat: Some(90.0),
            on: true,
        };
        boost.feed(None, curve);
        assert_eq!(boost.level(curve), None);
    }
}
