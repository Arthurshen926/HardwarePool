use std::{
    net::{IpAddr, Ipv4Addr},
    path::PathBuf,
    time::Duration,
};

use capyio_audio_share_adapter::{
    AudioShareSupervisor, ReceiverTcpPresence, SupervisorLimits, SupervisorStatus,
};
use serde::{Deserialize, Serialize};

#[cfg(windows)]
mod control;

#[cfg(windows)]
pub use control::{BrokerServiceClient, control_server_loop, wake_control_server};

pub const SERVICE_NAME: &str = "CapyIOBroker";
pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(250);
pub const DEFAULT_STABLE_RECEIVER_POLLS: u8 = 3;
const MAX_CONSOLE_RUN: Duration = Duration::from_secs(600);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceConfig {
    pub broker_executable: PathBuf,
    pub bind_ip: Ipv4Addr,
    pub port: u16,
    pub console_run_for: Option<Duration>,
}

impl ServiceConfig {
    pub fn parse<I, S>(arguments: I) -> Result<Self, ConfigError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut arguments = arguments.into_iter().map(Into::into);
        let _program = arguments.next();
        let mut broker_executable = None;
        let mut bind_ip: Option<Ipv4Addr> = None;
        let mut port: Option<u16> = None;
        let mut console = false;
        let mut console_run_for = None;

        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--console" => console = true,
                "--broker" => {
                    broker_executable =
                        Some(PathBuf::from(next_value(&mut arguments, "--broker")?));
                }
                "--bind-ip" => {
                    let raw = next_value(&mut arguments, "--bind-ip")?;
                    bind_ip = Some(raw.parse().map_err(|_| ConfigError::InvalidBindIp)?);
                }
                "--port" => {
                    let raw = next_value(&mut arguments, "--port")?;
                    port = Some(raw.parse().map_err(|_| ConfigError::InvalidPort)?);
                }
                "--run-for-ms" => {
                    let raw = next_value(&mut arguments, "--run-for-ms")?;
                    let milliseconds: u64 = raw.parse().map_err(|_| ConfigError::InvalidRunFor)?;
                    let duration = Duration::from_millis(milliseconds);
                    if duration.is_zero() || duration > MAX_CONSOLE_RUN {
                        return Err(ConfigError::InvalidRunFor);
                    }
                    console_run_for = Some(duration);
                }
                _ => return Err(ConfigError::UnknownArgument(argument)),
            }
        }

        if console != console_run_for.is_some() {
            return Err(ConfigError::ConsoleRunPairRequired);
        }
        let broker_executable = broker_executable.ok_or(ConfigError::MissingBroker)?;
        if broker_executable.as_os_str().is_empty() {
            return Err(ConfigError::MissingBroker);
        }
        let bind_ip = bind_ip.ok_or(ConfigError::MissingBindIp)?;
        if bind_ip.is_unspecified() {
            return Err(ConfigError::InvalidBindIp);
        }
        Ok(Self {
            broker_executable,
            bind_ip,
            port: port
                .filter(|port| *port != 0)
                .ok_or(ConfigError::InvalidPort)?,
            console_run_for,
        })
    }

    pub fn supervisor(&self) -> Result<AudioShareSupervisor, String> {
        AudioShareSupervisor::new_virtual_speaker(
            self.broker_executable.clone(),
            IpAddr::V4(self.bind_ip),
            self.port,
            SupervisorLimits::default(),
        )
        .map_err(|error| error.to_string())
    }
}

