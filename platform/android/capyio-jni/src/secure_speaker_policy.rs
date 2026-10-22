//! Live Android speaker selection and narrow launch DTO. Never a UDP config.
use capyio_core::*;
use capyio_protocol::SECURE_AUDIO_STEREO_FORMAT;
use capyio_runtime::NodeRuntime;

pub(crate) const PROFILE: &str = "secure_stereo_tls13_v1";

pub(crate) fn selection_changed(
    before: Option<&CapabilityDescriptor>,
    after: Option<&CapabilityDescriptor>,
) -> bool {
    let (Some(before), Some(after)) = (before, after) else {
        return before != after;
    };
    before.metadata.get("speaker.profile") != after.metadata.get("speaker.profile")
        || before.metadata.get("speaker.selectionRevision")
            != after.metadata.get("speaker.selectionRevision")
        || before.ports.len() != after.ports.len()
        || before.ports.iter().any(|(id, port)| {
            after
                .ports
                .get(id)
                .is_none_or(|fresh| fresh.formats != port.formats)
        })
}

#[cfg(any(target_os = "android", test))]
pub(crate) fn require_ui_activation(route: &Route, local_direction: &str) -> Result<(), String> {
    if local_direction == "speaker" && crate::secure_audio_negotiation::proposed(route) {
        return Err("secure speaker acceptance is not Route Active or audibility".into());
    }
    Ok(())
}

pub(crate) fn exact_profile(route: &Route) -> bool {
    let exact = FormatDescriptor::new(SECURE_AUDIO_STEREO_FORMAT);
    route.profile == ProfileId::audio_frames_v1()
        && route.backend == RouteBackend::AdapterManaged
        && route.compatible_formats == [exact.clone()]
        && route
            .selected_format
            .as_ref()
            .is_none_or(|selected| selected == &exact)
        && route
            .selected_qos_mode
            .as_ref()
            .is_none_or(|qos| *qos == QosMode::Interactive)
}

/// A launch binds a positive revision from the exact current owned Sink. The
/// consent stamp and job snapshot retain the full descriptor, including this
/// metadata; later selection changes cannot refresh or inherit consent.
pub(crate) fn selection(runtime: &NodeRuntime, route: &Route) -> Result<u64, String> {
    if !exact_profile(route) {
        return Err("unsupported exact secure speaker profile".into());
    }
    let local = runtime.snapshot().local_node;
    let cap = local
        .capabilities
        .get(&route.sink.capability_id)
        .ok_or("secure speaker Capability absent")?;
    let port = cap
        .ports
        .get(&route.sink.port_id)
        .ok_or("secure speaker Port absent")?;
    if local.id != route.sink.node_id
        || local.platform != Platform::Android
        || cap.class != CapabilityClass::Speaker
        || port.direction != PortDirection::Sink
        || port.profile != route.profile
        || port.formats != [FormatDescriptor::new(SECURE_AUDIO_STEREO_FORMAT)]
        || port.qos_modes != std::collections::BTreeSet::from([QosMode::Interactive])
        || port.interoperability_mode != InteroperabilityMode::AdapterManaged
        || cap.metadata.get("speaker.profile").map(String::as_str) != Some(PROFILE)
    {
        return Err("secure speaker requires explicit current selection".into());
    }
    let revision = cap
        .metadata
        .get("speaker.selectionRevision")
        .ok_or("speaker selection revision absent")?;
    let value = revision
        .parse::<u64>()
        .ok()
        .filter(|n| *n > 0 && *n <= 9_007_199_254_740_991)
        .ok_or("invalid speaker selection revision")?;
    if value.to_string() != *revision {
        return Err("noncanonical speaker selection revision".into());
    }
    Ok(value)
}

