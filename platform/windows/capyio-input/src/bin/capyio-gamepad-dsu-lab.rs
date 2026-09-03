use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    error::Error,
    io,
    net::{IpAddr, Ipv4Addr, SocketAddrV4, UdpSocket},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use capyio_core::{
    AdapterDeploymentMode, AdapterHealth, AdapterInstanceDescriptor, AdapterState, Availability,
    CapabilityClass, CapabilityDescriptor, FormatDescriptor, InteroperabilityMode, NodeDescriptor,
    NodeId, PermissionRequirement, Platform, PortDescriptor, PortDirection, PortRef, ProfileId,
    ProtocolVersion, QosMode, RouteBackend, RouteId, RouteState, SessionId,
};
use capyio_dsu_adapter::{
    AxisPermutation, DSU_CONVENTIONAL_PORT, DsuImuWorkerConfig, DsuLoopbackConfig,
    DsuMotionMapping, DsuSubmitOutcome, SignedSourceAxis, SourceAxis,
};
use capyio_runtime::NodeRuntime;
use capyio_sensor_server_adapter::{
    AssembleOutcome, SensorKind, SensorServerConnectionConfig, SensorServerEndpoint,
    SensorServerImuAssembler, SensorServerReadOutcome, SensorServerReading,
    SensorServerWebSocketClient,
};
use capyio_windows_input::DsuImuRouteController;

const EVENT_CAPACITY: usize = 256;
const DEFAULT_SAMPLE_COUNT: usize = 3_000;
const MAX_SAMPLE_COUNT: usize = 100_000;
const MAX_PAIR_SKEW_NANOS: u64 = 1_000_000_000;
const EVENT_DEADLINE: Duration = Duration::from_secs(10);
const DRAIN_DEADLINE: Duration = Duration::from_secs(5);
const SOURCE_ADAPTER_TYPE: &str = "dev.capyio.sensorserver.dsu-lab";
const IMU_FORMAT: &str = "imu-si-f32-le";

#[derive(Clone, Debug, Eq, PartialEq)]
enum Command {
    Help,
    Preflight { dsu_port: u16 },
    Run(LabConfig),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LabConfig {
    phone_ip: IpAddr,
    sensor_port: u16,
    sample_count: usize,
    dsu_port: u16,
    axis_mapping: String,
}

#[derive(Debug)]
enum WorkerEvent {
    Connected(SensorKind),
    Reading(SensorServerReading),
    Failed { kind: SensorKind, detail: String },
}

fn main() -> Result<(), Box<dyn Error>> {
    match parse_command(env::args().skip(1))? {
        Command::Help => {
            print_usage();
            Ok(())
        }
        Command::Preflight { dsu_port } => run_preflight(dsu_port),
        Command::Run(config) => run_lab(config),
    }
}

fn run_preflight(dsu_port: u16) -> Result<(), Box<dyn Error>> {
    let socket =
        UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, dsu_port)).map_err(|error| {
            invalid_input(format!(
                "DSU loopback port {dsu_port} is unavailable: {error}"
            ))
        })?;
    drop(socket);
    println!("mode=dsu_lab_preflight");
    println!("projection=DSU_v1001");
    println!("driver_required=false");
    println!("bind_scope=ipv4_loopback_only");
    println!("dsu_port={dsu_port}");
    println!("dsu_port_available=true");
    println!("preflight_result=pass");
    Ok(())
}

