#[cfg(windows)]
mod windows_lab {

    use std::{
        error::Error,
        ffi::c_void,
        io::{self, Write},
        net::Ipv4Addr,
        path::PathBuf,
        process::{Child, Command, ExitStatus},
        ptr, thread,
        time::{Duration, Instant},
    };

    use capyio_windows_camera::{
        MfVirtualCameraPlan, MfVirtualCameraRegistrar, MfVirtualCameraRegistrationBackend,
        fixture_stream_spec,
    };
    use capyio_windows_camera_mf::{MediaFoundationRuntime, WindowsMfVirtualCameraBackend};
    use capyio_windows_camera_share::{CameraSharedIngressConsumer, CameraSharedIngressError};
    use windows::{
        Win32::{
            Media::MediaFoundation::{
                IMFActivate, IMFAttributes, IMFMediaSource, IMFSample,
                MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME, MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
                MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
                MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
                MF_SOURCE_READER_FIRST_VIDEO_STREAM, MF_SOURCE_READERF_ENDOFSTREAM,
                MF_SOURCE_READERF_ERROR, MFCreateAttributes, MFCreateMemoryBuffer,
                MFCreateSourceReaderFromMediaSource, MFEnumDeviceSources,
            },
            System::Com::CoTaskMemFree,
        },
        core::Interface,
    };

    const MAX_ENUMERATED_CAMERAS: usize = 64;
    const MAX_DEVICE_ATTRIBUTE_UTF16: usize = 4096;
    const MAX_SOURCE_READER_EMPTY_READS: usize = 64;
    const EXTERNAL_CONSUMER_COUNT: usize = 2;
    const EXTERNAL_CONSUMER_TIMEOUT: Duration = Duration::from_secs(20);
    const EXTERNAL_CONSUMER_POLL_INTERVAL: Duration = Duration::from_millis(25);
    const CONSUMER_ENUMERATION_ATTEMPTS: usize = 100;
    const CONSUMER_ENUMERATION_RETRY_DELAY: Duration = Duration::from_millis(100);
    const GUI_HOLD_DURATION: Duration = Duration::from_secs(180);
    const LIVE_RECEIVER_START_TIMEOUT: Duration = Duration::from_secs(120);
    const LIVE_RECEIVER_STOP_TIMEOUT: Duration = Duration::from_secs(5);
    const LIVE_RECEIVER_POLL_INTERVAL: Duration = Duration::from_millis(100);
    const LIVE_RECONNECT_GRACE_MILLIS: u64 = 60_000;
    const LIVE_RECEIVER_EXECUTABLE: &str = "capyio-avc-lab-receiver.exe";
    const CONSUMER_SYMBOLIC_LINK_ENV: &str = "CAPYIO_CAMERA_LAB_SYMBOLIC_LINK";
    const VIRTUAL_CAMERA_SYMBOLIC_LINK_PREFIX: &str = r"\\?\SWD#VCAMDEVAPI#";

    pub(super) type LabResult<T = ()> = Result<T, Box<dyn Error>>;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum LabCommand {
        Preflight,
        Roundtrip,
        SharedRoundtrip,
        GuiHold,
        LiveHold,
        TrustedLanLiveHold {
            bind_ip: Ipv4Addr,
            peer_ip: Ipv4Addr,
        },
        ConsumerProbe,
        Cleanup,
    }

    pub(super) fn run() -> LabResult {
        match parse_command(std::env::args().skip(1))? {
            LabCommand::Preflight => preflight(),
            LabCommand::Roundtrip => roundtrip(),
            LabCommand::SharedRoundtrip => shared_roundtrip(),
            LabCommand::GuiHold => gui_hold(),
            LabCommand::LiveHold => live_hold(LiveTransport::AdbReverse),
            LabCommand::TrustedLanLiveHold { bind_ip, peer_ip } => {
                live_hold(LiveTransport::TrustedLan { bind_ip, peer_ip })
            }
            LabCommand::ConsumerProbe => consumer_probe(),
            LabCommand::Cleanup => cleanup(),
        }
    }

    fn parse_command(mut arguments: impl Iterator<Item = String>) -> LabResult<LabCommand> {
        let command = arguments
            .next()
            .ok_or_else(|| lab_error("expected a closed camera lab command"))?;
        if command == "trusted-lan-live-hold" {
            let bind_ip = parse_trusted_lan_ipv4(
                &arguments.next().ok_or_else(|| {
                    lab_error("trusted-lan-live-hold requires bind and peer IPv4 literals")
                })?,
                "trusted-lan-live-hold bind",
            )?;
            let peer_ip = parse_trusted_lan_ipv4(
                &arguments.next().ok_or_else(|| {
                    lab_error("trusted-lan-live-hold requires a peer IPv4 literal")
                })?,
                "trusted-lan-live-hold peer",
            )?;
            if arguments.next().is_some() {
                return Err(lab_error("unexpected extra trusted-lan-live-hold argument"));
            }
            if bind_ip == peer_ip {
                return Err(lab_error(
                    "trusted-lan-live-hold bind and peer IPv4 literals must differ",
                ));
            }
            return Ok(LabCommand::TrustedLanLiveHold { bind_ip, peer_ip });
        }
        if arguments.next().is_some() {
            return Err(lab_error("unexpected extra argument"));
        }
        match command.as_str() {
            "preflight" => Ok(LabCommand::Preflight),
            "roundtrip" => Ok(LabCommand::Roundtrip),
            "shared-roundtrip" => Ok(LabCommand::SharedRoundtrip),
            "gui-hold" => Ok(LabCommand::GuiHold),
            "live-hold" => Ok(LabCommand::LiveHold),
            "consumer-probe" => Ok(LabCommand::ConsumerProbe),
            "cleanup" => Ok(LabCommand::Cleanup),
            _ => Err(lab_error(
                "unknown command; expected preflight, roundtrip, shared-roundtrip, gui-hold, live-hold, trusted-lan-live-hold, consumer-probe, or cleanup",
            )),
        }
    }

