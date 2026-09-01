use std::{cell::RefCell, error::Error, fmt, rc::Rc};

use capyio_core::{PortRef, RouteId, SessionId};
use capyio_input::{
    InputFrameHeader, InputStreamDescriptor, TouchpadButtonState, TouchpadButtonType,
    TouchpadContact, TouchpadDescriptor, TouchpadFrame, TouchpadFrameKind, TouchpadPhysicalSize,
    TouchpadPosition,
};
use capyio_remote_touchpad_adapter::{
    MIN_PRIVATE_TOUCHPAD_IDLE_TIMEOUT_NANOS, PrivateTouchpadPacketCodecV1,
    PrivateTouchpadPollOutcome, PrivateTouchpadReceiverLimits, PrivateTouchpadRouteBinding,
    PrivateTouchpadSink, PrivateTouchpadSinkFactory, PrivateTouchpadTransportCodecV1,
    PrivateTouchpadTransportReceiver, PrivateTouchpadTransportReceiverBuildError,
    PrivateTouchpadTransportReceiverError, PrivateTouchpadTransportReceiverState,
};

fn id<T: std::str::FromStr>(value: &str) -> T
where
    T::Err: fmt::Debug,
{
    value.parse().expect("fixture ID")
}

fn port(node: &str, capability: &str, port: &str) -> PortRef {
    PortRef {
        node_id: id(node),
        capability_id: id(capability),
        port_id: id(port),
    }
}

fn binding(epoch: u64) -> PrivateTouchpadRouteBinding {
    PrivateTouchpadRouteBinding {
        route_id: id::<RouteId>("00000000-0000-4000-8000-00000000f101"),
        session_id: id::<SessionId>("00000000-0000-4000-8000-00000000f102"),
        source: port(
            "00000000-0000-4000-8000-00000000f103",
            "00000000-0000-4000-8000-00000000f104",
            "00000000-0000-4000-8000-00000000f105",
        ),
        sink: port(
            "00000000-0000-4000-8000-00000000f106",
            "00000000-0000-4000-8000-00000000f107",
            "00000000-0000-4000-8000-00000000f108",
        ),
        route_epoch: epoch,
        authorization_expires_at_ms: None,
    }
}

fn stream(epoch: u64) -> InputStreamDescriptor {
    InputStreamDescriptor {
        stream_id: id("00000000-0000-4000-8000-00000000f109"),
        stream_epoch: epoch,
        clock_domain_id: "android.uptime_nanos".to_owned(),
    }
}

fn descriptor() -> TouchpadDescriptor {
    TouchpadDescriptor {
        physical_size: TouchpadPhysicalSize {
            width_himetric: 12_000,
            height_himetric: 7_000,
        },
        max_contacts: 5,
        button_type: TouchpadButtonType::NonClickable,
        reports_contact_size: false,
        reports_pressure: false,
    }
}

fn limits() -> PrivateTouchpadReceiverLimits {
    PrivateTouchpadReceiverLimits {
        max_packets_per_second: 240,
        active_idle_timeout_nanos: MIN_PRIVATE_TOUCHPAD_IDLE_TIMEOUT_NANOS,
    }
}

fn frame(sequence: u64, active: bool) -> TouchpadFrame {
    TouchpadFrame {
        header: InputFrameHeader {
            stream_id: stream(1).stream_id,
            stream_epoch: 1,
            sequence,
            source_timestamp_nanos: 1_000_000 + sequence * 16_000_000,
        },
        kind: TouchpadFrameKind::Update,
        button: TouchpadButtonState::Released,
        contacts: active
            .then_some(TouchpadContact {
                contact_id: 1,
                position: TouchpadPosition {
                    x_himetric: 2_000,
                    y_himetric: 3_000,
                },
                confidence: true,
                size: None,
                pressure: None,
            })
            .into_iter()
            .collect(),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FakeError {
    Open,
    Close,
}

impl fmt::Display for FakeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "fake {self:?} failure")
    }
}

impl Error for FakeError {}

