use capyio_core::{
    AuthorizationState, FormatDescriptor, ProfileId, QosMode, Route, RouteBackend, RouteState,
    StreamId,
};
use capyio_protocol::*;
use prost::Message;

fn fixture() -> (Route, SecureAudioDataOffer) {
    let lab = capyio_testkit::DemoLab::new().unwrap();
    let mut route = lab
        .runtime
        .route(lab.routes.phone_microphone_to_windows)
        .unwrap()
        .clone();
    route.profile = ProfileId::audio_frames_v1();
    route.backend = RouteBackend::AdapterManaged;
    route.state = RouteState::Starting;
    route.epoch = 2;
    route.compatible_formats = vec![FormatDescriptor::new(SECURE_AUDIO_MONO_FORMAT)];
    route.selected_format = route.compatible_formats.first().cloned();
    route.compatible_qos_modes = [QosMode::Interactive].into_iter().collect();
    route.selected_qos_mode = Some(QosMode::Interactive);
    route.authorization = AuthorizationState::Authorized {
        expires_at_ms: Some(60_000),
    };
    (
        route,
        SecureAudioDataOffer {
            port: 49152,
            stream_id: StreamId::new(),
            transport: SecureAudioTransport::Tls13MtlsV1,
        },
    )
}

#[test]
fn secure_audio_has_distinct_tag_exact_format_and_actual_port() {
    let (route, offer) = fixture();
    for port in [1, 49152, 65535] {
        let offer = SecureAudioDataOffer { port, ..offer };
        let bytes =
            encode_route_status_with_secure_audio_data(route.session_id, &route, offer).unwrap();
        let decoded = decode_route_status(&bytes).unwrap();
        assert_eq!(decoded.secure_audio_data, Some(offer));
        assert!(decoded.native_audio_data.is_none());
        assert!(decoded.secure_camera_data.is_none());
    }
    let only_offer = v1::RouteStatus {
        secure_audio_data: Some(v1::SecureAudioDataOffer {
            port: 1,
            stream_id: "x".into(),
            transport: 1,
        }),
        ..Default::default()
    };
    // Frozen independent tag7; earlier offer tags retain their meaning.
    assert_eq!(
        only_offer.encode_to_vec(),
        [58, 7, 8, 1, 18, 1, b'x', 24, 1]
    );
}

#[test]
fn secure_audio_rejects_unknown_transport_port_stream_and_every_mixed_offer() {
    let (route, offer) = fixture();
    let bytes =
        encode_route_status_with_secure_audio_data(route.session_id, &route, offer).unwrap();
    for case in 0..13 {
        let mut envelope = decode_envelope(&bytes).unwrap();
        let Some(v1::envelope::Payload::RouteStatus(status)) = envelope.payload.as_mut() else {
            panic!()
        };
        let secure = status.secure_audio_data.as_mut().unwrap();
        match case {
            0 => secure.transport = 0,
            1 => secure.transport = 2,
            2 => secure.transport = -1,
            3 => secure.port = 0,
            4 => secure.port = 65536,
            5 => secure.stream_id = "00000000-0000-0000-0000-000000000000".into(),
            6 => secure.stream_id = "invalid".into(),
            7 => {
                status.native_audio_data = Some(v1::NativeAudioDataOffer {
                    port: 1,
                    stream_id: offer.stream_id.to_string(),
                    endpoint: 2,
                    preset: 1,
                    transport: 1,
                })
            }
            8 => {
                status.touchpad_data = Some(v1::TouchpadDataOffer {
                    port: 1,
                    stream_id: offer.stream_id.to_string(),
                })
            }
            9 => {
                status.haptics_data = Some(v1::HapticsDataOffer {
                    port: 1,
                    stream_id: offer.stream_id.to_string(),
                })
            }
            10 => {
                status.camera_data = Some(v1::CameraDataOffer {
                    port: 38173,
                    stream_id: offer.stream_id.to_string(),
                })
            }
            11 => {
                status.secure_camera_data = Some(v1::SecureCameraDataOffer {
                    port: 1,
                    stream_id: offer.stream_id.to_string(),
                    transport: 1,
                })
            }
            _ => status.route.as_mut().unwrap().selected_qos_mode = None,
        }
        assert!(
            decode_route_status(&envelope.encode_to_vec()).is_err(),
            "case {case}"
        );
    }
}

