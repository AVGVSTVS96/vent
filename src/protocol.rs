use std::fmt;
use std::str::FromStr;

use crate::fan::Speed;

pub const SOCKET: &str = "/var/run/vent.sock";

#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    Get,
    Set { fan: String, speed: Speed },
    Release,
}

impl FromStr for Request {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let words: Vec<&str> = text.split_whitespace().collect();
        match words.as_slice() {
            ["get"] => Ok(Self::Get),
            ["release"] => Ok(Self::Release),
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
            Self::Release => f.write_str("release"),
            Self::Set { fan, speed } => write!(f, "set {fan} {speed}"),
        }
    }
}

/// The daemon answers `ok <version>` and its hold, one fan per line, or `error <reason>`.
pub fn reply(version: &str, result: &Result<String, String>) -> String {
    match result {
        Ok(hold) => format!("ok {version}\n{hold}"),
        Err(reason) => format!("error {reason}\n"),
    }
}

pub fn parse_reply(text: &str) -> Result<(String, String), String> {
    let (head, body) = text.split_once('\n').unwrap_or((text, ""));
    match head.split_once(' ') {
        Some(("ok", version)) => Ok((version.to_owned(), body.to_owned())),
        Some(("error", reason)) => Err(reason.to_owned()),
        _ => Err(format!("unexpected reply from the vent daemon: {head:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_round_trip() {
        for request in [
            Request::Get,
            Request::Release,
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
        let ok = reply("0.1.0", &Ok("right max\n".into()));
        assert_eq!(parse_reply(&ok), Ok(("0.1.0".into(), "right max\n".into())));
        let error = reply("0.1.0", &Err("no fan named \"middle\"".into()));
        assert_eq!(parse_reply(&error), Err("no fan named \"middle\"".into()));
    }
}
