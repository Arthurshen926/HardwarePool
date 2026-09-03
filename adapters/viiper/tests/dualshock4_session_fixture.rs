use std::{
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    sync::mpsc::{self, Receiver},
    thread::{self, JoinHandle},
    time::Duration,
};

use capyio_core::StreamId;
use capyio_data_plane::{DataEnvelope, ImuSampleV1, parse_imu_fixture_jsonl};
use capyio_input::{
    GamepadButton, GamepadButtons, GamepadControls, GamepadState, InputFrameHeader,
};
use capyio_viiper_adapter::{
    ViiperAutoAttachDisabled, ViiperDs4ControlsMapping, ViiperDs4MotionMapping,
    ViiperDs4SessionError, ViiperDs4WorkerState, ViiperLoopbackClient, ViiperLoopbackConfig,
};

const FIXTURE_IMU: &str = include_str!("../../../fixtures/imu/imu_samples_v1.jsonl");
const TEST_TIMEOUT: Duration = Duration::from_secs(2);
const BUS_ID: u32 = 9;
const DEVICE_ID: &str = "4";
const HANDSHAKE: &[u8] = b"bus/9/4\0";

fn safe_state() -> [u8; 31] {
    let mut state = [0_u8; 31];
    state[29..31].copy_from_slice(&(-5023_i16).to_le_bytes());
    state
}

