use super::*;
use capyio_control_tls::{
    ControlTlsBinding,
    touchpad::{TouchpadTlsBinding, TouchpadTlsChannel, TouchpadTlsStopHandle},
};
use capyio_core::{
    AdapterHealth, AdapterInstanceDescriptor, AdapterInstanceId, AdapterState,
    CapabilityDescriptor, FormatDescriptor, InteroperabilityMode, PortDescriptor, PortId, Problem,
    ProblemCategory, ProblemId, ProblemSeverity, ProfileId, SessionState,
};
use capyio_protocol::TouchpadDataOffer;
use capyio_remote_touchpad_adapter::{
    PrivateTouchpadSink, PrivateTouchpadSinkFactory,
    secure_receiver::{SecureTouchpadReceiver, TouchpadSinkAdmission},
};
use capyio_runtime::PortLeasePolicy;
use capyio_windows_input::{VhfBrokerInterfaceProbe, probe_vhf_broker_interface};

const ADAPTER: &str = "d41251b3-63ed-4e83-807e-4c4ab9b20a01";
const CAPABILITY: &str = "d41251b3-63ed-4e83-807e-4c4ab9b20a02";
const PORT: &str = "d41251b3-63ed-4e83-807e-4c4ab9b20a03";

/// A separate production contract, not a relabeled testkit Sink. Publication
/// means an installed projection can be requested, not that opening succeeded.
pub(super) fn install_catalog(runtime: &mut NodeRuntime) -> Result<(), String> {
    let id = AdapterInstanceId::from_str(ADAPTER).map_err(|e| e.to_string())?;
    let capability_id = CapabilityId::from_str(CAPABILITY).map_err(|e| e.to_string())?;
    let port_id = PortId::from_str(PORT).map_err(|e| e.to_string())?;
    if runtime
        .snapshot()
        .local_node
        .adapter_instances
        .contains_key(&id)
    {
        return Ok(());
    }
    let availability = match probe_vhf_broker_interface() {
        VhfBrokerInterfaceProbe::Single | VhfBrokerInterfaceProbe::Absent => {
            Availability::Available
        }
        _ => Availability::Offline,
    };
    let port = PortDescriptor {
        id: port_id,
        capability_id,
        display_name: "Windows 触控输入".into(),
        direction: PortDirection::Sink,
        profile: ProfileId::touchpad_frames_v1(),
        schema_id: None,
        formats: vec![FormatDescriptor::new("touchpad-frame-v1")],
        qos_modes: BTreeSet::from([QosMode::Interactive]),
        clock_domain: None,
        availability,
        permission_requirement: PermissionRequirement::UserConfirmation,
        interoperability_mode: InteroperabilityMode::AdapterManaged,
    };
    let capability = CapabilityDescriptor {
        id: capability_id,
        adapter_instance_id: id,
        display_name: "Windows 触控板接收器".into(),
        class: CapabilityClass::Touchpad,
        availability,
        permission_requirement: PermissionRequirement::UserConfirmation,
        metadata: BTreeMap::from([
            ("transport".into(), "capyio-touchpad/1 mTLS".into()),
            (
                "evidence".into(),
                "native_projection_requires_runtime_open".into(),
            ),
        ]),
        ports: BTreeMap::from([(port_id, port)]),
    };
    let adapter = AdapterInstanceDescriptor {
        id,
        adapter_type: "dev.capyio.windows.touchpad-tls".into(),
        display_name: "Windows native touchpad receiver".into(),
        deployment_mode: AdapterDeploymentMode::InProcess,
        version: env!("CARGO_PKG_VERSION").into(),
        state: AdapterState::Ready,
        health: AdapterHealth::Unknown,
        owned_capabilities: BTreeSet::new(),
        supported_route_modes: BTreeSet::from([RouteBackend::AdapterManaged]),
    };
    runtime
        .register_adapter_catalog(runtime.snapshot().local_node.id, adapter, vec![capability])
        .map_err(|e| e.to_string())
}

pub(super) fn preflight_projection() -> Result<(), String> {
    match probe_vhf_broker_interface() {
        VhfBrokerInterfaceProbe::Single => {
            #[cfg(windows)]
            {
                validate_vhf_session(
                    windows_remote_session(),
                    crate::camera_session::process_is_elevated()?,
                )?;
            }
            Ok(())
        }
        VhfBrokerInterfaceProbe::Absent => Ok(()),
        _ => Err("Windows 触控板投影检测失败或存在多个设备；未启动输入。".into()),
    }
}

fn validate_vhf_session(remote_session: bool, elevated: bool) -> Result<(), String> {
    if remote_session {
        return Err("当前 CapyIO 运行在 Windows 远程桌面（RDP）会话中。VHF 会把触控报告交给本机 Precision Touchpad 栈，而 RDP 可见指针使用独立的远程输入通道，因此不能在此会话中验收原生触控。请在 Windows 本机控制台会话运行 CapyIO 后重试。".into());
    }
    if !elevated {
        return Err("已检测到 VHF 触控板，但当前 Windows 进程没有权限。请以管理员身份运行 CapyIO 后重试；不会自动安装驱动或更改权限。".into());
    }
    Ok(())
}

