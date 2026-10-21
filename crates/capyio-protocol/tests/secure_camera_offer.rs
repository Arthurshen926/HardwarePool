use capyio_core::{
    AuthorizationState, FormatDescriptor, ProfileId, Route, RouteBackend, RouteState, StreamId,
};
use capyio_protocol::*;
use prost::Message;

fn fixture() -> (Route, SecureCameraDataOffer) {
    let lab = capyio_testkit::DemoLab::new().unwrap();
    let mut route = lab
        .runtime
        .route(lab.routes.phone_microphone_to_windows)
        .unwrap()
        .clone();
    route.profile = ProfileId::video_frames_v1();
    route.backend = RouteBackend::AdapterManaged;
    route.state = RouteState::Starting;
    route.epoch = 2;
    route.compatible_formats = vec![FormatDescriptor::new("cavc-avc-access-unit-v1")];
    route.selected_format = route.compatible_formats.first().cloned();
    route.authorization = AuthorizationState::Authorized {
        expires_at_ms: Some(60_000),
    };
    (
        route,
        SecureCameraDataOffer {
            port: 49152,
            stream_id: StreamId::new(),
            transport: SecureCameraTransport::Tls13MtlsV1,
        },
    )
}

#[test]
fn secure_offer_has_distinct_tag_transport_and_dynamic_port() {
    let (route, offer) = fixture();
    let bytes =
        encode_route_status_with_secure_camera_data(route.session_id, &route, offer).unwrap();
    let decoded = decode_route_status(&bytes).unwrap();
    assert_eq!(decoded.secure_camera_data, Some(offer));
    assert!(decoded.camera_data.is_none());
    for port in [1, 65535] {
        let mut active = route.clone();
        active.state = RouteState::Active;
        let boundary = SecureCameraDataOffer { port, ..offer };
        let bytes =
            encode_route_status_with_secure_camera_data(active.session_id, &active, boundary)
                .unwrap();
        assert_eq!(
            decode_route_status(&bytes).unwrap().secure_camera_data,
            Some(boundary)
        );
    }
    let v1::envelope::Payload::RouteStatus(status) =
        decode_envelope(&bytes).unwrap().payload.unwrap()
    else {
        panic!()
    };
    let wire = status.secure_camera_data.unwrap();
    assert_eq!(wire.transport, 1);
    assert_eq!(wire.port, 49152);
    // Standalone canonical protobuf bytes: tag 1 port, tag 2 Stream ID, tag 3 transport.
    assert_eq!(
        &wire.encode_to_vec()[wire.encode_to_vec().len() - 2..],
        &[24, 1]
    );
    let legacy = CameraDataOffer {
        port: 38173,
        stream_id: offer.stream_id,
    };
    let bytes = encode_route_status_with_camera_data(route.session_id, &route, legacy).unwrap();
    let decoded = decode_route_status(&bytes).unwrap();
    assert_eq!(decoded.camera_data, Some(legacy));
    assert!(decoded.secure_camera_data.is_none());
    assert!(
        CameraDataOffer {
            port: offer.port,
            ..legacy
        }
        .validate(&route)
        .is_err()
    );
    // Frozen legacy message encoding: port 38173, one-byte stream placeholder, no transport field.
    assert_eq!(
        v1::CameraDataOffer {
            port: 38173,
            stream_id: "x".into()
        }
        .encode_to_vec(),
        vec![8, 157, 170, 2, 18, 1, b'x']
    );
}

#[test]
fn malformed_secure_wire_and_conflicting_offers_fail_closed() {
    let (route, offer) = fixture();
    let encoded =
        encode_route_status_with_secure_camera_data(route.session_id, &route, offer).unwrap();
    for case in 0..11 {
        let mut envelope = decode_envelope(&encoded).unwrap();
        let Some(v1::envelope::Payload::RouteStatus(status)) = envelope.payload.as_mut() else {
            panic!()
        };
        let wire = status.secure_camera_data.as_mut().unwrap();
        match case {
            0 => wire.transport = 0,
            1 => wire.transport = 2,
            2 => wire.transport = -1,
            3 => wire.port = 0,
            4 => wire.port = 65536,
            5 => wire.stream_id = "00000000-0000-0000-0000-000000000000".into(),
            6 => {
                status.camera_data = Some(v1::CameraDataOffer {
                    port: 38173,
                    stream_id: offer.stream_id.to_string(),
                })
            }
            7 => {
                status.touchpad_data = Some(v1::TouchpadDataOffer {
                    port: 12,
                    stream_id: offer.stream_id.to_string(),
                })
            }
            8 => {
                status.haptics_data = Some(v1::HapticsDataOffer {
                    port: 12,
                    stream_id: offer.stream_id.to_string(),
                })
            }
            9 => wire.stream_id = "invalid".into(),
            10 => {
                status.native_audio_data = Some(v1::NativeAudioDataOffer {
                    port: 12,
                    stream_id: offer.stream_id.to_string(),
                    endpoint: 1,
                    preset: 1,
                    transport: 1,
                })
            }
            _ => unreachable!(),
        }
        assert!(
            decode_route_status(&envelope.encode_to_vec()).is_err(),
            "case {case}"
        );
    }
}

#[test]
fn exact_format_epoch_state_and_bounded_grant_are_required() {
    let (route, offer) = fixture();
    for case in 0..13 {
        let mut bad = route.clone();
        match case {
            0 => bad.epoch = 0,
            1 => bad.epoch = i32::MAX as u64 + 1,
            2 => bad.selected_format = Some(FormatDescriptor::new("h264")),
            3 => bad.selected_format = None,
            4 => bad.backend = RouteBackend::CapyDataPlane,
            5 => bad.state = RouteState::Draft,
            6 => bad.authorization = AuthorizationState::Pending,
            7 => {
                bad.authorization = AuthorizationState::Authorized {
                    expires_at_ms: None,
                }
            }
            8 => {
                bad.authorization = AuthorizationState::Authorized {
                    expires_at_ms: Some(0),
                }
            }
            9 => {
                bad.authorization = AuthorizationState::Authorized {
                    expires_at_ms: Some(9_007_199_254_740_992),
                }
            }
            10 => bad.source.node_id = bad.sink.node_id,
            11 => bad.profile = ProfileId::audio_frames_v1(),
            12 => {
                bad.selected_format
                    .as_mut()
                    .unwrap()
                    .parameters
                    .insert("extra".into(), "1".into());
            }
            _ => unreachable!(),
        }
        assert!(
            encode_route_status_with_secure_camera_data(bad.session_id, &bad, offer).is_err(),
            "case {case}"
        );
    }
}