fn run_lab(config: LabConfig) -> Result<(), Box<dyn Error>> {
    let mapping = parse_motion_mapping(&config.axis_mapping)?;
    let endpoint = SensorServerEndpoint::new(config.phone_ip, config.sensor_port)?;
    let (mut runtime, session_id, source) = build_runtime()?;
    let route_id = RouteId::new();
    let worker_config = DsuImuWorkerConfig {
        queue_capacity: EVENT_CAPACITY,
        motion_mapping: mapping,
        ..DsuImuWorkerConfig::new(DsuLoopbackConfig::local_lab(config.dsu_port, 0x4341_5059))
    };
    let mut controller =
        DsuImuRouteController::install(&mut runtime, session_id, route_id, source, worker_config)?;
    let epoch = controller.begin_start(&mut runtime, monotonic_millis())?;
    let connection_config =
        SensorServerConnectionConfig::new(Duration::from_secs(5), Duration::from_millis(500))?;
    let (sender, receiver) = mpsc::sync_channel(EVENT_CAPACITY);
    let running = Arc::new(AtomicBool::new(true));
    let readers = [
        spawn_reader(
            endpoint,
            SensorKind::Accelerometer,
            connection_config,
            sender.clone(),
            Arc::clone(&running),
        ),
        spawn_reader(
            endpoint,
            SensorKind::Gyroscope,
            connection_config,
            sender,
            Arc::clone(&running),
        ),
    ];

    println!("mode=live_sensorserver_to_dsu_lab");
    println!("profile=capyio.motion.imu-samples/1");
    println!("route_backend=ExternalProtocol");
    println!("route_epoch={epoch}");
    println!("sensor_endpoint_configured=true");
    println!("sensor_port={}", config.sensor_port);
    println!("axis_mapping={}", config.axis_mapping);
    println!("requested_samples={}", config.sample_count);

    let started = Instant::now();
    let stream_result = stream_to_dsu(
        &config,
        epoch,
        &receiver,
        &running,
        &mut controller,
        &mut runtime,
        started,
    );
    running.store(false, Ordering::Release);
    let reader_cleanup = join_readers(readers);

    let result = match stream_result {
        Ok(submitted) => finish_success(
            submitted,
            &mut controller,
            &mut runtime,
            started,
            reader_cleanup,
        ),
        Err(error) => finish_failure(error, &mut controller, &mut runtime, reader_cleanup),
    };
    result.map_err(|error| invalid_input(error).into())
}

#[allow(clippy::too_many_arguments)]
fn stream_to_dsu(
    config: &LabConfig,
    epoch: u64,
    receiver: &mpsc::Receiver<WorkerEvent>,
    running: &AtomicBool,
    controller: &mut DsuImuRouteController,
    runtime: &mut NodeRuntime,
    started: Instant,
) -> Result<usize, String> {
    let stream_id = capyio_core::StreamId::new();
    let mut assembler = SensorServerImuAssembler::new(stream_id, epoch, MAX_PAIR_SKEW_NANOS, 0)
        .map_err(|error| error.to_string())?;
    let mut accelerometer_connected = false;
    let mut gyroscope_connected = false;
    let mut activated = false;
    let mut submitted = 0usize;
    let mut skew_exceeded = 0u64;
    while submitted < config.sample_count {
        let event = receiver.recv_timeout(EVENT_DEADLINE).map_err(|error| {
            if running.load(Ordering::Acquire) {
                format!("no SensorServer IMU event arrived within {EVENT_DEADLINE:?}: {error}")
            } else {
                format!("SensorServer reader stopped before the lab completed: {error}")
            }
        })?;
        match event {
            WorkerEvent::Connected(SensorKind::Accelerometer) => {
                accelerometer_connected = true;
                println!("accelerometer_connected=true");
            }
            WorkerEvent::Connected(SensorKind::Gyroscope) => {
                gyroscope_connected = true;
                println!("gyroscope_connected=true");
            }
            WorkerEvent::Connected(SensorKind::MagneticField) => {
                return Err("unexpected magnetic-field reader connection".to_owned());
            }
            WorkerEvent::Reading(reading) => {
                let receive_timestamp_nanos = u64::try_from(started.elapsed().as_nanos())
                    .unwrap_or(u64::MAX)
                    .saturating_add(1);
                match assembler
                    .ingest(reading, receive_timestamp_nanos)
                    .map_err(|error| error.to_string())?
                {
                    AssembleOutcome::Emitted { envelope, .. } => {
                        if !accelerometer_connected || !gyroscope_connected {
                            return Err(
                                "IMU envelope arrived before both readers confirmed connection"
                                    .to_owned(),
                            );
                        }
                        if !activated {
                            controller.activate(runtime, &envelope)?;
                            let address = controller
                                .status(runtime)?
                                .local_address
                                .ok_or_else(|| "Active DSU Route has no endpoint".to_owned())?;
                            println!("route_state=active");
                            println!("dsu_endpoint={address}");
                            activated = true;
                        }
                        match controller.submit(runtime, *envelope)? {
                            DsuSubmitOutcome::Submitted => submitted += 1,
                            DsuSubmitOutcome::QueueFull => {
                                return Err(format!(
                                    "DSU input queue filled after {submitted} submitted samples"
                                ));
                            }
                            DsuSubmitOutcome::Stopped => {
                                return Err("DSU Worker stopped while submitting IMU".to_owned());
                            }
                        }
                        if submitted.is_multiple_of(250) {
                            let stats = controller.poll_health(runtime)?;
                            println!("progress_samples={submitted}");
                            println!("progress_motion_packets={}", stats.motion_packets_sent);
                        }
                    }
                    AssembleOutcome::SkewExceeded { .. } => {
                        skew_exceeded = skew_exceeded.saturating_add(1);
                    }
                    AssembleOutcome::AwaitingCounterpart { .. }
                    | AssembleOutcome::MagneticFieldUpdated => {}
                }
            }
            WorkerEvent::Failed { kind, detail } => {
                return Err(format!("{kind:?} reader failed: {detail}"));
            }
        }
    }
    println!("pair_skew_exceeded={skew_exceeded}");
    Ok(submitted)
}

