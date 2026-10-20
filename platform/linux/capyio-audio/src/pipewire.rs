use crate::{PcmPlaybackSink, validate_target};
use std::{
    ffi::OsString,
    io::{self, Write},
    net::Shutdown,
    os::{fd::OwnedFd, unix::net::UnixStream},
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

/// External process boundary: no shell, device installation, module loading or
/// callback network I/O. `pw-cat` must already exist on the selected Linux host.
pub struct PipeWirePlayback {
    input: Option<UnixStream>,
    child: Child,
}

impl PipeWirePlayback {
    pub fn open(target: &str) -> io::Result<Self> {
        let args = playback_arguments(target)?;
        // Fixed distributor-owned path, not PATH search or peer-provided input.
        let mut command = Command::new(Path::new("/usr/bin/pw-cat"));
        command.args(args);
        Self::spawn(command)
    }

    fn spawn(mut command: Command) -> io::Result<Self> {
        let (input, child_input) = UnixStream::pair()?;
        input.set_nonblocking(true)?;
        let child_fd: OwnedFd = child_input.into();
        let child = command
            .stdin(Stdio::from(child_fd))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        Ok(Self {
            input: Some(input),
            child,
        })
    }

    /// EOF first, then kill only our child if it does not exit. Both waits are
    /// polling with a deadline, not a potentially unbounded `Child::wait`.
    /// An unkillable OS process is reported as cleanup failure, not success.
    pub fn stop(&mut self) -> io::Result<()> {
        if let Some(input) = self.input.take() {
            let _ = input.shutdown(Shutdown::Both);
        }
        if self.poll_exit(Duration::from_millis(100))? {
            return Ok(());
        }
        match self.child.kill() {
            Ok(()) => {}
            Err(error) => {
                if self.child.try_wait()?.is_none() {
                    return Err(error);
                }
            }
        }
        if self.poll_exit(Duration::from_secs(1))? {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "owned PCM process did not exit after kill",
            ))
        }
    }

    fn poll_exit(&mut self, budget: Duration) -> io::Result<bool> {
        let deadline = Instant::now() + budget;
        loop {
            if self.child.try_wait()?.is_some() {
                return Ok(true);
            }
            if Instant::now() >= deadline {
                return Ok(false);
            }
            thread::sleep(Duration::from_millis(5));
        }
    }
}

impl PcmPlaybackSink for PipeWirePlayback {
    fn try_write_pcm(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.input
            .as_mut()
            .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "PCM process is stopped"))?
            .write(bytes)
    }
    fn check_alive(&mut self) -> io::Result<()> {
        if self.child.try_wait()?.is_some() {
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "PCM process exited",
            ))
        } else {
            Ok(())
        }
    }
}

