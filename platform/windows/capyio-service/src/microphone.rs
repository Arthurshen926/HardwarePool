use std::net::SocketAddr;

use capyio_micyou_adapter::{MicYouError, MicYouSupervisor, PeerTcpPresence, SupervisorStatus};
use serde::{Deserialize, Serialize};

pub const DEFAULT_STABLE_PHONE_POLLS: u8 = 3;
pub const DEFAULT_PHONE_WAIT_POLLS: u16 = 120;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MicrophoneHostState {
    Stopped,
    WaitingForPhone,
    Active,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MicrophoneHostSnapshot {
    pub schema_version: u8,
    pub state: MicrophoneHostState,
    pub generation: u64,
    pub phone_present: bool,
    pub bind_address: SocketAddr,
    pub problem_code: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MicrophoneHostStartError {
    ConfiguredEndpointUnavailable,
    Other(String),
}

pub trait MicrophoneHostProcess {
    fn start(&mut self) -> Result<(), MicrophoneHostStartError>;
    fn running(&mut self) -> Result<bool, String>;
    fn phone_present(&mut self) -> Result<bool, String>;
    fn stop(&mut self) -> Result<(), String>;
}

impl MicrophoneHostProcess for MicYouSupervisor {
    fn start(&mut self) -> Result<(), MicrophoneHostStartError> {
        MicYouSupervisor::start(self).map(|_| ()).map_err(|error| {
            if matches!(
                error,
                MicYouError::ConfiguredDeviceMissing
                    | MicYouError::ConfiguredDeviceChanged
                    | MicYouError::DuplicateDeviceId
            ) {
                MicrophoneHostStartError::ConfiguredEndpointUnavailable
            } else {
                MicrophoneHostStartError::Other(error.to_string())
            }
        })
    }

    fn running(&mut self) -> Result<bool, String> {
        MicYouSupervisor::status(self)
            .map(|status| matches!(status, SupervisorStatus::Running { .. }))
            .map_err(|error| error.to_string())
    }

    fn phone_present(&mut self) -> Result<bool, String> {
        MicYouSupervisor::peer_tcp_presence(self)
            .map(|presence| matches!(presence, PeerTcpPresence::Established { connection_count } if connection_count > 0))
            .map_err(|error| error.to_string())
    }

    fn stop(&mut self) -> Result<(), String> {
        MicYouSupervisor::stop(self)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

pub struct MicrophoneHostRuntime<P> {
    process: P,
    bind_address: SocketAddr,
    state: MicrophoneHostState,
    generation: u64,
    required_phone_polls: u8,
    stable_phone_polls: u8,
    required_phone_wait_polls: u16,
    phone_wait_polls: u16,
    problem_code: Option<&'static str>,
}

impl<P: MicrophoneHostProcess> MicrophoneHostRuntime<P> {
    pub fn new(
        process: P,
        bind_address: SocketAddr,
        required_phone_polls: u8,
        required_phone_wait_polls: u16,
    ) -> Result<Self, String> {
        if !matches!(bind_address.ip(), std::net::IpAddr::V4(ip) if !ip.is_unspecified())
            || bind_address.port() == 0
        {
            return Err("microphone host requires a non-unspecified IPv4 address".to_owned());
        }
        if required_phone_polls == 0 || required_phone_wait_polls < u16::from(required_phone_polls)
        {
            return Err("microphone host phone polling bounds are invalid".to_owned());
        }
        Ok(Self {
            process,
            bind_address,
            state: MicrophoneHostState::Stopped,
            generation: 0,
            required_phone_polls,
            stable_phone_polls: 0,
            required_phone_wait_polls,
            phone_wait_polls: 0,
            problem_code: None,
        })
    }

    pub fn ensure_started(&mut self) -> Result<MicrophoneHostSnapshot, String> {
        match self.state {
            MicrophoneHostState::Stopped => self.start(),
            MicrophoneHostState::Failed => {
                self.ensure_stopped()?;
                self.start()
            }
            MicrophoneHostState::WaitingForPhone | MicrophoneHostState::Active => {
                Ok(self.snapshot())
            }
        }
    }

    pub fn poll(&mut self) -> MicrophoneHostSnapshot {
        if !matches!(
            self.state,
            MicrophoneHostState::WaitingForPhone | MicrophoneHostState::Active
        ) {
            return self.snapshot();
        }
        match self.process.running() {
            Ok(true) => {}
            Ok(false) | Err(_) => {
                self.fail("CAPY.MICROPHONE_HOST.PROCESS_EXITED");
                return self.snapshot();
            }
        }
        match self.process.phone_present() {
            Ok(true) => {
                self.stable_phone_polls = self
                    .stable_phone_polls
                    .saturating_add(1)
                    .min(self.required_phone_polls);
                if self.stable_phone_polls == self.required_phone_polls {
                    self.state = MicrophoneHostState::Active;
                    self.phone_wait_polls = 0;
                }
            }
            Ok(false) if self.state == MicrophoneHostState::Active => {
                self.fail("CAPY.MICROPHONE_HOST.PHONE_DISCONNECTED");
            }
            Ok(false) => {
                self.stable_phone_polls = 0;
                self.phone_wait_polls = self.phone_wait_polls.saturating_add(1);
                if self.phone_wait_polls >= self.required_phone_wait_polls {
                    self.fail("CAPY.MICROPHONE_HOST.PHONE_WAIT_EXHAUSTED");
                }
            }
            Err(_) => self.fail("CAPY.MICROPHONE_HOST.PHONE_OBSERVATION_FAILED"),
        }
        self.snapshot()
    }

    pub fn ensure_stopped(&mut self) -> Result<MicrophoneHostSnapshot, String> {
        if self.state == MicrophoneHostState::Stopped {
            return Ok(self.snapshot());
        }
        let result = self.process.stop();
        self.state = MicrophoneHostState::Stopped;
        self.reset_poll_state();
        self.problem_code = None;
        result.map(|()| self.snapshot())
    }

    #[must_use]
    pub fn snapshot(&self) -> MicrophoneHostSnapshot {
        MicrophoneHostSnapshot {
            schema_version: 1,
            state: self.state,
            generation: self.generation,
            phone_present: self.state == MicrophoneHostState::Active,
            bind_address: self.bind_address,
            problem_code: self.problem_code.map(str::to_owned),
        }
    }

    fn start(&mut self) -> Result<MicrophoneHostSnapshot, String> {
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| "microphone host generation exhausted".to_owned())?;
        self.reset_poll_state();
        match self.process.start() {
            Ok(()) => {
                self.state = MicrophoneHostState::WaitingForPhone;
                self.problem_code = None;
                Ok(self.snapshot())
            }
            Err(MicrophoneHostStartError::ConfiguredEndpointUnavailable) => {
                self.state = MicrophoneHostState::Failed;
                self.problem_code = Some("CAPY.MICROPHONE_HOST.ENDPOINT_UNAVAILABLE");
                Err("configured microphone ingress endpoint is unavailable".to_owned())
            }
            Err(MicrophoneHostStartError::Other(detail)) => {
                self.state = MicrophoneHostState::Failed;
                self.problem_code = Some("CAPY.MICROPHONE_HOST.PROCESS_START_FAILED");
                Err(detail)
            }
        }
    }

    fn fail(&mut self, problem_code: &'static str) {
        self.state = MicrophoneHostState::Failed;
        self.reset_poll_state();
        self.problem_code = Some(problem_code);
        let _ = self.process.stop();
    }

    fn reset_poll_state(&mut self) {
        self.stable_phone_polls = 0;
        self.phone_wait_polls = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FakeProcess {
        running: bool,
        phone: bool,
        starts: usize,
        stops: usize,
        start_error: Option<MicrophoneHostStartError>,
    }

    impl MicrophoneHostProcess for FakeProcess {
        fn start(&mut self) -> Result<(), MicrophoneHostStartError> {
            self.starts += 1;
            if let Some(error) = self.start_error.take() {
                return Err(error);
            }
            self.running = true;
            Ok(())
        }

        fn running(&mut self) -> Result<bool, String> {
            Ok(self.running)
        }

        fn phone_present(&mut self) -> Result<bool, String> {
            Ok(self.phone)
        }

        fn stop(&mut self) -> Result<(), String> {
            self.running = false;
            self.stops += 1;
            Ok(())
        }
    }

    fn runtime(process: FakeProcess) -> MicrophoneHostRuntime<FakeProcess> {
        MicrophoneHostRuntime::new(process, "100.64.0.10:8554".parse().expect("address"), 3, 5)
            .expect("runtime")
    }

    #[test]
    fn stable_phone_presence_activates_and_loss_fails_closed() {
        let mut runtime = runtime(FakeProcess::default());
        assert_eq!(
            runtime.ensure_started().expect("start").state,
            MicrophoneHostState::WaitingForPhone
        );
        runtime.process.phone = true;
        assert_eq!(runtime.poll().state, MicrophoneHostState::WaitingForPhone);
        assert_eq!(runtime.poll().state, MicrophoneHostState::WaitingForPhone);
        assert_eq!(runtime.poll().state, MicrophoneHostState::Active);
        runtime.process.phone = false;
        let snapshot = runtime.poll();
        assert_eq!(snapshot.state, MicrophoneHostState::Failed);
        assert_eq!(
            snapshot.problem_code.as_deref(),
            Some("CAPY.MICROPHONE_HOST.PHONE_DISCONNECTED")
        );
        assert_eq!(runtime.process.stops, 1);
    }

    #[test]
    fn wait_is_bounded_and_retry_advances_generation() {
        let mut runtime = runtime(FakeProcess::default());
        runtime.ensure_started().expect("start");
        for _ in 0..5 {
            runtime.poll();
        }
        assert_eq!(runtime.snapshot().state, MicrophoneHostState::Failed);
        let retried = runtime.ensure_started().expect("retry");
        assert_eq!(retried.generation, 2);
        assert_eq!(runtime.process.starts, 2);
        assert_eq!(runtime.process.stops, 2);
    }

    #[test]
    fn endpoint_failure_is_typed_and_private_details_are_not_in_snapshot() {
        let mut runtime = runtime(FakeProcess {
            start_error: Some(MicrophoneHostStartError::ConfiguredEndpointUnavailable),
            ..FakeProcess::default()
        });
        assert!(runtime.ensure_started().is_err());
        let snapshot = runtime.snapshot();
        assert_eq!(snapshot.state, MicrophoneHostState::Failed);
        assert_eq!(
            snapshot.problem_code.as_deref(),
            Some("CAPY.MICROPHONE_HOST.ENDPOINT_UNAVAILABLE")
        );
        let json = serde_json::to_string(&snapshot).expect("snapshot");
        assert!(!json.contains("endpoint-id"));
        assert!(!json.contains("micyou.exe"));
    }

    #[test]
    fn invalid_address_and_poll_bounds_fail_before_process_use() {
        assert!(
            MicrophoneHostRuntime::new(
                FakeProcess::default(),
                "0.0.0.0:8554".parse().unwrap(),
                3,
                5
            )
            .is_err()
        );
        assert!(
            MicrophoneHostRuntime::new(
                FakeProcess::default(),
                "100.64.0.10:8554".parse().unwrap(),
                0,
                5
            )
            .is_err()
        );
    }
}
