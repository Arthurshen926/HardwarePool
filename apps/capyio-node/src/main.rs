use anyhow::Context;
use capyio_audio::{AudioFormat, AudioFrame, ClockDriftEstimator, InsertOutcome, ReorderBuffer};
use capyio_core::StreamId;
use capyio_data_plane::{
    BoundedFanout, BoundedJsonlRecorder, ImuSampleV1, NumericImuPanel, RecorderOutcome,
    parse_imu_fixture_jsonl,
};
use capyio_protocol::{decode_envelope, encode_envelope, new_envelope, v1};
use capyio_testkit::{DemoLab, android_node};
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(name = "capyio-node")]
#[command(about = "CapyIO bootstrap headless node and deterministic demo")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Runs four independent mock Routes across audio, motion and video Profiles.
    Demo,
    /// Prints the initial deterministic Runtime snapshot as JSON.
    Snapshot,
    /// Converts the sample Android node through Protobuf and binary Envelope encoding.
    ProtocolRoundtrip,
    /// Exercises frame validation, out-of-order delivery and clock estimation without hardware.
    AudioFrameDemo,
    /// Replays the bounded deterministic IMU fixture into independent Panel and Recorder sinks.
    ImuFixtureDemo,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .compact()
        .init();

    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Demo) {
        Command::Demo => run_demo(),
        Command::Snapshot => print_snapshot(),
        Command::ProtocolRoundtrip => protocol_roundtrip(),
        Command::AudioFrameDemo => audio_frame_demo(),
        Command::ImuFixtureDemo => imu_fixture_demo(),
    }
}

fn run_demo() -> anyhow::Result<()> {
    let mut lab = DemoLab::new().context("create deterministic lab")?;
    for (index, route_id) in lab.routes.all().into_iter().enumerate() {
        tracing::info!(%route_id, "activating deterministic Route");
        lab.set_route_active(route_id, true, 1_000 + index as u64)?;
    }
    tracing::info!(
        route_id = %lab.routes.phone_microphone_to_windows,
        "stopping one Route while leaving the other three active"
    );
    lab.set_route_active(lab.routes.phone_microphone_to_windows, false, 2_000)?;

    println!("{}", serde_json::to_string_pretty(&lab.runtime.snapshot())?);
    Ok(())
}

fn print_snapshot() -> anyhow::Result<()> {
    let lab = DemoLab::new().context("create deterministic lab")?;
    println!("{}", serde_json::to_string_pretty(&lab.runtime.snapshot())?);
    Ok(())
}

fn protocol_roundtrip() -> anyhow::Result<()> {
    let node = android_node();
    let hello = v1::Hello {
        node: Some(v1::NodeDescriptor::try_from(&node)?),
        supported_protocol_majors: vec![capyio_protocol::PROTOCOL_MAJOR],
    };
    let envelope = new_envelope(None, v1::envelope::Payload::Hello(hello));
    let encoded = encode_envelope(&envelope);
    let decoded = decode_envelope(&encoded)?;

    println!("encoded_bytes={}", encoded.len());
    println!("message_id={}", decoded.message_id);
    println!(
        "protocol={}.{}",
        decoded.protocol_major, decoded.protocol_minor
    );
    Ok(())
}

fn audio_frame_demo() -> anyhow::Result<()> {
    let format = AudioFormat::microphone_baseline();
    let stream_id = StreamId::new();
    let make_frame = |sequence: u64| AudioFrame {
        stream_id,
        stream_epoch: 1,
        sequence,
        source_timestamp_micros: sequence * 10_000,
        first_sample_index: sequence * 480,
        sample_count: 480,
        discontinuity: false,
        payload: vec![0; 960],
    };

    let later = make_frame(1);
    let first = make_frame(0);
    later.validate(&format)?;
    first.validate(&format)?;

    let mut buffer = ReorderBuffer::new(stream_id, 1, 0, 8)?;
    anyhow::ensure!(buffer.insert(later) == InsertOutcome::Accepted);
    anyhow::ensure!(buffer.insert(first) == InsertOutcome::Accepted);
    let emitted = [
        buffer
            .pop_next()
            .context("expected sequence zero")?
            .sequence,
        buffer.pop_next().context("expected sequence one")?.sequence,
    ];

    let mut drift = ClockDriftEstimator::new(format.sample_rate_hz);
    let _origin = drift.observe(0, 0);
    let estimate = drift
        .observe(48_005, 1_000_000)
        .context("expected drift estimate")?;

    println!("emitted_sequences={emitted:?}");
    println!("buffer_stats={:?}", buffer.stats());
    println!("observed_rate_hz={:.3}", estimate.observed_source_rate_hz);
    println!("drift_ppm={:.3}", estimate.drift_ppm);
    Ok(())
}

fn imu_fixture_demo() -> anyhow::Result<()> {
    const FIXTURE: &str = include_str!("../../../fixtures/imu/imu_samples_v1.jsonl");
    let envelopes = parse_imu_fixture_jsonl(FIXTURE, 64)?;
    let first = envelopes.first().context("IMU fixture is empty")?;
    let mut fanout =
        BoundedFanout::new(ImuSampleV1::profile(), first.stream_id, first.stream_epoch);
    fanout.register_consumer("numeric-panel", 64)?;
    fanout.register_consumer("jsonl-recorder", 64)?;
    for envelope in envelopes.iter().cloned() {
        let outcomes = fanout.publish(envelope);
        anyhow::ensure!(outcomes.values().all(|outcome| outcome.is_ok()));
    }

    let mut panel = NumericImuPanel::default();
    let mut recorder = BoundedJsonlRecorder::new(64, 4096)?;
    while let Some(delivery) = fanout.pop("numeric-panel")? {
        panel.consume(delivery);
    }
    while let Some(delivery) = fanout.pop("jsonl-recorder")? {
        anyhow::ensure!(recorder.record(&delivery)? == RecorderOutcome::Recorded);
    }
    let sample = panel
        .last_sample
        .context("numeric Panel received no sample")?;
    println!("mode=deterministic_fixture (not live phone data)");
    println!("profile=capyio.motion.imu-samples/1");
    println!("panel_received={}", panel.received);
    println!("panel_missing_sequences={}", panel.missing_sequences);
    println!("acceleration_mps2={:?}", sample.acceleration);
    println!("angular_velocity_rads={:?}", sample.angular_velocity);
    println!("recorder_records={}", recorder.len());
    println!("recorder_jsonl_begin");
    print!("{}", recorder.as_jsonl());
    println!("recorder_jsonl_end");
    Ok(())
}
