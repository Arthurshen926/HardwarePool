//! Session-scoped camera owner. COM deployment is an external prerequisite.
#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use capyio_windows_camera::{
        MfVirtualCameraPlan, MfVirtualCameraRegistrar, MfVirtualCameraRegistrationBackend,
    };
    use capyio_windows_camera_mf::{MediaFoundationRuntime, WindowsMfVirtualCameraBackend};
    use std::io::{Read, Write};

    if std::env::args_os().len() != 1 {
        return Err("camera session host accepts no arguments".into());
    }
    let _runtime = MediaFoundationRuntime::startup()?;
    let plan = MfVirtualCameraPlan::capyio_fixture();
    let mut backend = WindowsMfVirtualCameraBackend::default();
    backend.prepare(&plan)?;
    let existing = backend.symbolic_link().is_some();
    backend.shutdown()?;
    if existing {
        return Err("refusing to own an existing camera registration".into());
    }
    let mut registrar =
        MfVirtualCameraRegistrar::new(plan, WindowsMfVirtualCameraBackend::default());
    registrar.prepare()?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        registrar.start()?;
        println!("CAPYIO_CAMERA_SESSION_READY");
        std::io::stdout().flush()?;
        // A closed control pipe (including parent exit) ends this exact lease.
        // No frame data or arbitrary commands cross the control pipe.
        let mut byte = [0_u8; 1];
        match std::io::stdin().read(&mut byte)? {
            0 => Ok(()),
            _ => Err("unexpected camera session control payload".into()),
        }
    })();
    let stopped = registrar.stop();
    let shutdown = registrar.shutdown();
    stopped?;
    shutdown?;
    result?;
    println!("CAPYIO_CAMERA_SESSION_CLOSED");
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("camera session host requires Windows");
    std::process::exit(1);
}
