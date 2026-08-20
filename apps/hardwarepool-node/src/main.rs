use anyhow::Context;
use clap::{Parser, Subcommand};
use hardwarepool_audio::{AudioFrame, ClockDriftEstimator, InsertOutcome, ReorderBuffer};
use hardwarepool_core::{AudioFormat, StreamId};
use hardwarepool_protocol::{decode_envelope, encode_envelope, new_envelope, v1};
use hardwarepool_testkit::{DemoLab, android_node};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(name = "hardwarepool-node")]
#[command(about = "HardwarePool bootstrap headless node and deterministic demo")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Runs the complete mock speaker/microphone lifecycle.
    Demo,
    /// Prints the initial deterministic Runtime snapshot as JSON.
    Snapshot,
    /// Converts the sample Android node through Protobuf and binary Envelope encoding.
    ProtocolRoundtrip,
    /// Exercises frame validation, out-of-order delivery and clock estimation without hardware.
    AudioFrameDemo,
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
    }
}

fn run_demo() -> anyhow::Result<()> {
    let mut lab = DemoLab::new().context("create deterministic lab")?;
    tracing::info!("activating phone speaker projection");
    lab.set_speaker_active(true, 1_000)?;
    tracing::info!("activating phone microphone projection");
    lab.set_microphone_active(true, 2_000)?;
    tracing::info!("stopping microphone while leaving speaker active");
    lab.set_microphone_active(false, 3_000)?;

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
        supported_protocol_majors: vec![hardwarepool_protocol::PROTOCOL_MAJOR],
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
