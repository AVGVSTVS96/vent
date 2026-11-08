use std::ffi::c_void;

use super::ffi;

const CAN_SYSTEM_SLEEP: u32 = 0xe000_0270;
const SYSTEM_WILL_SLEEP: u32 = 0xe000_0280;
const SYSTEM_HAS_POWERED_ON: u32 = 0xe000_0300;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Sleep,
    Wake,
}

struct Watch {
    root: ffi::IoConnect,
    handler: Box<dyn FnMut(Event)>,
}

extern "C" fn on_message(
    refcon: *mut c_void,
    _: ffi::IoObject,
    message: u32,
    argument: *mut c_void,
) {
    // SAFETY: refcon is the Watch leaked in `watch`, alive for the whole process.
    let watch = unsafe { &mut *refcon.cast::<Watch>() };
    match message {
        SYSTEM_WILL_SLEEP => (watch.handler)(Event::Sleep),
        SYSTEM_HAS_POWERED_ON => (watch.handler)(Event::Wake),
        _ => {}
    }
    if matches!(message, CAN_SYSTEM_SLEEP | SYSTEM_WILL_SLEEP) {
        // SAFETY: argument is the notification id the kernel passed for this message.
        unsafe {
            ffi::IOAllowPowerChange(watch.root, argument as isize);
        }
    }
}

/// Runs `handler` before each system sleep and after each wake. Never returns.
pub fn watch(handler: impl FnMut(Event) + 'static) -> ! {
    let watch = Box::into_raw(Box::new(Watch {
        root: 0,
        handler: Box::new(handler),
    }));
    // SAFETY: the Watch is leaked so the callback's refcon stays valid, and the
    // run loop source comes from the notification port IOKit just created.
    unsafe {
        let mut port = std::ptr::null_mut();
        let mut notifier = 0;
        (*watch).root = ffi::IORegisterForSystemPower(
            watch.cast(),
            &raw mut port,
            on_message,
            &raw mut notifier,
        );
        let source = ffi::IONotificationPortGetRunLoopSource(port);
        ffi::CFRunLoopAddSource(
            ffi::CFRunLoopGetCurrent(),
            source,
            ffi::kCFRunLoopDefaultMode,
        );
        ffi::CFRunLoopRun();
    }
    unreachable!("the power run loop never stops")
}
