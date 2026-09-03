use std::{
    fmt::{self, Display, Formatter},
    io::{self, Read, Write},
    net::{Shutdown, TcpStream},
    time::Instant,
};

use capyio_input::{
    GamepadControls, GamepadState, InputContractError, InputSequenceOutcome, InputSequenceTracker,
    SequenceGap,
};
use serde::{Deserialize, de::DeserializeOwned};
use thiserror::Error;

use crate::{
    VIIPER_XBOX360_RUMBLE_FEEDBACK_BYTES, ViiperClientError, ViiperLoopbackClient,
    ViiperXbox360Error, ViiperXbox360Mapping, Xbox360RumbleFeedback, decode_xbox360_rumble,
    encode_xbox360_input_state,
};

const XBOX360_TYPE: &str = "xbox360";
const XBOX360_VENDOR_ID: &str = "0x045e";
const XBOX360_PRODUCT_ID: &str = "0x028e";
const XBOX360_SUBTYPE: u8 = 1;
const MAX_DEVICE_ID_BYTES: usize = 10;

/// Explicit caller assertion required before a mutating VIIPER session.
///
/// The pinned v0.7.0 server can auto-attach a newly created localhost device
/// to USB/IP. CapyIO cannot query that setting through the reviewed API, so a
/// lifecycle owner must independently configure and verify
/// `--api.auto-attach-local-client=false` before constructing this token. This
/// token records that assertion; it does not prove the external process state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ViiperAutoAttachDisabled(());