impl Drop for PipeWirePlayback {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn playback_arguments(target: &str) -> io::Result<Vec<OsString>> {
    validate_target(target)
        .map_err(|reason| io::Error::new(io::ErrorKind::InvalidInput, reason))?;
    if cfg!(target_endian = "big") {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "S16LE playback requires little-endian Linux",
        ));
    }
    Ok([
        "--playback",
        "--rate=48000",
        "--channels=1",
        "--format=s16",
        "--channel-map=mono",
        "--latency=20ms",
        r#"--properties={"node.dont-fallback":true,"node.dont-reconnect":true,"node.dont-move":true}"#,
        &format!("--target={target}"),
        "-",
    ]
    .into_iter()
    .map(OsString::from)
    .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_pipewire_arguments_never_use_a_shell_or_default_target() {
        let args = playback_arguments("alsa_output.test").unwrap();
        assert_eq!(
            args,
            [
                "--playback",
                "--rate=48000",
                "--channels=1",
                "--format=s16",
                "--channel-map=mono",
                "--latency=20ms",
        r#"--properties={"node.dont-fallback":true,"node.dont-reconnect":true,"node.dont-move":true}"#,
                "--target=alsa_output.test",
                "-"
            ]
            .map(OsString::from)
        );
        for target in [
            "auto",
            "0",
            "-1",
            "bad;cmd",
            "$(id)",
            "--record",
            "two words",
            "",
        ] {
            assert!(playback_arguments(target).is_err());
        }
    }

    // These tests require Unix sockets. Run only on an authorized socket-capable
    // CI host; there are no ignored tests or silent environment skips.
    #[test]
    fn ipc_child_eof_and_repeated_stop_reap_owned_process() {
        let mut sink = PipeWirePlayback::spawn(Command::new("/bin/cat")).unwrap();
        assert_eq!(sink.try_write_pcm(&[1; 960]).unwrap(), 960);
        sink.stop().unwrap();
        sink.stop().unwrap();
        assert!(sink.child.try_wait().unwrap().is_some());
        assert!(sink.check_alive().is_err());
    }

    #[test]
    fn ipc_early_child_exit_is_not_playback_readiness() {
        let mut sink = PipeWirePlayback::spawn(Command::new("/bin/true")).unwrap();
        assert!(sink.poll_exit(Duration::from_secs(1)).unwrap());
        assert_eq!(
            sink.check_alive().unwrap_err().kind(),
            io::ErrorKind::BrokenPipe
        );
        sink.stop().unwrap();
    }

    #[test]
    fn ipc_android_wire_udp_to_owned_pcm_process_preserves_exact_bytes() {
        use crate::{PlaybackPump, tests::packet};
        use capyio_native_audio_lan::{
            NativeLanEndpointConfig, NativeLanReceiveOutcome, NativeLanUdpEndpoint,
            microphone_lab_binding,
        };
        use std::{fs, net::UdpSocket};
        let a = UdpSocket::bind("127.0.0.1:0").unwrap();
        let b = UdpSocket::bind("127.0.0.1:0").unwrap();
        let config = |peer| NativeLanEndpointConfig {
            peer,
            read_timeout: Duration::from_millis(100),
            inflight_packet_capacity: 2,
        };
        let a_addr = a.local_addr().unwrap();
        let b_addr = b.local_addr().unwrap();
        let mut sender =
            NativeLanUdpEndpoint::from_socket(a, config(b_addr), microphone_lab_binding()).unwrap();
        let mut receiver =
            NativeLanUdpEndpoint::from_socket(b, config(a_addr), microphone_lab_binding()).unwrap();
        let path = std::env::temp_dir().join(format!(
            "capyio-synthetic-pcm-{}-{}",
            std::process::id(),
            a_addr.port()
        ));
        // create_new avoids following/replacing an existing path; all retained
        // bytes are generated synthetic fixture PCM, never captured media.
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = fs::remove_file(&self.0);
            }
        }
        let _cleanup = Cleanup(path.clone());
        let (input, child_input) = UnixStream::pair().unwrap();
        input.set_nonblocking(true).unwrap();
        let fd: OwnedFd = child_input.into();
        let child = Command::new("/bin/cat")
            .stdin(Stdio::from(fd))
            .stdout(Stdio::from(file))
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut sink = PipeWirePlayback {
            input: Some(input),
            child,
        };
        let mut pump = PlaybackPump::new(Duration::from_secs(1)).unwrap();
        let sent = packet(0);
        sender.send_packet(&sent).unwrap();
        let NativeLanReceiveOutcome::Packet(received) = receiver.receive().unwrap() else {
            panic!("expected complete PCM packet");
        };
        pump.accept(&received, Duration::ZERO).unwrap();
        let start = Instant::now();
        while pump.metrics().packets_written == 0 {
            pump.flush(&mut sink, start.elapsed()).unwrap();
        }
        sink.stop().unwrap();
        assert!(sink.child.try_wait().unwrap().is_some());
        assert_eq!(fs::read(&path).unwrap(), sent.payload);
        assert_eq!(receiver.metrics().packets_received, 1);
        assert_eq!(pump.metrics().bytes_written, 960);
    }

    #[test]
    fn ipc_full_stdin_never_blocks_stop_and_child_is_reaped() {
        let mut command = Command::new("/bin/sleep");
        command.arg("10");
        let mut sink = PipeWirePlayback::spawn(command).unwrap();
        let start = Instant::now();
        let mut full = false;
        for _ in 0..4096 {
            match sink.try_write_pcm(&[1; 960]) {
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    full = true;
                    break;
                }
                Err(error) => panic!("unexpected write error: {error}"),
            }
            assert!(start.elapsed() < Duration::from_secs(2));
        }
        assert!(full, "fixture must establish real socket backpressure");
        sink.stop().unwrap();
        assert!(sink.child.try_wait().unwrap().is_some());
        assert!(start.elapsed() < Duration::from_secs(3));
    }
}
