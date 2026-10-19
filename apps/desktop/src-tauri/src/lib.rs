use std::{
    collections::{BTreeMap, BTreeSet},
    io::ErrorKind,
    net::{IpAddr, SocketAddr, TcpListener},
    str::FromStr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use capyio_control_host::project_publication_catalog;
use capyio_control_link::{CatalogControlLink, ReceivedControlMessage};
use capyio_control_tls::{
    AuthenticatedControlClientChannel, AuthenticatedControlServer,
    AuthenticatedControlServerChannel, ControlTlsClientIdentity, ControlTlsEndpoint,
    ControlTlsError, ControlTlsNodeBinding, ControlTlsServerIdentity,
};
use capyio_core::{
    AdapterDeploymentMode, AuthorizationState, Availability, CapabilityClass, CapabilityId,
    MessageId, NodeDescriptor, NodeId, OnlineState, PermissionRequirement, PortDirection, PortRef,
    QosMode, Route, RouteBackend, RouteId, RouteState, SessionId,
};
use capyio_data_plane::{
    BoundedFanout, BoundedJsonlRecorder, ImuSampleV1, NumericImuPanel, RecorderOutcome,
    parse_imu_fixture_jsonl,
};
use capyio_discovery::DiscoveryIngress;
use capyio_discovery_udp::{DiscoveryReceive, DiscoveryUdpSocket};
use capyio_pairing::{CandidateTrust, PairedControlCandidate, PairedPeerDirectory, PairingState};
use capyio_protocol::{PortLeaseAction, PortLeaseOutcome, RouteCommandAction};
use capyio_runtime::NodeRuntime;
use capyio_sensor_server_adapter::{
    AssembleOutcome, SensorKind, SensorServerConnectionConfig, SensorServerEndpoint,
    SensorServerImuAssembler, SensorServerReadOutcome, SensorServerReading,
    SensorServerWebSocketClient,
};
use capyio_testkit::{DemoLab, DemoRoutes};
use capyio_windows_pairing_store::WindowsPairingStore;
use serde::{Deserialize, Serialize};

mod android_gamepad;
mod android_receivers;
pub mod audio_share_runtime;
#[cfg(windows)]
mod background_host;
mod camera_route;
mod camera_session;
mod control_exchange;
mod control_worker;
mod controller_control;
mod controller_permission;
mod controller_settings;
mod controller_workflow;
mod desktop_host;
mod desktop_identity;
mod diagnostics;
mod display_control;
mod display_workflow;
mod fileless_pairing;
mod gamepad_lab;
mod haptics_data;
mod host_commands;
mod input_routes;
mod media_lab_endpoints;
mod microphone_quick_action;
pub mod micyou_runtime;
mod native_audio_route;
mod network_interfaces;
#[cfg(windows)]
mod node_process;
mod paired_control_listener;
mod pairing_enrollment;
mod physical_imu_runtime;
mod quick_actions;
mod remote_resource_contract;
mod route_port_leases;
mod route_readiness;
mod route_status_inbox;
mod route_use_coordinator;
mod touchpad_data;
mod touchpad_lease;
mod windows_gamepad_host;

use gamepad_lab::{GamepadLab, UiGamepadState, UpdateGamepadRequest};
use microphone_quick_action::{MICYOU_ACTION_ID, MicrophoneQuickAction};
use physical_imu_runtime::PhysicalImuRoute;
use quick_actions::{
    AudioShareQuickAction, InvokeQuickActionRequest, SelectAudioEndpointRequest,
    UiAudioEndpointCatalog, UiQuickAction,
};
use windows_gamepad_host::WindowsGamepadHost;

#[cfg(windows)]
pub fn run_camera_broker_host(requesting_user_sid: &str) -> Result<(), String> {
    use capyio_windows_service::{
        CameraBrokerRuntime, camera_control_server_loop_until_stopped_for_user,
    };

    if !camera_session::process_is_elevated()? {
        return Err("camera Broker host requires an elevated process token".to_owned());
    }
    let runtime = Arc::new(Mutex::new(CameraBrokerRuntime::new(
        android_receivers::LocalSystemCameraProcess::new(),
    )));
    camera_control_server_loop_until_stopped_for_user(
        runtime,
        Arc::new(AtomicBool::new(false)),
        requesting_user_sid,
    )
}

/// Headless counterpart of the Settings action for lab automation and
/// accessibility tooling. It prints/returns only the invitation path.
pub fn create_pairing_invitation_cli(
    target_node_id: &str,
    target_display_name: &str,
) -> Result<std::path::PathBuf, String> {
    pairing_invitation_cli(target_node_id, target_display_name, false)
}

/// Explicit owner-requested recovery. Uses the same old-credential DPAPI check
/// as Settings; it never adopts a certificate observed on the network.
pub fn renew_pairing_invitation_cli(
    target_node_id: &str,
    target_display_name: &str,
) -> Result<std::path::PathBuf, String> {
    pairing_invitation_cli(target_node_id, target_display_name, true)
}

fn pairing_invitation_cli(
    target_node_id: &str,
    target_display_name: &str,
    renew: bool,
) -> Result<std::path::PathBuf, String> {
    let node = DesktopNodeState::production()?;
    let snapshot = node.runtime.snapshot();
    let local_node_id = snapshot.local_node.id;
    let mut control = DesktopControlPlaneState::new(local_node_id)?;
    control.attach_default_pairing_store(local_node_id);
    pairing_enrollment::create_invitation(
        &mut control,
        local_node_id,
        &snapshot.local_node.display_name,
        NodeId::from_str(target_node_id)
            .map_err(|_| "target Node ID must be a canonical UUID".to_owned())?,
        target_display_name,
        renew,
    )
}

/// Performs one bounded authenticated catalog exchange and heartbeat without
/// starting any hardware Route. Intended for installer/lab diagnostics.
pub fn verify_paired_control_endpoint_cli(
    target_node_id: &str,
    host: &str,
    port: u16,
) -> Result<usize, String> {
    let peer_id = NodeId::from_str(target_node_id)
        .map_err(|_| "target Node ID must be a canonical UUID".to_owned())?;
    let address = SocketAddr::new(
        IpAddr::from_str(host).map_err(|_| "control host must be a numeric IP".to_owned())?,
        port,
    );
    if port == 0 || address.ip().is_unspecified() || address.ip().is_multicast() {
        return Err("control endpoint is invalid".to_owned());
    }
    let mut node = DesktopNodeState::production()?;
    let local_node_id = node.runtime.snapshot().local_node.id;
    let mut control = DesktopControlPlaneState::new(local_node_id)?;
    control.attach_default_pairing_store(local_node_id);
    let plan = control.resolve_manual_control_dial(peer_id, address)?;
    let channel = connect_authenticated_control_channel(peer_id, local_node_id, &plan)?;
    let mut link =
        CatalogControlLink::admit_dialed_peer(&mut node.runtime, channel, plan.display_name)
            .map_err(|error| error.to_string())?;
    let prepared = link
        .prepare_catalog(&node.runtime, &BTreeSet::new())
        .map_err(|error| error.to_string())?;
    link.send_catalog(prepared)
        .map_err(|error| error.to_string())?;
    let catalog = link.receive_catalog().map_err(|error| error.to_string())?;
    link.apply_catalog(&mut node.runtime, catalog)
        .map_err(|error| error.to_string())?;
    link.send_heartbeat(1, false)
        .map_err(|error| error.to_string())?;
    receive_probe_heartbeat(|| {
        link.receive_control_message()
            .map_err(|error| error.to_string())
    })?;
    let count = node
        .runtime
        .snapshot()
        .peers
        .iter()
        .find(|peer| peer.id == peer_id)
        .map(|peer| peer.capabilities.len())
        .ok_or_else(|| "authenticated peer catalog is absent".to_owned())?;
    link.close_and_reconcile(&mut node.runtime)
        .map_err(|error| error.to_string())?;
    Ok(count)
}

/// One bounded inbound control diagnostic for a specifically paired peer.
/// It never starts a hardware Route and does not own the desktop app lifecycle.
pub fn verify_paired_control_listener_cli(
    target_node_id: &str,
    bind_host: &str,
    port: u16,
    on_listening: impl FnOnce(SocketAddr),
) -> Result<usize, String> {
    paired_control_listener::verify_once(target_node_id, bind_host, port, on_listening)
}

/// Physical alpha gate for the unified remote-resource action. It performs the
/// same authenticated catalog, Route proposal, owner lease, Start, Stop and
/// release sequence as the desktop UI, but never configures a feature data
/// endpoint or installs a driver.
pub fn verify_paired_route_endpoint_cli(
    target_node_id: &str,
    host: &str,
    port: u16,
    capability_id: &str,
) -> Result<RouteId, String> {
    verify_paired_route_endpoint(target_node_id, host, port, capability_id, false, 0)
}

/// Bounded consent gate; waits for the real owner UI and never starts a data Adapter.
pub fn verify_paired_touchpad_consent_cli(
    target_node_id: &str,
    host: &str,
    port: u16,
) -> Result<RouteId, String> {
    verify_paired_route_endpoint(target_node_id, host, port, "touchpad", true, 0)
}

/// Explicit physical gate; real input opens only after local preflight and
/// the tablet owner's visible consent. No driver deployment is performed.
pub fn verify_paired_touchpad_data_cli(
    node: &str,
    host: &str,
    port: u16,
) -> Result<RouteId, String> {
    verify_paired_route_endpoint(node, host, port, "touchpad", true, 15)
}

/// Fixed-duration physical renewal gate, longer than the original five-minute lease.
pub fn verify_paired_touchpad_soak_cli(
    node: &str,
    host: &str,
    port: u16,
) -> Result<RouteId, String> {
    verify_paired_route_endpoint(node, host, port, "touchpad", true, 370)
}

fn verify_paired_route_endpoint(
    target_node_id: &str,
    host: &str,
    port: u16,
    capability_id: &str,
    wait_for_consent: bool,
    data_seconds: u64,
) -> Result<RouteId, String> {
    let real_data = data_seconds != 0;
    let peer_id = NodeId::from_str(target_node_id)
        .map_err(|_| "target Node ID must be a canonical UUID".to_owned())?;
    let capability_selector = capability_id.to_owned();
    let address = SocketAddr::new(
        IpAddr::from_str(host).map_err(|_| "control host must be a numeric IP".to_owned())?,
        port,
    );
    if port == 0 || address.ip().is_unspecified() || address.ip().is_multicast() {
        return Err("control endpoint is invalid".to_owned());
    }
    let mut node = DesktopNodeState::production()?;
    if real_data {
        touchpad_data::preflight_projection()?;
        touchpad_data::install_catalog(&mut node.runtime)?;
    }
    if wait_for_consent && !real_data {
        // Diagnostic-only simulated Sink; never changes ordinary desktop inventory.
        let mut catalog = node.runtime.snapshot().local_node;
        let capability = catalog
            .capabilities
            .values_mut()
            .find(|c| c.class == CapabilityClass::Touchpad)
            .ok_or("touchpad consent fixture missing")?;
        for port in capability.ports.values_mut() {
            port.interoperability_mode = capyio_core::InteroperabilityMode::AdapterManaged;
        }
        catalog
            .adapter_instances
            .get_mut(&capability.adapter_instance_id)
            .ok_or("fixture adapter missing")?
            .supported_route_modes
            .insert(RouteBackend::AdapterManaged);
        node.runtime
            .replace_local_catalog(catalog)
            .map_err(|e| e.to_string())?;
        println!("CAPYIO_CONSENT_FIXTURE sink=simulated input_injected=false");
    }
    let local_node_id = node.runtime.snapshot().local_node.id;
    let mut control = DesktopControlPlaneState::new(local_node_id)?;
    control.attach_default_pairing_store(local_node_id);
    let plan = control.resolve_manual_control_dial(peer_id, address)?;
    let bundle = plan
        .store
        .load_control_credential(&plan.candidate.credential_handle)
        .map_err(|error| error.to_string())?;
    let identity =
        ControlTlsClientIdentity::from_bundle(plan.candidate.peer_certificate_sha256, bundle);
    let channel = AuthenticatedControlClientChannel::connect_fresh_session(
        ControlTlsEndpoint {
            address,
            io_timeout: Duration::from_secs(5),
        },
        &plan.candidate.server_name,
        identity,
        ControlTlsNodeBinding::new(peer_id, local_node_id).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let node = Arc::new(Mutex::new(node));
    let mut link = {
        let mut node_guard = node
            .lock()
            .map_err(|_| "Node state lock poisoned".to_owned())?;
        CatalogControlLink::admit_dialed_peer(
            &mut node_guard.runtime,
            channel,
            plan.display_name.clone(),
        )
        .map_err(|error| error.to_string())?
    };
    let prepared = {
        let node_guard = node
            .lock()
            .map_err(|_| "Node state lock poisoned".to_owned())?;
        link.prepare_catalog(&node_guard.runtime, &BTreeSet::new())
            .map_err(|error| error.to_string())?
    };
    link.send_catalog(prepared)
        .map_err(|error| error.to_string())?;
    let catalog = link.receive_catalog().map_err(|error| error.to_string())?;
    {
        let mut node_guard = node
            .lock()
            .map_err(|_| "Node state lock poisoned".to_owned())?;
        link.apply_catalog(&mut node_guard.runtime, catalog)
            .map_err(|error| error.to_string())?;
    }
    let capability_id = {
        let node_guard = node
            .lock()
            .map_err(|_| "Node state lock poisoned".to_owned())?;
        let peer = node_guard
            .runtime
            .snapshot()
            .peers
            .into_iter()
            .find(|peer| peer.id == peer_id)
            .ok_or_else(|| "authenticated peer catalog is absent".to_owned())?;
        CapabilityId::from_str(&capability_selector)
            .ok()
            .or_else(|| {
                peer.capabilities.values().find_map(|capability| {
                    (capability_class_label(&capability.class)
                        .eq_ignore_ascii_case(&capability_selector)
                        || capability
                            .display_name
                            .eq_ignore_ascii_case(&capability_selector))
                    .then_some(capability.id)
                })
            })
            .ok_or_else(|| "Capability selector is absent from the peer catalog".to_owned())?
    };
    let deadline = Instant::now() + Duration::from_secs(45);
    let (route_id, remote_port, worker) = loop {
        match start_remote_capability_route_with_data(
            &mut link,
            &node,
            capability_id,
            real_data.then_some(&plan),
            None,
            None,
        ) {
            Ok(Some(started)) => break started,
            Ok(None) => {
                if !wait_for_consent || Instant::now() >= deadline {
                    return Err("等待本机资源所有者确认超时；未启动输入".into());
                }
                let pending_route = node
                    .lock()
                    .map_err(|_| "Node state lock poisoned".to_owned())?
                    .runtime
                    .snapshot()
                    .routes
                    .into_iter()
                    .find(|route| {
                        route.session_id == link.session_id()
                            && route.source.capability_id == capability_id
                            && route.state == RouteState::Draft
                            && route.authorization == AuthorizationState::Pending
                    })
                    .ok_or("pending resource request disappeared")?;
                println!(
                    "CAPYIO_RESOURCE_CONSENT_PENDING route={} input_injected=false",
                    pending_route.id
                );
                {
                    let mut stdout = std::io::stdout();
                    let _ = std::io::Write::flush(&mut stdout);
                }
                thread::sleep(Duration::from_secs(1));
            }
            Err(error) => return Err(error),
        }
    };
    if real_data {
        if worker.is_none() {
            return Err("peer did not negotiate a real touchpad data connection".into());
        }
        println!("CAPYIO_TOUCHPAD_DATA_READY transport=mtls driver_installed=false");
        for sequence in 1..=data_seconds {
            thread::sleep(Duration::from_secs(1));
            if worker.as_ref().is_none_or(|worker| worker.is_finished()) {
                return Err("触控板数据接收器提前退出；不能判定实机验证通过".into());
            }
            if sequence % touchpad_lease::RENEW_INTERVAL.as_secs() == 0 {
                touchpad_lease::renew(&mut link, &node, route_id, remote_port)?;
                println!("CAPYIO_TOUCHPAD_LEASE_RENEWED elapsed_seconds={sequence}");
            }
            link.send_heartbeat(sequence, false)
                .map_err(|e| e.to_string())?;
            loop {
                match link.receive_control_message().map_err(|e| e.to_string())? {
                    ReceivedControlMessage::Heartbeat(heartbeat)
                        if heartbeat.acknowledgement && heartbeat.sequence == sequence =>
                    {
                        break;
                    }
                    ReceivedControlMessage::Catalog(catalog) => {
                        let mut node = node.lock().map_err(|_| "Node lock poisoned")?;
                        link.apply_catalog(&mut node.runtime, catalog)
                            .map_err(|e| e.to_string())?;
                    }
                    ReceivedControlMessage::Heartbeat(heartbeat) if !heartbeat.acknowledgement => {
                        link.send_heartbeat(heartbeat.sequence, true)
                            .map_err(|e| e.to_string())?;
                    }
                    _ => return Err("unexpected physical gate control response".into()),
                }
            }
        }
    }
    stop_remote_capability_route(&mut link, &node, route_id, remote_port)?;
    drop(worker);
    {
        let mut node_guard = node
            .lock()
            .map_err(|_| "Node state lock poisoned".to_owned())?;
        link.close_and_reconcile(&mut node_guard.runtime)
            .map_err(|error| error.to_string())?;
    }
    Ok(route_id)
}

#[cfg(not(windows))]
pub fn run_camera_broker_host(_requesting_user_sid: &str) -> Result<(), String> {
    Err("camera Broker host requires Windows".to_owned())
}

struct HostState(Arc<desktop_host::DesktopHost>);

/// Process-owned Node state. `DemoLab` is only a deterministic bootstrap source;
/// long-lived platform workers and future authenticated control connections share
/// this Runtime directly instead of depending on the test fixture wrapper.
struct DesktopNodeState {
    runtime: NodeRuntime,
    bootstrap_session_id: SessionId,
    demo_routes: DemoRoutes,
}

/// Host-owned control-plane inventory. Socket listeners, OS credential loading
/// and authenticated connections will update this owner in later slices; the
/// UI only receives its bounded observation snapshot.
struct DesktopControlPlaneState {
    clock: Instant,
    discovery_ingress: DiscoveryIngress,
    paired_peers: PairedPeerDirectory,
    discovery_listening: bool,
    credential_store_ready: bool,
    credential_store_failed: bool,
    authenticated_connection_count: usize,
    catalog_exchange_active: bool,
    pairing_store: Option<WindowsPairingStore>,
}

struct ResolvedControlDial {
    candidate: PairedControlCandidate,
    display_name: String,
    store: WindowsPairingStore,
}

impl DesktopControlPlaneState {
    fn new(local_node_id: capyio_core::NodeId) -> Result<Self, String> {
        Ok(Self {
            clock: Instant::now(),
            discovery_ingress: DiscoveryIngress::new(local_node_id)
                .map_err(|error| error.to_string())?,
            paired_peers: PairedPeerDirectory::new(local_node_id)
                .map_err(|error| error.to_string())?,
            discovery_listening: false,
            credential_store_ready: false,
            credential_store_failed: false,
            authenticated_connection_count: 0,
            catalog_exchange_active: false,
            pairing_store: None,
        })
    }

    fn attach_pairing_store(&mut self, store: WindowsPairingStore) -> Result<(), String> {
        let directory = store.load_directory().map_err(|error| error.to_string())?;
        self.paired_peers = directory;
        self.pairing_store = Some(store);
        self.credential_store_ready = true;
        self.credential_store_failed = false;
        Ok(())
    }

    fn attach_default_pairing_store(&mut self, local_node_id: capyio_core::NodeId) {
        let result = WindowsPairingStore::default_for_user(local_node_id)
            .map_err(|error| error.to_string())
            .and_then(|store| self.attach_pairing_store(store));
        if result.is_err() {
            self.credential_store_ready = false;
            self.credential_store_failed = true;
            self.pairing_store = None;
        }
    }

    fn refresh_pairing_directory(&mut self) -> Result<(), String> {
        let store = self
            .pairing_store
            .clone()
            .ok_or_else(|| "pairing credential store is unavailable".to_owned())?;
        match store.load_directory() {
            Ok(directory) => {
                self.paired_peers = directory;
                self.credential_store_ready = true;
                self.credential_store_failed = false;
                Ok(())
            }
            Err(error) => {
                self.credential_store_ready = false;
                self.credential_store_failed = true;
                // Never dial with cached trust after a failed refresh.
                Err(format!("pairing directory refresh failed: {error}"))
            }
        }
    }

    fn resolve_control_dial(
        &mut self,
        peer_id: NodeId,
        now_ms: u64,
    ) -> Result<ResolvedControlDial, String> {
        self.refresh_pairing_directory()?;
        self.discovery_ingress.expire(now_ms);
        let candidate = self
            .discovery_ingress
            .candidates()
            .find(|candidate| candidate.node_id == peer_id)
            .copied()
            .ok_or_else(|| "selected discovery candidate is absent or expired".to_owned())?;
        let CandidateTrust::Paired(candidate) = self.paired_peers.classify(candidate, now_ms)
        else {
            return Err("selected discovery candidate is not actively paired".to_owned());
        };
        let display_name = self
            .paired_peers
            .peers()
            .find(|peer| peer.node_id == peer_id)
            .map(|peer| peer.display_name.clone())
            .ok_or_else(|| "paired peer metadata is unavailable".to_owned())?;
        let store = self
            .pairing_store
            .clone()
            .ok_or_else(|| "pairing credential store is unavailable".to_owned())?;
        Ok(ResolvedControlDial {
            candidate,
            display_name,
            store,
        })
    }

    fn resolve_manual_control_dial(
        &mut self,
        peer_id: NodeId,
        endpoint: SocketAddr,
    ) -> Result<ResolvedControlDial, String> {
        self.refresh_pairing_directory()?;
        let peer = self
            .paired_peers
            .peers()
            .find(|peer| peer.node_id == peer_id)
            .ok_or_else(|| "manual endpoint belongs to an unpaired Node".to_owned())?;
        if !matches!(peer.state, PairingState::Active) {
            return Err("manual endpoint belongs to a revoked Node".to_owned());
        }
        let store = self
            .pairing_store
            .clone()
            .ok_or_else(|| "pairing credential store is unavailable".to_owned())?;
        Ok(ResolvedControlDial {
            candidate: PairedControlCandidate {
                node_id: peer_id,
                endpoint,
                server_name: peer.server_name.clone(),
                credential_handle: peer.credential_handle.clone(),
                peer_certificate_sha256: peer.peer_certificate_sha256,
                instance_nonce: 0,
            },
            display_name: peer.display_name.clone(),
            store,
        })
    }
}

impl DesktopNodeState {
    fn production() -> Result<Self, String> {
        Self::with_local_identity(desktop_identity::for_current_user()?)
    }

    /// Keep the compatibility fixture catalogs, but bind their local endpoints
    /// to the account's persisted Node before installing any real Adapter.
    fn with_local_identity(local_node_id: NodeId) -> Result<Self, String> {
        if local_node_id.as_uuid().is_nil() {
            return Err("desktop Node identity cannot be nil".into());
        }
        let lab = DemoLab::new().map_err(|error| error.to_string())?;
        let snapshot = lab.runtime.snapshot();
        let previous_id = snapshot.local_node.id;
        let mut local = snapshot.local_node;
        local.id = local_node_id;
        if snapshot.peers.iter().any(|peer| peer.id == local_node_id) {
            return Err("desktop Node identity conflicts with the compatibility peer".into());
        }
        let mut runtime = NodeRuntime::new(local).map_err(|error| error.to_string())?;
        for peer in snapshot.peers {
            let online = peer.online_state == OnlineState::Online;
            runtime
                .register_peer(peer, online)
                .map_err(|error| error.to_string())?;
        }
        for session in snapshot.sessions {
            runtime
                .open_session_with_id(session.id, session.remote_node_id)
                .map_err(|error| error.to_string())?;
        }
        for mut route in snapshot.routes {
            if route.source.node_id == previous_id {
                route.source.node_id = local_node_id;
            }
            if route.sink.node_id == previous_id {
                route.sink.node_id = local_node_id;
            }
            runtime
                .create_route_with_id(
                    route.id,
                    route.session_id,
                    route.source,
                    route.sink,
                    route.backend,
                )
                .map_err(|error| error.to_string())?;
        }
        Ok(Self {
            runtime,
            bootstrap_session_id: lab.session_id,
            demo_routes: lab.routes,
        })
    }

    #[cfg(test)]
    fn demo() -> Result<Self, String> {
        let lab = DemoLab::new().map_err(|error| error.to_string())?;
        Ok(Self {
            runtime: lab.runtime,
            bootstrap_session_id: lab.session_id,
            demo_routes: lab.routes,
        })
    }

    fn set_demo_route_active(
        &mut self,
        route_id: RouteId,
        active: bool,
        now_ms: u64,
    ) -> Result<(), String> {
        if !self.demo_routes.all().contains(&route_id) {
            return Err("Route is not controlled by the deterministic demo".to_owned());
        }
        self.runtime
            .set_route_active(route_id, active, now_ms)
            .map_err(|error| error.to_string())
    }
}

struct AtomicSingleFlight<'a> {
    flag: &'a AtomicBool,
}

impl<'a> AtomicSingleFlight<'a> {
    fn acquire(flag: &'a AtomicBool, busy_message: &'static str) -> Result<Self, String> {
        flag.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| busy_message.to_owned())?;
        Ok(Self { flag })
    }
}

