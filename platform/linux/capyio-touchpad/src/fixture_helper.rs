//! Explicit software fixture entrypoint. This archive has no libevdev or
//! /dev/uinput syscalls, even in builds that also enable native-uinput.
use std::ffi::c_int;
#[link(name = "capyio_touchpad_fixture", kind = "static")]
unsafe extern "C" {
    fn capyio_touchpad_fixture_main() -> c_int;
}
fn main() {
    // SAFETY: the C fixture entrypoint takes no pointers and exclusively owns
    // this process's private IPC and trace descriptors. Its backend is fake.
    let result = unsafe { capyio_touchpad_fixture_main() };
    std::process::exit(result);
}