fn controls_with_fixture_motion(buttons: u16) -> [u8; 31] {
    let mut state = [0_u8; 31];
    state[4..6].copy_from_slice(&buttons.to_le_bytes());
    for (offset, value) in [
        (19, 1_i16),
        (21, 2),
        (23, -1),
        (25, 5),
        (27, -10),
        (29, 5018),
    ] {
        state[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }
    state
}

#[test]
fn owned_ds4_session_orders_safe_state_input_feedback_and_cleanup() {
    let fixture = Ds4Fixture::start(
        br#"{"busId":9,"devId":"4","vid":"0x054c","pid":"0x09cc","type":"dualshock4","deviceSpecific":{"serial_number":"1111020BF619A500"}}"#,
        vec![1, 2, 3, 4, 5, 6, 7],
    );
    let client = client(fixture.address);
    let controls_id = StreamId::new();
    let mut motion = imu_anchor();
    motion.sequence = 0;
    let mut worker = client
        .open_dualshock4(
            ViiperAutoAttachDisabled::confirmed_by_caller(),
            gamepad(controls_id, 1, 0, GamepadControls::neutral()),
            &motion,
            ViiperDs4ControlsMapping::preserve(),
            ViiperDs4MotionMapping::identity(),
        )
        .unwrap();

    assert_eq!(worker.state(), ViiperDs4WorkerState::Running);
    assert_eq!(worker.bus_id(), BUS_ID);
    assert_eq!(worker.device_id(), DEVICE_ID);
    let outcome = worker
        .submit(
            gamepad(controls_id, 1, 0, controls_with(GamepadButton::South)),
            &motion,
        )
        .unwrap();
    assert_eq!(outcome.gap(), None);
    assert!(!outcome.exhausted());

    let mut next_motion = motion.clone();
    next_motion.sequence = 1;
    let recovered = worker
        .submit(
            gamepad(controls_id, 1, 2, controls_with(GamepadButton::East)),
            &next_motion,
        )
        .unwrap();
    let gap = recovered.gap().unwrap();
    assert_eq!(gap.first_missing, 1);
    assert_eq!(gap.last_missing, 1);
    let feedback = worker.poll_feedback().unwrap().unwrap();
    assert_eq!([feedback.small_motor, feedback.large_motor], [1, 2]);
    assert_eq!(
        [feedback.led_red, feedback.led_green, feedback.led_blue],
        [3, 4, 5]
    );
    assert_eq!([feedback.flash_on, feedback.flash_off], [6, 7]);
    worker.request_safe_state().unwrap();
    worker.stop().unwrap();
    assert_eq!(worker.state(), ViiperDs4WorkerState::Stopped);
    worker.stop().unwrap();

    let observed = fixture.finish();
    assert_eq!(
        observed.management,
        vec![
            b"ping\0".to_vec(),
            b"bus/create\0".to_vec(),
            b"bus/9/add {\"type\":\"dualshock4\"}\0".to_vec(),
            b"bus/remove 9\0".to_vec(),
        ]
    );
    let mut expected = HANDSHAKE.to_vec();
    expected.extend_from_slice(&safe_state());
    expected.extend_from_slice(&controls_with_fixture_motion(0x0020));
    // Only the controls stream skipped sequence 1. The fail-safe report clears
    // controls but preserves the last valid motion before accepting the later
    // complete state.
    expected.extend_from_slice(&controls_with_fixture_motion(0));
    expected.extend_from_slice(&controls_with_fixture_motion(0x0040));
    expected.extend_from_slice(&safe_state());
    expected.extend_from_slice(&safe_state());
    assert_eq!(observed.stream, expected);
}

#[test]
fn wrong_ds4_identity_rolls_back_owned_bus() {
    let fixture = Ds4Fixture::start(
        br#"{"busId":9,"devId":"4","vid":"0x054c","pid":"0x028e","type":"dualshock4","deviceSpecific":{}}"#,
        Vec::new(),
    );
    let client = client(fixture.address);
    let motion = imu_anchor();
    let error = client
        .open_dualshock4(
            ViiperAutoAttachDisabled::confirmed_by_caller(),
            gamepad(StreamId::new(), 1, 0, GamepadControls::neutral()),
            &motion,
            ViiperDs4ControlsMapping::preserve(),
            ViiperDs4MotionMapping::identity(),
        )
        .unwrap_err();
    assert_eq!(
        error.cause(),
        &ViiperDs4SessionError::ResponseMismatch("product ID")
    );
    assert_eq!(error.cleanup(), None);
    let observed = fixture.finish();
    assert_eq!(
        observed.management,
        vec![
            b"ping\0".to_vec(),
            b"bus/create\0".to_vec(),
            b"bus/9/add {\"type\":\"dualshock4\"}\0".to_vec(),
            b"bus/remove 9\0".to_vec(),
        ]
    );
    assert!(observed.stream.is_empty());
}

fn client(address: SocketAddr) -> ViiperLoopbackClient {
    ViiperLoopbackClient::new(
        ViiperLoopbackConfig::new(address, TEST_TIMEOUT, TEST_TIMEOUT, 4096).unwrap(),
    )
}

fn imu_anchor() -> DataEnvelope<ImuSampleV1> {
    parse_imu_fixture_jsonl(FIXTURE_IMU, 6).unwrap().remove(0)
}

fn gamepad(
    stream_id: StreamId,
    epoch: u64,
    sequence: u64,
    controls: GamepadControls,
) -> GamepadState {
    GamepadState {
        header: InputFrameHeader {
            stream_id,
            stream_epoch: epoch,
            sequence,
            source_timestamp_nanos: sequence + 1,
        },
        controls,
    }
}

fn controls_with(button: GamepadButton) -> GamepadControls {
    GamepadControls {
        buttons: GamepadButtons::empty().with(button),
        ..GamepadControls::neutral()
    }
}

#[derive(Debug)]
struct Observed {
    management: Vec<Vec<u8>>,
    stream: Vec<u8>,
}

struct Ds4Fixture {
    address: SocketAddr,
    observed: Receiver<Observed>,
    server: JoinHandle<()>,
}

impl Ds4Fixture {
    fn start(device_response: &'static [u8], feedback: Vec<u8>) -> Self {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let (observed_tx, observed) = mpsc::channel();
        let server = thread::spawn(move || {
            let mut management = Vec::new();
            serve_management(
                &listener,
                &mut management,
                b"ping\0",
                br#"{"server":"VIIPER","version":"0.7.0"}"#,
            );
            serve_management(
                &listener,
                &mut management,
                b"bus/create\0",
                br#"{"busId":9}"#,
            );
            serve_management(
                &listener,
                &mut management,
                b"bus/9/add {\"type\":\"dualshock4\"}\0",
                device_response,
            );

            let mut stream_bytes = Vec::new();
            if device_response
                .windows(b"\"pid\":\"0x09cc\"".len())
                .any(|window| window == b"\"pid\":\"0x09cc\"")
            {
                let (mut stream, peer) = listener.accept().unwrap();
                assert!(peer.ip().is_loopback());
                configure(&stream);
                let mut initial = vec![0_u8; HANDSHAKE.len() + 31];
                stream.read_exact(&mut initial).unwrap();
                stream.write_all(&feedback).unwrap();
                let mut remainder = Vec::new();
                stream.read_to_end(&mut remainder).unwrap();
                initial.extend_from_slice(&remainder);
                stream_bytes = initial;
            }
            serve_management(
                &listener,
                &mut management,
                b"bus/remove 9\0",
                br#"{"busId":9}"#,
            );
            observed_tx
                .send(Observed {
                    management,
                    stream: stream_bytes,
                })
                .unwrap();
        });
        Self {
            address,
            observed,
            server,
        }
    }

    fn finish(self) -> Observed {
        let observed = self.observed.recv_timeout(TEST_TIMEOUT).unwrap();
        self.server.join().unwrap();
        observed
    }
}

fn serve_management(
    listener: &TcpListener,
    observed: &mut Vec<Vec<u8>>,
    expected: &[u8],
    response: &[u8],
) {
    let (mut stream, peer) = listener.accept().unwrap();
    assert!(peer.ip().is_loopback());
    configure(&stream);
    let mut request = Vec::new();
    stream.read_to_end(&mut request).unwrap();
    assert_eq!(request, expected);
    observed.push(request);
    stream.write_all(response).unwrap();
}

fn configure(stream: &TcpStream) {
    stream.set_read_timeout(Some(TEST_TIMEOUT)).unwrap();
    stream.set_write_timeout(Some(TEST_TIMEOUT)).unwrap();
}