#[derive(Default)]
struct FakeState {
    opens: usize,
    frames: Vec<TouchpadFrame>,
    closes: usize,
}

struct FakeSink {
    shared: Rc<RefCell<FakeState>>,
    fail_close: bool,
}

impl PrivateTouchpadSink for FakeSink {
    type Error = FakeError;

    fn submit_frame(&mut self, frame: &TouchpadFrame) -> Result<(), Self::Error> {
        self.shared.borrow_mut().frames.push(frame.clone());
        Ok(())
    }

    fn advance_epoch(&mut self, _new_epoch: u64, _first_sequence: u64) -> Result<(), Self::Error> {
        Ok(())
    }

    fn close(&mut self) -> Result<(), Self::Error> {
        self.shared.borrow_mut().closes += 1;
        if self.fail_close {
            Err(FakeError::Close)
        } else {
            Ok(())
        }
    }
}

struct FakeFactory {
    shared: Rc<RefCell<FakeState>>,
    fail_open: bool,
    fail_close: bool,
}

impl PrivateTouchpadSinkFactory for FakeFactory {
    type Sink = FakeSink;
    type Error = FakeError;

    fn open(
        &mut self,
        _stream: &InputStreamDescriptor,
        _descriptor: TouchpadDescriptor,
        _first_sequence: u64,
    ) -> Result<Self::Sink, Self::Error> {
        self.shared.borrow_mut().opens += 1;
        if self.fail_open {
            return Err(FakeError::Open);
        }
        Ok(FakeSink {
            shared: Rc::clone(&self.shared),
            fail_close: self.fail_close,
        })
    }
}

fn factory(shared: &Rc<RefCell<FakeState>>) -> FakeFactory {
    FakeFactory {
        shared: Rc::clone(shared),
        fail_open: false,
        fail_close: false,
    }
}

fn receiver(shared: &Rc<RefCell<FakeState>>) -> PrivateTouchpadTransportReceiver<FakeFactory> {
    PrivateTouchpadTransportReceiver::new(
        binding(1),
        stream(1),
        descriptor(),
        0,
        limits(),
        factory(shared),
    )
    .expect("transport receiver")
}

fn data(codec: &PrivateTouchpadTransportCodecV1, frame: &TouchpadFrame) -> Vec<u8> {
    let packet = PrivateTouchpadPacketCodecV1::new(stream(1), descriptor())
        .expect("packet codec")
        .encode(frame)
        .expect("packet");
    codec
        .encode_data(&packet)
        .expect("data record")
        .as_bytes()
        .to_vec()
}

#[test]
fn construction_rejects_epoch_mismatch_before_factory_open() {
    let shared = Rc::new(RefCell::new(FakeState::default()));
    let result = PrivateTouchpadTransportReceiver::new(
        binding(1),
        stream(2),
        descriptor(),
        0,
        limits(),
        factory(&shared),
    );
    assert!(matches!(
        result,
        Err(PrivateTouchpadTransportReceiverBuildError::EpochMismatch {
            route: 1,
            stream: 2
        })
    ));
    assert_eq!(shared.borrow().opens, 0);
}

#[test]
fn mismatched_hello_fails_before_factory_open() {
    let shared = Rc::new(RefCell::new(FakeState::default()));
    let mut receiver = receiver(&shared);
    let wrong = PrivateTouchpadTransportCodecV1::new(binding(2)).encode_hello();
    assert!(matches!(
        receiver.accept_hello(wrong.as_bytes()),
        Err(PrivateTouchpadTransportReceiverError::Transport { .. })
    ));
    assert_eq!(
        receiver.state(),
        PrivateTouchpadTransportReceiverState::Failed
    );
    assert_eq!(shared.borrow().opens, 0);
}