fn finish_success(
    submitted: usize,
    controller: &mut DsuImuRouteController,
    runtime: &mut NodeRuntime,
    started: Instant,
    reader_cleanup: Option<String>,
) -> Result<(), String> {
    if let Some(error) = reader_cleanup {
        let route_cleanup = controller.stop(runtime).err();
        println!("lab_result=fail");
        return Err(combine_cleanup(error, route_cleanup));
    }
    let deadline = Instant::now() + DRAIN_DEADLINE;
    let stats = loop {
        let stats = controller.poll_health(runtime)?;
        if stats.samples_accepted >= u64::try_from(submitted).unwrap_or(u64::MAX) {
            break stats;
        }
        if Instant::now() >= deadline {
            let cleanup = controller.stop(runtime).err();
            return Err(combine_cleanup(
                format!(
                    "DSU Worker accepted only {} of {submitted} submitted samples",
                    stats.samples_accepted
                ),
                cleanup,
            ));
        }
        thread::yield_now();
    };
    let mut failure = None;
    if stats.subscriptions_added == 0 || stats.motion_packets_sent == 0 {
        failure = Some(
            "no DSU emulator subscription/motion delivery was observed; configure Cemu or Dolphin for 127.0.0.1 and the printed DSU port before the run"
                .to_owned(),
        );
    } else if stats.queue_full != 0
        || stats.invalid_envelopes != 0
        || stats.projection_errors != 0
        || stats.transport_failures != 0
        || stats.input_gaps != 0
        || stats.missing_sequences != 0
        || stats.late_samples != 0
        || stats.wrong_stream_samples != 0
        || stats.stale_epoch_samples != 0
        || stats.future_epoch_samples != 0
        || stats.motion_packets_would_block != 0
        || stats.motion_packet_send_errors != 0
    {
        failure = Some("DSU evidence counters contain a loss or contract failure".to_owned());
    }
    println!("submitted_samples={submitted}");
    println!("accepted_samples={}", stats.samples_accepted);
    println!("queue_full={}", stats.queue_full);
    println!("input_gaps={}", stats.input_gaps);
    println!("invalid_envelopes={}", stats.invalid_envelopes);
    println!("projection_errors={}", stats.projection_errors);
    println!("subscriptions_added={}", stats.subscriptions_added);
    println!("subscriptions_renewed={}", stats.subscriptions_renewed);
    println!("motion_packets_sent={}", stats.motion_packets_sent);
    println!("transport_failures={}", stats.transport_failures);
    println!("elapsed_millis={}", started.elapsed().as_millis());
    let cleanup = controller.stop(runtime).err();
    let state_after_cleanup = controller.status(runtime).map(|status| status.route_state);
    match &state_after_cleanup {
        Ok(state) => println!(
            "route_state_after_cleanup={}",
            format!("{state:?}").to_ascii_lowercase()
        ),
        Err(error) => println!("route_state_after_cleanup=unknown:{error}"),
    }
    if let Some(failure) = failure {
        println!("lab_result=fail");
        return Err(combine_cleanup(failure, cleanup));
    }
    if let Some(cleanup) = cleanup {
        println!("lab_result=fail");
        return Err(format!("DSU Route cleanup failed: {cleanup}"));
    }
    if state_after_cleanup? != RouteState::Stopped {
        println!("lab_result=fail");
        return Err("DSU Route did not reach Stopped after cleanup".to_owned());
    }
    println!("lab_result=pass");
    Ok(())
}

