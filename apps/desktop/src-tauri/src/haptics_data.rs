//! Reverse actuator Route: projection feedback Source -> paired Android Sink.
use super::*;
use capyio_control_tls::{
    ControlTlsBinding,
    haptics::{HapticsRecord, HapticsTlsBinding, HapticsTlsChannel, HapticsTlsStopHandle},
};
use capyio_core::{
    AdapterHealth, AdapterInstanceDescriptor, AdapterInstanceId, AdapterState,
    CapabilityDescriptor, FormatDescriptor, InteroperabilityMode, PortDescriptor, PortId, Problem,
    ProblemCategory, ProblemId, ProblemSeverity, ProfileId, SessionState,
};
use capyio_runtime::PortLeasePolicy;
pub(super) const CAPABILITY: &str = "d41251b3-63ed-4e83-807e-4c4ab9b20b02";
const ADAPTER: &str = "d41251b3-63ed-4e83-807e-4c4ab9b20b01";
const PORT: &str = "d41251b3-63ed-4e83-807e-4c4ab9b20b03";

pub(super) fn install_catalog(runtime: &mut NodeRuntime) -> Result<(), String> {
    let id = AdapterInstanceId::from_str(ADAPTER).map_err(|e| e.to_string())?;
    if runtime
        .snapshot()
        .local_node
        .adapter_instances
        .contains_key(&id)
    {
        return Ok(());
    }
    let capability_id = CapabilityId::from_str(CAPABILITY).map_err(|e| e.to_string())?;
    let port_id = PortId::from_str(PORT).map_err(|e| e.to_string())?;
    let port = PortDescriptor {
        id: port_id,
        capability_id,
        display_name: "游戏振动输出".into(),
        direction: PortDirection::Source,
        profile: ProfileId::haptics_feedback_v1(),
        schema_id: None,
        formats: vec![FormatDescriptor::new("dual-rumble-v1")],
        qos_modes: BTreeSet::from([QosMode::Interactive]),
        clock_domain: None,
        availability: Availability::Available,
        permission_requirement: PermissionRequirement::None,
        interoperability_mode: InteroperabilityMode::AdapterManaged,
    };
    runtime
        .register_adapter_catalog(
            runtime.snapshot().local_node.id,
            AdapterInstanceDescriptor {
                id,
                adapter_type: "dev.capyio.windows.gamepad-feedback".into(),
                display_name: "Windows game feedback".into(),
                deployment_mode: AdapterDeploymentMode::InProcess,
                version: env!("CARGO_PKG_VERSION").into(),
                state: AdapterState::Ready,
                health: AdapterHealth::Unknown,
                owned_capabilities: BTreeSet::new(),
                supported_route_modes: BTreeSet::from([RouteBackend::AdapterManaged]),
            },
            vec![CapabilityDescriptor {
                id: capability_id,
                adapter_instance_id: id,
                display_name: "手柄游戏振动反馈".into(),
                class: CapabilityClass::Haptics,
                availability: Availability::Available,
                permission_requirement: PermissionRequirement::None,
                metadata: BTreeMap::from([("transport".into(), "capyio-haptics/1 mTLS".into())]),
                ports: BTreeMap::from([(port_id, port)]),
            }],
        )
        .map_err(|e| e.to_string())
}