#[test]
fn hello_data_ack_release_and_close_are_one_bounded_lifecycle() {
    let shared = Rc::new(RefCell::new(FakeState::default()));
    let codec = PrivateTouchpadTransportCodecV1::new(binding(1));
    let mut receiver = receiver(&shared);
    receiver
        .accept_hello(codec.encode_hello().as_bytes())
        .expect("Hello");
    assert_eq!(shared.borrow().opens, 1);
    assert_eq!(
        receiver.state(),
        PrivateTouchpadTransportReceiverState::Active
    );

    let active = receiver
        .receive_data(&data(&codec, &frame(0, true)), 10)
        .expect("active data");
    assert_eq!(active.receive.active_contacts, 1);
    codec
        .validate_ack(active.ack.as_bytes(), 0)
        .expect("active Ack");
    let released = receiver
        .receive_data(&data(&codec, &frame(1, false)), 20)
        .expect("release data");
    assert_eq!(released.receive.active_contacts, 0);
    codec
        .validate_ack(released.ack.as_bytes(), 1)
        .expect("release Ack");
    receiver
        .accept_close(codec.encode_close().as_bytes())
        .expect("Close");

    assert_eq!(
        receiver.state(),
        PrivateTouchpadTransportReceiverState::Closed
    );
    assert_eq!(
        shared.borrow().frames,
        vec![frame(0, true), frame(1, false)]
    );
    assert_eq!(shared.borrow().closes, 1);
    assert!(matches!(
        receiver.receive_data(&data(&codec, &frame(2, false)), 30),
        Err(PrivateTouchpadTransportReceiverError::Inactive(
            PrivateTouchpadTransportReceiverState::Closed
        ))
    ));
}

#[test]
fn malformed_data_fails_closed_after_hello() {
    let shared = Rc::new(RefCell::new(FakeState::default()));
    let codec = PrivateTouchpadTransportCodecV1::new(binding(1));
    let mut receiver = receiver(&shared);
    receiver
        .accept_hello(codec.encode_hello().as_bytes())
        .expect("Hello");
    let mut malformed = data(&codec, &frame(0, true));
    malformed[6] = 0x80;
    assert!(matches!(
        receiver.receive_data(&malformed, 10),
        Err(PrivateTouchpadTransportReceiverError::Transport { cleanup: None, .. })
    ));
    assert_eq!(
        receiver.state(),
        PrivateTouchpadTransportReceiverState::Failed
    );
    assert_eq!(shared.borrow().frames.len(), 0);
    assert_eq!(shared.borrow().closes, 1);
}

#[test]
fn factory_failure_and_idle_timeout_are_terminal() {
    let failed_shared = Rc::new(RefCell::new(FakeState::default()));
    let mut failed = PrivateTouchpadTransportReceiver::new(
        binding(1),
        stream(1),
        descriptor(),
        0,
        limits(),
        FakeFactory {
            shared: Rc::clone(&failed_shared),
            fail_open: true,
            fail_close: false,
        },
    )
    .expect("transport receiver");
    let hello = PrivateTouchpadTransportCodecV1::new(binding(1)).encode_hello();
    assert!(matches!(
        failed.accept_hello(hello.as_bytes()),
        Err(PrivateTouchpadTransportReceiverError::SinkFactory(
            FakeError::Open
        ))
    ));
    assert_eq!(
        failed.state(),
        PrivateTouchpadTransportReceiverState::Failed
    );

    let shared = Rc::new(RefCell::new(FakeState::default()));
    let codec = PrivateTouchpadTransportCodecV1::new(binding(1));
    let mut timed_out = receiver(&shared);
    timed_out
        .accept_hello(codec.encode_hello().as_bytes())
        .expect("Hello");
    timed_out
        .receive_data(&data(&codec, &frame(0, true)), 10)
        .expect("active data");
    assert_eq!(
        timed_out
            .poll_timeout(10 + MIN_PRIVATE_TOUCHPAD_IDLE_TIMEOUT_NANOS)
            .expect("timeout"),
        PrivateTouchpadPollOutcome::TimedOut
    );
    assert_eq!(
        timed_out.state(),
        PrivateTouchpadTransportReceiverState::TimedOut
    );
    assert_eq!(shared.borrow().closes, 1);
}
