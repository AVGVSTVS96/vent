use std::fmt;
use std::str::FromStr;

use crate::curve::Curve;
use crate::fan::Speed;
use crate::settings::Settings;

pub const SOCKET: &str = "/var/run/vent.sock";

#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    Get,
    Set { fan: String, speed: Speed },
    Auto,
    Curve(Curve),
}

impl FromStr for Request {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let words: Vec<&str> = text.split_whitespace().collect();
        match words.as_slice() {
            ["get"] => Ok(Self::Get),
            ["auto"] => Ok(Self::Auto),
            ["curve", start, full] => Ok(Self::Curve(format!("{start} {full}").parse()?)),
            ["set", fan, speed] => Ok(Self::Set {
                fan: (*fan).to_owned(),
                speed: speed.parse()?,
            }),
            _ => Err(format!("unknown request {text:?}")),
        }
    }
}

impl fmt::Display for Request {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Get => f.write_str("get"),
            Self::Auto => f.write_str("auto"),
            Self::Curve(curve) => write!(f, "curve {curve}"),
            Self::Set { fan, speed } => write!(f, "set {fan} {speed}"),
        }
    }
}

/// What the daemon is doing: its version, the heat its curve sees, and its settings.
#[derive(Debug, Clone, PartialEq)]
pub struct Status {
    pub version: String,
    pub heat: Option<f64>,
    pub settings: Settings,
}

/// The daemon answers `ok <version> <heat or ->` and its settings, or `error <reason>`.
pub fn reply(result: &Result<Status, String>) -> String {
    match result {
        Ok(status) => {
            let heat = status
                .heat
                .map_or_else(|| "-".to_owned(), |heat| format!("{heat:.1}"));
            format!("ok {} {heat}\n{}", status.version, status.settings)
        }
        Err(reason) => format!("error {reason}\n"),
    }
}

pub fn parse_reply(text: &str) -> Result<Status, String> {
    let (first, body) = text.split_once('\n').unwrap_or((text, ""));
    let unexpected = || format!("unexpected reply from the vent daemon: {first:?}");
    match first.split_once(' ') {
        Some(("ok", rest)) => {
            let (version, heat) = rest.split_once(' ').ok_or_else(unexpected)?;
            Ok(Status {
                version: version.to_owned(),
                heat: heat.parse().ok(),
                settings: body.parse()?,
            })
        }
        Some(("error", reason)) => Err(reason.to_owned()),
        _ => Err(unexpected()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_round_trip() {
        for request in [
            Request::Get,
            Request::Auto,
            Request::Curve(Curve::default()),
            Request::Set {
                fan: "right".into(),
                speed: Speed::Percent(80.0),
            },
        ] {
            assert_eq!(request.to_string().parse::<Request>(), Ok(request));
        }
    }

    #[test]
    fn replies_round_trip() {
        for heat in [Some(64.5), None] {
            let status = Status {
                version: "0.2.0".into(),
                heat,
                settings: "right max\n".parse().unwrap(),
            };
            assert_eq!(parse_reply(&reply(&Ok(status.clone()))), Ok(status));
        }
        let error = reply(&Err("no fan named \"middle\"".into()));
        assert_eq!(parse_reply(&error), Err("no fan named \"middle\"".into()));
    }
}