    fn parse_trusted_lan_ipv4(value: &str, label: &str) -> LabResult<Ipv4Addr> {
        if value.len() > 15 {
            return Err(lab_error(format!("{label} IPv4 literal is too long")));
        }
        let address: Ipv4Addr = value
            .parse()
            .map_err(|_| lab_error(format!("{label} must be a canonical IPv4 literal")))?;
        if address.to_string() != value || !is_trusted_lan_ipv4(address) {
            return Err(lab_error(format!(
                "{label} must be canonical RFC1918, link-local or 100.64.0.0/10 IPv4"
            )));
        }
        Ok(address)
    }

    fn is_trusted_lan_ipv4(address: Ipv4Addr) -> bool {
        let [first, second, _, _] = address.octets();
        first == 10
            || (first == 172 && (16..=31).contains(&second))
            || (first == 192 && second == 168)
            || (first == 100 && (64..=127).contains(&second))
            || (first == 169 && second == 254)
    }

    fn preflight() -> LabResult {
        let _media_foundation = MediaFoundationRuntime::startup()?;
        let plan = MfVirtualCameraPlan::capyio_fixture();
        let mut backend = WindowsMfVirtualCameraBackend::default();
        backend.prepare(&plan)?;
        let existing = backend.symbolic_link().is_some();
        backend.shutdown()?;
        println!("preflight=pass");
        println!("scope=session_current_user");
        println!("existing_registration={existing}");
        Ok(())
    }

    fn cleanup() -> LabResult {
        let _media_foundation = MediaFoundationRuntime::startup()?;
        let plan = MfVirtualCameraPlan::capyio_fixture();
        let mut backend = WindowsMfVirtualCameraBackend::default();
        backend.prepare(&plan)?;
        let removed = backend.symbolic_link().is_some();
        if removed {
            backend.remove()?;
        }
        backend.shutdown()?;
        println!("cleanup=pass");
        println!("removed_existing_registration={removed}");
        Ok(())
    }

    fn roundtrip() -> LabResult {
        let evidence = with_started_camera(validate_started_camera)?;
        println!("roundtrip=pass");
        println!("enumerated_matches={}", evidence.enumerated_matches);
        println!("friendly_name={}", evidence.friendly_name);
        println!("sample_bytes={}", evidence.sample_bytes);
        println!("sample_duration_100ns={}", evidence.sample_duration_100ns);
        println!("sample_delta_100ns={}", evidence.sample_delta_100ns);
        println!("first_luma={}", evidence.first_luma);
        println!("cleanup=pass");
        Ok(())
    }

    fn shared_roundtrip() -> LabResult {
        let evidence = with_started_camera(validate_shared_consumers)?;
        println!("shared_roundtrip=pass");
        println!("enumerated_matches={}", evidence.enumerated_matches);
        println!("friendly_name={}", evidence.friendly_name);
        println!("external_consumers={}", evidence.external_consumers);
        println!("cleanup=pass");
        Ok(())
    }

