//! One-shot read-only inspection of exactly one authenticated peer catalog.
//!
//! This private FIFO entry publishes an empty local catalog. Pairing is an
//! identity prerequisite, never resource consent. It creates no local Adapter,
//! Capability, Port, Route, grant, lease, media endpoint, or helper. Display
//! names and arbitrary metadata are deliberately omitted from the projection.

use crate::{
    launch::{CredentialSource, PairedStoreSelection, launcher_credentials},
    launcher_io::LauncherIo,
};
use capyio_control_link::{CatalogControlLink, ReceivedControlMessage};
use capyio_control_tls::{
    AuthenticatedControlClientChannel, ControlTlsClientIdentity, ControlTlsEndpoint,
    ControlTlsNodeBinding,
};
use capyio_core::*;
use capyio_runtime::NodeRuntime;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    io,
    net::SocketAddr,
    time::{Duration, Instant},
};

const LOCAL_FRAME: usize = 4096;
const OPERATION_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_PORTS: usize = 64;
const MAX_EVENT_BYTES: usize = 4096;
const MAX_OUTPUT_BYTES: usize = 65_536;
const MAX_FORMATS: usize = 16;
const MAX_PARAMETERS: usize = 16;
const MAX_TEXT_BYTES: usize = 128;
const MAX_ESCAPED_TEXT_BYTES: usize = 512;

fn fail() -> io::Error {
    io::Error::other("Linux catalog inspection failed")
}

fn sanitized<T>(result: Result<T, impl std::fmt::Display>) -> io::Result<T> {
    result.map_err(|_| fail())
}

