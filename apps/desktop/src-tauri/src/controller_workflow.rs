//! One admitted input Route owns one controller operation. Pairing alone is not consent.
use capyio_core::*;
use capyio_runtime::NodeRuntime;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ControllerMode {
    #[serde(rename = "dualshock4")]
    Dualshock4,
    Xbox360,
    Dsu,
}

pub(crate) fn sink(mode: ControllerMode) -> (CapabilityId, PortId) {
    let suffix = match mode {
        ControllerMode::Dualshock4 => ("21", "31"),
        ControllerMode::Xbox360 => ("22", "32"),
        ControllerMode::Dsu => ("23", "33"),
    };
    (
        format!("d41251b3-63ed-4e83-807e-4c4ab9b20c{}", suffix.0)
            .parse()
            .unwrap(),
        format!("d41251b3-63ed-4e83-807e-4c4ab9b20c{}", suffix.1)
            .parse()
            .unwrap(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn published_modes_are_real_adapter_managed_input_sinks() {
        let mut runtime = NodeRuntime::new(capyio_testkit::windows_node()).unwrap();
        install_catalog(&mut runtime).unwrap();
        install_catalog(&mut runtime).unwrap();
        let snapshot = runtime.snapshot();
        for mode in [
            ControllerMode::Dualshock4,
            ControllerMode::Xbox360,
            ControllerMode::Dsu,
        ] {
            let (cap, port) = sink(mode);
            let capability = &snapshot.local_node.capabilities[&cap];
            let endpoint = &capability.ports[&port];
            assert_eq!(endpoint.profile, ProfileId::gamepad_state_v1());
            assert_eq!(endpoint.direction, PortDirection::Sink);
            assert_eq!(
                endpoint.interoperability_mode,
                InteroperabilityMode::AdapterManaged
            );
            assert_eq!(
                endpoint.permission_requirement,
                PermissionRequirement::UserConfirmation
            );
        }
    }
    #[test]
    fn projection_sinks_are_distinct_and_round_trip() {
        let mut ports = BTreeSet::new();
        for mode in [
            ControllerMode::Dualshock4,
            ControllerMode::Xbox360,
            ControllerMode::Dsu,
        ] {
            let (_, port) = sink(mode);
            assert!(ports.insert(port));
            assert_eq!(mode_for_sink(port), Some(mode));
        }
        assert_eq!(mode_for_sink(PortId::new()), None);
    }
    #[test]
    fn one_operation_cannot_be_replaced_until_stopped() {
        let mut owner = ControllerWorkflow::default();
        let peer = NodeId::new();
        let route = RouteId::new();
        owner
            .begin(peer, ControllerMode::Dualshock4, route)
            .unwrap();
        assert!(
            owner
                .begin(NodeId::new(), ControllerMode::Xbox360, RouteId::new())
                .is_err()
        );
        owner.starting();
        owner.active();
        assert_eq!(owner.snapshot().unwrap().status, "active");
        assert_eq!(owner.route(), Some(route));
        owner.stopping();
        owner.clear();
        assert_eq!(owner.route(), None);
        owner
            .begin(peer, ControllerMode::Dsu, RouteId::new())
            .unwrap();
        owner.fail("source offline".into());
        assert_eq!(
            owner.snapshot().unwrap().error.as_deref(),
            Some("source offline")
        );
    }
}
pub(crate) fn mode_for_sink(port: PortId) -> Option<ControllerMode> {
    [
        ControllerMode::Dualshock4,
        ControllerMode::Xbox360,
        ControllerMode::Dsu,
    ]
    .into_iter()
    .find(|mode| sink(*mode).1 == port)
}
pub(crate) fn install_catalog(runtime: &mut NodeRuntime) -> Result<(), String> {
    let id: AdapterInstanceId = "d41251b3-63ed-4e83-807e-4c4ab9b20c01".parse().unwrap();
    if runtime
        .snapshot()
        .local_node
        .adapter_instances
        .contains_key(&id)
    {
        return Ok(());
    }
    let caps = [
        ControllerMode::Dualshock4,
        ControllerMode::Xbox360,
        ControllerMode::Dsu,
    ]
    .into_iter()
    .map(|mode| {
        let (capability_id, port_id) = sink(mode);
        let name = match mode {
            ControllerMode::Dualshock4 => "Windows DS4 手柄",
            ControllerMode::Xbox360 => "Windows Xbox 手柄",
            ControllerMode::Dsu => "Windows DSU 体感",
        };
        CapabilityDescriptor {
            id: capability_id,
            adapter_instance_id: id,
            display_name: name.into(),
            class: CapabilityClass::Gamepad,
            availability: Availability::Available,
            permission_requirement: PermissionRequirement::UserConfirmation,
            metadata: BTreeMap::from([
                (
                    "controller_mode".into(),
                    match mode {
                        ControllerMode::Dualshock4 => "dualshock4",
                        ControllerMode::Xbox360 => "xbox360",
                        ControllerMode::Dsu => "dsu",
                    }
                    .into(),
                ),
                (
                    "transport".into(),
                    "capyio-gamepad-udp/1; optional motion in input packet".into(),
                ),
            ]),
            ports: BTreeMap::from([(
                port_id,
                PortDescriptor {
                    id: port_id,
                    capability_id,
                    display_name: name.into(),
                    direction: PortDirection::Sink,
                    profile: ProfileId::gamepad_state_v1(),
                    schema_id: None,
                    formats: vec![FormatDescriptor::new("gamepad-state-v1")],
                    qos_modes: BTreeSet::from([QosMode::Interactive]),
                    clock_domain: None,
                    availability: Availability::Available,
                    permission_requirement: PermissionRequirement::UserConfirmation,
                    interoperability_mode: InteroperabilityMode::AdapterManaged,
                },
            )]),
        }
    })
    .collect();
    runtime
        .register_adapter_catalog(
            runtime.snapshot().local_node.id,
            AdapterInstanceDescriptor {
                id,
                adapter_type: "dev.capyio.windows.controller".into(),
                display_name: "Windows controller projection".into(),
                deployment_mode: AdapterDeploymentMode::InProcess,
                version: env!("CARGO_PKG_VERSION").into(),
                state: AdapterState::Ready,
                health: AdapterHealth::Unknown,
                owned_capabilities: BTreeSet::new(),
                supported_route_modes: BTreeSet::from([RouteBackend::AdapterManaged]),
            },
            caps,
        )
        .map_err(|e| e.to_string())
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UiControllerUse {
    pub peer_node_id: NodeId,
    pub mode: ControllerMode,
    pub status: &'static str,
    pub error: Option<String>,
}
#[derive(Default)]
pub(crate) struct ControllerWorkflow {
    state: Option<UiControllerUse>,
    route: Option<RouteId>,
}
impl ControllerWorkflow {
    pub(crate) fn begin(
        &mut self,
        peer: NodeId,
        mode: ControllerMode,
        route: RouteId,
    ) -> Result<(), String> {
        if self
            .state
            .as_ref()
            .is_some_and(|s| !matches!(s.status, "idle" | "failed"))
        {
            return Err("请先停止当前手柄再切换设备或模式".into());
        }
        self.route = Some(route);
        self.state = Some(UiControllerUse {
            peer_node_id: peer,
            mode,
            status: "awaiting_consent",
            error: None,
        });
        Ok(())
    }
    pub(crate) fn snapshot(&self) -> Option<UiControllerUse> {
        self.state.clone()
    }
    pub(crate) fn route(&self) -> Option<RouteId> {
        self.route
    }
    pub(crate) fn peer(&self) -> Option<NodeId> {
        self.state.as_ref().map(|s| s.peer_node_id)
    }
    pub(crate) fn mode(&self) -> Option<ControllerMode> {
        self.state.as_ref().map(|s| s.mode)
    }
    pub(crate) fn starting(&mut self) {
        if let Some(s) = &mut self.state {
            s.status = "starting";
        }
    }
    pub(crate) fn active(&mut self) {
        if let Some(s) = &mut self.state {
            s.status = "active";
        }
    }
    pub(crate) fn stopping(&mut self) {
        if let Some(s) = &mut self.state {
            s.status = "stopping";
        }
    }
    pub(crate) fn fail(&mut self, error: String) {
        if let Some(s) = &mut self.state {
            s.status = "failed";
            s.error = Some(error);
        }
    }
    pub(crate) fn clear(&mut self) {
        self.route = None;
        if let Some(s) = &mut self.state {
            s.status = "idle";
            s.error = None;
        }
    }
}