fn finish_failure(
    error: String,
    controller: &mut DsuImuRouteController,
    runtime: &mut NodeRuntime,
    reader_cleanup: Option<String>,
) -> Result<(), String> {
    let state = controller.status(runtime)?.route_state;
    let offline = matches!(state, RouteState::Starting | RouteState::Active)
        .then(|| {
            controller
                .report_upstream_offline(runtime, error.clone())
                .err()
        })
        .flatten();
    let stop = controller.stop(runtime).err();
    let mut combined = error;
    for (label, detail) in [
        ("reader cleanup", reader_cleanup),
        ("Offline transition", offline),
        ("Route cleanup", stop),
    ] {
        if let Some(detail) = detail {
            combined.push_str(&format!("; {label} failed: {detail}"));
        }
    }
    println!("lab_result=fail");
    Err(combined)
}

fn spawn_reader(
    endpoint: SensorServerEndpoint,
    kind: SensorKind,
    config: SensorServerConnectionConfig,
    sender: SyncSender<WorkerEvent>,
    running: Arc<AtomicBool>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        if let Err(detail) = read_sensor(endpoint, kind, config, &sender, &running) {
            running.store(false, Ordering::Release);
            let _ = sender.try_send(WorkerEvent::Failed { kind, detail });
        }
    })
}

fn read_sensor(
    endpoint: SensorServerEndpoint,
    kind: SensorKind,
    config: SensorServerConnectionConfig,
    sender: &SyncSender<WorkerEvent>,
    running: &AtomicBool,
) -> Result<(), String> {
    let mut client = SensorServerWebSocketClient::connect(endpoint, kind, config)
        .map_err(|error| error.to_string())?;
    send_event(sender, WorkerEvent::Connected(kind))?;
    while running.load(Ordering::Acquire) {
        match client.read().map_err(|error| error.to_string())? {
            SensorServerReadOutcome::Reading(reading) => {
                send_event(sender, WorkerEvent::Reading(reading))?;
            }
            SensorServerReadOutcome::TimedOut | SensorServerReadOutcome::ControlHandled(_) => {}
            SensorServerReadOutcome::Closed { code } => {
                return Err(format!("connection closed with code {code:?}"));
            }
        }
    }
    client.close().map_err(|error| error.to_string())
}

fn send_event(sender: &SyncSender<WorkerEvent>, event: WorkerEvent) -> Result<(), String> {
    sender.try_send(event).map_err(|error| match error {
        TrySendError::Full(_) => "bounded SensorServer event queue is full".to_owned(),
        TrySendError::Disconnected(_) => "SensorServer event consumer stopped".to_owned(),
    })
}

fn join_readers(readers: [JoinHandle<()>; 2]) -> Option<String> {
    let failures = readers
        .into_iter()
        .filter_map(|reader| {
            reader
                .join()
                .err()
                .map(|_| "SensorServer reader thread panicked")
        })
        .collect::<Vec<_>>();
    (!failures.is_empty()).then(|| failures.join("; "))
}

