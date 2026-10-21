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
    /// Runs the Linux headless host over inherited stdin/stdout (no hardware or network).
    LinuxHost {
        #[arg(long)]
        node_id: capyio_core::NodeId,
        #[arg(long, default_value = "CapyIO Linux Node")]
        name: String,
        /// Enable synthetic lifecycle using repository mock binaries beside this executable.
        #[arg(long)]
        mock_routes: bool,
        /// Transfer one finite fixture via separate Linux-local IPC; no hardware or IP network.
        #[arg(long, requires = "mock_routes")]
        mock_payload: bool,
        /// Transfer a fixed four-record ordered fixture, not a continuous stream.
        #[arg(long, requires = "mock_routes", conflicts_with = "mock_payload")]
        mock_batch: bool,
    },
    /// Explicit finite pairing management over exclusive inherited FIFO pipes.
    #[cfg(feature = "paired-store")]
    LinuxPairing {
        #[arg(long, required = true)]
        trusted_lab: bool,
        #[arg(long)]
        pairing_state: std::path::PathBuf,
        #[arg(long)]
        pairing_node: capyio_core::NodeId,
        #[arg(long)]
        pairing_bus_address: String,
        #[arg(long)]
        pairing_collection: String,
    },
    /// Explicit one-shot trusted-lab playback. Private inherited FIFO bootstrap;
    /// no credentials in arguments, no enrollment, no listener without this command.
    LinuxPlayback {
        #[arg(long, required = true)]
        trusted_lab: bool,
        #[cfg(feature = "paired-store")]
        #[command(flatten)]
        paired_store: PairedStoreArgs,
    },
    /// Explicit selected-source stereo capture with framed private bootstrap.
    LinuxCapture {
        #[arg(long, required = true)]
        trusted_lab: bool,
        #[cfg(feature = "paired-store")]
        #[command(flatten)]
        paired_store: PairedStoreArgs,
    },
    /// Finite pinned Android touchpad reception via private inherited bootstrap.
    LinuxTouchpad {
        #[arg(long, required = true)]
        trusted_lab: bool,
        #[cfg(feature = "paired-store")]
        #[command(flatten)]
        paired_store: PairedStoreArgs,
    },
    /// Software-only Type-B syscall fixture; never opens an input device.
    #[cfg(feature = "touchpad-test-fixture")]
    LinuxTouchpadFixture {
        #[arg(long, required = true)]
        trusted_lab: bool,
        #[arg(long, value_parser = ["normal", "fail-create", "partial-write", "delayed-commit", "backpressure"])]
        fixture_mode: String,
        #[cfg(feature = "paired-store")]
        #[command(flatten)]
        paired_store: PairedStoreArgs,
    },
    /// Finite pinned Android camera reception via private inherited bootstrap.
    LinuxCamera {
        #[arg(long, required = true)]
        trusted_lab: bool,
        #[cfg(feature = "paired-store")]
        #[command(flatten)]
        paired_store: PairedStoreArgs,
    },
    /// Reports passive Linux playback prerequisites as JSON; never starts playback or connects.
    LinuxPlaybackPreflight {
        /// Optional intended PipeWire target; syntax only, not discovery or authorization.
        #[arg(long)]
        target: Option<String>,
    },
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
        .with_writer(std::io::stderr)
        .with_target(false)
        .compact()
        .init();

    // This opt-in command accepts secret material only through framed stdin.
    // Never echo even accidentally supplied argv values on its syntax errors.
    let lab_command = std::env::args_os().nth(1).is_some_and(|arg| {
        arg == "linux-playback"
            || arg == "linux-capture"
            || arg == "linux-camera"
            || arg == "linux-touchpad"
            || arg == "linux-touchpad-fixture"
    });
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error)
            if lab_command
                && !matches!(
                    error.kind(),
                    clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
                ) =>
        {
            anyhow::bail!(
                "invalid Linux lab playback command; --trusted-lab is required; credentials must not be supplied in arguments"
            )
        }
        Err(error) => error.exit(),
    };
    match cli.command.unwrap_or(Command::Demo) {
        Command::LinuxHost {
            node_id,
            name,
            mock_routes,
            mock_payload,
            mock_batch,
        } => {
            anyhow::ensure!(cfg!(target_os = "linux"), "linux-host requires Linux");
            let mut host = capyio_linux_host::LinuxNodeHost::new(node_id, &name)?;
            if mock_routes {
                let executable = std::env::current_exe()?;
                host = host.with_mock_routes(
                    executable
                        .parent()
                        .context("missing executable directory")?,
                )?;
            }
            if mock_payload {
                host = host.with_mock_payload()?;
            }
            if mock_batch {
                host = host.with_mock_batch()?;
            }
            host.serve(std::io::stdin().lock(), std::io::stdout().lock())?;
            Ok(())
        }
        #[cfg(feature = "paired-store")]
        Command::LinuxPairing {
            trusted_lab,
            pairing_state,
            pairing_node,
            pairing_bus_address,
            pairing_collection,
        } => {
            anyhow::ensure!(trusted_lab, "explicit trusted-lab opt-in required");
            #[cfg(target_os = "linux")]
            {
                capyio_linux_host::run_pairing_management_stdio(
                    capyio_linux_host::PairingManagementSelection {
                        local_node: pairing_node,
                        state_path: pairing_state,
                        bus_address: pairing_bus_address,
                        collection: pairing_collection,
                    },
                )
                .map_err(|_| anyhow::anyhow!("Linux pairing management failed"))
            }
            #[cfg(not(target_os = "linux"))]
            {
                let _ = (
                    pairing_state,
                    pairing_node,
                    pairing_bus_address,
                    pairing_collection,
                );
                anyhow::bail!("Linux pairing management requires Linux")
            }
        }
        Command::LinuxPlayback {
            trusted_lab,
            #[cfg(feature = "paired-store")]
            paired_store,
        } => {
            anyhow::ensure!(trusted_lab, "explicit trusted-lab opt-in required");
            #[cfg(target_os = "linux")]
            {
                #[cfg(feature = "paired-store")]
                if let Some(selection) = paired_store.selection()? {
                    return capyio_linux_host::run_trusted_lab_playback_paired_store_stdio(
                        selection,
                    )
                    .map_err(|_| anyhow::anyhow!("Linux lab playback launch failed"));
                }
                capyio_linux_host::run_trusted_lab_playback_stdio()
                    .map_err(|_| anyhow::anyhow!("Linux lab playback launch failed"))
            }
            #[cfg(not(target_os = "linux"))]
            {
                #[cfg(feature = "paired-store")]
                let _ = paired_store;
                anyhow::bail!("Linux lab playback requires Linux")
            }
        }
        Command::LinuxCapture {
            trusted_lab,
            #[cfg(feature = "paired-store")]
            paired_store,
        } => {
            anyhow::ensure!(trusted_lab, "explicit trusted-lab opt-in required");
            #[cfg(target_os = "linux")]
            {
                #[cfg(feature = "paired-store")]
                if let Some(selection) = paired_store.selection()? {
                    return capyio_linux_host::run_trusted_lab_capture_paired_store_stdio(
                        selection,
                    )
                    .map_err(|_| anyhow::anyhow!("Linux lab capture launch failed"));
                }
                capyio_linux_host::run_trusted_lab_capture_stdio()
                    .map_err(|_| anyhow::anyhow!("Linux lab capture launch failed"))
            }
            #[cfg(not(target_os = "linux"))]
            {
                #[cfg(feature = "paired-store")]
                let _ = paired_store;
                anyhow::bail!("Linux lab capture requires Linux")
            }
        }
        Command::LinuxTouchpad {
            trusted_lab,
            #[cfg(feature = "paired-store")]
            paired_store,
        } => {
            anyhow::ensure!(trusted_lab, "explicit trusted-lab opt-in required");
            #[cfg(target_os = "linux")]
            {
                #[cfg(feature = "paired-store")]
                if let Some(selection) = paired_store.selection()? {
                    return capyio_linux_host::run_touchpad_paired_store_stdio(selection)
                        .map_err(|_| anyhow::anyhow!("Linux touchpad launch failed"));
                }
                capyio_linux_host::run_touchpad_stdio()
                    .map_err(|_| anyhow::anyhow!("Linux touchpad launch failed"))
            }
            #[cfg(not(target_os = "linux"))]
            {
                #[cfg(feature = "paired-store")]
                let _ = paired_store;
                anyhow::bail!("Linux touchpad requires Linux")
            }
        }
        #[cfg(feature = "touchpad-test-fixture")]
        Command::LinuxTouchpadFixture {
            trusted_lab,
            fixture_mode,
            #[cfg(feature = "paired-store")]
            paired_store,
        } => {
            anyhow::ensure!(trusted_lab, "explicit trusted-lab opt-in required");
            #[cfg(target_os = "linux")]
            {
                #[cfg(feature = "paired-store")]
                if let Some(selection) = paired_store.selection()? {
                    return capyio_linux_host::run_touchpad_fixture_paired_store_stdio(
                        &fixture_mode,
                        selection,
                    )
                    .map_err(|_| anyhow::anyhow!("Linux touchpad launch failed"));
                }
                capyio_linux_host::run_touchpad_fixture_stdio(&fixture_mode)
                    .map_err(|_| anyhow::anyhow!("Linux touchpad launch failed"))
            }
            #[cfg(not(target_os = "linux"))]
            {
                let _ = fixture_mode;
                #[cfg(feature = "paired-store")]
                let _ = paired_store;
                anyhow::bail!("Linux touchpad requires Linux")
            }
        }
        Command::LinuxCamera {
            trusted_lab,
            #[cfg(feature = "paired-store")]
            paired_store,
        } => {
            anyhow::ensure!(trusted_lab, "explicit trusted-lab opt-in required");
            #[cfg(target_os = "linux")]
            {
                #[cfg(feature = "paired-store")]
                if let Some(selection) = paired_store.selection()? {
                    return capyio_linux_host::run_camera_paired_store_stdio(selection)
                        .map_err(|_| anyhow::anyhow!("Linux camera launch failed"));
                }
                capyio_linux_host::run_camera_stdio()
                    .map_err(|_| anyhow::anyhow!("Linux camera launch failed"))
            }
            #[cfg(not(target_os = "linux"))]
            {
                #[cfg(feature = "paired-store")]
                let _ = paired_store;
                anyhow::bail!("Linux camera requires Linux")
            }
        }
        Command::LinuxPlaybackPreflight { target } => {
            #[cfg(target_os = "linux")]
            {
                let report = capyio_linux_host::inspect_playback_prerequisites(target.as_deref());
                let mut output = std::io::stdout().lock();
                serde_json::to_writer(&mut output, &report)?;
                std::io::Write::write_all(&mut output, b"\n")?;
                Ok(())
            }
            #[cfg(not(target_os = "linux"))]
            {
                let _ = target;
                anyhow::bail!("Linux playback preflight requires Linux")
            }
        }
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

#[cfg(test)]
mod preflight_cli_tests {
    use super::*;

    #[test]
    fn passive_preflight_is_separate_from_playback_and_default() {
        assert!(
            Cli::try_parse_from(["capyio-node"])
                .unwrap()
                .command
                .is_none()
        );
        assert!(matches!(
            Cli::try_parse_from(["capyio-node", "linux-playback-preflight"])
                .unwrap()
                .command,
            Some(Command::LinuxPlaybackPreflight { target: None })
        ));
        assert!(matches!(
            Cli::try_parse_from(["capyio-node", "linux-playback-preflight", "--target", "42"])
                .unwrap()
                .command,
            Some(Command::LinuxPlaybackPreflight { target: Some(target) }) if target == "42"
        ));
        assert!(Cli::try_parse_from(["capyio-node", "linux-playback"]).is_err());
        assert!(
            Cli::try_parse_from(["capyio-node", "linux-playback-preflight", "--trusted-lab"])
                .is_err()
        );
    }
}

/// Nonsecret selectors only. Supplying any selector requires the complete set.
#[cfg(feature = "paired-store")]
#[derive(Debug, clap::Args)]
#[group(multiple = true, requires_all = ["pairing_state", "pairing_node", "pairing_peer", "pairing_bus_address", "pairing_collection"])]
struct PairedStoreArgs {
    /// Existing owner-only pairing metadata directory; never a credential file.
    #[arg(long)]
    pairing_state: Option<std::path::PathBuf>,
    #[arg(long)]
    pairing_node: Option<capyio_core::NodeId>,
    #[arg(long)]
    pairing_peer: Option<capyio_core::NodeId>,
    /// Explicit local Unix D-Bus address; no session-bus environment default.
    #[arg(long)]
    pairing_bus_address: Option<String>,
    /// Existing unlocked persistent Secret Service collection object path.
    #[arg(long)]
    pairing_collection: Option<String>,
}
#[cfg(all(target_os = "linux", feature = "paired-store"))]
impl PairedStoreArgs {
    fn selection(self) -> anyhow::Result<Option<capyio_linux_host::PairedStoreSelection>> {
        match (
            self.pairing_state,
            self.pairing_node,
            self.pairing_peer,
            self.pairing_bus_address,
            self.pairing_collection,
        ) {
            (None, None, None, None, None) => Ok(None),
            (
                Some(state_path),
                Some(local_node),
                Some(peer),
                Some(bus_address),
                Some(collection),
            ) => Ok(Some(capyio_linux_host::PairedStoreSelection {
                local_node,
                state_path,
                bus_address,
                collection,
                peer,
            })),
            _ => anyhow::bail!("complete paired-store selector set required"),
        }
    }
}
#[cfg(all(test, feature = "paired-store"))]
mod paired_store_cli_tests {
    use super::*;
    #[test]
    fn pairing_management_requires_explicit_public_selectors_only() {
        let args = [
            "node",
            "linux-pairing",
            "--trusted-lab",
            "--pairing-state",
            "/synthetic",
            "--pairing-node",
            "00000000-0000-0000-0000-000000000001",
            "--pairing-bus-address",
            "unix:path=/synthetic",
            "--pairing-collection",
            "/org/freedesktop/secrets/collection/test",
        ];
        assert!(Cli::try_parse_from(args).is_ok());
        for index in [2, 3, 5, 7, 9] {
            let count = if index == 2 { 1 } else { 2 };
            let mut missing = args.to_vec();
            missing.drain(index..index + count);
            assert!(Cli::try_parse_from(missing).is_err());
        }
        for secret_flag in ["--pin", "--credentials", "--private-key", "--pairing-peer"] {
            let mut extra = args.to_vec();
            extra.extend([secret_flag, "synthetic"]);
            assert!(Cli::try_parse_from(extra).is_err());
        }
    }

    #[test]
    fn paired_store_selector_group_is_all_or_none() {
        for command in [
            "linux-camera",
            "linux-touchpad",
            "linux-playback",
            "linux-capture",
        ] {
            assert!(Cli::try_parse_from(["node", command, "--trusted-lab"]).is_ok());
            for flag in [
                "--pairing-state",
                "--pairing-node",
                "--pairing-peer",
                "--pairing-bus-address",
                "--pairing-collection",
            ] {
                let value = if flag == "--pairing-node" || flag == "--pairing-peer" {
                    "00000000-0000-0000-0000-000000000001"
                } else {
                    "/synthetic"
                };
                assert!(
                    Cli::try_parse_from(["node", command, "--trusted-lab", flag, value]).is_err()
                );
            }
            assert!(
                Cli::try_parse_from([
                    "node",
                    command,
                    "--trusted-lab",
                    "--pairing-state",
                    "/synthetic",
                    "--pairing-node",
                    "00000000-0000-0000-0000-000000000001",
                    "--pairing-peer",
                    "00000000-0000-0000-0000-000000000002",
                    "--pairing-bus-address",
                    "unix:path=/synthetic",
                    "--pairing-collection",
                    "/org/freedesktop/secrets/collection/capyio"
                ])
                .is_ok()
            );
        }
    }

    #[cfg(feature = "touchpad-test-fixture")]
    #[test]
    fn paired_fixture_selectors_remain_all_or_none() {
        let base = [
            "node",
            "linux-touchpad-fixture",
            "--trusted-lab",
            "--fixture-mode",
            "normal",
        ];
        assert!(Cli::try_parse_from(base).is_ok());
        let mut partial = base.to_vec();
        partial.extend(["--pairing-state", "/synthetic"]);
        assert!(Cli::try_parse_from(partial).is_err());
        let mut full = base.to_vec();
        full.extend([
            "--pairing-state",
            "/synthetic",
            "--pairing-node",
            "00000000-0000-0000-0000-000000000001",
            "--pairing-peer",
            "00000000-0000-0000-0000-000000000002",
            "--pairing-bus-address",
            "unix:path=/synthetic",
            "--pairing-collection",
            "/org/freedesktop/secrets/collection/capyio",
        ]);
        assert!(Cli::try_parse_from(full).is_ok());
    }
}
