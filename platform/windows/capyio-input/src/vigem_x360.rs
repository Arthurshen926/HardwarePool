use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use capyio_input::{GamepadButton, GamepadControls};
use capyio_viiper_adapter::{ViiperXbox360Error, ViiperXbox360Mapping, encode_xbox360_input_state};

const READY_LINE: &str = "CAPYIO_VIGEM_X360_SIDECAR_READY";
const DEFAULT_START_TIMEOUT: Duration = Duration::from_secs(3);
const DEFAULT_STOP_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VigemX360SidecarConfig {
    executable: PathBuf,
    start_timeout: Duration,
    stop_timeout: Duration,
}

impl VigemX360SidecarConfig {
    pub fn new(executable: PathBuf) -> Result<Self, VigemX360Error> {
        if !executable.is_absolute() {
            return Err(VigemX360Error::InvalidConfig(
                "ViGEm sidecar executable must be an absolute path".to_owned(),
            ));
        }
        Ok(Self {
            executable,
            start_timeout: DEFAULT_START_TIMEOUT,
            stop_timeout: DEFAULT_STOP_TIMEOUT,
        })
    }

    #[must_use]
    pub fn executable(&self) -> &Path {
        &self.executable
    }
}

#[derive(Debug)]
pub enum VigemX360Error {
    InvalidConfig(String),
    Spawn(io::Error),
    MissingPipe(&'static str),
    ReadyTimeout,
    UnexpectedReady(String),
    Codec(ViiperXbox360Error),
    Write(io::Error),
    Stop(io::Error),
}

impl Display for VigemX360Error {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig(message) => formatter.write_str(message),
            Self::Spawn(error) => write!(formatter, "could not start ViGEm sidecar: {error}"),
            Self::MissingPipe(pipe) => write!(formatter, "ViGEm sidecar has no piped {pipe}"),
            Self::ReadyTimeout => formatter.write_str("ViGEm sidecar readiness timed out"),
            Self::UnexpectedReady(line) => {
                write!(
                    formatter,
                    "ViGEm sidecar returned unexpected readiness: {line}"
                )
            }
            Self::Codec(error) => write!(formatter, "could not project Xbox 360 controls: {error}"),
            Self::Write(error) => {
                write!(formatter, "could not write ViGEm Xbox 360 state: {error}")
            }
            Self::Stop(error) => write!(formatter, "could not stop ViGEm sidecar: {error}"),
        }
    }
}

impl Error for VigemX360Error {}

/// Owns one fixed Xbox 360 ViGEm target in a separate user-mode process.
///
/// Complete controls cross stdin as the existing fixed 20-byte Xbox 360 state
/// contract. Stdout is reserved for the one readiness control message; the
/// sidecar emits failures to stderr. IMU remains on the independent DS4/DSU
/// Routes and is never collapsed into this compatibility projection.
pub struct VigemX360Companion {
    child: Child,
    input: Option<ChildStdin>,
    stop_timeout: Duration,
}

impl VigemX360Companion {
    pub fn start(config: VigemX360SidecarConfig) -> Result<Self, VigemX360Error> {
        let mut child = Command::new(&config.executable)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(VigemX360Error::Spawn)?;
        let input = child
            .stdin
            .take()
            .ok_or(VigemX360Error::MissingPipe("stdin"))?;
        let output = child
            .stdout
            .take()
            .ok_or(VigemX360Error::MissingPipe("stdout"))?;
        let (sender, receiver) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut line = String::new();
            let result = BufReader::new(output)
                .read_line(&mut line)
                .map(|_| line.trim().to_owned());
            let _ = sender.send(result);
        });
        let readiness = match receiver.recv_timeout(config.start_timeout) {
            Ok(Ok(line)) => line,
            Ok(Err(error)) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(VigemX360Error::Spawn(error));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(VigemX360Error::ReadyTimeout);
            }
        };
        if readiness != READY_LINE {
            let _ = child.kill();
            let _ = child.wait();
            return Err(VigemX360Error::UnexpectedReady(readiness));
        }
        let mut companion = Self {
            child,
            input: Some(input),
            stop_timeout: config.stop_timeout,
        };
        companion.submit(GamepadControls::neutral())?;
        Ok(companion)
    }

    pub fn submit(&mut self, controls: GamepadControls) -> Result<(), VigemX360Error> {
        let report = encode_companion_controls(controls)?;
        self.input
            .as_mut()
            .ok_or_else(|| {
                VigemX360Error::Write(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "ViGEm sidecar stdin is closed",
                ))
            })?
            .write_all(&report)
            .map_err(VigemX360Error::Write)
    }

    pub fn stop(mut self) -> Result<(), VigemX360Error> {
        self.submit(GamepadControls::neutral())?;
        if let Some(mut input) = self.input.take() {
            input.flush().map_err(VigemX360Error::Write)?;
        }
        let deadline = Instant::now() + self.stop_timeout;
        loop {
            match self.child.try_wait().map_err(VigemX360Error::Stop)? {
                Some(_) => return Ok(()),
                None if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                None => {
                    self.child.kill().map_err(VigemX360Error::Stop)?;
                    self.child.wait().map_err(VigemX360Error::Stop)?;
                    return Err(VigemX360Error::Stop(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "ViGEm sidecar did not stop after stdin closed",
                    )));
                }
            }
        }
    }
}

fn encode_companion_controls(mut controls: GamepadControls) -> Result<[u8; 20], VigemX360Error> {
    for button in [
        GamepadButton::Touchpad,
        GamepadButton::Paddle1,
        GamepadButton::Paddle2,
        GamepadButton::Paddle3,
        GamepadButton::Paddle4,
    ] {
        controls.buttons = controls.buttons.without(button);
    }
    encode_xbox360_input_state(controls, ViiperXbox360Mapping::preserve())
        .map_err(VigemX360Error::Codec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use capyio_input::GamepadButtons;

    #[test]
    fn config_rejects_relative_sidecar_path() {
        assert!(matches!(
            VigemX360SidecarConfig::new(PathBuf::from("sidecar.exe")),
            Err(VigemX360Error::InvalidConfig(_))
        ));
    }

    #[test]
    fn companion_omits_touch_only_buttons_and_preserves_xinput_controls() {
        let controls = GamepadControls {
            buttons: GamepadButtons::empty()
                .with(GamepadButton::South)
                .with(GamepadButton::Touchpad)
                .with(GamepadButton::Paddle1),
            ..GamepadControls::neutral()
        };
        let report = encode_companion_controls(controls).expect("companion report");
        assert_eq!(u32::from_le_bytes(report[0..4].try_into().unwrap()), 0x1000);
    }
}