fn build_runtime() -> Result<(NodeRuntime, SessionId, PortRef), String> {
    let local_node_id = NodeId::new();
    let remote_node_id = NodeId::new();
    let source_adapter_id = capyio_core::AdapterInstanceId::new();
    let capability_id = capyio_core::CapabilityId::new();
    let port_id = capyio_core::PortId::new();
    let local = NodeDescriptor::new(
        local_node_id,
        "CapyIO DSU Lab Host",
        Platform::Windows,
        "local-lab",
        env!("CARGO_PKG_VERSION"),
        [ProtocolVersion::new(1, 0)],
    );
    let mut remote = NodeDescriptor::new(
        remote_node_id,
        "SensorServer Lab Source",
        Platform::Android,
        "external-service",
        env!("CARGO_PKG_VERSION"),
        [ProtocolVersion::new(1, 0)],
    );
    remote
        .add_adapter(AdapterInstanceDescriptor {
            id: source_adapter_id,
            adapter_type: SOURCE_ADAPTER_TYPE.to_owned(),
            display_name: "SensorServer DSU Lab Source".to_owned(),
            deployment_mode: AdapterDeploymentMode::ExternalService,
            version: env!("CARGO_PKG_VERSION").to_owned(),
            state: AdapterState::Ready,
            health: AdapterHealth::Healthy,
            owned_capabilities: BTreeSet::new(),
            supported_route_modes: BTreeSet::from([RouteBackend::ExternalProtocol]),
        })
        .map_err(|error| error.to_string())?;
    remote
        .add_capability(CapabilityDescriptor {
            id: capability_id,
            adapter_instance_id: source_adapter_id,
            display_name: "Phone IMU".to_owned(),
            class: CapabilityClass::Imu,
            availability: Availability::Available,
            permission_requirement: PermissionRequirement::None,
            metadata: BTreeMap::new(),
            ports: BTreeMap::from([(
                port_id,
                PortDescriptor {
                    id: port_id,
                    capability_id,
                    display_name: "SensorServer IMU Source".to_owned(),
                    direction: PortDirection::Source,
                    profile: ProfileId::imu_samples_v1(),
                    schema_id: None,
                    formats: vec![FormatDescriptor::new(IMU_FORMAT)],
                    qos_modes: BTreeSet::from([QosMode::Measurement]),
                    clock_domain: Some("android.sensor.elapsed_realtime".to_owned()),
                    availability: Availability::Available,
                    permission_requirement: PermissionRequirement::None,
                    interoperability_mode: InteroperabilityMode::StandardPort,
                },
            )]),
        })
        .map_err(|error| error.to_string())?;
    let mut runtime = NodeRuntime::new(local).map_err(|error| error.to_string())?;
    runtime
        .register_peer(remote, true)
        .map_err(|error| error.to_string())?;
    let session_id = runtime
        .open_session(remote_node_id)
        .map_err(|error| error.to_string())?;
    Ok((
        runtime,
        session_id,
        PortRef {
            node_id: remote_node_id,
            capability_id,
            port_id,
        },
    ))
}

