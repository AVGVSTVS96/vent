mod controller;

use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::{Arc, Mutex, PoisonError};
use std::{fs, thread};

use crate::fan::{Fans, Speed};
use crate::hold::Hold;
use crate::protocol::{self, Request};
use crate::smc::{Smc, power};

use controller::Controller;

const ADMIN_GROUP: u32 = 80;
const VERSION: &str = env!("CARGO_PKG_VERSION");

type Shared = Arc<Mutex<Controller>>;

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let signals = block_signals();
    std::panic::set_hook(Box::new(|info| {
        eprintln!("vent daemon: {info}");
        give_every_fan_back();
    }));

    let controller = Controller::new(Smc::open()?, Hold::load()?);
    if let Err(error) = controller.apply() {
        eprintln!("vent daemon: could not apply the saved hold: {error}");
    }
    let controller: Shared = Arc::new(Mutex::new(controller));

    let listener = listen()?;
    thread::spawn({
        let controller = controller.clone();
        move || serve(&listener, &controller)
    });
    thread::spawn({
        let controller = controller.clone();
        move || stop_on_signal(signals, &controller)
    });
    power::watch(move |event| {
        let controller = controller.lock().unwrap_or_else(PoisonError::into_inner);
        let result = match event {
            power::Event::Sleep => controller.restore(),
            power::Event::Wake => controller.apply(),
        };
        if let Err(error) = result {
            eprintln!("vent daemon: {event:?}: {error}");
        }
    })
}

fn listen() -> std::io::Result<UnixListener> {
    let _ = fs::remove_file(protocol::SOCKET);
    let listener = UnixListener::bind(protocol::SOCKET)?;
    std::os::unix::fs::chown(protocol::SOCKET, Some(0), Some(ADMIN_GROUP))?;
    fs::set_permissions(protocol::SOCKET, fs::Permissions::from_mode(0o660))?;
    Ok(listener)
}

fn serve(listener: &UnixListener, controller: &Shared) {
    for stream in listener.incoming().flatten() {
        if let Err(error) = answer(&stream, controller) {
            eprintln!("vent daemon: {error}");
        }
    }
}

fn answer(stream: &UnixStream, controller: &Shared) -> std::io::Result<()> {
    let mut line = String::new();
    if BufReader::new(stream).read_line(&mut line)? == 0 {
        return Ok(());
    }
    let result = line.parse::<Request>().and_then(|request| {
        controller
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .handle(request)
    });
    let mut stream = stream;
    stream.write_all(protocol::reply(VERSION, &result).as_bytes())
}

fn block_signals() -> libc::sigset_t {
    // SAFETY: builds a signal set on the stack and blocks it for this thread and
    // every thread spawned after it, so only `stop_on_signal` receives them.
    unsafe {
        let mut set = std::mem::zeroed();
        libc::sigemptyset(&raw mut set);
        for signal in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
            libc::sigaddset(&raw mut set, signal);
        }
        libc::pthread_sigmask(libc::SIG_BLOCK, &raw const set, std::ptr::null_mut());
        set
    }
}

fn stop_on_signal(signals: libc::sigset_t, controller: &Shared) {
    let mut signal = 0;
    // SAFETY: waits on the set blocked in `block_signals`.
    unsafe {
        libc::sigwait(&raw const signals, &raw mut signal);
    }
    if let Err(error) = controller
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .restore()
    {
        eprintln!("vent daemon: could not give the fans back: {error}");
    }
    let _ = fs::remove_file(protocol::SOCKET);
    std::process::exit(0);
}

/// Last resort after a panic: a fresh SMC connection, since the shared one may be locked.
fn give_every_fan_back() {
    let Ok(smc) = Smc::open() else { return };
    let fans = Fans::new(&smc);
    for fan in fans.all().unwrap_or_default() {
        let _ = fans.set(&fan, Speed::Auto);
    }
}
