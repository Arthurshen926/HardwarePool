#![forbid(unsafe_code)]

//! Shared deterministic server loop for the two repository Mock Sidecars.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, BufRead, Write};
use std::str::FromStr;

use capyio_adapter_sdk::{
    ADAPTER_CONTROL_PROTOCOL_MAJOR, AdapterCatalog, ControlProtocolVersion, InitializeParams,
    InitializeResult, ProbeResult, RoutePrepareRequest, RoutePrepareResult, RouteStartRequest,
    RouteStatusRequest, RouteStatusResult, RouteStopRequest, RouteStopResult, RpcFailure,
    RpcRequest, RpcResponse, decode_request_line, encode_response_line,
};
use capyio_core::{
    AdapterInstanceId, Availability, CapabilityClass, CapabilityDescriptor, CapabilityId,
    FormatDescriptor, InteroperabilityMode, PermissionRequirement, PortDescriptor, PortDirection,
    PortId, ProfileId, QosMode, RouteId, RouteState,
};
use serde::Serialize;

#[derive(Serialize)]
struct MockSmokeSample {
    test_only: bool,
    sequence: u64,
    payload: String,
}

#[derive(Serialize)]
struct MockRouteStartResult {
    accepted: bool,
    data_endpoint: Option<capyio_adapter_sdk::DataEndpointDescriptor>,
    warnings: Vec<capyio_adapter_sdk::AdapterProblemDescriptor>,
    test_sample: MockSmokeSample,
}

#[derive(Clone, Copy, Debug)]
pub enum MockKind {
    Source,
    Sink,
}

pub fn run(kind: MockKind, crash_on_probe: bool) -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("CapyIO {kind:?} Sidecar started; finite smoke-test control only");
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    let mut adapter_instance_id = None;
    let mut prepared_routes = BTreeMap::new();
    let mut active_routes = BTreeMap::new();

    for line in stdin.lock().split(b'\n') {
        let mut line = line?;
        line.push(b'\n');
        let request = match decode_request_line(&line) {
            Ok(request) => request,
            Err(error) => {
                eprintln!("rejected malformed control line: {error}");
                continue;
            }
        };
        eprintln!("control method={} id={}", request.method, request.id);
        if request.method == "test.crash" || (crash_on_probe && request.method == "adapter.probe") {
            eprintln!("intentional smoke-test crash");
            std::process::exit(23);
        }
        let shutdown = request.method == "adapter.shutdown";
        let response = handle_request(
            kind,
            request,
            &mut adapter_instance_id,
            &mut prepared_routes,
            &mut active_routes,
        )?;
        stdout.write_all(&encode_response_line(&response)?)?;
        stdout.flush()?;
        if shutdown {
            eprintln!("Sidecar shutdown complete");
            break;
        }
    }
    Ok(())
}

