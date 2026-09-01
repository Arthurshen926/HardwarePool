use std::{cell::RefCell, error::Error, fmt, rc::Rc};

use capyio_input::{
    InputContractError, InputFrameHeader, InputSequenceOutcome, InputStreamDescriptor, SequenceGap,
    TouchpadButtonState, TouchpadButtonType, TouchpadContact, TouchpadDescriptor, TouchpadFrame,
    TouchpadFrameKind, TouchpadPhysicalSize, TouchpadPosition,
};
use capyio_remote_touchpad_adapter::{
    MAX_PRIVATE_TOUCHPAD_IDLE_TIMEOUT_NANOS, MAX_PRIVATE_TOUCHPAD_PACKETS_PER_SECOND,
    MIN_PRIVATE_TOUCHPAD_IDLE_TIMEOUT_NANOS, PrivateTouchpadPacketCodecV1,
    PrivateTouchpadPacketError, PrivateTouchpadPollOutcome, PrivateTouchpadReceiver,
    PrivateTouchpadReceiverBuildError, PrivateTouchpadReceiverError, PrivateTouchpadReceiverFault,
    PrivateTouchpadReceiverLimits, PrivateTouchpadReceiverState, PrivateTouchpadSink,
};
use capyio_windows_input::{
    WindowsTouchpadContactPhase, WindowsTouchpadProjectionDisposition, WindowsTouchpadProjector,
};

