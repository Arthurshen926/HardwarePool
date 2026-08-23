#![forbid(unsafe_code)]

//! Deterministic Nodes, Ports and Routes shared by tests, the CLI and the UI demo.
//! No fixture reads host hardware or grants real device permission.

use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;

use capyio_core::{
    AdapterDeploymentMode, AdapterHealth, AdapterInstanceDescriptor, AdapterInstanceId,
    AdapterState, Availability, CapabilityClass, CapabilityDescriptor, FormatDescriptor,
    InteroperabilityMode, NodeDescriptor, NodeId, PermissionRequirement, Platform, PortDescriptor,
    PortDirection, PortRef, ProfileId, ProtocolVersion, QosMode, RouteBackend, RouteId, SessionId,
};
use capyio_runtime::{NodeRuntime, RuntimeError};

pub const WINDOWS_NODE_ID: &str = "00000000-0000-4000-8000-000000000001";
pub const ANDROID_NODE_ID: &str = "00000000-0000-4000-8000-000000000002";
const DEMO_SESSION_ID: &str = "00000000-0000-4000-8000-000000000901";
const PHONE_MIC_ROUTE_ID: &str = "00000000-0000-4000-8000-000000000911";
const SYSTEM_MIX_ROUTE_ID: &str = "00000000-0000-4000-8000-000000000912";
const PHONE_IMU_ROUTE_ID: &str = "00000000-0000-4000-8000-000000000913";
const PHONE_CAMERA_ROUTE_ID: &str = "00000000-0000-4000-8000-000000000914";
const WINDOWS_AUDIO_ADAPTER_ID: &str = "00000000-0000-4000-8000-000000000011";
const WINDOWS_PROJECTION_ADAPTER_ID: &str = "00000000-0000-4000-8000-000000000012";
const ANDROID_HARDWARE_ADAPTER_ID: &str = "00000000-0000-4000-8000-000000000021";

const WIN_SYSTEM_MIX_CAPABILITY_ID: &str = "00000000-0000-4000-8000-000000000101";
const WIN_MICROPHONE_CAPABILITY_ID: &str = "00000000-0000-4000-8000-000000000102";
const WIN_SPEAKER_CAPABILITY_ID: &str = "00000000-0000-4000-8000-000000000103";
const WIN_VIRTUAL_MIC_CAPABILITY_ID: &str = "00000000-0000-4000-8000-000000000104";
const WIN_GAMEPAD_SINK_CAPABILITY_ID: &str = "00000000-0000-4000-8000-000000000105";
const WIN_CAMERA_PANEL_CAPABILITY_ID: &str = "00000000-0000-4000-8000-000000000106";
const PHONE_MICROPHONE_CAPABILITY_ID: &str = "00000000-0000-4000-8000-000000000201";
const PHONE_SPEAKER_CAPABILITY_ID: &str = "00000000-0000-4000-8000-000000000202";
const PHONE_CAMERA_CAPABILITY_ID: &str = "00000000-0000-4000-8000-000000000203";
const PHONE_IMU_CAPABILITY_ID: &str = "00000000-0000-4000-8000-000000000204";
const PHONE_GAMEPAD_CAPABILITY_ID: &str = "00000000-0000-4000-8000-000000000205";
const PHONE_VIBRATION_CAPABILITY_ID: &str = "00000000-0000-4000-8000-000000000206";

const WIN_SYSTEM_MIX_PORT_ID: &str = "00000000-0000-4000-8000-000000001101";
const WIN_MICROPHONE_PORT_ID: &str = "00000000-0000-4000-8000-000000001102";
const WIN_SPEAKER_PORT_ID: &str = "00000000-0000-4000-8000-000000001103";
const WIN_VIRTUAL_MIC_PORT_ID: &str = "00000000-0000-4000-8000-000000001104";
const WIN_GAMEPAD_SINK_PORT_ID: &str = "00000000-0000-4000-8000-000000001105";
const WIN_CAMERA_PANEL_PORT_ID: &str = "00000000-0000-4000-8000-000000001106";
const PHONE_MICROPHONE_PORT_ID: &str = "00000000-0000-4000-8000-000000001201";
const PHONE_SPEAKER_PORT_ID: &str = "00000000-0000-4000-8000-000000001202";
const PHONE_CAMERA_PORT_ID: &str = "00000000-0000-4000-8000-000000001203";
const PHONE_IMU_PORT_ID: &str = "00000000-0000-4000-8000-000000001204";
const PHONE_GAMEPAD_PORT_ID: &str = "00000000-0000-4000-8000-000000001205";
const PHONE_VIBRATION_PORT_ID: &str = "00000000-0000-4000-8000-000000001206";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DemoRoutes {
    pub phone_microphone_to_windows: RouteId,
    pub windows_system_mix_to_phone: RouteId,
    pub phone_imu_to_gamepad: RouteId,
    pub phone_camera_to_panel: RouteId,
}