#[cfg(windows)]
fn windows_remote_session() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_REMOTESESSION};
    // Read-only process/session query; no window or device state is changed.
    unsafe { GetSystemMetrics(SM_REMOTESESSION) != 0 }
}

pub(super) struct TouchpadDataWorker {
    stop: TouchpadTlsStopHandle,
    thread: Option<JoinHandle<Result<(), String>>>,
}

pub(super) fn report_lease_failure(
    node: &Arc<Mutex<DesktopNodeState>>,
    route: RouteId,
    source: PortRef,
    error: String,
) {
    if let Ok(mut node) = node.lock() {
        let _ = node.runtime.report_route_offline(
            route,
            Problem {
                id: ProblemId::new(),
                code: "CAPY.TOUCHPAD.LEASE_STOPPED".into(),
                category: ProblemCategory::Transport,
                severity: ProblemSeverity::Warning,
                retryable: true,
                related_node: Some(source.node_id),
                related_adapter: None,
                related_route: Some(route),
                human_message: "触控板占用已结束，输入已清理；可重新申请".into(),
                technical_detail: Some(error.chars().take(1000).collect()),
            },
        );
    }
}
impl TouchpadDataWorker {
    pub(super) fn is_finished(&self) -> bool {
        self.thread
            .as_ref()
            .is_none_or(|thread| thread.is_finished())
    }
}
impl Drop for TouchpadDataWorker {
    fn drop(&mut self) {
        self.stop.stop();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn admission(
    node: &Arc<Mutex<DesktopNodeState>>,
    binding: TouchpadTlsBinding,
) -> Result<TouchpadSinkAdmission, String> {
    let mut node = node.lock().map_err(|_| "Node state lock poisoned")?;
    node.runtime.expire_port_leases(unix_now_ms()?);
    let route = node
        .runtime
        .route(binding.route_id)
        .map_err(|e| e.to_string())?
        .clone();
    let local_lease_expires_at_ms = node
        .runtime
        .route_port_lease_claim(route.id, route.sink)
        .ok_or("Windows touchpad lease is absent")?
        .expires_at_ms;
    let session_ready = node
        .runtime
        .session(route.session_id)
        .map_err(|e| e.to_string())?
        .state
        == SessionState::Ready;
    Ok(TouchpadSinkAdmission {
        route,
        local_lease_expires_at_ms,
        session_ready,
    })
}

fn sink_lease_expiry(authorization_expiry: u64, now: u64) -> Result<u64, String> {
    super::touchpad_lease::sink_expiry(authorization_expiry, now)
}

pub(super) fn start(
    node: Arc<Mutex<DesktopNodeState>>,
    dial: &ResolvedControlDial,
    session: ControlTlsBinding,
    remote: Route,
    offer: TouchpadDataOffer,
) -> Result<TouchpadDataWorker, String> {
    preflight_projection()?;
    if remote.source.node_id != dial.candidate.node_id
        || remote.sink.capability_id.to_string() != CAPABILITY
    {
        return Err(
            "touchpad offer does not name the native receiver and authenticated peer".into(),
        );
    }
    let now = unix_now_ms()?;
    let binding = TouchpadTlsBinding::from_route(session, &remote, offer.stream_id, now)
        .map_err(|e| e.to_string())?;
    {
        let mut node = node.lock().map_err(|_| "Node state lock poisoned")?;
        node.runtime
            .set_local_port_lease_policy(remote.sink, PortLeasePolicy::Exclusive)
            .map_err(|e| e.to_string())?;
        node.runtime
            .acquire_route_port_lease(
                remote.id,
                remote.sink,
                sink_lease_expiry(binding.expires_at_ms, now)?,
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
                    .ok_or("touchpad QoS missing")?,
                now,
            )
            .map_err(|e| e.to_string())?;
        node.runtime
            .begin_route_start(remote.id, now)
            .map_err(|e| e.to_string())?;
        if node
            .runtime
            .route(remote.id)
            .map_err(|e| e.to_string())?
            .epoch
            != remote.epoch
        {
            return Err("touchpad Route epochs differ".into());
        }
    }
    let credential = dial
        .store
        .load_control_credential(&dial.candidate.credential_handle)
        .map_err(|e| e.to_string())?;
    let identity =
        ControlTlsClientIdentity::from_bundle(dial.candidate.peer_certificate_sha256, credential);
    let channel = TouchpadTlsChannel::connect(
        ControlTlsEndpoint {
            // The remote offer cannot nominate a different host or relay.
            address: SocketAddr::new(dial.candidate.endpoint.ip(), offer.port),
            io_timeout: Duration::from_secs(2),
        },
        &dial.candidate.server_name,
        identity,
        binding,
    )
    .map_err(|e| e.to_string())?;
    let stop = channel.stop_handle().map_err(|e| e.to_string())?;
    let (ready, receiver) = mpsc::sync_channel(1);
    let thread = thread::Builder::new()
        .name("capyio-touchpad-tls".into())
        .spawn(move || {
            let result = run_projection(channel, &node, binding, &ready);
            if let Err(error) = &result {
                let _ = ready.try_send(Err(error.clone()));
                if let Ok(mut node) = node.lock()
                    && node.runtime.route(binding.route_id).is_ok_and(|route| {
                        matches!(route.state, RouteState::Active | RouteState::Starting)
                    })
                {
                    let _ = node.runtime.report_route_offline(
                        binding.route_id,
                        Problem {
                            id: ProblemId::new(),
                            code: "CAPY.TOUCHPAD.DATA_STOPPED".into(),
                            category: ProblemCategory::Transport,
                            severity: ProblemSeverity::Error,
                            retryable: true,
                            related_node: Some(binding.source.node_id),
                            related_adapter: None,
                            related_route: Some(binding.route_id),
                            human_message: "触控板数据连接已停止，输入已清理".into(),
                            technical_detail: Some(error.chars().take(1000).collect()),
                        },
                    );
                }
            } else if let Ok(mut node) = node.lock() {
                let _ = node.runtime.set_route_active(
                    binding.route_id,
                    false,
                    unix_now_ms().unwrap_or(u64::MAX),
                );
            }
            result
        })
        .map_err(|e| e.to_string())?;
    let worker = TouchpadDataWorker {
        stop,
        thread: Some(thread),
    };
    match receiver
        .recv_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?
    {
        Ok(()) => Ok(worker),
        Err(error) => Err(error),
    }
}

#[cfg(windows)]
fn run_projection(
    channel: TouchpadTlsChannel,
    node: &Arc<Mutex<DesktopNodeState>>,
    binding: TouchpadTlsBinding,
    ready: &SyncSender<Result<(), String>>,
) -> Result<(), String> {
    use capyio_remote_touchpad_adapter::{
        WindowsSyntheticTouchpadSinkFactory, WindowsVhfPrecisionTouchpadSinkFactory,
    };
    match probe_vhf_broker_interface() {
        VhfBrokerInterfaceProbe::Single => run_receiver(
            channel,
            node,
            binding,
            WindowsVhfPrecisionTouchpadSinkFactory,
            ready,
        ),
        VhfBrokerInterfaceProbe::Absent => run_receiver(
            channel,
            node,
            binding,
            WindowsSyntheticTouchpadSinkFactory,
            ready,
        ),
        _ => Err("Windows touchpad projection changed before opening".into()),
    }
}
#[cfg(not(windows))]
fn run_projection(
    _: TouchpadTlsChannel,
    _: &Arc<Mutex<DesktopNodeState>>,
    _: TouchpadTlsBinding,
    _: &SyncSender<Result<(), String>>,
) -> Result<(), String> {
    Err("Windows touchpad unavailable on this platform".into())
}

fn run_receiver<F: PrivateTouchpadSinkFactory>(
    channel: TouchpadTlsChannel,
    node: &Arc<Mutex<DesktopNodeState>>,
    binding: TouchpadTlsBinding,
    factory: F,
    ready: &SyncSender<Result<(), String>>,
) -> Result<(), String>
where
    F::Error: std::fmt::Display,
    <F::Sink as PrivateTouchpadSink>::Error: std::fmt::Display,
{
    let mut receiver =
        SecureTouchpadReceiver::accept(channel, factory, || admission(node, binding))?;
    if !receiver.receive_next(|| admission(node, binding))? {
        return Err("touchpad closed before initial frame".into());
    }
    node.lock()
        .map_err(|_| "Node state lock poisoned")?
        .runtime
        .activate_route(binding.route_id)
        .map_err(|e| e.to_string())?;
    let _ = ready.send(Ok(()));
    while receiver.receive_next(|| admission(node, binding))? {}
    receiver.close()
}

#[cfg(test)]
mod tests {
    use super::{sink_lease_expiry, validate_vhf_session};

    #[test]
    fn local_lease_uses_absolute_expiry_not_a_wire_ttl() {
        let now = 1_800_000_000_000;
        assert_eq!(
            sink_lease_expiry(now + 300_000, now).unwrap(),
            now + 299_000
        );
        assert!(sink_lease_expiry(now + 1_000, now).is_err());
        assert!(sink_lease_expiry(now - 1, now).is_err());
        assert!(sink_lease_expiry(0, now).is_err());
    }

    #[test]
    fn vhf_preflight_rejects_rdp_before_elevation_and_accepts_local_admin() {
        assert!(validate_vhf_session(false, true).is_ok());
        assert!(
            validate_vhf_session(false, false)
                .unwrap_err()
                .contains("管理员")
        );
        let remote = validate_vhf_session(true, true).unwrap_err();
        assert!(remote.contains("RDP"));
        assert!(remote.contains("本机控制台"));
    }
}
