mod controller;

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use std::{fs, thread};

use crate::fan::Fans;
use crate::protocol::{self, Request};
use crate::settings::Settings;
use crate::smc::{Smc, power};
use crate::temps::Sensors;

use controller::Controller;

const ADMIN_GROUP: u32 = 80;
const LONGEST_REQUEST: u64 = 256;
const CLIENT_TIMEOUT: Duration = Duration::from_secs(1);
/// Temperatures can only be read, not watched, so vent reads them once a second.
const TICK: Duration = Duration::from_secs(1);

type Shared = Arc<Mutex<Controller>>;

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let signals = block_signals();
    std::panic::set_hook(Box::new(|info| {
        eprintln!("vent daemon: {info}");
        give_every_fan_back();
    }));

    let settings = Settings::load().unwrap_or_else(|error| {
        eprintln!("vent daemon: ignoring the saved settings: {error}");
        Settings::default()
    });
    let smc = Smc::open()?;
    let sensors = Sensors::find(&smc)?;
    let controller: Shared = Arc::new(Mutex::new(Controller::new(smc, sensors, settings)));

    let listener = listen()?;
    thread::spawn({
        let controller = controller.clone();
        move || serve(&listener, &controller)
    });
    thread::spawn({
        let controller = controller.clone();
        move || stop_on_signal(signals, &controller)
    });
    thread::spawn({
        let controller = controller.clone();
        move || follow_heat(&controller)
    });
    match power::watch(move |event| {
        let mut controller = lock(&controller);
        let result = match event {
            power::Event::Sleep => controller.sleep(),
            power::Event::Wake => controller.wake(),
        };
        if let Err(error) = result {
            eprintln!("vent daemon: {event:?}: {error}");
        }
    })? {}
}

fn lock(controller: &Shared) -> std::sync::MutexGuard<'_, Controller> {
    controller.lock().unwrap_or_else(PoisonError::into_inner)
}

fn follow_heat(controller: &Shared) {
    let mut failing = false;
    loop {
        match lock(controller).tick() {
            Ok(()) => failing = false,
            Err(error) if !failing => {
                eprintln!("vent daemon: {error}");
                failing = true;
            }
            Err(_) => {}
        }
        thread::sleep(TICK);
    }
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
    if stream.set_read_timeout(Some(CLIENT_TIMEOUT)).is_err() {
        return Ok(());
    }
    stream.set_write_timeout(Some(CLIENT_TIMEOUT))?;
    let mut line = String::new();
    if BufReader::new(stream.take(LONGEST_REQUEST)).read_line(&mut line)? == 0 {
        return Ok(());
    }
    let result = line
        .parse::<Request>()
        .and_then(|request| lock(controller).handle(request));
    let mut stream = stream;
    stream.write_all(protocol::reply(&result).as_bytes())
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
    if let Err(error) = lock(controller).sleep() {
        eprintln!("vent daemon: could not give the fans back: {error}");
    }
    let _ = fs::remove_file(protocol::SOCKET);
    std::process::exit(0);
}

/// Last resort after a panic: a fresh SMC connection, since the shared one may be locked.
fn give_every_fan_back() {
    if let Ok(smc) = Smc::open() {
        let _ = Fans::new(&smc).release_all();
    }
}
