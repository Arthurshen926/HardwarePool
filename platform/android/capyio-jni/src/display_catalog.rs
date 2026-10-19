use capyio_core::*;
use std::collections::{BTreeMap, BTreeSet};
pub(crate) const SINK_CAP: &str = "7d5ad480-23d8-4aac-9ea9-8e18672a7211";
pub(crate) const SINK_PORT: &str = "7d5ad480-23d8-4aac-9ea9-8e18672a7212";
#[cfg(target_os = "android")]
pub(crate) const SOURCE_CAP: &str = "7d5ad480-23d8-4aac-9ea9-8e18672a7221";
pub(crate) fn install(node: &mut NodeDescriptor) {
    let adapter: AdapterInstanceId = "7d5ad480-23d8-4aac-9ea9-8e18672a7210".parse().unwrap();
    let id: CapabilityId = SINK_CAP.parse().unwrap();
    let port: PortId = SINK_PORT.parse().unwrap();
    node.adapter_instances.insert(
        adapter,
        AdapterInstanceDescriptor {
            id: adapter,
            adapter_type: "dev.capyio.android.display-receiver".into(),
            display_name: "Android display receiver".into(),
            deployment_mode: AdapterDeploymentMode::InProcess,
            version: "1".into(),
            state: AdapterState::Ready,
            health: AdapterHealth::Unknown,
            owned_capabilities: BTreeSet::from([id]),
            supported_route_modes: BTreeSet::from([RouteBackend::AdapterManaged]),
        },
    );
    node.capabilities.insert(
        id,
        CapabilityDescriptor {
            id,
            adapter_instance_id: adapter,
            display_name: "平板扩展屏".into(),
            class: CapabilityClass::Display,
            availability: Availability::Available,
            permission_requirement: PermissionRequirement::UserConfirmation,
            metadata: BTreeMap::new(),
            ports: BTreeMap::from([(
                port,
                PortDescriptor {
                    id: port,
                    capability_id: id,
                    display_name: "扩展显示接收".into(),
                    direction: PortDirection::Sink,
                    profile: ProfileId::video_frames_v1(),
                    schema_id: None,
                    formats: vec![FormatDescriptor::new("gamestream-h264-sdr-v1")],
                    qos_modes: BTreeSet::from([QosMode::Interactive]),
                    clock_domain: None,
                    availability: Availability::Available,
                    permission_requirement: PermissionRequirement::UserConfirmation,
                    interoperability_mode: InteroperabilityMode::AdapterManaged,
                },
            )]),
        },
    );
}

#[derive(Clone, Default)]
pub(crate) struct DisplayUse {
    #[cfg(any(target_os = "android", test))]
    pub peer: String,
    pub operation: String,
    pub route: String,
    #[cfg(any(target_os = "android", test))]
    pub local_intent: bool,
    #[cfg(any(target_os = "android", test))]
    pub selection: Option<capyio_protocol::v1::DisplayStreamSelection>,
    pub status: String,
    #[cfg(target_os = "android")]
    pub error: String,
    #[cfg(target_os = "android")]
    pub launch: Vec<u8>,
    #[cfg(target_os = "android")]
    pub pending: Vec<capyio_protocol::v1::DisplaySessionControl>,
    #[cfg(any(target_os = "android", test))]
    pub probe: bool,
    #[cfg(any(target_os = "android", test))]
    pub automatic: bool,
    #[cfg(target_os = "android")]
    pub epoch: u64,
    #[cfg(target_os = "android")]
    pub ready: bool,
    pub peer_deadline: Option<u64>,
}
impl std::fmt::Debug for DisplayUse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DisplayUse")
            .field("operation", &self.operation)
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

// Platform startup owns transport state; the selection policy stays host-testable.
#[cfg(any(target_os = "android", test))]
fn initial_selection(
    requested: capyio_protocol::v1::DisplayStreamSelection,
    automatic: bool,
) -> capyio_protocol::v1::DisplayStreamSelection {
    use capyio_protocol::v1::{DisplayStreamMode, DisplayStreamSelection};
    if automatic {
        DisplayStreamSelection {
            mode: Some(DisplayStreamMode {
                width: 1920,
                height: 1080,
                refresh_millihertz: 60_000,
            }),
            bitrate_kbps: 15_000,
        }
    } else {
        requested
    }
}

#[cfg(any(target_os = "android", test))]
impl DisplayUse {
    pub(crate) fn local_start(
        peer: String,
        requested: capyio_protocol::v1::DisplayStreamSelection,
        automatic: bool,
    ) -> Self {
        Self {
            peer,
            operation: MessageId::new().to_string(),
            selection: Some(initial_selection(requested, automatic)),
            status: "awaiting_consent".into(),
            local_intent: true,
            probe: !automatic,
            automatic,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn automatic_selection_ignores_stale_manual_quality() {
        use capyio_protocol::v1::{DisplayStreamMode, DisplayStreamSelection};
        let old = DisplayStreamSelection {
            mode: Some(DisplayStreamMode {
                width: 1280,
                height: 720,
                refresh_millihertz: 30_000,
            }),
            bitrate_kbps: 6000,
        };
        let auto = DisplayUse::local_start("peer".into(), old, true);
        let expected = DisplayStreamSelection {
            mode: Some(DisplayStreamMode {
                width: 1920,
                height: 1080,
                refresh_millihertz: 60_000,
            }),
            bitrate_kbps: 15_000,
        };
        assert_eq!(auto.selection, Some(expected));
        assert_eq!(auto.peer, "peer");
        assert!(auto.local_intent);
        assert!(auto.automatic);
        assert_eq!(auto.status, "awaiting_consent");
        assert!(auto.route.is_empty());
        assert!(!auto.probe);
        let manual = DisplayUse::local_start("peer".into(), old, false);
        assert_eq!(manual.selection, Some(old));
        assert!(manual.probe);
        assert!(!manual.automatic);
    }
    #[test]
    fn display_sink_has_adapter_owned_exact_format_and_consent() {
        let mut node = capyio_testkit::android_node();
        install(&mut node);
        let capability = &node.capabilities[&SINK_CAP.parse().unwrap()];
        assert_eq!(
            capability.permission_requirement,
            PermissionRequirement::UserConfirmation
        );
        let port = &capability.ports[&SINK_PORT.parse().unwrap()];
        assert_eq!(port.direction, PortDirection::Sink);
        assert_eq!(
            port.formats,
            vec![FormatDescriptor::new("gamestream-h264-sdr-v1")]
        );
        assert!(
            node.adapter_instances[&capability.adapter_instance_id]
                .owned_capabilities
                .contains(&capability.id)
        );
        capyio_runtime::NodeRuntime::new(node).unwrap();
    }
}