impl DemoRoutes {
    #[must_use]
    pub const fn all(self) -> [RouteId; 4] {
        [
            self.phone_microphone_to_windows,
            self.windows_system_mix_to_phone,
            self.phone_imu_to_gamepad,
            self.phone_camera_to_panel,
        ]
    }
}

#[derive(Clone, Debug)]
pub struct DemoLab {
    pub runtime: NodeRuntime,
    pub session_id: SessionId,
    pub routes: DemoRoutes,
}

impl DemoLab {
    pub fn new() -> Result<Self, RuntimeError> {
        let windows = windows_node();
        let android = android_node();
        let windows_id = windows.id;
        let android_id = android.id;
        let mut runtime = NodeRuntime::new(windows)?;
        runtime.register_peer(android, true)?;
        let session_id = runtime.open_session_with_id(parse_id(DEMO_SESSION_ID), android_id)?;

        let phone_microphone_to_windows = runtime.create_route_with_id(
            parse_id(PHONE_MIC_ROUTE_ID),
            session_id,
            port_ref(
                android_id,
                PHONE_MICROPHONE_CAPABILITY_ID,
                PHONE_MICROPHONE_PORT_ID,
            ),
            port_ref(
                windows_id,
                WIN_VIRTUAL_MIC_CAPABILITY_ID,
                WIN_VIRTUAL_MIC_PORT_ID,
            ),
            RouteBackend::CapyDataPlane,
        )?;
        let windows_system_mix_to_phone = runtime.create_route_with_id(
            parse_id(SYSTEM_MIX_ROUTE_ID),
            session_id,
            port_ref(
                windows_id,
                WIN_SYSTEM_MIX_CAPABILITY_ID,
                WIN_SYSTEM_MIX_PORT_ID,
            ),
            port_ref(
                android_id,
                PHONE_SPEAKER_CAPABILITY_ID,
                PHONE_SPEAKER_PORT_ID,
            ),
            RouteBackend::CapyDataPlane,
        )?;
        let phone_imu_to_gamepad = runtime.create_route_with_id(
            parse_id(PHONE_IMU_ROUTE_ID),
            session_id,
            port_ref(android_id, PHONE_IMU_CAPABILITY_ID, PHONE_IMU_PORT_ID),
            port_ref(
                windows_id,
                WIN_GAMEPAD_SINK_CAPABILITY_ID,
                WIN_GAMEPAD_SINK_PORT_ID,
            ),
            RouteBackend::LocalPipeline,
        )?;
        let phone_camera_to_panel = runtime.create_route_with_id(
            parse_id(PHONE_CAMERA_ROUTE_ID),
            session_id,
            port_ref(android_id, PHONE_CAMERA_CAPABILITY_ID, PHONE_CAMERA_PORT_ID),
            port_ref(
                windows_id,
                WIN_CAMERA_PANEL_CAPABILITY_ID,
                WIN_CAMERA_PANEL_PORT_ID,
            ),
            RouteBackend::CapyDataPlane,
        )?;

        Ok(Self {
            runtime,
            session_id,
            routes: DemoRoutes {
                phone_microphone_to_windows,
                windows_system_mix_to_phone,
                phone_imu_to_gamepad,
                phone_camera_to_panel,
            },
        })
    }

    /// Synthetic authorization is intentionally confined to the deterministic demo.
    pub fn set_route_active(
        &mut self,
        route_id: RouteId,
        active: bool,
        now_ms: u64,
    ) -> Result<(), RuntimeError> {
        self.runtime.set_route_active(route_id, active, now_ms)
    }

    pub fn set_microphone_active(&mut self, active: bool, now_ms: u64) -> Result<(), RuntimeError> {
        self.set_route_active(self.routes.phone_microphone_to_windows, active, now_ms)
    }

    pub fn set_speaker_active(&mut self, active: bool, now_ms: u64) -> Result<(), RuntimeError> {
        self.set_route_active(self.routes.windows_system_mix_to_phone, active, now_ms)
    }
}

