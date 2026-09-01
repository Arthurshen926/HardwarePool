use std::{cell::RefCell, collections::BTreeSet, error::Error, fmt, rc::Rc};

use capyio_core::{
    AuthorizationState, FormatDescriptor, PortRef, QosMode, Route, RouteBackend, RouteId,
    RouteState, SessionId,
};
use capyio_input::{
    InputFrameHeader, InputStreamDescriptor, TouchpadButtonState, TouchpadButtonType,
    TouchpadContact, TouchpadDescriptor, TouchpadFrame, TouchpadFrameKind, TouchpadPhysicalSize,
    TouchpadPosition, touchpad_frame_format, touchpad_frames_profile,
};
use capyio_remote_touchpad_adapter::{
    MAX_PRIVATE_TOUCHPAD_QUEUE_PACKETS, MIN_PRIVATE_TOUCHPAD_IDLE_TIMEOUT_NANOS,
    PRIVATE_TOUCHPAD_PACKET_MAX_BYTES, PrivateTouchpadIngressLimits, PrivateTouchpadPacketCodecV1,
    PrivateTouchpadPollOutcome, PrivateTouchpadReceiverError, PrivateTouchpadReceiverLimits,
    PrivateTouchpadRouteBindingError, PrivateTouchpadRouteSession,
    PrivateTouchpadRouteSessionBuildError, PrivateTouchpadRouteSessionError,
    PrivateTouchpadRouteSessionFault, PrivateTouchpadRouteSessionState, PrivateTouchpadSink,
};
use capyio_windows_input::{WindowsTouchpadProjectionDisposition, WindowsTouchpadProjector};

fn source() -> PortRef {
    PortRef {
        node_id: "00000000-0000-4000-8000-00000000d001"
            .parse()
            .expect("source Node"),
        capability_id: "00000000-0000-4000-8000-00000000d002"
            .parse()
            .expect("source Capability"),
        port_id: "00000000-0000-4000-8000-00000000d003"
            .parse()
            .expect("source Port"),
    }
}

fn sink() -> PortRef {
    PortRef {
        node_id: "00000000-0000-4000-8000-00000000d011"
            .parse()
            .expect("sink Node"),
        capability_id: "00000000-0000-4000-8000-00000000d012"
            .parse()
            .expect("sink Capability"),
        port_id: "00000000-0000-4000-8000-00000000d013"
            .parse()
            .expect("sink Port"),
    }
}

fn route(state: RouteState, epoch: u64, expires_at_ms: Option<u64>) -> Route {
    Route {
        id: "00000000-0000-4000-8000-00000000d021"
            .parse::<RouteId>()
            .expect("Route"),
        session_id: "00000000-0000-4000-8000-00000000d022"
            .parse::<SessionId>()
            .expect("Session"),
        source: source(),
        sink: sink(),
        profile: touchpad_frames_profile(),
        backend: RouteBackend::AdapterManaged,
        compatible_formats: vec![touchpad_frame_format()],
        compatible_qos_modes: BTreeSet::from([QosMode::Interactive]),
        selected_format: Some(touchpad_frame_format()),
        selected_qos_mode: Some(QosMode::Interactive),
        state,
        authorization: AuthorizationState::Authorized { expires_at_ms },
        epoch,
        diagnostic_ids: Vec::new(),
    }
}

fn stream(epoch: u64) -> InputStreamDescriptor {
    InputStreamDescriptor {
        stream_id: "00000000-0000-4000-8000-00000000d023"
            .parse()
            .expect("Stream"),
        stream_epoch: epoch,
        clock_domain_id: "android.uptime_nanos".to_owned(),
    }
}

fn descriptor() -> TouchpadDescriptor {
    TouchpadDescriptor {
        physical_size: TouchpadPhysicalSize {
            width_himetric: 10_000,
            height_himetric: 6_000,
        },
        max_contacts: 5,
        button_type: TouchpadButtonType::NonClickable,
        reports_contact_size: false,
        reports_pressure: false,
    }
}

fn contact(contact_id: u32) -> TouchpadContact {
    TouchpadContact {
        contact_id,
        position: TouchpadPosition {
            x_himetric: 2_000,
            y_himetric: 3_000,
        },
        confidence: true,
        size: None,
        pressure: None,
    }
}

fn frame(epoch: u64, sequence: u64, contacts: Vec<TouchpadContact>) -> TouchpadFrame {
    TouchpadFrame {
        header: InputFrameHeader {
            stream_id: stream(epoch).stream_id,
            stream_epoch: epoch,
            sequence,
            source_timestamp_nanos: sequence * 1_000,
        },
        kind: TouchpadFrameKind::Update,
        button: TouchpadButtonState::Released,
        contacts,
    }
}

