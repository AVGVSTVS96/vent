mod fan;
mod smc;

use std::process::ExitCode;

use clap::{Parser, Subcommand};

use fan::{Fans, Speed};
use smc::{Key, Smc};

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Show fan speeds
    Fans,
    /// Set a fan, by name (left, right) or index, or all fans
    Set { fan: String, speed: Speed },
    /// Hand every fan back to the system
    Auto,
    /// Raw SMC access
    #[command(subcommand)]
    Smc(SmcCommand),
}

#[derive(Subcommand)]
enum SmcCommand {
    /// List keys and their values, optionally only those starting with a prefix
    List { prefix: Option<String> },
    /// Read one key
    Read { key: Key },
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    let smc = Smc::open()?;
    let fans = Fans::new(&smc);
    match cli.command {
        Command::Fans => print_fans(&fans)?,
        Command::Set { fan, speed } => {
            let targets = if fan == "all" {
                fans.all()?
            } else {
                vec![
                    fans.find(&fan)?
                        .ok_or_else(|| format!("no fan named {fan:?}"))?,
                ]
            };
            for target in &targets {
                fans.set(target, speed)?;
            }
            print_fans(&fans)?;
        }
        Command::Auto => {
            for fan in fans.all()? {
                fans.set(&fan, Speed::Auto)?;
            }
            print_fans(&fans)?;
        }
        Command::Smc(SmcCommand::List { prefix }) => {
            for key in smc.keys()? {
                if prefix
                    .as_deref()
                    .is_some_and(|p| !key.as_str().starts_with(p))
                {
                    continue;
                }
                match smc.read(key) {
                    Ok(value) => println!("{key}  {}  {value}", value.kind()),
                    Err(error) => println!("{key}  ({error})"),
                }
            }
        }
        Command::Smc(SmcCommand::Read { key }) => {
            let value = smc.read(key)?;
            println!("{key}  {}  {value}", value.kind());
        }
    }
    Ok(())
}

fn print_fans(fans: &Fans) -> smc::Result<()> {
    for fan in fans.all()? {
        let mode = if fan.manual {
            format!("manual {:.0}", fan.target)
        } else {
            "auto".into()
        };
        println!(
            "{:<6} {:>5.0} rpm  {mode:<12} {:.0}-{:.0}",
            fan.name, fan.actual, fan.min, fan.max
        );
    }
    Ok(())
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("vent: {error}");
            ExitCode::FAILURE
        }
    }
}