    fn gui_hold() -> LabResult {
        with_started_camera(|backend| {
            let symbolic_link = backend.symbolic_link().ok_or_else(|| {
                stage_error("gui_hold_identity", "started camera has no symbolic link")
            })?;
            let inventory = enumerate_exact_camera(symbolic_link)
                .map_err(|error| stage_error("gui_hold_enumerate", error))?;
            if inventory.matches != 1 {
                return Err(stage_error(
                    "gui_hold_enumerate",
                    format!(
                        "expected one exact camera match, found {}",
                        inventory.matches
                    ),
                ));
            }

            println!("gui_hold_ready=pass");
            println!("enumerated_matches={}", inventory.matches);
            println!(
                "friendly_name={}",
                inventory.friendly_name.as_deref().unwrap_or("<missing>")
            );
            println!("hold_seconds={}", GUI_HOLD_DURATION.as_secs());
            io::stdout()
                .flush()
                .map_err(|error| stage_error("gui_hold_flush", error))?;
            thread::sleep(GUI_HOLD_DURATION);
            Ok(())
        })?;
        println!("gui_hold=pass");
        println!("cleanup=pass");
        Ok(())
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum LiveTransport {
        AdbReverse,
        TrustedLan {
            bind_ip: Ipv4Addr,
            peer_ip: Ipv4Addr,
        },
    }

    fn live_hold(transport: LiveTransport) -> LabResult {
        ensure_live_hold_preflight()?;
        let receiver_path = live_receiver_path()?;
        let child = Command::new(&receiver_path)
            .args(live_receiver_arguments(transport))
            .spawn()
            .map_err(|error| stage_error("live_receiver_spawn", error))?;
        let mut receiver = ManagedChild::new(0, child);

        let validation = (|| {
            wait_for_live_mapping(&mut receiver)?;
            with_started_camera(|backend| validate_live_hold(backend, &mut receiver))
        })();
        let receiver_cleanup = receiver
            .terminate_and_reap()
            .map_err(|error| stage_error("live_receiver_cleanup", error));
        let mapping_cleanup = wait_for_live_mapping_removal();

        finish_live_hold(validation, receiver_cleanup, mapping_cleanup)?;
        println!("live_hold=pass");
        println!("receiver_cleanup=pass");
        println!("cleanup=pass");
        Ok(())
    }

    fn ensure_live_hold_preflight() -> LabResult {
        match CameraSharedIngressConsumer::open_current() {
            Ok(_) => {
                return Err(stage_error(
                    "live_preflight_mapping",
                    "refusing to reuse an existing production camera mapping",
                ));
            }
            Err(error) if mapping_is_absent(&error) => {}
            Err(error) => return Err(stage_error("live_preflight_mapping", error)),
        }

        let _media_foundation = MediaFoundationRuntime::startup()?;
        let plan = MfVirtualCameraPlan::capyio_fixture();
        let mut backend = WindowsMfVirtualCameraBackend::default();
        backend
            .prepare(&plan)
            .map_err(|error| stage_error("live_preflight_registration", error))?;
        let existing = backend.symbolic_link().is_some();
        backend
            .shutdown()
            .map_err(|error| stage_error("live_preflight_registration", error))?;
        if existing {
            return Err(stage_error(
                "live_preflight_registration",
                "refusing to reuse an existing CapyIO Camera registration",
            ));
        }
        println!("live_preflight=pass");
        Ok(())
    }

    fn live_receiver_path() -> LabResult<PathBuf> {
        let current = std::env::current_exe()
            .map_err(|error| stage_error("live_receiver_identity", error))?;
        let receiver = current.with_file_name(LIVE_RECEIVER_EXECUTABLE);
        if !receiver.is_file() {
            return Err(stage_error(
                "live_receiver_identity",
                format!("fixed sibling receiver is missing: {}", receiver.display()),
            ));
        }
        Ok(receiver)
    }

    fn live_receiver_arguments(transport: LiveTransport) -> Vec<String> {
        let mut arguments = vec![
            "--max-access-units".to_owned(),
            "7200".to_owned(),
            "--publish-shared".to_owned(),
            "--reconnect-grace-millis".to_owned(),
            LIVE_RECONNECT_GRACE_MILLIS.to_string(),
        ];
        if let LiveTransport::TrustedLan { bind_ip, peer_ip } = transport {
            arguments.extend([
                "--trusted-lan-bind".to_owned(),
                bind_ip.to_string(),
                "--trusted-lan-peer".to_owned(),
                peer_ip.to_string(),
            ]);
        }
        arguments
    }

    fn finish_live_hold(
        validation: LabResult,
        receiver_cleanup: LabResult,
        mapping_cleanup: LabResult,
    ) -> LabResult {
        if let Err(error) = receiver_cleanup {
            return Err(stage_error(
                "cleanup",
                format!("receiver cleanup failed: {error}"),
            ));
        }
        if let Err(error) = mapping_cleanup {
            return Err(stage_error(
                "cleanup",
                format!("mapping cleanup failed: {error}"),
            ));
        }
        validation
    }

    fn wait_for_live_mapping(receiver: &mut ManagedChild) -> LabResult {
        let deadline = Instant::now() + LIVE_RECEIVER_START_TIMEOUT;
        loop {
            if let Some(status) = receiver
                .try_reap()
                .map_err(|error| stage_error("live_receiver_exit", error))?
            {
                return Err(stage_error(
                    "live_receiver_exit",
                    format!(
                        "receiver exited before mapping readiness with {}",
                        describe_exit_status(status)
                    ),
                ));
            }
            match CameraSharedIngressConsumer::open_current() {
                Ok(consumer) => {
                    println!("live_mapping_ready=pass");
                    println!("stream_id={:?}", consumer.stream_id());
                    println!("stream_epoch={}", consumer.stream_epoch());
                    return Ok(());
                }
                Err(error) if mapping_is_absent(&error) => {}
                Err(error) => return Err(stage_error("live_mapping_open", error)),
            }
            if Instant::now() >= deadline {
                return Err(stage_error(
                    "live_mapping_timeout",
                    format!(
                        "receiver did not publish the fixed mapping within {} seconds",
                        LIVE_RECEIVER_START_TIMEOUT.as_secs()
                    ),
                ));
            }
            thread::sleep(LIVE_RECEIVER_POLL_INTERVAL);
        }
    }

    fn validate_live_hold(
        backend: &WindowsMfVirtualCameraBackend,
        receiver: &mut ManagedChild,
    ) -> LabResult {
        let symbolic_link = backend.symbolic_link().ok_or_else(|| {
            stage_error("live_hold_identity", "started camera has no symbolic link")
        })?;
        let inventory = enumerate_exact_camera(symbolic_link)
            .map_err(|error| stage_error("live_hold_enumerate", error))?;
        if inventory.matches != 1 {
            return Err(stage_error(
                "live_hold_enumerate",
                format!(
                    "expected one exact camera match, found {}",
                    inventory.matches
                ),
            ));
        }
        println!("live_hold_ready=pass");
        println!("enumerated_matches={}", inventory.matches);
        println!(
            "friendly_name={}",
            inventory.friendly_name.as_deref().unwrap_or("<missing>")
        );
        println!("hold_seconds={}", GUI_HOLD_DURATION.as_secs());
        io::stdout()
            .flush()
            .map_err(|error| stage_error("live_hold_flush", error))?;

        let deadline = Instant::now() + GUI_HOLD_DURATION;
        while Instant::now() < deadline {
            if let Some(status) = receiver
                .try_reap()
                .map_err(|error| stage_error("live_receiver_exit", error))?
            {
                return Err(stage_error(
                    "live_receiver_exit",
                    format!(
                        "receiver exited during hold with {}",
                        describe_exit_status(status)
                    ),
                ));
            }
            CameraSharedIngressConsumer::open_current()
                .map_err(|error| stage_error("live_mapping_lost", error))?;
            thread::sleep(LIVE_RECEIVER_POLL_INTERVAL);
        }
        Ok(())
    }

    fn wait_for_live_mapping_removal() -> LabResult {
        let deadline = Instant::now() + LIVE_RECEIVER_STOP_TIMEOUT;
        loop {
            match CameraSharedIngressConsumer::open_current() {
                Err(error) if mapping_is_absent(&error) => return Ok(()),
                Ok(_) => {}
                Err(error) => return Err(stage_error("live_mapping_cleanup", error)),
            }
            if Instant::now() >= deadline {
                return Err(stage_error(
                    "live_mapping_cleanup",
                    "production camera mapping remained after receiver cleanup",
                ));
            }
            thread::sleep(LIVE_RECEIVER_POLL_INTERVAL);
        }
    }

    fn mapping_is_absent(error: &CameraSharedIngressError) -> bool {
        matches!(
            error,
            CameraSharedIngressError::Windows {
                operation: "OpenFileMappingW",
                code: 2
            }
        )
    }

    fn with_started_camera<T>(
        validate: impl FnOnce(&WindowsMfVirtualCameraBackend) -> LabResult<T>,
    ) -> LabResult<T> {
        let _media_foundation = MediaFoundationRuntime::startup()?;
        let plan = MfVirtualCameraPlan::capyio_fixture();
        let mut registrar =
            MfVirtualCameraRegistrar::new(plan, WindowsMfVirtualCameraBackend::default());
        registrar
            .prepare()
            .map_err(|error| stage_error("prepare", error))?;
        registrar
            .start()
            .map_err(|error| stage_error("start", error))?;

        let validation = validate(registrar.backend());
        let stop_error = registrar.stop().err().map(|error| error.to_string());
        let shutdown_error = registrar.shutdown().err().map(|error| error.to_string());
        if let Some(error) = stop_error.as_ref() {
            eprintln!("stop_error={error}");
        }
        if let Some(error) = shutdown_error.as_ref() {
            eprintln!("shutdown_error={error}");
        }
        if stop_error.is_some() || shutdown_error.is_some() {
            return Err(stage_error(
                "cleanup",
                "virtual-camera cleanup did not complete",
            ));
        }

        validation
    }

    struct RoundtripEvidence {
        enumerated_matches: usize,
        friendly_name: String,
        sample_bytes: u32,
        sample_duration_100ns: i64,
        sample_delta_100ns: i64,
        first_luma: u8,
    }

    struct SharedRoundtripEvidence {
        enumerated_matches: usize,
        friendly_name: String,
        external_consumers: usize,
    }

    fn validate_started_camera(
        backend: &WindowsMfVirtualCameraBackend,
    ) -> LabResult<RoundtripEvidence> {
        let symbolic_link = backend
            .symbolic_link()
            .ok_or_else(|| lab_error("started virtual camera has no symbolic link"))?;
        let inventory = enumerate_exact_camera(symbolic_link)
            .map_err(|error| stage_error("enumerate", error))?;
        if inventory.matches != 1 {
            return Err(stage_error(
                "enumerate",
                format!(
                    "expected exactly one enumerated symbolic-link match, found {}",
                    inventory.matches
                ),
            ));
        }

        let source = backend
            .get_media_source()
            .map_err(|error| stage_error("get_media_source", error))?;
        let frame_evidence = validate_frame_delivery(&source)?;
        Ok(RoundtripEvidence {
            enumerated_matches: inventory.matches,
            friendly_name: inventory
                .friendly_name
                .ok_or_else(|| stage_error("enumerate", "matching camera has no friendly name"))?,
            sample_bytes: frame_evidence.sample_bytes,
            sample_duration_100ns: frame_evidence.sample_duration_100ns,
            sample_delta_100ns: frame_evidence.sample_delta_100ns,
            first_luma: frame_evidence.first_luma,
        })
    }

    fn validate_shared_consumers(
        backend: &WindowsMfVirtualCameraBackend,
    ) -> LabResult<SharedRoundtripEvidence> {
        let symbolic_link = backend
            .symbolic_link()
            .ok_or_else(|| lab_error("started virtual camera has no symbolic link"))?;
        let inventory = enumerate_exact_camera(symbolic_link)
            .map_err(|error| stage_error("enumerate", error))?;
        if inventory.matches != 1 {
            return Err(stage_error(
                "enumerate",
                format!(
                    "expected exactly one enumerated symbolic-link match, found {}",
                    inventory.matches
                ),
            ));
        }
        let friendly_name = inventory
            .friendly_name
            .ok_or_else(|| stage_error("enumerate", "matching camera has no friendly name"))?;
        let external_consumers = run_external_consumers(symbolic_link)?;
        Ok(SharedRoundtripEvidence {
            enumerated_matches: inventory.matches,
            friendly_name,
            external_consumers,
        })
    }

    fn run_external_consumers(symbolic_link: &str) -> LabResult<usize> {
        let executable =
            std::env::current_exe().map_err(|error| stage_error("share_consumer_spawn", error))?;
        let mut completed = 0_usize;
        for index in 0..EXTERNAL_CONSUMER_COUNT {
            let child = Command::new(&executable)
                .arg("consumer-probe")
                .env(CONSUMER_SYMBOLIC_LINK_ENV, symbolic_link)
                .spawn()
                .map_err(|error| {
                    stage_error("share_consumer_spawn", format!("consumer {index}: {error}"))
                })?;
            let mut consumer = ManagedChild::new(index, child);
            wait_for_external_consumer(&mut consumer)?;
            completed += 1;
        }
        Ok(completed)
    }

    fn wait_for_external_consumer(consumer: &mut ManagedChild) -> LabResult {
        let deadline = Instant::now() + EXTERNAL_CONSUMER_TIMEOUT;
        loop {
            if let Some(status) = consumer
                .try_reap()
                .map_err(|error| stage_error("share_consumer_exit", error))?
            {
                if status.success() {
                    return Ok(());
                }
                return Err(stage_error(
                    "share_consumer_exit",
                    format!(
                        "consumer {} exited with {}",
                        consumer.index,
                        describe_exit_status(status)
                    ),
                ));
            }
            if Instant::now() >= deadline {
                return Err(stage_error(
                    "share_consumer_timeout",
                    format!(
                        "consumer {} did not complete within {} seconds",
                        consumer.index,
                        EXTERNAL_CONSUMER_TIMEOUT.as_secs()
                    ),
                ));
            }
            thread::sleep(EXTERNAL_CONSUMER_POLL_INTERVAL);
        }
    }

    struct ManagedChild {
        index: usize,
        child: Option<Child>,
    }

    impl ManagedChild {
        fn new(index: usize, child: Child) -> Self {
            Self {
                index,
                child: Some(child),
            }
        }

        fn try_reap(&mut self) -> io::Result<Option<ExitStatus>> {
            let Some(child) = self.child.as_mut() else {
                return Ok(None);
            };
            if child.try_wait()?.is_none() {
                return Ok(None);
            }
            let mut child = self.child.take().expect("checked above");
            child.wait().map(Some)
        }

        fn terminate_and_reap(&mut self) -> io::Result<()> {
            let Some(mut child) = self.child.take() else {
                return Ok(());
            };
            if child.try_wait()?.is_none() {
                child.kill()?;
            }
            child.wait()?;
            Ok(())
        }
    }

    impl Drop for ManagedChild {
        fn drop(&mut self) {
            if let Some(mut child) = self.child.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }

    fn describe_exit_status(status: ExitStatus) -> String {
        status.code().map_or_else(
            || "no exit code".to_owned(),
            |code| format!("exit code {code}"),
        )
    }

    fn consumer_probe() -> LabResult {
        let _media_foundation = MediaFoundationRuntime::startup()?;
        let symbolic_link = validated_consumer_symbolic_link()?;
        let activate = wait_for_exact_camera(&symbolic_link)?;
        let source: IMFMediaSource = unsafe { activate.ActivateObject() }
            .map_err(|error| stage_error("consumer_activate", error))?;
        let validation = validate_frame_delivery(&source);
        let shutdown = unsafe { activate.ShutdownObject() }
            .map_err(|error| stage_error("consumer_shutdown", error));
        let evidence = validation?;
        shutdown?;
        println!("consumer_probe=pass");
        println!("sample_bytes={}", evidence.sample_bytes);
        println!("sample_duration_100ns={}", evidence.sample_duration_100ns);
        println!("sample_delta_100ns={}", evidence.sample_delta_100ns);
        println!("first_luma={}", evidence.first_luma);
        Ok(())
    }

    fn validated_consumer_symbolic_link() -> LabResult<String> {
        let symbolic_link = std::env::var(CONSUMER_SYMBOLIC_LINK_ENV).map_err(|_| {
            stage_error(
                "consumer_identity_missing",
                "missing internal symbolic link",
            )
        })?;
        if !consumer_symbolic_link_is_valid(&symbolic_link) {
            return Err(stage_error(
                "consumer_identity_shape",
                "internal symbolic link is outside the closed virtual-camera shape",
            ));
        }
        Ok(symbolic_link)
    }

    fn consumer_symbolic_link_is_valid(symbolic_link: &str) -> bool {
        let utf16_length = symbolic_link.encode_utf16().count();
        let has_valid_prefix = symbolic_link
            .get(..VIRTUAL_CAMERA_SYMBOLIC_LINK_PREFIX.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(VIRTUAL_CAMERA_SYMBOLIC_LINK_PREFIX));
        utf16_length > 0
            && utf16_length <= MAX_DEVICE_ATTRIBUTE_UTF16
            && !symbolic_link.chars().any(char::is_control)
            && has_valid_prefix
    }

    fn wait_for_exact_camera(symbolic_link: &str) -> LabResult<IMFActivate> {
        for attempt in 0..CONSUMER_ENUMERATION_ATTEMPTS {
            let inventory = enumerate_exact_camera(symbolic_link)
                .map_err(|error| stage_error("consumer_enumerate_api", error))?;
            if inventory.matches > 1 {
                return Err(stage_error(
                    "consumer_duplicate",
                    format!(
                        "expected at most one exact symbolic-link match, found {}",
                        inventory.matches
                    ),
                ));
            }
            if let Some(activate) = inventory.activate {
                return Ok(activate);
            }
            if attempt + 1 < CONSUMER_ENUMERATION_ATTEMPTS {
                thread::sleep(CONSUMER_ENUMERATION_RETRY_DELAY);
            }
        }
        Err(stage_error(
            "consumer_not_found",
            format!(
                "exact CapyIO camera did not appear after {CONSUMER_ENUMERATION_ATTEMPTS} attempts"
            ),
        ))
    }

    struct CameraInventoryMatch {
        matches: usize,
        friendly_name: Option<String>,
        activate: Option<IMFActivate>,
    }

    fn enumerate_exact_camera(symbolic_link: &str) -> LabResult<CameraInventoryMatch> {
        let array = enumerate_video_capture_activations()?;
        let mut matches = 0_usize;
        let mut friendly_name = None;
        let mut matching_activate = None;
        for activate in array.iter() {
            let attributes: IMFAttributes = activate.cast()?;
            let candidate = read_attribute_string(
                &attributes,
                &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
            )?;
            if device_symbolic_links_equal(&candidate, symbolic_link) {
                matches += 1;
                friendly_name = Some(read_attribute_string(
                    &attributes,
                    &MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME,
                )?);
                matching_activate = Some(activate.clone());
            }
        }
        Ok(CameraInventoryMatch {
            matches,
            friendly_name,
            activate: matching_activate,
        })
    }

    fn enumerate_video_capture_activations() -> LabResult<ActivateArray> {
        let mut attributes = None;
        unsafe { MFCreateAttributes(&mut attributes, 1)? };
        let attributes = attributes.ok_or_else(|| lab_error("MFCreateAttributes returned null"))?;
        unsafe {
            attributes.SetGUID(
                &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
                &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
            )?;
        }

        let mut raw = ptr::null_mut();
        let mut count = 0_u32;
        unsafe { MFEnumDeviceSources(&attributes, &mut raw, &mut count)? };
        let array = ActivateArray::new(raw, count as usize)?;
        if array.len > MAX_ENUMERATED_CAMERAS {
            return Err(lab_error(format!(
                "camera inventory {} exceeds bound {}",
                array.len, MAX_ENUMERATED_CAMERAS
            )));
        }
        Ok(array)
    }

    fn device_symbolic_links_equal(left: &str, right: &str) -> bool {
        left.eq_ignore_ascii_case(right)
    }

    struct ActivateArray {
        raw: *mut Option<IMFActivate>,
        len: usize,
    }

    impl ActivateArray {
        fn new(raw: *mut Option<IMFActivate>, len: usize) -> LabResult<Self> {
            if len > 0 && raw.is_null() {
                return Err(lab_error(
                    "MFEnumDeviceSources returned a null non-empty array",
                ));
            }
            Ok(Self { raw, len })
        }

        fn iter(&self) -> impl Iterator<Item = &IMFActivate> {
            let slice: &[Option<IMFActivate>] = if self.len == 0 {
                &[]
            } else {
                unsafe { std::slice::from_raw_parts(self.raw, self.len) }
            };
            slice.iter().filter_map(Option::as_ref)
        }
    }

    impl Drop for ActivateArray {
        fn drop(&mut self) {
            if !self.raw.is_null() {
                for index in 0..self.len {
                    unsafe { ptr::drop_in_place(self.raw.add(index)) };
                }
                unsafe { CoTaskMemFree(Some(self.raw.cast::<c_void>())) };
            }
        }
    }

    fn read_attribute_string(
        attributes: &IMFAttributes,
        key: *const windows::core::GUID,
    ) -> LabResult<String> {
        let length = unsafe { attributes.GetStringLength(key)? } as usize;
        if length == 0 || length > MAX_DEVICE_ATTRIBUTE_UTF16 {
            return Err(lab_error(format!(
                "device attribute UTF-16 length {length} is outside the accepted bound"
            )));
        }
        let mut buffer = vec![0_u16; length + 1];
        let mut written = 0_u32;
        unsafe { attributes.GetString(key, &mut buffer, Some(&mut written))? };
        if written as usize != length || buffer[length] != 0 {
            return Err(lab_error("device attribute length changed during read"));
        }
        let value = String::from_utf16(&buffer[..length])?;
        if value.chars().any(char::is_control) {
            return Err(lab_error("device attribute contains control characters"));
        }
        Ok(value)
    }

    struct FrameEvidence {
        sample_bytes: u32,
        sample_duration_100ns: i64,
        sample_delta_100ns: i64,
        first_luma: u8,
    }

    fn validate_frame_delivery(source: &IMFMediaSource) -> LabResult<FrameEvidence> {
        (|| -> LabResult<FrameEvidence> {
            let reader =
                unsafe { MFCreateSourceReaderFromMediaSource(source, None::<&IMFAttributes>) }
                    .map_err(|error| stage_error("frame_source_start", error))?;
            let video_stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
            unsafe { reader.SetStreamSelection(video_stream, true) }
                .map_err(|error| stage_error("frame_stream_started", error))?;
            let first = read_source_reader_sample(&reader, video_stream)
                .map_err(|error| stage_error("frame_first_sample", error))?;
            let sample_bytes = unsafe { first.GetTotalLength() }
                .map_err(|error| stage_error("frame_validate_bytes", error))?;
            let required = u32::try_from(fixture_stream_spec().packed_frame_bytes().unwrap())
                .map_err(|error| stage_error("frame_validate_bytes", error))?;
            if sample_bytes < required {
                return Err(stage_error(
                    "frame_validate_bytes",
                    format!("sample buffer {sample_bytes} is smaller than required {required}"),
                ));
            }
            let sample_duration_100ns = unsafe { first.GetSampleDuration() }
                .map_err(|error| stage_error("frame_validate_timing", error))?;
            let first_time = unsafe { first.GetSampleTime() }
                .map_err(|error| stage_error("frame_validate_timing", error))?;
            if sample_duration_100ns != 333_333 {
                return Err(stage_error(
                    "frame_validate_duration",
                    format!("unexpected 30 fps duration={sample_duration_100ns}"),
                ));
            }
            let first_luma = inspect_first_luma(&first)
                .map_err(|error| stage_error("frame_validate_content", error))?;
            let second = read_source_reader_sample(&reader, video_stream)
                .map_err(|error| stage_error("frame_second_sample", error))?;
            let second_time = unsafe { second.GetSampleTime() }
                .map_err(|error| stage_error("frame_validate_timing", error))?;
            let sample_delta_100ns = second_time - first_time;
            if sample_delta_100ns <= 0 {
                return Err(stage_error(
                    "frame_validate_delta_nonpositive",
                    format!("non-increasing sample delta={sample_delta_100ns}"),
                ));
            }
            if sample_delta_100ns > 10_000_000 {
                return Err(stage_error(
                    "frame_validate_delta_gap",
                    format!("sample delta exceeds one second: {sample_delta_100ns}"),
                ));
            }
            Ok(FrameEvidence {
                sample_bytes,
                sample_duration_100ns,
                sample_delta_100ns,
                first_luma,
            })
        })()
    }

    fn read_source_reader_sample(
        reader: &windows::Win32::Media::MediaFoundation::IMFSourceReader,
        stream_index: u32,
    ) -> LabResult<IMFSample> {
        let terminal_flags = (MF_SOURCE_READERF_ERROR.0 | MF_SOURCE_READERF_ENDOFSTREAM.0) as u32;
        let mut last_stream_flags = 0_u32;
        for _ in 0..MAX_SOURCE_READER_EMPTY_READS {
            let mut actual_stream_index = u32::MAX;
            let mut timestamp = 0_i64;
            let mut sample = None;
            unsafe {
                reader.ReadSample(
                    stream_index,
                    0,
                    Some(&mut actual_stream_index),
                    Some(&mut last_stream_flags),
                    Some(&mut timestamp),
                    Some(&mut sample),
                )
            }
            .map_err(|error| stage_hresult_error("source_reader_read", error))?;
            if last_stream_flags & terminal_flags != 0 {
                return Err(stage_error(
                    "source_reader_flags",
                    format!("source reader returned terminal flags 0x{last_stream_flags:08X}"),
                ));
            }
            if actual_stream_index != 0 {
                return Err(lab_error(format!(
                    "source reader returned stream {actual_stream_index}; expected 0"
                )));
            }
            if let Some(sample) = sample {
                return Ok(sample);
            }
        }
        Err(stage_error(
            "source_reader_empty",
            format!(
                "source reader returned no sample after {MAX_SOURCE_READER_EMPTY_READS} reads; last flags=0x{last_stream_flags:08X}"
            ),
        ))
    }

    fn inspect_first_luma(sample: &IMFSample) -> LabResult<u8> {
        let required = u32::try_from(fixture_stream_spec().packed_frame_bytes().unwrap())
            .map_err(|error| stage_error("content_layout", error))?;
        let buffer = unsafe { MFCreateMemoryBuffer(required) }
            .map_err(|error| stage_error("content_buffer", error))?;
        unsafe { sample.CopyToBuffer(&buffer) }
            .map_err(|error| stage_error("content_buffer", error))?;

        let mut bytes = ptr::null_mut();
        let mut maximum = 0_u32;
        let mut current = 0_u32;
        unsafe { buffer.Lock(&mut bytes, Some(&mut maximum), Some(&mut current)) }
            .map_err(|error| stage_error("content_buffer", error))?;
        let result = if bytes.is_null() || maximum < current || current < required {
            Err(stage_error(
                "content_layout",
                format!(
                    "invalid linear NV12 buffer maximum={maximum} current={current} required={required}"
                ),
            ))
        } else {
            validate_luma(unsafe { *bytes })
        };
        unsafe { buffer.Unlock() }.map_err(|error| stage_error("content_buffer", error))?;
        result
    }

    fn validate_luma(value: u8) -> LabResult<u8> {
        if (16..=235).contains(&value) {
            Ok(value)
        } else {
            Err(stage_error("content_luma", format!("value={value}")))
        }
    }

    fn lab_error(message: impl Into<String>) -> Box<dyn Error> {
        Box::new(io::Error::other(message.into()))
    }

    fn stage_error(stage: &str, error: impl std::fmt::Display) -> Box<dyn Error> {
        lab_error(format!("stage={stage}: {error}"))
    }

    fn stage_hresult_error(stage: &str, error: windows::core::Error) -> Box<dyn Error> {
        lab_error(format!("stage={stage}: hresult_i32={}", error.code().0))
    }

    pub(super) fn exit_code(error: &dyn Error) -> i32 {
        let message = error.to_string();
        if message.contains("stage=source_reader_read:") {
            message
                .rsplit("hresult_i32=")
                .next()
                .and_then(|value| value.parse::<i32>().ok())
                .unwrap_or(60)
        } else if message.contains("stage=source_reader_flags:") {
            61
        } else if message.contains("stage=source_reader_empty:") {
            62
        } else if message.contains("stage=frame_validate_bytes:") {
            63
        } else if message.contains("stage=frame_validate_timing:") {
            64
        } else if message.contains("stage=frame_validate_duration:") {
            68
        } else if message.contains("stage=frame_validate_delta_nonpositive:") {
            69
        } else if message.contains("stage=frame_validate_delta_gap:") {
            70
        } else if message.contains("stage=frame_validate_content:") {
            if let Some(value) = message
                .split("stage=content_luma: value=")
                .nth(1)
                .and_then(|value| value.parse::<i32>().ok())
            {
                100 + value
            } else if message.contains("stage=content_layout:") {
                66
            } else if message.contains("stage=content_buffer:") {
                67
            } else {
                65
            }
        } else if message.starts_with("stage=share_consumer_spawn:") {
            71
        } else if message.starts_with("stage=share_consumer_exit:") {
            let child_code = message
                .rsplit("exit code ")
                .next()
                .and_then(|value| value.parse::<i32>().ok());
            match child_code {
                Some(value) if value < 0 => value,
                Some(value) if (1..=199).contains(&value) => 200 + value,
                _ => 72,
            }
        } else if message.starts_with("stage=share_consumer_timeout:") {
            73
        } else if message.starts_with("stage=consumer_enumerate_api:") {
            74
        } else if message.starts_with("stage=consumer_activate:") {
            75
        } else if message.starts_with("stage=consumer_shutdown:") {
            76
        } else if message.starts_with("stage=consumer_not_found:") {
            77
        } else if message.starts_with("stage=consumer_duplicate:") {
            78
        } else if message.starts_with("stage=consumer_identity_missing:") {
            80
        } else if message.starts_with("stage=consumer_identity_shape:") {
            81
        } else if message.starts_with("stage=prepare:") {
            9
        } else if message.starts_with("stage=start:") {
            10
        } else if message.starts_with("stage=enumerate:") {
            20
        } else if message.starts_with("stage=get_media_source:") {
            30
        } else if message.starts_with("stage=frame_create_presentation:") {
            41
        } else if message.starts_with("stage=frame_source_start:") {
            42
        } else if message.starts_with("stage=frame_stream_announcement:") {
            43
        } else if message.starts_with("stage=frame_stream_started:") {
            44
        } else if message.starts_with("stage=frame_source_started:") {
            45
        } else if message.starts_with("stage=frame_first_request:") {
            46
        } else if message.starts_with("stage=frame_first_sample:") {
            47
        } else if message.starts_with("stage=frame_second_request:") {
            48
        } else if message.starts_with("stage=frame_second_sample:") {
            49
        } else if message.starts_with("stage=frame_validate_sample:") {
            51
        } else if message.starts_with("stage=frame_cleanup:") {
            52
        } else if message.starts_with("stage=frame_delivery:") {
            40
        } else if message.starts_with("stage=cleanup:") {
            50
        } else {
            1
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{
            LabCommand, LiveTransport, consumer_symbolic_link_is_valid,
            device_symbolic_links_equal, exit_code, finish_live_hold, live_receiver_arguments,
            mapping_is_absent, parse_command, stage_error,
        };
        use capyio_windows_camera_share::CameraSharedIngressError;
        use std::net::Ipv4Addr;

        #[test]
        fn virtual_camera_symbolic_links_follow_windows_case_insensitive_semantics() {
            let upper = r"\\?\SWD#VCAMDEVAPI#ABC#{E5323777-F976-4F5B-9B55-B94699C46E44}";
            let lower = r"\\?\swd#vcamdevapi#abc#{e5323777-f976-4f5b-9b55-b94699c46e44}";
            assert!(device_symbolic_links_equal(upper, lower));
            assert!(!device_symbolic_links_equal(
                upper,
                r"\\?\swd#vcamdevapi#def"
            ));
        }

        #[test]
        fn command_contract_is_closed_and_trusted_lan_has_exact_parameters() {
            assert_eq!(
                parse_command(["shared-roundtrip".to_owned()].into_iter()).unwrap(),
                LabCommand::SharedRoundtrip
            );
            assert_eq!(
                parse_command(["consumer-probe".to_owned()].into_iter()).unwrap(),
                LabCommand::ConsumerProbe
            );
            assert_eq!(
                parse_command(["gui-hold".to_owned()].into_iter()).unwrap(),
                LabCommand::GuiHold
            );
            assert_eq!(
                parse_command(["live-hold".to_owned()].into_iter()).unwrap(),
                LabCommand::LiveHold
            );
            assert_eq!(
                parse_command(
                    [
                        "trusted-lan-live-hold".to_owned(),
                        "100.70.0.1".to_owned(),
                        "100.70.0.2".to_owned(),
                    ]
                    .into_iter()
                )
                .unwrap(),
                LabCommand::TrustedLanLiveHold {
                    bind_ip: Ipv4Addr::new(100, 70, 0, 1),
                    peer_ip: Ipv4Addr::new(100, 70, 0, 2),
                }
            );
            assert!(
                parse_command(["shared-roundtrip".to_owned(), "unexpected".to_owned()].into_iter())
                    .is_err()
            );
            assert!(parse_command(["unknown".to_owned()].into_iter()).is_err());
            for arguments in [
                vec!["trusted-lan-live-hold", "0.0.0.0", "192.168.1.20"],
                vec!["trusted-lan-live-hold", "192.168.1.10", "8.8.8.8"],
                vec!["trusted-lan-live-hold", "192.168.1.10", "192.168.1.10"],
            ] {
                assert!(
                    parse_command(arguments.into_iter().map(str::to_owned)).is_err(),
                    "unexpectedly accepted a non-closed trusted-LAN command"
                );
            }
            assert_eq!(
                live_receiver_arguments(LiveTransport::AdbReverse),
                [
                    "--max-access-units",
                    "7200",
                    "--publish-shared",
                    "--reconnect-grace-millis",
                    "60000",
                ]
                .map(str::to_owned)
            );
            assert_eq!(
                live_receiver_arguments(LiveTransport::TrustedLan {
                    bind_ip: Ipv4Addr::new(192, 168, 1, 10),
                    peer_ip: Ipv4Addr::new(192, 168, 1, 20),
                }),
                [
                    "--max-access-units",
                    "7200",
                    "--publish-shared",
                    "--reconnect-grace-millis",
                    "60000",
                    "--trusted-lan-bind",
                    "192.168.1.10",
                    "--trusted-lan-peer",
                    "192.168.1.20",
                ]
                .map(str::to_owned)
            );
        }

        #[test]
        fn live_mapping_wait_retries_only_exact_not_found() {
            assert!(mapping_is_absent(&CameraSharedIngressError::Windows {
                operation: "OpenFileMappingW",
                code: 2,
            }));
            assert!(!mapping_is_absent(&CameraSharedIngressError::Windows {
                operation: "OpenFileMappingW",
                code: 5,
            }));
            assert!(!mapping_is_absent(&CameraSharedIngressError::InvalidLayout));
        }

        #[test]
        fn live_hold_never_hides_a_cleanup_failure() {
            let error = finish_live_hold(
                Err(stage_error("live_receiver_exit", "receiver ended early")),
                Ok(()),
                Err(stage_error("live_mapping_cleanup", "mapping remained")),
            )
            .unwrap_err();
            assert_eq!(
                error.to_string(),
                "stage=cleanup: mapping cleanup failed: stage=live_mapping_cleanup: mapping remained"
            );

            let validation = finish_live_hold(
                Err(stage_error("live_receiver_exit", "receiver ended early")),
                Ok(()),
                Ok(()),
            )
            .unwrap_err();
            assert_eq!(
                validation.to_string(),
                "stage=live_receiver_exit: receiver ended early"
            );
        }

        #[test]
        fn parent_exit_code_retains_the_external_consumer_stage() {
            let error = stage_error("share_consumer_exit", "consumer 1 exited with exit code 74");
            assert_eq!(exit_code(error.as_ref()), 274);
            let hresult = stage_error(
                "share_consumer_exit",
                "consumer 0 exited with exit code -1072875854",
            );
            assert_eq!(exit_code(hresult.as_ref()), -1072875854);
        }

        #[test]
        fn child_identity_rejects_non_virtual_camera_links() {
            assert!(consumer_symbolic_link_is_valid(
                r"\\?\SWD#VCAMDEVAPI#ABC#{E5323777-F976-4F5B-9B55-B94699C46E44}"
            ));
            assert!(!consumer_symbolic_link_is_valid(r"\\?\USB#VID_1234"));
            assert!(!consumer_symbolic_link_is_valid(
                "\\\\?\\SWD#VCAMDEVAPI#ABC\n"
            ));
        }
    }
}

#[cfg(windows)]
fn main() {
    if let Err(error) = windows_lab::run() {
        eprintln!("camera_lab_error={error}");
        std::process::exit(windows_lab::exit_code(error.as_ref()));
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("capyio-camera-virtual-lab is available only on Windows");
    std::process::exit(1);
}
