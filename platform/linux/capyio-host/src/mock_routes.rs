//! Explicit synthetic lifecycle and opt-in finite local IPC. No peer authentication.
use std::{collections::BTreeSet, io, path::Path};

use capyio_adapter_host::SidecarHost;
use capyio_adapter_sdk::DataEndpointDescriptor;
use capyio_adapter_sdk::{
    AdapterManifest, BoundedJsonObject, RoutePrepareRequest, RouteStartRequest, RouteStatusRequest,
    RouteStatusResult, RouteStopRequest,
};
use capyio_core::{
    AdapterDeploymentMode, AdapterHealth, AdapterInstanceDescriptor, AdapterInstanceId,
    AdapterState, NodeDescriptor, NodeId, Platform, PortDirection, PortRef, ProtocolVersion,
    RouteBackend, RouteId, RouteState,
};
use capyio_mock_sidecar_support::payload::{Endpoint, Receipt, validate_receipt};
use capyio_runtime::NodeRuntime;

const PEER: &str = "00000000-0000-4000-8000-000000000302";
const ROUTE: &str = "00000000-0000-4000-8000-000000000303";
const SESSION: &str = "00000000-0000-4000-8000-000000000304";
const SOURCE: &str = "00000000-0000-4000-8000-000000000305";
const SINK: &str = "00000000-0000-4000-8000-000000000306";

fn error(message: impl ToString) -> io::Error {
    io::Error::other(message.to_string())
}

trait Control {
    fn prepare(&mut self, request: RoutePrepareRequest) -> io::Result<()>;
    fn start(&mut self, request: RouteStartRequest) -> io::Result<Option<DataEndpointDescriptor>>;
    fn status(&mut self, request: RouteStatusRequest) -> io::Result<RouteStatusResult>;
    fn stop(&mut self, request: RouteStopRequest) -> io::Result<()>;
    fn inject_crash(&mut self) -> io::Result<()>;
}
impl Control for SidecarHost {
    fn inject_crash(&mut self) -> io::Result<()> {
        match self.crash_for_smoke_test() {
            Err(capyio_adapter_host::HostError::UnexpectedExit(status))
                if status.code() == Some(23) =>
            {
                Ok(())
            }
            _ => Err(error("mock crash was not observed")),
        }
    }
    fn prepare(&mut self, request: RoutePrepareRequest) -> io::Result<()> {
        self.prepare_route(request).map(|_| ()).map_err(error)
    }
    fn start(&mut self, request: RouteStartRequest) -> io::Result<Option<DataEndpointDescriptor>> {
        self.start_route(request)
            .map(|result| result.data_endpoint)
            .map_err(error)
    }
    fn status(&mut self, request: RouteStatusRequest) -> io::Result<RouteStatusResult> {
        self.route_status(request).map_err(error)
    }
    fn stop(&mut self, request: RouteStopRequest) -> io::Result<()> {
        self.stop_route(request).map_err(error)
    }
}

pub(crate) struct MockRoutes {
    children: Vec<Box<dyn Control>>,
    owners: [(NodeId, AdapterInstanceId); 2],
    prepared: [Option<u64>; 2],
    route_id: RouteId,
    failed: bool,
    pub(crate) payload_enabled: bool,
    pub(crate) batch_enabled: bool,
    endpoint: Option<Endpoint>,
    pub(crate) receipt: Option<Receipt>,
}

