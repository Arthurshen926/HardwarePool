use crate::{CaptureEndpoint, PcmCaptureSource};
use std::{
    ffi::OsString,
    io::{self, Read},
    net::Shutdown,
    os::{
        fd::OwnedFd,
        unix::{fs::MetadataExt, net::UnixStream},
    },
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

/// An explicitly admitted source process, never a default device or a shell.
/// The owner must authorize the exact local target before opening this boundary.
/// stdout is PCM only and is never logged or mixed with the control plane.
pub struct PipeWireCapture {
    output: Option<UnixStream>,
    child: Child,
}

impl PipeWireCapture {
    pub fn open(target: &str) -> io::Result<Self> {
        Self::open_checked(&CaptureEndpoint::SelectedSource(target.into()), &mut || {
            Ok(())
        })
    }

    pub(crate) fn open_checked(
        endpoint: &CaptureEndpoint,
        revalidate: &mut dyn FnMut() -> io::Result<()>,
    ) -> io::Result<Self> {
        // Validate without opening any resource, then identify the exact binary.
        capture_endpoint_arguments(endpoint, RawMode::Explicit)?;
        let identity = binary_identity()?;
        let mode = probe_raw_mode()?;
        if binary_identity()? != identity {
            return Err(unsupported("capture binary changed during probe"));
        }
        let command = capture_endpoint_command(endpoint, mode)?;
        revalidate()?; // Probe delay never extends permission to start capture.
        let mut capture = Self::spawn(command)?;
        if binary_identity()? != identity {
            let _ = capture.stop();
            return Err(unsupported("capture binary changed during opening"));
        }
        Ok(capture)
    }

    #[cfg(test)]
    pub(crate) fn synthetic_pcm() -> io::Result<Self> {
        let mut command = Command::new("/usr/bin/yes");
        command.arg("S");
        Self::spawn(command)
    }

    fn spawn(mut command: Command) -> io::Result<Self> {
        let (output, child_output) = UnixStream::pair()?;
        output.set_nonblocking(true)?;
        let child_fd: OwnedFd = child_output.into();
        let child = command
            .stdin(Stdio::null())
            .stdout(Stdio::from(child_fd))
            .stderr(Stdio::null())
            .spawn()?;
        Ok(Self {
            output: Some(output),
            child,
        })
    }

    /// Close the only PCM reader first, allow 100 ms for exit, then kill only the
    /// owned child and allow one second to reap. Never use unbounded Child::wait.
    pub fn stop(&mut self) -> io::Result<()> {
        if let Some(output) = self.output.take() {
            let _ = output.shutdown(Shutdown::Both);
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
                "owned PCM capture process did not exit after kill",
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

impl PcmCaptureSource for PipeWireCapture {
    fn try_read_pcm(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.output
            .as_mut()
            .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "PCM capture is stopped"))?
            .read(bytes)
    }

    fn check_alive(&mut self) -> io::Result<()> {
        if self.output.is_none() || self.child.try_wait()?.is_some() {
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "PCM capture process exited or stopped",
            ))
        } else {
            Ok(())
        }
    }
}

