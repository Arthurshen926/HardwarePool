//! Thin standalone FFI boundary. The owning library remains forbid(unsafe_code).
use std::ffi::{CString, c_char, c_int};
#[link(name = "capyio_camera_native", kind = "static")]
#[link(name = "pipewire-0.3")]
unsafe extern "C" {
    fn capyio_camera_main(argc: c_int, argv: *mut *mut c_char) -> c_int;
}
fn main() {
    let args: Vec<CString> = std::env::args()
        .map(|v| CString::new(v).expect("NUL argument"))
        .collect();
    let mut pointers: Vec<*mut c_char> = args.iter().map(|v| v.as_ptr().cast_mut()).collect();
    pointers.push(std::ptr::null_mut());
    // SAFETY: NUL-terminated strings and argv remain alive for the synchronous
    // C entrypoint; it only reads arguments and owns its native loop/resources.
    let code = unsafe { capyio_camera_main(args.len() as c_int, pointers.as_mut_ptr()) };
    std::process::exit(code);
}
