use capyio_core::{
    AuthorizationState, FormatDescriptor, ProfileId, RouteBackend, RouteState, StreamId,
};
use capyio_protocol::*;

#[test]
fn camera_offer_roundtrip_is_closed_to_live_finite_cavc_routes() {
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
    let offer = CameraDataOffer {
        port: 38173,
        stream_id: StreamId::new(),
    };
    let encoded = encode_route_status_with_camera_data(route.session_id, &route, offer).unwrap();
    let decoded = decode_route_status(&encoded).unwrap();
    assert_eq!(decoded.camera_data, Some(offer));
    assert!(decoded.native_audio_data.is_none());
    assert!(decoded.touchpad_data.is_none());
    assert!(
        decode_route_status(&encode_route_status(route.session_id, &route).unwrap())
            .unwrap()
            .camera_data
            .is_none()
    );
    assert!(
        CameraDataOffer { port: 1, ..offer }
            .validate(&route)
            .is_err()
    );
    route.authorization = AuthorizationState::Authorized {
        expires_at_ms: None,
    };
    assert!(offer.validate(&route).is_err());
}
