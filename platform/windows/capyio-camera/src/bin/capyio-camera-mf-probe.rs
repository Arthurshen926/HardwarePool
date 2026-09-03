#[cfg(windows)]
fn main() -> std::process::ExitCode {
    use windows::Win32::Media::MediaFoundation::{
        MFIsVirtualCameraTypeSupported, MFVirtualCameraType_SoftwareCameraSource,
    };

    // SAFETY: this read-only API takes a closed enum value and returns a BOOL.
    // It does not create, start, stop, register, remove, or enumerate a camera.
    let result =
        unsafe { MFIsVirtualCameraTypeSupported(MFVirtualCameraType_SoftwareCameraSource) };
    match result {
        Ok(supported) => {
            println!("mode=read_only_support_probe");
            println!("software_camera_supported={}", supported.as_bool());
            if supported.as_bool() {
                std::process::ExitCode::SUCCESS
            } else {
                std::process::ExitCode::from(2)
            }
        }
        Err(error) => {
            eprintln!("media_foundation_probe_failed={error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(not(windows))]
fn main() -> std::process::ExitCode {
    eprintln!("media_foundation_probe_failed=not_windows");
    std::process::ExitCode::FAILURE
}
