use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use std::time::Duration;
use std::{fs, io, thread};

use crate::{client, hold, protocol};

const LABEL: &str = "io.github.avgvstvs96.vent";
const HELPER: &str = "/Library/PrivilegedHelperTools/io.github.avgvstvs96.vent";
const PLIST: &str = "/Library/LaunchDaemons/io.github.avgvstvs96.vent.plist";
const LOG: &str = "/var/log/vent.log";

pub fn install() -> io::Result<()> {
    require_root()?;
    stop();
    let draft = format!("{HELPER}.new");
    fs::copy(std::env::current_exe()?, &draft)?;
    std::os::unix::fs::chown(&draft, Some(0), Some(0))?;
    fs::set_permissions(&draft, fs::Permissions::from_mode(0o755))?;
    fs::rename(&draft, HELPER)?;
    fs::write(PLIST, plist())?;
    launchctl(&["bootstrap", "system", PLIST])?;
    for _ in 0..50 {
        if client::is_running() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err(io::Error::other(format!(
        "the daemon didn't start; see {LOG}"
    )))
}

pub fn uninstall() -> io::Result<()> {
    require_root()?;
    stop();
    for path in [PLIST, HELPER, LOG, protocol::SOCKET, hold::PATH] {
        match fs::remove_file(path) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
            _ => {}
        }
    }
    Ok(())
}

/// Stopping the daemon makes it give the fans back before it exits.
fn stop() {
    let _ = launchctl(&["bootout", &format!("system/{LABEL}")]);
}

fn require_root() -> io::Result<()> {
    // SAFETY: geteuid has no preconditions.
    if unsafe { libc::geteuid() } == 0 {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "run it with sudo",
        ))
    }
}

fn launchctl(args: &[&str]) -> io::Result<()> {
    let output = Command::new("/bin/launchctl").args(args).output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ))
    }
}

fn plist() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>{LABEL}</string>
	<key>ProgramArguments</key>
	<array>
		<string>{HELPER}</string>
		<string>daemon</string>
	</array>
	<key>RunAtLoad</key>
	<true/>
	<key>KeepAlive</key>
	<true/>
	<key>StandardErrorPath</key>
	<string>{LOG}</string>
</dict>
</plist>
"#
    )
}