impl ViiperAutoAttachDisabled {
    #[must_use]
    pub const fn confirmed_by_caller() -> Self {
        Self(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViiperXbox360WorkerState {
    Running,
    Exhausted,
    Failed,
    Stopped,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ViiperSubmitOutcome {
    gap: Option<SequenceGap>,
    exhausted: bool,
}

impl ViiperSubmitOutcome {
    #[must_use]
    pub(crate) const fn new(gap: Option<SequenceGap>, exhausted: bool) -> Self {
        Self { gap, exhausted }
    }

    #[must_use]
    pub const fn gap(self) -> Option<SequenceGap> {
        self.gap
    }

    #[must_use]
    pub const fn exhausted(self) -> bool {
        self.exhausted
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ViiperSessionError {
    #[error(transparent)]
    Client(#[from] ViiperClientError),
    #[error(transparent)]
    Input(#[from] InputContractError),
    #[error(transparent)]
    Projection(#[from] ViiperXbox360Error),
    #[error("VIIPER returned bus ID zero")]
    InvalidBusId,
    #[error("VIIPER device ID must contain 1..={MAX_DEVICE_ID_BYTES} ASCII digits and be non-zero")]
    InvalidDeviceId,
    #[error("VIIPER response does not match the owned request: {0}")]
    ResponseMismatch(&'static str),
    #[error("VIIPER Xbox 360 stream connection timed out")]
    StreamConnectTimedOut,
    #[error("VIIPER Xbox 360 stream connection failed: {0}")]
    StreamConnectFailed(String),
    #[error("VIIPER Xbox 360 stream configuration failed: {0}")]
    StreamConfigurationFailed(String),
    #[error("VIIPER Xbox 360 stream write failed: {0}")]
    StreamWriteFailed(String),
    #[error("VIIPER Xbox 360 stream read failed: {0}")]
    StreamReadFailed(String),
    #[error("VIIPER Xbox 360 stream closed before feedback began")]
    StreamPeerClosed,
    #[error("VIIPER Xbox 360 feedback ended after {actual} of {expected} bytes")]
    TruncatedFeedback { actual: usize, expected: usize },
    #[error("VIIPER Xbox 360 worker is not running (state: {0:?})")]
    WorkerNotRunning(ViiperXbox360WorkerState),
}

#[derive(Debug, Eq, PartialEq)]
pub struct ViiperOpenError {
    cause: ViiperSessionError,
    cleanup: Option<ViiperSessionError>,
}

impl ViiperOpenError {
    #[must_use]
    pub const fn cause(&self) -> &ViiperSessionError {
        &self.cause
    }

    #[must_use]
    pub const fn cleanup(&self) -> Option<&ViiperSessionError> {
        self.cleanup.as_ref()
    }
}

impl Display for ViiperOpenError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "VIIPER Xbox 360 session open failed: {}",
            self.cause
        )?;
        if let Some(cleanup) = &self.cleanup {
            write!(formatter, "; owned-bus cleanup also failed: {cleanup}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ViiperOpenError {}

#[derive(Debug, Eq, PartialEq)]
pub struct ViiperStopError {
    neutral: Option<ViiperSessionError>,
    cleanup: Option<ViiperSessionError>,
}

impl ViiperStopError {
    #[must_use]
    pub const fn neutral(&self) -> Option<&ViiperSessionError> {
        self.neutral.as_ref()
    }

    #[must_use]
    pub const fn cleanup(&self) -> Option<&ViiperSessionError> {
        self.cleanup.as_ref()
    }
}

impl Display for ViiperStopError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("VIIPER Xbox 360 stop did not complete cleanly")?;
        if let Some(neutral) = &self.neutral {
            write!(formatter, "; neutral failed: {neutral}")?;
        }
        if let Some(cleanup) = &self.cleanup {
            write!(formatter, "; cleanup failed: {cleanup}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ViiperStopError {}

#[derive(Debug)]
pub struct ViiperXbox360Worker {
    client: ViiperLoopbackClient,
    bus_id: u32,
    device_id: String,
    stream: Option<TcpStream>,
    mapping: ViiperXbox360Mapping,
    tracker: InputSequenceTracker,
    state: ViiperXbox360WorkerState,
}

impl ViiperLoopbackClient {
    /// Opens one fully owned Xbox 360 session against the pinned loopback API.
    ///
    /// The anchor is validated and seeds stream/epoch/sequence ownership but
    /// is not emitted. The operation re-probes compatibility, creates one bus,
    /// adds only the fixed default Xbox 360 type, immediately opens its device
    /// stream and sends a neutral frame before returning `Running`. Once the
    /// bus ID is known, every later open failure attempts bounded bus removal.
    pub fn open_xbox360(
        &self,
        _auto_attach_disabled: ViiperAutoAttachDisabled,
        anchor: GamepadState,
        mapping: ViiperXbox360Mapping,
    ) -> Result<ViiperXbox360Worker, ViiperOpenError> {
        let prepare = || -> Result<InputSequenceTracker, ViiperSessionError> {
            anchor.validate()?;
            encode_xbox360_input_state(anchor.controls, mapping)?;
            Ok(InputSequenceTracker::new(
                anchor.header.stream_id,
                anchor.header.stream_epoch,
                anchor.header.sequence,
            )?)
        };
        let tracker = prepare().map_err(|cause| ViiperOpenError {
            cause,
            cleanup: None,
        })?;
        self.probe().map_err(|cause| ViiperOpenError {
            cause: cause.into(),
            cleanup: None,
        })?;

        let bus = decode_bus(
            self.request(b"bus/create\0")
                .map_err(|cause| ViiperOpenError {
                    cause: cause.into(),
                    cleanup: None,
                })?,
        )
        .map_err(|cause| ViiperOpenError {
            cause,
            cleanup: None,
        })?;

        let result = (|| -> Result<ViiperXbox360Worker, ViiperSessionError> {
            let request = format!("bus/{}/add {{\"type\":\"{XBOX360_TYPE}\"}}\0", bus.bus_id);
            let device = decode_device(self.request(request.as_bytes())?, bus.bus_id)?;
            let mut stream = connect_device_stream(self, bus.bus_id, &device.device_id)?;
            write_controls(&mut stream, GamepadControls::neutral(), mapping)?;
            Ok(ViiperXbox360Worker {
                client: *self,
                bus_id: bus.bus_id,
                device_id: device.device_id,
                stream: Some(stream),
                mapping,
                tracker,
                state: ViiperXbox360WorkerState::Running,
            })
        })();

        result.map_err(|cause| ViiperOpenError {
            cleanup: remove_bus(self, bus.bus_id).err(),
            cause,
        })
    }
}

impl ViiperXbox360Worker {
    #[must_use]
    pub const fn state(&self) -> ViiperXbox360WorkerState {
        self.state
    }

    #[must_use]
    pub const fn bus_id(&self) -> u32 {
        self.bus_id
    }

    #[must_use]
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    pub fn submit(
        &mut self,
        gamepad: GamepadState,
    ) -> Result<ViiperSubmitOutcome, ViiperSessionError> {
        self.require_running()?;
        gamepad.validate()?;
        let report = encode_xbox360_input_state(gamepad.controls, self.mapping)?;
        let mut candidate = self.tracker;
        let sequence_outcome = candidate.observe(gamepad.header)?;
        let gap = match sequence_outcome {
            InputSequenceOutcome::InOrder => None,
            InputSequenceOutcome::Gap(gap) => Some(gap),
        };

        let write_result = (|| -> Result<(), ViiperSessionError> {
            if gap.is_some() {
                self.write_neutral()?;
            }
            self.write_report(&report)?;
            if gamepad.header.sequence == u64::MAX {
                self.write_neutral()?;
            }
            Ok(())
        })();
        if let Err(error) = write_result {
            self.fail_stream();
            return Err(error);
        }

        self.tracker = candidate;
        let exhausted = gamepad.header.sequence == u64::MAX;
        if exhausted {
            self.state = ViiperXbox360WorkerState::Exhausted;
        }
        Ok(ViiperSubmitOutcome { gap, exhausted })
    }

    pub fn advance_epoch(
        &mut self,
        new_epoch: u64,
        first_sequence: u64,
    ) -> Result<(), ViiperSessionError> {
        if !matches!(
            self.state,
            ViiperXbox360WorkerState::Running | ViiperXbox360WorkerState::Exhausted
        ) {
            return Err(ViiperSessionError::WorkerNotRunning(self.state));
        }
        let mut candidate = self.tracker;
        candidate.advance_epoch(new_epoch, first_sequence)?;
        if let Err(error) = self.write_neutral() {
            self.fail_stream();
            return Err(error);
        }
        self.tracker = candidate;
        self.state = ViiperXbox360WorkerState::Running;
        Ok(())
    }

    /// Writes one complete neutral state without changing stream sequence.
    ///
    /// A host that owns an external USB/IP attachment uses this before
    /// detaching the Windows device, then calls [`Self::stop`] to close the
    /// stream and remove the owned VIIPER bus. A write failure is terminal.
    pub fn request_neutral(&mut self) -> Result<(), ViiperSessionError> {
        if !matches!(
            self.state,
            ViiperXbox360WorkerState::Running | ViiperXbox360WorkerState::Exhausted
        ) {
            return Err(ViiperSessionError::WorkerNotRunning(self.state));
        }
        if let Err(error) = self.write_neutral() {
            self.fail_stream();
            return Err(error);
        }
        Ok(())
    }

    /// Reads at most one exact two-byte feedback frame under one absolute I/O
    /// deadline. A timeout before any byte is a normal no-feedback poll; a
    /// timeout or EOF after one byte is terminal truncation.
    pub fn poll_rumble(&mut self) -> Result<Option<Xbox360RumbleFeedback>, ViiperSessionError> {
        if !matches!(
            self.state,
            ViiperXbox360WorkerState::Running | ViiperXbox360WorkerState::Exhausted
        ) {
            return Err(ViiperSessionError::WorkerNotRunning(self.state));
        }
        let deadline = Instant::now() + self.client_config().io_timeout();
        let mut report = [0_u8; VIIPER_XBOX360_RUMBLE_FEEDBACK_BYTES];
        let mut received = 0;
        while received < report.len() {
            let remaining = match deadline
                .checked_duration_since(Instant::now())
                .filter(|remaining| !remaining.is_zero())
            {
                Some(remaining) => remaining,
                None if received == 0 => return Ok(None),
                None => {
                    self.fail_stream();
                    return Err(ViiperSessionError::TruncatedFeedback {
                        actual: received,
                        expected: report.len(),
                    });
                }
            };
            let timeout_result = {
                let stream = self.stream_mut()?;
                stream.set_read_timeout(Some(remaining))
            };
            if let Err(error) = timeout_result {
                self.fail_stream();
                return Err(ViiperSessionError::StreamConfigurationFailed(
                    error.to_string(),
                ));
            }
            let read = {
                let stream = self.stream_mut()?;
                stream.read(&mut report[received..])
            };
            match read {
                Ok(0) if received == 0 => {
                    self.fail_stream();
                    return Err(ViiperSessionError::StreamPeerClosed);
                }
                Ok(0) => {
                    self.fail_stream();
                    return Err(ViiperSessionError::TruncatedFeedback {
                        actual: received,
                        expected: report.len(),
                    });
                }
                Ok(count) => received += count,
                Err(error) if is_timeout(&error) && received == 0 => return Ok(None),
                Err(error) if is_timeout(&error) => {
                    self.fail_stream();
                    return Err(ViiperSessionError::TruncatedFeedback {
                        actual: received,
                        expected: report.len(),
                    });
                }
                Err(error) => {
                    self.fail_stream();
                    return Err(ViiperSessionError::StreamReadFailed(error.to_string()));
                }
            }
        }
        Ok(Some(decode_xbox360_rumble(&report)?))
    }

    pub fn stop(&mut self) -> Result<(), ViiperStopError> {
        if self.state == ViiperXbox360WorkerState::Stopped {
            return Ok(());
        }
        let neutral = if matches!(
            self.state,
            ViiperXbox360WorkerState::Running | ViiperXbox360WorkerState::Exhausted
        ) {
            self.write_neutral().err()
        } else {
            None
        };
        self.shutdown_stream();
        let cleanup = remove_bus(&self.client, self.bus_id).err();
        self.state = ViiperXbox360WorkerState::Stopped;
        if neutral.is_none() && cleanup.is_none() {
            Ok(())
        } else {
            Err(ViiperStopError { neutral, cleanup })
        }
    }

    fn require_running(&self) -> Result<(), ViiperSessionError> {
        if self.state == ViiperXbox360WorkerState::Running {
            Ok(())
        } else {
            Err(ViiperSessionError::WorkerNotRunning(self.state))
        }
    }

    fn client_config(&self) -> crate::ViiperLoopbackConfig {
        // The client is Copy, but its config deliberately remains encapsulated;
        // these public bounded getters preserve that construction invariant.
        self.client.config()
    }

    fn stream_mut(&mut self) -> Result<&mut TcpStream, ViiperSessionError> {
        self.stream
            .as_mut()
            .ok_or(ViiperSessionError::WorkerNotRunning(self.state))
    }

    fn write_report(&mut self, report: &[u8]) -> Result<(), ViiperSessionError> {
        self.stream_mut()?
            .write_all(report)
            .map_err(|error| ViiperSessionError::StreamWriteFailed(error.to_string()))
    }

    fn write_neutral(&mut self) -> Result<(), ViiperSessionError> {
        let neutral = encode_xbox360_input_state(GamepadControls::neutral(), self.mapping)?;
        self.write_report(&neutral)
    }

    fn fail_stream(&mut self) {
        self.shutdown_stream();
        self.state = ViiperXbox360WorkerState::Failed;
    }

    fn shutdown_stream(&mut self) {
        if let Some(stream) = self.stream.take() {
            let _ = stream.shutdown(Shutdown::Both);
        }
    }
}

impl Drop for ViiperXbox360Worker {
    fn drop(&mut self) {
        // Drop never performs management network I/O. Explicit `stop` owns the
        // neutral and bus-removal guarantees; Drop only releases the socket.
        self.shutdown_stream();
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BusResponse {
    #[serde(rename = "busId")]
    pub(crate) bus_id: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeviceResponse {
    #[serde(rename = "busId")]
    bus_id: u32,
    #[serde(rename = "devId")]
    device_id: String,
    vid: String,
    pid: String,
    #[serde(rename = "type")]
    device_type: String,
    #[serde(rename = "deviceSpecific")]
    device_specific: Xbox360DeviceSpecific,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Xbox360DeviceSpecific {
    #[serde(rename = "subType")]
    subtype: u8,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProblemResponse {
    status: u16,
    title: String,
    detail: String,
}

pub(crate) fn decode_bus(response: Vec<u8>) -> Result<BusResponse, ViiperSessionError> {
    let bus: BusResponse = decode_response(&response)?;
    if bus.bus_id == 0 {
        return Err(ViiperSessionError::InvalidBusId);
    }
    Ok(bus)
}

fn decode_device(
    response: Vec<u8>,
    expected_bus_id: u32,
) -> Result<DeviceResponse, ViiperSessionError> {
    let device: DeviceResponse = decode_response(&response)?;
    if device.bus_id != expected_bus_id {
        return Err(ViiperSessionError::ResponseMismatch("device bus ID"));
    }
    validate_device_id(&device.device_id)?;
    if device.vid != XBOX360_VENDOR_ID {
        return Err(ViiperSessionError::ResponseMismatch("Xbox 360 vendor ID"));
    }
    if device.pid != XBOX360_PRODUCT_ID {
        return Err(ViiperSessionError::ResponseMismatch("Xbox 360 product ID"));
    }
    if device.device_type != XBOX360_TYPE {
        return Err(ViiperSessionError::ResponseMismatch("Xbox 360 device type"));
    }
    if device.device_specific.subtype != XBOX360_SUBTYPE {
        return Err(ViiperSessionError::ResponseMismatch("Xbox 360 subtype"));
    }
    Ok(device)
}

pub(crate) fn decode_response<T: DeserializeOwned>(
    response: &[u8],
) -> Result<T, ViiperSessionError> {
    if let Ok(problem) = serde_json::from_slice::<ProblemResponse>(response) {
        return Err(ViiperClientError::RemoteProblem {
            status: problem.status,
            title: problem.title,
            detail: problem.detail,
        }
        .into());
    }
    serde_json::from_slice(response)
        .map_err(|error| ViiperClientError::InvalidJson(error.to_string()).into())
}

pub(crate) fn validate_device_id(device_id: &str) -> Result<(), ViiperSessionError> {
    if device_id.is_empty()
        || device_id.len() > MAX_DEVICE_ID_BYTES
        || !device_id.bytes().all(|byte| byte.is_ascii_digit())
        || device_id.bytes().all(|byte| byte == b'0')
    {
        return Err(ViiperSessionError::InvalidDeviceId);
    }
    Ok(())
}

pub(crate) fn connect_device_stream(
    client: &ViiperLoopbackClient,
    bus_id: u32,
    device_id: &str,
) -> Result<TcpStream, ViiperSessionError> {
    let config = client.config();
    let mut stream = TcpStream::connect_timeout(&config.address(), config.connect_timeout())
        .map_err(|error| {
            if is_timeout(&error) {
                ViiperSessionError::StreamConnectTimedOut
            } else {
                ViiperSessionError::StreamConnectFailed(error.to_string())
            }
        })?;
    stream
        .set_read_timeout(Some(config.io_timeout()))
        .and_then(|()| stream.set_write_timeout(Some(config.io_timeout())))
        .map_err(|error| ViiperSessionError::StreamConfigurationFailed(error.to_string()))?;
    let handshake = format!("bus/{bus_id}/{device_id}\0");
    stream
        .write_all(handshake.as_bytes())
        .map_err(|error| ViiperSessionError::StreamWriteFailed(error.to_string()))?;
    Ok(stream)
}

fn write_controls(
    stream: &mut TcpStream,
    controls: GamepadControls,
    mapping: ViiperXbox360Mapping,
) -> Result<(), ViiperSessionError> {
    let report = encode_xbox360_input_state(controls, mapping)?;
    stream
        .write_all(&report)
        .map_err(|error| ViiperSessionError::StreamWriteFailed(error.to_string()))
}

pub(crate) fn remove_bus(
    client: &ViiperLoopbackClient,
    bus_id: u32,
) -> Result<(), ViiperSessionError> {
    let request = format!("bus/remove {bus_id}\0");
    let removed = decode_bus(client.request(request.as_bytes())?)?;
    if removed.bus_id != bus_id {
        return Err(ViiperSessionError::ResponseMismatch("removed bus ID"));
    }
    Ok(())
}

pub(crate) fn is_timeout(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
    )
}