fn parse_command(arguments: impl IntoIterator<Item = String>) -> Result<Command, io::Error> {
    let mut arguments = arguments.into_iter();
    let Some(command) = arguments.next() else {
        return Ok(Command::Help);
    };
    match command.as_str() {
        "help" | "--help" | "-h" => {
            require_end(arguments)?;
            Ok(Command::Help)
        }
        "preflight" => {
            let dsu_port = parse_optional(&mut arguments, DSU_CONVENTIONAL_PORT, "DSU port")?;
            require_end(arguments)?;
            require_nonzero(dsu_port, "DSU port")?;
            Ok(Command::Preflight { dsu_port })
        }
        "run" => {
            let phone_ip = arguments
                .next()
                .ok_or_else(|| invalid_input("missing phone IP"))?
                .parse()
                .map_err(|error| invalid_input(format!("invalid phone IP: {error}")))?;
            let sensor_port = parse_required(&mut arguments, "SensorServer port")?;
            let sample_count =
                parse_optional(&mut arguments, DEFAULT_SAMPLE_COUNT, "sample count")?;
            let dsu_port = parse_optional(&mut arguments, DSU_CONVENTIONAL_PORT, "DSU port")?;
            let axis_mapping = arguments
                .next()
                .unwrap_or_else(|| "+x,+y,+z;+x,+y,+z".to_owned());
            require_end(arguments)?;
            require_nonzero(sensor_port, "SensorServer port")?;
            require_nonzero(dsu_port, "DSU port")?;
            if !(1..=MAX_SAMPLE_COUNT).contains(&sample_count) {
                return Err(invalid_input(format!(
                    "sample count must be within 1..={MAX_SAMPLE_COUNT}"
                )));
            }
            parse_motion_mapping(&axis_mapping)?;
            Ok(Command::Run(LabConfig {
                phone_ip,
                sensor_port,
                sample_count,
                dsu_port,
                axis_mapping,
            }))
        }
        value => Err(invalid_input(format!(
            "unknown command {value:?}; expected run, preflight or help"
        ))),
    }
}

fn parse_motion_mapping(value: &str) -> Result<DsuMotionMapping, io::Error> {
    let (acceleration, angular_velocity) = value
        .split_once(';')
        .ok_or_else(|| invalid_input("axis mapping must be ACCEL_XYZ;GYRO_PITCH_YAW_ROLL"))?;
    Ok(DsuMotionMapping::new(
        parse_permutation(acceleration)?,
        parse_permutation(angular_velocity)?,
    ))
}

fn parse_permutation(value: &str) -> Result<AxisPermutation, io::Error> {
    let axes = value
        .split(',')
        .map(parse_axis)
        .collect::<Result<Vec<_>, _>>()?;
    if axes.len() != 3 {
        return Err(invalid_input(
            "each axis mapping must contain exactly three comma-separated signed axes",
        ));
    }
    AxisPermutation::new(axes[0], axes[1], axes[2])
        .map_err(|error| invalid_input(error.to_string()))
}

fn parse_axis(value: &str) -> Result<SignedSourceAxis, io::Error> {
    let (negative, axis) = match value.trim().to_ascii_lowercase().as_str() {
        "+x" | "x" => (false, SourceAxis::X),
        "-x" => (true, SourceAxis::X),
        "+y" | "y" => (false, SourceAxis::Y),
        "-y" => (true, SourceAxis::Y),
        "+z" | "z" => (false, SourceAxis::Z),
        "-z" => (true, SourceAxis::Z),
        value => {
            return Err(invalid_input(format!(
                "invalid signed source axis {value:?}"
            )));
        }
    };
    Ok(if negative {
        SignedSourceAxis::negative(axis)
    } else {
        SignedSourceAxis::positive(axis)
    })
}

fn parse_required<T: std::str::FromStr>(
    arguments: &mut impl Iterator<Item = String>,
    label: &str,
) -> Result<T, io::Error>
where
    T::Err: std::fmt::Display,
{
    arguments
        .next()
        .ok_or_else(|| invalid_input(format!("missing {label}")))?
        .parse()
        .map_err(|error| invalid_input(format!("invalid {label}: {error}")))
}

fn parse_optional<T: std::str::FromStr>(
    arguments: &mut impl Iterator<Item = String>,
    default: T,
    label: &str,
) -> Result<T, io::Error>
where
    T::Err: std::fmt::Display,
{
    arguments.next().map_or(Ok(default), |value| {
        value
            .parse()
            .map_err(|error| invalid_input(format!("invalid {label}: {error}")))
    })
}

fn require_nonzero(value: u16, label: &str) -> Result<(), io::Error> {
    if value == 0 {
        Err(invalid_input(format!("{label} must be non-zero")))
    } else {
        Ok(())
    }
}

fn require_end(mut arguments: impl Iterator<Item = String>) -> Result<(), io::Error> {
    if let Some(extra) = arguments.next() {
        Err(invalid_input(format!(
            "unexpected extra command-line argument {extra:?}"
        )))
    } else {
        Ok(())
    }
}

