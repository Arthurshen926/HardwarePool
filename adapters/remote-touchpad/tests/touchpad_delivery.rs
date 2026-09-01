use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use capyio_core::{CapabilityId, NodeId, PortId, PortRef, RouteId, SessionId};
use capyio_input::{
    InputFrameHeader, InputStreamDescriptor, TouchpadButtonState, TouchpadButtonType,
    TouchpadContact, TouchpadDescriptor, TouchpadFrame, TouchpadFrameKind, TouchpadPhysicalSize,
    TouchpadPosition,
};
use capyio_remote_touchpad_adapter::{
    PrivateTouchpadAdmittedChannel, PrivateTouchpadBindingMismatch,
    PrivateTouchpadChannelAdmissionError, PrivateTouchpadChannelSendOutcome,
    PrivateTouchpadDeliveryBuildError, PrivateTouchpadDeliveryError,
    PrivateTouchpadDeliverySession, PrivateTouchpadDeliveryState, PrivateTouchpadPacketSource,
    PrivateTouchpadPacketV1, PrivateTouchpadRouteBinding,
};

fn stream() -> InputStreamDescriptor {
    InputStreamDescriptor {
        stream_id: "00000000-0000-4000-8000-00000000c404"
            .parse()
            .expect("stream ID"),
        stream_epoch: 13,
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

fn binding() -> PrivateTouchpadRouteBinding {
    PrivateTouchpadRouteBinding {
        route_id: "00000000-0000-4000-8000-00000000d401"
            .parse::<RouteId>()
            .expect("route ID"),
        session_id: "00000000-0000-4000-8000-00000000d402"
            .parse::<SessionId>()
            .expect("session ID"),
        source: PortRef {
            node_id: "00000000-0000-4000-8000-00000000d403"
                .parse::<NodeId>()
                .expect("source node ID"),
            capability_id: "00000000-0000-4000-8000-00000000d404"
                .parse::<CapabilityId>()
                .expect("source capability ID"),
            port_id: "00000000-0000-4000-8000-00000000d405"
                .parse::<PortId>()
                .expect("source port ID"),
        },
        sink: PortRef {
            node_id: "00000000-0000-4000-8000-00000000d406"
                .parse::<NodeId>()
                .expect("sink node ID"),
            capability_id: "00000000-0000-4000-8000-00000000d407"
                .parse::<CapabilityId>()
                .expect("sink capability ID"),
            port_id: "00000000-0000-4000-8000-00000000d408"
                .parse::<PortId>()
                .expect("sink port ID"),
        },
        route_epoch: stream().stream_epoch,
        authorization_expires_at_ms: Some(10_000),
    }
}

fn frame(sequence: u64, kind: TouchpadFrameKind, active: bool) -> TouchpadFrame {
    TouchpadFrame {
        header: InputFrameHeader {
            stream_id: stream().stream_id,
            stream_epoch: stream().stream_epoch,
            sequence,
            source_timestamp_nanos: 1_000 + sequence,
        },
        kind,
        button: TouchpadButtonState::Released,
        contacts: if active {
            vec![TouchpadContact {
                contact_id: 1,
                position: TouchpadPosition {
                    x_himetric: 2_000,
                    y_himetric: 2_000,
                },
                confidence: true,
                size: None,
                pressure: None,
            }]
        } else {
            Vec::new()
        },
    }
}

#[derive(Debug)]
struct ChannelControl {
    admission: Result<PrivateTouchpadRouteBinding, PrivateTouchpadChannelAdmissionError>,
    outcomes: VecDeque<PrivateTouchpadChannelSendOutcome>,
    sent: Vec<Vec<u8>>,
    close_count: u64,
}

#[derive(Clone, Debug)]
struct FakeChannel {
    control: Rc<RefCell<ChannelControl>>,
}

impl FakeChannel {
    fn new(binding: PrivateTouchpadRouteBinding) -> (Self, Rc<RefCell<ChannelControl>>) {
        let control = Rc::new(RefCell::new(ChannelControl {
            admission: Ok(binding),
            outcomes: VecDeque::new(),
            sent: Vec::new(),
            close_count: 0,
        }));
        (
            Self {
                control: Rc::clone(&control),
            },
            control,
        )
    }
}

impl PrivateTouchpadAdmittedChannel for FakeChannel {
    fn current_binding(
        &self,
    ) -> Result<PrivateTouchpadRouteBinding, PrivateTouchpadChannelAdmissionError> {
        self.control.borrow().admission.clone()
    }

    fn send(&mut self, packet: &PrivateTouchpadPacketV1) -> PrivateTouchpadChannelSendOutcome {
        let mut control = self.control.borrow_mut();
        let outcome = control
            .outcomes
            .pop_front()
            .unwrap_or(PrivateTouchpadChannelSendOutcome::Delivered);
        if outcome != PrivateTouchpadChannelSendOutcome::RejectedBeforeWrite {
            control.sent.push(packet.as_bytes().to_vec());
        }
        outcome
    }

    fn close(&mut self) {
        self.control.borrow_mut().close_count += 1;
    }
}

fn delivery(
    channel: FakeChannel,
) -> Result<PrivateTouchpadDeliverySession<FakeChannel>, PrivateTouchpadDeliveryBuildError> {
    PrivateTouchpadDeliverySession::new(
        binding(),
        PrivateTouchpadPacketSource::new(stream(), descriptor(), 0).expect("source"),
        channel,
    )
}

#[test]
fn delivered_frames_commit_source_and_close_after_release() {
    let (channel, control) = FakeChannel::new(binding());
    let mut delivery = delivery(channel).expect("delivery");
    delivery
        .deliver(&frame(0, TouchpadFrameKind::CancelAll, false))
        .expect("cancel");
    delivery
        .deliver(&frame(1, TouchpadFrameKind::Update, true))
        .expect("down");
    assert!(matches!(
        delivery.close(),
        Err(PrivateTouchpadDeliveryError::Source(_))
    ));
    delivery
        .deliver(&frame(2, TouchpadFrameKind::Update, false))
        .expect("release");
    delivery.close().expect("close");
    delivery.close().expect("idempotent close");

    assert_eq!(delivery.state(), PrivateTouchpadDeliveryState::Closed);
    assert_eq!(delivery.metrics().attempts, 3);
    assert_eq!(delivery.metrics().packets_delivered, 3);
    assert_eq!(control.borrow().sent.len(), 3);
    assert_eq!(control.borrow().close_count, 1);
}

#[test]
fn rejected_before_write_keeps_same_frame_retryable() {
    let (channel, control) = FakeChannel::new(binding());
    control.borrow_mut().outcomes.extend([
        PrivateTouchpadChannelSendOutcome::RejectedBeforeWrite,
        PrivateTouchpadChannelSendOutcome::Delivered,
    ]);
    let mut delivery = delivery(channel).expect("delivery");
    let cancel = frame(0, TouchpadFrameKind::CancelAll, false);

    assert_eq!(
        delivery.deliver(&cancel).expect_err("rejected"),
        PrivateTouchpadDeliveryError::RejectedBeforeWrite
    );
    delivery.deliver(&cancel).expect("same-frame retry");
    assert_eq!(delivery.metrics().attempts, 2);
    assert_eq!(delivery.metrics().packets_delivered, 1);
    assert_eq!(delivery.metrics().rejected_before_write, 1);
    assert_eq!(control.borrow().sent.len(), 1);
}

#[test]
fn unknown_delivery_faults_and_closes_without_retry() {
    let (channel, control) = FakeChannel::new(binding());
    control
        .borrow_mut()
        .outcomes
        .push_back(PrivateTouchpadChannelSendOutcome::DeliveryUnknown);
    let mut delivery = delivery(channel).expect("delivery");
    let cancel = frame(0, TouchpadFrameKind::CancelAll, false);

    assert_eq!(
        delivery.deliver(&cancel).expect_err("unknown"),
        PrivateTouchpadDeliveryError::DeliveryUnknown
    );
    assert_eq!(delivery.state(), PrivateTouchpadDeliveryState::Faulted);
    assert_eq!(delivery.metrics().delivery_unknown, 1);
    assert_eq!(control.borrow().close_count, 1);
    assert_eq!(
        delivery.deliver(&cancel).expect_err("terminal fault"),
        PrivateTouchpadDeliveryError::Faulted
    );
    assert_eq!(delivery.metrics().attempts, 1);
    delivery.close().expect("fault close remains idempotent");
    assert_eq!(control.borrow().close_count, 1);
}

#[test]
fn admission_loss_or_binding_drift_faults_before_send() {
    let (channel, control) = FakeChannel::new(binding());
    let mut revoked_delivery = delivery(channel).expect("delivery");
    control.borrow_mut().admission = Err(PrivateTouchpadChannelAdmissionError::Revoked);
    assert_eq!(
        revoked_delivery
            .deliver(&frame(0, TouchpadFrameKind::CancelAll, false))
            .expect_err("revoked"),
        PrivateTouchpadDeliveryError::Admission(PrivateTouchpadChannelAdmissionError::Revoked)
    );
    assert_eq!(
        revoked_delivery.state(),
        PrivateTouchpadDeliveryState::Faulted
    );
    assert_eq!(revoked_delivery.metrics().attempts, 0);
    assert!(control.borrow().sent.is_empty());

    let (channel, control) = FakeChannel::new(binding());
    let mut delivery = delivery(channel).expect("delivery");
    let mut changed = binding();
    changed.route_epoch += 1;
    control.borrow_mut().admission = Ok(changed.clone());
    assert_eq!(
        delivery
            .deliver(&frame(0, TouchpadFrameKind::CancelAll, false))
            .expect_err("binding drift"),
        PrivateTouchpadDeliveryError::BindingChanged(PrivateTouchpadBindingMismatch::Epoch {
            expected: binding().route_epoch,
            actual: changed.route_epoch,
        })
    );
    assert_eq!(delivery.state(), PrivateTouchpadDeliveryState::Faulted);
    assert!(control.borrow().sent.is_empty());
}

#[test]
fn construction_rejects_wrong_binding_epoch_or_unavailable_admission() {
    let mut wrong = binding();
    wrong.session_id = "00000000-0000-4000-8000-00000000ffff"
        .parse()
        .expect("other session");
    let (channel, wrong_control) = FakeChannel::new(wrong.clone());
    assert!(matches!(
        delivery(channel),
        Err(PrivateTouchpadDeliveryBuildError::BindingMismatch(
            PrivateTouchpadBindingMismatch::Session { actual, .. }
        )) if actual == wrong.session_id
    ));
    assert_eq!(wrong_control.borrow().close_count, 1);

    let (channel, control) = FakeChannel::new(binding());
    control.borrow_mut().admission = Err(PrivateTouchpadChannelAdmissionError::Unavailable);
    assert!(matches!(
        delivery(channel),
        Err(PrivateTouchpadDeliveryBuildError::Admission(
            PrivateTouchpadChannelAdmissionError::Unavailable
        ))
    ));
    assert_eq!(control.borrow().close_count, 1);

    let mut epoch = binding();
    epoch.route_epoch += 1;
    let (channel, epoch_control) = FakeChannel::new(epoch.clone());
    assert!(matches!(
        PrivateTouchpadDeliverySession::new(
            epoch,
            PrivateTouchpadPacketSource::new(stream(), descriptor(), 0).expect("source"),
            channel,
        ),
        Err(PrivateTouchpadDeliveryBuildError::EpochMismatch { .. })
    ));
    assert_eq!(epoch_control.borrow().close_count, 1);
}

#[test]
fn abandoned_active_session_closes_channel_once() {
    let (channel, control) = FakeChannel::new(binding());
    let delivery = delivery(channel).expect("delivery");
    drop(delivery);
    assert_eq!(control.borrow().close_count, 1);
}
