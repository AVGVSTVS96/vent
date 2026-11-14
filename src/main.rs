mod client;
mod curve;
mod daemon;
mod fan;
mod protocol;
mod service;
mod settings;
mod smc;
mod temps;

use std::error::Error;
use std::io::{self, Write};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use curve::Curve;
use fan::{Fans, Speed};
use protocol::Request;
use smc::{Key, Smc};

/// Fan control and temperatures for Apple Silicon Macs. Run with no command to see both.
///
/// Every fan starts on auto: macOS runs it while the chip is cool, and vent speeds it up
/// along the curve as heat builds.
#[derive(Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Keep a fan (left, right, an index, or all) at auto, system, min, max, an rpm, or a percent like 80%
    Set { fan: String, speed: Speed },
    /// Put every fan back on auto
    Auto,
    /// Show the curve auto fans follow, or set where it starts and where fans reach full speed, like `vent curve 60 85`
    Curve {
        #[arg(num_args = 2, value_names = ["START", "FULL"])]
        ends: Option<Vec<String>>,
    },
    /// Install the root daemon that holds fan speeds, or update it
    Install,
    /// Remove the daemon and give every fan back to the system
    Uninstall,
    /// Read raw SMC keys
    #[command(subcommand)]
    Smc(SmcCommand),
    #[command(hide = true)]
    Daemon,
}

#[derive(Subcommand)]
enum SmcCommand {
    /// List every key and its value, or only keys starting with a prefix
    List { prefix: Option<String> },
    /// Read one key
    Read { key: Key },
}

fn run(command: Option<Command>) -> Result<(), Box<dyn Error>> {
    match command {
        None | Some(Command::Curve { ends: None }) => status(),
        Some(Command::Set { fan, speed }) => send(&Request::Set { fan, speed }),
        Some(Command::Auto) => send(&Request::Auto),
        Some(Command::Curve { ends: Some(ends) }) => {
            send(&Request::Curve(ends.join(" ").parse::<Curve>()?))
        }
        Some(Command::Install) => {
            service::install()?;
            status()
        }
        Some(Command::Uninstall) => Ok(service::uninstall()?),
        Some(Command::Smc(command)) => smc_command(command),
        Some(Command::Daemon) => daemon::run(),
    }
}

fn send(request: &Request) -> Result<(), Box<dyn Error>> {
    client::send(request)?;
    status()
}

fn status() -> Result<(), Box<dyn Error>> {
    let smc = Smc::open()?;
    let reply = client::send(&Request::Get).ok();
    let mut out = io::stdout().lock();
    for fan in Fans::new(&smc).all()? {
        let mode = match &reply {
            Some(reply) => reply.settings.speed(&fan.name).to_string(),
            None if fan.manual => "manual".into(),
            None => "system".into(),
        };
        writeln!(out, "{:<7}{:>5.0} rpm  {mode}", fan.name, fan.actual)?;
    }
    let temps: Vec<String> = temps::Sensors::find(&smc)?
        .read(&smc)
        .into_iter()
        .map(|temp| match temp.hottest {
            Some(celsius) => format!("{} {celsius:.0}°", temp.group),
            None => format!("{} –", temp.group),
        })
        .collect();
    writeln!(out, "{}", temps.join("  "))?;
    if let Some(reply) = &reply {
        let Curve { start, full } = reply.settings.curve;
        let heat = reply
            .heat
            .map_or_else(|| "–".to_owned(), |heat| format!("{heat:.0}°"));
        writeln!(out, "heat {heat}, auto curve {start}° to {full}°")?;
    }
    if let Some(reply) = reply.filter(|reply| reply.version != env!("CARGO_PKG_VERSION")) {
        eprintln!(
            "vent: the daemon is version {}; run `sudo vent install` to update it",
            reply.version
        );
    }
    Ok(())
}

fn smc_command(command: SmcCommand) -> Result<(), Box<dyn Error>> {
    let smc = Smc::open()?;
    let mut out = io::stdout().lock();
    let keys = match command {
        SmcCommand::Read { key } => vec![key],
        SmcCommand::List { prefix } => smc.keys(prefix.as_deref().unwrap_or_default())?,
    };
    for key in keys {
        match smc.read(key) {
            Ok(value) => writeln!(out, "{key}  {}  {value}", value.kind())?,
            Err(error) => writeln!(out, "{key}  ({error})")?,
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match run(Cli::parse().command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error)
            if error
                .downcast_ref::<io::Error>()
                .is_some_and(|e| e.kind() == io::ErrorKind::BrokenPipe) =>
        {
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("vent: {error}");
            ExitCode::FAILURE
        }
    }
}
