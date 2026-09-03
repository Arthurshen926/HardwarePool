use std::{
    io::{self, Read, Write},
    net::{Shutdown, SocketAddr, TcpStream},
    time::{Duration, Instant},
};

use serde::Deserialize;
use thiserror::Error;

pub const PINNED_VIIPER_SERVER: &str = "VIIPER";
pub const PINNED_VIIPER_VERSION: &str = "0.7.0";
pub const EXPERIMENTAL_VIIPER_URB_FIX_VERSION: &str = "0.7.0-capyio-88f66f1";
pub const EXPERIMENTAL_VIIPER_DS4WINDOWS_VERSION: &str = "0.1.0-capyio-fd298a0";
pub const EXPERIMENTAL_VIIPER_DS4WINDOWS_V012_VERSION: &str = "0.1.2";
pub const MAX_VIIPER_MANAGEMENT_RESPONSE_BYTES: usize = 4096;
pub const MAX_VIIPER_CONNECTION_TIMEOUT: Duration = Duration::from_secs(60);

const PING_REQUEST: &[u8] = b"ping\0";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ViiperLoopbackConfig {
    address: SocketAddr,
    connect_timeout: Duration,
    io_timeout: Duration,
    response_limit: usize,
}

impl ViiperLoopbackConfig {
    pub fn new(
        address: SocketAddr,
        connect_timeout: Duration,
        io_timeout: Duration,
        response_limit: usize,
    ) -> Result<Self, ViiperClientError> {
        if !address.ip().is_loopback() {
            return Err(ViiperClientError::NonLoopbackAddress(address));
        }
        if address.port() == 0 {
            return Err(ViiperClientError::InvalidPort);
        }
        if !valid_timeout(connect_timeout) || !valid_timeout(io_timeout) {
            return Err(ViiperClientError::InvalidTimeout);
        }
        if !(1..=MAX_VIIPER_MANAGEMENT_RESPONSE_BYTES).contains(&response_limit) {
            return Err(ViiperClientError::InvalidResponseLimit {
                actual: response_limit,
                maximum: MAX_VIIPER_MANAGEMENT_RESPONSE_BYTES,
            });
        }
        Ok(Self {
            address,
            connect_timeout,
            io_timeout,
            response_limit,
        })
    }

    #[must_use]
    pub const fn address(self) -> SocketAddr {
        self.address
    }

    #[must_use]
    pub const fn connect_timeout(self) -> Duration {
        self.connect_timeout
    }

    #[must_use]
    pub const fn io_timeout(self) -> Duration {
        self.io_timeout
    }

    #[must_use]
    pub const fn response_limit(self) -> usize {
        self.response_limit
    }
}

fn valid_timeout(timeout: Duration) -> bool {
    !timeout.is_zero() && timeout <= MAX_VIIPER_CONNECTION_TIMEOUT
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompatibleViiperProbe {
    address: SocketAddr,
    version: CompatibleViiperVersion,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompatibleViiperVersion {
    ReleaseV070,
    ExperimentalUrbFix88f66f1,
    ExperimentalDs4WindowsFd298a0,
    ExperimentalDs4WindowsV012,
}

impl CompatibleViiperVersion {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReleaseV070 => PINNED_VIIPER_VERSION,
            Self::ExperimentalUrbFix88f66f1 => EXPERIMENTAL_VIIPER_URB_FIX_VERSION,
            Self::ExperimentalDs4WindowsFd298a0 => EXPERIMENTAL_VIIPER_DS4WINDOWS_VERSION,
            Self::ExperimentalDs4WindowsV012 => EXPERIMENTAL_VIIPER_DS4WINDOWS_V012_VERSION,
        }
    }

    #[must_use]
    pub const fn is_experimental(self) -> bool {
        !matches!(self, Self::ReleaseV070)
    }
}

impl CompatibleViiperProbe {
    #[must_use]
    pub const fn address(self) -> SocketAddr {
        self.address
    }