impl Drop for AtomicSingleFlight<'_> {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::Release);
    }
}

struct LiveImuController {
    stop: Arc<AtomicBool>,
    worker: JoinHandle<()>,
}

struct DiscoveryController {
    stop: Arc<AtomicBool>,
    worker: JoinHandle<()>,
}

struct PairedListenerController {
    stop: Arc<AtomicBool>,
    worker: JoinHandle<()>,
    peer_id: NodeId,
    local_addr: SocketAddr,
}

struct PairedListenerContext {
    gamepad_lab: Arc<Mutex<GamepadLab>>,
    android_receivers: Arc<Mutex<android_receivers::AndroidReceiverActions>>,
    node: Arc<Mutex<DesktopNodeState>>,
    control_plane: Arc<Mutex<DesktopControlPlaneState>>,
    links: Arc<Mutex<BTreeMap<NodeId, ControlLinkController>>>,
    selected: Arc<Mutex<BTreeSet<CapabilityId>>>,
}

impl PairedListenerController {
    fn stop_and_join(self) {
        self.stop.store(true, Ordering::Release);
        let _ = self.worker.join();
    }
}

enum ControlLinkCommand {
    ObserveMedia {
        reply: mpsc::Sender<Vec<route_readiness::RouteMediaSnapshot>>,
    },
    StartDisplay {
        selection: capyio_protocol::v1::DisplayStreamSelection,
        automatic: bool,
        reply: mpsc::Sender<Result<(), String>>,
    },
    StopDisplay {
        reply: mpsc::Sender<Result<(), String>>,
    },
    StartController {
        mode: controller_workflow::ControllerMode,
        reply: mpsc::Sender<Result<(), String>>,
    },
    StopController {
        reply: mpsc::Sender<Result<(), String>>,
    },
    ResolveController {
        route: RouteId,
        allow: bool,
        remember: bool,
        reply: mpsc::Sender<Result<(), String>>,
    },
    Publish,
    StartRemoteCapability {
        capability_id: CapabilityId,
        reply: mpsc::Sender<Result<(), String>>,
    },
    StopRemoteCapability {
        capability_id: CapabilityId,
        reply: mpsc::Sender<Result<(), String>>,
    },
    Stop,
}

/// Owns one authenticated control transport after the initial catalog exchange.
/// The socket never enters a Tauri command or snapshot path.
struct ControlLinkController {
    active: Arc<AtomicBool>,
    commands: SyncSender<ControlLinkCommand>,
    worker: Option<JoinHandle<Result<(), String>>>,
}

fn receive_route_status_for(
    link: &mut CatalogControlLink,
    node: &Arc<Mutex<DesktopNodeState>>,
    route_id: RouteId,
) -> Result<Route, String> {
    receive_route_status_details_for(link, node, route_id).map(|status| status.route)
}