fn cancel(epoch: u64, sequence: u64) -> TouchpadFrame {
    TouchpadFrame {
        kind: TouchpadFrameKind::CancelAll,
        ..frame(epoch, sequence, Vec::new())
    }
}

fn ingress_limits(queue_packets: u8) -> PrivateTouchpadIngressLimits {
    PrivateTouchpadIngressLimits {
        queue_packets,
        receiver: PrivateTouchpadReceiverLimits {
            max_packets_per_second: 100,
            active_idle_timeout_nanos: MIN_PRIVATE_TOUCHPAD_IDLE_TIMEOUT_NANOS,
        },
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FakeSinkError {
    Close,
}

impl fmt::Display for FakeSinkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("fake Sink close failure")
    }
}

impl Error for FakeSinkError {}

#[derive(Default)]
struct FakeSinkState {
    frames: Vec<TouchpadFrame>,
    epochs: Vec<(u64, u64)>,
    closes: usize,
}

#[derive(Clone, Default)]
struct FakeSink {
    shared: Rc<RefCell<FakeSinkState>>,
    fail_close: bool,
}

impl PrivateTouchpadSink for FakeSink {
    type Error = FakeSinkError;

    fn submit_frame(&mut self, frame: &TouchpadFrame) -> Result<(), Self::Error> {
        self.shared.borrow_mut().frames.push(frame.clone());
        Ok(())
    }

    fn advance_epoch(&mut self, new_epoch: u64, first_sequence: u64) -> Result<(), Self::Error> {
        self.shared
            .borrow_mut()
            .epochs
            .push((new_epoch, first_sequence));
        Ok(())
    }

    fn close(&mut self) -> Result<(), Self::Error> {
        self.shared.borrow_mut().closes += 1;
        if self.fail_close {
            Err(FakeSinkError::Close)
        } else {
            Ok(())
        }
    }
}

fn build_session(
    current_route: &Route,
    queue_packets: u8,
    sink_impl: FakeSink,
) -> Result<PrivateTouchpadRouteSession<FakeSink>, PrivateTouchpadRouteSessionBuildError> {
    PrivateTouchpadRouteSession::new(
        current_route,
        sink(),
        stream(current_route.epoch),
        descriptor(),
        0,
        0,
        ingress_limits(queue_packets),
        sink_impl,
    )
}

fn packet(codec: &PrivateTouchpadPacketCodecV1, frame: &TouchpadFrame) -> Vec<u8> {
    codec.encode(frame).expect("encode").as_bytes().to_vec()
}

fn build_error(
    current_route: &Route,
    expected_sink: PortRef,
    stream_epoch: u64,
    now_ms: u64,
    limits: PrivateTouchpadIngressLimits,
) -> PrivateTouchpadRouteSessionBuildError {
    match PrivateTouchpadRouteSession::new(
        current_route,
        expected_sink,
        stream(stream_epoch),
        descriptor(),
        0,
        now_ms,
        limits,
        FakeSink::default(),
    ) {
        Ok(_) => panic!("invalid binding unexpectedly succeeded"),
        Err(error) => error,
    }
}

