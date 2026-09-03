#[cfg(windows)]
use std::{
    error::Error,
    thread,
    time::{Duration, Instant},
};

#[cfg(windows)]
use capyio_windows_camera_share::{CameraSharedIngressConsumer, CameraSharedIngressError};

#[cfg(windows)]
const REQUIRED_FRAMES: u64 = 30;
#[cfg(windows)]
const PROBE_TIMEOUT: Duration = Duration::from_secs(30);
#[cfg(windows)]
const POLL_INTERVAL: Duration = Duration::from_millis(5);

#[cfg(windows)]
fn main() {
    if let Err(error) = run() {
        eprintln!("CAPYIO_CAMERA_SHARED_PROBE_ERROR {error}");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("CAPYIO_CAMERA_SHARED_PROBE_ERROR this lab probe requires Windows");
    std::process::exit(1);
}

#[cfg(windows)]
fn run() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let local_lab = match arguments.as_slice() {
        [] => false,
        [argument] if argument == "--local-lab" => true,
        _ => return Err("expected no arguments or the exact --local-lab flag".into()),
    };
    if local_lab && !cfg!(feature = "lab-support") {
        return Err("--local-lab requires the compile-time lab-support feature".into());
    }
    let scope = if local_lab { "local-lab" } else { "global" };
    println!(
        "CAPYIO_CAMERA_SHARED_PROBE_READY scope={} required_frames={} timeout_seconds={}",
        scope,
        REQUIRED_FRAMES,
        PROBE_TIMEOUT.as_secs()
    );
    let deadline = Instant::now() + PROBE_TIMEOUT;
    let mut consumer = loop {
        match open_selected_mapping(local_lab) {
            Ok(consumer) => break consumer,
            Err(CameraSharedIngressError::Windows {
                operation: "OpenFileMappingW",
                code: 2,
            }) if Instant::now() < deadline => thread::sleep(POLL_INTERVAL),
            Err(error) => return Err(error.into()),
        }
    };

    let stream_id = consumer.stream_id();
    let stream_epoch = consumer.stream_epoch();
    println!(
        "CAPYIO_CAMERA_SHARED_PROBE_OPEN scope={} stream={} epoch={}",
        scope, stream_id, stream_epoch
    );
    let mut observed_frames = 0_u64;
    let mut discontinuities = 0_u64;
    let mut first_checksum = None;
    let mut last_checksum = None;
    let mut first_sequence = None;
    let mut last_sequence = 0_u64;

    while observed_frames < REQUIRED_FRAMES && Instant::now() < deadline {
        if let Some(frame) = consumer.try_read_latest()? {
            let checksum = fnv1a64(&frame.payload);
            observed_frames += 1;
            discontinuities += u64::from(frame.descriptor.flags.discontinuity);
            first_checksum.get_or_insert(checksum);
            last_checksum = Some(checksum);
            first_sequence.get_or_insert(frame.descriptor.sequence);
            last_sequence = frame.descriptor.sequence;
        } else {
            thread::sleep(POLL_INTERVAL);
        }
    }
    if observed_frames < REQUIRED_FRAMES {
        return Err(
            format!("observed {observed_frames} frames before the fixed probe deadline").into(),
        );
    }
    println!(
        "CAPYIO_CAMERA_SHARED_PROBE_OK scope={} stream={} epoch={} observed_frames={} discontinuities={} first_sequence={} last_sequence={} first_checksum={:016x} last_checksum={:016x}",
        scope,
        stream_id,
        stream_epoch,
        observed_frames,
        discontinuities,
        first_sequence.ok_or("missing first shared sequence")?,
        last_sequence,
        first_checksum.ok_or("missing first shared checksum")?,
        last_checksum.ok_or("missing last shared checksum")?
    );
    Ok(())
}

#[cfg(windows)]
fn open_selected_mapping(
    local_lab: bool,
) -> Result<CameraSharedIngressConsumer, CameraSharedIngressError> {
    if local_lab {
        #[cfg(feature = "lab-support")]
        {
            return CameraSharedIngressConsumer::open_local_lab_current();
        }
        #[cfg(not(feature = "lab-support"))]
        {
            return Err(CameraSharedIngressError::InvalidMappingName);
        }
    }
    CameraSharedIngressConsumer::open_current()
}

#[cfg(windows)]
fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}
