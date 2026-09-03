use capyio_core::{PortRef, RouteId, SessionId};
use capyio_input::{
    InputFrameHeader, InputStreamDescriptor, TouchpadButtonState, TouchpadButtonType,
    TouchpadDescriptor, TouchpadFrame, TouchpadFrameKind, TouchpadPhysicalSize,
};
use capyio_remote_touchpad_adapter::{
    MAX_PRIVATE_TOUCHPAD_HOST_CHANNEL_PACKETS, PrivateTouchpadAdmittedChannel,
    PrivateTouchpadChannelAdmissionError, PrivateTouchpadChannelSendOutcome,
    PrivateTouchpadHostChannelBuildError, PrivateTouchpadHostChannelReceiveOutcome,
    PrivateTouchpadPacketCodecV1, PrivateTouchpadRouteBinding, private_touchpad_host_channel,
};

fn id<T: std::str::FromStr>(value: &str) -> T
where
    T::Err: std::fmt::Debug,
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
        route_id: id::<RouteId>("00000000-0000-4000-8000-00000000f001"),
        session_id: id::<SessionId>("00000000-0000-4000-8000-00000000f002"),
        source: port(
            "00000000-0000-4000-8000-00000000f003",
            "00000000-0000-4000-8000-00000000f004",
            "00000000-0000-4000-8000-00000000f005",
        ),
        sink: port(
            "00000000-0000-4000-8000-00000000f006",
            "00000000-0000-4000-8000-00000000f007",
            "00000000-0000-4000-8000-00000000f008",
        ),
        route_epoch: epoch,
        authorization_expires_at_ms: Some(10_000),
    }
}

fn stream() -> InputStreamDescriptor {
    InputStreamDescriptor {
        stream_id: id("00000000-0000-4000-8000-00000000f009"),
        stream_epoch: 1,
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

fn packet(sequence: u64) -> capyio_remote_touchpad_adapter::PrivateTouchpadPacketV1 {
    PrivateTouchpadPacketCodecV1::new(stream(), descriptor())
        .expect("codec")
        .encode(&TouchpadFrame {
            header: InputFrameHeader {
                stream_id: stream().stream_id,
                stream_epoch: 1,
                sequence,
                source_timestamp_nanos: sequence + 1,
            },
            kind: TouchpadFrameKind::CancelAll,
            button: TouchpadButtonState::Released,
            contacts: Vec::new(),
        })
        .expect("packet")
}

#[test]
fn channel_capacity_is_closed_and_full_rejection_writes_nothing() {
    for capacity in [0, MAX_PRIVATE_TOUCHPAD_HOST_CHANNEL_PACKETS + 1] {
        assert!(matches!(
            private_touchpad_host_channel(binding(1), capacity),
            Err(PrivateTouchpadHostChannelBuildError::InvalidCapacity { .. })
        ));
    }

    let (admission, mut sender, mut receiver) =
        private_touchpad_host_channel(binding(1), 1).expect("channel");
    assert_eq!(
        sender.send(&packet(0)),
        PrivateTouchpadChannelSendOutcome::Delivered
    );
    assert_eq!(
        sender.send(&packet(1)),
        PrivateTouchpadChannelSendOutcome::RejectedBeforeWrite
    );
    assert!(matches!(
        receiver.receive(),
        PrivateTouchpadHostChannelReceiveOutcome::Packet(_)
    ));
    assert_eq!(
        sender.send(&packet(1)),
        PrivateTouchpadChannelSendOutcome::Delivered
    );
    sender.close();
    sender.close();
    assert!(matches!(
        receiver.receive(),
        PrivateTouchpadHostChannelReceiveOutcome::Packet(_)
    ));
    assert_eq!(
        receiver.receive(),
        PrivateTouchpadHostChannelReceiveOutcome::Closed
    );

    let metrics = admission.metrics();
    assert_eq!(metrics.packets_enqueued, 2);
    assert_eq!(metrics.packets_received, 2);
    assert_eq!(metrics.rejected_before_write, 1);
    assert_eq!(metrics.sender_closes, 1);
}

#[test]
fn denial_binding_replacement_and_receiver_close_discard_stale_packets() {
    let (mut admission, mut sender, mut receiver) =
        private_touchpad_host_channel(binding(1), 2).expect("channel");
    assert_eq!(
        sender.send(&packet(0)),
        PrivateTouchpadChannelSendOutcome::Delivered
    );
    admission.deny(PrivateTouchpadChannelAdmissionError::Revoked);
    assert_eq!(
        sender.current_binding(),
        Err(PrivateTouchpadChannelAdmissionError::Revoked)
    );
    assert_eq!(admission.metrics().packets_discarded, 1);

    admission.replace_binding(binding(2));
    assert_eq!(
        sender.current_binding().expect("new binding").route_epoch,
        2
    );
    assert_eq!(
        sender.send(&packet(1)),
        PrivateTouchpadChannelSendOutcome::Delivered
    );
    receiver.close();
    receiver.close();
    assert_eq!(
        sender.current_binding(),
        Err(PrivateTouchpadChannelAdmissionError::Unavailable)
    );
    assert_eq!(
        sender.send(&packet(2)),
        PrivateTouchpadChannelSendOutcome::RejectedBeforeWrite
    );

    let metrics = admission.metrics();
    assert_eq!(metrics.packets_discarded, 2);
    assert_eq!(metrics.receiver_closes, 1);
    assert_eq!(metrics.rejected_before_write, 1);
}
