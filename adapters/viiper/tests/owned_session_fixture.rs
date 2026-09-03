use std::{
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    sync::mpsc::{self, Receiver},
    thread::{self, JoinHandle},
    time::Duration,
};

use capyio_core::StreamId;
use capyio_input::{
    GamepadButton, GamepadButtons, GamepadControls, GamepadState, InputContractError,
    InputFrameHeader,
};
use capyio_viiper_adapter::{
    ViiperAutoAttachDisabled, ViiperClientError, ViiperLoopbackClient, ViiperLoopbackConfig,
    ViiperSessionError, ViiperXbox360Error, ViiperXbox360Mapping, ViiperXbox360WorkerState,
    Xbox360RumbleFeedback,
};

const TEST_TIMEOUT: Duration = Duration::from_secs(2);
const BUS_ID: u32 = 7;
const DEVICE_ID: &str = "3";
const HANDSHAKE: &[u8] = b"bus/7/3\0";
const NEUTRAL: [u8; 20] = [0; 20];
const SOUTH: [u8; 20] = [
    0x00, 0x10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];
const EAST: [u8; 20] = [
    0x00, 0x20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

#[test]
fn owned_session_orders_provision_gap_epoch_and_stop_cleanup() {
    let fixture = OwnedFixture::start(StreamBehavior::Feedback(Vec::new()));
    let client = client(fixture.address, TEST_TIMEOUT);
    let stream_id = StreamId::new();
    let mut worker = client
        .open_xbox360(
            ViiperAutoAttachDisabled::confirmed_by_caller(),
            state(stream_id, 1, 10, GamepadControls::neutral()),
            ViiperXbox360Mapping::preserve(),
        )
        .unwrap();

    assert_eq!(worker.state(), ViiperXbox360WorkerState::Running);
    assert_eq!(worker.bus_id(), BUS_ID);
    assert_eq!(worker.device_id(), DEVICE_ID);

    let unsupported = state(stream_id, 1, 10, controls_with(GamepadButton::Touchpad));
    assert_eq!(
        worker.submit(unsupported),
        Err(ViiperSessionError::Projection(
            ViiperXbox360Error::UnsupportedButtons(1 << GamepadButton::Touchpad as u8)
        ))
    );

    let first = worker
        .submit(state(stream_id, 1, 10, controls_with(GamepadButton::South)))
        .unwrap();
    assert_eq!(first.gap(), None);
    assert!(!first.exhausted());
    assert!(matches!(
        worker.submit(state(stream_id, 1, 10, controls_with(GamepadButton::East))),
        Err(ViiperSessionError::Input(
            InputContractError::DuplicateOrLate { .. }
        ))
    ));

    let recovered = worker
        .submit(state(stream_id, 1, 12, controls_with(GamepadButton::East)))
        .unwrap();
    let gap = recovered.gap().unwrap();
    assert_eq!(gap.first_missing, 11);
    assert_eq!(gap.last_missing, 11);

    worker.request_neutral().unwrap();
    assert_eq!(worker.state(), ViiperXbox360WorkerState::Running);

    worker.advance_epoch(2, 0).unwrap();
    assert!(matches!(
        worker.submit(state(stream_id, 3, 0, controls_with(GamepadButton::South))),
        Err(ViiperSessionError::Input(
            InputContractError::FutureEpoch { .. }
        ))
    ));
    worker
        .submit(state(stream_id, 2, 0, controls_with(GamepadButton::South)))
        .unwrap();

    worker.stop().unwrap();
    assert_eq!(worker.state(), ViiperXbox360WorkerState::Stopped);
    worker.stop().unwrap();

    let observed = fixture.finish();
    assert_eq!(
        observed.management,
        expected_management_requests()
            .into_iter()
            .map(Vec::from)
            .collect::<Vec<_>>()
    );
    let mut expected_stream = HANDSHAKE.to_vec();
    for frame in [
        NEUTRAL, SOUTH, NEUTRAL, EAST, NEUTRAL, NEUTRAL, SOUTH, NEUTRAL,
    ] {
        expected_stream.extend_from_slice(&frame);
    }
    assert_eq!(observed.stream, expected_stream);
}

#[test]
fn sequence_exhaustion_writes_neutral_and_requires_explicit_epoch_advance() {
    let fixture = OwnedFixture::start(StreamBehavior::Feedback(Vec::new()));
    let client = client(fixture.address, TEST_TIMEOUT);
    let stream_id = StreamId::new();
    let mut worker = client
        .open_xbox360(
            ViiperAutoAttachDisabled::confirmed_by_caller(),
            state(stream_id, 1, u64::MAX, GamepadControls::neutral()),
            ViiperXbox360Mapping::preserve(),
        )
        .unwrap();

    let exhausted = worker
        .submit(state(
            stream_id,
            1,
            u64::MAX,
            controls_with(GamepadButton::South),
        ))
        .unwrap();
    assert!(exhausted.exhausted());
    assert_eq!(worker.state(), ViiperXbox360WorkerState::Exhausted);
    assert_eq!(
        worker.submit(state(
            stream_id,
            1,
            u64::MAX,
            controls_with(GamepadButton::East)
        )),
        Err(ViiperSessionError::WorkerNotRunning(
            ViiperXbox360WorkerState::Exhausted
        ))
    );

    worker.advance_epoch(2, 5).unwrap();
    worker
        .submit(state(stream_id, 2, 5, controls_with(GamepadButton::East)))
        .unwrap();
    worker.stop().unwrap();

    let observed = fixture.finish();
    let mut expected_stream = HANDSHAKE.to_vec();
    for frame in [NEUTRAL, SOUTH, NEUTRAL, NEUTRAL, EAST, NEUTRAL] {
        expected_stream.extend_from_slice(&frame);
    }
    assert_eq!(observed.stream, expected_stream);
}

#[test]
fn rumble_frames_are_exact_and_no_feedback_timeout_is_non_terminal() {
    let fixture = OwnedFixture::start(StreamBehavior::Feedback(vec![1, 2, 3, 4]));
    let client = client(fixture.address, Duration::from_millis(50));
    let stream_id = StreamId::new();
    let mut worker = client
        .open_xbox360(
            ViiperAutoAttachDisabled::confirmed_by_caller(),
            state(stream_id, 1, 0, GamepadControls::neutral()),
            ViiperXbox360Mapping::preserve(),
        )
        .unwrap();

    assert_eq!(
        worker.poll_rumble().unwrap(),
        Some(Xbox360RumbleFeedback {
            left_motor: 1,
            right_motor: 2,
        })
    );
    assert_eq!(
        worker.poll_rumble().unwrap(),
        Some(Xbox360RumbleFeedback {
            left_motor: 3,
            right_motor: 4,
        })
    );
    assert_eq!(worker.poll_rumble().unwrap(), None);
    assert_eq!(worker.state(), ViiperXbox360WorkerState::Running);
    worker.stop().unwrap();
    fixture.finish();
}

#[test]
fn partial_feedback_is_terminal_but_stop_still_removes_owned_bus() {
    let fixture = OwnedFixture::start(StreamBehavior::PartialFeedback(0x55));
    let client = client(fixture.address, TEST_TIMEOUT);
    let stream_id = StreamId::new();
    let mut worker = client
        .open_xbox360(
            ViiperAutoAttachDisabled::confirmed_by_caller(),
            state(stream_id, 1, 0, GamepadControls::neutral()),
            ViiperXbox360Mapping::preserve(),
        )
        .unwrap();

    assert_eq!(
        worker.poll_rumble(),
        Err(ViiperSessionError::TruncatedFeedback {
            actual: 1,
            expected: 2,
        })
    );
    assert_eq!(worker.state(), ViiperXbox360WorkerState::Failed);
    worker.stop().unwrap();
    assert_eq!(worker.state(), ViiperXbox360WorkerState::Stopped);
    fixture.finish();
}

#[test]
fn clean_peer_close_before_feedback_is_distinct_and_cleanup_still_runs() {
    let fixture = OwnedFixture::start(StreamBehavior::PeerClosed);
    let client = client(fixture.address, TEST_TIMEOUT);
    let stream_id = StreamId::new();
    let mut worker = client
        .open_xbox360(
            ViiperAutoAttachDisabled::confirmed_by_caller(),
            state(stream_id, 1, 0, GamepadControls::neutral()),
            ViiperXbox360Mapping::preserve(),
        )
        .unwrap();

    assert_eq!(
        worker.poll_rumble(),
        Err(ViiperSessionError::StreamPeerClosed)
    );
    assert_eq!(worker.state(), ViiperXbox360WorkerState::Failed);
    worker.stop().unwrap();
    fixture.finish();
}

#[test]
fn add_failure_rolls_back_the_known_owned_bus() {
    let fixture = RollbackFixture::start(
        br#"{"status":409,"title":"Conflict","detail":"fixture add failed"}"#.to_vec(),
        br#"{"busId":7}"#.to_vec(),
    );
    let client = client(fixture.address, TEST_TIMEOUT);
    let error = client
        .open_xbox360(
            ViiperAutoAttachDisabled::confirmed_by_caller(),
            state(StreamId::new(), 1, 0, GamepadControls::neutral()),
            ViiperXbox360Mapping::preserve(),
        )
        .unwrap_err();

    assert!(matches!(
        error.cause(),
        ViiperSessionError::Client(ViiperClientError::RemoteProblem { status: 409, .. })
    ));
    assert_eq!(error.cleanup(), None);
    assert_eq!(
        fixture.finish(),
        vec![
            b"ping\0".to_vec(),
            b"bus/create\0".to_vec(),
            add_request(),
            b"bus/remove 7\0".to_vec()
        ]
    );
}

#[test]
fn rollback_failure_is_preserved_with_the_primary_open_failure() {
    let fixture = RollbackFixture::start(
        br#"{"busId":7,"devId":"../1","vid":"0x045e","pid":"0x028e","type":"xbox360","deviceSpecific":{"subType":1}}"#.to_vec(),
        b"not-json".to_vec(),
    );
    let client = client(fixture.address, TEST_TIMEOUT);
    let error = client
        .open_xbox360(
            ViiperAutoAttachDisabled::confirmed_by_caller(),
            state(StreamId::new(), 1, 0, GamepadControls::neutral()),
            ViiperXbox360Mapping::preserve(),
        )
        .unwrap_err();

    assert_eq!(error.cause(), &ViiperSessionError::InvalidDeviceId);
    assert!(matches!(
        error.cleanup(),
        Some(ViiperSessionError::Client(ViiperClientError::InvalidJson(
            _
        )))
    ));
    fixture.finish();
}

fn client(address: SocketAddr, io_timeout: Duration) -> ViiperLoopbackClient {
    ViiperLoopbackClient::new(
        ViiperLoopbackConfig::new(address, TEST_TIMEOUT, io_timeout, 4096).unwrap(),
    )
}

fn state(
    stream_id: StreamId,
    stream_epoch: u64,
    sequence: u64,
    controls: GamepadControls,
) -> GamepadState {
    GamepadState {
        header: InputFrameHeader {
            stream_id,
            stream_epoch,
            sequence,
            source_timestamp_nanos: sequence,
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

fn add_request() -> Vec<u8> {
    b"bus/7/add {\"type\":\"xbox360\"}\0".to_vec()
}

fn expected_management_requests() -> [&'static [u8]; 4] {
    [
        b"ping\0",
        b"bus/create\0",
        b"bus/7/add {\"type\":\"xbox360\"}\0",
        b"bus/remove 7\0",
    ]
}

#[derive(Debug)]
enum StreamBehavior {
    Feedback(Vec<u8>),
    PartialFeedback(u8),
    PeerClosed,
}

#[derive(Debug)]
struct Observed {
    management: Vec<Vec<u8>>,
    stream: Vec<u8>,
}

struct OwnedFixture {
    address: SocketAddr,
    observed: Receiver<Observed>,
    server: JoinHandle<()>,
}

impl OwnedFixture {
    fn start(behavior: StreamBehavior) -> Self {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let (observed_tx, observed) = mpsc::channel();
        let server = thread::spawn(move || {
            let mut management = Vec::new();
            serve_management(&listener, &mut management, b"ping\0", compatible_response());
            serve_management(
                &listener,
                &mut management,
                b"bus/create\0",
                br#"{"busId":7}"#,
            );
            serve_management(
                &listener,
                &mut management,
                &add_request(),
                br#"{"busId":7,"devId":"3","vid":"0x045e","pid":"0x028e","type":"xbox360","deviceSpecific":{"subType":1}}"#,
            );

            let (mut stream, peer) = listener.accept().unwrap();
            assert!(peer.ip().is_loopback());
            configure(&stream);
            let mut initial = vec![0_u8; HANDSHAKE.len() + NEUTRAL.len()];
            stream.read_exact(&mut initial).unwrap();
            match behavior {
                StreamBehavior::Feedback(bytes) => stream.write_all(&bytes).unwrap(),
                StreamBehavior::PartialFeedback(byte) => {
                    stream.write_all(&[byte]).unwrap();
                    stream.shutdown(std::net::Shutdown::Write).unwrap();
                }
                StreamBehavior::PeerClosed => {
                    stream.shutdown(std::net::Shutdown::Write).unwrap();
                }
            }
            let mut remainder = Vec::new();
            stream.read_to_end(&mut remainder).unwrap();
            initial.extend_from_slice(&remainder);

            serve_management(
                &listener,
                &mut management,
                b"bus/remove 7\0",
                br#"{"busId":7}"#,
            );
            observed_tx
                .send(Observed {
                    management,
                    stream: initial,
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

struct RollbackFixture {
    address: SocketAddr,
    observed: Receiver<Vec<Vec<u8>>>,
    server: JoinHandle<()>,
}

impl RollbackFixture {
    fn start(add_response: Vec<u8>, remove_response: Vec<u8>) -> Self {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let (observed_tx, observed) = mpsc::channel();
        let server = thread::spawn(move || {
            let mut requests = Vec::new();
            serve_management(&listener, &mut requests, b"ping\0", compatible_response());
            serve_management(&listener, &mut requests, b"bus/create\0", br#"{"busId":7}"#);
            serve_management(&listener, &mut requests, &add_request(), &add_response);
            serve_management(
                &listener,
                &mut requests,
                b"bus/remove 7\0",
                &remove_response,
            );
            observed_tx.send(requests).unwrap();
        });
        Self {
            address,
            observed,
            server,
        }
    }

    fn finish(self) -> Vec<Vec<u8>> {
        let observed = self.observed.recv_timeout(TEST_TIMEOUT).unwrap();
        self.server.join().unwrap();
        observed
    }
}

fn serve_management(
    listener: &TcpListener,
    observed: &mut Vec<Vec<u8>>,
    expected_request: &[u8],
    response: &[u8],
) {
    let (mut stream, peer) = listener.accept().unwrap();
    assert!(peer.ip().is_loopback());
    configure(&stream);
    let mut request = Vec::new();
    stream.read_to_end(&mut request).unwrap();
    assert_eq!(request, expected_request);
    observed.push(request);
    stream.write_all(response).unwrap();
}

fn configure(stream: &TcpStream) {
    stream.set_read_timeout(Some(TEST_TIMEOUT)).unwrap();
    stream.set_write_timeout(Some(TEST_TIMEOUT)).unwrap();
}

fn compatible_response() -> &'static [u8] {
    br#"{"server":"VIIPER","version":"0.7.0"}"#
}
