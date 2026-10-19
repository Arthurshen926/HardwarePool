use capyio_core::{AuthorizationState, ProfileId, RouteBackend, RouteState, StreamId};
use capyio_protocol::*;
use capyio_testkit::DemoLab;

#[test]
fn haptics_offer_roundtrip_and_endpoint_validation() {
    let lab = DemoLab::new().unwrap();
    let mut route = lab
        .runtime
        .route(lab.routes.phone_microphone_to_windows)
        .unwrap()
        .clone();
    route.profile = ProfileId::haptics_feedback_v1();
    route.backend = RouteBackend::AdapterManaged;
    route.state = RouteState::Starting;
    route.epoch = 2;
    route.authorization = AuthorizationState::Authorized {
        expires_at_ms: Some(60_000),
    };
    let offer = HapticsDataOffer {
        port: 40211,
        stream_id: StreamId::new(),
    };
    let bytes = encode_route_status_with_haptics_data(route.session_id, &route, offer).unwrap();
    let decoded = decode_route_status(&bytes).unwrap();
    assert_eq!(decoded.haptics_data, Some(offer));
    assert_eq!(decoded.route, route);
    assert!(decoded.touchpad_data.is_none());
    for case in 0..4 {
        let mut envelope = decode_envelope(&bytes).unwrap();
        if let Some(v1::envelope::Payload::RouteStatus(status)) = &mut envelope.payload {
            match case {
                0 => status.haptics_data.as_mut().unwrap().port = 0,
                1 => status.haptics_data.as_mut().unwrap().port = 65536,
                2 => status.haptics_data.as_mut().unwrap().stream_id = "invalid".into(),
                _ => {
                    status.touchpad_data = Some(v1::TouchpadDataOffer {
                        port: 1,
                        stream_id: offer.stream_id.to_string(),
                    })
                }
            }
        }
        assert!(decode_route_status(&encode_envelope(&envelope)).is_err());
    }
    route.profile = ProfileId::touchpad_frames_v1();
    assert!(encode_route_status_with_haptics_data(route.session_id, &route, offer).is_err());
}
