use std::io::{ErrorKind, Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::time::Duration;

use crate::protocol::{self, Request, Status};

const TIMEOUT: Duration = Duration::from_secs(5);

pub fn send(request: &Request) -> Result<Status, String> {
    let mut stream = UnixStream::connect(protocol::SOCKET).map_err(|error| match error.kind() {
        ErrorKind::NotFound | ErrorKind::ConnectionRefused => {
            "the vent daemon isn't running: install it with `sudo vent install`".to_owned()
        }
        ErrorKind::PermissionDenied => "only admin accounts can change fan speeds".to_owned(),
        _ => error.to_string(),
    })?;
    stream
        .set_read_timeout(Some(TIMEOUT))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(TIMEOUT))
        .map_err(|e| e.to_string())?;
    let mut text = String::new();
    writeln!(stream, "{request}")
        .and_then(|()| stream.shutdown(Shutdown::Write))
        .and_then(|()| stream.read_to_string(&mut text).map(drop))
        .map_err(|error| format!("lost the vent daemon: {error}"))?;
    protocol::parse_reply(&text)
}

pub fn is_running() -> bool {
    UnixStream::connect(protocol::SOCKET).is_ok()
}