#[must_use]
pub fn windows_node() -> NodeDescriptor {
    let mut node = fixture_node(
        WINDOWS_NODE_ID,
        "HP OmniBook Ultra Flip 14",
        Platform::Windows,
        "Windows fixture",
    );
    let audio_adapter = adapter(
        WINDOWS_AUDIO_ADAPTER_ID,
        "capyio.windows.audio",
        "Windows Audio Adapter",
        AdapterDeploymentMode::Sidecar,
        [RouteBackend::CapyDataPlane],
    );
    let audio_adapter_id = audio_adapter.id;
    node.add_adapter(audio_adapter)
        .expect("valid fixture Adapter");
    let projection_adapter = adapter(
        WINDOWS_PROJECTION_ADAPTER_ID,
        "capyio.windows.projection",
        "Windows Projection Adapter",
        AdapterDeploymentMode::Sidecar,
        [RouteBackend::CapyDataPlane, RouteBackend::LocalPipeline],
    );
    let projection_adapter_id = projection_adapter.id;
    node.add_adapter(projection_adapter)
        .expect("valid fixture Adapter");

    add_capability(
        &mut node,
        audio_adapter_id,
        WIN_SYSTEM_MIX_CAPABILITY_ID,
        "System Mix",
        CapabilityClass::Custom("system_audio_capture".to_owned()),
        port(
            WIN_SYSTEM_MIX_CAPABILITY_ID,
            WIN_SYSTEM_MIX_PORT_ID,
            "System Mix Source",
            PortDirection::Source,
            ProfileId::audio_frames_v1(),
            "pcm-s16le-48000-stereo",
            QosMode::Interactive,
        ),
    );
    add_capability(
        &mut node,
        audio_adapter_id,
        WIN_MICROPHONE_CAPABILITY_ID,
        "Windows Microphone",
        CapabilityClass::Microphone,
        port(
            WIN_MICROPHONE_CAPABILITY_ID,
            WIN_MICROPHONE_PORT_ID,
            "Windows Microphone Source",
            PortDirection::Source,
            ProfileId::audio_frames_v1(),
            "pcm-s16le-48000-mono",
            QosMode::Interactive,
        ),
    );
    add_capability(
        &mut node,
        audio_adapter_id,
        WIN_SPEAKER_CAPABILITY_ID,
        "Windows Speaker",
        CapabilityClass::Speaker,
        port(
            WIN_SPEAKER_CAPABILITY_ID,
            WIN_SPEAKER_PORT_ID,
            "Windows Speaker Sink",
            PortDirection::Sink,
            ProfileId::audio_frames_v1(),
            "pcm-s16le-48000-stereo",
            QosMode::Interactive,
        ),
    );
    add_capability(
        &mut node,
        audio_adapter_id,
        WIN_VIRTUAL_MIC_CAPABILITY_ID,
        "Windows Virtual Microphone",
        CapabilityClass::Microphone,
        port(
            WIN_VIRTUAL_MIC_CAPABILITY_ID,
            WIN_VIRTUAL_MIC_PORT_ID,
            "Virtual Microphone Sink",
            PortDirection::Sink,
            ProfileId::audio_frames_v1(),
            "pcm-s16le-48000-mono",
            QosMode::Interactive,
        ),
    );
    add_capability(
        &mut node,
        projection_adapter_id,
        WIN_GAMEPAD_SINK_CAPABILITY_ID,
        "Windows Gamepad Projection",
        CapabilityClass::Gamepad,
        port(
            WIN_GAMEPAD_SINK_CAPABILITY_ID,
            WIN_GAMEPAD_SINK_PORT_ID,
            "Gamepad Projection Sink",
            PortDirection::Sink,
            ProfileId::imu_samples_v1(),
            "imu-si-f32-le",
            QosMode::Measurement,
        ),
    );
    add_capability(
        &mut node,
        projection_adapter_id,
        WIN_CAMERA_PANEL_CAPABILITY_ID,
        "Camera Preview Panel",
        CapabilityClass::Panel,
        port(
            WIN_CAMERA_PANEL_CAPABILITY_ID,
            WIN_CAMERA_PANEL_PORT_ID,
            "Camera Preview Sink",
            PortDirection::Sink,
            ProfileId::video_frames_v1(),
            "bgra8-1280x720-30",
            QosMode::Basic,
        ),
    );
    node.validate().expect("valid Windows fixture");
    node
}