fn check(deadline: Instant) -> io::Result<()> {
    if Instant::now() >= deadline {
        Err(fail())
    } else {
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InspectConfig {
    version: u32,
    local_node_id: NodeId,
    peer_node_id: NodeId,
    endpoint: SocketAddr,
}

impl InspectConfig {
    fn parse(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() > LOCAL_FRAME
            || bytes.iter().find(|byte| !byte.is_ascii_whitespace()) != Some(&b'{')
        {
            return Err(fail());
        }
        let value: Self = sanitized(serde_json::from_slice(bytes))?;
        let SocketAddr::V4(endpoint) = value.endpoint else {
            return Err(fail());
        };
        let ip = endpoint.ip();
        if value.version != 1
            || value.local_node_id.as_uuid().is_nil()
            || value.peer_node_id.as_uuid().is_nil()
            || value.local_node_id == value.peer_node_id
            || endpoint.port() == 0
            || !(ip.is_loopback() || ip.is_private() || ip.is_link_local())
        {
            return Err(fail());
        }
        Ok(value)
    }
}

// The endpoint never performs DNS resolution. This name comes exclusively from
// the selected paired record and is only the bounded TLS certificate identity.
fn validate_server_name(name: &str) -> io::Result<()> {
    if name.is_empty()
        || name.len() > 253
        || name.parse::<std::net::IpAddr>().is_ok()
        || !name.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_alphanumeric)
                && label
                    .as_bytes()
                    .last()
                    .is_some_and(u8::is_ascii_alphanumeric)
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
    {
        return Err(fail());
    }
    Ok(())
}

/// Requires exclusive trusted inherited FIFO stdin/stdout, the explicit paired
/// store selectors, and a second-frame paired-store marker. No legacy private
/// credential frame, automatic discovery, enrollment, or store write is used.
/// Errors are fixed text, and the complete projected catalog is checked before
/// any catalog event is emitted. Consumers must accept only after complete and
/// successful process exit, because output-pipe failure can interrupt delivery.
pub fn run_catalog_inspect_stdio(selection: PairedStoreSelection) -> io::Result<()> {
    // One budget includes local bootstrap, protected loading, TLS, catalog I/O,
    // validation and output. In particular, a failed operation gets no new timer.
    let deadline = Instant::now() + OPERATION_TIMEOUT;
    let mut io = sanitized(LauncherIo::stdio())?;
    let result = inspect(&mut io, selection, deadline);
    if result.is_err() {
        let _ = io.send_json(&json!({"version":1,"event":"failed"}), deadline);
    }
    result.map_err(|_| fail())
}

// There is no resource command in this protocol. Any third frame, partial
// command, closed input, or malformed input cancels the inspection. Check this
// before network access and between bounded stages. An in-progress TLS/catalog
// read is bounded by the same absolute operation deadline, not a new phase timer.
fn require_open_idle_input(io: &mut LauncherIo, deadline: Instant) -> io::Result<()> {
    check(deadline)?;
    if sanitized(io.poll_frame(LOCAL_FRAME))?.is_some() || io.has_pending_frame() {
        return Err(fail());
    }
    check(deadline)
}

fn inspect(
    io: &mut LauncherIo,
    selection: PairedStoreSelection,
    deadline: Instant,
) -> io::Result<()> {
    let bytes = sanitized(io.read_frame(LOCAL_FRAME, deadline))?;
    let config = InspectConfig::parse(bytes.as_slice())?;
    drop(bytes);
    let (directory, credentials) = sanitized(launcher_credentials(
        io,
        config.local_node_id,
        config.peer_node_id,
        deadline,
        CredentialSource::PairedStore(selection),
    ))?;
    let peer = directory.active_peers().next().ok_or_else(fail)?;
    if peer.node_id != config.peer_node_id {
        return Err(fail());
    }
    validate_server_name(&peer.server_name)?;
    require_open_idle_input(io, deadline)?;
    let mut runtime = empty_runtime(config.local_node_id)?;
    let identity = ControlTlsClientIdentity::from_bundle(peer.peer_certificate_sha256, credentials);
    let nodes = sanitized(ControlTlsNodeBinding::new(
        config.peer_node_id,
        config.local_node_id,
    ))?;
    let channel = sanitized(
        AuthenticatedControlClientChannel::connect_fresh_session_until(
            ControlTlsEndpoint {
                address: config.endpoint,
                io_timeout: OPERATION_TIMEOUT,
            },
            &peer.server_name,
            identity,
            nodes,
            deadline,
        ),
    )?;
    let mut link = sanitized(CatalogControlLink::admit_dialed_new_peer(
        &mut runtime,
        channel,
        peer.display_name.clone(),
    ))?;
    let projected = exchange(io, &mut runtime, &mut link, config.peer_node_id, deadline);
    // Cleanup happens for every admitted-session outcome, before output. Even a
    // malformed/oversized/unexpected remote message cannot leave a Ready Session.
    let closed = sanitized(link.close_and_reconcile(&mut runtime));
    let events = projected?;
    closed?;
    require_open_idle_input(io, deadline)?;
    for event in events {
        sanitized(io.send_json(&event, deadline))?;
    }
    check(deadline)
}

fn empty_runtime(local_node_id: NodeId) -> io::Result<NodeRuntime> {
    sanitized(NodeRuntime::new(NodeDescriptor::new(
        local_node_id,
        "Linux read-only catalog inspection",
        Platform::Linux,
        "not probed",
        env!("CARGO_PKG_VERSION"),
        [ProtocolVersion::new(1, 1)],
    )))
}

fn exchange(
    io: &mut LauncherIo,
    runtime: &mut NodeRuntime,
    link: &mut CatalogControlLink,
    peer_node_id: NodeId,
    deadline: Instant,
) -> io::Result<Vec<Value>> {
    require_open_idle_input(io, deadline)?;
    let catalog = sanitized(link.prepare_catalog(runtime, &BTreeSet::new()))?;
    sanitized(link.send_catalog_until(catalog, deadline))?;
    require_open_idle_input(io, deadline)?;
    // Exactly one authenticated message is consumed. Heartbeats, Routes, lease
    // requests, endpoint offers and every other payload fail closed, not handled.
    let ReceivedControlMessage::Catalog(catalog) =
        sanitized(link.receive_control_message_until(deadline))?
    else {
        return Err(fail());
    };
    require_open_idle_input(io, deadline)?;
    sanitized(link.apply_catalog(runtime, catalog))?;
    check(deadline)?;
    let snapshot = runtime.snapshot();
    if snapshot.peers.len() != 1
        || !snapshot.routes.is_empty()
        || !snapshot.port_leases.is_empty()
        || !snapshot.local_node.capabilities.is_empty()
        || !snapshot.local_node.adapter_instances.is_empty()
    {
        return Err(fail());
    }
    let peer = snapshot.peers.first().ok_or_else(fail)?;
    let events = project_catalog(peer, peer_node_id)?;
    check(deadline)?;
    Ok(events)
}

// Printable ASCII preserves identifiers unambiguously as escaped display text,
// not terminal control. Even literal backslashes are escaped to avoid ambiguity.
// Consumers must not interpret these strings as executable paths or commands.
fn display_text(value: &str) -> io::Result<String> {
    if value.len() > MAX_TEXT_BYTES {
        return Err(fail());
    }
    let escaped: String = value.chars().flat_map(char::escape_default).collect();
    if escaped.len() > MAX_ESCAPED_TEXT_BYTES {
        return Err(fail());
    }
    Ok(escaped)
}

fn project_catalog(peer: &NodeDescriptor, expected: NodeId) -> io::Result<Vec<Value>> {
    if peer.id != expected || peer.id.as_uuid().is_nil() {
        return Err(fail());
    }
    let mut port_count = 0_usize;
    for capability in peer.capabilities.values() {
        port_count = port_count
            .checked_add(capability.ports.len())
            .ok_or_else(fail)?;
        if port_count > MAX_PORTS {
            return Err(fail());
        }
    }
    sanitized(peer.validate())?;
    let mut events = Vec::with_capacity(port_count + 2);
    let mut output_bytes = 0;
    push_event(
        &mut events,
        &mut output_bytes,
        json!({
            "version":1,"event":"catalog_begin","peer_node_id":peer.id,
            "port_count":port_count,"text_encoding":"ascii_escaped",
        }),
    )?;
    for capability in peer.capabilities.values() {
        if capability.id.as_uuid().is_nil() {
            return Err(fail());
        }
        for port in capability.ports.values() {
            if port.id.as_uuid().is_nil() || port.formats.len() > MAX_FORMATS {
                return Err(fail());
            }
            let mut formats = Vec::with_capacity(port.formats.len());
            for format in &port.formats {
                if format.parameters.len() > MAX_PARAMETERS {
                    return Err(fail());
                }
                let mut parameters = serde_json::Map::new();
                for (key, value) in &format.parameters {
                    parameters.insert(display_text(key)?, Value::String(display_text(value)?));
                }
                formats.push(json!({"id":display_text(&format.id)?,"parameters":parameters}));
            }
            let (status, reason) = launcher_hint(peer, capability, port);
            push_event(
                &mut events,
                &mut output_bytes,
                json!({
                    "version":1,"event":"catalog_port","peer_node_id":peer.id,
                    "capability_id":capability.id,"port_id":port.id,
                    "direction":port.direction,
                    "profile":{"name":display_text(&port.profile.name)?,"major":port.profile.major},
                    "formats":formats,
                    "capability_availability":capability.availability,
                    "port_availability":port.availability,
                    "compatibility":{"status":status,"reason":reason},
                }),
            )?;
        }
    }
    push_event(
        &mut events,
        &mut output_bytes,
        json!({
            "version":1,"event":"catalog_complete","peer_node_id":peer.id,"port_count":port_count,
        }),
    )?;
    Ok(events)
}

// This is only a conservative metadata hint for the existing Linux launchers,
// not generic Route compatibility. No local counterpart was selected or probed;
// no backend contract, resource authority, or live lease has been established.
fn launcher_hint(
    peer: &NodeDescriptor,
    capability: &CapabilityDescriptor,
    port: &PortDescriptor,
) -> (&'static str, &'static str) {
    let unsupported = |reason| ("not_supported", reason);
    if capability.availability != Availability::Available
        || port.availability != Availability::Available
    {
        return unsupported("resource_unavailable");
    }
    if port.direction == PortDirection::Control {
        return unsupported("direction_not_supported");
    }
    if port.interoperability_mode != InteroperabilityMode::AdapterManaged {
        return unsupported("interoperability_not_supported");
    }
    if !peer
        .adapter_instances
        .get(&capability.adapter_instance_id)
        .is_some_and(|adapter| {
            adapter
                .supported_route_modes
                .contains(&RouteBackend::AdapterManaged)
        })
    {
        return unsupported("backend_not_supported");
    }
    if !port.qos_modes.contains(&QosMode::Interactive) {
        return unsupported("qos_not_supported");
    }
    let (format, reason) = if port.profile == ProfileId::audio_frames_v1() {
        match port.direction {
            PortDirection::Source => (
                "pcm-s16le-48000-mono",
                "audio_playback_needs_local_selection",
            ),
            PortDirection::Sink => (
                "pcm-s16le-48000-stereo",
                "audio_capture_needs_local_selection",
            ),
            PortDirection::Control => return unsupported("direction_not_supported"),
        }
    } else if port.profile == ProfileId::video_frames_v1() {
        if port.direction != PortDirection::Source {
            return unsupported("direction_not_supported");
        }
        if peer.platform != Platform::Android || capability.class != CapabilityClass::Camera {
            return unsupported("platform_or_class_not_supported");
        }
        ("cavc-avc-access-unit-v1", "camera_needs_local_selection")
    } else if port.profile == ProfileId::touchpad_frames_v1() {
        if port.direction != PortDirection::Source {
            return unsupported("direction_not_supported");
        }
        if peer.platform != Platform::Android || capability.class != CapabilityClass::Touchpad {
            return unsupported("platform_or_class_not_supported");
        }
        ("touchpad-frame-v1", "touchpad_needs_local_selection")
    } else {
        return unsupported("profile_not_supported");
    };
    if !port.formats.contains(&FormatDescriptor::new(format))
        && !(port.profile == ProfileId::audio_frames_v1()
            && port.direction == PortDirection::Source
            && matches!(port.formats.as_slice(), [format] if capyio_protocol::SecureAudioSessionProfile::from_descriptor(format).is_some())
            && port.qos_modes.len() == 1)
    {
        return unsupported("format_not_supported");
    }
    ("needs_local_selection", reason)
}

// Serialize every event through fixed storage before emitting catalog_begin.
// LauncherIo serializes the immutable validated Values again for private FIFO
// framing; no unchecked later event can turn a rejected catalog into a prefix.
fn event_length(event: &Value) -> io::Result<usize> {
    let mut bytes = [0_u8; MAX_EVENT_BYTES];
    let mut writer = io::Cursor::new(bytes.as_mut_slice());
    sanitized(serde_json::to_writer(&mut writer, event))?;
    sanitized(usize::try_from(writer.position()))
}

fn push_event(events: &mut Vec<Value>, total: &mut usize, event: Value) -> io::Result<()> {
    if events.len() >= MAX_PORTS + 2 {
        return Err(fail());
    }
    let length = event_length(&event)?;
    let next = total.checked_add(length + 4).ok_or_else(fail)?;
    if next > MAX_OUTPUT_BYTES {
        return Err(fail());
    }
    *total = next;
    events.push(event);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn validate_output(events: &[Value]) -> io::Result<()> {
        let mut projected = Vec::new();
        let mut total = 0;
        for event in events {
            push_event(&mut projected, &mut total, event.clone())?;
        }
        Ok(())
    }

    fn config() -> Value {
        json!({
            "version":1,"local_node_id":NodeId::new(),"peer_node_id":NodeId::new(),
            "endpoint":"127.0.0.1:42100",
        })
    }

    fn parse(value: &Value) -> io::Result<InspectConfig> {
        InspectConfig::parse(&serde_json::to_vec(value).unwrap())
    }

    fn peer(port_count: usize) -> NodeDescriptor {
        let mut peer = NodeDescriptor::new(
            NodeId::new(),
            "untrusted peer name",
            Platform::Android,
            "fixture",
            "fixture",
            [ProtocolVersion::new(1, 1)],
        );
        let adapter = AdapterInstanceId::new();
        peer.add_adapter(AdapterInstanceDescriptor {
            id: adapter,
            adapter_type: "fixture.catalog".into(),
            display_name: "untrusted adapter name".into(),
            deployment_mode: AdapterDeploymentMode::InProcess,
            version: "fixture".into(),
            state: AdapterState::Ready,
            health: AdapterHealth::Healthy,
            owned_capabilities: BTreeSet::new(),
            supported_route_modes: BTreeSet::from([RouteBackend::AdapterManaged]),
        })
        .unwrap();
        for _ in 0..port_count {
            let capability_id = CapabilityId::new();
            let port_id = PortId::new();
            peer.add_capability(CapabilityDescriptor {
                id: capability_id,
                adapter_instance_id: adapter,
                display_name: "untrusted capability name".into(),
                class: CapabilityClass::Microphone,
                availability: Availability::Available,
                permission_requirement: PermissionRequirement::UserConfirmation,
                metadata: BTreeMap::from([("private metadata".into(), "SECRET-METADATA".into())]),
                ports: BTreeMap::from([(
                    port_id,
                    PortDescriptor {
                        id: port_id,
                        capability_id,
                        display_name: "untrusted port name".into(),
                        direction: PortDirection::Source,
                        profile: ProfileId::audio_frames_v1(),
                        schema_id: Some("SECRET-SCHEMA".into()),
                        formats: vec![FormatDescriptor::new("pcm-s16le-48000-mono")],
                        qos_modes: BTreeSet::from([QosMode::Interactive]),
                        clock_domain: Some("SECRET-CLOCK".into()),
                        availability: Availability::Available,
                        permission_requirement: PermissionRequirement::UserConfirmation,
                        interoperability_mode: InteroperabilityMode::AdapterManaged,
                    },
                )]),
            })
            .unwrap();
        }
        peer
    }

    #[test]
    fn schema_accepts_only_explicit_local_ipv4_endpoints_and_distinct_nodes() {
        for endpoint in [
            "127.0.0.1:1",
            "10.1.2.3:65535",
            "172.16.0.1:100",
            "192.168.1.2:90",
            "169.254.1.2:80",
        ] {
            let mut value = config();
            value["endpoint"] = json!(endpoint);
            assert!(parse(&value).is_ok(), "{endpoint}");
        }
        for endpoint in [
            "0.0.0.0:90",
            "127.0.0.1:0",
            "224.0.0.1:90",
            "255.255.255.255:90",
            "8.8.8.8:90",
            "100.64.0.1:90",
            "[::1]:90",
            "[::ffff:127.0.0.1]:90",
            "localhost:90",
        ] {
            let mut value = config();
            value["endpoint"] = json!(endpoint);
            assert!(parse(&value).is_err(), "{endpoint}");
        }
        let mut value = config();
        value["peer_node_id"] = value["local_node_id"].clone();
        assert!(parse(&value).is_err());
        for field in ["peer_node_id", "local_node_id"] {
            let mut value = config();
            value[field] = json!("00000000-0000-0000-0000-000000000000");
            assert!(parse(&value).is_err());
        }
    }

    #[test]
    fn schema_rejects_unknown_fields_wrong_versions_arrays_and_secret_input() {
        for field in [
            "server_name",
            "credential_source",
            "private_key_der",
            "route_id",
            "source",
            "target",
            "listen",
        ] {
            let mut value = config();
            value[field] = json!("SECRET-INPUT");
            let error = parse(&value).err().unwrap();
            assert_eq!(error.to_string(), "Linux catalog inspection failed");
        }
        for version in [json!(0), json!(2), json!("1"), json!(1.5)] {
            let mut value = config();
            value["version"] = version;
            assert!(parse(&value).is_err());
        }
        let value = config();
        assert!(
            InspectConfig::parse(
                &serde_json::to_vec(&json!([
                    1,
                    value["local_node_id"],
                    value["peer_node_id"],
                    value["endpoint"]
                ]))
                .unwrap()
            )
            .is_err()
        );
        let mut encoded = serde_json::to_string(&value).unwrap();
        encoded.insert_str(1, "\"version\":1,");
        assert!(InspectConfig::parse(encoded.as_bytes()).is_err());
        assert!(InspectConfig::parse(&vec![b' '; LOCAL_FRAME + 1]).is_err());
    }

    #[test]
    fn stored_tls_name_is_dns_only_bounded_and_never_endpoint_resolution() {
        for name in ["localhost", "paired-node.example", "NODE-1.local"] {
            assert!(validate_server_name(name).is_ok());
        }
        for name in [
            "",
            "127.0.0.1",
            "::1",
            "-peer.local",
            "peer-.local",
            "peer..local",
            "peer.local.",
            "peer_name",
            "peer\n.local",
            "péér.local",
            "https://peer.local",
        ] {
            assert!(validate_server_name(name).is_err(), "{name}");
        }
        assert!(validate_server_name(&format!("{}.local", "x".repeat(64))).is_err());
        assert!(validate_server_name(&"x.".repeat(128)).is_err());
    }

    #[test]
    fn empty_runtime_has_no_resource_authority_and_empty_catalog_is_valid() {
        let runtime = empty_runtime(NodeId::new()).unwrap();
        let snapshot = runtime.snapshot();
        assert!(snapshot.local_node.adapter_instances.is_empty());
        assert!(snapshot.local_node.capabilities.is_empty());
        assert!(snapshot.sessions.is_empty());
        assert!(snapshot.routes.is_empty());
        assert!(snapshot.port_leases.is_empty());
        let peer = peer(0);
        let events = project_catalog(&peer, peer.id).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["port_count"], 0);
        assert_eq!(events[1]["event"], "catalog_complete");
    }

    #[test]
    fn projection_preserves_real_ids_and_omits_names_and_arbitrary_metadata() {
        let mut peer = peer(1);
        peer.display_name = "\u{1b}[31mSECRET-PEER\n".into();
        let capability = peer.capabilities.values_mut().next().unwrap();
        capability.display_name = "SECRET-CAPABILITY".into();
        let port = capability.ports.values_mut().next().unwrap();
        port.display_name = "SECRET-PORT".into();
        port.formats.push(FormatDescriptor {
            id: "custom\u{1b}[2J\n中文".into(),
            parameters: BTreeMap::from([("param\r".into(), "value\t\\quoted\"".into())]),
        });
        let expected = (capability.id, port.id);
        let events = project_catalog(&peer, peer.id).unwrap();
        assert_eq!(events[0]["text_encoding"], "ascii_escaped");
        assert_eq!(events[1]["peer_node_id"], json!(peer.id));
        assert_eq!(events[1]["capability_id"], json!(expected.0));
        assert_eq!(events[1]["port_id"], json!(expected.1));
        assert_eq!(events[1]["direction"], "source");
        assert_eq!(
            events[1]["compatibility"]["status"],
            "needs_local_selection"
        );
        let text = serde_json::to_string(&events).unwrap();
        assert!(text.is_ascii());
        assert!(!text.contains("SECRET"));
        assert!(!text.contains("untrusted"));
        for format in events[1]["formats"].as_array().unwrap() {
            assert!(
                format["id"]
                    .as_str()
                    .unwrap()
                    .bytes()
                    .all(|byte| (32..=126).contains(&byte))
            );
        }
        assert_ne!(display_text("\\n").unwrap(), display_text("\n").unwrap());
    }

    #[test]
    fn secure_audio_source_hint_requires_exact_profile_without_legacy_format() {
        for profile in [
            capyio_protocol::SecureAudioSessionProfile::Finite60sV1,
            capyio_protocol::SecureAudioSessionProfile::Finite300sV1,
        ] {
            let mut peer = peer(1);
            let capability_id = *peer.capabilities.keys().next().unwrap();
            let capability = peer.capabilities.get_mut(&capability_id).unwrap();
            let port = capability.ports.values_mut().next().unwrap();
            port.formats = vec![profile.descriptor()];
            let capability = &peer.capabilities[&capability_id];
            assert_eq!(
                launcher_hint(&peer, capability, capability.ports.values().next().unwrap()),
                (
                    "needs_local_selection",
                    "audio_playback_needs_local_selection"
                )
            );
            let capability = peer.capabilities.get_mut(&capability_id).unwrap();
            capability.ports.values_mut().next().unwrap().formats[0]
                .parameters
                .insert("unknown".into(), "required".into());
            let capability = &peer.capabilities[&capability_id];
            assert_eq!(
                launcher_hint(&peer, capability, capability.ports.values().next().unwrap()),
                ("not_supported", "format_not_supported")
            );
        }
    }

    #[test]
    fn hints_are_conservative_and_require_exact_format_semantics() {
        let mut peer = peer(1);
        let capability_id = *peer.capabilities.keys().next().unwrap();
        let hint = |peer: &NodeDescriptor| {
            let capability = &peer.capabilities[&capability_id];
            launcher_hint(peer, capability, capability.ports.values().next().unwrap())
        };
        assert_eq!(
            hint(&peer),
            (
                "needs_local_selection",
                "audio_playback_needs_local_selection"
            )
        );
        let port = peer
            .capabilities
            .get_mut(&capability_id)
            .unwrap()
            .ports
            .values_mut()
            .next()
            .unwrap();
        port.formats[0]
            .parameters
            .insert("unrecognized".into(), "required".into());
        assert_eq!(hint(&peer), ("not_supported", "format_not_supported"));
        let capability = peer.capabilities.get_mut(&capability_id).unwrap();
        capability.availability = Availability::Busy;
        assert_eq!(hint(&peer), ("not_supported", "resource_unavailable"));
        let capability = peer.capabilities.get_mut(&capability_id).unwrap();
        capability.availability = Availability::Available;
        capability.ports.values_mut().next().unwrap().direction = PortDirection::Control;
        assert_eq!(hint(&peer), ("not_supported", "direction_not_supported"));
        let capability = peer.capabilities.get_mut(&capability_id).unwrap();
        let port = capability.ports.values_mut().next().unwrap();
        port.direction = PortDirection::Sink;
        port.formats = vec![FormatDescriptor::new("pcm-s16le-48000-stereo")];
        assert_eq!(
            hint(&peer),
            (
                "needs_local_selection",
                "audio_capture_needs_local_selection"
            )
        );
    }

    #[test]
    fn camera_touchpad_hints_require_exact_platform_profile_and_backend_metadata() {
        for (class, profile, format, reason) in [
            (
                CapabilityClass::Camera,
                ProfileId::video_frames_v1(),
                "cavc-avc-access-unit-v1",
                "camera_needs_local_selection",
            ),
            (
                CapabilityClass::Touchpad,
                ProfileId::touchpad_frames_v1(),
                "touchpad-frame-v1",
                "touchpad_needs_local_selection",
            ),
        ] {
            let mut peer = peer(1);
            let capability = peer.capabilities.values_mut().next().unwrap();
            capability.class = class;
            let port = capability.ports.values_mut().next().unwrap();
            port.profile = profile.clone();
            port.formats = vec![FormatDescriptor::new(format)];
            let hint = |peer: &NodeDescriptor| {
                let capability = peer.capabilities.values().next().unwrap();
                launcher_hint(peer, capability, capability.ports.values().next().unwrap())
            };
            assert_eq!(hint(&peer), ("needs_local_selection", reason));
            peer.platform = Platform::Linux;
            assert_eq!(
                hint(&peer),
                ("not_supported", "platform_or_class_not_supported")
            );
            peer.platform = Platform::Android;
            let capability = peer.capabilities.values_mut().next().unwrap();
            capability.ports.values_mut().next().unwrap().profile.major = 2;
            assert_eq!(hint(&peer), ("not_supported", "profile_not_supported"));
            let capability = peer.capabilities.values_mut().next().unwrap();
            let port = capability.ports.values_mut().next().unwrap();
            port.profile = profile;
            port.qos_modes = BTreeSet::from([QosMode::Basic]);
            assert_eq!(hint(&peer), ("not_supported", "qos_not_supported"));
            let capability = peer.capabilities.values_mut().next().unwrap();
            let port = capability.ports.values_mut().next().unwrap();
            port.qos_modes = BTreeSet::from([QosMode::Interactive]);
            port.interoperability_mode = InteroperabilityMode::StandardPort;
            assert_eq!(
                hint(&peer),
                ("not_supported", "interoperability_not_supported")
            );
            let capability = peer.capabilities.values_mut().next().unwrap();
            capability
                .ports
                .values_mut()
                .next()
                .unwrap()
                .interoperability_mode = InteroperabilityMode::AdapterManaged;
            peer.adapter_instances
                .values_mut()
                .next()
                .unwrap()
                .supported_route_modes = BTreeSet::from([RouteBackend::CapyDataPlane]);
            assert_eq!(hint(&peer), ("not_supported", "backend_not_supported"));
        }
    }

    #[test]
    fn projection_rejects_wrong_peer_and_all_oversize_catalogs_without_partial_output() {
        let peer = peer(1);
        assert!(project_catalog(&peer, NodeId::new()).is_err());
        let mut oversized = self::peer(MAX_PORTS + 1);
        assert!(project_catalog(&oversized, oversized.id).is_err());
        oversized = self::peer(MAX_PORTS);
        assert!(project_catalog(&oversized, oversized.id).is_ok());
        let mut oversized = self::peer(1);
        let port = oversized
            .capabilities
            .values_mut()
            .next()
            .unwrap()
            .ports
            .values_mut()
            .next()
            .unwrap();
        port.formats = vec![FormatDescriptor::new("f"); MAX_FORMATS + 1];
        assert!(project_catalog(&oversized, oversized.id).is_err());
        let port = oversized
            .capabilities
            .values_mut()
            .next()
            .unwrap()
            .ports
            .values_mut()
            .next()
            .unwrap();
        port.formats = vec![FormatDescriptor::new("x".repeat(MAX_TEXT_BYTES + 1))];
        assert!(project_catalog(&oversized, oversized.id).is_err());
        let port = oversized
            .capabilities
            .values_mut()
            .next()
            .unwrap()
            .ports
            .values_mut()
            .next()
            .unwrap();
        port.formats = vec![FormatDescriptor {
            id: "format".into(),
            parameters: (0..=MAX_PARAMETERS)
                .map(|index| (format!("p{index}"), "v".into()))
                .collect(),
        }];
        assert!(project_catalog(&oversized, oversized.id).is_err());
    }

    #[test]
    fn output_serialization_checks_per_event_and_aggregate_framed_bounds() {
        let exact = json!("x".repeat(MAX_EVENT_BYTES - 2));
        assert!(validate_output(std::slice::from_ref(&exact)).is_ok());
        assert!(validate_output(&[json!("x".repeat(MAX_EVENT_BYTES - 1))]).is_err());
        assert!(validate_output(&vec![exact; 16]).is_err());
        assert!(validate_output(&vec![json!(0); MAX_PORTS + 3]).is_err());
        assert!(display_text(&"\n".repeat(MAX_TEXT_BYTES)).is_ok());
        assert!(display_text(&"\0".repeat(MAX_TEXT_BYTES)).is_err());
        assert!(display_text(&"\u{7f}".repeat(MAX_TEXT_BYTES)).is_err());
    }
}
