use std::{
    env,
    error::Error,
    io,
    net::IpAddr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};

use capyio_core::StreamId;
use capyio_data_plane::{
    BoundedFanout, BoundedJsonlRecorder, ImuSampleV1, NumericImuPanel, RecorderOutcome,
};
use capyio_sensor_server_adapter::{
    AssembleOutcome, SensorKind, SensorServerConnectionConfig, SensorServerEndpoint,
    SensorServerImuAssembler, SensorServerReadOutcome, SensorServerReading,
    SensorServerWebSocketClient,
};

const EVENT_CAPACITY: usize = 256;
const DEFAULT_SAMPLE_COUNT: usize = 32;
const MAX_SAMPLE_COUNT: usize = 10_000;
const MAX_PAIR_SKEW_NANOS: u64 = 1_000_000_000;
const RECEIVE_DEADLINE: Duration = Duration::from_secs(10);

#[derive(Debug)]
enum WorkerEvent {
    Connected(SensorKind),
    Reading(SensorServerReading),
    Failed { kind: SensorKind, detail: String },
}

fn main() -> Result<(), Box<dyn Error>> {
    let (endpoint, sample_count) = parse_arguments()?;
    let config =
        SensorServerConnectionConfig::new(Duration::from_secs(5), Duration::from_millis(500))?;
    let (sender, receiver) = mpsc::sync_channel(EVENT_CAPACITY);
    let running = Arc::new(AtomicBool::new(true));
    let accelerometer = spawn_reader(
        endpoint,
        SensorKind::Accelerometer,
        config,
        sender.clone(),
        Arc::clone(&running),
    );
    match receiver.recv_timeout(RECEIVE_DEADLINE)? {
        WorkerEvent::Connected(SensorKind::Accelerometer) => {}
        WorkerEvent::Failed { kind, detail } => {
            return Err(invalid_input(format!("{kind:?} reader failed: {detail}")).into());
        }
        event => {
            return Err(invalid_input(format!(
                "unexpected event before accelerometer connection: {event:?}"
            ))
            .into());
        }
    }
    let gyroscope = spawn_reader(
        endpoint,
        SensorKind::Gyroscope,
        config,
        sender,
        Arc::clone(&running),
    );

    let stream_id = StreamId::new();
    let mut assembler = SensorServerImuAssembler::new(stream_id, 1, MAX_PAIR_SKEW_NANOS, 0)?;
    let mut fanout = BoundedFanout::new(ImuSampleV1::profile(), stream_id, 1);
    fanout.register_consumer("numeric-panel", sample_count)?;
    fanout.register_consumer("jsonl-recorder", sample_count)?;
    let started = Instant::now();
    let mut emitted = 0usize;
    println!("mode=live_sensorserver_lab");
    println!("endpoint={}", endpoint.address());
    println!("profile=capyio.motion.imu-samples/1");
    while emitted < sample_count {
        let event = receiver.recv_timeout(RECEIVE_DEADLINE).map_err(|error| {
            invalid_input(format!(
                "no SensorServer IMU event arrived within {RECEIVE_DEADLINE:?}: {error}"
            ))
        })?;
        match event {
            WorkerEvent::Connected(_) => {}
            WorkerEvent::Reading(reading) => {
                let receive_timestamp_nanos = u64::try_from(started.elapsed().as_nanos())
                    .unwrap_or(u64::MAX)
                    .saturating_add(1);
                if let AssembleOutcome::Emitted { envelope, .. } =
                    assembler.ingest(reading, receive_timestamp_nanos)?
                {
                    let outcomes = fanout.publish(*envelope);
                    if !outcomes.values().all(Result::is_ok) {
                        return Err(invalid_input("live IMU fan-out rejected an envelope").into());
                    }
                    emitted += 1;
                }
            }
            WorkerEvent::Failed { kind, detail } => {
                return Err(invalid_input(format!("{kind:?} reader failed: {detail}")).into());
            }
        }
    }
    running.store(false, Ordering::Release);
    accelerometer
        .join()
        .map_err(|_| invalid_input("accelerometer reader thread panicked"))?;
    gyroscope
        .join()
        .map_err(|_| invalid_input("gyroscope reader thread panicked"))?;
    let mut panel = NumericImuPanel::default();
    let mut recorder = BoundedJsonlRecorder::new(sample_count, 4096)?;
    while let Some(delivery) = fanout.pop("numeric-panel")? {
        panel.consume(delivery);
    }
    while let Some(delivery) = fanout.pop("jsonl-recorder")? {
        if recorder.record(&delivery)? != RecorderOutcome::Recorded {
            return Err(invalid_input("live IMU recorder rejected a delivery").into());
        }
    }
    let sample = panel
        .last_sample
        .ok_or_else(|| invalid_input("live IMU Panel received no sample"))?;
    println!("panel_received={}", panel.received);
    println!("panel_missing_sequences={}", panel.missing_sequences);
    println!("acceleration_mps2={:?}", sample.acceleration);
    println!("angular_velocity_rads={:?}", sample.angular_velocity);
    println!("recorder_records={}", recorder.len());
    println!("recorder_jsonl_begin");
    print!("{}", recorder.as_jsonl());
    println!("recorder_jsonl_end");
    println!("emitted_samples={emitted}");
    println!("elapsed_millis={}", started.elapsed().as_millis());
    Ok(())
}

fn parse_arguments() -> Result<(SensorServerEndpoint, usize), Box<dyn Error>> {
    let mut arguments = env::args().skip(1);
    let ip: IpAddr = arguments
        .next()
        .ok_or_else(|| invalid_input("usage: sensor-server-live <ip> <port> [sample-count]"))?
        .parse()
        .map_err(|error| invalid_input(format!("invalid IP address: {error}")))?;
    let port: u16 = arguments
        .next()
        .ok_or_else(|| invalid_input("missing SensorServer port"))?
        .parse()
        .map_err(|error| invalid_input(format!("invalid port: {error}")))?;
    let sample_count = arguments
        .next()
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|error| invalid_input(format!("invalid sample count: {error}")))
        })
        .transpose()?
        .unwrap_or(DEFAULT_SAMPLE_COUNT);
    if arguments.next().is_some() {
        return Err(invalid_input("unexpected extra command-line argument").into());
    }
    if !(1..=MAX_SAMPLE_COUNT).contains(&sample_count) {
        return Err(invalid_input(format!(
            "sample count must be within 1..={MAX_SAMPLE_COUNT}"
        ))
        .into());
    }
    Ok((SensorServerEndpoint::new(ip, port)?, sample_count))
}

fn spawn_reader(
    endpoint: SensorServerEndpoint,
    kind: SensorKind,
    config: SensorServerConnectionConfig,
    sender: SyncSender<WorkerEvent>,
    running: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let result = read_sensor(endpoint, kind, config, &sender, &running);
        if let Err(detail) = result {
            let _ = sender.send(WorkerEvent::Failed { kind, detail });
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
    sender
        .send(WorkerEvent::Connected(kind))
        .map_err(|error| error.to_string())?;
    while running.load(Ordering::Acquire) {
        match client.read().map_err(|error| error.to_string())? {
            SensorServerReadOutcome::Reading(reading) => sender
                .send(WorkerEvent::Reading(reading))
                .map_err(|error| error.to_string())?,
            SensorServerReadOutcome::TimedOut | SensorServerReadOutcome::ControlHandled(_) => {}
            SensorServerReadOutcome::Closed { code } => {
                return Err(format!("connection closed with code {code:?}"));
            }
        }
    }
    client.close().map_err(|error| error.to_string())
}

fn invalid_input(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}