#[must_use]
pub fn android_node() -> NodeDescriptor {
    let mut node = fixture_node(
        ANDROID_NODE_ID,
        "vivo X200 Pro mini",
        Platform::Android,
        "Android fixture",
    );
    let hardware_adapter = adapter(
        ANDROID_HARDWARE_ADAPTER_ID,
        "capyio.android.integrated-hardware",
        "Android Integrated Hardware Adapter",
        AdapterDeploymentMode::InProcess,
        [RouteBackend::CapyDataPlane, RouteBackend::LocalPipeline],
    );
    let hardware_adapter_id = hardware_adapter.id;
    node.add_adapter(hardware_adapter)
        .expect("valid fixture Adapter");

    add_capability(
        &mut node,
        hardware_adapter_id,
        PHONE_MICROPHONE_CAPABILITY_ID,
        "Phone Microphone",
        CapabilityClass::Microphone,
        permission_port(
            PHONE_MICROPHONE_CAPABILITY_ID,
            PHONE_MICROPHONE_PORT_ID,
            "Phone Microphone Source",
            PortDirection::Source,
            ProfileId::audio_frames_v1(),
            "pcm-s16le-48000-mono",
            QosMode::Interactive,
            PermissionRequirement::ForegroundService,
        ),
    );
    add_capability(
        &mut node,
        hardware_adapter_id,
        PHONE_SPEAKER_CAPABILITY_ID,
        "Phone Speaker",
        CapabilityClass::Speaker,
        port(
            PHONE_SPEAKER_CAPABILITY_ID,
            PHONE_SPEAKER_PORT_ID,
            "Phone Speaker Sink",
            PortDirection::Sink,
            ProfileId::audio_frames_v1(),
            "pcm-s16le-48000-stereo",
            QosMode::Interactive,
        ),
    );
    add_capability(
        &mut node,
        hardware_adapter_id,
        PHONE_CAMERA_CAPABILITY_ID,
        "Back Camera",
        CapabilityClass::Camera,
        permission_port(
            PHONE_CAMERA_CAPABILITY_ID,
            PHONE_CAMERA_PORT_ID,
            "Back Camera Source",
            PortDirection::Source,
            ProfileId::video_frames_v1(),
            "bgra8-1280x720-30",
            QosMode::Basic,
            PermissionRequirement::ForegroundService,
        ),
    );
    add_capability(
        &mut node,
        hardware_adapter_id,
        PHONE_IMU_CAPABILITY_ID,
        "Phone IMU",
        CapabilityClass::Imu,
        port(
            PHONE_IMU_CAPABILITY_ID,
            PHONE_IMU_PORT_ID,
            "IMU Sample Source",
            PortDirection::Source,
            ProfileId::imu_samples_v1(),
            "imu-si-f32-le",
            QosMode::Measurement,
        ),
    );
    add_capability(
        &mut node,
        hardware_adapter_id,
        PHONE_GAMEPAD_CAPABILITY_ID,
        "Touch Gamepad",
        CapabilityClass::Gamepad,
        port(
            PHONE_GAMEPAD_CAPABILITY_ID,
            PHONE_GAMEPAD_PORT_ID,
            "Touch Gamepad Source",
            PortDirection::Source,
            ProfileId::new("capyio.input.gamepad", 1),
            "gamepad-state-v1",
            QosMode::Interactive,
        ),
    );
    add_capability(
        &mut node,
        hardware_adapter_id,
        PHONE_VIBRATION_CAPABILITY_ID,
        "Phone Vibration",
        CapabilityClass::Haptics,
        port(
            PHONE_VIBRATION_CAPABILITY_ID,
            PHONE_VIBRATION_PORT_ID,
            "Vibration Sink",
            PortDirection::Sink,
            ProfileId::new("capyio.haptics.pattern", 1),
            "haptic-pattern-v1",
            QosMode::Interactive,
        ),
    );
    node.validate().expect("valid Android fixture");
    node
}

fn fixture_node(
    id: &str,
    name: &str,
    platform: Platform,
    platform_version: &str,
) -> NodeDescriptor {
    NodeDescriptor::new(
        parse_id(id),
        name,
        platform,
        platform_version,
        env!("CARGO_PKG_VERSION"),
        [ProtocolVersion::new(1, 0)],
    )
}

fn adapter(
    id: &str,
    adapter_type: &str,
    display_name: &str,
    deployment_mode: AdapterDeploymentMode,
    route_modes: impl IntoIterator<Item = RouteBackend>,
) -> AdapterInstanceDescriptor {
    AdapterInstanceDescriptor {
        id: parse_id(id),
        adapter_type: adapter_type.to_owned(),
        display_name: display_name.to_owned(),
        deployment_mode,
        version: env!("CARGO_PKG_VERSION").to_owned(),
        state: AdapterState::Ready,
        health: AdapterHealth::Healthy,
        owned_capabilities: BTreeSet::new(),
        supported_route_modes: route_modes.into_iter().collect(),
    }
}