fn receive_route_status_details_for(
    link: &mut CatalogControlLink,
    node: &Arc<Mutex<DesktopNodeState>>,
    route_id: RouteId,
) -> Result<capyio_protocol::ValidatedRouteStatus, String> {
    for _ in 0..control_exchange::MAX_EXCHANGE_MESSAGES {
        match link
            .receive_control_message()
            .map_err(|error| error.to_string())?
        {
            ReceivedControlMessage::RouteStatus(status) if status.route.id == route_id => {
                let peer_route = status.route.clone();
                let mut node = node
                    .lock()
                    .map_err(|_| "Node state lock poisoned".to_owned())?;
                link.apply_route_status(&mut node.runtime, &status, false)
                    .map_err(|error| error.to_string())?;
                if let AuthorizationState::Authorized { expires_at_ms } = peer_route.authorization {
                    if node
                        .runtime
                        .route(peer_route.id)
                        .map_err(|e| e.to_string())?
                        .authorization
                        == AuthorizationState::Pending
                    {
                        node.runtime
                            .authorize_route(peer_route.id, expires_at_ms)
                            .map_err(|e| e.to_string())?;
                    }
                    link.apply_route_status(&mut node.runtime, &status, true)
                        .map_err(|e| e.to_string())?;
                }
                if peer_route.authorization == AuthorizationState::Denied {
                    node.runtime
                        .deny_pending_route(peer_route.id)
                        .map_err(|e| e.to_string())?;
                }
                return Ok(*status);
            }
            ReceivedControlMessage::Catalog(catalog) => {
                let mut node = node
                    .lock()
                    .map_err(|_| "Node state lock poisoned".to_owned())?;
                link.apply_catalog(&mut node.runtime, catalog)
                    .map_err(|error| error.to_string())?;
            }
            ReceivedControlMessage::Heartbeat(heartbeat) if !heartbeat.acknowledgement => {
                link.send_heartbeat(heartbeat.sequence, true)
                    .map_err(|error| error.to_string())?;
            }
            ReceivedControlMessage::ControllerSettingsState(value) => {
                controller_settings::received(
                    &link.peer_node_id().to_string(),
                    &link.session_id().to_string(),
                    value,
                );
            }
            ReceivedControlMessage::RouteStatus(status) => {
                control_exchange::apply_intervening_route(link, node, &status)?;
            }
            ReceivedControlMessage::DisplaySessionControl(value) => {
                display_workflow::defer_message(link.session_id(), value)?;
            }
            _ => return Err("peer returned an unexpected Route control message".to_owned()),
        }
    }
    Err("peer Route response limit exceeded".to_owned())
}

enum ResourceDataWorker {
    Touchpad(touchpad_data::TouchpadDataWorker),
    Haptics(haptics_data::HapticsDataWorker),
}
impl ResourceDataWorker {
    fn is_finished(&self) -> bool {
        match self {
            Self::Touchpad(worker) => worker.is_finished(),
            Self::Haptics(worker) => worker.is_finished(),
        }
    }
}
type StartedRemoteRoute = (RouteId, PortRef, Option<ResourceDataWorker>);

fn start_remote_capability_route_with_data(
    link: &mut CatalogControlLink,
    node: &Arc<Mutex<DesktopNodeState>>,
    capability_id: CapabilityId,
    dial: Option<&ResolvedControlDial>,
    gamepad: Option<&Arc<Mutex<GamepadLab>>>,
    selected: Option<&Arc<Mutex<BTreeSet<CapabilityId>>>>,
) -> Result<Option<StartedRemoteRoute>, String> {
    let request_time = unix_now_ms()?;
    let reverse_haptics = node
        .lock()
        .map_err(|_| "Node lock poisoned")?
        .runtime
        .snapshot()
        .peers
        .iter()
        .find(|peer| peer.id == link.peer_node_id())
        .and_then(|peer| peer.capabilities.get(&capability_id))
        .is_some_and(|cap| {
            cap.class == CapabilityClass::Haptics
                && cap.ports.values().any(|port| {
                    port.direction == PortDirection::Sink
                        && port.profile == capyio_core::ProfileId::haptics_feedback_v1()
                })
        });
    if reverse_haptics {
        if gamepad
            .ok_or("振动需要桌面手柄宿主")?
            .lock()
            .map_err(|_| "gamepad lock poisoned")?
            .feedback_motors()
            .is_none()
        {
            return Err("请先启动 DS4 或 Xbox 手柄，再申请游戏振动".into());
        }
        let outgoing = {
            let mut node = node.lock().map_err(|_| "Node lock poisoned")?;
            haptics_data::install_catalog(&mut node.runtime)?;
            let mut selected = selected
                .ok_or("振动 Source 发布上下文缺失")?
                .lock()
                .map_err(|_| "publication lock poisoned")?;
            selected.insert(
                CapabilityId::from_str(haptics_data::CAPABILITY).map_err(|e| e.to_string())?,
            );
            link.prepare_catalog(&node.runtime, &selected)
                .map_err(|e| e.to_string())?
        };
        link.send_catalog(outgoing).map_err(|e| e.to_string())?;
    }
    let (route, remote_port) = {
        let mut node = node
            .lock()
            .map_err(|_| "Node state lock poisoned".to_owned())?;
        let snapshot = node.runtime.snapshot();
        let peer = snapshot
            .peers
            .iter()
            .find(|peer| peer.id == link.peer_node_id())
            .ok_or_else(|| "authenticated peer catalog is unavailable".to_owned())?;
        let capability = peer
            .capabilities
            .get(&capability_id)
            .ok_or_else(|| "selected remote Capability is no longer published".to_owned())?;
        let source = capability
            .ports
            .values()
            .find(|port| {
                port.direction
                    == if reverse_haptics {
                        PortDirection::Sink
                    } else {
                        PortDirection::Source
                    }
            })
            .ok_or_else(|| "selected remote Capability has no Source Port".to_owned())?;
        native_audio_route::require_product_data_path(&source.profile)?;
        if source.profile == capyio_core::ProfileId::touchpad_frames_v1() && dial.is_some() {
            touchpad_data::preflight_projection()?;
        }
        let remote_port = PortRef {
            node_id: peer.id,
            capability_id,
            port_id: source.id,
        };
        let local_port = snapshot
            .local_node
            .capabilities
            .values()
            .flat_map(|capability| {
                capability
                    .ports
                    .values()
                    .map(move |port| (capability.id, port))
            })
            .find(|(_, port)| {
                port.direction == if reverse_haptics { PortDirection::Source } else { PortDirection::Sink }
                    && port.profile == source.profile
                    && port.interoperability_mode == source.interoperability_mode
                    && port
                        .formats
                        .iter()
                        .any(|format| source.formats.contains(format))
                    && port
                        .qos_modes
                        .iter()
                        .any(|mode| source.qos_modes.contains(mode))
            })
            .map(|(capability_id, port)| PortRef {
                node_id: snapshot.local_node.id,
                capability_id,
                port_id: port.id,
            })
            .ok_or_else(|| {
                if capability.class == CapabilityClass::Touchpad {
                    return "Windows 触控板接收器尚未接入此远程资源协议；当前演示 Sink 不能代替真实接收器。请勿将目录已连接当作触控板已就绪。".to_owned();
                }
                format!(
                    "Windows has no compatible Sink for {}/{}",
                    source.profile.name, source.profile.major
                )
            })?;
        let (route_source, route_sink) = if reverse_haptics {
            (local_port, remote_port)
        } else {
            (remote_port, local_port)
        };
        let mut last_error = None;
        let mut route_id = snapshot.routes.iter().find(|route| {
            route.session_id == link.session_id() && route.source == route_source && route.sink == route_sink
                && route.state == RouteState::Draft
                && matches!(route.authorization, AuthorizationState::Pending | AuthorizationState::Authorized { .. })
                && !matches!(route.authorization, AuthorizationState::Authorized { expires_at_ms: Some(expiry) } if expiry <= request_time.saturating_add(1_000))
        }).map(|route| route.id);
        for backend in [
            RouteBackend::CapyDataPlane,
            RouteBackend::AdapterManaged,
            RouteBackend::ExternalProtocol,
        ] {
            if route_id.is_some() {
                break;
            }
            match node
                .runtime
                .create_route(link.session_id(), route_source, route_sink, backend)
            {
                Ok(created) => {
                    route_id = Some(created);
                    break;
                }
                Err(error) => last_error = Some(error.to_string()),
            }
        }
        let route_id = route_id.ok_or_else(|| {
            last_error.unwrap_or_else(|| "no compatible Route backend".to_owned())
        })?;
        (
            node.runtime
                .route(route_id)
                .map_err(|error| error.to_string())?
                .clone(),
            remote_port,
        )
    };
    link.send_route_status(&route)
        .map_err(|error| error.to_string())?;
    let accepted = receive_route_status_for(link, node, route.id)?;
    if accepted.authorization == AuthorizationState::Pending {
        return Ok(None);
    }
    if !matches!(
        accepted.authorization,
        AuthorizationState::Authorized { .. }
    ) {
        return Err("平板已拒绝或请求已过期，未申请占用、未启动硬件。".to_owned());
    }
    let lease_ttl = touchpad_lease::ttl(&accepted.authorization, unix_now_ms()?)?;
    let request_id = link
        .send_port_lease_request(route.id, remote_port, PortLeaseAction::Acquire, lease_ttl)
        .map_err(|error| error.to_string())?;
    let response = control_exchange::receive_lease(link, node, request_id, route.id, remote_port)?;
    if !matches!(
        response.outcome,
        PortLeaseOutcome::Acquired | PortLeaseOutcome::Renewed
    ) {
        return Err(match response.outcome {
            PortLeaseOutcome::Conflict => "remote hardware is already occupied".to_owned(),
            _ => "remote node denied the hardware lease".to_owned(),
        });
    }
    if route.profile == capyio_core::ProfileId::touchpad_frames_v1() && dial.is_none() {
        // Consent-only diagnostic: do not request or open a real data endpoint.
        return Ok(Some((route.id, remote_port, None)));
    }
    link.send_route_command(route.id, RouteCommandAction::Start)
        .map_err(|error| error.to_string())?;
    let status = receive_route_status_details_for(link, node, route.id)?;
    let remote = status.route;
    if let Some(offer) = status.haptics_data {
        let worker = haptics_data::start(
            node.clone(),
            gamepad.ok_or("gamepad host missing")?.clone(),
            dial.ok_or("haptics data requires a paired endpoint")?,
            link.tls_binding(),
            remote,
            offer,
        );
        return match worker {
            Ok(worker) => Ok(Some((
                route.id,
                remote_port,
                Some(ResourceDataWorker::Haptics(worker)),
            ))),
            Err(error) => {
                let _ = stop_remote_capability_route(link, node, route.id, remote_port);
                Err(error)
            }
        };
    }
    if let Some(offer) = status.touchpad_data {
        let dial = dial.ok_or("触控板返回真实数据端口；当前诊断模式不会打开输入设备")?;
        let worker = touchpad_data::start(node.clone(), dial, link.tls_binding(), remote, offer);
        return match worker {
            Ok(worker) => Ok(Some((
                route.id,
                remote_port,
                Some(ResourceDataWorker::Touchpad(worker)),
            ))),
            Err(error) => {
                let _ = stop_remote_capability_route(link, node, route.id, remote_port);
                Err(error)
            }
        };
    }
    if route.profile == capyio_core::ProfileId::touchpad_frames_v1() || reverse_haptics {
        let _ = stop_remote_capability_route(link, node, route.id, remote_port);
        return Err(
            "对端没有返回加密触控数据端点，请更新两端软件；未将逻辑连接当作输入就绪。".into(),
        );
    }
    if remote.state != RouteState::Active {
        return Err("remote Route did not become active".to_owned());
    }
    let now_ms = unix_now_ms()?;
    node.lock()
        .map_err(|_| "Node state lock poisoned".to_owned())?
        .runtime
        .set_route_active(route.id, true, now_ms)
        .map_err(|error| error.to_string())?;
    Ok(Some((route.id, remote_port, None)))
}

fn stop_remote_capability_route(
    link: &mut CatalogControlLink,
    node: &Arc<Mutex<DesktopNodeState>>,
    route_id: RouteId,
    remote_port: PortRef,
) -> Result<(), String> {
    // Record local intent before the peer closes its data channel. Otherwise
    // the data worker can report a transport failure during a normal Stop.
    {
        let mut node = node.lock().map_err(|_| "Node state lock poisoned")?;
        if matches!(
            node.runtime
                .route(route_id)
                .map_err(|e| e.to_string())?
                .state,
            RouteState::Prepared | RouteState::Starting | RouteState::Active | RouteState::Offline
        ) {
            node.runtime
                .begin_route_stop(route_id)
                .map_err(|e| e.to_string())?;
        }
    }
    let remote_result = (|| {
        link.send_route_command(route_id, RouteCommandAction::Stop)
            .map_err(|error| error.to_string())?;
        let stopped = receive_route_status_for(link, node, route_id)?;
        let request_id = link
            .send_port_lease_request(route_id, remote_port, PortLeaseAction::Release, 0)
            .map_err(|error| error.to_string())?;
        let response =
            control_exchange::receive_lease(link, node, request_id, route_id, remote_port)?;
        if response.outcome != PortLeaseOutcome::Released
            && !(response.outcome == PortLeaseOutcome::Denied
                && stopped.state == RouteState::Stopped)
        {
            return Err("remote node did not release the hardware lease".to_owned());
        }
        Ok(())
    })();
    // A failed exchange must still release this node's hardware lease.
    let local_result = node
        .lock()
        .map_err(|_| "Node state lock poisoned".to_owned())?
        .runtime
        .set_route_active(route_id, false, unix_now_ms()?)
        .map_err(|error| error.to_string());
    remote_result.and(local_result)
}