pub(super) struct HapticsDataWorker {
    stop: HapticsTlsStopHandle,
    thread: Option<JoinHandle<Result<(), String>>>,
}
impl HapticsDataWorker {
    pub(super) fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(|t| t.is_finished())
    }
}
impl Drop for HapticsDataWorker {
    fn drop(&mut self) {
        self.stop.stop();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn validate(node: &Arc<Mutex<DesktopNodeState>>, binding: HapticsTlsBinding) -> Result<(), String> {
    let mut node = node.lock().map_err(|_| "Node state lock poisoned")?;
    let now = unix_now_ms()?;
    node.runtime.expire_port_leases(now);
    let route = node
        .runtime
        .route(binding.route_id)
        .map_err(|e| e.to_string())?;
    if node
        .runtime
        .session(route.session_id)
        .map_err(|e| e.to_string())?
        .state
        != SessionState::Ready
        || node
            .runtime
            .route_port_lease_claim(route.id, route.source)
            .is_none_or(|claim| claim.expires_at_ms <= now)
        || HapticsTlsBinding::from_route(binding.session, route, binding.stream_id, now)
            .map_err(|e| e.to_string())?
            != binding
    {
        return Err("振动 Route 或本机占用已结束".into());
    }
    Ok(())
}
fn exchange(channel: &mut HapticsTlsChannel, record: HapticsRecord) -> Result<(), String> {
    channel
        .send_record(&record.encode().map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let reply = HapticsRecord::decode(&channel.receive_record().map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if reply
        != (HapticsRecord::Ack {
            sequence: record.sequence(),
        })
    {
        return Err("振动执行确认不匹配".into());
    }
    Ok(())
}
pub(super) fn start(
    node: Arc<Mutex<DesktopNodeState>>,
    lab: Arc<Mutex<GamepadLab>>,
    dial: &ResolvedControlDial,
    session: ControlTlsBinding,
    remote: Route,
    offer: capyio_protocol::HapticsDataOffer,
) -> Result<HapticsDataWorker, String> {
    if remote.sink.node_id != dial.candidate.node_id
        || remote.source.capability_id.to_string() != CAPABILITY
    {
        return Err("振动数据端点与已配对设备不匹配".into());
    }
    if lab
        .lock()
        .map_err(|_| "gamepad lock poisoned")?
        .feedback_motors()
        .is_none()
    {
        return Err("请先启动 DS4 或 Xbox 手柄".into());
    }
    let now = unix_now_ms()?;
    let binding = HapticsTlsBinding::from_route(session, &remote, offer.stream_id, now)
        .map_err(|e| e.to_string())?;
    {
        let mut node = node.lock().map_err(|_| "Node state lock poisoned")?;
        node.runtime
            .set_local_port_lease_policy(remote.source, PortLeasePolicy::Exclusive)
            .map_err(|e| e.to_string())?;
        node.runtime
            .acquire_route_port_lease(
                remote.id,
                remote.source,
                touchpad_lease::sink_expiry(binding.expires_at_ms, now)?,
                now,
            )
            .map_err(|e| e.to_string())?;
        node.runtime
            .prepare_route(
                remote.id,
                remote.selected_format.clone(),
                remote
                    .selected_qos_mode
                    .clone()
                    .ok_or("haptics QoS missing")?,
                now,
            )
            .map_err(|e| e.to_string())?;
        node.runtime
            .begin_route_start(remote.id, now)
            .map_err(|e| e.to_string())?;
    }
    validate(&node, binding)?;
    let credential = dial
        .store
        .load_control_credential(&dial.candidate.credential_handle)
        .map_err(|e| e.to_string())?;
    let mut channel = HapticsTlsChannel::connect(
        ControlTlsEndpoint {
            address: SocketAddr::new(dial.candidate.endpoint.ip(), offer.port),
            io_timeout: Duration::from_secs(2),
        },
        &dial.candidate.server_name,
        ControlTlsClientIdentity::from_bundle(dial.candidate.peer_certificate_sha256, credential),
        binding,
    )
    .map_err(|e| e.to_string())?;
    exchange(&mut channel, HapticsRecord::Stop { sequence: 0 })?;
    validate(&node, binding)?;
    node.lock()
        .map_err(|_| "Node state lock poisoned")?
        .runtime
        .activate_route(remote.id)
        .map_err(|e| e.to_string())?;
    let stop = channel.stop_handle().map_err(|e| e.to_string())?;
    let thread = thread::Builder::new()
        .name("capyio-haptics-tls".into())
        .spawn(move || {
            let result = (|| {
                for sequence in 1..u64::MAX {
                    validate(&node, binding)?;
                    let motors = lab
                        .lock()
                        .map_err(|_| "gamepad lock poisoned")?
                        .feedback_motors();
                    let record = match motors {
                        Some([small, large]) if small != 0 || large != 0 => HapticsRecord::Rumble {
                            sequence,
                            low_frequency: u16::from(large) * 257,
                            high_frequency: u16::from(small) * 257,
                            duration_ms: 160,
                        },
                        _ => HapticsRecord::Stop { sequence },
                    };
                    exchange(&mut channel, record)?;
                    if motors.is_none() {
                        return Ok(());
                    }
                    thread::sleep(Duration::from_millis(80));
                }
                Err("haptics sequence exhausted".to_owned())
            })();
            if let Ok(mut node) = node.lock() {
                let live = node
                    .runtime
                    .route(binding.route_id)
                    .is_ok_and(|r| matches!(r.state, RouteState::Active | RouteState::Starting));
                if live {
                    if let Err(error) = &result {
                        let _ = node.runtime.report_route_offline(
                            binding.route_id,
                            Problem {
                                id: ProblemId::new(),
                                code: "CAPY.HAPTICS.DATA_STOPPED".into(),
                                category: ProblemCategory::Transport,
                                severity: ProblemSeverity::Warning,
                                retryable: true,
                                related_node: Some(binding.sink.node_id),
                                related_adapter: None,
                                related_route: Some(binding.route_id),
                                human_message: "游戏振动连接已停止".into(),
                                technical_detail: Some(error.clone()),
                            },
                        );
                    } else {
                        let _ = node.runtime.set_route_active(
                            binding.route_id,
                            false,
                            unix_now_ms().unwrap_or(u64::MAX),
                        );
                    }
                }
            }
            result
        })
        .map_err(|e| e.to_string())?;
    Ok(HapticsDataWorker {
        stop,
        thread: Some(thread),
    })
}