impl Drop for PipeWireCapture {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RawMode {
    LegacyPipe,
    Explicit,
}
fn unsupported(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, message)
}
// Fail closed outside source-verified pw-cat contracts. Versions describe both
// the tool headers and linked library; they are not inferred from OS branding.
fn raw_mode(version: &[u8]) -> io::Result<RawMode> {
    match version {
        b"pw-cat\nCompiled with libpipewire 1.0.5\nLinked with libpipewire 1.0.5\n" => {
            Ok(RawMode::LegacyPipe)
        }
        b"pw-cat\nCompiled with libpipewire 1.6.9\nLinked with libpipewire 1.6.9\n" => {
            Ok(RawMode::Explicit)
        }
        _ => Err(unsupported("unsupported pw-cat raw stdout contract")),
    }
}
fn binary_identity() -> io::Result<[u64; 7]> {
    let meta = std::fs::metadata("/usr/bin/pw-cat")?;
    if !meta.is_file() {
        return Err(unsupported("capture executable is not a file"));
    }
    Ok([
        meta.dev(),
        meta.ino(),
        meta.len(),
        meta.mtime() as u64,
        meta.mtime_nsec() as u64,
        meta.ctime() as u64,
        meta.ctime_nsec() as u64,
    ])
}
fn probe_raw_mode() -> io::Result<RawMode> {
    let mut command = crate::pipewire_command::fixed_command();
    command.arg("--version").env("LC_ALL", "C");
    probe_command(command)
}
fn probe_command(command: Command) -> io::Result<RawMode> {
    // --version exits before connecting a PipeWire context. The same owned
    // child boundary bounds output, waiting and cleanup; no wait_with_output.
    let mut probe = PipeWireCapture::spawn(command)?;
    let deadline = Instant::now() + Duration::from_millis(500);
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 513];
    let mut eof = false;
    loop {
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "capture version probe timed out",
            ));
        }
        if !eof {
            match probe
                .output
                .as_mut()
                .expect("owned probe output")
                .read(&mut buffer)
            {
                Ok(0) => eof = true,
                Ok(count) => {
                    if bytes.len() + count > 512 {
                        return Err(unsupported("capture version output exceeds bound"));
                    }
                    bytes.extend_from_slice(&buffer[..count]);
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) => {}
                Err(error) => return Err(error),
            }
        }
        if eof && let Some(status) = probe.child.try_wait()? {
            if !status.success() {
                return Err(unsupported("capture version probe failed"));
            }
            return raw_mode(&bytes);
        }
        thread::sleep(Duration::from_millis(2));
    }
}