fn cancel_pending_remote_route(
    link: &mut CatalogControlLink,
    node: &Arc<Mutex<DesktopNodeState>>,
    route_id: RouteId,
) -> Result<(), String> {
    link.send_route_command(route_id, RouteCommandAction::Stop)
        .map_err(|e| e.to_string())?;
    receive_route_status_for(link, node, route_id)?;
    let mut node = node.lock().map_err(|_| "Node state lock poisoned")?;
    if node
        .runtime
        .route(route_id)
        .map_err(|e| e.to_string())?
        .authorization
        == AuthorizationState::Pending
    {
        node.runtime
            .deny_pending_route(route_id)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn get_controller_use(
    state: &HostState,
) -> Result<Option<controller_workflow::UiControllerUse>, String> {
    state.0.get_controller_use()
}

fn start_controller_use(
    state: &HostState,
    peer_node_id: String,
    mode: controller_workflow::ControllerMode,
) -> Result<(), String> {
    state.0.start_controller_use(peer_node_id, mode)
}

fn stop_controller_use(state: &HostState) -> Result<(), String> {
    state.0.stop_controller_use()
}

fn get_resource_requests() -> Result<Vec<controller_control::ResourceRequest>, String> {
    controller_control::pending()
}

fn resolve_resource_request(
    state: &HostState,
    route_id: String,
    allow: bool,
    remember: bool,
) -> Result<(), String> {
    state.0.resolve_resource_request(route_id, allow, remember)
}

fn unix_now_ms() -> Result<u64, String> {
    let value = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "system clock is before Unix epoch".to_owned())?
        .as_millis();
    u64::try_from(value).map_err(|_| "system clock exceeds Runtime range".to_owned())
}

struct AudioQuickActionWorker {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

struct GamepadProjectionWorker {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

fn receive_probe_heartbeat(
    mut receive: impl FnMut() -> Result<ReceivedControlMessage, String>,
) -> Result<(), String> {
    // Settings may precede the heartbeat response, but a probe must remain
    // bounded and must not silently consume unrelated commands or bad sequences.
    for _ in 0..8 {
        match receive()? {
            ReceivedControlMessage::Heartbeat(value)
                if value.acknowledgement && value.sequence == 1 =>
            {
                return Ok(());
            }
            ReceivedControlMessage::ControllerSettingsState(_) => {}
            _ => return Err("peer heartbeat acknowledgement is invalid".to_owned()),
        }
    }
    Err("peer heartbeat acknowledgement did not arrive within probe message bound".to_owned())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StartLiveImuRequest {
    ip: String,
    port: u16,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StartGamepadDsuRequest {
    port: u16,
    mode: gamepad_lab::UiDsuMode,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StartAndroidGamepadRequest {
    port: u16,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RefreshWindowsGamepadPreflightRequest {
    controller_kind: windows_gamepad_host::UiWindowsControllerKind,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StartWindowsGamepadProjectionRequest {
    controller_kind: windows_gamepad_host::UiWindowsControllerKind,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StartControllerHostRequest {
    controller_kind: windows_gamepad_host::UiWindowsControllerKind,
    port: u16,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiLiveImu {
    status: &'static str,
    simulated: bool,
    route_id: String,
    route_state: &'static str,
    endpoint: Option<String>,
    profile: &'static str,
    stream_epoch: u64,
    sequence: Option<u64>,
    source_timestamp_nanos: Option<u64>,
    clock_domain_id: Option<String>,
    acceleration: Option<UiVector3>,
    angular_velocity: Option<UiVector3>,
    received_samples: u64,
    problem_code: Option<String>,
    problem: Option<String>,
}

impl UiLiveImu {
    fn idle(
        route: PhysicalImuRoute,
        runtime: &capyio_runtime::NodeRuntime,
    ) -> Result<Self, String> {
        Ok(Self {
            status: "idle",
            simulated: false,
            route_id: route.route_id().to_string(),
            route_state: route_state_label(route.route_state(runtime)?),
            endpoint: None,
            profile: "capyio.motion.imu-samples/1",
            stream_epoch: 0,
            sequence: None,
            source_timestamp_nanos: None,
            clock_domain_id: None,
            acceleration: None,
            angular_velocity: None,
            received_samples: 0,
            problem_code: None,
            problem: None,
        })
    }
}

#[derive(Debug)]
enum LiveWorkerEvent {
    Connected(SensorKind),
    Reading(SensorServerReading),
    Failed { kind: SensorKind, detail: String },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetRouteRequest {
    route_id: String,
    active: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SetLocalCapabilitySharedRequest {
    capability_id: String,
    selected: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SetRemoteCapabilityUseRequest {
    capability_id: String,
    active: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StartDiscoveryRequest {
    interface_ipv4: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiSnapshot {
    backend_mode: &'static str,
    schema_version: u8,
    project_version: &'static str,
    control_plane: UiControlPlaneStatus,
    nodes: Vec<UiNode>,
    capabilities: Vec<UiCapability>,
    routes: Vec<UiRoute>,
    port_leases: Vec<UiPortLease>,
    adapters: Vec<UiAdapter>,
    events: Vec<UiEvent>,
    warnings: Vec<String>,
    imu_fixture: UiImuFixture,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiControlPlaneStatus {
    schema_version: u8,
    simulated: bool,
    discovery_state: &'static str,
    pairing_state: &'static str,
    transport_state: &'static str,
    catalog_exchange_state: &'static str,
    discovered_candidate_count: usize,
    paired_peer_count: usize,
    authenticated_connection_count: usize,
    listen_port: Option<u16>,
    pairing_store_path: Option<String>,
    process_user_sid: Option<String>,
}

#[cfg(windows)]
fn current_process_sid_for_ui() -> Option<String> {
    capyio_windows_service::current_process_user_sid().ok()
}

#[cfg(not(windows))]
fn current_process_sid_for_ui() -> Option<String> {
    None
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiPortLease {
    node_id: String,
    capability_id: String,
    port_id: String,
    route_id: String,
    session_id: String,
    expires_at_ms: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiNode {
    id: String,
    display_name: String,
    platform: String,
    platform_version: String,
    online: bool,
    local: bool,
    capability_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiCapability {
    id: String,
    node_id: String,
    node_name: String,
    display_name: String,
    capability_class: String,
    local: bool,
    availability: &'static str,
    permission_requirement: &'static str,
    inventory_source: &'static str,
    simulated: bool,
    selectable_for_sharing: bool,
    selected_for_sharing: bool,
    ports: Vec<UiCapabilityPort>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiCapabilityPort {
    id: String,
    display_name: String,
    direction: &'static str,
    profile: String,
    formats: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiRoute {
    id: String,
    error: Option<String>,
    source_capability_id: Option<String>,
    sink_capability_id: Option<String>,
    title: String,
    summary: String,
    profile: String,
    backend: String,
    state: String,
    active: bool,
    authorization: &'static str,
    authorization_expires_at_ms: Option<u64>,
    source: UiPort,
    sink: UiPort,
    format_summary: Option<String>,
    qos_modes: Vec<String>,
    projection_note: String,
    metrics: UiMetricSet,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiPort {
    node_name: String,
    capability_name: String,
    capability_class: String,
    port_name: String,
    direction: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiAdapter {
    id: String,
    node_name: String,
    display_name: String,
    adapter_type: String,
    deployment_mode: String,
    state: String,
    health: String,
    capability_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiMetricSet {
    estimated_latency_ms: Option<f64>,
    packet_loss_percent: Option<f64>,
    buffer_fill_ms: Option<f64>,
    simulated: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiEvent {
    sequence: u64,
    summary: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiImuFixture {
    mode: &'static str,
    simulated: bool,
    profile: &'static str,
    sequence: u64,
    source_timestamp_nanos: u64,
    clock_domain_id: String,
    acceleration: UiVector3,
    angular_velocity: UiVector3,
    panel_received: u64,
    panel_missing_sequences: u64,
    recorder_records: usize,
    panel_route_state: &'static str,
    recorder_route_state: &'static str,
}

#[derive(Clone, Debug, Serialize)]
struct UiVector3 {
    x: f64,
    y: f64,
    z: f64,
}

fn get_snapshot(state: &HostState) -> Result<UiSnapshot, String> {
    state.0.snapshot()
}

fn get_discovery_interfaces() -> Result<Vec<network_interfaces::DiscoveryInterface>, String> {
    network_interfaces::enumerate()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UiPairedPeer {
    node_id: String,
    display_name: String,
    revoked: bool,
    paired_at_ms: u64,
}

fn paired_peer_metadata(control: &DesktopControlPlaneState) -> Vec<UiPairedPeer> {
    control
        .paired_peers
        .peers()
        .map(|peer| UiPairedPeer {
            node_id: peer.node_id.to_string(),
            display_name: peer.display_name.clone(),
            revoked: !matches!(peer.state, PairingState::Active),
            paired_at_ms: peer.paired_at_ms,
        })
        .collect()
}

fn get_paired_peers(state: &HostState) -> Result<Vec<UiPairedPeer>, String> {
    state.0.get_paired_peers()
}

fn check_paired_credential(request: PeerControlRequest, state: &HostState) -> Result<(), String> {
    state.0.check_paired_credential(request)
}

fn verify_credential_readable(
    store: &WindowsPairingStore,
    handle: &capyio_pairing::CredentialHandle,
) -> Result<(), String> {
    store.load_control_credential(handle)
        .map(|_| ())
        .map_err(|_| "当前进程账号无法读取该设备的配对凭据；请使用原配对账号运行 CapyIO，或在当前账号重新配对。".to_owned())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UiDiscoveryCandidate {
    node_id: String,
    host: String,
    control_port: u16,
    last_seen_age_ms: u64,
    pairing_state: &'static str,
}

fn discovery_candidates(
    control: &mut DesktopControlPlaneState,
    now_ms: u64,
) -> Vec<UiDiscoveryCandidate> {
    control.discovery_ingress.expire(now_ms);
    control
        .discovery_ingress
        .candidates()
        .filter_map(|candidate| {
            let pairing_state = match control.paired_peers.classify(*candidate, now_ms) {
                CandidateTrust::Paired(_) => "paired",
                CandidateTrust::Unpaired => "unpaired",
                CandidateTrust::Revoked => "revoked",
                CandidateTrust::Expired => return None,
            };
            Some(UiDiscoveryCandidate {
                node_id: candidate.node_id.to_string(),
                host: candidate.endpoint.ip().to_string(),
                control_port: candidate.endpoint.port(),
                last_seen_age_ms: now_ms.saturating_sub(candidate.last_seen_ms),
                pairing_state,
            })
        })
        .collect()
}

fn get_discovery_candidates(state: &HostState) -> Result<Vec<UiDiscoveryCandidate>, String> {
    state.0.get_discovery_candidates()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PeerControlRequest {
    node_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PairingInvitationRequest {
    node_id: String,
    display_name: String,
    #[serde(default)]
    renew_existing: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManualPeerControlRequest {
    node_id: String,
    host: String,
    port: u16,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PairedListenerRequest {
    node_id: String,
    bind_host: String,
    port: u16,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UiPairedListenerStatus {
    node_id: String,
    bind_host: String,
    port: u16,
}

fn get_paired_listener_status(state: &HostState) -> Result<Option<UiPairedListenerStatus>, String> {
    state.0.get_paired_listener_status()
}

fn start_paired_listener(request: PairedListenerRequest, state: &HostState) -> Result<u16, String> {
    state.0.start_paired_listener(request)
}

fn stop_paired_listener(state: &HostState) -> Result<(), String> {
    state.0.stop_paired_listener()
}

fn create_pairing_invitation(
    request: PairingInvitationRequest,
    state: &HostState,
) -> Result<String, String> {
    state.0.create_pairing_invitation(request)
}

fn connect_discovered_peer(request: PeerControlRequest, state: &HostState) -> Result<(), String> {
    state.0.connect_discovered_peer_blocking(request)
}

fn connect_paired_peer_at(
    request: ManualPeerControlRequest,
    state: &HostState,
) -> Result<(), String> {
    state.0.connect_paired_peer_at_blocking(request)
}

fn connect_authenticated_control_channel(
    peer_id: NodeId,
    local_node_id: NodeId,
    plan: &ResolvedControlDial,
) -> Result<AuthenticatedControlClientChannel, String> {
    let fingerprint = plan.candidate.peer_certificate_sha256.as_bytes();
    let public_context = format!(
        "endpoint={}, paired_name={:?}, peer_cert_sha256_prefix={:02x}{:02x}{:02x}{:02x}",
        plan.candidate.endpoint,
        plan.display_name,
        fingerprint[0],
        fingerprint[1],
        fingerprint[2],
        fingerprint[3]
    );
    let bundle = plan
        .store
        .load_control_credential(&plan.candidate.credential_handle)
        .map_err(|error| {
            format!("paired control credential could not be loaded ({public_context}): {error}")
        })?;
    let identity =
        ControlTlsClientIdentity::from_bundle(plan.candidate.peer_certificate_sha256, bundle);
    let expected_nodes =
        ControlTlsNodeBinding::new(peer_id, local_node_id).map_err(|error| error.to_string())?;
    AuthenticatedControlClientChannel::connect_fresh_session(
        ControlTlsEndpoint {
            address: plan.candidate.endpoint,
            io_timeout: Duration::from_secs(5),
        },
        &plan.candidate.server_name,
        identity,
        expected_nodes,
    )
    .map_err(|error| format!("authenticated control connection failed ({public_context}): {error}"))
}

fn disconnect_peer(request: PeerControlRequest, state: &HostState) -> Result<(), String> {
    let peer_id = NodeId::from_str(&request.node_id)
        .map_err(|_| "peer Node ID must be a canonical UUID".to_owned())?;
    state.0.disconnect_peer(peer_id)
}

fn start_discovery(
    request: StartDiscoveryRequest,
    state: &HostState,
) -> Result<UiSnapshot, String> {
    state.0.start_discovery(&request.interface_ipv4)
}

fn stop_discovery(state: &HostState) -> Result<UiSnapshot, String> {
    state.0.stop_discovery()
}

fn set_route(request: SetRouteRequest, state: &HostState) -> Result<UiSnapshot, String> {
    let route_id = RouteId::from_str(&request.route_id).map_err(|_| "invalid Route ID")?;
    state.0.set_route(route_id, request.active)
}

fn set_local_capability_shared(
    request: SetLocalCapabilitySharedRequest,
    state: &HostState,
) -> Result<UiSnapshot, String> {
    let capability_id = CapabilityId::from_str(&request.capability_id)
        .map_err(|_| "invalid Capability ID".to_owned())?;
    state
        .0
        .set_local_capability_shared(capability_id, request.selected)
}

fn set_remote_capability_use(
    request: SetRemoteCapabilityUseRequest,
    state: &HostState,
) -> Result<UiSnapshot, String> {
    let capability_id = CapabilityId::from_str(&request.capability_id)
        .map_err(|_| "invalid Capability ID".to_owned())?;
    state
        .0
        .set_remote_capability_use(capability_id, request.active)
}

fn reset_demo(state: &HostState) -> Result<UiSnapshot, String> {
    state.0.reset_demo()
}

fn get_quick_actions(state: &HostState) -> Result<Vec<UiQuickAction>, String> {
    state.0.get_quick_actions()
}

fn invoke_quick_action(
    request: InvokeQuickActionRequest,
    state: &HostState,
) -> Result<UiQuickAction, String> {
    state.0.invoke_quick_action(request)
}

fn get_audio_endpoints(state: &HostState) -> Result<UiAudioEndpointCatalog, String> {
    state.0.get_audio_endpoints()
}

fn select_audio_endpoint(
    request: SelectAudioEndpointRequest,
    state: &HostState,
) -> Result<UiQuickAction, String> {
    state.0.select_audio_endpoint(request)
}

fn get_live_imu(state: &HostState) -> Result<UiLiveImu, String> {
    state.0.get_live_imu()
}

fn get_gamepad_state(state: &HostState) -> Result<UiGamepadState, String> {
    state.0.get_gamepad_state()
}

async fn refresh_windows_gamepad_preflight(
    request: RefreshWindowsGamepadPreflightRequest,
    state: &HostState,
) -> Result<UiGamepadState, String> {
    let _single_flight = state.0.begin_windows_preflight()?;
    let projection = tauri::async_runtime::spawn_blocking(move || {
        windows_gamepad_host::probe_read_only(request.controller_kind)
    })
    .await
    .map_err(|_| "Windows gamepad preflight worker panicked".to_owned())?;
    state.0.apply_windows_preflight(projection)
}

fn update_gamepad_state(
    request: UpdateGamepadRequest,
    state: &HostState,
) -> Result<UiGamepadState, String> {
    state.0.update_gamepad_state(request)
}

fn start_gamepad_dsu(
    request: StartGamepadDsuRequest,
    state: &HostState,
) -> Result<UiGamepadState, String> {
    state.0.start_gamepad_dsu(request)
}

fn stop_gamepad_dsu(state: &HostState) -> Result<UiGamepadState, String> {
    state.0.stop_gamepad_dsu()
}

fn start_android_gamepad(
    request: StartAndroidGamepadRequest,
    state: &HostState,
) -> Result<UiGamepadState, String> {
    state.0.start_android_gamepad(request)
}

fn stop_android_gamepad(state: &HostState) -> Result<UiGamepadState, String> {
    state.0.stop_android_gamepad()
}

fn start_windows_gamepad_projection(
    request: StartWindowsGamepadProjectionRequest,
    state: &HostState,
) -> Result<UiGamepadState, String> {
    state.0.start_windows_gamepad_projection(request)
}

fn stop_windows_gamepad_projection(state: &HostState) -> Result<UiGamepadState, String> {
    state.0.stop_windows_gamepad_projection()
}

fn start_controller_host(
    request: StartControllerHostRequest,
    state: &HostState,
) -> Result<UiGamepadState, String> {
    state.0.start_controller_host(request)
}

fn stop_controller_host(state: &HostState) -> Result<UiGamepadState, String> {
    state.0.stop_controller_host()
}

fn start_live_imu(request: StartLiveImuRequest, state: &HostState) -> Result<UiLiveImu, String> {
    state.0.start_live_imu(request)
}

fn stop_live_imu(state: &HostState) -> Result<UiLiveImu, String> {
    state.0.stop_live_imu()
}

fn run_live_imu(
    endpoint: SensorServerEndpoint,
    epoch: u64,
    stop: &Arc<AtomicBool>,
    shared: &Arc<Mutex<UiLiveImu>>,
    runtime: &Arc<Mutex<DesktopNodeState>>,
    route: PhysicalImuRoute,
) {
    if let Err(problem) = run_live_imu_inner(endpoint, epoch, stop, shared, runtime, route) {
        stop.store(true, Ordering::Release);
        let runtime_update = runtime
            .lock()
            .map_err(|_| "physical IMU Runtime lock poisoned".to_owned())
            .and_then(|mut node| {
                route.report_offline(&mut node.runtime, problem.clone())?;
                let retained = route.latest_problem(&node.runtime);
                Ok((
                    route.route_state(&node.runtime)?,
                    route.route_epoch(&node.runtime)?,
                    retained,
                ))
            });
        if let Ok(mut snapshot) = shared.lock() {
            match runtime_update {
                Ok((route_state, route_epoch, retained)) => {
                    snapshot.status = "offline";
                    snapshot.route_state = route_state_label(route_state);
                    snapshot.stream_epoch = route_epoch;
                    snapshot.problem_code = retained.as_ref().map(|item| item.code.clone());
                    snapshot.problem = retained
                        .and_then(|item| item.technical_detail)
                        .or(Some(problem));
                }
                Err(runtime_error) => {
                    snapshot.status = "failed";
                    snapshot.route_state = "failed";
                    snapshot.problem_code = Some("CAPY.RUNTIME.IMU_LIFECYCLE".to_owned());
                    snapshot.problem = Some(runtime_error);
                }
            }
        }
    }
}

fn run_live_imu_inner(
    endpoint: SensorServerEndpoint,
    epoch: u64,
    stop: &Arc<AtomicBool>,
    shared: &Arc<Mutex<UiLiveImu>>,
    runtime: &Arc<Mutex<DesktopNodeState>>,
    route: PhysicalImuRoute,
) -> Result<(), String> {
    let config =
        SensorServerConnectionConfig::new(Duration::from_secs(5), Duration::from_millis(500))
            .map_err(|error| error.to_string())?;
    let (sender, receiver) = mpsc::sync_channel(256);
    let accelerometer = spawn_live_reader(
        endpoint,
        SensorKind::Accelerometer,
        config,
        sender.clone(),
        Arc::clone(stop),
    );
    match receiver
        .recv_timeout(Duration::from_secs(10))
        .map_err(|error| format!("accelerometer connection timed out: {error}"))?
    {
        LiveWorkerEvent::Connected(SensorKind::Accelerometer) => {}
        LiveWorkerEvent::Failed { kind, detail } => {
            return Err(format!("{kind:?} reader failed: {detail}"));
        }
        event => return Err(format!("unexpected initial live IMU event: {event:?}")),
    }
    let gyroscope = spawn_live_reader(
        endpoint,
        SensorKind::Gyroscope,
        config,
        sender,
        Arc::clone(stop),
    );
    let mut assembler =
        SensorServerImuAssembler::new(capyio_core::StreamId::new(), epoch, 1_000_000_000, 0)
            .map_err(|error| error.to_string())?;
    let started = Instant::now();
    while !stop.load(Ordering::Acquire) {
        let event = match receiver.recv_timeout(Duration::from_secs(1)) {
            Ok(event) => event,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(error) => return Err(format!("live IMU event channel failed: {error}")),
        };
        match event {
            LiveWorkerEvent::Connected(SensorKind::Gyroscope) => {
                let (route_state, route_epoch) = {
                    let mut node = runtime
                        .lock()
                        .map_err(|_| "physical IMU Runtime lock poisoned".to_owned())?;
                    route.activate(&mut node.runtime)?;
                    (
                        route.route_state(&node.runtime)?,
                        route.route_epoch(&node.runtime)?,
                    )
                };
                let mut snapshot = shared
                    .lock()
                    .map_err(|_| "live IMU state lock poisoned".to_owned())?;
                snapshot.status = "active";
                snapshot.route_state = route_state_label(route_state);
                snapshot.stream_epoch = route_epoch;
            }
            LiveWorkerEvent::Connected(_) => {}
            LiveWorkerEvent::Reading(reading) => {
                let receive_timestamp_nanos = u64::try_from(started.elapsed().as_nanos())
                    .unwrap_or(u64::MAX)
                    .saturating_add(1);
                if let AssembleOutcome::Emitted { envelope, .. } = assembler
                    .ingest(reading, receive_timestamp_nanos)
                    .map_err(|error| error.to_string())?
                {
                    let mut snapshot = shared
                        .lock()
                        .map_err(|_| "live IMU state lock poisoned".to_owned())?;
                    snapshot.sequence = Some(envelope.sequence);
                    snapshot.source_timestamp_nanos = Some(envelope.source_timestamp_nanos);
                    snapshot.clock_domain_id = Some(envelope.clock_domain_id.clone());
                    snapshot.acceleration = Some(vector3(envelope.payload.acceleration));
                    snapshot.angular_velocity = Some(vector3(envelope.payload.angular_velocity));
                    snapshot.received_samples = snapshot.received_samples.saturating_add(1);
                }
            }
            LiveWorkerEvent::Failed { kind, detail } => {
                return Err(format!("{kind:?} reader failed: {detail}"));
            }
        }
    }
    stop.store(true, Ordering::Release);
    accelerometer
        .join()
        .map_err(|_| "accelerometer reader thread panicked".to_owned())?;
    gyroscope
        .join()
        .map_err(|_| "gyroscope reader thread panicked".to_owned())?;
    Ok(())
}

fn spawn_live_reader(
    endpoint: SensorServerEndpoint,
    kind: SensorKind,
    config: SensorServerConnectionConfig,
    sender: SyncSender<LiveWorkerEvent>,
    stop: Arc<AtomicBool>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        if let Err(detail) = read_live_sensor(endpoint, kind, config, &sender, &stop) {
            let _ = sender.try_send(LiveWorkerEvent::Failed { kind, detail });
        }
    })
}

fn read_live_sensor(
    endpoint: SensorServerEndpoint,
    kind: SensorKind,
    config: SensorServerConnectionConfig,
    sender: &SyncSender<LiveWorkerEvent>,
    stop: &AtomicBool,
) -> Result<(), String> {
    let mut client = SensorServerWebSocketClient::connect(endpoint, kind, config)
        .map_err(|error| error.to_string())?;
    sender
        .try_send(LiveWorkerEvent::Connected(kind))
        .map_err(|error| error.to_string())?;
    while !stop.load(Ordering::Acquire) {
        match client.read().map_err(|error| error.to_string())? {
            SensorServerReadOutcome::Reading(reading) => sender
                .try_send(LiveWorkerEvent::Reading(reading))
                .map_err(|error| error.to_string())?,
            SensorServerReadOutcome::TimedOut | SensorServerReadOutcome::ControlHandled(_) => {}
            SensorServerReadOutcome::Closed { code } => {
                return Err(format!("connection closed with code {code:?}"));
            }
        }
    }
    client.close().map_err(|error| error.to_string())
}

/// Debug-only operator-assisted physical Android -> DSU gate.
#[cfg(debug_assertions)]
pub fn run_gamepad_physical_gate(android_port: u16, dsu_port: u16) -> Result<(), String> {
    gamepad_lab::run_physical_gamepad_gate(android_port, dsu_port)
}

/// Debug-only Runtime-owned physical Android -> Windows DS4 Gate.
#[cfg(debug_assertions)]
pub fn run_windows_ds4_runtime_gate(android_port: u16, hold_seconds: u64) -> Result<(), String> {
    gamepad_lab::run_windows_ds4_runtime_gate(android_port, hold_seconds)
}

/// Debug-only Runtime-owned physical Android -> Windows DS4-only Gate.
#[cfg(debug_assertions)]
pub fn run_windows_ds4_only_runtime_gate(
    android_port: u16,
    hold_seconds: u64,
) -> Result<(), String> {
    gamepad_lab::run_windows_ds4_only_runtime_gate(android_port, hold_seconds)
}

/// Debug-only operator-assisted physical Android -> DSU + VIIPER gate.
#[cfg(debug_assertions)]
pub fn run_gamepad_viiper_physical_gate(
    android_port: u16,
    dsu_port: u16,
    viiper_port: u16,
    hold_seconds: u64,
) -> Result<(), String> {
    gamepad_lab::run_physical_viiper_gamepad_gate(android_port, dsu_port, viiper_port, hold_seconds)
}

/// Debug-only operator-assisted physical Android touch+IMU -> native DS4 gate.
#[cfg(debug_assertions)]
pub fn run_gamepad_ds4_physical_gate(
    android_port: u16,
    viiper_port: u16,
    hold_seconds: u64,
) -> Result<(), String> {
    gamepad_lab::run_physical_ds4_gamepad_gate(android_port, viiper_port, hold_seconds)
}

/// Debug-only fixed-config read-only Windows gamepad host preflight.
#[cfg(debug_assertions)]
pub fn run_windows_gamepad_read_only_preflight() -> Result<(), String> {
    windows_gamepad_host::run_read_only_preflight_gate()
}

pub fn run() {
    #[cfg(windows)]
    node_process::run_shell();
    #[cfg(not(windows))]
    {
        let state = Arc::new(HostState(Arc::new(desktop_host::DesktopHost::start())));
        tauri::Builder::default()
            .invoke_handler(move |invoke| {
                let command = invoke.message.command().to_owned();
                let tauri::ipc::InvokeBody::Json(args) = invoke.message.payload() else {
                    return false;
                };
                let result = host_commands::dispatch(&state, &command, args);
                invoke
                    .resolver
                    .respond(result.map_err(tauri::ipc::InvokeError::from));
                true
            })
            .run(tauri::generate_context!())
            .expect("run CapyIO");
    }
}

#[cfg(windows)]
pub fn run_node_host() -> Result<(), String> {
    node_process::run_host()
}

fn to_ui_snapshot(
    node: &DesktopNodeState,
    selected: &BTreeSet<CapabilityId>,
    control_plane: &DesktopControlPlaneState,
) -> UiSnapshot {
    let snapshot = node.runtime.snapshot();
    let local_node_id = snapshot.local_node.id;
    let bootstrap_peer_id = node
        .runtime
        .session(node.bootstrap_session_id)
        .ok()
        .map(|session| session.remote_node_id);
    let all_nodes = std::iter::once(&snapshot.local_node).chain(snapshot.peers.iter());
    let nodes = all_nodes
        .clone()
        .map(|node| UiNode {
            id: node.id.to_string(),
            display_name: node.display_name.clone(),
            platform: platform_label(&node.platform),
            platform_version: node.platform_version.clone(),
            online: node.online_state == OnlineState::Online,
            local: node.id == local_node_id,
            capability_count: node.capabilities.len(),
        })
        .collect();
    let capabilities = all_nodes
        .clone()
        .flat_map(|node| {
            let local = node.id == local_node_id;
            node.capabilities.values().map(move |capability| {
                let adapter_type = node
                    .adapter_instances
                    .get(&capability.adapter_instance_id)
                    .map(|adapter| adapter.adapter_type.as_str())
                    .unwrap_or("unknown");
                let simulated = Some(node.id) == bootstrap_peer_id
                    || (local
                        && matches!(
                            adapter_type,
                            "capyio.windows.audio" | "capyio.windows.projection"
                        ));
                let selectable_for_sharing = local
                    && capability.availability != Availability::Offline
                    && capability
                        .ports
                        .values()
                        .any(|port| port.direction == PortDirection::Source);
                UiCapability {
                    id: capability.id.to_string(),
                    node_id: node.id.to_string(),
                    node_name: node.display_name.clone(),
                    display_name: capability.display_name.clone(),
                    capability_class: capability_class_label(&capability.class),
                    local,
                    availability: availability_label(capability.availability),
                    permission_requirement: permission_requirement_label(
                        capability.permission_requirement,
                    ),
                    inventory_source: if simulated {
                        "runtime_catalog"
                    } else if local {
                        "host_adapter_catalog"
                    } else {
                        "authenticated_peer_catalog"
                    },
                    simulated,
                    selectable_for_sharing,
                    selected_for_sharing: selectable_for_sharing
                        && selected.contains(&capability.id),
                    ports: capability
                        .ports
                        .values()
                        .map(|port| UiCapabilityPort {
                            id: port.id.to_string(),
                            display_name: port.display_name.clone(),
                            direction: port_direction_label(port.direction),
                            profile: format!("{}/{}", port.profile.name, port.profile.major),
                            formats: port
                                .formats
                                .iter()
                                .map(|format| format.id.clone())
                                .collect(),
                        })
                        .collect(),
                }
            })
        })
        .collect();
    let adapters = all_nodes
        .clone()
        .flat_map(|node| {
            node.adapter_instances
                .values()
                .map(move |adapter| UiAdapter {
                    id: adapter.id.to_string(),
                    node_name: node.display_name.clone(),
                    display_name: adapter.display_name.clone(),
                    adapter_type: adapter.adapter_type.clone(),
                    deployment_mode: deployment_label(adapter.deployment_mode).to_owned(),
                    state: format!("{:?}", adapter.state).to_lowercase(),
                    health: format!("{:?}", adapter.health).to_lowercase(),
                    capability_count: adapter.owned_capabilities.len(),
                })
        })
        .collect();
    let routes = snapshot
        .routes
        .iter()
        // Real pending/starting Routes must be visible too: the resource UI
        // derives waiting/Cancel/Stop from this authoritative runtime list.
        .map(|route| {
            let mut ui = route_to_ui(
                route,
                &snapshot.local_node,
                &snapshot.peers,
                node.demo_routes.all().contains(&route.id),
            );
            ui.error = snapshot
                .problems
                .iter()
                .rev()
                .find(|problem| route.diagnostic_ids.contains(&problem.id))
                .map(|problem| {
                    format!(
                        "{}：{}",
                        problem.human_message,
                        problem.technical_detail.as_deref().unwrap_or(&problem.code)
                    )
                });
            ui
        })
        .collect();
    let events = snapshot
        .events
        .iter()
        .map(|event| UiEvent {
            sequence: event.sequence,
            summary: format!("{:?}", event.kind),
        })
        .collect();
    let port_leases = snapshot
        .port_leases
        .iter()
        .map(|lease| UiPortLease {
            node_id: lease.port.node_id.to_string(),
            capability_id: lease.port.capability_id.to_string(),
            port_id: lease.port.port_id.to_string(),
            route_id: lease.route_id.to_string(),
            session_id: lease.session_id.to_string(),
            expires_at_ms: lease.expires_at_ms,
        })
        .collect();

    UiSnapshot {
        backend_mode: "tauri_demo",
        schema_version: 5,
        project_version: env!("CARGO_PKG_VERSION"),
        control_plane: UiControlPlaneStatus {
            schema_version: 1,
            simulated: false,
            discovery_state: if control_plane.discovery_listening {
                "listening"
            } else {
                "not_started"
            },
            pairing_state: if control_plane.credential_store_failed {
                "failed"
            } else if control_plane.credential_store_ready {
                "ready"
            } else {
                "not_configured"
            },
            transport_state: if control_plane.authenticated_connection_count > 0 {
                "connected"
            } else {
                "not_connected"
            },
            catalog_exchange_state: if control_plane.catalog_exchange_active {
                "active"
            } else {
                "not_connected"
            },
            discovered_candidate_count: control_plane.discovery_ingress.candidates().len(),
            paired_peer_count: control_plane.paired_peers.peers().len(),
            authenticated_connection_count: control_plane.authenticated_connection_count,
            listen_port: None,
            pairing_store_path: control_plane.pairing_store.as_ref().map(|store| store.directory_path().display().to_string()),
            process_user_sid: current_process_sid_for_ui(),
        },
        nodes,
        capabilities,
        routes,
        port_leases,
        adapters,
        events,
        warnings: vec![
            "Tauri Demo 的 Route/授权/指标仍为确定性数据；只有显式启动的 Physical IMU Lab 会访问一个经校验的 IP 端点。".to_owned(),
            "能力目录当前由确定性 bootstrap 初始化；选择仅保存在当前进程，不代表真实硬件枚举、授权或网络发布。".to_owned(),
        ],
        imu_fixture: build_imu_fixture(),
    }
}

fn update_local_capability_selection(
    runtime: &NodeRuntime,
    selected: &mut BTreeSet<CapabilityId>,
    capability_id: CapabilityId,
    enabled: bool,
) -> Result<(), String> {
    let snapshot = runtime.snapshot();
    snapshot
        .local_node
        .capabilities
        .get(&capability_id)
        .ok_or_else(|| "Capability is not advertised by the local Node".to_owned())?;
    let mut candidate = selected.clone();
    if enabled {
        candidate.insert(capability_id);
    } else {
        candidate.remove(&capability_id);
    }
    project_publication_catalog(&snapshot.local_node, &candidate)
        .map_err(|error| error.to_string())?;
    *selected = candidate;
    Ok(())
}

fn build_imu_fixture() -> UiImuFixture {
    const FIXTURE: &str = include_str!("../../../../fixtures/imu/imu_samples_v1.jsonl");
    let envelopes = parse_imu_fixture_jsonl(FIXTURE, 64).expect("valid compiled IMU fixture");
    let first = envelopes.first().expect("non-empty compiled IMU fixture");
    let mut fanout =
        BoundedFanout::new(ImuSampleV1::profile(), first.stream_id, first.stream_epoch);
    fanout
        .register_consumer("numeric-panel", 64)
        .expect("valid Panel queue");
    fanout
        .register_consumer("jsonl-recorder", 64)
        .expect("valid Recorder queue");
    for envelope in envelopes.iter().cloned() {
        let outcomes = fanout.publish(envelope);
        assert!(outcomes.values().all(Result::is_ok));
    }
    let mut panel = NumericImuPanel::default();
    let mut recorder = BoundedJsonlRecorder::new(64, 4096).expect("valid Recorder bounds");
    while let Some(delivery) = fanout.pop("numeric-panel").expect("registered Panel") {
        panel.consume(delivery);
    }
    while let Some(delivery) = fanout.pop("jsonl-recorder").expect("registered Recorder") {
        assert_eq!(
            recorder.record(&delivery).expect("serializable delivery"),
            RecorderOutcome::Recorded
        );
    }
    let last_envelope = envelopes.last().expect("non-empty compiled IMU fixture");
    let sample = panel
        .last_sample
        .expect("numeric Panel consumed compiled IMU fixture");
    UiImuFixture {
        mode: "deterministic_fixture",
        simulated: true,
        profile: "capyio.motion.imu-samples/1",
        sequence: last_envelope.sequence,
        source_timestamp_nanos: last_envelope.source_timestamp_nanos,
        clock_domain_id: last_envelope.clock_domain_id.clone(),
        acceleration: vector3(sample.acceleration),
        angular_velocity: vector3(sample.angular_velocity),
        panel_received: panel.received,
        panel_missing_sequences: panel.missing_sequences,
        recorder_records: recorder.len(),
        panel_route_state: "active",
        recorder_route_state: "active",
    }
}

fn vector3(value: [f64; 3]) -> UiVector3 {
    UiVector3 {
        x: value[0],
        y: value[1],
        z: value[2],
    }
}

fn route_to_ui(
    route: &Route,
    local: &NodeDescriptor,
    peers: &[NodeDescriptor],
    simulated: bool,
) -> UiRoute {
    let source = resolve_port(route.source, local, peers);
    let sink = resolve_port(route.sink, local, peers);
    let active = route.state == RouteState::Active;
    let title = format!("{} → {}", source.capability_name, sink.capability_name);
    let summary = format!("{} → {}", source.node_name, sink.node_name);
    let format_summary = route
        .selected_format
        .as_ref()
        .or_else(|| route.compatible_formats.first())
        .map(|format| format.id.clone());
    let qos_modes = route
        .compatible_qos_modes
        .iter()
        .map(qos_label)
        .map(str::to_owned)
        .collect();
    let projection_note = if !simulated {
        "由目标节点的 Adapter 提供；就绪状态见 Route，媒体观测见诊断"
    } else {
        match sink.capability_class.as_str() {
            "panel" => "CapyIO 应用内 Panel（模拟）",
            "gamepad" => "本地游戏手柄投影（best-effort，模拟）",
            "microphone" => "Windows 系统端点投影（驱动尚未实现）",
            "speaker" => "Android 应用内播放端（模拟）",
            _ => "标准 Port Sink",
        }
    }
    .to_owned();

    UiRoute {
        id: route.id.to_string(),
        error: None,
        source_capability_id: Some(route.source.capability_id.to_string()),
        sink_capability_id: Some(route.sink.capability_id.to_string()),
        title,
        summary,
        profile: format!("{}/{}", route.profile.name, route.profile.major),
        backend: format!("{:?}", route.backend).to_lowercase(),
        state: route_state_label(route.state).to_owned(),
        active,
        authorization: match route.authorization {
            AuthorizationState::Pending => "pending",
            AuthorizationState::Authorized { .. } => "authorized",
            AuthorizationState::Denied => "denied",
            AuthorizationState::Revoked => "revoked",
        },
        authorization_expires_at_ms: match route.authorization {
            AuthorizationState::Authorized { expires_at_ms } => expires_at_ms,
            _ => None,
        },
        source,
        sink,
        format_summary,
        qos_modes,
        projection_note,
        metrics: if simulated {
            demo_metrics(&route.profile.name, active)
        } else {
            UiMetricSet {
                estimated_latency_ms: None,
                packet_loss_percent: None,
                buffer_fill_ms: None,
                simulated: false,
            }
        },
    }
}

fn resolve_port(reference: PortRef, local: &NodeDescriptor, peers: &[NodeDescriptor]) -> UiPort {
    let node = std::iter::once(local)
        .chain(peers.iter())
        .find(|node| node.id == reference.node_id)
        .expect("demo Route references advertised Node");
    let capability = node
        .capabilities
        .get(&reference.capability_id)
        .expect("demo Route references advertised Capability");
    let port = capability
        .ports
        .get(&reference.port_id)
        .expect("demo Route references advertised Port");
    UiPort {
        node_name: node.display_name.clone(),
        capability_name: capability.display_name.clone(),
        capability_class: capability_class_label(&capability.class),
        port_name: port.display_name.clone(),
        direction: port_direction_label(port.direction).to_owned(),
    }
}

fn capability_class_label(value: &CapabilityClass) -> String {
    match value {
        CapabilityClass::Custom(custom) => custom.clone(),
        other => format!("{other:?}").to_lowercase(),
    }
}

fn platform_label(value: &capyio_core::Platform) -> String {
    match value {
        capyio_core::Platform::Unknown(detail) => detail.clone(),
        other => format!("{other:?}").to_lowercase(),
    }
}

const fn deployment_label(value: AdapterDeploymentMode) -> &'static str {
    match value {
        AdapterDeploymentMode::InProcess => "in_process",
        AdapterDeploymentMode::Sidecar => "sidecar",
        AdapterDeploymentMode::ExternalService => "external_service",
        AdapterDeploymentMode::DriverBacked => "driver_backed",
    }
}

const fn port_direction_label(value: PortDirection) -> &'static str {
    match value {
        PortDirection::Source => "source",
        PortDirection::Sink => "sink",
        PortDirection::Control => "control",
    }
}

const fn availability_label(value: Availability) -> &'static str {
    match value {
        Availability::Available => "available",
        Availability::Busy => "busy",
        Availability::PermissionRequired => "permission_required",
        Availability::Offline => "offline",
    }
}

const fn permission_requirement_label(value: PermissionRequirement) -> &'static str {
    match value {
        PermissionRequirement::None => "none",
        PermissionRequirement::UserConfirmation => "user_confirmation",
        PermissionRequirement::ForegroundService => "foreground_service",
        PermissionRequirement::Privileged => "privileged",
    }
}

fn qos_label(value: &QosMode) -> &str {
    match value {
        QosMode::Basic => "basic",
        QosMode::Interactive => "interactive",
        QosMode::Measurement => "measurement",
        QosMode::Custom(custom) => custom,
    }
}

const fn route_state_label(value: RouteState) -> &'static str {
    match value {
        RouteState::Draft => "draft",
        RouteState::Prepared => "prepared",
        RouteState::Starting => "starting",
        RouteState::Active => "active",
        RouteState::Stopping => "stopping",
        RouteState::Stopped => "stopped",
        RouteState::Failed => "failed",
        RouteState::Offline => "offline",
    }
}

fn demo_metrics(profile: &str, active: bool) -> UiMetricSet {
    if !active {
        return UiMetricSet {
            estimated_latency_ms: None,
            packet_loss_percent: None,
            buffer_fill_ms: None,
            simulated: true,
        };
    }
    let (latency, loss, buffer) = if profile.contains("audio") {
        (47.3, 0.03, 30.0)
    } else if profile.contains("video") {
        (81.2, 0.07, 66.0)
    } else {
        (18.6, 0.01, 12.0)
    };
    UiMetricSet {
        estimated_latency_ms: Some(latency),
        packet_loss_percent: Some(loss),
        buffer_fill_ms: Some(buffer),
        simulated: true,
    }
}

fn unix_time_ms() -> Result<u64, String> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "system clock is before UNIX epoch".to_owned())?;
    u64::try_from(elapsed.as_millis()).map_err(|_| "system time is out of range".to_owned())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        net::{Ipv4Addr, TcpListener},
        path::PathBuf,
    };

    use tungstenite::{Message, accept};

    use super::*;

    #[test]
    fn probe_heartbeat_accepts_bounded_settings_interleave_only() {
        let settings = || {
            ReceivedControlMessage::ControllerSettingsState(
                capyio_protocol::v1::ControllerSettingsState {
                    revision: 1,
                    motion_enabled: true,
                    haptics_strength: 70,
                    request_id: 0,
                    error: String::new(),
                },
            )
        };
        let heartbeat = |sequence| {
            ReceivedControlMessage::Heartbeat(capyio_protocol::ValidatedControlHeartbeat {
                message_id: capyio_core::MessageId::new(),
                session_id: capyio_core::SessionId::new(),
                sequence,
                acknowledgement: true,
            })
        };
        let mut messages = vec![settings(), heartbeat(1)].into_iter();
        assert!(receive_probe_heartbeat(|| Ok(messages.next().unwrap())).is_ok());
        assert!(receive_probe_heartbeat(|| Ok(settings())).is_err());
        assert!(receive_probe_heartbeat(|| Ok(heartbeat(2))).is_err());
        assert!(
            receive_probe_heartbeat(|| Ok(ReceivedControlMessage::ControllerSettingsRequest(
                capyio_protocol::v1::ControllerSettingsRequest {
                    request_id: 1,
                    expected_revision: 1,
                    motion_enabled: true,
                    haptics_strength: 70,
                }
            )))
            .is_err()
        );
    }

    struct TemporaryDirectory(PathBuf);

    impl TemporaryDirectory {
        fn new(name: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("capyio-desktop-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir(&path).expect("create temporary desktop test directory");
            Self(path)
        }
    }

    impl Drop for TemporaryDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn physical_node() -> (DesktopNodeState, PhysicalImuRoute) {
        let mut node = DesktopNodeState::demo().expect("demo Runtime bootstrap");
        let session_id = node.bootstrap_session_id;
        let route = PhysicalImuRoute::install(&mut node.runtime, session_id)
            .expect("physical Route fixture");
        (node, route)
    }

    #[test]
    fn idle_live_imu_dto_is_explicitly_real_but_inactive() {
        let (node, route) = physical_node();
        let snapshot = UiLiveImu::idle(route, &node.runtime).expect("idle DTO");
        assert_eq!(snapshot.status, "idle");
        assert!(!snapshot.simulated);
        assert_eq!(snapshot.route_state, "draft");
        assert_eq!(snapshot.stream_epoch, 0);
        assert!(snapshot.acceleration.is_none());
        assert!(snapshot.problem.is_none());
    }

    #[test]
    fn capability_catalog_separates_local_sharing_from_peer_inventory() {
        let node = DesktopNodeState::demo().expect("demo Runtime bootstrap");
        let runtime = node.runtime.snapshot();
        let local_source = runtime
            .local_node
            .capabilities
            .values()
            .find(|capability| {
                capability
                    .ports
                    .values()
                    .any(|port| port.direction == PortDirection::Source)
            })
            .expect("local Source Capability")
            .id;
        let local_sink = runtime
            .local_node
            .capabilities
            .values()
            .find(|capability| {
                capability
                    .ports
                    .values()
                    .all(|port| port.direction == PortDirection::Sink)
            })
            .expect("local Sink-only Capability")
            .id;
        let peer_source = runtime
            .peers
            .first()
            .expect("peer Node")
            .capabilities
            .values()
            .find(|capability| {
                capability
                    .ports
                    .values()
                    .any(|port| port.direction == PortDirection::Source)
            })
            .expect("peer Source Capability")
            .id;
        let mut selected = BTreeSet::new();

        update_local_capability_selection(&node.runtime, &mut selected, local_source, true)
            .expect("select local Source");
        assert!(
            update_local_capability_selection(&node.runtime, &mut selected, local_sink, true)
                .is_err()
        );
        assert!(
            update_local_capability_selection(&node.runtime, &mut selected, peer_source, true)
                .is_err()
        );

        let control_plane =
            DesktopControlPlaneState::new(runtime.local_node.id).expect("control plane");
        let dto = to_ui_snapshot(&node, &selected, &control_plane);
        assert_eq!(dto.schema_version, 5);
        assert_eq!(dto.control_plane.schema_version, 1);
        assert!(!dto.control_plane.simulated);
        assert_eq!(dto.control_plane.discovery_state, "not_started");
        assert_eq!(dto.control_plane.pairing_state, "not_configured");
        assert_eq!(dto.control_plane.transport_state, "not_connected");
        assert_eq!(dto.control_plane.catalog_exchange_state, "not_connected");
        assert_eq!(dto.control_plane.discovered_candidate_count, 0);
        assert_eq!(dto.control_plane.paired_peer_count, 0);
        assert_eq!(dto.control_plane.authenticated_connection_count, 0);
        assert!(dto.capabilities.iter().any(|capability| {
            capability.local
                && capability.id == local_source.to_string()
                && capability.selectable_for_sharing
                && capability.selected_for_sharing
        }));
        assert!(dto.capabilities.iter().any(|capability| {
            !capability.local
                && capability.id == peer_source.to_string()
                && !capability.selectable_for_sharing
                && !capability.selected_for_sharing
        }));
        assert_eq!(
            dto.capabilities
                .iter()
                .filter(|capability| capability.capability_class == "touchpad")
                .count(),
            2
        );
        for route in &dto.routes {
            let source = runtime
                .routes
                .iter()
                .find(|candidate| candidate.id.to_string() == route.id)
                .expect("UI Route remains Runtime-owned");
            assert_eq!(
                route.source_capability_id.as_deref(),
                Some(source.source.capability_id.to_string()).as_deref()
            );
            assert_eq!(
                route.sink_capability_id.as_deref(),
                Some(source.sink.capability_id.to_string()).as_deref()
            );
            assert_ne!(route.authorization, "unknown");
        }
    }

    #[test]
    fn pending_non_demo_route_is_visible_for_resource_cancel() {
        let mut node = DesktopNodeState::demo().unwrap();
        let template = node
            .runtime
            .route(node.demo_routes.phone_microphone_to_windows)
            .unwrap()
            .clone();
        let id = RouteId::new();
        node.runtime
            .create_route_with_id(
                id,
                template.session_id,
                template.source,
                template.sink,
                template.backend,
            )
            .unwrap();
        assert!(!node.demo_routes.all().contains(&id));
        let control = DesktopControlPlaneState::new(node.runtime.snapshot().local_node.id).unwrap();
        let dto = to_ui_snapshot(&node, &BTreeSet::new(), &control);
        let pending = dto
            .routes
            .iter()
            .find(|route| route.id == id.to_string())
            .expect("real pending Route must reach resource UI");
        assert_eq!(pending.state, "draft");
        assert_eq!(pending.authorization, "pending");
        assert!(!pending.active);
        assert!(!pending.metrics.simulated);
        let snapshot = node.runtime.snapshot();
        let mut active = node.runtime.route(id).unwrap().clone();
        active.state = RouteState::Active;
        let real = route_to_ui(&active, &snapshot.local_node, &snapshot.peers, false);
        assert!(real.metrics.estimated_latency_ms.is_none());
        assert!(real.metrics.packet_loss_percent.is_none());
        assert!(real.metrics.buffer_fill_ms.is_none());
        assert!(!real.projection_note.contains("模拟"));
        let demo = route_to_ui(&active, &snapshot.local_node, &snapshot.peers, true);
        assert!(demo.metrics.simulated);
        assert!(demo.metrics.estimated_latency_ms.is_some());
        assert_eq!(
            pending.source_capability_id.as_deref(),
            Some(template.source.capability_id.to_string()).as_deref()
        );
        assert_eq!(
            pending.sink_capability_id.as_deref(),
            Some(template.sink.capability_id.to_string()).as_deref()
        );
        node.runtime
            .report_route_offline(
                id,
                capyio_core::Problem {
                    id: capyio_core::ProblemId::new(),
                    code: "CAPY.AUDIO_ROUTE.FAILED".into(),
                    category: capyio_core::ProblemCategory::Transport,
                    severity: capyio_core::ProblemSeverity::Warning,
                    retryable: true,
                    related_node: None,
                    related_adapter: None,
                    related_route: Some(id),
                    human_message: "音频启动失败".into(),
                    technical_detail: Some("phase=worker; exact failure".into()),
                },
            )
            .unwrap();
        let dto = to_ui_snapshot(&node, &BTreeSet::new(), &control);
        let failed = dto
            .routes
            .iter()
            .find(|route| route.id == id.to_string())
            .unwrap();
        assert_eq!(failed.state, "offline");
        assert!(
            failed
                .error
                .as_deref()
                .unwrap()
                .contains("phase=worker; exact failure")
        );
    }

    #[test]
    fn desktop_control_plane_snapshot_comes_from_host_owned_state() {
        use capyio_discovery::DiscoveryAnnouncement;
        use capyio_pairing::{CertificateFingerprint, CredentialHandle, PairedPeerRecord};

        let node = DesktopNodeState::demo().expect("demo Runtime bootstrap");
        let mut control_plane =
            DesktopControlPlaneState::new(node.runtime.snapshot().local_node.id)
                .expect("control plane");
        let peer_id = capyio_core::NodeId::new();
        control_plane
            .discovery_ingress
            .ingest(
                Ipv4Addr::new(192, 168, 1, 20).into(),
                &DiscoveryAnnouncement::new(peer_id, 42_420, 7)
                    .expect("announcement")
                    .encode(),
                100,
            )
            .expect("candidate");
        control_plane
            .paired_peers
            .add(
                PairedPeerRecord::new(
                    peer_id,
                    "Test peer",
                    "peer.capyio.test",
                    CredentialHandle::new("os-keystore:test-peer").expect("handle"),
                    CertificateFingerprint::from_certificate_der(b"test certificate")
                        .expect("fingerprint"),
                    90,
                )
                .expect("peer record"),
            )
            .expect("paired peer");
        control_plane.discovery_listening = true;
        control_plane.credential_store_ready = true;
        control_plane.authenticated_connection_count = 1;
        control_plane.catalog_exchange_active = true;

        let dto = to_ui_snapshot(&node, &BTreeSet::new(), &control_plane);
        assert_eq!(dto.control_plane.discovery_state, "listening");
        assert_eq!(dto.control_plane.pairing_state, "ready");
        assert_eq!(dto.control_plane.transport_state, "connected");
        assert_eq!(dto.control_plane.catalog_exchange_state, "active");
        assert_eq!(dto.control_plane.discovered_candidate_count, 1);
        assert_eq!(dto.control_plane.paired_peer_count, 1);
        assert_eq!(dto.control_plane.authenticated_connection_count, 1);

        control_plane.credential_store_ready = false;
        control_plane.credential_store_failed = true;
        let failed = to_ui_snapshot(&node, &BTreeSet::new(), &control_plane);
        assert_eq!(failed.control_plane.pairing_state, "failed");
    }

    #[test]
    fn authenticated_peer_catalog_is_not_labeled_as_demo_inventory() {
        let mut node = DesktopNodeState::demo().expect("demo Runtime bootstrap");
        let mut peer = capyio_testkit::android_node();
        peer.id = NodeId::new();
        peer.display_name = "Authenticated tablet".to_owned();
        let peer_id = peer.id;
        node.runtime.register_peer(peer, true).expect("real peer");
        node.runtime.open_session(peer_id).expect("peer Session");
        let control = DesktopControlPlaneState::new(node.runtime.snapshot().local_node.id)
            .expect("control plane");

        let dto = to_ui_snapshot(&node, &BTreeSet::new(), &control);
        let capabilities = dto
            .capabilities
            .iter()
            .filter(|capability| capability.node_id == peer_id.to_string())
            .collect::<Vec<_>>();
        assert!(!capabilities.is_empty());
        assert!(capabilities.iter().all(|capability| !capability.simulated));
        assert!(
            capabilities
                .iter()
                .all(|capability| capability.inventory_source == "authenticated_peer_catalog")
        );
    }

    #[test]
    fn desktop_control_plane_recovers_paired_peer_metadata_from_store() {
        use capyio_pairing::{CertificateFingerprint, CredentialHandle, PairedPeerRecord};

        let temp = TemporaryDirectory::new("pairing-store");
        let local = capyio_core::NodeId::new();
        let peer = capyio_core::NodeId::new();
        let store = WindowsPairingStore::from_local_app_data(&temp.0, local).expect("store");
        let mut directory = PairedPeerDirectory::new(local).expect("directory");
        directory
            .add(
                PairedPeerRecord::new(
                    peer,
                    "Recovered tablet",
                    "tablet.capyio.test",
                    CredentialHandle::new(format!("windows-dpapi:{}", "a".repeat(64)))
                        .expect("handle"),
                    CertificateFingerprint::new([9; 32]).expect("fingerprint"),
                    100,
                )
                .expect("record"),
            )
            .expect("add record");
        store
            .commit_directory(&directory)
            .expect("commit directory");

        let mut control = DesktopControlPlaneState::new(local).expect("control plane");
        control
            .attach_pairing_store(store.clone())
            .expect("load store");
        assert!(control.credential_store_ready);
        assert!(!control.credential_store_failed);
        assert_eq!(control.paired_peers.peers().count(), 1);
        assert_eq!(control.paired_peers.peers().next().unwrap().node_id, peer);
        assert_eq!(
            control.pairing_store.as_ref().unwrap().directory_path(),
            store.directory_path()
        );
        let metadata = serde_json::to_value(paired_peer_metadata(&control)).unwrap();
        assert_eq!(
            metadata,
            serde_json::json!([{
                "nodeId": peer.to_string(), "displayName": "Recovered tablet", "revoked": false,
                "pairedAtMs": 100
            }])
        );
        control
            .discovery_ingress
            .ingest(
                Ipv4Addr::new(192, 168, 1, 50).into(),
                &capyio_discovery::DiscoveryAnnouncement::new(peer, 42420, 9)
                    .unwrap()
                    .encode(),
                100,
            )
            .unwrap();
        let plan = control
            .resolve_control_dial(peer, 101)
            .expect("paired fresh dial plan");
        assert_eq!(plan.candidate.node_id, peer);
        assert_eq!(plan.candidate.endpoint.to_string(), "192.168.1.50:42420");
        assert_eq!(plan.display_name, "Recovered tablet");
        assert_eq!(
            plan.candidate.credential_handle.as_str(),
            format!("windows-dpapi:{}", "a".repeat(64))
        );
        let manual = control
            .resolve_manual_control_dial(
                peer,
                "100.110.59.35:42095".parse().expect("manual endpoint"),
            )
            .expect("active paired manual dial plan");
        assert_eq!(manual.candidate.endpoint.to_string(), "100.110.59.35:42095");
        assert!(
            verify_credential_readable(&store, &manual.candidate.credential_handle)
                .unwrap_err()
                .contains("当前进程账号无法读取")
        );
        assert_eq!(manual.candidate.instance_nonce, 0);
        assert!(
            control
                .resolve_manual_control_dial(
                    NodeId::new(),
                    "192.168.1.9:42420".parse().expect("unpaired endpoint")
                )
                .is_err()
        );
        control.paired_peers.revoke(peer, 200).unwrap();
        store.commit_directory(&control.paired_peers).unwrap();
        assert!(paired_peer_metadata(&control)[0].revoked);
        assert!(
            control
                .resolve_manual_control_dial(peer, "192.168.1.50:42420".parse().unwrap())
                .is_err()
        );
    }

    #[test]
    fn pairing_dials_refresh_replaced_revoked_and_corrupt_records() {
        use capyio_pairing::{CertificateFingerprint, CredentialHandle, PairedPeerRecord};

        let temp = TemporaryDirectory::new("pairing-refresh");
        let local = NodeId::new();
        let peer = NodeId::new();
        let store = WindowsPairingStore::from_local_app_data(&temp.0, local).unwrap();
        let directory = |name: &str, byte: u8| {
            let mut directory = PairedPeerDirectory::new(local).unwrap();
            directory
                .add(
                    PairedPeerRecord::new(
                        peer,
                        name,
                        "tablet.capyio.test",
                        CredentialHandle::new(format!(
                            "windows-dpapi:{}",
                            format!("{byte:02x}").repeat(32)
                        ))
                        .unwrap(),
                        CertificateFingerprint::new([byte; 32]).unwrap(),
                        100,
                    )
                    .unwrap(),
                )
                .unwrap();
            directory
        };
        store
            .commit_directory(&directory("Old pairing", 1))
            .unwrap();
        let mut control = DesktopControlPlaneState::new(local).unwrap();
        control.attach_pairing_store(store.clone()).unwrap();
        let endpoint = "192.168.1.50:42420".parse().unwrap();
        control
            .discovery_ingress
            .ingest(
                Ipv4Addr::new(192, 168, 1, 50).into(),
                &capyio_discovery::DiscoveryAnnouncement::new(peer, 42420, 9)
                    .unwrap()
                    .encode(),
                100,
            )
            .unwrap();

        // Simulate a pairing update by another local process while the UI lives.
        store
            .commit_directory(&directory("Current pairing", 2))
            .unwrap();
        let manual = control.resolve_manual_control_dial(peer, endpoint).unwrap();
        assert_eq!(manual.display_name, "Current pairing");
        assert_eq!(
            manual.candidate.peer_certificate_sha256,
            CertificateFingerprint::new([2; 32]).unwrap()
        );
        assert_eq!(
            manual.candidate.credential_handle.as_str(),
            format!("windows-dpapi:{}", "02".repeat(32))
        );
        store
            .commit_directory(&directory("Discovery pairing", 3))
            .unwrap();
        assert_eq!(
            control
                .resolve_control_dial(peer, 101)
                .unwrap()
                .display_name,
            "Discovery pairing"
        );
        assert_eq!(
            paired_peer_metadata(&control)[0].display_name,
            "Discovery pairing"
        );

        let mut revoked = directory("Revoked pairing", 3);
        revoked.revoke(peer, 200).unwrap();
        store.commit_directory(&revoked).unwrap();
        assert!(control.resolve_manual_control_dial(peer, endpoint).is_err());
        assert!(control.resolve_control_dial(peer, 201).is_err());
        assert!(paired_peer_metadata(&control)[0].revoked);

        store
            .commit_directory(&directory("Restored pairing", 4))
            .unwrap();
        control.refresh_pairing_directory().unwrap();
        fs::write(temp.0.join("CapyIO/Control/Pairing/peers-v1.json"), b"{}").unwrap();
        assert!(
            control
                .resolve_manual_control_dial(peer, endpoint)
                .err()
                .expect("corrupt store must fail")
                .contains("pairing directory refresh failed")
        );
        assert!(control.resolve_control_dial(peer, 202).is_err());
        assert!(!control.credential_store_ready);
        assert!(control.credential_store_failed);

        // A missing directory is empty trust, never permission to use old keys.
        store
            .commit_directory(&PairedPeerDirectory::new(local).unwrap())
            .unwrap();
        assert!(control.resolve_manual_control_dial(peer, endpoint).is_err());
        assert_eq!(control.paired_peers.peers().count(), 0);
    }

    #[test]
    fn desktop_control_plane_rejects_corrupt_pairing_directory_without_partial_load() {
        let temp = TemporaryDirectory::new("corrupt-pairing-store");
        let local = capyio_core::NodeId::new();
        let store = WindowsPairingStore::from_local_app_data(&temp.0, local).expect("store");
        let root = temp.0.join("CapyIO/Control/Pairing");
        fs::create_dir_all(&root).expect("pairing root");
        fs::write(root.join("peers-v1.json"), b"{}").expect("corrupt directory");

        let mut control = DesktopControlPlaneState::new(local).expect("control plane");
        assert!(control.attach_pairing_store(store).is_err());
        assert!(!control.credential_store_ready);
        assert_eq!(control.paired_peers.peers().count(), 0);
    }

    #[test]
    fn discovered_device_observer_expires_candidates_even_after_listener_stop() {
        use capyio_discovery::{DISCOVERY_CANDIDATE_TTL_MS, DiscoveryAnnouncement};
        let local = capyio_core::NodeId::new();
        let peer = capyio_core::NodeId::new();
        let mut control = DesktopControlPlaneState::new(local).unwrap();
        control
            .discovery_ingress
            .ingest(
                Ipv4Addr::new(192, 168, 1, 20).into(),
                &DiscoveryAnnouncement::new(peer, 42420, 7).unwrap().encode(),
                100,
            )
            .unwrap();
        let observed = discovery_candidates(&mut control, 200);
        assert_eq!(observed.len(), 1);
        assert_eq!(observed[0].node_id, peer.to_string());
        assert_eq!(observed[0].host, "192.168.1.20");
        assert_eq!(observed[0].control_port, 42420);
        assert_eq!(observed[0].last_seen_age_ms, 100);
        assert_eq!(observed[0].pairing_state, "unpaired");
        assert!(!control.discovery_listening);
        assert_eq!(control.paired_peers.peers().len(), 0);
        assert_eq!(control.authenticated_connection_count, 0);
        assert!(discovery_candidates(&mut control, 101 + DISCOVERY_CANDIDATE_TTL_MS).is_empty());
        assert_eq!(control.discovery_ingress.candidates().len(), 0);
    }

    #[test]
    fn discovered_device_observer_projects_pairing_state_without_authentication() {
        use capyio_discovery::DiscoveryAnnouncement;
        use capyio_pairing::{CertificateFingerprint, CredentialHandle, PairedPeerRecord};

        let local = capyio_core::NodeId::new();
        let active_peer = capyio_core::NodeId::new();
        let revoked_peer = capyio_core::NodeId::new();
        let mut control = DesktopControlPlaneState::new(local).unwrap();
        for (peer, nonce) in [(active_peer, 7), (revoked_peer, 8)] {
            control
                .discovery_ingress
                .ingest(
                    Ipv4Addr::new(192, 168, 1, nonce as u8).into(),
                    &DiscoveryAnnouncement::new(peer, 42420, nonce)
                        .unwrap()
                        .encode(),
                    100,
                )
                .unwrap();
            control
                .paired_peers
                .add(
                    PairedPeerRecord::new(
                        peer,
                        format!("Peer {nonce}"),
                        format!("peer-{nonce}.capyio.test"),
                        CredentialHandle::new(format!("test:{nonce}")).unwrap(),
                        CertificateFingerprint::new([nonce as u8; 32]).unwrap(),
                        90,
                    )
                    .unwrap(),
                )
                .unwrap();
        }
        control.paired_peers.revoke(revoked_peer, 110).unwrap();

        let observed = discovery_candidates(&mut control, 200);
        assert_eq!(observed.len(), 2);
        assert_eq!(
            observed
                .iter()
                .find(|candidate| candidate.node_id == active_peer.to_string())
                .unwrap()
                .pairing_state,
            "paired"
        );
        assert_eq!(
            observed
                .iter()
                .find(|candidate| candidate.node_id == revoked_peer.to_string())
                .unwrap()
                .pairing_state,
            "revoked"
        );
        assert_eq!(control.authenticated_connection_count, 0);
        assert!(!control.catalog_exchange_active);
    }

    #[test]
    fn desktop_node_only_exposes_fixture_routes_to_demo_mutation() {
        let (mut node, physical_imu) = physical_node();
        let before = node.runtime.snapshot();

        let error = node
            .set_demo_route_active(physical_imu.route_id(), true, 1)
            .expect_err("physical Route must use its own lifecycle boundary");

        assert_eq!(error, "Route is not controlled by the deterministic demo");
        assert_eq!(node.runtime.snapshot(), before);
    }

    #[test]
    fn desktop_can_remove_stale_publication_intent_after_source_goes_offline() {
        let mut node = DesktopNodeState::demo().expect("demo Runtime bootstrap");
        let snapshot = node.runtime.snapshot();
        let source = snapshot
            .local_node
            .capabilities
            .values()
            .find(|capability| {
                capability.availability != Availability::Offline
                    && capability
                        .ports
                        .values()
                        .any(|port| port.direction == PortDirection::Source)
            })
            .expect("available Source Capability");
        let capability_id = source.id;
        let adapter_id = source.adapter_instance_id;
        let mut selected = BTreeSet::new();
        update_local_capability_selection(&node.runtime, &mut selected, capability_id, true)
            .expect("select Source");

        let mut replacement: Vec<_> = snapshot
            .local_node
            .capabilities
            .values()
            .filter(|capability| capability.adapter_instance_id == adapter_id)
            .cloned()
            .collect();
        replacement
            .iter_mut()
            .find(|capability| capability.id == capability_id)
            .expect("selected Source in Adapter catalog")
            .availability = Availability::Offline;
        node.runtime
            .replace_adapter_catalog(snapshot.local_node.id, adapter_id, replacement)
            .expect("replace local Adapter catalog");

        update_local_capability_selection(&node.runtime, &mut selected, capability_id, false)
            .expect("remove stale selection");
        assert!(selected.is_empty());
    }

    #[test]
    fn windows_preflight_single_flight_is_bounded_and_released_by_drop() {
        let flag = AtomicBool::new(false);
        let first = AtomicSingleFlight::acquire(&flag, "busy").expect("first permit");
        assert!(AtomicSingleFlight::acquire(&flag, "busy").is_err());
        drop(first);
        assert!(AtomicSingleFlight::acquire(&flag, "busy").is_ok());
    }

    #[test]
    fn loopback_worker_drives_runtime_active_and_stopped_states() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("loopback listener");
        let address = listener.local_addr().expect("loopback address");
        let server = thread::spawn(move || {
            let (accelerometer_stream, _) = listener.accept().expect("accelerometer connection");
            let mut accelerometer = accept(accelerometer_stream).expect("accelerometer handshake");
            let (gyroscope_stream, _) = listener.accept().expect("gyroscope connection");
            let mut gyroscope = accept(gyroscope_stream).expect("gyroscope handshake");
            for index in 0..12_u64 {
                let timestamp = 1_000_000_000 + index * 10_000_000;
                if accelerometer
                    .send(Message::Text(
                        serde_json::json!({
                            "accuracy": 3,
                            "timestamp": timestamp,
                            "values": [0.1 + index as f64, 0.2, 9.8]
                        })
                        .to_string()
                        .into(),
                    ))
                    .is_err()
                {
                    break;
                }
                if gyroscope
                    .send(Message::Text(
                        serde_json::json!({
                            "accuracy": 3,
                            "timestamp": timestamp + 1_000,
                            "values": [0.01, 0.02 + index as f64, 0.03]
                        })
                        .to_string()
                        .into(),
                    ))
                    .is_err()
                {
                    break;
                }
                thread::sleep(Duration::from_millis(5));
            }
        });
        let endpoint = SensorServerEndpoint::new(address.ip(), address.port()).expect("endpoint");
        let (mut node, route) = physical_node();
        let epoch = route
            .begin_start(&mut node.runtime, 1)
            .expect("begin physical Route");
        let route_runtime = Arc::new(Mutex::new(node));
        let stop = Arc::new(AtomicBool::new(false));
        let shared = Arc::new(Mutex::new(
            UiLiveImu::idle(route, &route_runtime.lock().expect("Runtime lock").runtime)
                .expect("idle DTO"),
        ));
        let worker_stop = Arc::clone(&stop);
        let worker_shared = Arc::clone(&shared);
        let worker_runtime = Arc::clone(&route_runtime);
        let worker = thread::spawn(move || {
            run_live_imu(
                endpoint,
                epoch,
                &worker_stop,
                &worker_shared,
                &worker_runtime,
                route,
            )
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let received = shared.lock().expect("state lock").received_samples;
            if received >= 4 {
                break;
            }
            assert!(Instant::now() < deadline, "loopback worker timed out");
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            route.route_state(&route_runtime.lock().expect("Runtime lock").runtime),
            Ok(RouteState::Active)
        );
        stop.store(true, Ordering::Release);
        worker.join().expect("worker joins");
        route
            .stop(&mut route_runtime.lock().expect("Runtime lock").runtime)
            .expect("stop Route");
        assert_eq!(
            route.route_state(&route_runtime.lock().expect("Runtime lock").runtime),
            Ok(RouteState::Stopped)
        );
        server.join().expect("server joins");
    }

    #[test]
    fn loopback_connect_failure_offlines_route_and_retry_advances_epoch() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("loopback listener");
        let address = listener.local_addr().expect("loopback address");
        drop(listener);
        let endpoint = SensorServerEndpoint::new(address.ip(), address.port()).expect("endpoint");
        let (mut node, route) = physical_node();
        let first_epoch = route
            .begin_start(&mut node.runtime, 1)
            .expect("begin physical Route");
        let route_runtime = Arc::new(Mutex::new(node));
        let shared = Arc::new(Mutex::new(
            UiLiveImu::idle(route, &route_runtime.lock().expect("Runtime lock").runtime)
                .expect("idle DTO"),
        ));
        run_live_imu(
            endpoint,
            first_epoch,
            &Arc::new(AtomicBool::new(false)),
            &shared,
            &route_runtime,
            route,
        );
        let snapshot = shared.lock().expect("state lock").clone();
        assert_eq!(snapshot.status, "offline");
        assert_eq!(snapshot.route_state, "offline");
        assert_eq!(
            snapshot.problem_code.as_deref(),
            Some("CAPY.IMU.SENSORSERVER_DISCONNECTED")
        );
        let offline_epoch = route
            .route_epoch(&route_runtime.lock().expect("Runtime lock").runtime)
            .expect("offline epoch");
        assert!(offline_epoch > first_epoch);
        let retry_epoch = route
            .begin_start(&mut route_runtime.lock().expect("Runtime lock").runtime, 2)
            .expect("retry Route");
        assert!(retry_epoch > offline_epoch);
    }

    #[test]
    #[ignore = "requires explicitly configured physical SensorServer lab"]
    fn physical_live_imu_worker_updates_the_tauri_dto_and_stops_cleanly() {
        let ip = std::env::var("CAPYIO_LIVE_IMU_IP")
            .expect("CAPYIO_LIVE_IMU_IP must name the authorized physical lab")
            .parse::<IpAddr>()
            .expect("CAPYIO_LIVE_IMU_IP must be an IP literal");
        let port = std::env::var("CAPYIO_LIVE_IMU_PORT")
            .expect("CAPYIO_LIVE_IMU_PORT must name the authorized physical lab")
            .parse::<u16>()
            .expect("CAPYIO_LIVE_IMU_PORT must be a valid port");
        let endpoint = SensorServerEndpoint::new(ip, port).expect("valid physical endpoint");
        let (mut node, route) = physical_node();
        let epoch = route
            .begin_start(&mut node.runtime, 1)
            .expect("begin physical Route");
        let route_runtime = Arc::new(Mutex::new(node));
        let stop = Arc::new(AtomicBool::new(false));
        let shared = Arc::new(Mutex::new(
            UiLiveImu::idle(route, &route_runtime.lock().expect("Runtime lock").runtime)
                .expect("idle DTO"),
        ));
        shared.lock().expect("state lock").stream_epoch = epoch;
        let worker_stop = Arc::clone(&stop);
        let worker_shared = Arc::clone(&shared);
        let worker_runtime = Arc::clone(&route_runtime);
        let worker = thread::spawn(move || {
            run_live_imu(
                endpoint,
                epoch,
                &worker_stop,
                &worker_shared,
                &worker_runtime,
                route,
            )
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let snapshot = shared.lock().expect("state lock").clone();
            if snapshot.received_samples >= 4 {
                assert_eq!(snapshot.status, "active");
                assert!(snapshot.acceleration.is_some());
                assert!(snapshot.angular_velocity.is_some());
                assert!(snapshot.problem.is_none());
                break;
            }
            assert_ne!(snapshot.status, "offline", "{:?}", snapshot.problem);
            assert!(
                Instant::now() < deadline,
                "physical DTO did not receive four samples"
            );
            thread::sleep(Duration::from_millis(100));
        }
        stop.store(true, Ordering::Release);
        worker.join().expect("live worker joins");
        route
            .stop(&mut route_runtime.lock().expect("Runtime lock").runtime)
            .expect("stop physical Route");
        assert_eq!(
            route.route_state(&route_runtime.lock().expect("Runtime lock").runtime),
            Ok(RouteState::Stopped)
        );
    }
}
