use capyio_android_jni::{
    ANDROID_ACTION_DOWN, ANDROID_ACTION_MOVE, ANDROID_ACTION_POINTER_DOWN,
    ANDROID_ACTION_POINTER_UP, ANDROID_ACTION_UP, ANDROID_TOOL_TYPE_FINGER, AndroidMotionDtoV1,
    AndroidTouchpadBridgeConfigV1, AndroidTouchpadPacketSessionV1, AndroidTouchpadRecordSessionV1,
    AndroidTouchpadRouteConfigV1,
};
use capyio_remote_touchpad_adapter::{
    PrivateTouchpadPacketCodecV1, PrivateTouchpadRouteBinding, PrivateTouchpadTransportCodecV1,
};

fn config() -> AndroidTouchpadBridgeConfigV1 {
    AndroidTouchpadBridgeConfigV1 {
        stream_id: "4cdb7d96-2485-44ce-9e1c-f1cd7856482a".to_owned(),
        stream_epoch: 7,
        clock_domain_id: "android.elapsed-realtime-nanos".to_owned(),
        width_px: 1080,
        height_px: 1920,
        width_himetric: 6800,
        height_himetric: 12100,
        max_contacts: 5,
        reports_pressure: true,
        first_sequence: 10,
    }
}

fn route_config() -> AndroidTouchpadRouteConfigV1 {
    AndroidTouchpadRouteConfigV1 {
        route_id: "00000000-0000-4000-8000-00000000f101".to_owned(),
        session_id: "00000000-0000-4000-8000-00000000f102".to_owned(),
        source_node_id: "00000000-0000-4000-8000-00000000f103".to_owned(),
        source_capability_id: "00000000-0000-4000-8000-00000000f104".to_owned(),
        source_port_id: "00000000-0000-4000-8000-00000000f105".to_owned(),
        sink_node_id: "00000000-0000-4000-8000-00000000f106".to_owned(),
        sink_capability_id: "00000000-0000-4000-8000-00000000f107".to_owned(),
        sink_port_id: "00000000-0000-4000-8000-00000000f108".to_owned(),
        authorization_expires_at_ms: None,
    }
}

fn transport(epoch: u64) -> PrivateTouchpadTransportCodecV1 {
    let route = route_config();
    let port = |node: &str, capability: &str, port: &str| capyio_core::PortRef {
        node_id: node.parse().expect("node"),
        capability_id: capability.parse().expect("capability"),
        port_id: port.parse().expect("port"),
    };
    PrivateTouchpadTransportCodecV1::new(PrivateTouchpadRouteBinding {
        route_id: route.route_id.parse().expect("route"),
        session_id: route.session_id.parse().expect("session"),
        source: port(
            &route.source_node_id,
            &route.source_capability_id,
            &route.source_port_id,
        ),
        sink: port(
            &route.sink_node_id,
            &route.sink_capability_id,
            &route.sink_port_id,
        ),
        route_epoch: epoch,
        authorization_expires_at_ms: None,
    })
}

// Keep the primitive-array shape visible at each test call, matching the JNI contract.
#[allow(clippy::too_many_arguments)]
fn motion<'a>(
    timestamp: u64,
    action: i32,
    action_index: usize,
    ids: &'a [i32],
    x: &'a [f32],
    y: &'a [f32],
    tool_types: &'a [i32],
    pressure: &'a [f32],
) -> AndroidMotionDtoV1<'a> {
    AndroidMotionDtoV1 {
        event_time_nanos: timestamp,
        action,
        action_index,
        pointer_ids: ids,
        tool_types,
        x_px: x,
        y_px: y,
        pressure,
    }
}

#[test]
fn one_finger_motion_becomes_contiguous_private_packets() {
    let config = config();
    let stream_id = config.stream_id.parse().expect("stream ID");
    let stream = capyio_input::InputStreamDescriptor {
        stream_id,
        stream_epoch: config.stream_epoch,
        clock_domain_id: config.clock_domain_id.clone(),
    };
    let descriptor = capyio_input::TouchpadDescriptor {
        physical_size: capyio_input::TouchpadPhysicalSize {
            width_himetric: config.width_himetric,
            height_himetric: config.height_himetric,
        },
        max_contacts: config.max_contacts,
        button_type: capyio_input::TouchpadButtonType::NonClickable,
        reports_contact_size: false,
        reports_pressure: true,
    };
    let codec = PrivateTouchpadPacketCodecV1::new(stream, descriptor).expect("codec");
    let mut session = AndroidTouchpadPacketSessionV1::new(config).expect("session");

    let cancel = session.start(100).expect("start cancellation");
    let down = session
        .motion(motion(
            110,
            ANDROID_ACTION_DOWN,
            0,
            &[3],
            &[100.0],
            &[200.0],
            &[ANDROID_TOOL_TYPE_FINGER],
            &[0.5],
        ))
        .expect("down");
    let moved = session
        .motion(motion(
            120,
            ANDROID_ACTION_MOVE,
            0,
            &[3],
            &[150.0],
            &[260.0],
            &[ANDROID_TOOL_TYPE_FINGER],
            &[0.5],
        ))
        .expect("move");
    let up = session
        .motion(motion(
            130,
            ANDROID_ACTION_UP,
            0,
            &[3],
            &[150.0],
            &[260.0],
            &[ANDROID_TOOL_TYPE_FINGER],
            &[0.5],
        ))
        .expect("up");
    assert_eq!(
        codec
            .decode(cancel.as_bytes())
            .expect("cancel")
            .header
            .sequence,
        10
    );
    assert_eq!(
        codec.decode(down.as_bytes()).expect("down").contacts.len(),
        1
    );
    assert_eq!(
        codec
            .decode(moved.as_bytes())
            .expect("move")
            .header
            .sequence,
        12
    );
    assert!(codec.decode(up.as_bytes()).expect("up").contacts.is_empty());
    assert!(session.close(140).expect("close").is_some());
}