#[cfg(test)]
fn capture_command(target: &str, mode: RawMode) -> io::Result<Command> {
    capture_endpoint_command(&CaptureEndpoint::SelectedSource(target.into()), mode)
}
fn capture_endpoint_command(endpoint: &CaptureEndpoint, mode: RawMode) -> io::Result<Command> {
    let mut command = crate::pipewire_command::fixed_command();
    command.args(capture_endpoint_arguments(endpoint, mode)?);
    Ok(command)
}
#[cfg(test)]
fn capture_arguments(target: &str, mode: RawMode) -> io::Result<Vec<OsString>> {
    capture_endpoint_arguments(&CaptureEndpoint::SelectedSource(target.into()), mode)
}
fn capture_endpoint_arguments(
    endpoint: &CaptureEndpoint,
    mode: RawMode,
) -> io::Result<Vec<OsString>> {
    endpoint
        .validate()
        .map_err(|reason| io::Error::new(io::ErrorKind::InvalidInput, reason))?;
    let (target, properties) = match endpoint {
        CaptureEndpoint::SelectedSource(target) => (target.clone(), r#"--properties={"node.dont-fallback":true,"node.dont-reconnect":true,"node.dont-move":true}"#.to_owned()),
        CaptureEndpoint::VirtualSpeaker(_) => ("0".into(), format!(r#"--properties={{"media.class":"Audio/Sink","node.virtual":true,"node.name":"{}","node.description":"CapyIO Virtual Speaker","node.autoconnect":false,"node.dont-fallback":true,"node.dont-reconnect":true,"node.dont-move":true}}"#, endpoint.node_name())),
    };
    if cfg!(target_endian = "big") {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "S16LE capture requires little-endian Linux",
        ));
    }
    let mut args: Vec<OsString> = [
        "--record",
        "--rate=48000",
        "--channels=2",
        "--channel-map=stereo",
        "--format=s16",
        "--latency=20ms",
        &properties,
        &format!("--target={target}"),
        "-",
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    if mode == RawMode::Explicit {
        args.insert(1, OsString::from("--raw"));
    }
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CAPTURE_PCM_PACKET_BYTES, CapturePump};

    #[test]
    fn virtual_speaker_owned_sink_exact_stereo_no_target_and_scrubbed_env() {
        let source = capyio_core::PortId::new();
        let endpoint = CaptureEndpoint::VirtualSpeaker(source);
        for mode in [RawMode::LegacyPipe, RawMode::Explicit] {
            let command = capture_endpoint_command(&endpoint, mode).unwrap();
            let args: Vec<_> = command.get_args().map(|a| a.to_str().unwrap()).collect();
            assert!(args.contains(&"--record"));
            assert!(args.contains(&"--target=0"));
            assert!(args.contains(&"--rate=48000"));
            assert!(args.contains(&"--channels=2"));
            assert!(args.contains(&"--channel-map=stereo"));
            assert!(args.contains(&"--format=s16"));
            assert_eq!(args.contains(&"--raw"), mode == RawMode::Explicit);
            assert_eq!(args.last(), Some(&"-"));
            let props = args
                .iter()
                .find(|a| a.starts_with("--properties="))
                .unwrap();
            assert!(props.contains("\"media.class\":\"Audio/Sink\""));
            assert!(props.contains("\"node.virtual\":true"));
            assert!(props.contains("\"node.autoconnect\":false"));
            assert!(props.contains(&crate::virtual_speaker_name(source)));
            assert!(!props.contains("monitor"));
            for key in crate::pipewire_command::STREAM_OVERRIDES {
                assert!(command.get_envs().any(|(k, v)| k == key && v.is_none()));
            }
        }
        let nil = "00000000-0000-0000-0000-000000000000".parse().unwrap();
        assert!(
            capture_endpoint_arguments(&CaptureEndpoint::VirtualSpeaker(nil), RawMode::Explicit)
                .is_err()
        );
    }

    #[test]
    fn capture_pipewire_exact_raw_stereo_target_without_environment_override() {
        let command = capture_command("alsa_input.approved", RawMode::Explicit).unwrap();
        assert_eq!(command.get_program(), "/usr/bin/pw-cat");
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_str().unwrap())
            .collect();
        assert_eq!(
            args,
            [
                "--record",
                "--raw",
                "--rate=48000",
                "--channels=2",
                "--channel-map=stereo",
                "--format=s16",
                "--latency=20ms",
                r#"--properties={"node.dont-fallback":true,"node.dont-reconnect":true,"node.dont-move":true}"#,
                "--target=alsa_input.approved",
                "-",
            ]
        );
        assert!(
            command
                .get_envs()
                .any(|(key, value)| key == "PIPEWIRE_NODE" && value.is_none())
        );
        assert!(capture_arguments("91", RawMode::Explicit).is_ok());
    }

    #[test]
    fn capture_pipewire_rejects_default_ambiguous_or_injected_targets() {
        for target in [
            "",
            "auto",
            "AUTO",
            "default",
            "DEFAULT",
            "0",
            "-1",
            "@DEFAULT_AUDIO_SOURCE@",
            "two words",
            "--playback",
            "bad;cmd",
            "$(id)",
            "source\nnext",
        ] {
            assert!(
                capture_arguments(target, RawMode::Explicit).is_err(),
                "accepted {target:?}"
            );
        }
        assert!(capture_arguments(&"a".repeat(129), RawMode::Explicit).is_err());
    }

    #[test]
    fn capture_raw_contract_requires_exact_supported_tool_and_library() {
        for (version, mode) in [("1.0.5", RawMode::LegacyPipe), ("1.6.9", RawMode::Explicit)] {
            let text = format!(
                "pw-cat\nCompiled with libpipewire {version}\nLinked with libpipewire {version}\n"
            );
            assert_eq!(raw_mode(text.as_bytes()).unwrap(), mode);
            let args = capture_arguments("approved.source", mode).unwrap();
            assert_eq!(args.iter().any(|a| a == "--raw"), mode == RawMode::Explicit);
            assert_eq!(args.last().unwrap(), "-");
            assert!(args.iter().any(|a| a == "--format=s16"));
        }
        for bad in [
            "",
            "pw-cat\n",
            "1.0.5",
            "pw-cat\nCompiled with libpipewire 1.0.5\nLinked with libpipewire 1.6.9\n",
            "pw-cat\nCompiled with libpipewire 1.7.0\nLinked with libpipewire 1.7.0\n",
            ".snd",
            "RIFF",
        ] {
            assert!(raw_mode(bad.as_bytes()).is_err());
        }
    }

    #[test]
    fn ipc_capture_version_probe_bounds_output_failure_and_stalled_child() {
        let mut valid = Command::new("/usr/bin/printf");
        valid.arg("pw-cat\nCompiled with libpipewire 1.0.5\nLinked with libpipewire 1.0.5\n");
        assert_eq!(probe_command(valid).unwrap(), RawMode::LegacyPipe);
        assert!(probe_command(Command::new("/usr/bin/false")).is_err());
        assert!(probe_command(Command::new("/usr/bin/yes")).is_err());
        let mut stalled = Command::new("/usr/bin/sleep");
        stalled.arg("10");
        let started = Instant::now();
        assert_eq!(
            probe_command(stalled).unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    // These mandatory tests need actual Unix sockets/processes (and UDP below).
    // Restricted local execution must compile them, never retry denied sockets.
    #[test]
    fn ipc_capture_owned_stdout_packet_and_repeated_stop_reap_child() {
        let mut source = PipeWireCapture::synthetic_pcm().unwrap();
        let mut pump =
            CapturePump::with_binding(Duration::from_secs(3), crate::capture::tests::binding())
                .unwrap();
        let start = Instant::now();
        let packet = loop {
            if let Some(packet) = pump.poll(&mut source, start.elapsed()).unwrap() {
                break packet;
            }
            assert!(
                start.elapsed() < Duration::from_secs(2),
                "synthetic capture did not produce PCM"
            );
            thread::sleep(Duration::from_millis(1));
        };
        assert_eq!(packet.payload, b"S\n".repeat(CAPTURE_PCM_PACKET_BYTES / 2));
        source.stop().unwrap();
        source.stop().unwrap();
        assert!(source.child.try_wait().unwrap().is_some());
        assert!(source.check_alive().is_err());
        assert!(source.try_read_pcm(&mut [0; 1]).is_err());
    }

    #[test]
    fn ipc_capture_early_exit_and_backpressured_stdout_cleanup_are_bounded() {
        let mut source = PipeWireCapture::spawn(Command::new("/bin/true")).unwrap();
        assert!(source.poll_exit(Duration::from_secs(1)).unwrap());
        assert_eq!(
            source.check_alive().unwrap_err().kind(),
            io::ErrorKind::BrokenPipe
        );
        source.stop().unwrap();

        let mut source = PipeWireCapture::synthetic_pcm().unwrap();
        // Intentionally leave output unread, so a continuous producer reaches
        // the OS-bounded socket buffer rather than allocating an application queue.
        thread::sleep(Duration::from_millis(100));
        let start = Instant::now();
        source.stop().unwrap();
        assert!(start.elapsed() < Duration::from_secs(2));
        assert!(source.child.try_wait().unwrap().is_some());
    }

    #[test]
    fn ipc_capture_dynamic_stereo_packet_native_udp_preserves_exact_bytes() {
        use capyio_native_audio_lan::{
            NativeLanEndpointConfig, NativeLanReceiveOutcome, NativeLanUdpEndpoint,
        };
        use std::net::UdpSocket;
        let sender_socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let receiver_socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let sender_address = sender_socket.local_addr().unwrap();
        let receiver_address = receiver_socket.local_addr().unwrap();
        let media = crate::capture::tests::binding();
        let config = |peer| NativeLanEndpointConfig {
            peer,
            read_timeout: Duration::from_millis(10),
            inflight_packet_capacity: 2,
        };
        let mut sender = NativeLanUdpEndpoint::from_socket(
            sender_socket,
            config(receiver_address),
            media.clone(),
        )
        .unwrap();
        let mut receiver = NativeLanUdpEndpoint::from_socket(
            receiver_socket,
            config(sender_address),
            media.clone(),
        )
        .unwrap();
        let mut source = PipeWireCapture::synthetic_pcm().unwrap();
        let mut pump = CapturePump::with_binding(Duration::from_secs(3), media).unwrap();
        let start = Instant::now();
        let packet = loop {
            if let Some(packet) = pump.poll(&mut source, start.elapsed()).unwrap() {
                break packet;
            }
            assert!(start.elapsed() < Duration::from_secs(2));
            thread::sleep(Duration::from_millis(1));
        };
        sender.send_packet(&packet).unwrap();
        let received = loop {
            match receiver.receive().unwrap() {
                NativeLanReceiveOutcome::Packet(packet) => break packet,
                NativeLanReceiveOutcome::Pending => {}
                other => panic!("unexpected native UDP outcome: {other:?}"),
            }
            assert!(start.elapsed() < Duration::from_secs(2));
        };
        assert_eq!(received, packet);
        assert_eq!(
            received.payload,
            b"S\n".repeat(CAPTURE_PCM_PACKET_BYTES / 2)
        );
        assert_eq!(sender.metrics().packets_sent, 1);
        source.stop().unwrap();
        assert!(source.child.try_wait().unwrap().is_some());
    }
}