fn handle_request(
    kind: MockKind,
    request: RpcRequest,
    adapter_instance_id: &mut Option<AdapterInstanceId>,
    prepared_routes: &mut BTreeMap<RouteId, u64>,
    active_routes: &mut BTreeMap<RouteId, u64>,
) -> Result<RpcResponse, capyio_adapter_sdk::RpcError> {
    match request.method.as_str() {
        "adapter.initialize" => {
            let params: InitializeParams = request.decode_params()?;
            if params.control_protocol.major != ADAPTER_CONTROL_PROTOCOL_MAJOR {
                return failure(
                    request.id,
                    "unsupported_protocol",
                    "unsupported control major",
                );
            }
            let instance_id = match AdapterInstanceId::from_str(&params.adapter_instance_id) {
                Ok(id) => id,
                Err(_) => {
                    return failure(
                        request.id,
                        "invalid_instance_id",
                        "instance ID must be a UUID",
                    );
                }
            };
            *adapter_instance_id = Some(instance_id);
            RpcResponse::success(
                request.id,
                &InitializeResult {
                    adapter_id: adapter_id(kind).to_owned(),
                    adapter_name: format!("CapyIO Mock {kind:?}"),
                    control_protocol: ControlProtocolVersion { major: 1, minor: 0 },
                },
            )
        }
        "adapter.probe" => RpcResponse::success(
            request.id,
            &ProbeResult {
                ready: adapter_instance_id.is_some(),
                detail: "deterministic process; no hardware accessed".to_owned(),
            },
        ),
        "adapter.catalog" => match *adapter_instance_id {
            Some(instance_id) => RpcResponse::success(request.id, &catalog(kind, instance_id)),
            None => failure(
                request.id,
                "not_initialized",
                "initialize before reading catalog",
            ),
        },
        "route.prepare" => {
            let params: RoutePrepareRequest = request.decode_params()?;
            params.validate()?;
            prepared_routes.insert(params.route_id, params.epoch);
            RpcResponse::success(
                request.id,
                &RoutePrepareResult {
                    accepted: true,
                    data_endpoint: params.data_endpoint,
                    warnings: Vec::new(),
                },
            )
        }
        "route.start" => {
            let params: RouteStartRequest = request.decode_params()?;
            params.validate()?;
            if prepared_routes.get(&params.route_id) != Some(&params.epoch) {
                return failure(
                    request.id,
                    "route_not_prepared",
                    "prepare the same Route epoch before start",
                );
            }
            active_routes.insert(params.route_id, params.epoch);
            RpcResponse::success(
                request.id,
                &MockRouteStartResult {
                    accepted: true,
                    data_endpoint: None,
                    warnings: Vec::new(),
                    test_sample: MockSmokeSample {
                        test_only: true,
                        sequence: 1,
                        payload: format!("finite-{kind:?}-sample").to_lowercase(),
                    },
                },
            )
        }
        "route.stop" => {
            let params: RouteStopRequest = request.decode_params()?;
            params.validate()?;
            if prepared_routes.get(&params.route_id) != Some(&params.epoch) {
                return failure(
                    request.id,
                    "route_epoch_mismatch",
                    "stop must reference the prepared Route epoch",
                );
            }
            prepared_routes.remove(&params.route_id);
            active_routes.remove(&params.route_id);
            RpcResponse::success(request.id, &RouteStopResult { accepted: true })
        }
        "route.status" => {
            let params: RouteStatusRequest = request.decode_params()?;
            let state = if active_routes.contains_key(&params.route_id) {
                RouteState::Active
            } else if prepared_routes.contains_key(&params.route_id) {
                RouteState::Prepared
            } else {
                RouteState::Stopped
            };
            let epoch = active_routes
                .get(&params.route_id)
                .or_else(|| prepared_routes.get(&params.route_id))
                .copied()
                .unwrap_or_default();
            RpcResponse::success(
                request.id,
                &RouteStatusResult {
                    route_id: params.route_id,
                    epoch,
                    state,
                    warnings: Vec::new(),
                },
            )
        }
        "adapter.health" => RpcResponse::success(
            request.id,
            &ProbeResult {
                ready: adapter_instance_id.is_some(),
                detail: "healthy mock".to_owned(),
            },
        ),
        "adapter.shutdown" => RpcResponse::success(request.id, &true),
        _ => failure(
            request.id,
            "method_not_found",
            "unknown Adapter control method",
        ),
    }
}

fn failure(
    id: u64,
    code: &str,
    message: &str,
) -> Result<RpcResponse, capyio_adapter_sdk::RpcError> {
    RpcResponse::failure(
        id,
        RpcFailure {
            code: code.to_owned(),
            message: message.to_owned(),
            retryable: false,
            data: None,
        },
    )
}

fn adapter_id(kind: MockKind) -> &'static str {
    match kind {
        MockKind::Source => "dev.capyio.mock-source",
        MockKind::Sink => "dev.capyio.mock-sink",
    }
}

fn catalog(kind: MockKind, adapter_instance_id: AdapterInstanceId) -> AdapterCatalog {
    let (capability_id, port_id, direction, name) = match kind {
        MockKind::Source => (
            "00000000-0000-4000-8000-000000008101",
            "00000000-0000-4000-8000-000000008201",
            PortDirection::Source,
            "Mock Sample Source",
        ),
        MockKind::Sink => (
            "00000000-0000-4000-8000-000000008102",
            "00000000-0000-4000-8000-000000008202",
            PortDirection::Sink,
            "Mock Sample Sink",
        ),
    };
    let capability_id = CapabilityId::from_str(capability_id).expect("stable Capability ID");
    let port_id = PortId::from_str(port_id).expect("stable Port ID");
    let port = PortDescriptor {
        id: port_id,
        capability_id,
        display_name: format!("Finite Smoke Sample {direction:?}"),
        direction,
        profile: ProfileId::new("capyio.test.samples", 1),
        schema_id: Some("dev.capyio.test.smoke-sample-v1".to_owned()),
        formats: vec![FormatDescriptor::new("utf8-test-token")],
        qos_modes: BTreeSet::from([QosMode::Basic]),
        clock_domain: None,
        availability: Availability::Available,
        permission_requirement: PermissionRequirement::None,
        interoperability_mode: InteroperabilityMode::StandardPort,
    };
    AdapterCatalog {
        adapter_id: adapter_id(kind).to_owned(),
        capabilities: vec![CapabilityDescriptor {
            id: capability_id,
            adapter_instance_id,
            display_name: name.to_owned(),
            class: CapabilityClass::Custom("smoke_test".to_owned()),
            availability: Availability::Available,
            permission_requirement: PermissionRequirement::None,
            metadata: BTreeMap::from([("test_only".to_owned(), "true".to_owned())]),
            ports: BTreeMap::from([(port_id, port)]),
        }],
    }
}
