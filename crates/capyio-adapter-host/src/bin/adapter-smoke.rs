use std::fs;
use std::path::{Path, PathBuf};

use capyio_adapter_host::{HostError, SidecarHost};
use capyio_adapter_sdk::{
    AdapterManifest, BoundedJsonObject, RoutePrepareRequest, RouteStartRequest, RouteStatusRequest,
    RouteStopRequest,
};
use capyio_core::{
    AdapterInstanceId, CapabilityId, NodeId, PortDirection, PortId, PortRef, RouteBackend, RouteId,
    RouteState,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = std::env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    if arguments.len() != 4 {
        return Err(std::io::Error::other(
            "usage: capyio-adapter-smoke <source-exe> <source-manifest> <sink-exe> <sink-manifest>",
        )
        .into());
    }
    run_lifecycle(&arguments[0], &arguments[1], PortDirection::Source)?;
    run_lifecycle(&arguments[2], &arguments[3], PortDirection::Sink)?;
    run_crash_detection(&arguments[0], &arguments[1])?;
    println!("Adapter smoke test: PASS (source, sink, crash isolation)");
    Ok(())
}

fn run_lifecycle(
    executable: &Path,
    manifest_path: &Path,
    expected_direction: PortDirection,
) -> Result<(), Box<dyn std::error::Error>> {
    let manifest = read_manifest(manifest_path)?;
    let mut host = SidecarHost::spawn(executable, std::iter::empty::<&str>(), manifest)?;
    let initialized = host.initialize(AdapterInstanceId::new())?;
    require(
        !initialized.adapter_id.is_empty(),
        "initialize returned no Adapter ID",
    )?;
    require(host.probe()?.ready, "probe did not report ready")?;
    require(host.health()?.ready, "health did not report ready")?;
    let catalog = host.catalog()?;
    require(
        catalog.capabilities.len() == 1,
        "catalog must contain one Capability",
    )?;
    let port = catalog.capabilities[0]
        .ports
        .values()
        .next()
        .ok_or_else(|| std::io::Error::other("catalog Capability must contain one Port"))?;
    require(
        port.direction == expected_direction,
        "catalog Port direction mismatch",
    )?;

    let route_id = RouteId::new();
    let advertised = PortRef {
        node_id: NodeId::new(),
        capability_id: port.capability_id,
        port_id: port.id,
    };
    let peer = PortRef {
        node_id: NodeId::new(),
        capability_id: CapabilityId::new(),
        port_id: PortId::new(),
    };
    let (source, sink) = match expected_direction {
        PortDirection::Source => (advertised, peer),
        PortDirection::Sink => (peer, advertised),
        PortDirection::Control => {
            return Err(std::io::Error::other("smoke Route requires a data Port").into());
        }
    };
    let epoch = 1;
    let prepared = host.prepare_route(RoutePrepareRequest {
        route_id,
        source,
        sink,
        profile: port.profile.clone(),
        selected_format: port.formats.first().cloned(),
        selected_qos: port
            .qos_modes
            .first()
            .cloned()
            .ok_or_else(|| std::io::Error::other("catalog Port must declare QoS"))?,
        backend: RouteBackend::CapyDataPlane,
        epoch,
        data_endpoint: None,
        adapter_config: BoundedJsonObject::default(),
    })?;
    require(prepared.accepted, "Route prepare was not accepted")?;
    let started = host.start_route(RouteStartRequest { route_id, epoch })?;
    require(started.accepted, "Route start was not accepted")?;
    require(
        host.route_status(RouteStatusRequest { route_id })?.state == RouteState::Active,
        "Route status did not become active",
    )?;
    host.stop_route(RouteStopRequest { route_id, epoch })?;
    require(
        host.route_status(RouteStatusRequest { route_id })?.state == RouteState::Stopped,
        "Route status did not become stopped",
    )?;
    host.shutdown()?;
    require(
        !host.stderr_lines().is_empty(),
        "ordinary Sidecar logs must use stderr",
    )?;
    Ok(())
}

fn run_crash_detection(
    executable: &Path,
    manifest_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let manifest = read_manifest(manifest_path)?;
    let mut host = SidecarHost::spawn(executable, std::iter::empty::<&str>(), manifest)?;
    host.initialize(AdapterInstanceId::new())?;
    let status = match host.crash_for_smoke_test() {
        Err(HostError::UnexpectedExit(status)) => status,
        Err(error) => return Err(error.into()),
        Ok(()) => return Err(std::io::Error::other("Mock Sidecar did not crash").into()),
    };
    require(status.code() == Some(23), "unexpected crash exit code")?;
    require(
        !host.stderr_lines().is_empty(),
        "crash diagnostics were not retained",
    )?;
    Ok(())
}

fn read_manifest(path: &Path) -> Result<AdapterManifest, Box<dyn std::error::Error>> {
    Ok(AdapterManifest::from_json(&fs::read(path)?)?)
}

fn require(condition: bool, message: &'static str) -> Result<(), std::io::Error> {
    if condition {
        Ok(())
    } else {
        Err(std::io::Error::other(message))
    }
}
