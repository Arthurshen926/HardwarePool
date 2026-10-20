//! Explicit hosted-only fixture, not a new application playback command.
//! The harness owns a private PipeWire daemon and manually links only synthetic
//! nodes. This example exercises the production PCM pump and fixed pw-cat path.

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("PipeWire fixture requires Linux");
    std::process::exit(1);
}

#[cfg(target_os = "linux")]
fn main() -> std::process::ExitCode {
    match fixture::run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(_) => {
            eprintln!("capyio_pipewire_fixture=failed");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(target_os = "linux")]
mod fixture {
    use std::{
        ffi::{OsStr, OsString},
        fs, io,
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
        thread,
        time::{Duration, Instant},
    };

    use capyio_audio::{AudioMediaPacket, AudioMediaStreamBinding, AudioStreamSpec};
    use capyio_core::{RouteId, SessionId, StreamId};
    use capyio_linux_audio::{PcmPlaybackSink, PipeWirePlayback, PlaybackPump};

    const MARKER: &[u8] = b"capyio-hosted-pipewire-loopback-v1\n";
    const FRAMES: u64 = 500;
    const FRAME_SAMPLES: u32 = 480;
    const LIFETIME: Duration = Duration::from_secs(14);

    fn rejected() -> io::Error {
        io::Error::other("isolated hosted PipeWire fixture contract failed")
    }

    fn validate_invocation(
        args: &[OsString],
        enabled: Option<&OsStr>,
        remote: Option<&OsStr>,
    ) -> io::Result<()> {
        if args != [OsString::from("--hosted-pipewire-loopback")]
            || enabled != Some(OsStr::new("1"))
            || remote != Some(OsStr::new("capyio-test"))
        {
            return Err(rejected());
        }
        Ok(())
    }

    fn exact_marker(path: &Path) -> io::Result<bool> {
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error),
        };
        if !metadata.is_file() || metadata.len() != MARKER.len() as u64 {
            return Err(rejected());
        }
        Ok(fs::read(path)? == MARKER)
    }

    fn private_runtime() -> io::Result<PathBuf> {
        let runtime = std::env::var_os("XDG_RUNTIME_DIR").ok_or_else(rejected)?;
        if std::env::var_os("PIPEWIRE_RUNTIME_DIR").as_ref() != Some(&runtime) {
            return Err(rejected());
        }
        let runtime = PathBuf::from(runtime);
        let metadata = fs::symlink_metadata(&runtime)?;
        if !runtime.is_absolute()
            || !metadata.is_dir()
            || metadata.permissions().mode() & 0o777 != 0o700
            || !exact_marker(&runtime.join(".capyio-pipewire-loopback"))?
        {
            return Err(rejected());
        }
        Ok(runtime)
    }

    // Two seconds of 1 kHz, one second of silence, two seconds of 2 kHz.
    // These are generated samples, never microphone or application data.
    fn packet(sequence: u64, binding: &AudioMediaStreamBinding) -> AudioMediaPacket {
        let frequency = if sequence < 200 {
            1_000.0
        } else if sequence < 300 {
            0.0
        } else {
            2_000.0
        };
        let first_sample = sequence * u64::from(FRAME_SAMPLES);
        let payload = (0..FRAME_SAMPLES)
            .flat_map(|index| {
                let angle =
                    std::f64::consts::TAU * frequency * (first_sample + u64::from(index)) as f64
                        / 48_000.0;
                let sample = (angle.sin() * 8_192.0).round() as i16;
                sample.to_le_bytes()
            })
            .collect();
        AudioMediaPacket {
            stream_id: binding.stream_id,
            stream_epoch: binding.stream_epoch,
            sequence,
            source_timestamp_micros: sequence * 10_000 + 1,
            first_sample_index: first_sample,
            sample_count: FRAME_SAMPLES,
            discontinuity: false,
            payload,
        }
    }

    pub(super) fn run() -> io::Result<()> {
        validate_invocation(
            &std::env::args_os().skip(1).take(2).collect::<Vec<_>>(),
            std::env::var_os("CAPYIO_PIPEWIRE_TEST").as_deref(),
            std::env::var_os("PIPEWIRE_REMOTE").as_deref(),
        )?;
        let runtime = private_runtime()?;
        let started = Instant::now();
        let binding = AudioMediaStreamBinding {
            session_id: SessionId::new(),
            route_id: RouteId::new(),
            stream_id: StreamId::new(),
            stream_epoch: 1,
            selected_spec: AudioStreamSpec::voice_interactive(),
        };
        let mut pump = PlaybackPump::with_binding(LIFETIME, binding.clone())?;
        let mut sink = PipeWirePlayback::open("capyio.test.null")?;
        eprintln!("capyio_pipewire_fixture=waiting_for_links");
        let result = (|| {
            // The harness observes actual ports then links them before signalling.
            // No PCM is queued while the production stream is not yet connected.
            while !exact_marker(&runtime.join(".capyio-links-ready"))? {
                sink.check_alive()?;
                if started.elapsed() >= Duration::from_secs(8) {
                    return Err(rejected());
                }
                thread::sleep(Duration::from_millis(5));
            }
            let signal_started = Instant::now();
            for sequence in 0..FRAMES {
                let frame_time = signal_started + Duration::from_millis(sequence * 10);
                while Instant::now() < frame_time {
                    sink.check_alive()?;
                    if started.elapsed() >= LIFETIME {
                        return Err(rejected());
                    }
                    thread::sleep(Duration::from_millis(1));
                }
                pump.accept(&packet(sequence, &binding), started.elapsed())?;
                while pump.metrics().packets_written <= sequence {
                    pump.flush(&mut sink, started.elapsed())?;
                    if pump.metrics().packets_dropped_busy != 0
                        || pump.metrics().malformed_packets != 0
                        || pump.metrics().stale_packets != 0
                    {
                        return Err(rejected());
                    }
                    thread::sleep(Duration::from_millis(1));
                }
            }
            // Let the real graph consume the final bounded queued quantum before
            // closing our process input. The harness verifies actual monitor PCM.
            thread::sleep(Duration::from_millis(100));
            if pump.metrics().packets_written != FRAMES
                || pump.metrics().bytes_written != FRAMES * u64::from(FRAME_SAMPLES) * 2
            {
                return Err(rejected());
            }
            Ok(())
        })();
        // Explicit stop reports/reaps the owned child; Drop is a second cleanup
        // attempt on every early-return/unwind path. No unowned process is killed.
        let stopped = sink.stop();
        result?;
        stopped?;
        if sink.check_alive().is_ok() || started.elapsed() >= Duration::from_secs(15) {
            return Err(rejected());
        }
        eprintln!("capyio_pipewire_fixture=passed packets=500 bytes=480000 child_reaped=true");
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn binding() -> AudioMediaStreamBinding {
            AudioMediaStreamBinding {
                session_id: SessionId::new(),
                route_id: RouteId::new(),
                stream_id: StreamId::new(),
                stream_epoch: 1,
                selected_spec: AudioStreamSpec::voice_interactive(),
            }
        }

        #[test]
        fn fixture_requires_deliberate_private_test_opt_in() {
            let args = [OsString::from("--hosted-pipewire-loopback")];
            validate_invocation(
                &args,
                Some(OsStr::new("1")),
                Some(OsStr::new("capyio-test")),
            )
            .unwrap();
            assert!(
                validate_invocation(&[], Some(OsStr::new("1")), Some(OsStr::new("capyio-test")))
                    .is_err()
            );
            assert!(validate_invocation(&args, None, Some(OsStr::new("capyio-test"))).is_err());
            assert!(
                validate_invocation(&args, Some(OsStr::new("1")), Some(OsStr::new("pipewire-0")))
                    .is_err()
            );
        }

        #[test]
        fn fixture_pcm_has_two_distinct_tones_and_middle_silence() {
            let binding = binding();
            for sequence in [0, 199, 200, 299, 300, 499] {
                let packet = packet(sequence, &binding);
                packet.validate_against(&binding).unwrap();
                assert_eq!(packet.payload.len(), 960);
                let samples: Vec<_> = packet
                    .payload
                    .chunks_exact(2)
                    .map(|bytes| i16::from_le_bytes([bytes[0], bytes[1]]))
                    .collect();
                if (200..300).contains(&sequence) {
                    assert!(samples.iter().all(|sample| *sample == 0));
                } else {
                    assert!(samples.iter().any(|sample| sample.abs() >= 8_000));
                    let rising = samples
                        .windows(2)
                        .filter(|pair| pair[0] <= 0 && pair[1] > 0)
                        .count();
                    assert_eq!(rising, if sequence < 200 { 10 } else { 20 });
                }
            }
        }
    }
}
