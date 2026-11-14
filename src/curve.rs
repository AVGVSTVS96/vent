use std::fmt;
use std::str::FromStr;

/// How auto fans follow heat: macOS keeps them below `start`, and vent runs them
/// from their minimum at `start` to their maximum at `full`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Curve {
    pub start: f64,
    pub full: f64,
}

/// Read off an M1 Max: light work averages under 65°, while a heavy load holds
/// 75° to 85° with macOS keeping the fans near their minimum.
impl Default for Curve {
    fn default() -> Self {
        Self {
            start: 65.0,
            full: 90.0,
        }
    }
}

/// Sensible bounds for a curve's ends, in °C.
const RANGE: std::ops::RangeInclusive<f64> = 30.0..=110.0;

impl Curve {
    pub fn new(start: f64, full: f64) -> Result<Self, String> {
        if RANGE.contains(&start) && RANGE.contains(&full) && start < full {
            Ok(Self { start, full })
        } else {
            Err(format!(
                "a curve runs from a lower to a higher temperature between {}° and {}°",
                RANGE.start(),
                RANGE.end()
            ))
        }
    }

    /// How far along the curve `heat` sits, from 0 at `start` to 1 at `full`.
    pub fn level(&self, heat: f64) -> f64 {
        ((heat - self.start) / (self.full - self.start)).clamp(0.0, 1.0)
    }
}

impl FromStr for Curve {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let numbers: Vec<f64> = text
            .split_whitespace()
            .map(|word| word.trim_end_matches(['°', 'c', 'C']).parse::<f64>())
            .collect::<Result<_, _>>()
            .map_err(|_| format!("{text:?} is not a curve: give two temperatures like 60 85"))?;
        match numbers.as_slice() {
            [start, full] => Self::new(*start, *full),
            _ => Err(format!(
                "{text:?} is not a curve: give two temperatures like 60 85"
            )),
        }
    }
}

impl fmt::Display for Curve {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.start, self.full)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_run_from_start_to_full() {
        let curve = Curve::new(60.0, 80.0).unwrap();
        assert!((curve.level(40.0)).abs() < f64::EPSILON);
        assert!((curve.level(70.0) - 0.5).abs() < f64::EPSILON);
        assert!((curve.level(95.0) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn curves_parse_and_print_back() {
        assert_eq!("65 90".parse::<Curve>(), Ok(Curve::default()));
        assert_eq!("65° 90°".parse::<Curve>(), Ok(Curve::default()));
        assert_eq!(Curve::default().to_string().parse(), Ok(Curve::default()));
        for bad in ["", "60", "85 60", "60 200", "hot cold", "60 nan"] {
            assert!(bad.parse::<Curve>().is_err(), "{bad} parsed");
        }
    }
}