#[test]
fn construction_requires_exact_authorized_live_route_binding_and_queue_bounds() {
    let active = route(RouteState::Active, 1, Some(100));
    for queue_packets in [0, MAX_PRIVATE_TOUCHPAD_QUEUE_PACKETS + 1] {
        assert!(matches!(
            build_error(&active, sink(), 1, 0, ingress_limits(queue_packets)),
            PrivateTouchpadRouteSessionBuildError::InvalidQueuePackets { .. }
        ));
    }

    let mut invalid = active.clone();
    invalid.backend = RouteBackend::CapyDataPlane;
    assert!(matches!(
        build_error(&invalid, sink(), 1, 0, ingress_limits(1)),
        PrivateTouchpadRouteSessionBuildError::Binding(
            PrivateTouchpadRouteBindingError::WrongBackend(RouteBackend::CapyDataPlane)
        )
    ));

    let mut invalid = active.clone();
    invalid.profile = capyio_input::pointer_events_profile();
    assert!(matches!(
        build_error(&invalid, sink(), 1, 0, ingress_limits(1)),
        PrivateTouchpadRouteSessionBuildError::Binding(
            PrivateTouchpadRouteBindingError::WrongProfile(_)
        )
    ));

    let mut invalid = active.clone();
    invalid.authorization = AuthorizationState::Pending;
    assert!(matches!(
        build_error(&invalid, sink(), 1, 0, ingress_limits(1)),
        PrivateTouchpadRouteSessionBuildError::Binding(
            PrivateTouchpadRouteBindingError::NotAuthorized(AuthorizationState::Pending)
        )
    ));

    assert!(matches!(
        build_error(&active, sink(), 1, 100, ingress_limits(1)),
        PrivateTouchpadRouteSessionBuildError::Binding(
            PrivateTouchpadRouteBindingError::AuthorizationExpired {
                expires_at_ms: 100,
                now_ms: 100,
            }
        )
    ));
    assert!(matches!(
        build_error(&active, source(), 1, 0, ingress_limits(1)),
        PrivateTouchpadRouteSessionBuildError::Binding(
            PrivateTouchpadRouteBindingError::WrongSink { .. }
        )
    ));
    assert!(matches!(
        build_error(&active, sink(), 2, 0, ingress_limits(1)),
        PrivateTouchpadRouteSessionBuildError::Binding(
            PrivateTouchpadRouteBindingError::EpochMismatch {
                expected: 1,
                actual: 2,
            }
        )
    ));

    let invalid = route(RouteState::Prepared, 1, Some(100));
    assert!(matches!(
        build_error(&invalid, sink(), 1, 0, ingress_limits(1)),
        PrivateTouchpadRouteSessionBuildError::Binding(
            PrivateTouchpadRouteBindingError::WrongState { .. }
        )
    ));
}