fn print_usage() {
    println!(
        "usage:\n  capyio-gamepad-dsu-lab preflight [dsu-port]\n  capyio-gamepad-dsu-lab run <phone-ip> <sensor-port> [sample-count] [dsu-port] [accel-map;gyro-map]\n\nexample mapping: +x,+y,+z;+x,+y,+z\nnegative/permuted example: +y,-x,+z;+y,-x,+z"
    );
}

fn monotonic_millis() -> u64 {
    1
}

fn combine_cleanup(primary: String, cleanup: Option<String>) -> String {
    cleanup.map_or(primary.clone(), |cleanup| {
        format!("{primary}; cleanup failed: {cleanup}")
    })
}

fn invalid_input(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        net::{SocketAddr, TcpListener, TcpStream},
        sync::Barrier,
    };
    use tungstenite::{
        Message, accept_hdr,
        handshake::server::{Request, Response},
    };

    fn args<'a>(values: &'a [&'a str]) -> impl Iterator<Item = String> + 'a {
        values.iter().map(|value| (*value).to_owned())
    }

    #[test]
    fn parser_has_closed_commands_and_bounded_run_configuration() {
        assert_eq!(parse_command(args(&[])).unwrap(), Command::Help);
        assert_eq!(
            parse_command(args(&["preflight"])).unwrap(),
            Command::Preflight {
                dsu_port: DSU_CONVENTIONAL_PORT
            }
        );
        assert_eq!(
            parse_command(args(&[
                "run",
                "192.0.2.1",
                "8080",
                "42",
                "26761",
                "+y,-x,+z;-z,+y,+x",
            ]))
            .unwrap(),
            Command::Run(LabConfig {
                phone_ip: "192.0.2.1".parse().unwrap(),
                sensor_port: 8080,
                sample_count: 42,
                dsu_port: 26761,
                axis_mapping: "+y,-x,+z;-z,+y,+x".to_owned(),
            })
        );
        for invalid in [
            vec!["unknown"],
            vec!["preflight", "0"],
            vec!["run", "bad", "8080"],
            vec!["run", "192.0.2.1", "0"],
            vec!["run", "192.0.2.1", "8080", "0"],
            vec!["run", "192.0.2.1", "8080", "1", "0"],
            vec![
                "run",
                "192.0.2.1",
                "8080",
                "1",
                "26760",
                "+x,+x,+z;+x,+y,+z",
            ],
        ] {
            assert!(
                parse_command(args(&invalid)).is_err(),
                "accepted {invalid:?}"
            );
        }
    }

    #[test]
    fn preflight_releases_the_exact_requested_port() {
        let reservation = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = reservation.local_addr().unwrap().port();
        drop(reservation);
        run_preflight(port).unwrap();
        let rebound = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port))
            .expect("preflight must release the checked UDP port");
        drop(rebound);
    }

    #[test]
    fn fixture_sensorserver_reaches_a_real_dsu_loopback_subscriber() {
        let server = FakeSensorServer::start();
        let dsu_port = reserve_udp_port();
        let (subscriber_stop, subscriber) = spawn_dsu_subscriber(dsu_port);
        let result = run_lab(LabConfig {
            phone_ip: server.address.ip(),
            sensor_port: server.address.port(),
            sample_count: 16,
            dsu_port,
            axis_mapping: "+x,+y,+z;+x,+y,+z".to_owned(),
        });
        subscriber_stop.store(true, Ordering::Release);
        assert!(result.is_ok(), "fixture lab failed: {result:?}");
        assert!(
            subscriber.join().unwrap(),
            "DSU subscriber saw no motion packet"
        );
        server.finish();
    }

    struct FakeSensorServer {
        address: SocketAddr,
        handle: JoinHandle<()>,
    }

    impl FakeSensorServer {
        fn start() -> Self {
            let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
            let address = listener.local_addr().unwrap();
            let handle = thread::spawn(move || {
                let barrier = Arc::new(Barrier::new(2));
                let mut connections = Vec::new();
                for _ in 0..2 {
                    let (stream, _) = listener.accept().unwrap();
                    let barrier = Arc::clone(&barrier);
                    connections.push(thread::spawn(move || serve_sensor(stream, barrier)));
                }
                for connection in connections {
                    connection.join().unwrap();
                }
            });
            Self { address, handle }
        }

        fn finish(self) {
            self.handle.join().unwrap();
        }
    }

    #[allow(clippy::result_large_err)]
    fn serve_sensor(stream: TcpStream, barrier: Arc<Barrier>) {
        stream
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        let mut sensor_kind = None;
        let mut socket = accept_hdr(stream, |request: &Request, response: Response| {
            sensor_kind = Some(
                if request
                    .uri()
                    .query()
                    .is_some_and(|query| query.contains("android.sensor.accelerometer"))
                {
                    SensorKind::Accelerometer
                } else {
                    SensorKind::Gyroscope
                },
            );
            Ok(response)
        })
        .unwrap();
        let sensor_kind = sensor_kind.unwrap();
        barrier.wait();
        for sequence in 0..64_u64 {
            let timestamp = 1_000_000_000 + sequence * 10_000_000;
            let values = match sensor_kind {
                SensorKind::Accelerometer => [0.25, 9.7, -0.5],
                SensorKind::Gyroscope => [0.01, -0.02, 0.03],
                SensorKind::MagneticField => unreachable!(),
            };
            let message = format!(
                "{{\"accuracy\":3,\"timestamp\":{timestamp},\"values\":[{},{},{}]}}",
                values[0], values[1], values[2]
            );
            if socket.send(Message::Text(message.into())).is_err() {
                return;
            }
            thread::sleep(Duration::from_millis(1));
        }
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            match socket.read() {
                Ok(Message::Close(_)) => return,
                Ok(_) => {}
                Err(tungstenite::Error::Io(error))
                    if matches!(
                        error.kind(),
                        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                    ) => {}
                Err(tungstenite::Error::ConnectionClosed) => return,
                Err(_) => return,
            }
        }
    }

    fn reserve_udp_port() -> u16 {
        let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)).unwrap();
        socket.local_addr().unwrap().port()
    }

    fn spawn_dsu_subscriber(port: u16) -> (Arc<AtomicBool>, JoinHandle<bool>) {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let handle = thread::spawn(move || {
            let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)).unwrap();
            socket.set_nonblocking(true).unwrap();
            let address = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
            let request = dsu_pad_request(0x1020_3040);
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut packet = [0_u8; 100];
            let mut observed = false;
            while Instant::now() < deadline && !thread_stop.load(Ordering::Acquire) {
                let _ = socket.send_to(&request, address);
                match socket.recv_from(&mut packet) {
                    Ok((100, source))
                        if source.ip().is_loopback()
                            && &packet[..4] == b"DSUS"
                            && u32::from_le_bytes(packet[16..20].try_into().unwrap())
                                == 0x10_0002 =>
                    {
                        observed = true;
                    }
                    Ok(_) | Err(_) => {}
                }
                thread::sleep(Duration::from_millis(1));
            }
            observed
        });
        (stop, handle)
    }

    fn dsu_pad_request(client_id: u32) -> Vec<u8> {
        let mut packet = vec![0_u8; 28];
        packet[..4].copy_from_slice(b"DSUC");
        packet[4..6].copy_from_slice(&capyio_dsu_adapter::DSU_PROTOCOL_VERSION.to_le_bytes());
        packet[6..8].copy_from_slice(&12_u16.to_le_bytes());
        packet[12..16].copy_from_slice(&client_id.to_le_bytes());
        packet[16..20].copy_from_slice(&0x10_0002_u32.to_le_bytes());
        packet[20] = 1;
        packet[21] = 0;
        let checksum = capyio_dsu_adapter::crc32_ieee(&packet);
        packet[8..12].copy_from_slice(&checksum.to_le_bytes());
        packet
    }
}