fn add_capability(
    node: &mut NodeDescriptor,
    adapter_instance_id: AdapterInstanceId,
    capability_id: &str,
    display_name: &str,
    class: CapabilityClass,
    port: PortDescriptor,
) {
    let capability = CapabilityDescriptor {
        id: parse_id(capability_id),
        adapter_instance_id,
        display_name: display_name.to_owned(),
        class,
        availability: port.availability,
        permission_requirement: port.permission_requirement,
        metadata: BTreeMap::new(),
        ports: BTreeMap::from([(port.id, port)]),
    };
    node.add_capability(capability)
        .expect("valid fixture Capability");
}

#[allow(clippy::too_many_arguments)]
fn permission_port(
    capability_id: &str,
    port_id: &str,
    display_name: &str,
    direction: PortDirection,
    profile: ProfileId,
    format: &str,
    qos: QosMode,
    permission_requirement: PermissionRequirement,
) -> PortDescriptor {
    PortDescriptor {
        id: parse_id(port_id),
        capability_id: parse_id(capability_id),
        display_name: display_name.to_owned(),
        direction,
        profile,
        schema_id: None,
        formats: vec![FormatDescriptor::new(format)],
        qos_modes: BTreeSet::from([qos]),
        clock_domain: None,
        availability: if permission_requirement == PermissionRequirement::None {
            Availability::Available
        } else {
            Availability::PermissionRequired
        },
        permission_requirement,
        interoperability_mode: InteroperabilityMode::StandardPort,
    }
}

fn port(
    capability_id: &str,
    port_id: &str,
    display_name: &str,
    direction: PortDirection,
    profile: ProfileId,
    format: &str,
    qos: QosMode,
) -> PortDescriptor {
    permission_port(
        capability_id,
        port_id,
        display_name,
        direction,
        profile,
        format,
        qos,
        PermissionRequirement::None,
    )
}

fn port_ref(node_id: NodeId, capability_id: &str, port_id: &str) -> PortRef {
    PortRef {
        node_id,
        capability_id: parse_id(capability_id),
        port_id: parse_id(port_id),
    }
}

fn parse_id<T: FromStr>(value: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    value.parse().expect("stable fixture UUID")
}

#[cfg(test)]
mod tests {
    use capyio_core::RouteState;

    use super::*;

    #[test]
    fn fixtures_are_symmetric_nodes_without_roles() {
        for node in [windows_node(), android_node()] {
            let directions = node
                .capabilities
                .values()
                .flat_map(|capability| capability.ports.values())
                .map(|port| port.direction)
                .collect::<BTreeSet<_>>();
            assert!(directions.contains(&PortDirection::Source));
            assert!(directions.contains(&PortDirection::Sink));
        }
    }

    #[test]
    fn demo_session_and_route_ids_are_stable() {
        let first = DemoLab::new().expect("first lab");
        let second = DemoLab::new().expect("second lab");
        assert_eq!(first.session_id, second.session_id);
        assert_eq!(first.routes, second.routes);
    }

    #[test]
    fn four_routes_span_both_directions_and_three_profiles() {
        let lab = DemoLab::new().expect("demo lab");
        let routes = lab
            .routes
            .all()
            .map(|route_id| lab.runtime.route(route_id).expect("fixture Route"));
        assert_eq!(routes.len(), 4);
        assert!(
            routes
                .iter()
                .any(|route| route.source.node_id == parse_id(ANDROID_NODE_ID)
                    && route.sink.node_id == parse_id(WINDOWS_NODE_ID))
        );
        assert!(
            routes
                .iter()
                .any(|route| route.source.node_id == parse_id(WINDOWS_NODE_ID)
                    && route.sink.node_id == parse_id(ANDROID_NODE_ID))
        );
        assert_eq!(
            routes
                .iter()
                .map(|route| route.profile.clone())
                .collect::<BTreeSet<_>>()
                .len(),
            3
        );
    }

    #[test]
    fn stopping_one_route_does_not_change_the_others() {
        let mut lab = DemoLab::new().expect("demo lab");
        for (index, route_id) in lab.routes.all().into_iter().enumerate() {
            lab.set_route_active(route_id, true, index as u64 + 1)
                .expect("start Route");
        }
        lab.set_route_active(lab.routes.phone_microphone_to_windows, false, 10)
            .expect("stop one Route");
        assert_eq!(
            lab.runtime
                .route(lab.routes.phone_microphone_to_windows)
                .expect("Route")
                .state,
            RouteState::Stopped
        );
        for route_id in [
            lab.routes.windows_system_mix_to_phone,
            lab.routes.phone_imu_to_gamepad,
            lab.routes.phone_camera_to_panel,
        ] {
            assert_eq!(
                lab.runtime.route(route_id).expect("Route").state,
                RouteState::Active
            );
        }
    }
}