    #[must_use]
    pub const fn server(self) -> &'static str {
        PINNED_VIIPER_SERVER
    }

    #[must_use]
    pub const fn version(self) -> &'static str {
        self.version.as_str()
    }

    #[must_use]
    pub const fn compatibility(self) -> CompatibleViiperVersion {
        self.version
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ViiperLoopbackClient {
    config: ViiperLoopbackConfig,
}

impl ViiperLoopbackClient {
    #[must_use]
    pub const fn new(config: ViiperLoopbackConfig) -> Self {
        Self { config }
    }

    pub(crate) const fn config(&self) -> ViiperLoopbackConfig {
        self.config
    }

    /// Performs the only read-only operation in the pinned management API.
    ///
    /// A fresh TCP connection sends exactly `ping\0`, then reads a bounded JSON
    /// response through connection close. It never creates a bus or device and
    /// cannot trigger VIIPER's localhost USB/IP auto-attach behavior.
    pub fn probe(&self) -> Result<CompatibleViiperProbe, ViiperClientError> {
        let response = self.request(PING_REQUEST)?;
        let version = validate_ping_response(&response)?;
        Ok(CompatibleViiperProbe {
            address: self.config.address,
            version,
        })
    }

    pub(crate) fn request(&self, request: &[u8]) -> Result<Vec<u8>, ViiperClientError> {
        let mut stream =
            TcpStream::connect_timeout(&self.config.address, self.config.connect_timeout)
                .map_err(classify_connect_error)?;
        stream
            .set_write_timeout(Some(self.config.io_timeout))
            .map_err(|error| ViiperClientError::SocketConfigurationFailed(error.to_string()))?;
        let io_deadline = Instant::now() + self.config.io_timeout;
        stream
            .write_all(request)
            .map_err(|error| ViiperClientError::RequestWriteFailed(error.to_string()))?;
        stream
            .shutdown(Shutdown::Write)
            .map_err(|error| ViiperClientError::RequestWriteFailed(error.to_string()))?;
        read_bounded_response(&mut stream, self.config.response_limit, io_deadline)
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ViiperClientError {
    #[error("VIIPER management address must be an explicit IP loopback socket: {0}")]
    NonLoopbackAddress(SocketAddr),
    #[error("VIIPER management port must be non-zero")]
    InvalidPort,
    #[error(
        "VIIPER connect and I/O timeouts must be within 1ns..={MAX_VIIPER_CONNECTION_TIMEOUT:?}"
    )]
    InvalidTimeout,
    #[error("VIIPER response limit is {actual}; expected 1..={maximum} bytes")]
    InvalidResponseLimit { actual: usize, maximum: usize },
    #[error("VIIPER loopback connection timed out")]
    ConnectTimedOut,
    #[error("VIIPER loopback connection failed: {0}")]
    ConnectFailed(String),
    #[error("VIIPER socket configuration failed: {0}")]
    SocketConfigurationFailed(String),
    #[error("VIIPER management request write failed: {0}")]
    RequestWriteFailed(String),
    #[error("VIIPER management response timed out before connection close")]
    ResponseTimedOut,
    #[error("VIIPER management response read failed: {0}")]
    ResponseReadFailed(String),
    #[error(
        "VIIPER management response exceeded {maximum} bytes (read at least {actual_at_least})"
    )]
    ResponseTooLarge {
        actual_at_least: usize,
        maximum: usize,
    },
    #[error("VIIPER management response is empty")]
    EmptyResponse,
    #[error("VIIPER management response JSON is invalid: {0}")]
    InvalidJson(String),
    #[error("VIIPER returned Problem {status}: {title}: {detail}")]
    RemoteProblem {
        status: u16,
        title: String,
        detail: String,
    },
    #[error("unexpected VIIPER server identity: {0}")]
    UnexpectedServer(String),
    #[error("unsupported VIIPER version: {0}")]
    UnsupportedVersion(String),
}

fn classify_connect_error(error: io::Error) -> ViiperClientError {
    if is_timeout(&error) {
        ViiperClientError::ConnectTimedOut
    } else {
        ViiperClientError::ConnectFailed(error.to_string())
    }
}

fn read_bounded_response(
    stream: &mut TcpStream,
    response_limit: usize,
    deadline: Instant,
) -> Result<Vec<u8>, ViiperClientError> {
    let mut response = Vec::with_capacity(response_limit.min(512));
    let mut buffer = [0_u8; 512];
    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .ok_or(ViiperClientError::ResponseTimedOut)?;
        stream
            .set_read_timeout(Some(remaining))
            .map_err(|error| ViiperClientError::SocketConfigurationFailed(error.to_string()))?;
        let remaining_with_sentinel = response_limit + 1 - response.len();
        let read_capacity = remaining_with_sentinel.min(buffer.len());
        match stream.read(&mut buffer[..read_capacity]) {
            Ok(0) => break,
            Ok(count) => {
                response.extend_from_slice(&buffer[..count]);
                if response.len() > response_limit {
                    return Err(ViiperClientError::ResponseTooLarge {
                        actual_at_least: response.len(),
                        maximum: response_limit,
                    });
                }
            }
            Err(error) if is_timeout(&error) => return Err(ViiperClientError::ResponseTimedOut),
            Err(error) => return Err(ViiperClientError::ResponseReadFailed(error.to_string())),
        }
    }
    if response.iter().all(u8::is_ascii_whitespace) {
        return Err(ViiperClientError::EmptyResponse);
    }
    Ok(response)
}

fn is_timeout(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
    )
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum PingEnvelope {
    Problem(ProblemResponse),
    Ping(PingResponse),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PingResponse {
    server: String,
    version: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProblemResponse {
    status: u16,
    title: String,
    detail: String,
}

fn validate_ping_response(response: &[u8]) -> Result<CompatibleViiperVersion, ViiperClientError> {
    let envelope: PingEnvelope = serde_json::from_slice(response)
        .map_err(|error| ViiperClientError::InvalidJson(error.to_string()))?;
    match envelope {
        PingEnvelope::Problem(problem) => Err(ViiperClientError::RemoteProblem {
            status: problem.status,
            title: problem.title,
            detail: problem.detail,
        }),
        PingEnvelope::Ping(ping) => {
            if ping.server != PINNED_VIIPER_SERVER {
                return Err(ViiperClientError::UnexpectedServer(ping.server));
            }
            match ping.version.as_str() {
                PINNED_VIIPER_VERSION => Ok(CompatibleViiperVersion::ReleaseV070),
                EXPERIMENTAL_VIIPER_URB_FIX_VERSION => {
                    Ok(CompatibleViiperVersion::ExperimentalUrbFix88f66f1)
                }
                EXPERIMENTAL_VIIPER_DS4WINDOWS_VERSION => {
                    Ok(CompatibleViiperVersion::ExperimentalDs4WindowsFd298a0)
                }
                EXPERIMENTAL_VIIPER_DS4WINDOWS_V012_VERSION => {
                    Ok(CompatibleViiperVersion::ExperimentalDs4WindowsV012)
                }
                _ => Err(ViiperClientError::UnsupportedVersion(ping.version)),
            }
        }
    }
}