fn stream(epoch: u64) -> InputStreamDescriptor {
    InputStreamDescriptor {
        stream_id: "00000000-0000-4000-8000-00000000c703"
            .parse()
            .expect("stream ID"),
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

fn limits(max_packets_per_second: u16) -> PrivateTouchpadReceiverLimits {
    PrivateTouchpadReceiverLimits {
        max_packets_per_second,
        active_idle_timeout_nanos: MIN_PRIVATE_TOUCHPAD_IDLE_TIMEOUT_NANOS,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FakeSinkError {
    Submit,
    Advance,
    Close,
}

impl fmt::Display for FakeSinkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "fake Sink {self:?} failure")
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
    fail_submit: bool,
    fail_advance: bool,
    fail_close: bool,
}

impl PrivateTouchpadSink for FakeSink {
    type Error = FakeSinkError;

    fn submit_frame(&mut self, frame: &TouchpadFrame) -> Result<(), Self::Error> {
        if self.fail_submit {
            return Err(FakeSinkError::Submit);
        }
        self.shared.borrow_mut().frames.push(frame.clone());
        Ok(())
    }

    fn advance_epoch(&mut self, new_epoch: u64, first_sequence: u64) -> Result<(), Self::Error> {
        if self.fail_advance {
            return Err(FakeSinkError::Advance);
        }
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

fn build_receiver(
    sink: FakeSink,
    receiver_limits: PrivateTouchpadReceiverLimits,
) -> PrivateTouchpadReceiver<FakeSink> {
    PrivateTouchpadReceiver::new(stream(1), descriptor(), 0, receiver_limits, sink)
        .expect("receiver")
}

fn packet(codec: &PrivateTouchpadPacketCodecV1, frame: &TouchpadFrame) -> Vec<u8> {
    codec.encode(frame).expect("packet").as_bytes().to_vec()
}

#[test]
fn invalid_limits_fail_before_the_sink_is_owned_by_a_receiver() {
    let invalid_rate =
        PrivateTouchpadReceiver::new(stream(1), descriptor(), 0, limits(0), FakeSink::default());
    assert!(matches!(
        invalid_rate,
        Err(PrivateTouchpadReceiverBuildError::InvalidPacketsPerSecond {
            actual: 0,
            maximum: MAX_PRIVATE_TOUCHPAD_PACKETS_PER_SECOND,
        })
    ));

    for actual in [
        MIN_PRIVATE_TOUCHPAD_IDLE_TIMEOUT_NANOS - 1,
        MAX_PRIVATE_TOUCHPAD_IDLE_TIMEOUT_NANOS + 1,
    ] {
        let invalid_timeout = PrivateTouchpadReceiver::new(
            stream(1),
            descriptor(),
            0,
            PrivateTouchpadReceiverLimits {
                max_packets_per_second: 1,
                active_idle_timeout_nanos: actual,
            },
            FakeSink::default(),
        );
        assert!(matches!(
            invalid_timeout,
            Err(PrivateTouchpadReceiverBuildError::InvalidIdleTimeout { .. })
        ));
    }
}

#[test]
fn duplicate_packet_is_rejected_and_closes_without_second_submission() {
    let codec = PrivateTouchpadPacketCodecV1::new(stream(1), descriptor()).expect("codec");
    let sink = FakeSink::default();
    let observed = Rc::clone(&sink.shared);
    let mut receiver = build_receiver(sink, limits(10));
    let cancel = packet(&codec, &cancel(1, 0));

    let accepted = receiver.receive(&cancel, 100).expect("first packet");
    assert_eq!(accepted.sequence_outcome, InputSequenceOutcome::InOrder);
    let error = receiver.receive(&cancel, 101).expect_err("duplicate");
    assert!(matches!(
        error,
        PrivateTouchpadReceiverError::Fault {
            fault: PrivateTouchpadReceiverFault::Sequence(InputContractError::DuplicateOrLate {
                expected: 1,
                actual: 0,
            }),
            cleanup: None,
        }
    ));
    assert_eq!(receiver.state(), PrivateTouchpadReceiverState::Failed);
    assert_eq!(observed.borrow().frames.len(), 1);
    assert_eq!(observed.borrow().closes, 1);
}

#[derive(Default)]
struct ProjectorObservations {
    dispositions: Vec<WindowsTouchpadProjectionDisposition>,
    phases: Vec<Vec<WindowsTouchpadContactPhase>>,
    closes: usize,
}

struct ProjectingSink {
    projector: WindowsTouchpadProjector,
    shared: Rc<RefCell<ProjectorObservations>>,
}

impl ProjectingSink {
    fn record(&mut self, projection: capyio_windows_input::WindowsTouchpadProjection) {
        let phases = projection
            .batches()
            .iter()
            .flat_map(|batch| batch.contacts())
            .map(|contact| contact.phase)
            .collect();
        let mut shared = self.shared.borrow_mut();
        shared.dispositions.push(projection.disposition);
        shared.phases.push(phases);
    }
}

impl PrivateTouchpadSink for ProjectingSink {
    type Error = capyio_windows_input::WindowsTouchpadProjectionError;

    fn submit_frame(&mut self, frame: &TouchpadFrame) -> Result<(), Self::Error> {
        let projection = self.projector.project(frame)?;
        self.record(projection);
        Ok(())
    }

    fn advance_epoch(&mut self, new_epoch: u64, first_sequence: u64) -> Result<(), Self::Error> {
        let projection = self.projector.advance_epoch(new_epoch, first_sequence)?;
        self.record(projection);
        Ok(())
    }

    fn close(&mut self) -> Result<(), Self::Error> {
        let projection = self.projector.cancel_active();
        self.record(projection);
        self.shared.borrow_mut().closes += 1;
        Ok(())
    }
}

#[test]
fn sequence_gap_is_observable_and_cancels_windows_projector_contacts() {
    let codec = PrivateTouchpadPacketCodecV1::new(stream(1), descriptor()).expect("codec");
    let shared = Rc::new(RefCell::new(ProjectorObservations::default()));
    let sink = ProjectingSink {
        projector: WindowsTouchpadProjector::new(&stream(1), descriptor(), 0).expect("projector"),
        shared: Rc::clone(&shared),
    };
    let mut receiver = PrivateTouchpadReceiver::new(stream(1), descriptor(), 0, limits(10), sink)
        .expect("receiver");

    receiver
        .receive(&packet(&codec, &cancel(1, 0)), 100)
        .expect("cancel");
    receiver
        .receive(&packet(&codec, &frame(1, 1, vec![contact(7)])), 101)
        .expect("active");
    let gap = receiver
        .receive(&packet(&codec, &frame(1, 3, vec![contact(7)])), 102)
        .expect("gap");

    assert_eq!(
        gap.sequence_outcome,
        InputSequenceOutcome::Gap(SequenceGap {
            first_missing: 2,
            last_missing: 2,
        })
    );
    assert_eq!(gap.active_contacts, 0);
    assert_eq!(
        shared.borrow().dispositions,
        vec![
            WindowsTouchpadProjectionDisposition::Cancelled,
            WindowsTouchpadProjectionDisposition::Applied,
            WindowsTouchpadProjectionDisposition::GapRequiresCancelAll(SequenceGap {
                first_missing: 2,
                last_missing: 2,
            }),
        ]
    );
    assert_eq!(
        shared.borrow().phases[2],
        vec![WindowsTouchpadContactPhase::Cancelled]
    );
}

#[test]
fn fixed_window_rate_limit_fails_closed_and_resets_after_epoch_advance() {
    let mut sender = PrivateTouchpadPacketCodecV1::new(stream(1), descriptor()).expect("codec");
    let sink = FakeSink::default();
    let observed = Rc::clone(&sink.shared);
    let mut receiver = build_receiver(sink, limits(1));

    receiver
        .receive(&packet(&sender, &cancel(1, 0)), 5)
        .expect("first packet");
    receiver.advance_epoch(2, 50).expect("epoch advance");
    sender.advance_epoch(2).expect("sender epoch");
    let accepted = receiver
        .receive(&packet(&sender, &cancel(2, 50)), 5)
        .expect("reset receive window");
    assert_eq!(accepted.sequence, 50);
    assert_eq!(observed.borrow().epochs, vec![(2, 50)]);

    let error = receiver
        .receive(&packet(&sender, &cancel(2, 51)), 6)
        .expect_err("rate limit");
    assert!(matches!(
        error,
        PrivateTouchpadReceiverError::Fault {
            fault: PrivateTouchpadReceiverFault::RateLimitExceeded {
                limit: 1,
                window_started_nanos: 5,
            },
            cleanup: None,
        }
    ));
    assert_eq!(receiver.state(), PrivateTouchpadReceiverState::Failed);
    assert_eq!(observed.borrow().closes, 1);

    let sink = FakeSink::default();
    let observed = Rc::clone(&sink.shared);
    let mut non_advancing = build_receiver(sink, limits(10));
    let error = non_advancing
        .advance_epoch(1, 0)
        .expect_err("epoch must increase");
    assert!(matches!(
        error,
        PrivateTouchpadReceiverError::Fault {
            fault: PrivateTouchpadReceiverFault::Packet(PrivateTouchpadPacketError::Contract(
                InputContractError::NonAdvancingEpoch {
                    current_epoch: 1,
                    new_epoch: 1,
                }
            )),
            cleanup: None,
        }
    ));
    assert_eq!(non_advancing.state(), PrivateTouchpadReceiverState::Failed);
    assert!(observed.borrow().epochs.is_empty());
    assert_eq!(observed.borrow().closes, 1);
}

#[test]
fn malformed_packet_and_receive_clock_regression_each_fail_closed() {
    let sink = FakeSink::default();
    let observed = Rc::clone(&sink.shared);
    let mut malformed_receiver = build_receiver(sink, limits(10));
    let malformed = malformed_receiver
        .receive(&[0; 31], 0)
        .expect_err("malformed packet");
    assert!(matches!(
        malformed,
        PrivateTouchpadReceiverError::Fault {
            fault: PrivateTouchpadReceiverFault::Packet(
                PrivateTouchpadPacketError::PacketTooShort { .. }
            ),
            cleanup: None,
        }
    ));
    assert_eq!(observed.borrow().closes, 1);

    let codec = PrivateTouchpadPacketCodecV1::new(stream(1), descriptor()).expect("codec");
    let sink = FakeSink::default();
    let observed = Rc::clone(&sink.shared);
    let mut regressed_receiver = build_receiver(sink, limits(10));
    regressed_receiver
        .receive(&packet(&codec, &cancel(1, 0)), 100)
        .expect("first packet");
    let regressed = regressed_receiver
        .receive(&packet(&codec, &cancel(1, 1)), 99)
        .expect_err("clock regression");
    assert!(matches!(
        regressed,
        PrivateTouchpadReceiverError::Fault {
            fault: PrivateTouchpadReceiverFault::ArrivalClockRegression {
                previous: 100,
                actual: 99,
            },
            cleanup: None,
        }
    ));
    assert_eq!(observed.borrow().frames.len(), 1);
    assert_eq!(observed.borrow().closes, 1);
}

#[test]
fn active_contacts_time_out_but_released_stream_remains_idle() {
    let codec = PrivateTouchpadPacketCodecV1::new(stream(1), descriptor()).expect("codec");
    let sink = FakeSink::default();
    let observed = Rc::clone(&sink.shared);
    let mut active_receiver = build_receiver(sink, limits(10));
    active_receiver
        .receive(&packet(&codec, &frame(1, 0, vec![contact(9)])), 1_000)
        .expect("active frame");
    assert_eq!(
        active_receiver.poll_timeout(10_000_999).expect("poll"),
        PrivateTouchpadPollOutcome::Pending
    );
    assert_eq!(
        active_receiver.poll_timeout(10_001_000).expect("timeout"),
        PrivateTouchpadPollOutcome::TimedOut
    );
    assert_eq!(
        active_receiver.state(),
        PrivateTouchpadReceiverState::TimedOut
    );
    assert_eq!(observed.borrow().closes, 1);

    let sink = FakeSink::default();
    let observed = Rc::clone(&sink.shared);
    let mut idle_receiver = build_receiver(sink, limits(10));
    idle_receiver
        .receive(&packet(&codec, &cancel(1, 0)), 1_000)
        .expect("released frame");
    assert_eq!(
        idle_receiver.poll_timeout(u64::MAX).expect("idle poll"),
        PrivateTouchpadPollOutcome::Idle
    );
    assert_eq!(idle_receiver.state(), PrivateTouchpadReceiverState::Active);
    assert_eq!(observed.borrow().closes, 0);
}

#[test]
fn sink_submission_failure_retains_cleanup_error() {
    let codec = PrivateTouchpadPacketCodecV1::new(stream(1), descriptor()).expect("codec");
    let sink = FakeSink {
        fail_submit: true,
        fail_close: true,
        ..FakeSink::default()
    };
    let observed = Rc::clone(&sink.shared);
    let mut receiver = build_receiver(sink, limits(10));
    let error = receiver
        .receive(&packet(&codec, &cancel(1, 0)), 0)
        .expect_err("Sink failure");
    assert!(matches!(
        error,
        PrivateTouchpadReceiverError::Sink {
            primary: FakeSinkError::Submit,
            cleanup: Some(FakeSinkError::Close),
        }
    ));
    assert_eq!(receiver.state(), PrivateTouchpadReceiverState::Failed);
    assert_eq!(observed.borrow().closes, 1);

    let sink = FakeSink {
        fail_advance: true,
        ..FakeSink::default()
    };
    let observed = Rc::clone(&sink.shared);
    let mut receiver = build_receiver(sink, limits(10));
    let error = receiver
        .advance_epoch(2, 50)
        .expect_err("Sink epoch failure");
    assert!(matches!(
        error,
        PrivateTouchpadReceiverError::Sink {
            primary: FakeSinkError::Advance,
            cleanup: None,
        }
    ));
    assert_eq!(receiver.state(), PrivateTouchpadReceiverState::Failed);
    assert!(observed.borrow().epochs.is_empty());
    assert_eq!(observed.borrow().closes, 1);
}

#[test]
fn explicit_disconnect_and_active_drop_each_close_once() {
    let explicit_sink = FakeSink::default();
    let explicit_observed = Rc::clone(&explicit_sink.shared);
    let mut explicit = build_receiver(explicit_sink, limits(10));
    explicit.disconnect().expect("disconnect");
    assert_eq!(explicit.state(), PrivateTouchpadReceiverState::Closed);
    drop(explicit);
    assert_eq!(explicit_observed.borrow().closes, 1);

    let dropped_sink = FakeSink::default();
    let dropped_observed = Rc::clone(&dropped_sink.shared);
    drop(build_receiver(dropped_sink, limits(10)));
    assert_eq!(dropped_observed.borrow().closes, 1);
}
