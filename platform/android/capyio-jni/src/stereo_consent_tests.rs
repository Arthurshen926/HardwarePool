//! Real TLS plus the dormant Rust request/receipt/resolve API. These tests do
//! not claim that existing JNI/audio dispatch has switched to bilateral consent.
use super::*;
use crate::stereo_negotiation::tests::fixture::{credentials, nodes};
use capyio_control_link::ReceivedControlMessage;
use capyio_control_tls::{
    AuthenticatedControlClientChannel, AuthenticatedControlServer, ControlTlsEndpoint,
};
use std::{collections::BTreeSet, net::TcpListener, thread};

fn catalog(revision: u64) -> String {
    let mut value: serde_json::Value =
        serde_json::from_str(include_str!("../../fixtures/android-core-catalog-v1.json")).unwrap();
    let cap = value["capabilities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|cap| cap["class"] == "speaker")
        .unwrap();
    cap["speaker_selection"] =
        serde_json::json!({"profile":crate::secure_speaker_policy::PROFILE,"revision":revision});
    cap["ports"][0]["formats"] = serde_json::json!([SECURE_AUDIO_STEREO_FORMAT]);
    value.to_string()
}
struct Fixture {
    host: AndroidCatalogRuntimeV1,
    source: NodeRuntime,
    sink_link: CatalogControlLink,
    source_link: CatalogControlLink,
    route: RouteId,
    android_identity: capyio_control_tls::ControlTlsServerIdentity,
    linux_identity: capyio_control_tls::ControlTlsClientIdentity,
    dispatch_error: Option<String>,
    collision_route: Option<Route>,
}
impl Fixture {
    fn new(android_accepts: bool) -> Self {
        Self::with_stage(android_accepts, true)
    }
    fn with_stage(android_accepts: bool, stage: bool) -> Self {
        Self::with_dispatch(android_accepts, stage, false)
    }
    fn with_dispatch(android_accepts: bool, stage: bool, dispatch: bool) -> Self {
        let fixture = Self::with_dispatch_tamper(android_accepts, stage, dispatch, |_| {}, false);
        assert!(fixture.dispatch_error.is_none());
        fixture
    }
    fn with_dispatch_tamper(
        android_accepts: bool,
        stage: bool,
        dispatch: bool,
        tamper: impl FnOnce(&mut NodeDescriptor),
        collision: bool,
    ) -> Self {
        let mut host = AndroidCatalogRuntimeV1::new(&catalog(1)).unwrap();
        let android = host.runtime.snapshot().local_node;
        let (_, mut linux, source, _) = nodes();
        if dispatch {
            linux
                .capabilities
                .get_mut(&source.capability_id)
                .unwrap()
                .metadata
                .insert(
                    crate::stereo_dispatch::BILATERAL_SOURCE_MARKER.into(),
                    crate::stereo_dispatch::BILATERAL_SOURCE_VERSION.into(),
                );
        }
        tamper(&mut linux);
        let cap = android
            .capabilities
            .values()
            .find(|cap| cap.class == CapabilityClass::Speaker)
            .unwrap();
        let sink = PortRef {
            node_id: android.id,
            capability_id: cap.id,
            port_id: *cap.ports.keys().next().unwrap(),
        };
        let binding = if android_accepts {
            ControlTlsBinding::new(SessionId::new(), android.id, linux.id)
        } else {
            ControlTlsBinding::new(SessionId::new(), linux.id, android.id)
        }
        .unwrap();
        let mut source_runtime = NodeRuntime::new(linux.clone()).unwrap();
        source_runtime.register_peer(android, true).unwrap();
        host.runtime.register_peer(linux, true).unwrap();
        let (android_server, android_client, linux_server, linux_client) = credentials();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = AuthenticatedControlServer::from_listener(
            listener,
            if android_accepts {
                android_server.clone()
            } else {
                linux_server
            },
            binding,
            Duration::from_secs(2),
        )
        .unwrap();
        let accepting = thread::spawn(move || server.accept().unwrap());
        let dialed = AuthenticatedControlClientChannel::connect(
            ControlTlsEndpoint {
                address,
                io_timeout: Duration::from_secs(2),
            },
            "localhost",
            if android_accepts {
                linux_client.clone()
            } else {
                android_client
            },
            binding,
        )
        .unwrap();
        let accepted = accepting.join().unwrap();
        let (mut sink_link, mut source_link) = if android_accepts {
            (
                CatalogControlLink::admit_accepted_peer(
                    &mut host.runtime,
                    accepted,
                    "Linux".into(),
                )
                .unwrap(),
                CatalogControlLink::admit_dialed_peer(
                    &mut source_runtime,
                    dialed,
                    "Android".into(),
                )
                .unwrap(),
            )
        } else {
            (
                CatalogControlLink::admit_dialed_peer(&mut host.runtime, dialed, "Linux".into())
                    .unwrap(),
                CatalogControlLink::admit_accepted_peer(
                    &mut source_runtime,
                    accepted,
                    "Android".into(),
                )
                .unwrap(),
            )
        };
        // Actual catalog wire exchange, not two pre-authorized fixture grants.
        source_link
            .send_catalog(
                source_link
                    .prepare_catalog(&source_runtime, &BTreeSet::from([source.capability_id]))
                    .unwrap(),
            )
            .unwrap();
        let incoming = sink_link.receive_catalog().unwrap();
        sink_link
            .apply_catalog(&mut host.runtime, incoming)
            .unwrap();
        sink_link
            .send_catalog(
                sink_link
                    .prepare_catalog(&host.runtime, &BTreeSet::new())
                    .unwrap(),
            )
            .unwrap();
        let incoming = source_link.receive_catalog().unwrap();
        source_link
            .apply_catalog(&mut source_runtime, incoming)
            .unwrap();
        let route = source_runtime
            .create_route(
                binding.session_id,
                source,
                sink,
                RouteBackend::AdapterManaged,
            )
            .unwrap();
        source_link
            .send_route_status(source_runtime.route(route).unwrap())
            .unwrap();
        let ReceivedControlMessage::RouteStatus(status) =
            sink_link.receive_control_message().unwrap()
        else {
            panic!("Pending status");
        };
        let collision_route = if collision {
            let id = host
                .runtime
                .create_route(
                    binding.session_id,
                    source,
                    sink,
                    RouteBackend::AdapterManaged,
                )
                .unwrap();
            Some(host.runtime.route(id).unwrap().clone())
        } else {
            None
        };
        let mut dispatch_error = None;
        if dispatch {
            match host.receive_bilateral_stereo_status(&mut sink_link, &status, 17) {
                Ok(value) => assert!(value.is_some()),
                Err(error) => dispatch_error = Some(error),
            }
        } else {
            assert!(
                host.receive_bilateral_stereo_status(&mut sink_link, &status, 17)
                    .unwrap()
                    .is_none()
            );
            host.receive_audio_route_status(
                &sink_link,
                &status,
                "127.0.0.1",
                ClockSample::now().unwrap().wall,
            )
            .unwrap();
            host.bind_stereo_consent_control(&sink_link, 17).unwrap();
            if stage {
                host.stage_stereo_consent(&sink_link, route).unwrap();
            }
        }
        Self {
            host,
            source: source_runtime,
            sink_link,
            source_link,
            route,
            android_identity: android_server,
            linux_identity: linux_client,
            dispatch_error,
            collision_route,
        }
    }
    fn other_same_binding(&self) -> (CatalogControlLink, CatalogControlLink) {
        let binding = self.sink_link.tls_binding();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = AuthenticatedControlServer::from_listener(
            listener,
            self.android_identity.clone(),
            binding,
            Duration::from_secs(2),
        )
        .unwrap();
        let accepting = thread::spawn(move || server.accept().unwrap());
        let dialed = AuthenticatedControlClientChannel::connect(
            ControlTlsEndpoint {
                address,
                io_timeout: Duration::from_secs(2),
            },
            "localhost",
            self.linux_identity.clone(),
            binding,
        )
        .unwrap();
        let accepted = accepting.join().unwrap();
        let mut android = NodeRuntime::new(self.host.runtime.snapshot().local_node).unwrap();
        android
            .register_peer(self.source.snapshot().local_node, true)
            .unwrap();
        let mut linux = NodeRuntime::new(self.source.snapshot().local_node).unwrap();
        linux
            .register_peer(self.host.runtime.snapshot().local_node, true)
            .unwrap();
        (
            CatalogControlLink::admit_accepted_peer(&mut android, accepted, "Linux".into())
                .unwrap(),
            CatalogControlLink::admit_dialed_peer(&mut linux, dialed, "Android".into()).unwrap(),
        )
    }
    fn wire_grant(&mut self, duration_ms: u64) -> ValidatedRouteStatus {
        let now = ClockSample::now().unwrap().wall;
        self.source
            .authorize_route(self.route, Some(now + duration_ms))
            .unwrap();
        self.source_link
            .send_route_status(self.source.route(self.route).unwrap())
            .unwrap();
        let ReceivedControlMessage::RouteStatus(status) =
            self.sink_link.receive_control_message().unwrap()
        else {
            panic!("Authorized status");
        };
        *status
    }
    fn admit(&mut self, duration_ms: u64) -> ValidatedRouteStatus {
        let status = self.wire_grant(duration_ms);
        self.host
            .receive_stereo_consent(&mut self.sink_link, &status)
            .unwrap();
        status
    }
    fn receipt(&self) -> Arc<StereoConsentReceipt> {
        let state = self.host.stereo_consent_gate.0.lock().unwrap();
        let Entry::Pending(p) = &state.entries[&self.route] else {
            panic!("Pending receipt");
        };
        p.receipt.as_ref().unwrap().clone()
    }
    fn assert_no_authority(&self) {
        let route = self.host.runtime.route(self.route).unwrap();
        assert_ne!(route.state, RouteState::Prepared);
        assert!(!matches!(
            route.authorization,
            AuthorizationState::Authorized { .. }
        ));
        assert!(
            self.host
                .runtime
                .route_port_lease_claim(self.route, route.sink)
                .is_none()
        );
        assert!(!self.host.stereo_consents.contains_key(&self.route));
    }
}

#[test]
fn dormant_real_tls_fresh_approval_requires_receipt_and_prepares_bounded_both_control_roles() {
    for accepts in [false, true] {
        let mut f = Fixture::new(accepts);
        let requests: serde_json::Value = serde_json::from_slice(
            &f.host
                .resource_requests_json(ClockSample::now().unwrap().wall)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            requests[0]["profileId"],
            crate::secure_speaker_policy::PROFILE
        );
        assert!(f.host.resolve_stereo_consent(f.route, true).is_err());
        f.assert_no_authority();
        let status = f.admit(15_000);
        f.assert_no_authority(); // the earlier click was not replayed
        let receipt = f.receipt();
        f.host.resolve_stereo_consent(f.route, true).unwrap();
        let route = f.host.runtime.route(f.route).unwrap();
        assert_eq!(route.state, RouteState::Prepared);
        assert_eq!(route.authorization, status.route.authorization);
        assert!(
            f.host.stereo_consents[&f.route].monotonic_deadline() <= receipt.monotonic_deadline()
        );
        assert!(
            f.host
                .runtime
                .route_port_lease_claim(f.route, route.sink)
                .is_none()
        );
        assert!(
            f.host
                .runtime
                .route_port_lease_claim(f.route, route.source)
                .is_none()
        );
        assert!(f.host.resolve_stereo_consent(f.route, true).is_err());
    }
}

#[test]
fn dormant_wider_peer_grant_cannot_widen_fresh_local_sixty_seconds() {
    let mut f = Fixture::new(true);
    f.admit(120_000);
    let clock = ClockSample::now().unwrap();
    f.host
        .resolve_stereo_consent_at(f.route, true, clock)
        .unwrap();
    assert_eq!(
        f.host.runtime.route(f.route).unwrap().authorization,
        AuthorizationState::Authorized {
            expires_at_ms: Some(clock.wall + CONSENT_MS)
        }
    );
    assert!(
        f.host.stereo_consents[&f.route].monotonic_deadline()
            <= clock.instant + Duration::from_millis(CONSENT_MS)
    );
}

#[test]
fn dormant_authenticated_duplicate_preserves_first_receipt_anchors() {
    let mut f = Fixture::new(true);
    f.admit(15_000);
    let first = f.receipt();
    f.source_link
        .send_route_status(f.source.route(f.route).unwrap())
        .unwrap();
    let ReceivedControlMessage::RouteStatus(status) =
        f.sink_link.receive_control_message().unwrap()
    else {
        panic!();
    };
    f.host
        .receive_stereo_consent(&mut f.sink_link, &status)
        .unwrap();
    assert!(Arc::ptr_eq(&first, &f.receipt()));
    f.host.resolve_stereo_consent(f.route, true).unwrap();
}

#[test]
fn dormant_fabricated_authorized_dto_with_genuine_link_is_not_receipt() {
    let mut f = Fixture::new(true);
    let route = f.host.runtime.route(f.route).unwrap().clone();
    let bytes = capyio_protocol::encode_route_status(route.session_id, &route).unwrap();
    let mut status = capyio_protocol::decode_route_status(&bytes).unwrap();
    status.route.authorization = AuthorizationState::Authorized {
        expires_at_ms: Some(ClockSample::now().unwrap().wall + 10_000),
    };
    assert!(
        f.host
            .receive_stereo_consent(&mut f.sink_link, &status)
            .is_err()
    );
    assert!(f.host.resolve_stereo_consent(f.route, true).is_err());
    f.assert_no_authority();
}

#[test]
fn dormant_host_clones_share_consumption_and_failed_resolution_lineage() {
    let mut f = Fixture::new(true);
    let mut cloned_before = f.host.clone();
    f.admit(15_000);
    let mut cloned_after = f.host.clone();
    f.host.resolve_stereo_consent(f.route, true).unwrap();
    assert!(cloned_before.resolve_stereo_consent(f.route, true).is_err());
    assert!(cloned_after.resolve_stereo_consent(f.route, true).is_err());
    assert_eq!(
        cloned_before.runtime.route(f.route).unwrap().authorization,
        AuthorizationState::Pending
    );
    assert_eq!(
        cloned_after.runtime.route(f.route).unwrap().authorization,
        AuthorizationState::Pending
    );
}

#[test]
fn dormant_profile_change_close_and_generation_rebind_retire_original() {
    for change in 0..3 {
        let mut f = Fixture::new(true);
        f.admit(15_000);
        match change {
            0 => {
                f.host.replace_catalog(&catalog(2)).unwrap();
            }
            1 => f.sink_link.close(),
            _ => {
                assert!(
                    f.host
                        .bind_stereo_consent_control(&f.sink_link, 18)
                        .is_err()
                );
            }
        }
        assert!(f.host.resolve_stereo_consent(f.route, true).is_err());
        f.assert_no_authority();
    }
}

#[test]
fn dormant_delay_clock_regression_and_expiry_do_not_restart_bounds() {
    for mode in 0..4 {
        let mut f = Fixture::new(true);
        f.admit(15_000);
        let receipt = f.receipt();
        let base = ClockSample::now().unwrap();
        let clock = match mode {
            0 => ClockSample {
                wall: receipt.received_wall_ms().saturating_sub(1),
                instant: base.instant,
            },
            1 => ClockSample {
                wall: base.wall,
                instant: receipt.monotonic_deadline(),
            },
            2 => ClockSample {
                wall: base.wall + CONSENT_MS,
                instant: base.instant + Duration::from_secs(60),
            },
            _ => ClockSample {
                wall: base.wall + 5_000,
                instant: base.instant + Duration::from_secs(5),
            },
        };
        let result = f.host.resolve_stereo_consent_at(f.route, true, clock);
        if mode == 3 {
            result.unwrap();
            assert!(
                f.host.stereo_consents[&f.route].monotonic_deadline()
                    <= receipt.monotonic_deadline()
            );
        } else {
            assert!(result.is_err());
            f.assert_no_authority();
        }
    }
}

#[test]
fn dormant_denial_is_available_before_and_after_peer_receipt() {
    for peer in [false, true] {
        let mut f = Fixture::new(true);
        if peer {
            f.admit(15_000);
        }
        f.host.resolve_stereo_consent(f.route, false).unwrap();
        assert_eq!(
            f.host.runtime.route(f.route).unwrap().authorization,
            AuthorizationState::Denied
        );
        assert!(f.host.resolve_stereo_consent(f.route, true).is_err());
        f.assert_no_authority();
    }
}

#[test]
fn dormant_changed_route_before_resolution_fails_without_authority() {
    let mut f = Fixture::new(true);
    f.admit(15_000);
    f.host.runtime.deny_pending_route(f.route).unwrap();
    assert!(f.host.resolve_stereo_consent(f.route, true).is_err());
    f.assert_no_authority();
}

thread_local! { static FAIL_STAGE: std::cell::Cell<u8> = const { std::cell::Cell::new(0) }; }
pub(super) fn fail_at(stage: u8) -> Result<(), String> {
    if FAIL_STAGE.with(|value| value.get() == stage) {
        return Err("injected finalization failure".into());
    }
    Ok(())
}
#[test]
fn dormant_partial_authorize_stamp_prepare_failure_is_terminal_for_all_clones() {
    for stage in 1..=3 {
        let mut f = Fixture::new(true);
        f.admit(15_000);
        let mut cloned = f.host.clone();
        FAIL_STAGE.with(|value| value.set(stage));
        let result = f.host.resolve_stereo_consent(f.route, true);
        FAIL_STAGE.with(|value| value.set(0));
        assert!(result.unwrap_err().contains("injected finalization"));
        assert_eq!(
            f.host.runtime.route(f.route).unwrap().state,
            RouteState::Failed
        );
        assert_eq!(
            f.host.runtime.route(f.route).unwrap().authorization,
            AuthorizationState::Revoked
        );
        assert!(
            f.host
                .runtime
                .recover_route(f.route, ClockSample::now().unwrap().wall)
                .is_err()
        );
        assert!(cloned.resolve_stereo_consent(f.route, true).is_err());
        assert!(f.host.resolve_stereo_consent(f.route, true).is_err());
        f.assert_no_authority();
    }
}

#[test]
fn dormant_gate_rejects_other_connection_with_identical_binding_and_certificate() {
    let mut f = Fixture::new(true);
    let (mut other, mut sender) = f.other_same_binding();
    assert_eq!(other.tls_binding(), f.sink_link.tls_binding());
    assert_eq!(
        other.authenticated_peer_certificate().unwrap(),
        f.sink_link.authenticated_peer_certificate().unwrap()
    );
    let status = f.wire_grant(15_000);
    sender.send_route_status(&status.route).unwrap();
    let ReceivedControlMessage::RouteStatus(other_status) =
        other.receive_control_message().unwrap()
    else {
        panic!();
    };
    assert!(
        f.host
            .receive_stereo_consent(&mut other, &other_status)
            .is_err()
    );
    f.assert_no_authority();
}

#[test]
fn dormant_bounded_stamp_rejects_other_instance_generation_or_closed_original() {
    for mode in 0..3 {
        let mut f = Fixture::new(true);
        f.admit(15_000);
        f.host.resolve_stereo_consent(f.route, true).unwrap();
        let stamp = f.host.stereo_consents[&f.route].clone();
        let original_end = stamp.monotonic_deadline();
        let (other, _sender) = f.other_same_binding();
        if mode == 2 {
            f.sink_link.close();
        }
        let link = if mode == 0 { &other } else { &f.sink_link };
        let admission = crate::secure_stereo_receiver::SecureStereoSinkAdmission {
            expected: f.sink_link.tls_binding(),
            route_id: f.route,
            stream_id: StreamId::new(),
            local_address: "127.0.0.1".parse().unwrap(),
            lifetime: Duration::from_secs(15),
        };
        let clock = ClockSample::now().unwrap();
        assert!(
            stamp
                .prepare(
                    &f.host.runtime,
                    link,
                    admission.clone(),
                    if mode == 1 { 18 } else { 17 },
                    clock.wall,
                    clock.instant
                )
                .is_err()
        );
        assert_eq!(
            f.host.stereo_consents[&f.route].monotonic_deadline(),
            original_end
        );
        // A failed attempt consumed the shared stamp; the genuine link cannot retry.
        assert!(
            f.host.stereo_consents[&f.route]
                .clone()
                .prepare(
                    &f.host.runtime,
                    &f.sink_link,
                    admission,
                    17,
                    clock.wall,
                    clock.instant
                )
                .is_err()
        );
    }
}

#[test]
fn dormant_clone_first_resolution_consumes_original_and_failure_retires_original() {
    for fail in [false, true] {
        let mut f = Fixture::new(true);
        f.admit(15_000);
        let mut cloned = f.host.clone();
        if fail {
            cloned.runtime.deny_pending_route(f.route).unwrap();
        }
        assert_eq!(cloned.resolve_stereo_consent(f.route, true).is_err(), fail);
        assert!(f.host.resolve_stereo_consent(f.route, true).is_err());
        f.assert_no_authority();
    }
}

#[test]
fn dormant_endpoint_catalog_metadata_and_adapter_changes_fail_closed() {
    for kind in 0..3 {
        let mut f = Fixture::new(true);
        f.admit(15_000);
        if kind == 0 {
            let json = catalog(1).replace("Android Speaker", "Changed Speaker");
            // Change exact capability metadata without changing profile selection.
            let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
            let cap = value["capabilities"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|cap| cap["class"] == "speaker")
                .unwrap();
            cap["display_name"] = serde_json::json!("Changed exact endpoint");
            f.host.replace_catalog(&value.to_string()).unwrap();
        } else {
            let mut peer = f.source.snapshot().local_node;
            let source = f.source.route(f.route).unwrap().source;
            let cap = peer.capabilities.get_mut(&source.capability_id).unwrap();
            if kind == 1 {
                cap.metadata.insert("changed".into(), "true".into());
            } else {
                peer.adapter_instances
                    .get_mut(&cap.adapter_instance_id)
                    .unwrap()
                    .state = AdapterState::Failed;
            }
            f.host.runtime.register_peer(peer, true).unwrap();
        }
        assert!(
            f.host.resolve_stereo_consent(f.route, true).is_err(),
            "kind={kind}"
        );
        f.assert_no_authority();
    }
}

#[test]
fn dormant_full_route_and_every_offer_field_are_exact_not_generic_admission() {
    use capyio_protocol::*;
    let mut f = Fixture::new(true);
    let original = f.host.runtime.route(f.route).unwrap().clone();
    let status = f.wire_grant(15_000);
    let now = ClockSample::now().unwrap().wall;
    assert!(peer_matches(&original, &status, now).is_ok());
    for mutation in 0..19 {
        let mut bad = status.clone();
        match mutation {
            0 => bad.route.id = RouteId::new(),
            1 => bad.route.session_id = SessionId::new(),
            2 => bad.session_id = SessionId::new(),
            3 => bad.route.source.port_id = PortId::new(),
            4 => bad.route.sink.capability_id = CapabilityId::new(),
            5 => bad.route.profile = ProfileId::video_frames_v1(),
            6 => bad.route.backend = RouteBackend::CapyDataPlane,
            7 => bad.route.state = RouteState::Prepared,
            8 => bad.route.epoch = 1,
            9 => {
                bad.route.selected_format = Some(FormatDescriptor::new(SECURE_AUDIO_STEREO_FORMAT))
            }
            10 => bad.route.selected_qos_mode = Some(QosMode::Interactive),
            11 => bad.route.compatible_formats[0]
                .parameters
                .insert("duration".into(), "60".into())
                .map(|_| ())
                .unwrap_or(()),
            12 => {
                bad.route.compatible_qos_modes.insert(QosMode::Basic);
            }
            13 => bad.route.diagnostic_ids.push(ProblemId::new()),
            14 => bad.route.source.node_id = NodeId::new(),
            15 => bad.route.sink.node_id = NodeId::new(),
            16 => {
                bad.route.authorization = AuthorizationState::Authorized {
                    expires_at_ms: None,
                }
            }
            17 => {
                bad.route.authorization = AuthorizationState::Authorized {
                    expires_at_ms: Some(now),
                }
            }
            _ => bad.route.authorization = AuthorizationState::Pending,
        }
        assert!(
            peer_matches(&original, &bad, now).is_err(),
            "route field={mutation}"
        );
    }
    for offer in 0..6 {
        let mut bad = status.clone();
        let stream_id = StreamId::new();
        match offer {
            0 => bad.touchpad_data = Some(TouchpadDataOffer { port: 1, stream_id }),
            1 => bad.haptics_data = Some(HapticsDataOffer { port: 1, stream_id }),
            2 => {
                bad.native_audio_data = Some(Box::new(NativeAudioDataOffer {
                    port: 1,
                    stream_id,
                    endpoint: NativeAudioEndpoint::Source,
                    preset: NativeAudioPreset::PcmS16le48000Stereo,
                }))
            }
            3 => {
                bad.camera_data = Some(CameraDataOffer {
                    port: 38173,
                    stream_id,
                })
            }
            4 => {
                bad.secure_camera_data = Some(SecureCameraDataOffer {
                    port: 1,
                    stream_id,
                    transport: SecureCameraTransport::Tls13MtlsV1,
                })
            }
            _ => {
                bad.secure_audio_data = Some(SecureAudioDataOffer {
                    port: 1,
                    stream_id,
                    transport: SecureAudioTransport::Tls13MtlsStereoV1,
                })
            }
        }
        assert!(peer_matches(&original, &bad, now).is_err(), "offer={offer}");
    }
}

#[test]
fn dormant_pending_request_anchor_is_not_restarted_by_late_staging() {
    let mut f = Fixture::with_stage(true, false);
    let original_end = f.host.touchpad_consent.pending_deadline(f.route).unwrap();
    let base = ClockSample::now().unwrap();
    let delayed = ClockSample {
        wall: base.wall + 30_000,
        instant: base.instant + Duration::from_secs(30),
    };
    f.host
        .stage_stereo_consent_at(&f.sink_link, f.route, delayed)
        .unwrap();
    let state = f.host.stereo_consent_gate.0.lock().unwrap();
    let Entry::Pending(pending) = &state.entries[&f.route] else {
        panic!();
    };
    assert_eq!(pending.request_wall_end, original_end);
    assert_eq!(
        pending.request_end,
        delayed.instant + Duration::from_millis(original_end - delayed.wall)
    );
    assert!(pending.request_end <= base.instant + Duration::from_secs(60));
}

#[test]
fn dormant_duplicate_observation_updates_wall_floor_but_never_deadline() {
    let mut f = Fixture::new(true);
    let status = f.admit(15_000);
    let first = f.receipt();
    let base = ClockSample::now().unwrap();
    let later = ClockSample {
        wall: base.wall + 5_000,
        instant: base.instant + Duration::from_secs(5),
    };
    f.host
        .receive_stereo_consent_at(&mut f.sink_link, &status, later)
        .unwrap();
    assert!(Arc::ptr_eq(&first, &f.receipt()));
    assert!(
        f.host
            .resolve_stereo_consent_at(
                f.route,
                true,
                ClockSample {
                    wall: base.wall + 4_000,
                    instant: base.instant + Duration::from_secs(6)
                }
            )
            .is_err()
    );
    f.assert_no_authority();
}

#[test]
fn dormant_request_tombstones_are_bounded_and_cannot_be_rearmed() {
    let mut f = Fixture::new(true);
    let original = f.host.runtime.route(f.route).unwrap().clone();
    f.host.resolve_stereo_consent(f.route, false).unwrap();
    assert!(f.host.stage_stereo_consent(&f.sink_link, f.route).is_err());
    for _ in 1..MAX_REQUESTS {
        let route = f
            .host
            .runtime
            .create_route(
                original.session_id,
                original.source,
                original.sink,
                original.backend,
            )
            .unwrap();
        f.host.stage_stereo_consent(&f.sink_link, route).unwrap();
        f.host.resolve_stereo_consent(route, false).unwrap();
    }
    let route = f
        .host
        .runtime
        .create_route(
            original.session_id,
            original.source,
            original.sink,
            original.backend,
        )
        .unwrap();
    assert!(
        f.host
            .stage_stereo_consent(&f.sink_link, route)
            .unwrap_err()
            .contains("bound reached")
    );
    assert_eq!(
        f.host.stereo_consent_gate.0.lock().unwrap().entries.len(),
        MAX_REQUESTS
    );
    assert!(
        f.host
            .bind_stereo_consent_control(&f.sink_link, 99)
            .is_err()
    );
    assert_eq!(
        f.host.stereo_consent_gate.0.lock().unwrap().entries.len(),
        MAX_REQUESTS
    );
}

#[test]
fn dormant_exact_duplicate_after_approval_preserves_prepared_stamp_without_rearming() {
    let mut f = Fixture::new(true);
    f.admit(15_000);
    f.host.resolve_stereo_consent(f.route, true).unwrap();
    let original = f.host.runtime.route(f.route).unwrap().clone();
    let end = f.host.stereo_consents[&f.route].monotonic_deadline();
    assert!(f.host.resolve_stereo_consent(f.route, true).is_err());
    assert!(f.host.resolve_stereo_consent(f.route, false).is_err());
    f.source_link
        .send_route_status(f.source.route(f.route).unwrap())
        .unwrap();
    let ReceivedControlMessage::RouteStatus(status) =
        f.sink_link.receive_control_message().unwrap()
    else {
        panic!();
    };
    f.host
        .receive_stereo_consent(&mut f.sink_link, &status)
        .unwrap();
    assert_eq!(f.host.runtime.route(f.route).unwrap(), &original);
    assert_eq!(f.host.stereo_consents[&f.route].monotonic_deadline(), end);
    assert!(f.host.resolve_stereo_consent(f.route, true).is_err());
}

#[test]
fn production_marked_dispatch_requires_fresh_bilateral_approval_both_tls_roles() {
    for accepts in [false, true] {
        let mut f = Fixture::with_dispatch(accepts, false, true);
        let original = f.host.runtime.route(f.route).unwrap().clone();
        assert_eq!(original.authorization, AuthorizationState::Pending);
        assert_eq!(original.state, RouteState::Draft);
        assert!(
            f.host
                .runtime
                .route_port_lease_claim(f.route, original.sink)
                .is_none()
        );
        assert!(
            f.host
                .resolve_touchpad_request(f.route, true, ClockSample::now().unwrap().wall)
                .is_err()
        );
        assert_eq!(*f.host.runtime.route(f.route).unwrap(), original);
        let status = f.wire_grant(3_000);
        f.host
            .receive_bilateral_stereo_status(&mut f.sink_link, &status, 17)
            .unwrap();
        assert_eq!(*f.host.runtime.route(f.route).unwrap(), original);
        f.host
            .resolve_touchpad_request(f.route, true, ClockSample::now().unwrap().wall)
            .unwrap();
        let prepared = f.host.runtime.route(f.route).unwrap();
        assert_eq!(prepared.state, RouteState::Prepared);
        assert_eq!(prepared.epoch, 0);
        assert_eq!(prepared.authorization, status.route.authorization);
        assert!(
            f.host
                .runtime
                .route_port_lease_claim(f.route, prepared.sink)
                .is_none()
        );
        assert!(f.host.bilateral_stereo.gate(f.route).unwrap().is_open());
        f.host
            .invalidate_bilateral_stereo_session(f.sink_link.session_id())
            .unwrap();
        assert!(!f.host.bilateral_stereo.gate(f.route).unwrap().is_open());
        assert_eq!(
            f.host.runtime.route(f.route).unwrap().state,
            RouteState::Failed
        );
        assert_eq!(
            f.host.runtime.route(f.route).unwrap().authorization,
            AuthorizationState::Revoked
        );
    }
}

#[test]
fn production_marked_dispatch_does_not_accept_forged_peer_dto_or_wrong_connection() {
    for wrong_connection in [false, true] {
        let mut f = Fixture::with_dispatch(true, false, true);
        let mut status = f.wire_grant(3_000);
        let result = if wrong_connection {
            let (mut other, _peer) = f.other_same_binding();
            f.host
                .receive_bilateral_stereo_status(&mut other, &status, 17)
        } else {
            status.route.authorization = AuthorizationState::Authorized {
                expires_at_ms: Some(ClockSample::now().unwrap().wall + 40_000),
            };
            f.host
                .receive_bilateral_stereo_status(&mut f.sink_link, &status, 17)
        };
        assert!(result.is_err());
        assert_eq!(
            f.host.runtime.route(f.route).unwrap().authorization,
            AuthorizationState::Revoked
        );
        assert!(!f.host.bilateral_stereo.gate(f.route).unwrap().is_open());
    }
}

#[test]
fn production_dispatch_unmarked_legacy_is_unchanged() {
    let f = Fixture::with_dispatch(true, false, false);
    assert!(f.host.stereo_dispatch_routes.is_empty());
    assert!(
        !f.host
            .bilateral_stereo
            .owns_session(f.sink_link.session_id())
    );
    let route = f.host.runtime.route(f.route).unwrap();
    assert_eq!(route.state, RouteState::Draft);
    assert_eq!(route.authorization, AuthorizationState::Pending);
    assert!(
        f.host
            .runtime
            .route_port_lease_claim(f.route, route.sink)
            .is_none()
    );
}

#[test]
fn production_dispatch_rejects_unknown_marker_and_non_linux_before_import() {
    for variant in 0..3 {
        let f = Fixture::with_dispatch_tamper(
            true,
            false,
            true,
            |node| {
                if variant == 2 {
                    node.platform = Platform::Windows;
                } else {
                    let cap = node.capabilities.values_mut().next().unwrap();
                    cap.metadata.insert(
                        crate::stereo_dispatch::BILATERAL_SOURCE_MARKER.into(),
                        if variant == 0 { "v2" } else { "v1,v2" }.into(),
                    );
                }
            },
            false,
        );
        assert!(f.dispatch_error.is_some());
        assert!(f.host.runtime.snapshot().routes.is_empty());
        assert!(f.host.stereo_dispatch_routes.is_empty());
        assert!(
            !f.host
                .bilateral_stereo
                .owns_session(f.sink_link.session_id())
        );
    }
}

#[test]
fn production_dispatch_dedicated_session_collision_preserves_existing_route() {
    let f = Fixture::with_dispatch_tamper(true, false, true, |_| {}, true);
    assert!(f.dispatch_error.is_some());
    let existing = f.collision_route.unwrap();
    assert_eq!(*f.host.runtime.route(existing.id).unwrap(), existing);
    assert_eq!(f.host.runtime.snapshot().routes.len(), 1);
    assert!(f.host.runtime.route(f.route).is_err());
    assert!(f.host.stereo_dispatch_routes.is_empty());
}

#[test]
fn production_dispatch_denial_is_terminal_even_before_peer_approval() {
    let mut f = Fixture::with_dispatch(true, false, true);
    f.host
        .resolve_touchpad_request(f.route, false, ClockSample::now().unwrap().wall)
        .unwrap();
    assert_eq!(
        f.host.runtime.route(f.route).unwrap().state,
        RouteState::Failed
    );
    assert_eq!(
        f.host.runtime.route(f.route).unwrap().authorization,
        AuthorizationState::Revoked
    );
    assert!(!f.host.bilateral_stereo.gate(f.route).unwrap().is_open());
}

#[test]
fn production_dispatch_pending_expiry_retires_before_request_projection() {
    let mut f = Fixture::with_dispatch(true, false, true);
    let end = f.host.touchpad_consent.pending_deadline(f.route).unwrap();
    f.host.resource_requests_json(end).unwrap();
    assert_eq!(
        f.host.runtime.route(f.route).unwrap().state,
        RouteState::Failed
    );
    assert_eq!(
        f.host.runtime.route(f.route).unwrap().authorization,
        AuthorizationState::Revoked
    );
    assert!(!f.host.bilateral_stereo.gate(f.route).unwrap().is_open());
}
