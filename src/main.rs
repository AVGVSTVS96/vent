mod client;
mod daemon;
mod fan;
mod hold;
mod protocol;
mod service;
mod smc;
mod temps;

use std::error::Error;
use std::io::{self, Write};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use fan::{Fans, Speed};
use protocol::Request;
use smc::{Key, Smc};

/// Fan control and temperatures for Apple Silicon Macs. Run with no command to see both.
#[derive(Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Keep a fan (left, right, an index, or all) at auto, min, max, an rpm, or a percent like 80%
    Set { fan: String, speed: Speed },
    /// Give every fan back to the system
    Auto,
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
        None => status(),
        Some(Command::Set { fan, speed }) => send(&Request::Set { fan, speed }),
        Some(Command::Auto) => send(&Request::Release),
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
        let mode = match reply.as_ref().and_then(|reply| reply.hold.get(&fan.name)) {
            Some(speed) => speed.to_string(),
            None if fan.manual => "manual".into(),
            None => "auto".into(),
        };
        writeln!(out, "{:<7}{:>5.0} rpm  {mode}", fan.name, fan.actual)?;
    }
    let temps: Vec<String> = temps::read(&smc)?
        .into_iter()
        .map(|temp| match temp.hottest {
            Some(celsius) => format!("{} {celsius:.0}°", temp.group),
            None => format!("{} –", temp.group),
        })
        .collect();
    writeln!(out, "{}", temps.join("  "))?;
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
        SmcCommand::List { prefix } => smc
            .keys()?
            .into_iter()
            .filter(|key| {
                prefix
                    .as_deref()
                    .is_none_or(|prefix| key.as_str().starts_with(prefix))
            })
            .collect(),
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
