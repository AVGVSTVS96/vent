use std::ffi::{c_char, c_void};

pub type KernReturn = i32;
pub type IoObject = u32;
pub type IoConnect = u32;
pub type MachPort = u32;

pub const KERN_SUCCESS: KernReturn = 0;
pub const MAIN_PORT_DEFAULT: MachPort = 0;

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    pub fn IOServiceMatching(name: *const c_char) -> *mut c_void;
    pub fn IOServiceGetMatchingService(main_port: MachPort, matching: *mut c_void) -> IoObject;
    pub fn IOServiceOpen(
        service: IoObject,
        owning_task: MachPort,
        kind: u32,
        connect: *mut IoConnect,
    ) -> KernReturn;
    pub fn IOServiceClose(connect: IoConnect) -> KernReturn;
    pub fn IOObjectRelease(object: IoObject) -> KernReturn;
    pub fn IOConnectCallStructMethod(
        connection: IoConnect,
        selector: u32,
        input: *const c_void,
        input_size: usize,
        output: *mut c_void,
        output_size: *mut usize,
    ) -> KernReturn;
}

unsafe extern "C" {
    static mach_task_self_: MachPort;
}

pub fn task_self() -> MachPort {
    // SAFETY: mach_task_self_ is set by the kernel at process start and never changes.
    unsafe { mach_task_self_ }
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct Version {
    pub major: u8,
    pub minor: u8,
    pub build: u8,
    pub reserved: u8,
    pub release: u16,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct PowerLimit {
    pub version: u16,
    pub length: u16,
    pub cpu: u32,
    pub gpu: u32,
    pub mem: u32,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct KeyInfo {
    pub size: u32,
    pub kind: u32,
    pub attributes: u8,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct KeyData {
    pub key: u32,
    pub version: Version,
    pub power_limit: PowerLimit,
    pub info: KeyInfo,
    pub result: u8,
    pub status: u8,
    pub command: u8,
    pub data32: u32,
    pub bytes: [u8; 32],
}

const _: () = assert!(size_of::<KeyData>() == 80);
