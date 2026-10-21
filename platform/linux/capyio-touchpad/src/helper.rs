//! Minimal standalone FFI boundary. The library remains forbid(unsafe_code).
use std::ffi::c_int;
#[link(name = "capyio_touchpad_native", kind = "static")]
#[link(name = "evdev")]
unsafe extern "C" {
    fn capyio_touchpad_main() -> c_int;
}
fn main() {
    // SAFETY: the C entrypoint accepts no pointers and exclusively owns its
    // fixed descriptor, stdin/stdout IPC and cleanup during this process.
    let result = unsafe { capyio_touchpad_main() };
    std::process::exit(result);
}