#[cfg(any(target_os = "android", test))]
pub(crate) fn config(runtime: &NodeRuntime, route: &Route) -> Result<serde_json::Value, String> {
    let revision = selection(runtime, route)?;
    Ok(serde_json::json!({
        "schemaVersion":3, "transport":"pinned-tls13-stereo-v1",
        "format":SECURE_AUDIO_STEREO_FORMAT, "profileId":PROFILE,
        "selectionRevision":revision.to_string(),
        "pcmBytesPerRecord":1920, "sampleRateHz":48000,
        "channels":2, "sampleFramesPerRecord":480
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AndroidCatalogRuntimeV1;

    fn catalog(revision: u64) -> String {
        let mut json: serde_json::Value =
            serde_json::from_str(include_str!("../../fixtures/android-core-catalog-v1.json"))
                .unwrap();
        let speaker = json["capabilities"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|cap| cap["class"] == "speaker")
            .unwrap();
        speaker["speaker_selection"] = serde_json::json!({"profile":PROFILE,"revision":revision});
        speaker["ports"][0]["formats"] = serde_json::json!([SECURE_AUDIO_STEREO_FORMAT]);
        json.to_string()
    }
    fn fixture() -> (AndroidCatalogRuntimeV1, RouteId) {
        let mut host = AndroidCatalogRuntimeV1::new(&catalog(1)).unwrap();
        let local = host.runtime.snapshot().local_node;
        let cap = local
            .capabilities
            .values()
            .find(|cap| cap.class == CapabilityClass::Speaker)
            .unwrap();
        let sink = PortRef {
            node_id: local.id,
            capability_id: cap.id,
            port_id: *cap.ports.keys().next().unwrap(),
        };
        let mut peer = local.clone();
        peer.id = NodeId::new();
        peer.platform = Platform::Linux;
        let source = PortRef {
            node_id: peer.id,
            ..sink
        };
        let source_cap = peer.capabilities.get_mut(&source.capability_id).unwrap();
        source_cap.class = CapabilityClass::Custom("system_audio_capture".into());
        source_cap.ports.get_mut(&source.port_id).unwrap().direction = PortDirection::Source;
        host.runtime.register_peer(peer, true).unwrap();
        let session = host.runtime.open_session(source.node_id).unwrap();
        let route = host
            .runtime
            .create_route(session, source, sink, RouteBackend::AdapterManaged)
            .unwrap();
        (host, route)
    }
    #[test]
    fn exact_secure_speaker_dto_never_contains_udp_or_authority_material() {
        let (host, id) = fixture();
        let route = host.runtime.route(id).unwrap();
        let dto = config(&host.runtime, route).unwrap();
        assert_eq!(dto["schemaVersion"], 3);
        assert_eq!(dto["selectionRevision"], "1");
        assert_eq!(dto["channels"], 2);
        assert_eq!(dto["pcmBytesPerRecord"], 1920);
        assert_eq!(dto["sampleFramesPerRecord"], 480);
        assert_eq!(
            crate::secure_audio_negotiation::direction(route, "speaker"),
            "speaker-secure"
        );
        assert!(crate::secure_audio_negotiation::require_live_profile(route).is_ok());
        assert!(crate::secure_audio_negotiation::config(route).is_err());
        assert!(require_ui_activation(route, "speaker").is_err());
        for state in [
            RouteState::Prepared,
            RouteState::Starting,
            RouteState::Active,
        ] {
            let mut copied = route.clone();
            copied.state = state;
            assert!(require_ui_activation(&copied, "speaker").is_err());
        }
        for key in [
            "host",
            "port",
            "peerPort",
            "streamId",
            "epoch",
            "expiresAtMs",
            "certificate",
            "privateKey",
            "pcmBytesPerFrame",
        ] {
            assert!(dto.get(key).is_none(), "{key}");
        }
    }
    #[test]
    fn mono_unknown_300s_parameters_and_mixed_formats_never_substitute_for_stereo() {
        let (host, id) = fixture();
        let original = host.runtime.route(id).unwrap();
        for format in [
            "pcm-s16le-48000-stereo",
            "unknown",
            capyio_protocol::SECURE_AUDIO_MONO_FORMAT,
            capyio_protocol::SECURE_AUDIO_MONO_300S_FORMAT,
        ] {
            let mut route = original.clone();
            route.compatible_formats = vec![FormatDescriptor::new(format)];
            assert!(config(&host.runtime, &route).is_err());
        }
        let mut route = original.clone();
        route.compatible_formats[0]
            .parameters
            .insert("duration".into(), "60".into());
        assert!(!exact_profile(&route));
        let mut route = original.clone();
        route
            .compatible_formats
            .push(FormatDescriptor::new("pcm-s16le-48000-stereo"));
        assert!(!exact_profile(&route));
        let mut route = original.clone();
        route.selected_qos_mode = Some(QosMode::Basic);
        assert!(!exact_profile(&route));
    }
    #[test]
    fn visible_consent_prepares_once_and_selection_revision_retires_original() {
        let (mut host, id) = fixture();
        host.enqueue_touchpad_request(id, 100).unwrap();
        let requests: serde_json::Value =
            serde_json::from_slice(&host.resource_requests_json(100).unwrap()).unwrap();
        assert_eq!(requests[0]["profileId"], PROFILE);
        assert_eq!(requests[0]["selectionRevision"], "1");
        host.resolve_touchpad_request(id, true, 101).unwrap();
        let route = host.runtime.route(id).unwrap();
        assert_eq!(route.state, RouteState::Prepared);
        assert_eq!(
            route.authorization,
            AuthorizationState::Authorized {
                expires_at_ms: Some(60_101)
            }
        );
        assert!(host.stereo_consents.contains_key(&id));
        assert!(
            host.runtime
                .route_port_lease_claim(id, route.sink)
                .is_none()
        );
        assert!(host.resolve_touchpad_request(id, true, 102).is_err());
        host.replace_catalog(&catalog(2)).unwrap();
        assert_eq!(host.runtime.route(id).unwrap().state, RouteState::Offline);
        assert!(!host.stereo_consents.contains_key(&id));
        assert_eq!(host.publication_revision(), 1);
    }
    #[test]
    fn changed_selection_denies_pending_request_before_republication() {
        let (mut host, id) = fixture();
        host.enqueue_touchpad_request(id, 100).unwrap();
        host.replace_catalog(&catalog(2)).unwrap();
        assert_eq!(
            host.runtime.route(id).unwrap().authorization,
            AuthorizationState::Denied
        );
        assert!(host.resolve_touchpad_request(id, true, 101).is_err());
    }
    #[test]
    fn default_legacy_catalog_remains_legacy_and_missing_opt_in_is_rejected() {
        let legacy = include_str!("../../fixtures/android-core-catalog-v1.json");
        let host = AndroidCatalogRuntimeV1::new(legacy).unwrap();
        let local = host.runtime.snapshot().local_node;
        let cap = local
            .capabilities
            .values()
            .find(|cap| cap.class == CapabilityClass::Speaker)
            .unwrap();
        assert_eq!(
            cap.ports.values().next().unwrap().formats,
            [FormatDescriptor::new("pcm-s16le-48000-stereo")]
        );
        assert!(
            AndroidCatalogRuntimeV1::new(
                &legacy.replace("pcm-s16le-48000-stereo", SECURE_AUDIO_STEREO_FORMAT)
            )
            .is_err()
        );
    }
}
