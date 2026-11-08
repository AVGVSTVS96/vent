use crate::smc::{self, Key, Smc};

const GROUPS: [(&str, &[&str]); 5] = [
    ("cpu", &["Tp", "Te"]),
    ("gpu", &["Tg"]),
    ("memory", &["Tm"]),
    ("ssd", &["TH"]),
    ("battery", &["TB"]),
];

/// Readings outside this range come from idle or unpopulated sensors.
const PLAUSIBLE: std::ops::RangeInclusive<f64> = 10.0..=130.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Temp {
    pub group: &'static str,
    pub hottest: Option<f64>,
}

pub fn summarize(readings: &[(Key, f64)]) -> Vec<Temp> {
    GROUPS
        .iter()
        .map(|&(group, prefixes)| Temp {
            group,
            hottest: readings
                .iter()
                .filter(|(key, value)| {
                    prefixes
                        .iter()
                        .any(|prefix| key.as_str().starts_with(prefix))
                        && PLAUSIBLE.contains(value)
                })
                .map(|&(_, value)| value)
                .reduce(f64::max),
        })
        .collect()
}

pub fn read(smc: &Smc) -> smc::Result<Vec<Temp>> {
    let readings: Vec<(Key, f64)> = smc
        .keys("T")?
        .into_iter()
        .filter_map(|key| Some((key, smc.read(key).ok()?.number()?)))
        .collect();
    Ok(summarize(&readings))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(text: &str) -> Key {
        text.parse().unwrap()
    }

    #[test]
    fn groups_report_their_hottest_plausible_sensor() {
        let temps = summarize(&[
            (key("Tp01"), 60.0),
            (key("Te02"), 71.5),
            (key("Tg05"), 9.2),
            (key("TH0a"), 48.0),
            (key("TH1a"), 0.0),
            (key("TB1T"), 43.5),
            (key("TaLT"), 46.0),
        ]);
        let hottest: Vec<_> = temps.iter().map(|t| (t.group, t.hottest)).collect();
        assert_eq!(
            hottest,
            [
                ("cpu", Some(71.5)),
                ("gpu", None),
                ("memory", None),
                ("ssd", Some(48.0)),
                ("battery", Some(43.5))
            ]
        );
    }
}