fn next_value<I>(arguments: &mut I, option: &'static str) -> Result<String, ConfigError>
where
    I: Iterator<Item = String>,
{
    arguments.next().ok_or(ConfigError::MissingValue(option))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConfigError {
    MissingValue(&'static str),
    UnknownArgument(String),
    MissingBroker,
    MissingBindIp,
    InvalidBindIp,
    InvalidPort,
    InvalidRunFor,
    ConsoleRunPairRequired,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingValue(option) => write!(formatter, "missing value for {option}"),
            Self::UnknownArgument(_) => formatter.write_str("unknown service argument"),
            Self::MissingBroker => formatter.write_str("--broker is required"),
            Self::MissingBindIp => formatter.write_str("--bind-ip is required"),
            Self::InvalidBindIp => {
                formatter.write_str("--bind-ip must be a non-unspecified IPv4 literal")
            }
            Self::InvalidPort => formatter.write_str("--port must be a non-zero u16"),
            Self::InvalidRunFor => formatter.write_str("--run-for-ms must be between 1 and 600000"),
            Self::ConsoleRunPairRequired => {
                formatter.write_str("--console and --run-for-ms must be supplied together")
            }
        }
    }
}

impl std::error::Error for ConfigError {}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BrokerServiceState {
    Stopped,
    WaitingForReceiver,
    Active,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrokerServiceSnapshot {
    pub schema_version: u8,
    pub state: BrokerServiceState,
    pub generation: u64,
    pub receiver_present: bool,
    pub problem_code: Option<String>,
}

pub trait BrokerProcess {
    fn start(&mut self) -> Result<(), String>;
    fn running(&mut self) -> Result<bool, String>;
    fn receiver_present(&mut self) -> Result<bool, String>;
    fn stop(&mut self) -> Result<(), String>;
}

impl BrokerProcess for AudioShareSupervisor {
    fn start(&mut self) -> Result<(), String> {
        AudioShareSupervisor::start(self)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    fn running(&mut self) -> Result<bool, String> {
        AudioShareSupervisor::status(self)
            .map(|status| matches!(status, SupervisorStatus::Running { .. }))
            .map_err(|error| error.to_string())
    }

    fn receiver_present(&mut self) -> Result<bool, String> {
        AudioShareSupervisor::receiver_tcp_presence(self)
            .map(|presence| matches!(presence, ReceiverTcpPresence::Established { .. }))
            .map_err(|error| error.to_string())
    }

    fn stop(&mut self) -> Result<(), String> {
        AudioShareSupervisor::stop(self)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

pub struct BrokerServiceRuntime<P> {
    process: P,
    state: BrokerServiceState,
    generation: u64,
    stable_receiver_polls: u8,
    receiver_polls: u8,
    problem_code: Option<&'static str>,
}

impl<P: BrokerProcess> BrokerServiceRuntime<P> {
    pub fn new(process: P, stable_receiver_polls: u8) -> Result<Self, String> {
        if stable_receiver_polls == 0 {
            return Err("stable receiver poll count must be non-zero".to_owned());
        }
        Ok(Self {
            process,
            state: BrokerServiceState::Stopped,
            generation: 0,
            stable_receiver_polls,
            receiver_polls: 0,
            problem_code: None,
        })
    }

    pub fn start(&mut self) -> Result<BrokerServiceSnapshot, String> {
        if self.state != BrokerServiceState::Stopped {
            return Err("Broker service Runtime is already started".to_owned());
        }
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| "Broker service generation exhausted".to_owned())?;
        match self.process.start() {
            Ok(()) => {
                self.state = BrokerServiceState::WaitingForReceiver;
                self.problem_code = None;
                Ok(self.snapshot())
            }
            Err(error) => {
                self.state = BrokerServiceState::Failed;
                self.problem_code = Some("CAPY.WINDOWS_SERVICE.BROKER_START_FAILED");
                Err(error)
            }
        }
    }

    pub fn ensure_started(&mut self) -> Result<BrokerServiceSnapshot, String> {
        match self.state {
            BrokerServiceState::Stopped => self.start(),
            BrokerServiceState::Failed => {
                self.stop()?;
                self.start()
            }
            BrokerServiceState::WaitingForReceiver | BrokerServiceState::Active => {
                Ok(self.snapshot())
            }
        }
    }

    pub fn poll(&mut self) -> BrokerServiceSnapshot {
        if !matches!(
            self.state,
            BrokerServiceState::WaitingForReceiver | BrokerServiceState::Active
        ) {
            return self.snapshot();
        }
        match self.process.running() {
            Ok(true) => {}
            Ok(false) | Err(_) => {
                self.fail("CAPY.WINDOWS_SERVICE.BROKER_EXITED");
                return self.snapshot();
            }
        }
        match self.process.receiver_present() {
            Ok(true) => {
                self.receiver_polls = self.receiver_polls.saturating_add(1);
                if self.receiver_polls >= self.stable_receiver_polls {
                    self.state = BrokerServiceState::Active;
                }
            }
            Ok(false) => {
                self.receiver_polls = 0;
                self.state = BrokerServiceState::WaitingForReceiver;
            }
            Err(_) => self.fail("CAPY.WINDOWS_SERVICE.RECEIVER_OBSERVATION_FAILED"),
        }
        self.snapshot()
    }

    pub fn stop(&mut self) -> Result<BrokerServiceSnapshot, String> {
        let result = self.process.stop();
        self.state = BrokerServiceState::Stopped;
        self.receiver_polls = 0;
        self.problem_code = None;
        result.map(|()| self.snapshot())
    }

    pub fn ensure_stopped(&mut self) -> Result<BrokerServiceSnapshot, String> {
        if self.state == BrokerServiceState::Stopped {
            Ok(self.snapshot())
        } else {
            self.stop()
        }
    }

    pub fn snapshot(&self) -> BrokerServiceSnapshot {
        BrokerServiceSnapshot {
            schema_version: 1,
            state: self.state,
            generation: self.generation,
            receiver_present: self.state == BrokerServiceState::Active,
            problem_code: self.problem_code.map(str::to_owned),
        }
    }

    fn fail(&mut self, code: &'static str) {
        self.state = BrokerServiceState::Failed;
        self.receiver_polls = 0;
        self.problem_code = Some(code);
        let _ = self.process.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FakeBroker {
        running: bool,
        receiver: bool,
        starts: usize,
        stops: usize,
    }

    impl BrokerProcess for FakeBroker {
        fn start(&mut self) -> Result<(), String> {
            self.running = true;
            self.starts += 1;
            Ok(())
        }

        fn running(&mut self) -> Result<bool, String> {
            Ok(self.running)
        }

        fn receiver_present(&mut self) -> Result<bool, String> {
            Ok(self.receiver)
        }

        fn stop(&mut self) -> Result<(), String> {
            self.running = false;
            self.stops += 1;
            Ok(())
        }
    }

    #[test]
    fn config_is_closed_bounded_and_requires_explicit_ipv4() {
        let parsed = ServiceConfig::parse([
            "service",
            "--broker",
            "broker.exe",
            "--bind-ip",
            "100.64.0.1",
            "--port",
            "65530",
        ])
        .expect("valid service config");
        assert_eq!(parsed.bind_ip, Ipv4Addr::new(100, 64, 0, 1));
        assert_eq!(parsed.port, 65530);
        assert!(
            ServiceConfig::parse([
                "service",
                "--broker",
                "x",
                "--bind-ip",
                "0.0.0.0",
                "--port",
                "1"
            ])
            .is_err()
        );
        assert!(
            ServiceConfig::parse([
                "service",
                "--broker",
                "x",
                "--bind-ip",
                "127.0.0.1",
                "--port",
                "1",
                "--shell"
            ])
            .is_err()
        );
    }

    #[test]
    fn console_mode_is_explicit_and_time_bounded() {
        assert!(
            ServiceConfig::parse([
                "service",
                "--broker",
                "x",
                "--bind-ip",
                "127.0.0.1",
                "--port",
                "1",
                "--console"
            ])
            .is_err()
        );
        assert!(
            ServiceConfig::parse([
                "service",
                "--broker",
                "x",
                "--bind-ip",
                "127.0.0.1",
                "--port",
                "1",
                "--console",
                "--run-for-ms",
                "600001"
            ])
            .is_err()
        );
    }

    #[test]
    fn receiver_requires_stable_presence_and_loss_returns_to_waiting() {
        let mut runtime = BrokerServiceRuntime::new(FakeBroker::default(), 3).expect("runtime");
        assert_eq!(
            runtime.start().expect("start").state,
            BrokerServiceState::WaitingForReceiver
        );
        runtime.process.receiver = true;
        assert_eq!(runtime.poll().state, BrokerServiceState::WaitingForReceiver);
        assert_eq!(runtime.poll().state, BrokerServiceState::WaitingForReceiver);
        assert_eq!(runtime.poll().state, BrokerServiceState::Active);
        runtime.process.receiver = false;
        assert_eq!(runtime.poll().state, BrokerServiceState::WaitingForReceiver);
        assert_eq!(
            runtime.stop().expect("stop").state,
            BrokerServiceState::Stopped
        );
    }

    #[test]
    fn broker_exit_is_failed_and_reaped() {
        let mut runtime = BrokerServiceRuntime::new(FakeBroker::default(), 1).expect("runtime");
        runtime.start().expect("start");
        runtime.process.running = false;
        let snapshot = runtime.poll();
        assert_eq!(snapshot.state, BrokerServiceState::Failed);
        assert_eq!(
            snapshot.problem_code.as_deref(),
            Some("CAPY.WINDOWS_SERVICE.BROKER_EXITED")
        );
        assert_eq!(runtime.process.stops, 1);
    }
}
