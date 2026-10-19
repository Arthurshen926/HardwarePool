use capyio_core::{AuthorizationState, Route, RouteBackend, RouteState, StreamId};
use capyio_protocol::*;
use capyio_testkit::DemoLab;

fn fixture() -> (Route, NativeAudioDataOffer) {
    let lab = DemoLab::new().unwrap();
    let mut route = lab
        .runtime
        .route(lab.routes.phone_microphone_to_windows)
        .unwrap()
        .clone();
    route.backend = RouteBackend::AdapterManaged;
    route.state = RouteState::Starting;
    route.epoch = 2;
    route.authorization = AuthorizationState::Authorized {
        expires_at_ms: Some(60_000),
    };
    (
        route,
        NativeAudioDataOffer {
            port: 40211,
            stream_id: StreamId::new(),
            endpoint: NativeAudioEndpoint::Sink,
            preset: NativeAudioPreset::PcmS16le48000Mono,
        },
    )
}

#[test]
fn audio_offer_roundtrips_both_endpoint_directions_without_touchpad_offer() {
    let (route, mut offer) = fixture();
    for endpoint in [NativeAudioEndpoint::Source, NativeAudioEndpoint::Sink] {
        offer.endpoint = endpoint;
        let bytes =
            encode_route_status_with_native_audio_data(route.session_id, &route, offer).unwrap();
        let decoded = decode_route_status(&bytes).unwrap();
        assert_eq!(decoded.route, route);
        assert_eq!(decoded.native_audio_data.as_deref(), Some(&offer));
        assert!(decoded.touchpad_data.is_none());
    }
    let plain =
        decode_route_status(&encode_route_status(route.session_id, &route).unwrap()).unwrap();
    assert!(plain.native_audio_data.is_none());
    let mut older =
        decode_envelope(&encode_route_status(route.session_id, &route).unwrap()).unwrap();
    older.protocol_minor = 3;
    assert!(
        decode_route_status(&encode_envelope(&older))
            .unwrap()
            .native_audio_data
            .is_none()
    );
}

#[test]
fn stereo_preset_roundtrips_without_becoming_mono() {
    let (mut route, mut offer) = fixture();
    route.selected_format = Some(capyio_core::FormatDescriptor::new("pcm-s16le-48000-stereo"));
    offer.preset = NativeAudioPreset::PcmS16le48000Stereo;
    let encoded =
        encode_route_status_with_native_audio_data(route.session_id, &route, offer).unwrap();
    let decoded = decode_route_status(&encoded).unwrap();
    assert_eq!(decoded.native_audio_data.as_deref(), Some(&offer));
    assert_eq!(offer.preset.channels(), 2);
    assert_eq!(NativeAudioPreset::PcmS16le48000Mono.channels(), 1);
    let envelope = decode_envelope(&encoded).unwrap();
    let Some(v1::envelope::Payload::RouteStatus(status)) = envelope.payload else {
        panic!("route status")
    };
    assert_eq!(status.native_audio_data.unwrap().preset, 2);
}

#[test]
fn malformed_wire_offers_never_become_a_supported_audio_transport() {
    let (route, offer) = fixture();
    let bytes =
        encode_route_status_with_native_audio_data(route.session_id, &route, offer).unwrap();
    for case in 0..10 {
        let mut envelope = decode_envelope(&bytes).unwrap();
        if let Some(v1::envelope::Payload::RouteStatus(status)) = &mut envelope.payload {
            let audio = status.native_audio_data.as_mut().unwrap();
            match case {
                0 => audio.port = 0,
                1 => audio.port = 65536,
                2 => audio.stream_id = "00000000-0000-0000-0000-000000000000".into(),
                3 => audio.endpoint = 0,
                4 => audio.endpoint = 99,
                5 => audio.preset = 0,
                6 => audio.preset = 99,
                7 => audio.transport = 0,
                8 => audio.transport = 99,
                _ => {
                    status.touchpad_data = Some(v1::TouchpadDataOffer {
                        port: 1,
                        stream_id: offer.stream_id.to_string(),
                    })
                }
            }
        }
        assert!(
            decode_route_status(&encode_envelope(&envelope)).is_err(),
            "case {case}"
        );
    }
}

#[test]
fn non_running_unbounded_or_wrong_profile_routes_cannot_carry_audio_offers() {
    let (route, offer) = fixture();
    for case in 0..8 {
        let mut invalid = route.clone();
        match case {
            0 => invalid.state = RouteState::Stopped,
            1 => invalid.authorization = AuthorizationState::Pending,
            2 => {
                invalid.authorization = AuthorizationState::Authorized {
                    expires_at_ms: None,
                }
            }
            3 => {
                invalid.authorization = AuthorizationState::Authorized {
                    expires_at_ms: Some(9_007_199_254_740_992),
                }
            }
            4 => invalid.epoch = i32::MAX as u64 + 1,
            5 => invalid.profile = capyio_core::ProfileId::touchpad_frames_v1(),
            6 => invalid.backend = RouteBackend::CapyDataPlane,
            _ => invalid.sink.node_id = invalid.source.node_id,
        }
        assert!(
            encode_route_status_with_native_audio_data(invalid.session_id, &invalid, offer)
                .is_err()
        );
    }
}