#[test]
fn starting_route_requires_explicit_activation_before_ordered_pump() {
    let mut current_route = route(RouteState::Starting, 1, Some(1_000));
    let codec = PrivateTouchpadPacketCodecV1::new(stream(1), descriptor()).expect("codec");
    let sink_impl = FakeSink::default();
    let observed = Rc::clone(&sink_impl.shared);
    let mut session = build_session(&current_route, 4, sink_impl).expect("session");
    assert_eq!(session.state(), PrivateTouchpadRouteSessionState::Starting);
    assert!(matches!(
        session.enqueue(&current_route, 0, &packet(&codec, &cancel(1, 0)), 100),
        Err(PrivateTouchpadRouteSessionError::Inactive(
            PrivateTouchpadRouteSessionState::Starting
        ))
    ));

    current_route.state = RouteState::Active;
    session.activate(&current_route, 0).expect("activate");
    assert_eq!(session.binding().route_id, current_route.id);
    assert_eq!(
        session
            .enqueue(&current_route, 0, &packet(&codec, &cancel(1, 0)), 100)
            .expect("cancel enqueue")
            .queued_packets,
        1
    );
    assert_eq!(
        session
            .enqueue(
                &current_route,
                0,
                &packet(&codec, &frame(1, 1, vec![contact(7)])),
                101,
            )
            .expect("active enqueue")
            .queued_packets,
        2
    );
    let pumped = session.pump(&current_route, 0, 101).expect("pump");
    assert_eq!(pumped.packets_processed, 2);
    assert_eq!(pumped.last_receive.expect("last").sequence, 1);
    assert_eq!(pumped.timeout, PrivateTouchpadPollOutcome::Pending);
    assert_eq!(session.queued_packets(), 0);
    assert_eq!(
        observed
            .borrow()
            .frames
            .iter()
            .map(|frame| frame.header.sequence)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    session.disconnect().expect("disconnect");
    assert_eq!(observed.borrow().closes, 1);
}

#[test]
fn queue_overflow_discards_pending_packets_and_fails_closed() {
    let current_route = route(RouteState::Active, 1, None);
    let codec = PrivateTouchpadPacketCodecV1::new(stream(1), descriptor()).expect("codec");
    let sink_impl = FakeSink::default();
    let observed = Rc::clone(&sink_impl.shared);
    let mut session = build_session(&current_route, 2, sink_impl).expect("session");
    session
        .enqueue(&current_route, 0, &packet(&codec, &cancel(1, 0)), 1)
        .expect("first");
    session
        .enqueue(&current_route, 0, &packet(&codec, &cancel(1, 1)), 2)
        .expect("second");
    let error = session
        .enqueue(&current_route, 0, &packet(&codec, &cancel(1, 2)), 3)
        .expect_err("overflow");
    assert!(matches!(
        error,
        PrivateTouchpadRouteSessionError::Fault {
            fault: PrivateTouchpadRouteSessionFault::QueueFull { capacity: 2 },
            cleanup: None,
        }
    ));
    assert_eq!(session.state(), PrivateTouchpadRouteSessionState::Failed);
    assert_eq!(session.queued_packets(), 0);
    assert!(observed.borrow().frames.is_empty());
    assert_eq!(observed.borrow().closes, 1);
}

#[test]
fn route_offline_authorization_expiry_and_epoch_change_each_fail_closed() {
    let active = route(RouteState::Active, 1, Some(10));

    let sink_impl = FakeSink::default();
    let observed = Rc::clone(&sink_impl.shared);
    let mut session = build_session(&active, 2, sink_impl).expect("session");
    let mut offline = active.clone();
    offline.state = RouteState::Offline;
    offline.epoch = 2;
    assert!(matches!(
        session.pump(&offline, 1, 1),
        Err(PrivateTouchpadRouteSessionError::Fault {
            fault: PrivateTouchpadRouteSessionFault::Binding(
                PrivateTouchpadRouteBindingError::WrongState { .. }
            ),
            cleanup: None,
        })
    ));
    assert_eq!(observed.borrow().closes, 1);

    let sink_impl = FakeSink::default();
    let observed = Rc::clone(&sink_impl.shared);
    let mut session = build_session(&active, 2, sink_impl).expect("session");
    assert!(matches!(
        session.pump(&active, 10, 1),
        Err(PrivateTouchpadRouteSessionError::Fault {
            fault: PrivateTouchpadRouteSessionFault::Binding(
                PrivateTouchpadRouteBindingError::AuthorizationExpired { .. }
            ),
            cleanup: None,
        })
    ));
    assert_eq!(observed.borrow().closes, 1);

    let sink_impl = FakeSink::default();
    let observed = Rc::clone(&sink_impl.shared);
    let mut session = build_session(&active, 2, sink_impl).expect("session");
    let mut changed_epoch = active.clone();
    changed_epoch.epoch = 2;
    assert!(matches!(
        session.pump(&changed_epoch, 1, 1),
        Err(PrivateTouchpadRouteSessionError::Fault {
            fault: PrivateTouchpadRouteSessionFault::Binding(
                PrivateTouchpadRouteBindingError::EpochMismatch {
                    expected: 1,
                    actual: 2,
                }
            ),
            cleanup: None,
        })
    ));
    assert_eq!(observed.borrow().closes, 1);
}

#[test]
fn oversized_packet_and_local_clock_regression_each_fail_closed() {
    let current_route = route(RouteState::Active, 1, None);
    let sink_impl = FakeSink::default();
    let observed = Rc::clone(&sink_impl.shared);
    let mut session = build_session(&current_route, 2, sink_impl).expect("session");
    let oversized = vec![0; PRIVATE_TOUCHPAD_PACKET_MAX_BYTES + 1];
    assert!(matches!(
        session.enqueue(&current_route, 0, &oversized, 1),
        Err(PrivateTouchpadRouteSessionError::Fault {
            fault: PrivateTouchpadRouteSessionFault::PacketTooLong {
                actual,
                maximum: PRIVATE_TOUCHPAD_PACKET_MAX_BYTES,
            },
            cleanup: None,
        }) if actual == PRIVATE_TOUCHPAD_PACKET_MAX_BYTES + 1
    ));
    assert_eq!(observed.borrow().closes, 1);

    let codec = PrivateTouchpadPacketCodecV1::new(stream(1), descriptor()).expect("codec");
    let sink_impl = FakeSink::default();
    let observed = Rc::clone(&sink_impl.shared);
    let mut session = build_session(&current_route, 2, sink_impl).expect("session");
    session
        .enqueue(&current_route, 0, &packet(&codec, &cancel(1, 0)), 100)
        .expect("enqueue");
    assert!(matches!(
        session.pump(&current_route, 0, 99),
        Err(PrivateTouchpadRouteSessionError::Fault {
            fault: PrivateTouchpadRouteSessionFault::LocalClockRegression {
                previous: 100,
                actual: 99,
            },
            cleanup: None,
        })
    ));
    assert_eq!(session.queued_packets(), 0);
    assert_eq!(observed.borrow().closes, 1);
}

#[test]
fn empty_pump_enforces_active_contact_timeout() {
    let current_route = route(RouteState::Active, 1, None);
    let codec = PrivateTouchpadPacketCodecV1::new(stream(1), descriptor()).expect("codec");
    let sink_impl = FakeSink::default();
    let observed = Rc::clone(&sink_impl.shared);
    let mut session = build_session(&current_route, 3, sink_impl).expect("session");
    session
        .enqueue(
            &current_route,
            0,
            &packet(&codec, &frame(1, 0, vec![contact(9)])),
            100,
        )
        .expect("active enqueue");
    let first = session.pump(&current_route, 0, 100).expect("first pump");
    assert_eq!(first.packets_processed, 1);
    assert_eq!(first.timeout, PrivateTouchpadPollOutcome::Pending);

    let timeout = session
        .pump(
            &current_route,
            0,
            100 + MIN_PRIVATE_TOUCHPAD_IDLE_TIMEOUT_NANOS,
        )
        .expect("timeout pump");
    assert_eq!(timeout.packets_processed, 0);
    assert_eq!(timeout.timeout, PrivateTouchpadPollOutcome::TimedOut);
    assert_eq!(session.state(), PrivateTouchpadRouteSessionState::TimedOut);
    assert_eq!(observed.borrow().closes, 1);

    let sink_impl = FakeSink::default();
    let observed = Rc::clone(&sink_impl.shared);
    let mut stale_queue = build_session(&current_route, 1, sink_impl).expect("session");
    stale_queue
        .enqueue(
            &current_route,
            0,
            &packet(&codec, &frame(1, 0, vec![contact(10)])),
            100,
        )
        .expect("enqueue");
    assert!(matches!(
        stale_queue.pump(
            &current_route,
            0,
            100 + MIN_PRIVATE_TOUCHPAD_IDLE_TIMEOUT_NANOS,
        ),
        Err(PrivateTouchpadRouteSessionError::Fault {
            fault: PrivateTouchpadRouteSessionFault::QueuedPacketExpired {
                arrival_nanos: 100,
                now_nanos,
                maximum_age_nanos: MIN_PRIVATE_TOUCHPAD_IDLE_TIMEOUT_NANOS,
            },
            cleanup: None,
        }) if now_nanos == 100 + MIN_PRIVATE_TOUCHPAD_IDLE_TIMEOUT_NANOS
    ));
    assert!(observed.borrow().frames.is_empty());
    assert_eq!(observed.borrow().closes, 1);
}

#[test]
fn later_route_epoch_discards_old_queue_and_advances_sink_before_reactivation() {
    let mut current_route = route(RouteState::Active, 1, None);
    let mut sender = PrivateTouchpadPacketCodecV1::new(stream(1), descriptor()).expect("codec");
    let sink_impl = FakeSink::default();
    let observed = Rc::clone(&sink_impl.shared);
    let mut session = build_session(&current_route, 3, sink_impl).expect("session");
    session
        .enqueue(&current_route, 0, &packet(&sender, &cancel(1, 0)), 100)
        .expect("old enqueue");

    current_route.state = RouteState::Starting;
    current_route.epoch = 2;
    let advanced = session
        .advance_epoch(&current_route, 50, 0, 101)
        .expect("advance");
    assert_eq!((advanced.previous_epoch, advanced.new_epoch), (1, 2));
    assert_eq!(advanced.discarded_packets, 1);
    assert_eq!(observed.borrow().epochs, vec![(2, 50)]);
    assert!(observed.borrow().frames.is_empty());
    assert_eq!(session.state(), PrivateTouchpadRouteSessionState::Starting);

    current_route.state = RouteState::Active;
    session.activate(&current_route, 0).expect("reactivate");
    sender.advance_epoch(2).expect("sender epoch");
    session
        .enqueue(&current_route, 0, &packet(&sender, &cancel(2, 50)), 102)
        .expect("fresh enqueue");
    let pumped = session.pump(&current_route, 0, 102).expect("fresh pump");
    assert_eq!(pumped.packets_processed, 1);
    assert_eq!(observed.borrow().frames[0].header.stream_epoch, 2);
    assert_eq!(observed.borrow().frames[0].header.sequence, 50);

    let sink_impl = FakeSink::default();
    let observed = Rc::clone(&sink_impl.shared);
    let starting = route(RouteState::Starting, 1, None);
    let mut non_advancing = build_session(&starting, 1, sink_impl).expect("session");
    assert!(matches!(
        non_advancing.advance_epoch(&starting, 0, 0, 1),
        Err(PrivateTouchpadRouteSessionError::Fault {
            fault: PrivateTouchpadRouteSessionFault::Binding(
                PrivateTouchpadRouteBindingError::NonAdvancingEpoch { current: 1, new: 1 }
            ),
            cleanup: None,
        })
    ));
    assert_eq!(observed.borrow().closes, 1);
}

#[derive(Default)]
struct ProjectorState {
    dispositions: Vec<WindowsTouchpadProjectionDisposition>,
    closes: usize,
}

struct ProjectingSink {
    projector: WindowsTouchpadProjector,
    shared: Rc<RefCell<ProjectorState>>,
}

impl PrivateTouchpadSink for ProjectingSink {
    type Error = capyio_windows_input::WindowsTouchpadProjectionError;

    fn submit_frame(&mut self, frame: &TouchpadFrame) -> Result<(), Self::Error> {
        let projection = self.projector.project(frame)?;
        self.shared
            .borrow_mut()
            .dispositions
            .push(projection.disposition);
        Ok(())
    }

    fn advance_epoch(&mut self, new_epoch: u64, first_sequence: u64) -> Result<(), Self::Error> {
        let projection = self.projector.advance_epoch(new_epoch, first_sequence)?;
        self.shared
            .borrow_mut()
            .dispositions
            .push(projection.disposition);
        Ok(())
    }

    fn close(&mut self) -> Result<(), Self::Error> {
        let projection = self.projector.cancel_active();
        let mut shared = self.shared.borrow_mut();
        shared.dispositions.push(projection.disposition);
        shared.closes += 1;
        Ok(())
    }
}

#[test]
fn authorized_queue_pumps_into_windows_projector_without_native_device() {
    let current_route = route(RouteState::Active, 1, None);
    let codec = PrivateTouchpadPacketCodecV1::new(stream(1), descriptor()).expect("codec");
    let shared = Rc::new(RefCell::new(ProjectorState::default()));
    let sink_impl = ProjectingSink {
        projector: WindowsTouchpadProjector::new(&stream(1), descriptor(), 0).expect("projector"),
        shared: Rc::clone(&shared),
    };
    let mut session = PrivateTouchpadRouteSession::new(
        &current_route,
        sink(),
        stream(1),
        descriptor(),
        0,
        0,
        ingress_limits(3),
        sink_impl,
    )
    .expect("session");
    for (arrival, frame) in [
        (100, cancel(1, 0)),
        (101, frame(1, 1, vec![contact(42)])),
        (102, frame(1, 2, Vec::new())),
    ] {
        session
            .enqueue(&current_route, 0, &packet(&codec, &frame), arrival)
            .expect("enqueue");
    }
    let pumped = session.pump(&current_route, 0, 102).expect("pump");
    assert_eq!(pumped.packets_processed, 3);
    assert_eq!(pumped.timeout, PrivateTouchpadPollOutcome::Idle);
    assert_eq!(
        shared.borrow().dispositions,
        vec![
            WindowsTouchpadProjectionDisposition::Cancelled,
            WindowsTouchpadProjectionDisposition::Applied,
            WindowsTouchpadProjectionDisposition::Applied,
        ]
    );
    session.disconnect().expect("disconnect");
    assert_eq!(shared.borrow().closes, 1);
}

#[test]
fn cleanup_error_is_retained_with_route_fault() {
    let current_route = route(RouteState::Active, 1, None);
    let sink_impl = FakeSink {
        fail_close: true,
        ..FakeSink::default()
    };
    let observed = Rc::clone(&sink_impl.shared);
    let mut session = build_session(&current_route, 1, sink_impl).expect("session");
    let oversized = vec![0; PRIVATE_TOUCHPAD_PACKET_MAX_BYTES + 1];
    let error = session
        .enqueue(&current_route, 0, &oversized, 1)
        .expect_err("oversized");
    let PrivateTouchpadRouteSessionError::Fault {
        fault: PrivateTouchpadRouteSessionFault::PacketTooLong { .. },
        cleanup: Some(cleanup),
    } = error
    else {
        panic!("unexpected Route fault");
    };
    assert!(matches!(
        *cleanup,
        PrivateTouchpadReceiverError::Close(FakeSinkError::Close)
    ));
    assert_eq!(session.state(), PrivateTouchpadRouteSessionState::Failed);
    assert_eq!(observed.borrow().closes, 1);
}

#[test]
fn route_fixture_retains_selected_touchpad_contract() {
    let current_route = route(RouteState::Active, 1, None);
    assert_eq!(current_route.profile, touchpad_frames_profile());
    assert_eq!(
        current_route.selected_format,
        Some(FormatDescriptor::new("touchpad-frame-v1"))
    );
    assert_eq!(current_route.backend, RouteBackend::AdapterManaged);
}