#[test]
fn secure_audio_rejects_nonexact_format_qos_state_and_grant() {
    let (route, offer) = fixture();
    for case in 0..14 {
        let mut bad = route.clone();
        match case {
            0 => bad.selected_format = Some(FormatDescriptor::new("pcm-s16le-48000-mono")),
            1 => bad.selected_format = Some(FormatDescriptor::new("pcm-s16le-48000-stereo")),
            2 => {
                bad.selected_format
                    .as_mut()
                    .unwrap()
                    .parameters
                    .insert("channels".into(), "1".into());
            }
            3 => bad.selected_format = None,
            4 => bad.selected_qos_mode = None,
            5 => bad.selected_qos_mode = Some(QosMode::Basic),
            6 => bad.profile = ProfileId::video_frames_v1(),
            7 => bad.epoch = 0,
            8 => bad.epoch = i32::MAX as u64 + 1,
            9 => bad.state = RouteState::Draft,
            10 => {
                bad.authorization = AuthorizationState::Authorized {
                    expires_at_ms: None,
                }
            }
            11 => {
                bad.authorization = AuthorizationState::Authorized {
                    expires_at_ms: Some(0),
                }
            }
            12 => {
                bad.authorization = AuthorizationState::Authorized {
                    expires_at_ms: Some(9_007_199_254_740_992),
                }
            }
            _ => bad.source.node_id = bad.sink.node_id,
        }
        assert!(offer.validate(&bad).is_err(), "case {case}");
        assert!(encode_route_status_with_secure_audio_data(bad.session_id, &bad, offer).is_err());
    }
}

#[test]
fn secure_audio_cannot_enter_legacy_udp_or_silently_downgrade() {
    let (mut route, offer) = fixture();
    let legacy = NativeAudioDataOffer {
        port: offer.port,
        stream_id: offer.stream_id,
        endpoint: NativeAudioEndpoint::Sink,
        preset: NativeAudioPreset::PcmS16le48000Mono,
    };
    assert!(encode_route_status_with_native_audio_data(route.session_id, &route, legacy).is_err());
    // An unrecognized secure transport never becomes a valid ordinary offer.
    let bytes =
        encode_route_status_with_secure_audio_data(route.session_id, &route, offer).unwrap();
    let mut envelope = decode_envelope(&bytes).unwrap();
    let Some(v1::envelope::Payload::RouteStatus(status)) = envelope.payload.as_mut() else {
        panic!()
    };
    status.secure_audio_data.as_mut().unwrap().transport = 99;
    assert!(decode_route_status(&envelope.encode_to_vec()).is_err());
    route.selected_format = Some(FormatDescriptor::new(legacy.preset.format_name()));
    route.compatible_formats = vec![route.selected_format.clone().unwrap()];
    let decoded = decode_route_status(
        &encode_route_status_with_native_audio_data(route.session_id, &route, legacy).unwrap(),
    )
    .unwrap();
    assert_eq!(decoded.native_audio_data.as_deref(), Some(&legacy));
    assert!(decoded.secure_audio_data.is_none());
    assert!(offer.validate(&route).is_err());
    assert_eq!(
        v1::NativeAudioDataOffer {
            port: 1,
            stream_id: "x".into(),
            endpoint: 2,
            preset: 1,
            transport: 1
        }
        .encode_to_vec(),
        [8, 1, 18, 1, b'x', 24, 2, 32, 1, 40, 1]
    );
}

#[test]
fn finite_profiles_are_exact_distinct_and_never_legacy_udp() {
    use std::time::Duration;
    for (profile, seconds) in [
        (SecureAudioSessionProfile::Finite60sV1, 60),
        (SecureAudioSessionProfile::Finite300sV1, 300),
    ] {
        assert_eq!(profile.max_duration(), Duration::from_secs(seconds));
        assert_eq!(
            SecureAudioSessionProfile::from_descriptor(&profile.descriptor()),
            Some(profile)
        );
        let (mut route, offer) = fixture();
        route.selected_format = Some(profile.descriptor());
        route.compatible_formats = vec![profile.descriptor()];
        let encoded =
            encode_route_status_with_secure_audio_data(route.session_id, &route, offer).unwrap();
        let decoded = decode_route_status(&encoded).unwrap();
        assert_eq!(decoded.route.selected_format, Some(profile.descriptor()));
        assert_eq!(decoded.secure_audio_data, Some(offer));
        let udp = NativeAudioDataOffer {
            port: offer.port,
            stream_id: offer.stream_id,
            endpoint: NativeAudioEndpoint::Sink,
            preset: NativeAudioPreset::PcmS16le48000Mono,
        };
        assert!(encode_route_status_with_native_audio_data(route.session_id, &route, udp).is_err());
        let mut malformed = profile.descriptor();
        malformed
            .parameters
            .insert("duration".into(), seconds.to_string());
        assert_eq!(SecureAudioSessionProfile::from_descriptor(&malformed), None);
        route.selected_format = Some(malformed);
        assert!(offer.validate(&route).is_err());
        assert!(encode_route_status_with_native_audio_data(route.session_id, &route, udp).is_err());
    }
    for unknown in [
        "pcm-s16le-48000-mono",
        "pcm-s16le-48000-mono-tls13-301s-v1",
        "",
    ] {
        assert_eq!(
            SecureAudioSessionProfile::from_descriptor(&FormatDescriptor::new(unknown)),
            None
        );
    }
}