#[test]
fn two_finger_lifecycle_preserves_both_contacts_in_motion_packet() {
    let config = config();
    let stream_id = config.stream_id.parse().expect("stream ID");
    let descriptor = capyio_input::TouchpadDescriptor {
        physical_size: capyio_input::TouchpadPhysicalSize {
            width_himetric: config.width_himetric,
            height_himetric: config.height_himetric,
        },
        max_contacts: 5,
        button_type: capyio_input::TouchpadButtonType::NonClickable,
        reports_contact_size: false,
        reports_pressure: true,
    };
    let codec = PrivateTouchpadPacketCodecV1::new(
        capyio_input::InputStreamDescriptor {
            stream_id,
            stream_epoch: config.stream_epoch,
            clock_domain_id: config.clock_domain_id.clone(),
        },
        descriptor,
    )
    .expect("codec");
    let mut session = AndroidTouchpadPacketSessionV1::new(config).expect("session");
    session.start(100).expect("start");
    session
        .motion(motion(
            110,
            ANDROID_ACTION_DOWN,
            0,
            &[1],
            &[100.0],
            &[200.0],
            &[ANDROID_TOOL_TYPE_FINGER],
            &[0.5],
        ))
        .expect("first down");
    session
        .motion(motion(
            120,
            ANDROID_ACTION_POINTER_DOWN,
            1,
            &[1, 4],
            &[100.0, 800.0],
            &[200.0, 1000.0],
            &[ANDROID_TOOL_TYPE_FINGER, ANDROID_TOOL_TYPE_FINGER],
            &[0.5, 0.6],
        ))
        .expect("second down");
    let moved = session
        .motion(motion(
            130,
            ANDROID_ACTION_MOVE,
            0,
            &[1, 4],
            &[100.0, 800.0],
            &[260.0, 1060.0],
            &[ANDROID_TOOL_TYPE_FINGER, ANDROID_TOOL_TYPE_FINGER],
            &[0.5, 0.6],
        ))
        .expect("two-finger move");
    assert_eq!(
        codec
            .decode(moved.as_bytes())
            .expect("decode")
            .contacts
            .len(),
        2
    );
    session
        .motion(motion(
            140,
            ANDROID_ACTION_POINTER_UP,
            1,
            &[1, 4],
            &[100.0, 800.0],
            &[260.0, 1060.0],
            &[ANDROID_TOOL_TYPE_FINGER, ANDROID_TOOL_TYPE_FINGER],
            &[0.5, 0.6],
        ))
        .expect("second up");
    session
        .motion(motion(
            150,
            ANDROID_ACTION_UP,
            0,
            &[1],
            &[100.0],
            &[260.0],
            &[ANDROID_TOOL_TYPE_FINGER],
            &[0.5],
        ))
        .expect("first up");
    assert!(session.close(160).expect("close").is_some());
}

#[test]
fn mismatched_pointer_arrays_are_rejected_before_capture_mutates() {
    let mut session = AndroidTouchpadPacketSessionV1::new(config()).expect("session");
    session.start(100).expect("start");
    let error = session
        .motion(AndroidMotionDtoV1 {
            event_time_nanos: 110,
            action: ANDROID_ACTION_DOWN,
            action_index: 0,
            pointer_ids: &[1],
            tool_types: &[],
            x_px: &[10.0],
            y_px: &[20.0],
            pressure: &[-1.0],
        })
        .expect_err("array mismatch");
    assert!(error.to_string().contains("identical lengths"));
}

#[test]
fn record_session_emits_bound_hello_data_ack_and_close() {
    let config = config();
    let codec = transport(config.stream_epoch);
    let mut session =
        AndroidTouchpadRecordSessionV1::new(config, route_config()).expect("record session");
    let hello = session.hello();
    codec.validate_hello(hello.as_bytes()).expect("bound hello");

    let initial = session.start(100).expect("initial cancellation");
    let initial_packet = codec.decode_data(initial.as_bytes()).expect("initial data");
    assert_eq!(initial_packet.as_bytes()[5], 1, "cancel_all frame kind");
    let ack = codec.encode_ack(10);
    session.validate_ack(ack.as_bytes(), 10).expect("exact ack");

    session
        .motion(motion(
            110,
            ANDROID_ACTION_DOWN,
            0,
            &[1],
            &[100.0],
            &[200.0],
            &[ANDROID_TOOL_TYPE_FINGER],
            &[0.5],
        ))
        .expect("down data");
    let cancellation = session.close(120).expect("close").expect("cancellation");
    codec
        .decode_data(cancellation.as_bytes())
        .expect("close data");
    codec
        .validate_close(session.close_record().as_bytes())
        .expect("close record");
}
