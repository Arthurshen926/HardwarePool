use std::{
    collections::HashSet,
    fs,
    net::{Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use capyio_core::{
    RouteId, SessionId, StreamId,
    ids::{AdapterInstanceId, CapabilityId},
};
use capyio_display::{DisplayCursorMode, DisplayStreamSpec};
use capyio_display_engine_host::{
    DisplayWorkerEngineConfigRecord, DisplayWorkerLease, DisplayWorkerPipeName,
    try_exchange_authenticated_engine_launch,
};
use capyio_display_stream_adapter::{
    DisplayMode, DisplayOnlyPolicy, DisplayStreamSessionRequest, DynamicRange,
    ValidatedDisplayStreamSession, VideoCodec,
    launch_record::{GameStreamLaunchBinding, GameStreamLaunchSecrets},
};
use capyio_display_tls::{
    AuthenticatedDisplaySender, AuthenticatedDisplayServer, AuthenticatedGameStreamLaunchServer,
    DisplayTlsError, DisplayTlsServerEndpoint, DisplayTlsServerIdentity,
};
use capyio_display_transport::{
    AvcLevel, AvcProfile, DisplayTransportBinding, EncodedDisplayCodec, EncodedDisplayStreamSpec,
};
use capyio_video::{FrameRate, VideoColorimetry, VideoPixelFormat};
use capyio_windows_display::{
    IDD_CONTROL_MAX_MONITORS, IddFrameBrokerMapping, IddMonitorCoordinator, IddMonitorIdentity,
    InteractiveDisplaySessionId, MediaFoundationDisplayPipeline, MediaFoundationH264EncoderConfig,
    MfDisplayPipelineError, inspect_capyio_display_path,
};
use serde::{Deserialize, Serialize};
use windows_sys::Win32::Security::Cryptography::{
    BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom,
};

use crate::{
    BoundDisplayWorkerChannel, DisplayRouteProjectionRuntime, DisplayRouteProjectionSnapshot,
    VerifiedDisplayWorkerExecutable, display_session::projection_spec,
};

const MAX_DISPLAY_CONFIG_BYTES: u64 = 16 * 1024;
const MAX_CREDENTIAL_BYTES: u64 = 64 * 1024;
const MAX_AUTHENTICATION_FAILURES: u8 = 8;
const ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(25);
const FRAME_POLL_INTERVAL: Duration = Duration::from_millis(2);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(2);
const MIN_HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(1);
const TOPOLOGY_POLL_INTERVAL: Duration = Duration::from_secs(1);
// A graceful engine-worker exit includes engine shutdown, asynchronous SudoVDA
// target departure, and a stable Windows topology-restore window. Keep this
// longer than the display lease's bounded teardown instead of treating its
// restoration work as a hung child process.
const ENGINE_WORKER_GRACEFUL_EXIT_TIMEOUT: Duration = Duration::from_secs(45);

const PATH_INACTIVE: &str = "CAPY.DISPLAY.PATH_INACTIVE";
const TOPOLOGY_UNAVAILABLE: &str = "CAPY.DISPLAY.TOPOLOGY_UNAVAILABLE";
const HEARTBEAT_TIMEOUT: &str = "CAPY.DISPLAY.HEARTBEAT_TIMEOUT";
const TRANSPORT_LOST: &str = "CAPY.DISPLAY.TRANSPORT_LOST";
const HEARTBEAT_PROTOCOL_VIOLATION: &str = "CAPY.DISPLAY.HEARTBEAT_PROTOCOL_VIOLATION";
const ENCODER_FAILED: &str = "CAPY.DISPLAY.ENCODER_FAILED";
const MODE_MISMATCH: &str = "CAPY.DISPLAY.MODE_MISMATCH";
const AUTHENTICATION_FAILURE_LIMIT: &str = "CAPY.DISPLAY.AUTHENTICATION_FAILURE_LIMIT";
const GENERATION_EXHAUSTED: &str = "CAPY.DISPLAY.GENERATION_EXHAUSTED";
const ENGINE_WORKER_FAILED: &str = "CAPY.DISPLAY.ENGINE_WORKER_FAILED";
const HOST_FAILED: &str = "CAPY.DISPLAY.HOST_FAILED";
const CLEANUP_FAILED: &str = "CAPY.DISPLAY.CLEANUP_FAILED";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SingleDisplayState {
    WaitingForDisplayPath,
    WaitingForReceiver,
    AuthenticatedWaitingForDisplayPath,
    AuthenticatedWaitingForSelectedMode,
    Authenticated,
    Streaming,
    Stopped,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SingleDisplaySnapshot {
    pub schema_version: u8,
    pub state: SingleDisplayState,
    pub generation: u64,
    pub receiver_connected: bool,
    pub path_active: Option<bool>,
    pub mode_matches: Option<bool>,
    pub frames_sent: u64,
    pub heartbeat_acknowledgements: u64,
    pub problem_code: Option<String>,
}

#[derive(Clone)]
pub struct SingleDisplayStatusHandle {
    inner: Arc<Mutex<SingleDisplaySnapshot>>,
}

impl SingleDisplayStatusHandle {
    #[must_use]
    pub fn snapshot(&self) -> SingleDisplaySnapshot {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

struct SingleDisplayLifecycle {
    snapshot: SingleDisplaySnapshot,
}

impl SingleDisplayLifecycle {
    fn new() -> Self {
        Self {
            snapshot: SingleDisplaySnapshot {
                schema_version: 1,
                state: SingleDisplayState::WaitingForDisplayPath,
                generation: 0,
                receiver_connected: false,
                path_active: None,
                mode_matches: None,
                frames_sent: 0,
                heartbeat_acknowledgements: 0,
                problem_code: Some(TOPOLOGY_UNAVAILABLE.to_owned()),
            },
        }
    }

    fn observe_path(&mut self, active: Result<bool, ()>) {
        match active {
            Ok(true) => {
                self.snapshot.path_active = Some(true);
                if self.snapshot.receiver_connected {
                    self.snapshot.state = if self.snapshot.mode_matches == Some(false) {
                        SingleDisplayState::AuthenticatedWaitingForSelectedMode
                    } else if self.snapshot.frames_sent == 0 {
                        SingleDisplayState::Authenticated
                    } else {
                        SingleDisplayState::Streaming
                    };
                    if self.snapshot.mode_matches != Some(false) {
                        self.snapshot.problem_code = None;
                    }
                } else {
                    self.snapshot.state = SingleDisplayState::WaitingForReceiver;
                    if matches!(
                        self.snapshot.problem_code.as_deref(),
                        Some(PATH_INACTIVE | TOPOLOGY_UNAVAILABLE)
                    ) {
                        self.snapshot.problem_code = None;
                    }
                }
            }
            Ok(false) => {
                self.snapshot.path_active = Some(false);
                self.snapshot.state = if self.snapshot.receiver_connected {
                    SingleDisplayState::AuthenticatedWaitingForDisplayPath
                } else {
                    SingleDisplayState::WaitingForDisplayPath
                };
                self.snapshot.problem_code = Some(PATH_INACTIVE.to_owned());
            }
            Err(()) => {
                self.snapshot.path_active = None;
                self.snapshot.state = if self.snapshot.receiver_connected {
                    SingleDisplayState::AuthenticatedWaitingForDisplayPath
                } else {
                    SingleDisplayState::WaitingForDisplayPath
                };
                self.snapshot.problem_code = Some(TOPOLOGY_UNAVAILABLE.to_owned());
            }
        }
    }

    fn receiver_authenticated(&mut self) -> Result<(), ()> {
        let Some(generation) = self.snapshot.generation.checked_add(1) else {
            self.fail(GENERATION_EXHAUSTED);
            return Err(());
        };
        self.snapshot.generation = generation;
        self.snapshot.receiver_connected = true;
        self.snapshot.frames_sent = 0;
        self.snapshot.heartbeat_acknowledgements = 0;
        self.snapshot.mode_matches = None;
        self.snapshot.problem_code = None;
        self.snapshot.state = if self.snapshot.path_active == Some(true) {
            SingleDisplayState::Authenticated
        } else {
            SingleDisplayState::AuthenticatedWaitingForDisplayPath
        };
        Ok(())
    }

    fn heartbeat_acknowledged(&mut self) {
        self.snapshot.heartbeat_acknowledgements =
            self.snapshot.heartbeat_acknowledgements.saturating_add(1);
    }

    fn frame_sent(&mut self) {
        self.snapshot.frames_sent = self.snapshot.frames_sent.saturating_add(1);
        self.snapshot.path_active = Some(true);
        self.snapshot.mode_matches = Some(true);
        self.snapshot.state = SingleDisplayState::Streaming;
        self.snapshot.problem_code = None;
    }

    fn frame_mode_mismatch(&mut self) {
        self.snapshot.mode_matches = Some(false);
        self.snapshot.state = SingleDisplayState::AuthenticatedWaitingForSelectedMode;
        self.snapshot.problem_code = Some(MODE_MISMATCH.to_owned());
    }

    fn transport_lost(&mut self, problem_code: &'static str) {
        self.snapshot.receiver_connected = false;
        self.snapshot.state = if self.snapshot.path_active == Some(true) {
            SingleDisplayState::WaitingForReceiver
        } else {
            SingleDisplayState::WaitingForDisplayPath
        };
        self.snapshot.problem_code = Some(problem_code.to_owned());
    }

    fn fail(&mut self, problem_code: &'static str) {
        self.snapshot.receiver_connected = false;
        self.snapshot.state = SingleDisplayState::Failed;
        self.snapshot.problem_code = Some(problem_code.to_owned());
    }

    fn stop(&mut self) {
        self.snapshot.receiver_connected = false;
        self.snapshot.state = SingleDisplayState::Stopped;
        self.snapshot.problem_code = None;
    }

    fn finish(&mut self, run_failed: bool, cleanup_failed: bool) {
        if cleanup_failed {
            self.fail(CLEANUP_FAILED);
        } else if run_failed {
            self.snapshot.receiver_connected = false;
            if self.snapshot.state != SingleDisplayState::Failed {
                self.fail(HOST_FAILED);
            }
        } else {
            self.stop();
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct DisplayServiceFile {
    schema_version: u8,
    routes: Vec<DisplayRouteFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct SingleDisplayServiceFile {
    schema_version: u8,
    route: DisplayRouteFile,
    engine_launch: EngineLaunchEndpointFile,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct EngineLaunchEndpointFile {
    address: String,
    port: u16,
    io_timeout_millis: u64,
    adapter_instance_id: String,
    engine_library_path: String,
    engine_sha256: String,
    worker_executable_path: String,
    worker_sha256: String,
    authorized_windows_session_id: u32,
    port_base: u16,
    codec: String,
    dynamic_range: String,
    bitrate_kbps: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct DisplayRouteFile {
    address: String,
    port: u16,
    io_timeout_millis: u64,
    server_certificate_path: String,
    server_private_key_path: String,
    client_ca_certificate_path: String,
    session_id: String,
    route_id: String,
    stream_id: String,
    stream_epoch: u64,
    sink_capability_id: String,
    width: u32,
    height: u32,
    frame_rate_numerator: u32,
    frame_rate_denominator: u32,
    maximum_access_unit_bytes: u32,
}

pub struct LoadedDisplayServiceConfig {
    routes: Vec<LoadedDisplayRouteConfig>,
}

pub struct LoadedSingleDisplayServiceConfig {
    route: LoadedDisplayRouteConfig,
    launch_endpoint: DisplayTlsServerEndpoint,
    launch_identity: DisplayTlsServerIdentity,
    engine: LoadedDisplayEngineConfig,
    worker: LoadedDisplayWorkerConfig,
}

struct LoadedDisplayEngineConfig {
    library_path: PathBuf,
    expected_sha256: [u8; 32],
    session: ValidatedDisplayStreamSession,
}

struct LoadedDisplayWorkerConfig {
    executable_path: PathBuf,
    expected_sha256: [u8; 32],
    authorized_session: InteractiveDisplaySessionId,
}

struct LoadedDisplayRouteConfig {
    endpoint: DisplayTlsServerEndpoint,
    identity: DisplayTlsServerIdentity,
    binding: DisplayTransportBinding,
    sink_identity: IddMonitorIdentity,
}

struct PreparedDisplayRouteConfig {
    endpoint: DisplayTlsServerEndpoint,
    binding: DisplayTransportBinding,
    sink_identity: IddMonitorIdentity,
    server_certificate_path: String,
    server_private_key_path: String,
    client_ca_certificate_path: String,
}

pub fn load_display_service_config(path: &Path) -> Result<LoadedDisplayServiceConfig, String> {
    let metadata =
        fs::metadata(path).map_err(|_| "read display service configuration".to_owned())?;
    if metadata.len() == 0 || metadata.len() > MAX_DISPLAY_CONFIG_BYTES {
        return Err("display service configuration is outside its size bound".to_owned());
    }
    let bytes = fs::read(path).map_err(|_| "read display service configuration".to_owned())?;
    parse_display_service_config(&bytes, read_credential)
}

pub fn load_single_display_service_config(
    path: &Path,
) -> Result<LoadedSingleDisplayServiceConfig, String> {
    let metadata =
        fs::metadata(path).map_err(|_| "read single-display service configuration".to_owned())?;
    if metadata.len() == 0 || metadata.len() > MAX_DISPLAY_CONFIG_BYTES {
        return Err("single-display service configuration is outside its size bound".to_owned());
    }
    let bytes =
        fs::read(path).map_err(|_| "read single-display service configuration".to_owned())?;
    parse_single_display_service_config(&bytes, read_credential)
}

/// Derives the Android receiver's public Route/engine expectation file from the
/// same closed schema-3 configuration consumed by the Windows service. It does
/// not read or serialize any credential bytes.
pub fn android_gamestream_properties_from_single_display_config(
    path: &Path,
) -> Result<String, String> {
    let metadata =
        fs::metadata(path).map_err(|_| "read single-display service configuration".to_owned())?;
    if metadata.len() == 0 || metadata.len() > MAX_DISPLAY_CONFIG_BYTES {
        return Err("single-display service configuration is outside its size bound".to_owned());
    }
    let bytes =
        fs::read(path).map_err(|_| "read single-display service configuration".to_owned())?;
    android_gamestream_properties_from_single_display_config_bytes(&bytes)
}

fn android_gamestream_properties_from_single_display_config_bytes(
    bytes: &[u8],
) -> Result<String, String> {
    // Parsing still validates every Route and engine field. The placeholder
    // prevents this metadata-only operation from loading the server private key.
    let loaded = parse_single_display_service_config(bytes, |_| Ok(vec![1]))?;
    let request = loaded.engine.session.request();
    let binding = &loaded.route.binding;
    let codec = match request.codec {
        VideoCodec::H264 => 1,
        VideoCodec::Hevc => 2,
        VideoCodec::Av1 => 3,
    };
    let dynamic_range = match request.dynamic_range {
        DynamicRange::Sdr8 => 1,
        DynamicRange::Hdr10 => 2,
    };
    Ok(format!(
        concat!(
            "host={address}\n",
            "host_ipv4={address}\n",
            "launch_port={launch_port}\n",
            "timeout_millis={timeout_millis}\n",
            "adapter_instance_id={adapter_instance_id}\n",
            "display_capability_id={display_capability_id}\n",
            "session_id={session_id}\n",
            "route_id={route_id}\n",
            "stream_id={stream_id}\n",
            "stream_epoch={stream_epoch}\n",
            "port_base={port_base}\n",
            "width={width}\n",
            "height={height}\n",
            "refresh_millihertz={refresh_millihertz}\n",
            "codec={codec}\n",
            "dynamic_range={dynamic_range}\n",
            "bitrate_kbps={bitrate_kbps}\n"
        ),
        address = loaded.launch_endpoint.address,
        launch_port = loaded.launch_endpoint.port,
        timeout_millis = loaded.launch_endpoint.io_timeout.as_millis(),
        adapter_instance_id = format_uuid_bytes(request.adapter_instance_id),
        display_capability_id = loaded.route.sink_identity.capability_id(),
        session_id = binding.session_id(),
        route_id = binding.route_id(),
        stream_id = binding.stream_id(),
        stream_epoch = binding.stream_epoch(),
        port_base = request.port_base,
        width = request.mode.width,
        height = request.mode.height,
        refresh_millihertz = request.mode.refresh_millihertz,
        codec = codec,
        dynamic_range = dynamic_range,
        bitrate_kbps = request.bitrate_kbps,
    ))
}

fn format_uuid_bytes(value: [u8; 16]) -> String {
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        value[0],
        value[1],
        value[2],
        value[3],
        value[4],
        value[5],
        value[6],
        value[7],
        value[8],
        value[9],
        value[10],
        value[11],
        value[12],
        value[13],
        value[14],
        value[15]
    )
}

fn parse_display_service_config<F>(
    bytes: &[u8],
    mut credential: F,
) -> Result<LoadedDisplayServiceConfig, String>
where
    F: FnMut(&str) -> Result<Vec<u8>, String>,
{
    if bytes.is_empty() || bytes.len() as u64 > MAX_DISPLAY_CONFIG_BYTES {
        return Err("display service configuration is outside its size bound".to_owned());
    }
    let stored: DisplayServiceFile = serde_json::from_slice(bytes)
        .map_err(|_| "display service configuration is invalid".to_owned())?;
    if stored.schema_version != 2 {
        return Err("display service configuration version is unsupported".to_owned());
    }
    if stored.routes.is_empty() || stored.routes.len() > IDD_CONTROL_MAX_MONITORS as usize {
        return Err("display service Route count is outside 1..=8".to_owned());
    }
    let mut endpoints = HashSet::new();
    let mut route_ids = HashSet::new();
    let mut stream_ids = HashSet::new();
    let mut capabilities = HashSet::new();
    let mut prepared = Vec::with_capacity(stored.routes.len());
    for route in stored.routes {
        let route_config = prepare_display_route(route)?;
        if !endpoints.insert((route_config.endpoint.address, route_config.endpoint.port)) {
            return Err("display service endpoint is duplicated".to_owned());
        }
        if !route_ids.insert(route_config.binding.route_id()) {
            return Err("display service Route is duplicated".to_owned());
        }
        if !stream_ids.insert(route_config.binding.stream_id()) {
            return Err("display service Stream is duplicated".to_owned());
        }
        if !capabilities.insert(route_config.sink_identity) {
            return Err("display service Sink Capability is duplicated".to_owned());
        }
        prepared.push(route_config);
    }
    let mut routes = Vec::with_capacity(prepared.len());
    for route in prepared {
        routes.push(load_prepared_route(route, &mut credential)?);
    }
    Ok(LoadedDisplayServiceConfig { routes })
}

fn parse_single_display_service_config<F>(
    bytes: &[u8],
    mut credential: F,
) -> Result<LoadedSingleDisplayServiceConfig, String>
where
    F: FnMut(&str) -> Result<Vec<u8>, String>,
{
    if bytes.is_empty() || bytes.len() as u64 > MAX_DISPLAY_CONFIG_BYTES {
        return Err("single-display service configuration is outside its size bound".to_owned());
    }
    let stored: SingleDisplayServiceFile = serde_json::from_slice(bytes)
        .map_err(|_| "single-display service configuration is invalid".to_owned())?;
    if stored.schema_version != 3 {
        return Err("single-display service configuration version is unsupported".to_owned());
    }
    let prepared = prepare_display_route(stored.route)?;
    if prepared.endpoint.io_timeout < MIN_HEARTBEAT_TIMEOUT {
        return Err("single-display I/O timeout must be inside 1000..=5000ms".to_owned());
    }
    let (launch_endpoint, engine_file) = prepare_engine_launch_endpoint(stored.engine_launch)?;
    if launch_endpoint.address != prepared.endpoint.address {
        return Err("single-display launch and media addresses must match".to_owned());
    }
    if launch_endpoint.port == prepared.endpoint.port {
        return Err("single-display launch and media ports must differ".to_owned());
    }
    let (engine, worker) = prepare_display_engine(engine_file, &prepared)?;
    let engine_ports = engine.session.ports();
    if [
        engine_ports.https,
        engine_ports.video,
        engine_ports.control,
        engine_ports.rtsp,
    ]
    .contains(&prepared.endpoint.port)
        || [
            engine_ports.https,
            engine_ports.video,
            engine_ports.control,
            engine_ports.rtsp,
        ]
        .contains(&launch_endpoint.port)
    {
        return Err("single-display service and GameStream ports must not overlap".to_owned());
    }
    let launch_identity = load_prepared_identity(&prepared, &mut credential)?;
    Ok(LoadedSingleDisplayServiceConfig {
        route: load_prepared_route(prepared, &mut credential)?,
        launch_endpoint,
        launch_identity,
        engine,
        worker,
    })
}

fn prepare_engine_launch_endpoint(
    stored: EngineLaunchEndpointFile,
) -> Result<(DisplayTlsServerEndpoint, EngineLaunchEndpointFile), String> {
    let address = stored
        .address
        .parse::<Ipv4Addr>()
        .map_err(|_| "single-display launch address must be an IPv4 literal".to_owned())?;
    let endpoint = DisplayTlsServerEndpoint {
        address,
        port: stored.port,
        io_timeout: Duration::from_millis(stored.io_timeout_millis),
    };
    endpoint.validate().map_err(|error| error.to_string())?;
    if !(1_000..=5_000).contains(&stored.io_timeout_millis) {
        return Err("single-display launch timeout must be inside 1000..=5000ms".to_owned());
    }
    Ok((endpoint, stored))
}

fn prepare_display_engine(
    stored: EngineLaunchEndpointFile,
    route: &PreparedDisplayRouteConfig,
) -> Result<(LoadedDisplayEngineConfig, LoadedDisplayWorkerConfig), String> {
    let adapter_instance =
        parse_id::<AdapterInstanceId>(&stored.adapter_instance_id, "Adapter instance")?;
    let library_path = PathBuf::from(&stored.engine_library_path);
    let extended_unc = stored
        .engine_library_path
        .get(..8)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(r"\\?\UNC\"));
    let ordinary_unc = stored.engine_library_path.starts_with(r"\\")
        && !stored.engine_library_path.starts_with(r"\\?\");
    if stored.engine_library_path.is_empty()
        || stored.engine_library_path.len() > 1024
        || !library_path.is_absolute()
        || extended_unc
        || ordinary_unc
        || stored.engine_library_path.starts_with("//")
        || !library_path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension == "dll")
    {
        return Err("display engine path must be an absolute local DLL path".to_owned());
    }
    let expected_sha256 = decode_lower_sha256(&stored.engine_sha256)?;
    let worker_executable_path = PathBuf::from(&stored.worker_executable_path);
    let worker_extended_unc = stored
        .worker_executable_path
        .get(..8)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(r"\\?\UNC\"));
    let worker_ordinary_unc = stored.worker_executable_path.starts_with(r"\\")
        && !stored.worker_executable_path.starts_with(r"\\?\");
    if stored.worker_executable_path.is_empty()
        || stored.worker_executable_path.len() > 1024
        || !worker_executable_path.is_absolute()
        || worker_extended_unc
        || worker_ordinary_unc
        || stored.worker_executable_path.starts_with("//")
        || !worker_executable_path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension == "exe")
    {
        return Err("display worker path must be an absolute local EXE path".to_owned());
    }
    let worker_expected_sha256 = decode_lower_sha256(&stored.worker_sha256)?;
    let authorized_session = InteractiveDisplaySessionId::new(stored.authorized_windows_session_id)
        .map_err(|_| "display worker authorized Windows Session is invalid".to_owned())?;
    let selected = route.binding.selected();
    let numerator = u64::from(selected.decoded.frame_rate.numerator());
    let denominator = u64::from(selected.decoded.frame_rate.denominator());
    let scaled = numerator
        .checked_mul(1000)
        .ok_or_else(|| "display engine refresh rate is invalid".to_owned())?;
    if denominator == 0 || !scaled.is_multiple_of(denominator) {
        return Err("display engine refresh rate is not exact millihertz".to_owned());
    }
    let refresh_millihertz = u32::try_from(scaled / denominator)
        .map_err(|_| "display engine refresh rate is invalid".to_owned())?;
    let codec = match stored.codec.as_str() {
        "h264" => VideoCodec::H264,
        "hevc" => VideoCodec::Hevc,
        "av1" => VideoCodec::Av1,
        _ => return Err("display engine codec is unsupported".to_owned()),
    };
    let dynamic_range = match stored.dynamic_range.as_str() {
        "sdr8" => DynamicRange::Sdr8,
        "hdr10" => DynamicRange::Hdr10,
        _ => return Err("display engine dynamic range is unsupported".to_owned()),
    };
    let session = ValidatedDisplayStreamSession::new(DisplayStreamSessionRequest {
        adapter_instance_id: *adapter_instance.as_uuid().as_bytes(),
        display_capability_id: *route.sink_identity.capability_id().as_uuid().as_bytes(),
        mode: DisplayMode {
            width: selected.decoded.width,
            height: selected.decoded.height,
            refresh_millihertz,
        },
        codec,
        dynamic_range,
        bitrate_kbps: stored.bitrate_kbps,
        port_base: stored.port_base,
        policy: DisplayOnlyPolicy::CAPYIO,
    })
    .map_err(|error| error.to_string())?;
    Ok((
        LoadedDisplayEngineConfig {
            library_path,
            expected_sha256,
            session,
        },
        LoadedDisplayWorkerConfig {
            executable_path: worker_executable_path,
            expected_sha256: worker_expected_sha256,
            authorized_session,
        },
    ))
}

fn decode_lower_sha256(value: &str) -> Result<[u8; 32], String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(
            "display engine SHA-256 must be 64 lowercase hexadecimal characters".to_owned(),
        );
    }
    let mut decoded = [0_u8; 32];
    for (index, byte) in decoded.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| "display engine SHA-256 is invalid".to_owned())?;
    }
    Ok(decoded)
}

fn load_prepared_identity<F>(
    route: &PreparedDisplayRouteConfig,
    credential: &mut F,
) -> Result<DisplayTlsServerIdentity, String>
where
    F: FnMut(&str) -> Result<Vec<u8>, String>,
{
    let server_certificate = credential_path(&route.server_certificate_path, credential)?;
    let server_private_key = credential_path(&route.server_private_key_path, credential)?;
    let client_ca = credential_path(&route.client_ca_certificate_path, credential)?;
    Ok(DisplayTlsServerIdentity {
        certificate_chain_der: vec![server_certificate, client_ca.clone()],
        private_key_der: server_private_key,
        client_ca_der: client_ca,
    })
}

fn load_prepared_route<F>(
    route: PreparedDisplayRouteConfig,
    credential: &mut F,
) -> Result<LoadedDisplayRouteConfig, String>
where
    F: FnMut(&str) -> Result<Vec<u8>, String>,
{
    let identity = load_prepared_identity(&route, credential)?;
    Ok(LoadedDisplayRouteConfig {
        endpoint: route.endpoint,
        identity,
        binding: route.binding,
        sink_identity: route.sink_identity,
    })
}

fn prepare_display_route(stored: DisplayRouteFile) -> Result<PreparedDisplayRouteConfig, String> {
    let address = stored
        .address
        .parse::<Ipv4Addr>()
        .map_err(|_| "display address must be an IPv4 literal".to_owned())?;
    let endpoint = DisplayTlsServerEndpoint {
        address,
        port: stored.port,
        io_timeout: Duration::from_millis(stored.io_timeout_millis),
    };
    endpoint.validate().map_err(|error| error.to_string())?;
    if stored.io_timeout_millis > 5_000 {
        return Err("display I/O timeout must be inside 100..=5000ms".to_owned());
    }
    let frame_rate = FrameRate::new(stored.frame_rate_numerator, stored.frame_rate_denominator)
        .ok_or_else(|| "display frame rate is invalid".to_owned())?;
    let decoded = DisplayStreamSpec {
        width: stored.width,
        height: stored.height,
        frame_rate,
        pixel_format: VideoPixelFormat::Bgra8,
        colorimetry: VideoColorimetry::SrgbFull,
        cursor_mode: DisplayCursorMode::Composited,
        target_latency_millis: 50,
        maximum_latency_millis: 150,
        allow_frame_drop: true,
    };
    let avc_level = AvcLevel::minimum_for(&decoded)
        .ok_or_else(|| "display mode exceeds the supported AVC levels".to_owned())?;
    let selected = EncodedDisplayStreamSpec {
        decoded,
        codec: EncodedDisplayCodec::AvcAnnexB,
        avc_profile: AvcProfile::Main,
        avc_level,
        maximum_access_unit_bytes: stored.maximum_access_unit_bytes,
    };
    let binding = DisplayTransportBinding::new(
        parse_id::<SessionId>(&stored.session_id, "Session")?,
        parse_id::<RouteId>(&stored.route_id, "Route")?,
        parse_id::<StreamId>(&stored.stream_id, "Stream")?,
        stored.stream_epoch,
        selected,
    )
    .map_err(|error| error.to_string())?;
    let capability = parse_id::<CapabilityId>(&stored.sink_capability_id, "Capability")?;
    let sink_identity = IddMonitorIdentity::new(capability).map_err(|error| error.to_string())?;
    for path in [
        &stored.server_certificate_path,
        &stored.server_private_key_path,
        &stored.client_ca_certificate_path,
    ] {
        validate_credential_path(path)?;
    }
    Ok(PreparedDisplayRouteConfig {
        endpoint,
        binding,
        sink_identity,
        server_certificate_path: stored.server_certificate_path,
        server_private_key_path: stored.server_private_key_path,
        client_ca_certificate_path: stored.client_ca_certificate_path,
    })
}

fn parse_id<T>(value: &str, label: &str) -> Result<T, String>
where
    T: std::str::FromStr,
{
    if value.len() != 36 {
        return Err(format!("display {label} identity is invalid"));
    }
    value
        .parse()
        .map_err(|_| format!("display {label} identity is invalid"))
}

fn credential_path<F>(path: &str, credential: &mut F) -> Result<Vec<u8>, String>
where
    F: FnMut(&str) -> Result<Vec<u8>, String>,
{
    validate_credential_path(path)?;
    credential(path)
}

fn validate_credential_path(path: &str) -> Result<(), String> {
    if path.is_empty() || path.len() > 1024 {
        Err("display credential path is outside its bound".to_owned())
    } else {
        Ok(())
    }
}

fn read_credential(path: &str) -> Result<Vec<u8>, String> {
    let metadata = fs::metadata(path).map_err(|_| "read display credential".to_owned())?;
    if metadata.len() == 0 || metadata.len() > MAX_CREDENTIAL_BYTES {
        return Err("display credential is outside its size bound".to_owned());
    }
    fs::read(path).map_err(|_| "read display credential".to_owned())
}

pub struct DisplayServiceHost {
    projections: DisplayRouteProjectionRuntime<IddMonitorCoordinator>,
    routes: Vec<DisplayRouteHost>,
}

struct DisplayRouteHost {
    server: AuthenticatedDisplayServer,
    sink_identity: IddMonitorIdentity,
    authentication_failures: u8,
    active: Option<ActiveDisplayRoute>,
}

struct ActiveDisplayRoute {
    sender: AuthenticatedDisplaySender,
    pipeline: MediaFoundationDisplayPipeline,
    snapshot: DisplayRouteProjectionSnapshot,
}

impl DisplayServiceHost {
    pub fn open(config: LoadedDisplayServiceConfig) -> Result<Self, String> {
        let coordinator =
            IddMonitorCoordinator::open_unique().map_err(|error| error.to_string())?;
        let mut routes = Vec::with_capacity(config.routes.len());
        for route in config.routes {
            let server =
                AuthenticatedDisplayServer::bind(route.endpoint, route.identity, route.binding)
                    .map_err(|error| error.to_string())?;
            server
                .set_nonblocking(true)
                .map_err(|error| error.to_string())?;
            routes.push(DisplayRouteHost {
                server,
                sink_identity: route.sink_identity,
                authentication_failures: 0,
                active: None,
            });
        }
        Ok(Self {
            projections: DisplayRouteProjectionRuntime::new(coordinator),
            routes,
        })
    }

    pub fn run(mut self, stop: Arc<AtomicBool>) -> Result<(), String> {
        while !stop.load(Ordering::Acquire) {
            let mut active_or_progress = false;
            for index in 0..self.routes.len() {
                if self.routes[index].active.is_some() {
                    active_or_progress = true;
                    self.poll_active_route(index)?;
                } else if self.poll_route_accept(index)? {
                    active_or_progress = true;
                }
            }
            thread::sleep(if active_or_progress {
                FRAME_POLL_INTERVAL
            } else {
                ACCEPT_POLL_INTERVAL
            });
        }
        self.disconnect_all()
    }

    fn poll_route_accept(&mut self, index: usize) -> Result<bool, String> {
        let accepted = self.routes[index].server.try_accept();
        let sender = match accepted {
            Ok(Some(sender)) => sender,
            Ok(None) => return Ok(false),
            Err(_) => {
                let failures = self.routes[index].authentication_failures.saturating_add(1);
                self.routes[index].authentication_failures = failures;
                if failures >= MAX_AUTHENTICATION_FAILURES {
                    return Err("display authentication failure limit reached".to_owned());
                }
                return Ok(true);
            }
        };
        self.routes[index].authentication_failures = 0;
        let spec = projection_spec(self.routes[index].sink_identity, sender.binding())
            .map_err(|error| error.to_string())?;
        let snapshot = self
            .projections
            .activate_authorized(spec)
            .map_err(|error| error.to_string())?;
        let pipeline = match MediaFoundationDisplayPipeline::new(
            sender.binding().clone(),
            MediaFoundationH264EncoderConfig::default(),
        ) {
            Ok(pipeline) => pipeline,
            Err(error) => {
                self.projections
                    .deactivate(snapshot.spec.route_id, snapshot.spec.stream_epoch)
                    .map_err(|cleanup| format!("{error}; display cleanup failed: {cleanup}"))?;
                return Err(error.to_string());
            }
        };
        self.routes[index].active = Some(ActiveDisplayRoute {
            sender,
            pipeline,
            snapshot,
        });
        Ok(true)
    }

    fn poll_active_route(&mut self, index: usize) -> Result<(), String> {
        let identity = self.routes[index]
            .active
            .as_ref()
            .expect("active display Route is present")
            .snapshot
            .spec
            .sink_identity;
        let Some(frame) = self
            .projections
            .projection_mut()
            .try_read(identity)
            .map_err(|error| error.to_string())?
        else {
            return Ok(());
        };
        let outcome = {
            let active = self.routes[index]
                .active
                .as_mut()
                .expect("active display Route remains present");
            active
                .pipeline
                .encode_frame_to_sender(&frame, &mut active.sender)
        };
        match outcome {
            Ok(_) => Ok(()),
            Err(MfDisplayPipelineError::Transport(_)) => self.disconnect_route(index),
            Err(error) => {
                let cleanup = self.disconnect_route(index);
                match cleanup {
                    Ok(()) => Err(error.to_string()),
                    Err(cleanup) => Err(format!("{error}; display cleanup failed: {cleanup}")),
                }
            }
        }
    }

    fn disconnect_route(&mut self, index: usize) -> Result<(), String> {
        let Some(active) = self.routes[index].active.take() else {
            return Ok(());
        };
        let result = self
            .projections
            .deactivate(
                active.snapshot.spec.route_id,
                active.snapshot.spec.stream_epoch,
            )
            .map(|_| ())
            .map_err(|error| error.to_string());
        drop(active);
        result
    }

    fn disconnect_all(&mut self) -> Result<(), String> {
        let mut first_error = None;
        for index in 0..self.routes.len() {
            if let Err(error) = self.disconnect_route(index)
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

/// Single-Sink display host. The mature engine worker owns the SudoVDA monitor;
/// this host retains one Capability-scoped handoff mapping only for the bounded
/// CPD1 compatibility path and never opens a dynamic-monitor controller itself.
pub struct SingleDisplayServiceHost {
    server: AuthenticatedDisplayServer,
    launch_server: AuthenticatedGameStreamLaunchServer,
    engine: LoadedDisplayEngineConfig,
    engine_binding: GameStreamLaunchBinding,
    worker_executable: VerifiedDisplayWorkerExecutable,
    worker_session: InteractiveDisplaySessionId,
    engine_worker: Option<BoundDisplayWorkerChannel>,
    mapping: IddFrameBrokerMapping,
    sink_identity: IddMonitorIdentity,
    authentication_failures: u8,
    active: Option<ActiveSingleDisplayRoute>,
    lifecycle: SingleDisplayLifecycle,
    status: SingleDisplayStatusHandle,
    next_topology_poll: Instant,
}

struct ActiveSingleDisplayRoute {
    sender: AuthenticatedDisplaySender,
    pipeline: MediaFoundationDisplayPipeline,
    next_heartbeat: Instant,
}

impl SingleDisplayServiceHost {
    pub fn open(config: LoadedSingleDisplayServiceConfig) -> Result<Self, String> {
        let route = config.route;
        let engine = config.engine;
        let worker_executable = VerifiedDisplayWorkerExecutable::open(
            &config.worker.executable_path,
            config.worker.expected_sha256,
        )
        .map_err(|error| error.to_string())?;
        let engine_binding = GameStreamLaunchBinding {
            session_id: *route.binding.session_id().as_uuid().as_bytes(),
            route_id: *route.binding.route_id().as_uuid().as_bytes(),
            stream_id: *route.binding.stream_id().as_uuid().as_bytes(),
            stream_epoch: route.binding.stream_epoch(),
            host_ipv4: config.launch_endpoint.address,
        };
        // Keep the compatibility handoff identity-scoped. The production
        // SudoVDA worker does not consume this mapping, and using the legacy
        // fixed v1 name would make an otherwise isolated single-Sink service
        // collide with an installed service instance.
        let sink_identity = route.sink_identity;
        let mapping = IddFrameBrokerMapping::create_for_monitor(sink_identity)
            .map_err(|error| error.to_string())?;
        let server =
            AuthenticatedDisplayServer::bind(route.endpoint, route.identity, route.binding)
                .map_err(|error| error.to_string())?;
        server
            .set_nonblocking(true)
            .map_err(|error| error.to_string())?;
        let launch_server = AuthenticatedGameStreamLaunchServer::bind(
            config.launch_endpoint,
            config.launch_identity,
        )
        .map_err(|error| error.to_string())?;
        launch_server
            .set_nonblocking(true)
            .map_err(|error| error.to_string())?;
        let lifecycle = SingleDisplayLifecycle::new();
        let status = SingleDisplayStatusHandle {
            inner: Arc::new(Mutex::new(lifecycle.snapshot.clone())),
        };
        let mut host = Self {
            server,
            launch_server,
            engine,
            engine_binding,
            worker_executable,
            worker_session: config.worker.authorized_session,
            engine_worker: None,
            mapping,
            sink_identity,
            authentication_failures: 0,
            active: None,
            lifecycle,
            status,
            next_topology_poll: Instant::now(),
        };
        host.poll_topology_if_due();
        Ok(host)
    }

    #[must_use]
    pub const fn sink_identity(&self) -> IddMonitorIdentity {
        self.sink_identity
    }

    #[must_use]
    pub fn status_handle(&self) -> SingleDisplayStatusHandle {
        self.status.clone()
    }

    pub fn launch_local_addr(&self) -> Result<SocketAddr, String> {
        self.launch_server
            .local_addr()
            .map_err(|error| error.to_string())
    }

    #[must_use]
    pub fn engine_library_path(&self) -> &Path {
        &self.engine.library_path
    }

    #[must_use]
    pub const fn engine_expected_sha256(&self) -> [u8; 32] {
        self.engine.expected_sha256
    }

    #[must_use]
    pub const fn engine_session(&self) -> &ValidatedDisplayStreamSession {
        &self.engine.session
    }

    pub fn run(mut self, stop: Arc<AtomicBool>) -> Result<(), String> {
        let (run, cleanup) = run_with_cleanup(
            &mut self,
            |host| host.run_until_stop(stop),
            |host| {
                let result = host.stop_engine_worker();
                host.disconnect();
                result
            },
        );
        self.lifecycle.finish(run.is_err(), cleanup.is_err());
        self.publish_status();
        // Cleanup failure takes precedence: callers must not report a clean stop.
        cleanup.and(run)
    }

    fn run_until_stop(&mut self, stop: Arc<AtomicBool>) -> Result<(), String> {
        while !stop.load(Ordering::Acquire) {
            self.poll_topology_if_due();
            let active_or_progress = if self.engine_worker.is_some() {
                self.poll_engine_worker()?
            } else if self.active.is_some() {
                self.poll_active_route()?;
                true
            } else {
                self.poll_engine_launch()? || self.poll_accept()?
            };
            thread::sleep(if active_or_progress {
                FRAME_POLL_INTERVAL
            } else {
                ACCEPT_POLL_INTERVAL
            });
        }
        Ok(())
    }

    fn poll_engine_launch(&mut self) -> Result<bool, String> {
        let secrets = generate_launch_secrets()?;
        let exchange = try_exchange_authenticated_engine_launch(
            &self.launch_server,
            &self.engine.session,
            self.engine_binding,
            secrets,
        );
        let launch = match exchange {
            Ok(Some(launch)) => launch,
            Ok(None) => return Ok(false),
            Err(_) => {
                self.authentication_failures = self.authentication_failures.saturating_add(1);
                if self.authentication_failures >= MAX_AUTHENTICATION_FAILURES {
                    self.lifecycle.fail(AUTHENTICATION_FAILURE_LIMIT);
                    self.publish_status();
                    return Err(
                        "single-display launch authentication failure limit reached".to_owned()
                    );
                }
                return Ok(true);
            }
        };
        self.authentication_failures = 0;

        let result = self.start_engine_worker(launch);
        if result.is_err() {
            self.lifecycle.fail(ENGINE_WORKER_FAILED);
            self.publish_status();
        }
        result.map(|()| true)
    }

    fn start_engine_worker(
        &mut self,
        launch: capyio_display_engine_host::AuthenticatedEngineLaunch,
    ) -> Result<(), String> {
        let request_id = secure_nonzero_u64()?;
        let lease = DisplayWorkerLease::new(
            request_id,
            self.worker_session,
            &self.engine.session,
            self.engine_binding,
        )
        .map_err(|error| error.to_string())?;
        let pipe_name = DisplayWorkerPipeName::parse(&format!(
            r"\\.\pipe\CapyIO.DisplayWorker.{request_id:016x}"
        ))
        .map_err(|error| error.to_string())?;
        let mut channel = BoundDisplayWorkerChannel::launch(
            &self.worker_executable,
            &pipe_name,
            self.worker_session,
            Duration::from_secs(5),
        )
        .map_err(|error| error.to_string())?;
        channel
            .exchange_lease(&lease, Duration::from_secs(5))
            .map_err(|error| error.to_string())?;
        let configuration = DisplayWorkerEngineConfigRecord::new(
            &lease,
            &self.engine.library_path,
            self.engine.expected_sha256,
            &self.engine.session,
        )
        .map_err(|error| error.to_string())?;
        channel
            .exchange_configuration(&configuration, Duration::from_secs(5))
            .map_err(|error| error.to_string())?;
        let start = launch
            .into_worker_start_record(&lease)
            .map_err(|error| error.to_string())?;
        channel
            .exchange_start(start, Duration::from_secs(5))
            .map_err(|error| error.to_string())?;
        self.lifecycle
            .receiver_authenticated()
            .map_err(|()| "single-display generation exhausted".to_owned())?;
        self.engine_worker = Some(channel);
        self.publish_status();
        Ok(())
    }

    fn poll_engine_worker(&mut self) -> Result<bool, String> {
        let running = self
            .engine_worker
            .as_ref()
            .expect("engine worker exists")
            .is_running()
            .map_err(|error| error.to_string())?;
        if running {
            return Ok(true);
        }
        self.engine_worker = None;
        self.lifecycle.transport_lost(TRANSPORT_LOST);
        self.publish_status();
        Ok(false)
    }

    fn stop_engine_worker(&mut self) -> Result<(), String> {
        let Some(mut worker) = self.engine_worker.take() else {
            return Ok(());
        };
        worker
            .graceful_stop(Duration::from_secs(5))
            .map_err(|error| error.to_string())?;
        worker
            .wait_for_exit(ENGINE_WORKER_GRACEFUL_EXIT_TIMEOUT)
            .map_err(|error| error.to_string())
    }

    fn poll_accept(&mut self) -> Result<bool, String> {
        let sender = match self.server.try_accept() {
            Ok(Some(sender)) => sender,
            Ok(None) => return Ok(false),
            Err(_) => {
                self.authentication_failures = self.authentication_failures.saturating_add(1);
                if self.authentication_failures >= MAX_AUTHENTICATION_FAILURES {
                    self.lifecycle.fail(AUTHENTICATION_FAILURE_LIMIT);
                    self.publish_status();
                    return Err("single-display authentication failure limit reached".to_owned());
                }
                return Ok(true);
            }
        };
        self.authentication_failures = 0;
        let pipeline = MediaFoundationDisplayPipeline::new(
            sender.binding().clone(),
            MediaFoundationH264EncoderConfig::default(),
        )
        .map_err(|error| {
            self.lifecycle.fail(ENCODER_FAILED);
            self.publish_status();
            error.to_string()
        })?;
        self.lifecycle.receiver_authenticated().map_err(|()| {
            self.publish_status();
            "single-display generation exhausted".to_owned()
        })?;
        self.active = Some(ActiveSingleDisplayRoute {
            sender,
            pipeline,
            next_heartbeat: Instant::now() + HEARTBEAT_INTERVAL,
        });
        self.publish_status();
        Ok(true)
    }

    fn poll_active_route(&mut self) -> Result<(), String> {
        if self
            .active
            .as_ref()
            .is_some_and(|active| Instant::now() >= active.next_heartbeat)
        {
            let heartbeat = self
                .active
                .as_mut()
                .expect("single-display Route remains active")
                .sender
                .round_trip_heartbeat();
            match heartbeat {
                Ok(()) => {
                    self.active
                        .as_mut()
                        .expect("single-display Route remains active")
                        .next_heartbeat = Instant::now() + HEARTBEAT_INTERVAL;
                    self.lifecycle.heartbeat_acknowledged();
                    self.publish_status();
                }
                Err(DisplayTlsError::Io(error)) => {
                    let problem = transport_problem(error.kind());
                    self.disconnect_with_problem(problem);
                    return Ok(());
                }
                Err(error) => {
                    self.disconnect();
                    self.lifecycle.fail(HEARTBEAT_PROTOCOL_VIOLATION);
                    self.publish_status();
                    return Err(error.to_string());
                }
            }
        }
        let Some(frame) = self.mapping.try_read().map_err(|error| {
            self.lifecycle.fail(ENCODER_FAILED);
            self.publish_status();
            error.to_string()
        })?
        else {
            return Ok(());
        };
        let (expected_width, expected_height) = {
            let selected = self
                .active
                .as_ref()
                .expect("single-display Route remains active")
                .sender
                .binding()
                .selected();
            (selected.decoded.width, selected.decoded.height)
        };
        if !display_dimensions_match(
            frame.header.width,
            frame.header.height,
            expected_width,
            expected_height,
        ) {
            self.lifecycle.frame_mode_mismatch();
            self.publish_status();
            return Ok(());
        }
        let active = self
            .active
            .as_mut()
            .expect("single-display Route remains active");
        match active
            .pipeline
            .encode_frame_to_sender(&frame, &mut active.sender)
        {
            Ok(_) => {
                self.lifecycle.frame_sent();
                self.publish_status();
                Ok(())
            }
            Err(MfDisplayPipelineError::Transport(DisplayTlsError::Io(error))) => {
                let problem = transport_problem(error.kind());
                self.disconnect_with_problem(problem);
                Ok(())
            }
            Err(MfDisplayPipelineError::Transport(_)) => {
                self.disconnect_with_problem(TRANSPORT_LOST);
                Ok(())
            }
            Err(error) => {
                self.disconnect();
                self.lifecycle.fail(ENCODER_FAILED);
                self.publish_status();
                Err(error.to_string())
            }
        }
    }

    fn disconnect(&mut self) {
        self.active = None;
    }

    fn disconnect_with_problem(&mut self, problem_code: &'static str) {
        self.disconnect();
        self.lifecycle.transport_lost(problem_code);
        self.publish_status();
    }

    fn poll_topology_if_due(&mut self) {
        let now = Instant::now();
        if now < self.next_topology_poll {
            return;
        }
        self.next_topology_poll = now + TOPOLOGY_POLL_INTERVAL;
        let active = inspect_capyio_display_path()
            .map(|path| path.active && path.target_available)
            .map_err(|_| ());
        self.lifecycle.observe_path(active);
        self.publish_status();
    }

    fn publish_status(&self) {
        *self
            .status
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = self.lifecycle.snapshot.clone();
    }
}

fn run_with_cleanup<T>(
    owner: &mut T,
    run: impl FnOnce(&mut T) -> Result<(), String>,
    cleanup: impl FnOnce(&mut T) -> Result<(), String>,
) -> (Result<(), String>, Result<(), String>) {
    let result = run(owner);
    let cleanup_result = cleanup(owner);
    (result, cleanup_result)
}

#[cfg(test)]
mod shutdown_tests {
    use super::*;

    #[test]
    fn returned_error_still_runs_cleanup_exactly_once() {
        let mut steps = Vec::new();
        let (run, cleanup) = run_with_cleanup(
            &mut steps,
            |steps| {
                steps.push("run");
                Err("poll failed".to_owned())
            },
            |steps| {
                steps.push("cleanup");
                Ok(())
            },
        );
        assert!(run.is_err());
        assert!(cleanup.is_ok());
        assert_eq!(steps, ["run", "cleanup"]);
    }

    #[test]
    fn normal_stop_also_runs_cleanup_exactly_once() {
        let mut calls = 0;
        let (run, cleanup) = run_with_cleanup(
            &mut calls,
            |_| Ok(()),
            |calls| {
                *calls += 1;
                Ok(())
            },
        );
        assert!(run.is_ok() && cleanup.is_ok());
        assert_eq!(calls, 1);
    }

    #[test]
    fn cleanup_failure_is_retained_alongside_original_error() {
        let (run, cleanup) = run_with_cleanup(
            &mut (),
            |_| Err("run failed".to_owned()),
            |_| Err("cleanup failed".to_owned()),
        );
        assert_eq!(run, Err("run failed".to_owned()));
        assert_eq!(cleanup, Err("cleanup failed".to_owned()));
    }

    #[test]
    fn final_status_preserves_failure_and_never_claims_clean_stop() {
        let mut lifecycle = SingleDisplayLifecycle::new();
        lifecycle.receiver_authenticated().unwrap();
        lifecycle.fail(ENGINE_WORKER_FAILED);
        lifecycle.finish(true, false);
        assert_eq!(lifecycle.snapshot.state, SingleDisplayState::Failed);
        assert_eq!(
            lifecycle.snapshot.problem_code.as_deref(),
            Some(ENGINE_WORKER_FAILED)
        );
        assert!(!lifecycle.snapshot.receiver_connected);
        lifecycle.finish(true, true);
        assert_eq!(
            lifecycle.snapshot.problem_code.as_deref(),
            Some(CLEANUP_FAILED)
        );
        lifecycle.finish(false, false);
        assert_eq!(lifecycle.snapshot.state, SingleDisplayState::Stopped);
        assert!(lifecycle.snapshot.problem_code.is_none());
    }

    #[test]
    fn unclassified_host_error_becomes_failed_not_waiting() {
        let mut lifecycle = SingleDisplayLifecycle::new();
        lifecycle.finish(true, false);
        assert_eq!(lifecycle.snapshot.state, SingleDisplayState::Failed);
        assert_eq!(
            lifecycle.snapshot.problem_code.as_deref(),
            Some(HOST_FAILED)
        );
    }
}

fn transport_problem(kind: std::io::ErrorKind) -> &'static str {
    if matches!(
        kind,
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
    ) {
        HEARTBEAT_TIMEOUT
    } else {
        TRANSPORT_LOST
    }
}

fn fill_secure_random(bytes: &mut [u8]) -> Result<(), String> {
    let length = u32::try_from(bytes.len())
        .map_err(|_| "display launch random request is too large".to_owned())?;
    // SAFETY: the system-preferred provider requires a null algorithm handle;
    // bytes is exclusively borrowed writable storage of exactly length bytes.
    let status = unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            bytes.as_mut_ptr(),
            length,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status < 0 {
        Err("Windows secure random generation failed".to_owned())
    } else {
        Ok(())
    }
}

fn secure_nonzero_u64() -> Result<u64, String> {
    loop {
        let mut bytes = [0_u8; 8];
        fill_secure_random(&mut bytes)?;
        let value = u64::from_be_bytes(bytes);
        if value != 0 {
            return Ok(value);
        }
    }
}

fn generate_launch_secrets() -> Result<GameStreamLaunchSecrets, String> {
    loop {
        let mut bytes = [0_u8; 36];
        fill_secure_random(&mut bytes)?;
        let launch_session_id = u32::from_be_bytes(bytes[0..4].try_into().unwrap());
        let key: [u8; 16] = bytes[4..20].try_into().unwrap();
        if launch_session_id == 0 || key == [0; 16] {
            continue;
        }
        return Ok(GameStreamLaunchSecrets {
            launch_session_id,
            key,
            key_id: u32::from_be_bytes(bytes[20..24].try_into().unwrap()),
            ping_payload: bytes[24..32].try_into().unwrap(),
            control_connect_data: u32::from_be_bytes(bytes[32..36].try_into().unwrap()),
        });
    }
}

const fn display_dimensions_match(
    actual_width: u32,
    actual_height: u32,
    expected_width: u32,
    expected_height: u32,
) -> bool {
    actual_width == expected_width && actual_height == expected_height
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"{
      "schemaVersion": 2,
      "routes": [{
        "address": "127.0.0.1",
        "port": 7443,
        "ioTimeoutMillis": 5000,
        "serverCertificatePath": "server.der",
        "serverPrivateKeyPath": "server.pk8",
        "clientCaCertificatePath": "ca.der",
        "sessionId": "00000000-0000-4000-8000-000000000001",
        "routeId": "00000000-0000-4000-8000-000000000002",
        "streamId": "00000000-0000-4000-8000-000000000003",
        "streamEpoch": 4,
        "sinkCapabilityId": "10112233-4455-6677-8899-aabbccddeeff",
        "width": 1280,
        "height": 720,
        "frameRateNumerator": 30,
        "frameRateDenominator": 1,
        "maximumAccessUnitBytes": 2097152
      }]
    }"#;

    #[test]
    fn closed_config_builds_exact_binding_and_loads_only_named_credentials() {
        let mut paths = Vec::new();
        let config = parse_display_service_config(VALID.as_bytes(), |path| {
            paths.push(path.to_owned());
            Ok(vec![1, 2, 3])
        })
        .unwrap();
        assert_eq!(config.routes.len(), 1);
        let route = &config.routes[0];
        assert_eq!(route.endpoint.address, Ipv4Addr::LOCALHOST);
        assert_eq!(route.endpoint.port, 7443);
        assert_eq!(route.binding.stream_epoch(), 4);
        assert_eq!(route.binding.selected().decoded.width, 1280);
        assert_eq!(route.binding.selected().decoded.height, 720);
        assert_eq!(paths, vec!["server.der", "server.pk8", "ca.der"]);
    }

    #[test]
    fn unknown_fields_and_invalid_network_or_identity_fail_before_credentials() {
        for invalid in [
            VALID.replace("\n    }", ",\n      \"extra\": true\n    }"),
            VALID.replace("127.0.0.1", "0.0.0.0"),
            VALID.replace("00000000-0000-4000-8000-000000000002", "not-a-route"),
        ] {
            let mut called = false;
            assert!(
                parse_display_service_config(invalid.as_bytes(), |_| {
                    called = true;
                    Ok(vec![1])
                })
                .is_err()
            );
            assert!(!called);
        }
    }

    #[test]
    fn fractional_millihertz_selection_is_preserved_for_session_boundary_to_reject() {
        let invalid = VALID
            .replace(
                "\"frameRateNumerator\": 30",
                "\"frameRateNumerator\": 30000",
            )
            .replace(
                "\"frameRateDenominator\": 1",
                "\"frameRateDenominator\": 1001",
            );
        let config = parse_display_service_config(invalid.as_bytes(), |_| Ok(vec![1])).unwrap();
        assert_eq!(
            config.routes[0]
                .binding
                .selected()
                .decoded
                .frame_rate
                .numerator(),
            30_000
        );
        assert_eq!(
            config.routes[0]
                .binding
                .selected()
                .decoded
                .frame_rate
                .denominator(),
            1001
        );
    }

    #[test]
    fn two_routes_keep_independent_endpoints_identities_and_credentials() {
        let second = VALID
            .replace("\"port\": 7443", "\"port\": 7444")
            .replace(
                "00000000-0000-4000-8000-000000000001",
                "20000000-0000-4000-8000-000000000001",
            )
            .replace(
                "00000000-0000-4000-8000-000000000002",
                "20000000-0000-4000-8000-000000000002",
            )
            .replace(
                "00000000-0000-4000-8000-000000000003",
                "20000000-0000-4000-8000-000000000003",
            )
            .replace(
                "10112233-4455-6677-8899-aabbccddeeff",
                "20112233-4455-6677-8899-aabbccddeeff",
            )
            .replace("server.der", "server-2.der")
            .replace("server.pk8", "server-2.pk8")
            .replace("ca.der", "ca-2.der");
        let second_route: serde_json::Value = serde_json::from_str(&second).unwrap();
        let mut combined: serde_json::Value = serde_json::from_str(VALID).unwrap();
        combined["routes"]
            .as_array_mut()
            .unwrap()
            .push(second_route["routes"][0].clone());
        let bytes = serde_json::to_vec(&combined).unwrap();
        let mut paths = Vec::new();
        let config = parse_display_service_config(&bytes, |path| {
            paths.push(path.to_owned());
            Ok(vec![1])
        })
        .unwrap();
        assert_eq!(config.routes.len(), 2);
        assert_eq!(config.routes[0].endpoint.port, 7443);
        assert_eq!(config.routes[1].endpoint.port, 7444);
        assert_ne!(
            config.routes[0].binding.route_id(),
            config.routes[1].binding.route_id()
        );
        assert_ne!(
            config.routes[0].sink_identity,
            config.routes[1].sink_identity
        );
        assert_eq!(paths.len(), 6);
    }

    #[test]
    fn duplicate_route_endpoint_stream_or_capability_fails_before_credentials() {
        let original: serde_json::Value = serde_json::from_str(VALID).unwrap();
        for field in ["port", "routeId", "streamId", "sinkCapabilityId"] {
            let mut combined = original.clone();
            let mut second = original["routes"][0].clone();
            second["sessionId"] =
                serde_json::Value::String("20000000-0000-4000-8000-000000000001".to_owned());
            second["routeId"] =
                serde_json::Value::String("20000000-0000-4000-8000-000000000002".to_owned());
            second["streamId"] =
                serde_json::Value::String("20000000-0000-4000-8000-000000000003".to_owned());
            second["sinkCapabilityId"] =
                serde_json::Value::String("20112233-4455-6677-8899-aabbccddeeff".to_owned());
            second["port"] = serde_json::Value::from(7444);
            second[field] = original["routes"][0][field].clone();
            combined["routes"].as_array_mut().unwrap().push(second);
            let mut called = false;
            assert!(
                parse_display_service_config(&serde_json::to_vec(&combined).unwrap(), |_| {
                    called = true;
                    Ok(vec![1])
                })
                .is_err(),
                "duplicate {field} must fail"
            );
            assert!(!called);
        }
    }

    fn valid_single_config() -> Vec<u8> {
        let multi: serde_json::Value = serde_json::from_str(VALID).unwrap();
        serde_json::to_vec(&serde_json::json!({
            "schemaVersion": 3,
            "engineLaunch": {
                "address": "127.0.0.1",
                "port": 7543,
                "ioTimeoutMillis": 5000,
                "adapterInstanceId": "30112233-4455-6677-8899-aabbccddeeff",
                "engineLibraryPath": "C:\\Program Files\\CapyIO\\capyio-display-engine.dll",
                "engineSha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                "workerExecutablePath": "C:\\Program Files\\CapyIO\\capyio-display-engine-worker.exe",
                "workerSha256": "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789",
                "authorizedWindowsSessionId": 1,
                "portBase": 50000,
                "codec": "hevc",
                "dynamicRange": "sdr8",
                "bitrateKbps": 45000
            },
            "route": multi["routes"][0]
        }))
        .unwrap()
    }

    #[test]
    fn single_display_config_has_exactly_one_route_and_static_sink_identity() {
        let mut paths = Vec::new();
        let config = parse_single_display_service_config(&valid_single_config(), |path| {
            paths.push(path.to_owned());
            Ok(vec![1])
        })
        .unwrap();
        assert_eq!(config.route.endpoint.port, 7443);
        assert_eq!(config.launch_endpoint.port, 7543);
        assert_eq!(config.launch_endpoint.address, Ipv4Addr::LOCALHOST);
        assert_eq!(
            config.engine.library_path,
            PathBuf::from(r"C:\Program Files\CapyIO\capyio-display-engine.dll")
        );
        assert_eq!(config.engine.expected_sha256[0], 0x01);
        assert_eq!(config.engine.expected_sha256[31], 0xef);
        assert_eq!(
            config.worker.executable_path,
            PathBuf::from(r"C:\Program Files\CapyIO\capyio-display-engine-worker.exe")
        );
        assert_eq!(config.worker.expected_sha256[0], 0xab);
        assert_eq!(config.worker.authorized_session.get(), 1);
        assert_eq!(config.engine.session.request().mode.width, 1280);
        assert_eq!(config.engine.session.request().mode.height, 720);
        assert_eq!(
            config.engine.session.request().mode.refresh_millihertz,
            30_000
        );
        assert_eq!(config.engine.session.request().codec, VideoCodec::Hevc);
        assert_eq!(config.engine.session.ports().rtsp, 50_021);
        assert_eq!(config.route.binding.stream_epoch(), 4);
        assert_eq!(
            config.route.sink_identity.capability_id().to_string(),
            "10112233-4455-6677-8899-aabbccddeeff"
        );
        assert_eq!(
            paths,
            vec![
                "server.der",
                "server.pk8",
                "ca.der",
                "server.der",
                "server.pk8",
                "ca.der"
            ]
        );
    }

    #[test]
    fn android_gamestream_properties_are_derived_from_the_validated_service_config() {
        let properties =
            android_gamestream_properties_from_single_display_config_bytes(&valid_single_config())
                .unwrap();
        assert_eq!(properties.lines().count(), 17);
        for expected in [
            "host=127.0.0.1",
            "host_ipv4=127.0.0.1",
            "launch_port=7543",
            "timeout_millis=5000",
            "adapter_instance_id=30112233-4455-6677-8899-aabbccddeeff",
            "display_capability_id=10112233-4455-6677-8899-aabbccddeeff",
            "stream_epoch=4",
            "port_base=50000",
            "width=1280",
            "height=720",
            "refresh_millihertz=30000",
            "codec=2",
            "dynamic_range=1",
            "bitrate_kbps=45000",
        ] {
            assert!(properties.lines().any(|line| line == expected));
        }
        let mut invalid: serde_json::Value =
            serde_json::from_slice(&valid_single_config()).unwrap();
        invalid["engineLaunch"]["codec"] = serde_json::Value::String("vp9".to_owned());
        assert!(
            android_gamestream_properties_from_single_display_config_bytes(
                &serde_json::to_vec(&invalid).unwrap()
            )
            .is_err()
        );
    }

    #[test]
    fn single_display_rejects_multi_route_shapes_before_credentials() {
        let multi = VALID.as_bytes();
        let mut wrong_version: serde_json::Value =
            serde_json::from_slice(&valid_single_config()).unwrap();
        wrong_version["schemaVersion"] = serde_json::Value::from(2);
        let mut short_timeout: serde_json::Value =
            serde_json::from_slice(&valid_single_config()).unwrap();
        short_timeout["route"]["ioTimeoutMillis"] = serde_json::Value::from(999);
        let mut same_port: serde_json::Value =
            serde_json::from_slice(&valid_single_config()).unwrap();
        same_port["engineLaunch"]["port"] = same_port["route"]["port"].clone();
        let mut different_address: serde_json::Value =
            serde_json::from_slice(&valid_single_config()).unwrap();
        different_address["engineLaunch"]["address"] =
            serde_json::Value::String("127.0.0.2".to_owned());
        let mut long_launch_timeout: serde_json::Value =
            serde_json::from_slice(&valid_single_config()).unwrap();
        long_launch_timeout["engineLaunch"]["ioTimeoutMillis"] = serde_json::Value::from(5001);
        let mut uppercase_hash: serde_json::Value =
            serde_json::from_slice(&valid_single_config()).unwrap();
        uppercase_hash["engineLaunch"]["engineSha256"] = serde_json::Value::String(
            "0123456789ABCDEF0123456789abcdef0123456789abcdef0123456789abcdef".to_owned(),
        );
        let mut network_dll: serde_json::Value =
            serde_json::from_slice(&valid_single_config()).unwrap();
        network_dll["engineLaunch"]["engineLibraryPath"] =
            serde_json::Value::String(r"\\server\share\engine.dll".to_owned());
        let mut forward_network_dll: serde_json::Value =
            serde_json::from_slice(&valid_single_config()).unwrap();
        forward_network_dll["engineLaunch"]["engineLibraryPath"] =
            serde_json::Value::String("//server/share/engine.dll".to_owned());
        let mut network_worker: serde_json::Value =
            serde_json::from_slice(&valid_single_config()).unwrap();
        network_worker["engineLaunch"]["workerExecutablePath"] =
            serde_json::Value::String(r"\\server\share\worker.exe".to_owned());
        let mut session_zero: serde_json::Value =
            serde_json::from_slice(&valid_single_config()).unwrap();
        session_zero["engineLaunch"]["authorizedWindowsSessionId"] = serde_json::Value::from(0);
        let mut uppercase_extension: serde_json::Value =
            serde_json::from_slice(&valid_single_config()).unwrap();
        uppercase_extension["engineLaunch"]["engineLibraryPath"] =
            serde_json::Value::String(r"C:\CapyIO\engine.DLL".to_owned());
        let mut foreign_ports: serde_json::Value =
            serde_json::from_slice(&valid_single_config()).unwrap();
        foreign_ports["engineLaunch"]["portBase"] = serde_json::Value::from(47_989);
        let mut h264_hdr: serde_json::Value =
            serde_json::from_slice(&valid_single_config()).unwrap();
        h264_hdr["engineLaunch"]["codec"] = serde_json::Value::String("h264".to_owned());
        h264_hdr["engineLaunch"]["dynamicRange"] = serde_json::Value::String("hdr10".to_owned());
        let mut overlapping_engine_port: serde_json::Value =
            serde_json::from_slice(&valid_single_config()).unwrap();
        overlapping_engine_port["engineLaunch"]["port"] = serde_json::Value::from(50_009);
        let mut inexact_refresh: serde_json::Value =
            serde_json::from_slice(&valid_single_config()).unwrap();
        inexact_refresh["route"]["frameRateNumerator"] = serde_json::Value::from(60_000);
        inexact_refresh["route"]["frameRateDenominator"] = serde_json::Value::from(1001);
        for invalid in [
            multi.to_vec(),
            serde_json::to_vec(&wrong_version).unwrap(),
            serde_json::to_vec(&short_timeout).unwrap(),
            serde_json::to_vec(&same_port).unwrap(),
            serde_json::to_vec(&different_address).unwrap(),
            serde_json::to_vec(&long_launch_timeout).unwrap(),
            serde_json::to_vec(&uppercase_hash).unwrap(),
            serde_json::to_vec(&network_dll).unwrap(),
            serde_json::to_vec(&forward_network_dll).unwrap(),
            serde_json::to_vec(&network_worker).unwrap(),
            serde_json::to_vec(&session_zero).unwrap(),
            serde_json::to_vec(&uppercase_extension).unwrap(),
            serde_json::to_vec(&foreign_ports).unwrap(),
            serde_json::to_vec(&h264_hdr).unwrap(),
            serde_json::to_vec(&overlapping_engine_port).unwrap(),
            serde_json::to_vec(&inexact_refresh).unwrap(),
        ] {
            let mut called = false;
            assert!(
                parse_single_display_service_config(&invalid, |_| {
                    called = true;
                    Ok(vec![1])
                })
                .is_err()
            );
            assert!(!called);
        }
    }

    #[test]
    fn single_display_lifecycle_reaches_streaming_and_preserves_disconnect_diagnosis() {
        let mut lifecycle = SingleDisplayLifecycle::new();
        lifecycle.observe_path(Ok(false));
        assert_eq!(
            lifecycle.snapshot.state,
            SingleDisplayState::WaitingForDisplayPath
        );
        assert_eq!(
            lifecycle.snapshot.problem_code.as_deref(),
            Some(PATH_INACTIVE)
        );

        lifecycle.observe_path(Ok(true));
        assert_eq!(
            lifecycle.snapshot.state,
            SingleDisplayState::WaitingForReceiver
        );
        assert_eq!(lifecycle.snapshot.problem_code, None);
        lifecycle.receiver_authenticated().unwrap();
        assert_eq!(lifecycle.snapshot.state, SingleDisplayState::Authenticated);
        assert_eq!(lifecycle.snapshot.generation, 1);

        lifecycle.heartbeat_acknowledged();
        lifecycle.frame_sent();
        assert_eq!(lifecycle.snapshot.state, SingleDisplayState::Streaming);
        assert_eq!(lifecycle.snapshot.frames_sent, 1);
        assert_eq!(lifecycle.snapshot.heartbeat_acknowledgements, 1);

        lifecycle.transport_lost(HEARTBEAT_TIMEOUT);
        lifecycle.observe_path(Ok(true));
        assert_eq!(
            lifecycle.snapshot.state,
            SingleDisplayState::WaitingForReceiver
        );
        assert_eq!(
            lifecycle.snapshot.problem_code.as_deref(),
            Some(HEARTBEAT_TIMEOUT)
        );
        assert!(!lifecycle.snapshot.receiver_connected);
    }

    #[test]
    fn single_display_lifecycle_distinguishes_receiver_from_inactive_path() {
        let mut lifecycle = SingleDisplayLifecycle::new();
        lifecycle.observe_path(Ok(false));
        lifecycle.receiver_authenticated().unwrap();
        assert_eq!(
            lifecycle.snapshot.state,
            SingleDisplayState::AuthenticatedWaitingForDisplayPath
        );
        assert!(lifecycle.snapshot.receiver_connected);

        lifecycle.observe_path(Ok(true));
        assert_eq!(lifecycle.snapshot.state, SingleDisplayState::Authenticated);
        lifecycle.frame_mode_mismatch();
        lifecycle.observe_path(Ok(true));
        assert_eq!(
            lifecycle.snapshot.state,
            SingleDisplayState::AuthenticatedWaitingForSelectedMode
        );
        assert_eq!(lifecycle.snapshot.mode_matches, Some(false));
        assert_eq!(
            lifecycle.snapshot.problem_code.as_deref(),
            Some(MODE_MISMATCH)
        );
        lifecycle.frame_sent();
        assert_eq!(lifecycle.snapshot.mode_matches, Some(true));
        lifecycle.observe_path(Ok(false));
        assert_eq!(
            lifecycle.snapshot.state,
            SingleDisplayState::AuthenticatedWaitingForDisplayPath
        );
        assert_eq!(
            lifecycle.snapshot.problem_code.as_deref(),
            Some(PATH_INACTIVE)
        );
    }

    #[test]
    fn single_display_lifecycle_reports_unavailable_topology_failure_and_stop() {
        let mut lifecycle = SingleDisplayLifecycle::new();
        lifecycle.observe_path(Err(()));
        assert_eq!(lifecycle.snapshot.path_active, None);
        assert_eq!(
            lifecycle.snapshot.problem_code.as_deref(),
            Some(TOPOLOGY_UNAVAILABLE)
        );

        lifecycle.fail(HEARTBEAT_PROTOCOL_VIOLATION);
        assert_eq!(lifecycle.snapshot.state, SingleDisplayState::Failed);
        lifecycle.stop();
        assert_eq!(lifecycle.snapshot.state, SingleDisplayState::Stopped);
        assert_eq!(lifecycle.snapshot.problem_code, None);
    }

    #[test]
    fn single_display_generation_exhaustion_fails_closed() {
        let mut lifecycle = SingleDisplayLifecycle::new();
        lifecycle.snapshot.generation = u64::MAX;
        assert_eq!(lifecycle.receiver_authenticated(), Err(()));
        assert_eq!(lifecycle.snapshot.state, SingleDisplayState::Failed);
        assert_eq!(
            lifecycle.snapshot.problem_code.as_deref(),
            Some(GENERATION_EXHAUSTED)
        );
    }

    #[test]
    fn single_display_transport_error_codes_are_stable() {
        assert_eq!(
            transport_problem(std::io::ErrorKind::TimedOut),
            HEARTBEAT_TIMEOUT
        );
        assert_eq!(
            transport_problem(std::io::ErrorKind::WouldBlock),
            HEARTBEAT_TIMEOUT
        );
        assert_eq!(
            transport_problem(std::io::ErrorKind::ConnectionReset),
            TRANSPORT_LOST
        );
        assert!(display_dimensions_match(1920, 1080, 1920, 1080));
        assert!(!display_dimensions_match(1920, 1080, 1980, 912));
    }

    #[test]
    fn mature_route_random_material_is_nonzero_and_not_reused() {
        let first_request = secure_nonzero_u64().unwrap();
        let second_request = secure_nonzero_u64().unwrap();
        assert_ne!(first_request, 0);
        assert_ne!(second_request, 0);
        assert_ne!(first_request, second_request);

        let first = generate_launch_secrets().unwrap();
        let second = generate_launch_secrets().unwrap();
        assert_ne!(first.launch_session_id, 0);
        assert_ne!(first.key, [0; 16]);
        assert_ne!(first.key, second.key);
    }
}