#[test]
fn secure_stereo_is_explicit_and_never_a_mono_profile_or_transport() {
    let (mut route, mono) = fixture();
    let stereo = SecureAudioDataOffer {
        transport: SecureAudioTransport::Tls13MtlsStereoV1,
        ..mono
    };
    // The old wire value keeps both old mono interpretations and no others.
    for name in [SECURE_AUDIO_MONO_FORMAT, SECURE_AUDIO_MONO_300S_FORMAT] {
        route.selected_format = Some(FormatDescriptor::new(name));
        route.compatible_formats = vec![route.selected_format.clone().unwrap()];
        assert!(mono.validate(&route).is_ok());
        assert!(stereo.validate(&route).is_err());
    }
    route.selected_format = Some(FormatDescriptor::new(SECURE_AUDIO_STEREO_FORMAT));
    route.compatible_formats = vec![route.selected_format.clone().unwrap()];
    assert_eq!(
        SecureAudioSessionProfile::from_descriptor(route.selected_format.as_ref().unwrap()),
        None
    );
    assert!(mono.validate(&route).is_err());
    assert!(stereo.validate(&route).is_ok());
    let bytes =
        encode_route_status_with_secure_audio_data(route.session_id, &route, stereo).unwrap();
    let decoded = decode_route_status(&bytes).unwrap();
    assert_eq!(decoded.secure_audio_data, Some(stereo));
    assert_eq!(decoded.route.selected_format, route.selected_format);
    let mut wire = decode_envelope(&bytes).unwrap();
    let Some(v1::envelope::Payload::RouteStatus(status)) = wire.payload.as_mut() else {
        panic!()
    };
    assert_eq!(status.secure_audio_data.as_ref().unwrap().transport, 2);
    status.secure_audio_data.as_mut().unwrap().transport = 1;
    assert!(decode_route_status(&wire.encode_to_vec()).is_err());
    // No implicit downgrade to a trusted-LAN stereo endpoint.
    let udp = NativeAudioDataOffer {
        port: stereo.port,
        stream_id: stereo.stream_id,
        endpoint: NativeAudioEndpoint::Sink,
        preset: NativeAudioPreset::PcmS16le48000Stereo,
    };
    assert!(encode_route_status_with_native_audio_data(route.session_id, &route, udp).is_err());
    assert_eq!(
        v1::SecureAudioDataOffer {
            port: 1,
            stream_id: "x".into(),
            transport: 2,
        }
        .encode_to_vec(),
        [8, 1, 18, 1, b'x', 24, 2]
    );
}

#[test]
fn secure_stereo_closed_descriptor_qos_and_offer_validation() {
    let (mut route, mut offer) = fixture();
    route.selected_format = Some(FormatDescriptor::new(SECURE_AUDIO_STEREO_FORMAT));
    route.compatible_formats = vec![route.selected_format.clone().unwrap()];
    offer.transport = SecureAudioTransport::Tls13MtlsStereoV1;
    for case in 0..11 {
        let mut bad = route.clone();
        match case {
            0 => {
                bad.selected_format
                    .as_mut()
                    .unwrap()
                    .parameters
                    .insert("channels".into(), "2".into());
            }
            1 => bad.selected_format = Some(FormatDescriptor::new("pcm-s16le-48000-stereo")),
            2 => {
                bad.selected_format = Some(FormatDescriptor::new(
                    "pcm-s16le-48000-stereo-tls13-300s-v1",
                ))
            }
            3 => bad.selected_format = None,
            4 => bad.selected_qos_mode = Some(QosMode::Basic),
            5 => bad.selected_qos_mode = None,
            6 => bad.epoch = 0,
            7 => bad.epoch = i32::MAX as u64 + 1,
            8 => bad.state = RouteState::Prepared,
            9 => bad.authorization = AuthorizationState::Pending,
            _ => bad.source.node_id = bad.sink.node_id,
        }
        assert!(offer.validate(&bad).is_err(), "case {case}");
    }
    let bytes =
        encode_route_status_with_secure_audio_data(route.session_id, &route, offer).unwrap();
    for bad_transport in [0, 1, 3, -1, i32::MAX] {
        let mut wire = decode_envelope(&bytes).unwrap();
        let Some(v1::envelope::Payload::RouteStatus(status)) = wire.payload.as_mut() else {
            panic!()
        };
        status.secure_audio_data.as_mut().unwrap().transport = bad_transport;
        assert!(decode_route_status(&wire.encode_to_vec()).is_err());
    }
}
