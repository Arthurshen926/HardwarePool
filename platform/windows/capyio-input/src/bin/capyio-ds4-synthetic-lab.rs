use std::io::{self, Write};
use std::net::{Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};

use capyio_core::StreamId;
use capyio_data_plane::{
    DataEnvelope, ImuAccuracy, ImuCalibration, ImuCoordinateFrame, ImuSampleV1,
    ImuSensorMetadataV1, ImuUnitsV1,
};
use capyio_input::{GamepadButton, GamepadControlUpdate, GamepadStateComposer};
use capyio_viiper_adapter::{
    MAX_VIIPER_MANAGEMENT_RESPONSE_BYTES, ViiperAutoAttachDisabled, ViiperDs4ControlsMapping,
    ViiperDs4MotionMapping, ViiperLoopbackClient, ViiperLoopbackConfig,
};

const DEFAULT_VIIPER_PORT: u16 = 3242;
const MINIMUM_HOLD_SECONDS: u64 = 5;
const MAXIMUM_HOLD_SECONDS: u64 = 300;
const REPORT_INTERVAL: Duration = Duration::from_millis(16);

fn main() {
    if let Err(error) = run(std::env::args().skip(1)) {
        eprintln!("CAPYIO_SYNTHETIC_DS4_LAB_FAILED: {error}");
        std::process::exit(2);
    }
    println!("CAPYIO_SYNTHETIC_DS4_LAB_PASSED");
}

fn run(arguments: impl Iterator<Item = String>) -> Result<(), String> {
    let (viiper_port, hold_seconds) = parse_arguments(arguments)?;
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, viiper_port));
    let client = ViiperLoopbackClient::new(
        ViiperLoopbackConfig::new(
            address,
            Duration::from_secs(2),
            Duration::from_secs(2),
            MAX_VIIPER_MANAGEMENT_RESPONSE_BYTES,
        )
        .map_err(|error| error.to_string())?,
    );
    let probe = client.probe().map_err(|error| error.to_string())?;
    let mut composer =
        GamepadStateComposer::new(StreamId::new(), 1, 0).map_err(|error| error.to_string())?;
    let controls_anchor = composer.anchor(1).map_err(|error| error.to_string())?;
    let mut motion = stationary_motion();
    let mut worker = client
        .open_dualshock4(
            ViiperAutoAttachDisabled::confirmed_by_caller(),
            controls_anchor,
            &motion,
            ViiperDs4ControlsMapping::gamepad_y_up(),
            ViiperDs4MotionMapping::identity(),
        )
        .map_err(|error| error.to_string())?;

    println!(
        "CAPYIO_SYNTHETIC_DS4_VIIPER={}:{}",
        probe.server(),
        probe.version()
    );
    println!("CAPYIO_SYNTHETIC_DS4_BUS_ID={}", worker.bus_id());
    println!("CAPYIO_SYNTHETIC_DS4_DEVICE_ID={}", worker.device_id());
    println!(
        "CAPYIO_SYNTHETIC_DS4_USBIP_BUS={}-{}",
        worker.bus_id(),
        worker.device_id()
    );
    println!("CAPYIO_SYNTHETIC_DS4_READY");
    io::stdout()
        .flush()
        .map_err(|error| format!("could not flush synthetic DS4 instructions: {error}"))?;

    let deadline = Instant::now() + Duration::from_secs(hold_seconds);
    let mut pressed = false;
    let mut reports = 0_u64;
    let mut sequence = 0_u64;
    let result = (|| -> Result<(), String> {
        while Instant::now() < deadline {
            pressed = !pressed;
            let timestamp = sequence.saturating_add(1);
            let state = composer
                .apply(
                    GamepadControlUpdate::Button {
                        button: GamepadButton::South,
                        pressed,
                    },
                    timestamp,
                )
                .map_err(|error| error.to_string())?;
            motion.sequence = sequence;
            motion.source_timestamp_nanos = timestamp;
            motion.receive_timestamp_nanos = timestamp;
            worker
                .submit(state, &motion)
                .map_err(|error| error.to_string())?;
            reports = reports.saturating_add(1);
            sequence = sequence
                .checked_add(1)
                .ok_or_else(|| "synthetic DS4 sequence exhausted".to_owned())?;
            std::thread::sleep(REPORT_INTERVAL);
        }
        Ok(())
    })();
    let cleanup = worker.stop().err().map(|error| error.to_string());
    println!("CAPYIO_SYNTHETIC_DS4_RESULT=reports={reports} toggled_south=true finite_imu=true");
    result?;
    if let Some(error) = cleanup {
        return Err(format!("synthetic DS4 cleanup failed: {error}"));
    }
    if reports == 0 {
        return Err("synthetic DS4 Gate submitted no reports".to_owned());
    }
    Ok(())
}

fn parse_arguments(mut arguments: impl Iterator<Item = String>) -> Result<(u16, u64), String> {
    let viiper_port = arguments
        .next()
        .unwrap_or_else(|| DEFAULT_VIIPER_PORT.to_string())
        .parse::<u16>()
        .map_err(|_| usage())?;
    let hold_seconds = arguments
        .next()
        .unwrap_or_else(|| "60".to_owned())
        .parse::<u64>()
        .map_err(|_| usage())?;
    if arguments.next().is_some()
        || viiper_port == 0
        || !(MINIMUM_HOLD_SECONDS..=MAXIMUM_HOLD_SECONDS).contains(&hold_seconds)
    {
        return Err(usage());
    }
    Ok((viiper_port, hold_seconds))
}

fn usage() -> String {
    "usage: capyio-ds4-synthetic-lab [viiper-api-port] [hold-seconds]; hold must be within 5..=300"
        .to_owned()
}

fn stationary_motion() -> DataEnvelope<ImuSampleV1> {
    DataEnvelope {
        profile: ImuSampleV1::profile(),
        stream_id: StreamId::new(),
        stream_epoch: 1,
        sequence: 0,
        source_timestamp_nanos: 1,
        receive_timestamp_nanos: 1,
        clock_domain_id: "capyio.synthetic_ds4_lab".to_owned(),
        payload: ImuSampleV1 {
            acceleration: [0.0, 0.0, 9.806_65],
            angular_velocity: [0.0; 3],
            magnetic_field: None,
            units: ImuUnitsV1::default(),
            coordinate_frame: ImuCoordinateFrame::AndroidDeviceXRightYUpZOut,
            accuracy: ImuAccuracy::High,
            calibration: ImuCalibration::RuntimeCalibrated,
            sensor: ImuSensorMetadataV1 {
                sensor_name: "CapyIO synthetic DS4 Gate".to_owned(),
                vendor: "CapyIO".to_owned(),
                version: 1,
                android_sensor_type: None,
            },
            component_timestamps: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_VIIPER_PORT, parse_arguments};

    #[test]
    fn arguments_are_bounded_and_have_closed_defaults() {
        assert_eq!(
            parse_arguments(Vec::<String>::new().into_iter()).unwrap(),
            (DEFAULT_VIIPER_PORT, 60)
        );
        assert_eq!(
            parse_arguments(["4242".to_owned(), "5".to_owned()].into_iter()).unwrap(),
            (4242, 5)
        );
        assert!(parse_arguments(["0".to_owned(), "5".to_owned()].into_iter()).is_err());
        assert!(parse_arguments(["3242".to_owned(), "301".to_owned()].into_iter()).is_err());
        assert!(
            parse_arguments(["3242".to_owned(), "5".to_owned(), "extra".to_owned()].into_iter())
                .is_err()
        );
    }
}
