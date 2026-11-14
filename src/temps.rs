use crate::smc::{self, Key, Smc};

const GROUPS: [(&str, &[&str]); 5] = [
    ("cpu", &["Tp", "Te"]),
    ("gpu", &["Tg"]),
    ("memory", &["Tm"]),
    ("ssd", &["TH"]),
    ("battery", &["TB"]),
];

/// The groups whose heat the fans answer to.
const CHIP: [&str; 2] = ["cpu", "gpu"];

/// Readings outside this range come from idle or unpopulated sensors.
const PLAUSIBLE: std::ops::RangeInclusive<f64> = 10.0..=130.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Temp {
    pub group: &'static str,
    pub hottest: Option<f64>,
    pub average: Option<f64>,
}

/// How hot the chip runs: the hotter of the CPU's and the GPU's average sensor.
/// An average follows the heat the fans have to carry away, where the hottest
/// sensor jumps with every short burst on one core.
pub fn heat(temps: &[Temp]) -> Option<f64> {
    temps
        .iter()
        .filter(|temp| CHIP.contains(&temp.group))
        .filter_map(|temp| temp.average)
        .reduce(f64::max)
}

pub fn summarize(readings: &[(Key, f64)]) -> Vec<Temp> {
    GROUPS
        .iter()
        .map(|&(group, prefixes)| {
            let values: Vec<f64> = readings
                .iter()
                .filter(|(key, value)| {
                    prefixes
                        .iter()
                        .any(|prefix| key.as_str().starts_with(prefix))
                        && PLAUSIBLE.contains(value)
                })
                .map(|&(_, value)| value)
                .collect();
            #[allow(clippy::cast_precision_loss)]
            let average =
                (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64);
            Temp {
                group,
                hottest: values.iter().copied().reduce(f64::max),
                average,
            }
        })
        .collect()
}

/// The temperature keys this Mac has, found once since the SMC's key list never changes.
pub struct Sensors(Vec<Key>);

impl Sensors {
    pub fn find(smc: &Smc) -> smc::Result<Self> {
        Ok(Self(smc.keys("T")?))
    }

    pub fn read(&self, smc: &Smc) -> Vec<Temp> {
        let readings: Vec<(Key, f64)> = self
            .0
            .iter()
            .filter_map(|&key| Some((key, smc.read(key).ok()?.number()?)))
            .collect();
        summarize(&readings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(text: &str) -> Key {
        text.parse().unwrap()
    }

    #[test]
    fn groups_report_their_hottest_and_average_plausible_sensor() {
        let temps = summarize(&[
            (key("Tp01"), 60.0),
            (key("Te02"), 71.0),
            (key("Tg05"), 9.2),
            (key("TH0a"), 48.0),
            (key("TH1a"), 0.0),
            (key("TB1T"), 43.5),
            (key("TaLT"), 46.0),
        ]);
        let seen: Vec<_> = temps
            .iter()
            .map(|t| (t.group, t.hottest, t.average))
            .collect();
        assert_eq!(
            seen,
            [
                ("cpu", Some(71.0), Some(65.5)),
                ("gpu", None, None),
                ("memory", None, None),
                ("ssd", Some(48.0), Some(48.0)),
                ("battery", Some(43.5), Some(43.5))
            ]
        );
        assert_eq!(heat(&temps), Some(65.5));
    }
}