impl MockRoutes {
    pub(crate) fn compose(runtime: &mut NodeRuntime, directory: &Path) -> io::Result<Self> {
        let local = runtime.snapshot().local_node.id;
        let peer = PEER.parse::<NodeId>().map_err(error)?;
        if local == peer {
            return Err(error("mock peer identity collides with local identity"));
        }
        let mut candidate = runtime.clone();
        candidate
            .register_peer(
                NodeDescriptor::new(
                    peer,
                    "Synthetic Linux Mock Peer (no network)",
                    Platform::Linux,
                    "synthetic",
                    env!("CARGO_PKG_VERSION"),
                    [ProtocolVersion::new(1, 0)],
                ),
                true,
            )
            .map_err(error)?;
        let mut children = Vec::new();
        let mut endpoints = Vec::new();
        let owners = [
            (local, SOURCE.parse().map_err(error)?),
            (peer, SINK.parse().map_err(error)?),
        ];
        for (index, (name, manifest_bytes, direction)) in [
            (
                "capyio-mock-source",
                include_bytes!("../../../../adapters/mock-source/adapter.json").as_slice(),
                PortDirection::Source,
            ),
            (
                "capyio-mock-sink",
                include_bytes!("../../../../adapters/mock-sink/adapter.json").as_slice(),
                PortDirection::Sink,
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let manifest = AdapterManifest::from_json(manifest_bytes).map_err(error)?;
            let mut child =
                SidecarHost::spawn(&directory.join(name), std::iter::empty::<&str>(), manifest)
                    .map_err(error)?;
            let (node_id, adapter_id) = owners[index];
            let initialized = child.initialize(adapter_id).map_err(error)?;
            if initialized.adapter_id != child.manifest().id || !child.probe().map_err(error)?.ready
            {
                return Err(error("mock initialize/probe mismatch"));
            }
            let catalog = child.catalog().map_err(error)?;
            if catalog.adapter_id != child.manifest().id || catalog.capabilities.len() != 1 {
                return Err(error("mock catalog identity mismatch"));
            }
            let capability = &catalog.capabilities[0];
            if capability.adapter_instance_id != adapter_id
                || capability.metadata.get("test_only").map(String::as_str) != Some("true")
                || capability.ports.len() != 1
            {
                return Err(error("mock catalog ownership/label mismatch"));
            }
            let port = capability.ports.values().next().expect("one port");
            if port.direction != direction {
                return Err(error("mock catalog direction mismatch"));
            }
            endpoints.push(PortRef {
                node_id,
                capability_id: capability.id,
                port_id: port.id,
            });
            candidate
                .register_adapter_catalog(
                    node_id,
                    AdapterInstanceDescriptor {
                        id: adapter_id,
                        adapter_type: catalog.adapter_id,
                        display_name: initialized.adapter_name,
                        deployment_mode: AdapterDeploymentMode::Sidecar,
                        version: child.manifest().version.clone(),
                        state: AdapterState::Ready,
                        health: AdapterHealth::Healthy,
                        owned_capabilities: BTreeSet::new(),
                        supported_route_modes: BTreeSet::from([RouteBackend::CapyDataPlane]),
                    },
                    catalog.capabilities,
                )
                .map_err(error)?;
            children.push(Box::new(child) as Box<dyn Control>);
        }
        let session = candidate
            .open_session_with_id(SESSION.parse().map_err(error)?, peer)
            .map_err(error)?;
        let route_id = candidate
            .create_route_with_id(
                ROUTE.parse().map_err(error)?,
                session,
                endpoints[0],
                endpoints[1],
                RouteBackend::CapyDataPlane,
            )
            .map_err(error)?;
        *runtime = candidate;
        Ok(Self {
            children,
            owners,
            prepared: [None; 2],
            route_id,
            failed: false,
            payload_enabled: false,
            batch_enabled: false,
            endpoint: None,
            receipt: None,
        })
    }

    pub(crate) fn inject_source_crash(&mut self) -> io::Result<()> {
        if self.failed {
            return Err(error("mock lifecycle already failed"));
        }
        self.children[0].inject_crash()
    }

    pub(crate) fn reconcile(&mut self, runtime: &mut NodeRuntime) {
        if self.failed {
            return;
        }
        let Ok(route) = runtime.route(self.route_id) else {
            return;
        };
        let (expected_state, expected_epoch) = if route.state == RouteState::Active {
            (RouteState::Active, route.epoch)
        } else {
            (RouteState::Stopped, 0)
        };
        for index in 0..2 {
            let status = self.children[index].status(RouteStatusRequest {
                route_id: self.route_id,
            });
            if !matches!(status, Ok(status) if status.route_id == self.route_id && status.epoch == expected_epoch && status.state == expected_state)
            {
                self.fail(runtime, index);
                return;
            }
        }
    }

    pub(crate) fn start(&mut self, runtime: &mut NodeRuntime) -> io::Result<()> {
        if self.failed {
            return Err(error("mock lifecycle failed; restart host"));
        }
        if runtime.route(self.route_id).map_err(error)?.state == RouteState::Active {
            self.reconcile(runtime);
            return if self.failed {
                Err(error("mock lifecycle failed; restart host"))
            } else {
                Ok(())
            };
        }
        // Synthetic consent exists only in this explicit mock composition.
        let route = runtime.route(self.route_id).map_err(error)?.clone();
        if route.state == RouteState::Draft {
            runtime
                .authorize_route(self.route_id, None)
                .map_err(error)?;
        }
        runtime
            .prepare_route(
                self.route_id,
                route.compatible_formats.first().cloned(),
                route
                    .compatible_qos_modes
                    .first()
                    .ok_or_else(|| error("missing QoS"))?
                    .clone(),
                0,
            )
            .map_err(error)?;
        runtime.begin_route_start(self.route_id, 0).map_err(error)?;
        let route = runtime.route(self.route_id).map_err(error)?.clone();
        if self.payload_enabled || self.batch_enabled {
            return self.start_payload(runtime);
        }
        for index in 0..2 {
            let result = (|| {
                self.children[index].prepare(RoutePrepareRequest {
                    route_id: self.route_id,
                    source: route.source,
                    sink: route.sink,
                    profile: route.profile.clone(),
                    selected_format: route.selected_format.clone(),
                    selected_qos: route.selected_qos_mode.clone().expect("prepared QoS"),
                    backend: route.backend,
                    epoch: route.epoch,
                    data_endpoint: None,
                    adapter_config: BoundedJsonObject::default(),
                })?;
                self.prepared[index] = Some(route.epoch);
                self.children[index].start(RouteStartRequest {
                    route_id: self.route_id,
                    epoch: route.epoch,
                })?;
                let status = self.children[index].status(RouteStatusRequest {
                    route_id: self.route_id,
                })?;
                validate_active(&status, self.route_id, route.epoch)
            })();
            if result.is_err() {
                self.fail(runtime, index);
                return Err(error("mock_start_failed"));
            }
        }
        runtime.activate_route(self.route_id).map_err(error)
    }

    fn start_payload(&mut self, runtime: &mut NodeRuntime) -> io::Result<()> {
        let route = runtime.route(self.route_id).map_err(error)?.clone();
        self.receipt = None;
        let endpoint = match if self.batch_enabled {
            Endpoint::create_batch(capyio_core::StreamId::new())
        } else {
            Endpoint::create(capyio_core::StreamId::new())
        } {
            Ok(value) => value,
            Err(err) => {
                self.fail(runtime, 0);
                return Err(err);
            }
        };
        let descriptor = endpoint.descriptor.clone();
        self.endpoint = Some(endpoint);
        // Bind Sink before Source can send its one finite datagram.
        for index in [1, 0] {
            let result = self.children[index].prepare(RoutePrepareRequest {
                route_id: self.route_id,
                source: route.source,
                sink: route.sink,
                profile: route.profile.clone(),
                selected_format: route.selected_format.clone(),
                selected_qos: route.selected_qos_mode.clone().expect("prepared QoS"),
                backend: route.backend,
                epoch: route.epoch,
                data_endpoint: Some(descriptor.clone()),
                adapter_config: BoundedJsonObject::default(),
            });
            if result.is_err() {
                self.fail(runtime, index);
                return Err(error("mock payload prepare failed"));
            }
            self.prepared[index] = Some(route.epoch);
        }
        for index in 0..2 {
            let result = (|| {
                let returned = self.children[index].start(RouteStartRequest {
                    route_id: self.route_id,
                    epoch: route.epoch,
                })?;
                if index == 1 {
                    self.receipt = Some(validate_receipt(
                        returned,
                        &descriptor,
                        self.route_id,
                        route.epoch,
                    )?);
                }
                let status = self.children[index].status(RouteStatusRequest {
                    route_id: self.route_id,
                })?;
                validate_active(&status, self.route_id, route.epoch)
            })();
            if result.is_err() {
                self.fail(runtime, index);
                return Err(error("mock payload transfer failed"));
            }
        }
        if let Err(err) = runtime.activate_route(self.route_id) {
            self.fail(runtime, 1);
            return Err(error(err));
        }
        Ok(())
    }

    fn fail(&mut self, runtime: &mut NodeRuntime, index: usize) {
        self.failed = true;
        // Stop both prepared participants even when one has already exited.
        for other in 0..2 {
            if let Some(epoch) = self.prepared[other].take() {
                let _ = self.children[other].stop(RouteStopRequest {
                    route_id: self.route_id,
                    epoch,
                });
            }
        }
        self.children.clear();
        self.receipt = None;
        self.endpoint = None; // Close children before removing owned IPC.
        // Drop kills/reaps both children after best-effort Stop.
        for (other, (node, adapter)) in self.owners.into_iter().enumerate() {
            let code = if other == index {
                "CAPY.LINUX.MOCK_LIFECYCLE_FAILED"
            } else {
                "CAPY.LINUX.MOCK_PAIR_CLOSED"
            };
            let _ = runtime.fail_adapter(node, adapter, code);
        }
    }

    pub(crate) fn stop(&mut self, runtime: &mut NodeRuntime) -> io::Result<()> {
        let state = runtime.route(self.route_id).map_err(error)?.state;
        if matches!(
            state,
            RouteState::Draft | RouteState::Stopped | RouteState::Failed
        ) {
            return Ok(());
        }
        runtime.begin_route_stop(self.route_id).map_err(error)?;
        for index in 0..2 {
            if let Some(epoch) = self.prepared[index] {
                if self.children[index]
                    .stop(RouteStopRequest {
                        route_id: self.route_id,
                        epoch,
                    })
                    .is_err()
                {
                    self.fail(runtime, index);
                    return Err(error("mock_stop_failed"));
                }
                self.prepared[index] = None;
            }
        }
        self.receipt = None;
        self.endpoint = None;
        runtime.stop_route(self.route_id).map_err(error)
    }
}

fn validate_active(status: &RouteStatusResult, route_id: RouteId, epoch: u64) -> io::Result<()> {
    if status.route_id != route_id || status.epoch != epoch || status.state != RouteState::Active {
        return Err(error("mock status binding/state mismatch"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use capyio_testkit::DemoLab;
    use std::{cell::RefCell, rc::Rc};

    #[derive(Clone, Copy, Default)]
    enum Fault {
        #[default]
        None,
        Start,
        Prepare,
        WrongId,
        WrongEpoch,
        Stop,
    }
    struct Fake {
        calls: Rc<RefCell<Vec<String>>>,
        fault: Fault,
        epoch: u64,
        data_endpoint: Option<DataEndpointDescriptor>,
        valid_receipt: bool,
    }
    impl Drop for Fake {
        fn drop(&mut self) {
            self.calls.borrow_mut().push("drop".into());
        }
    }
    impl Control for Fake {
        fn inject_crash(&mut self) -> io::Result<()> {
            Err(error("not used"))
        }
        fn prepare(&mut self, request: RoutePrepareRequest) -> io::Result<()> {
            self.calls.borrow_mut().push("prepare".into());
            self.epoch = request.epoch;
            self.data_endpoint = request.data_endpoint;
            if let Some(endpoint) = &self.data_endpoint {
                self.calls
                    .borrow_mut()
                    .push(format!("endpoint={}", endpoint.address));
            }
            if matches!(self.fault, Fault::Prepare) {
                Err(error("injected"))
            } else {
                Ok(())
            }
        }
        fn start(
            &mut self,
            request: RouteStartRequest,
        ) -> io::Result<Option<DataEndpointDescriptor>> {
            self.calls.borrow_mut().push("start".into());
            if matches!(self.fault, Fault::Start) {
                Err(error("injected"))
            } else if self.valid_receipt {
                let mut endpoint = self.data_endpoint.clone().expect("payload endpoint");
                let batch =
                    endpoint.transport == capyio_mock_sidecar_support::payload::BATCH_TRANSPORT;
                let mut digest = Vec::new();
                if batch {
                    for sequence in 0u64..4 {
                        digest.extend_from_slice(&sequence.to_le_bytes());
                        digest.extend_from_slice(
                            format!("finite-batch-sample-{sequence}").as_bytes(),
                        );
                    }
                } else {
                    digest.extend_from_slice(b"finite-source-sample");
                }
                let hash = digest.iter().fold(0xcbf29ce484222325u64, |hash, byte| {
                    (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
                });
                let receipt = Receipt {
                    route_id: request.route_id,
                    stream_id: endpoint.metadata["stream_id"].parse().unwrap(),
                    epoch: request.epoch,
                    received: if batch { 4 } else { 1 },
                    payload_bytes: if batch {
                        4 * b"finite-batch-sample-0".len()
                    } else {
                        b"finite-source-sample".len()
                    },
                    payload_checksum: format!("fnv1a64:{hash:016x}"),
                    first_sequence: batch.then_some(0),
                    last_sequence: batch.then_some(3),
                };
                endpoint
                    .metadata
                    .insert("receipt".into(), serde_json::to_string(&receipt).unwrap());
                Ok(Some(endpoint))
            } else {
                Ok(None)
            }
        }
        fn status(&mut self, request: RouteStatusRequest) -> io::Result<RouteStatusResult> {
            Ok(RouteStatusResult {
                route_id: if matches!(self.fault, Fault::WrongId) {
                    RouteId::new()
                } else {
                    request.route_id
                },
                epoch: self.epoch + u64::from(matches!(self.fault, Fault::WrongEpoch)),
                state: RouteState::Active,
                warnings: vec![],
            })
        }
        fn stop(&mut self, _: RouteStopRequest) -> io::Result<()> {
            self.calls.borrow_mut().push("stop".into());
            if matches!(self.fault, Fault::Stop) {
                Err(error("injected"))
            } else {
                Ok(())
            }
        }
    }
    type Calls = [Rc<RefCell<Vec<String>>>; 2];
    fn fixture(fault: Fault) -> (NodeRuntime, MockRoutes, Calls) {
        fixture_faults([Fault::None, fault], false)
    }
    fn fixture_faults(faults: [Fault; 2], valid_receipt: bool) -> (NodeRuntime, MockRoutes, Calls) {
        let lab = DemoLab::new().unwrap();
        let route_id = lab.routes.all()[0];
        let route = lab.runtime.route(route_id).unwrap();
        let snapshot = lab.runtime.snapshot();
        let owners = [route.source, route.sink].map(|endpoint| {
            let node = std::iter::once(&snapshot.local_node)
                .chain(snapshot.peers.iter())
                .find(|node| node.id == endpoint.node_id)
                .unwrap();
            (
                node.id,
                node.capabilities[&endpoint.capability_id].adapter_instance_id,
            )
        });
        let calls = [
            Rc::new(RefCell::new(Vec::new())),
            Rc::new(RefCell::new(Vec::new())),
        ];
        let children = vec![
            Box::new(Fake {
                calls: calls[0].clone(),
                fault: faults[0],
                epoch: 0,
                data_endpoint: None,
                valid_receipt,
            }) as Box<dyn Control>,
            Box::new(Fake {
                calls: calls[1].clone(),
                fault: faults[1],
                epoch: 0,
                data_endpoint: None,
                valid_receipt,
            }) as Box<dyn Control>,
        ];
        (
            lab.runtime,
            MockRoutes {
                children,
                owners,
                prepared: [None; 2],
                route_id,
                failed: false,
                payload_enabled: false,
                batch_enabled: false,
                endpoint: None,
                receipt: None,
            },
            calls,
        )
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn payload_faults_drop_both_children_and_remove_owned_directory() {
        assert_transfer_fault_cleanup(false);
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn batch_faults_drop_both_children_and_remove_owned_directory() {
        assert_transfer_fault_cleanup(true);
    }
    #[cfg(target_os = "linux")]
    fn assert_transfer_fault_cleanup(batch: bool) {
        for fault in [
            Fault::Prepare,
            Fault::Start,
            Fault::WrongId,
            Fault::WrongEpoch,
            Fault::Stop,
        ] {
            for side in 0..2 {
                let mut faults = [Fault::None; 2];
                faults[side] = fault;
                let (mut runtime, mut mocks, calls) = fixture_faults(faults, true);
                mocks.payload_enabled = !batch;
                mocks.batch_enabled = batch;
                if matches!(fault, Fault::Stop) {
                    mocks.start(&mut runtime).unwrap();
                    assert!(mocks.receipt.is_some());
                    assert!(mocks.stop(&mut runtime).is_err());
                } else {
                    assert!(mocks.start(&mut runtime).is_err());
                }
                assert_eq!(
                    runtime.route(mocks.route_id).unwrap().state,
                    RouteState::Failed
                );
                assert!(mocks.receipt.is_none());
                assert!(mocks.endpoint.is_none());
                assert!(mocks.children.is_empty());
                let mut observed_directory = false;
                for log in calls {
                    let log = log.borrow();
                    assert_eq!(log.last().unwrap(), "drop");
                    for call in log.iter() {
                        if let Some(path) = call.strip_prefix("endpoint=") {
                            observed_directory = true;
                            assert!(!std::path::Path::new(path).parent().unwrap().exists());
                        }
                    }
                }
                assert!(observed_directory);
            }
        }
    }
    #[test]
    fn payload_missing_receipt_fails_and_clears_owned_pair() {
        let (mut runtime, mut mocks, calls) = fixture(Fault::None);
        mocks.payload_enabled = true;
        // Fake children perform no socket I/O and return no consumed-data receipt.
        assert!(mocks.start(&mut runtime).is_err());
        assert_eq!(
            runtime.route(mocks.route_id).unwrap().state,
            RouteState::Failed
        );
        assert!(mocks.receipt.is_none());
        assert!(mocks.endpoint.is_none());
        assert!(mocks.children.is_empty());
        for log in calls {
            assert_eq!(log.borrow().last().unwrap(), "drop");
        }
    }
    #[test]
    fn start_stop_restart_is_staged_and_idempotent() {
        let (mut runtime, mut mocks, calls) = fixture(Fault::None);
        mocks.start(&mut runtime).unwrap();
        let epoch = runtime.route(mocks.route_id).unwrap().epoch;
        mocks.start(&mut runtime).unwrap();
        assert_eq!(runtime.route(mocks.route_id).unwrap().epoch, epoch);
        assert_eq!(&*calls[0].borrow(), &["prepare", "start"]);
        mocks.stop(&mut runtime).unwrap();
        mocks.stop(&mut runtime).unwrap();
        mocks.start(&mut runtime).unwrap();
        assert!(runtime.route(mocks.route_id).unwrap().epoch > epoch);
        let states: Vec<_> = runtime
            .snapshot()
            .events
            .into_iter()
            .filter_map(|event| match event.kind {
                capyio_runtime::RuntimeEventKind::RouteChanged { route_id, state }
                    if route_id == mocks.route_id =>
                {
                    Some(state)
                }
                _ => None,
            })
            .collect();
        assert!(states.windows(5).any(|states| states
            == [
                RouteState::Prepared,
                RouteState::Starting,
                RouteState::Active,
                RouteState::Stopping,
                RouteState::Stopped
            ]));
    }
    #[test]
    fn partial_start_and_status_binding_failures_stop_and_drop_both_children() {
        for fault in [Fault::Start, Fault::WrongId, Fault::WrongEpoch] {
            let (mut runtime, mut mocks, calls) = fixture(fault);
            assert!(mocks.start(&mut runtime).is_err());
            assert_eq!(
                runtime.route(mocks.route_id).unwrap().state,
                RouteState::Failed
            );
            for log in &calls {
                assert_eq!(&*log.borrow(), &["prepare", "start", "stop", "drop"]);
            }
            assert!(mocks.children.is_empty());
            assert!(mocks.start(&mut runtime).is_err());
        }
    }
    #[test]
    fn stop_failure_drops_children_and_preserves_failure() {
        let (mut runtime, mut mocks, calls) = fixture(Fault::Stop);
        mocks.start(&mut runtime).unwrap();
        assert!(mocks.stop(&mut runtime).is_err());
        assert_eq!(
            runtime.route(mocks.route_id).unwrap().state,
            RouteState::Failed
        );
        assert!(
            calls
                .iter()
                .all(|log| log.borrow().last().unwrap() == "drop")
        );
    }
    #[test]
    fn partial_prepare_stops_first_participant_and_drops_both() {
        let (mut runtime, mut mocks, calls) = fixture(Fault::Prepare);
        assert!(mocks.start(&mut runtime).is_err());
        assert_eq!(&*calls[0].borrow(), &["prepare", "start", "stop", "drop"]);
        assert_eq!(&*calls[1].borrow(), &["prepare", "drop"]);
    }

    #[test]
    fn output_failure_stops_and_drops_owned_participants() {
        struct BrokenOutput;
        impl std::io::Write for BrokenOutput {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(error("injected broken output"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let (mut runtime, mut mocks, calls) = fixture(Fault::None);
        mocks.start(&mut runtime).unwrap();
        let host = crate::LinuxNodeHost {
            runtime,
            mocks: Some(mocks),
        };
        assert!(
            host.serve(
                std::io::Cursor::new(b"{\"version\":1,\"id\":1,\"command\":\"get_snapshot\"}\n"),
                BrokenOutput
            )
            .is_err()
        );
        for log in calls {
            assert_eq!(&*log.borrow(), &["prepare", "start", "stop", "drop"]);
        }
    }
}
