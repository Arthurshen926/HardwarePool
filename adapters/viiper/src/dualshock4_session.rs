use std::{
    fmt::{self, Display, Formatter},
    io::{Read, Write},
    net::{Shutdown, TcpStream},
    time::Instant,
};

use capyio_data_plane::{DataEnvelope, ImuSampleV1};
use capyio_input::{
    GamepadControls, GamepadState, InputContractError, InputFrameHeader, InputSequenceOutcome,
    InputSequenceTracker,
};
use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;

use crate::session::{
    connect_device_stream, decode_bus, decode_response, is_timeout, remove_bus, validate_device_id,
};
use crate::{
    VIIPER_DS4_FEEDBACK_BYTES, ViiperAutoAttachDisabled, ViiperClientError,
    ViiperDs4ControlsMapping, ViiperDs4Error, ViiperDs4Feedback, ViiperDs4MotionMapping,
    ViiperDs4MotionState, ViiperLoopbackClient, ViiperSessionError, ViiperSubmitOutcome,
    decode_dualshock4_feedback, encode_dualshock4_input_state, project_dualshock4_motion,
};

const DS4_TYPE: &str = "dualshock4";
const DS4_VENDOR_ID: &str = "0x054c";
const DS4_PRODUCT_ID: &str = "0x09cc";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViiperDs4WorkerState {
    Running,
    Exhausted,
    Failed,
    Stopped,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ViiperDs4SessionError {
    #[error(transparent)]
    Client(#[from] ViiperClientError),
    #[error(transparent)]
    Input(#[from] InputContractError),
    #[error(transparent)]
    Projection(#[from] ViiperDs4Error),
    #[error("VIIPER returned bus ID zero")]
    InvalidBusId,
    #[error("VIIPER returned an invalid DualShock 4 device ID")]
    InvalidDeviceId,
    #[error("VIIPER DualShock 4 response does not match the owned request: {0}")]
    ResponseMismatch(&'static str),
    #[error("VIIPER DualShock 4 stream connection failed: {0}")]
    StreamConnect(String),
    #[error("VIIPER DualShock 4 stream write failed: {0}")]
    StreamWrite(String),
    #[error("VIIPER DualShock 4 stream read failed: {0}")]
    StreamRead(String),
    #[error("VIIPER DualShock 4 stream configuration failed: {0}")]
    StreamConfiguration(String),
    #[error("VIIPER DualShock 4 stream closed before feedback began")]
    StreamPeerClosed,
    #[error("VIIPER DualShock 4 feedback ended after {actual} of {expected} bytes")]
    TruncatedFeedback { actual: usize, expected: usize },
    #[error("VIIPER DualShock 4 worker is not running (state: {0:?})")]
    WorkerNotRunning(ViiperDs4WorkerState),
}

#[derive(Debug, Eq, PartialEq)]
pub struct ViiperDs4OpenError {
    cause: ViiperDs4SessionError,
    cleanup: Option<ViiperDs4SessionError>,
}

impl ViiperDs4OpenError {
    #[must_use]
    pub const fn cause(&self) -> &ViiperDs4SessionError {
        &self.cause
    }

    #[must_use]
    pub const fn cleanup(&self) -> Option<&ViiperDs4SessionError> {
        self.cleanup.as_ref()
    }
}

impl Display for ViiperDs4OpenError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "VIIPER DualShock 4 session open failed: {}",
            self.cause
        )?;
        if let Some(cleanup) = &self.cleanup {
            write!(formatter, "; owned-bus cleanup also failed: {cleanup}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ViiperDs4OpenError {}

#[derive(Debug, Eq, PartialEq)]
pub struct ViiperDs4StopError {
    safe_state: Option<ViiperDs4SessionError>,
    cleanup: Option<ViiperDs4SessionError>,
}

impl ViiperDs4StopError {
    #[must_use]
    pub const fn safe_state(&self) -> Option<&ViiperDs4SessionError> {
        self.safe_state.as_ref()
    }

    #[must_use]
    pub const fn cleanup(&self) -> Option<&ViiperDs4SessionError> {
        self.cleanup.as_ref()
    }
}

impl Display for ViiperDs4StopError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("VIIPER DualShock 4 stop did not complete cleanly")?;
        if let Some(error) = &self.safe_state {
            write!(formatter, "; safe state failed: {error}")?;
        }
        if let Some(error) = &self.cleanup {
            write!(formatter, "; cleanup failed: {error}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ViiperDs4StopError {}

#[derive(Debug)]
pub struct ViiperDs4Worker {
    client: ViiperLoopbackClient,
    bus_id: u32,
    device_id: String,
    stream: Option<TcpStream>,
    controls_mapping: ViiperDs4ControlsMapping,
    motion_mapping: ViiperDs4MotionMapping,
    controls_tracker: InputSequenceTracker,
    motion_tracker: InputSequenceTracker,
    latest_controls: GamepadControls,
    latest_motion: ViiperDs4MotionState,
    state: ViiperDs4WorkerState,
}

impl ViiperLoopbackClient {
    pub fn open_dualshock4(
        &self,
        _auto_attach_disabled: ViiperAutoAttachDisabled,
        controls_anchor: GamepadState,
        motion_anchor: &DataEnvelope<ImuSampleV1>,
        controls_mapping: ViiperDs4ControlsMapping,
        motion_mapping: ViiperDs4MotionMapping,
    ) -> Result<ViiperDs4Worker, ViiperDs4OpenError> {
        let prepare =
            || -> Result<(InputSequenceTracker, InputSequenceTracker), ViiperDs4SessionError> {
                controls_anchor.validate()?;
                project_dualshock4_motion(motion_anchor, motion_mapping)?;
                encode_dualshock4_input_state(
                    controls_anchor.controls,
                    ViiperDs4MotionState::stationary(),
                    controls_mapping,
                )?;
                let controls_tracker = InputSequenceTracker::new(
                    controls_anchor.header.stream_id,
                    controls_anchor.header.stream_epoch,
                    controls_anchor.header.sequence,
                )?;
                let motion_tracker = InputSequenceTracker::new(
                    motion_anchor.stream_id,
                    motion_anchor.stream_epoch,
                    motion_anchor.sequence,
                )?;
                Ok((controls_tracker, motion_tracker))
            };
        let (controls_tracker, motion_tracker) = prepare().map_err(|cause| ViiperDs4OpenError {
            cause,
            cleanup: None,
        })?;
        self.probe().map_err(|cause| ViiperDs4OpenError {
            cause: cause.into(),
            cleanup: None,
        })?;
        let bus_response = self
            .request(b"bus/create\0")
            .map_err(|cause| ViiperDs4OpenError {
                cause: cause.into(),
                cleanup: None,
            })?;
        let bus = decode_bus(bus_response)
            .map_err(map_shared_session_error)
            .map_err(|cause| ViiperDs4OpenError {
                cause,
                cleanup: None,
            })?;

        let result = (|| -> Result<ViiperDs4Worker, ViiperDs4SessionError> {
            let request = format!("bus/{}/add {{\"type\":\"{DS4_TYPE}\"}}\0", bus.bus_id);
            let device = decode_ds4_device(self.request(request.as_bytes())?, bus.bus_id)?;
            let mut stream = connect_device_stream(self, bus.bus_id, &device.device_id)
                .map_err(map_shared_session_error)?;
            let safe = encode_dualshock4_input_state(
                GamepadControls::neutral(),
                ViiperDs4MotionState::stationary(),
                controls_mapping,
            )?;
            stream
                .write_all(&safe)
                .map_err(|error| ViiperDs4SessionError::StreamWrite(error.to_string()))?;
            Ok(ViiperDs4Worker {
                client: *self,
                bus_id: bus.bus_id,
                device_id: device.device_id,
                stream: Some(stream),
                controls_mapping,
                motion_mapping,
                controls_tracker,
                motion_tracker,
                latest_controls: GamepadControls::neutral(),
                latest_motion: ViiperDs4MotionState::stationary(),
                state: ViiperDs4WorkerState::Running,
            })
        })();

        result.map_err(|cause| ViiperDs4OpenError {
            cleanup: remove_bus(self, bus.bus_id)
                .err()
                .map(map_shared_session_error),
            cause,
        })
    }
}

impl ViiperDs4Worker {
    #[must_use]
    pub const fn state(&self) -> ViiperDs4WorkerState {
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
        motion: &DataEnvelope<ImuSampleV1>,
    ) -> Result<ViiperSubmitOutcome, ViiperDs4SessionError> {
        self.require_running()?;
        gamepad.validate()?;
        let projected_motion = project_dualshock4_motion(motion, self.motion_mapping)?;
        let report = encode_dualshock4_input_state(
            gamepad.controls,
            projected_motion,
            self.controls_mapping,
        )?;
        let mut controls_candidate = self.controls_tracker;
        let controls_outcome = controls_candidate.observe(gamepad.header)?;
        let mut motion_candidate = self.motion_tracker;
        let motion_outcome = motion_candidate.observe(motion_header(motion))?;
        let controls_gap = matches!(controls_outcome, InputSequenceOutcome::Gap(_));
        let motion_gap = matches!(motion_outcome, InputSequenceOutcome::Gap(_));

        let write_result = (|| -> Result<(), ViiperDs4SessionError> {
            if controls_gap || motion_gap {
                let safe_controls = if controls_gap {
                    GamepadControls::neutral()
                } else {
                    self.latest_controls
                };
                let safe_motion = if motion_gap {
                    ViiperDs4MotionState::stationary()
                } else {
                    self.latest_motion
                };
                self.write_state(safe_controls, safe_motion)?;
            }
            self.write_report(&report)?;
            if gamepad.header.sequence == u64::MAX || motion.sequence == u64::MAX {
                self.write_safe_state()?;
            }
            Ok(())
        })();
        if let Err(error) = write_result {
            self.fail_stream();
            return Err(error);
        }

        self.controls_tracker = controls_candidate;
        self.motion_tracker = motion_candidate;
        self.latest_controls = gamepad.controls;
        self.latest_motion = projected_motion;
        let exhausted = gamepad.header.sequence == u64::MAX || motion.sequence == u64::MAX;
        if exhausted {
            self.state = ViiperDs4WorkerState::Exhausted;
        }
        let gap = match (controls_outcome, motion_outcome) {
            (InputSequenceOutcome::Gap(gap), _) => Some(gap),
            (_, InputSequenceOutcome::Gap(gap)) => Some(gap),
            _ => None,
        };
        Ok(ViiperSubmitOutcome::new(gap, exhausted))
    }

    pub fn advance_epochs(
        &mut self,
        controls_epoch: u64,
        controls_first_sequence: u64,
        motion_epoch: u64,
        motion_first_sequence: u64,
    ) -> Result<(), ViiperDs4SessionError> {
        if !matches!(
            self.state,
            ViiperDs4WorkerState::Running | ViiperDs4WorkerState::Exhausted
        ) {
            return Err(ViiperDs4SessionError::WorkerNotRunning(self.state));
        }
        let mut controls = self.controls_tracker;
        controls.advance_epoch(controls_epoch, controls_first_sequence)?;
        let mut motion = self.motion_tracker;
        motion.advance_epoch(motion_epoch, motion_first_sequence)?;
        self.write_safe_state()?;
        self.controls_tracker = controls;
        self.motion_tracker = motion;
        self.latest_controls = GamepadControls::neutral();
        self.latest_motion = ViiperDs4MotionState::stationary();
        self.state = ViiperDs4WorkerState::Running;
        Ok(())
    }

    pub fn request_safe_state(&mut self) -> Result<(), ViiperDs4SessionError> {
        if !matches!(
            self.state,
            ViiperDs4WorkerState::Running | ViiperDs4WorkerState::Exhausted
        ) {
            return Err(ViiperDs4SessionError::WorkerNotRunning(self.state));
        }
        if let Err(error) = self.write_safe_state() {
            self.fail_stream();
            return Err(error);
        }
        self.latest_controls = GamepadControls::neutral();
        self.latest_motion = ViiperDs4MotionState::stationary();
        Ok(())
    }

    pub fn poll_feedback(&mut self) -> Result<Option<ViiperDs4Feedback>, ViiperDs4SessionError> {
        if !matches!(
            self.state,
            ViiperDs4WorkerState::Running | ViiperDs4WorkerState::Exhausted
        ) {
            return Err(ViiperDs4SessionError::WorkerNotRunning(self.state));
        }
        let deadline = Instant::now() + self.client.config().io_timeout();
        let mut report = [0_u8; VIIPER_DS4_FEEDBACK_BYTES];
        let mut received = 0;
        while received < report.len() {
            let remaining = match deadline
                .checked_duration_since(Instant::now())
                .filter(|remaining| !remaining.is_zero())
            {
                Some(remaining) => remaining,
                None if received == 0 => return Ok(None),
                None => return self.truncated_feedback(received),
            };
            if let Err(error) = self.stream_mut()?.set_read_timeout(Some(remaining)) {
                self.fail_stream();
                return Err(ViiperDs4SessionError::StreamConfiguration(
                    error.to_string(),
                ));
            }
            match self.stream_mut()?.read(&mut report[received..]) {
                Ok(0) if received == 0 => {
                    self.fail_stream();
                    return Err(ViiperDs4SessionError::StreamPeerClosed);
                }
                Ok(0) => return self.truncated_feedback(received),
                Ok(count) => received += count,
                Err(error) if is_timeout(&error) && received == 0 => return Ok(None),
                Err(error) if is_timeout(&error) => return self.truncated_feedback(received),
                Err(error) => {
                    self.fail_stream();
                    return Err(ViiperDs4SessionError::StreamRead(error.to_string()));
                }
            }
        }
        Ok(Some(decode_dualshock4_feedback(&report)?))
    }

    pub fn stop(&mut self) -> Result<(), ViiperDs4StopError> {
        if self.state == ViiperDs4WorkerState::Stopped {
            return Ok(());
        }
        let safe_state = if matches!(
            self.state,
            ViiperDs4WorkerState::Running | ViiperDs4WorkerState::Exhausted
        ) {
            self.write_safe_state().err()
        } else {
            None
        };
        self.shutdown_stream();
        let cleanup = remove_bus(&self.client, self.bus_id)
            .err()
            .map(map_shared_session_error);
        self.state = ViiperDs4WorkerState::Stopped;
        if safe_state.is_none() && cleanup.is_none() {
            Ok(())
        } else {
            Err(ViiperDs4StopError {
                safe_state,
                cleanup,
            })
        }
    }

    fn require_running(&self) -> Result<(), ViiperDs4SessionError> {
        if self.state == ViiperDs4WorkerState::Running {
            Ok(())
        } else {
            Err(ViiperDs4SessionError::WorkerNotRunning(self.state))
        }
    }

    fn stream_mut(&mut self) -> Result<&mut TcpStream, ViiperDs4SessionError> {
        self.stream
            .as_mut()
            .ok_or(ViiperDs4SessionError::WorkerNotRunning(self.state))
    }

    fn write_state(
        &mut self,
        controls: GamepadControls,
        motion: ViiperDs4MotionState,
    ) -> Result<(), ViiperDs4SessionError> {
        let report = encode_dualshock4_input_state(controls, motion, self.controls_mapping)?;
        self.write_report(&report)
    }

    fn write_safe_state(&mut self) -> Result<(), ViiperDs4SessionError> {
        self.write_state(
            GamepadControls::neutral(),
            ViiperDs4MotionState::stationary(),
        )
    }

    fn write_report(&mut self, report: &[u8]) -> Result<(), ViiperDs4SessionError> {
        self.stream_mut()?
            .write_all(report)
            .map_err(|error| ViiperDs4SessionError::StreamWrite(error.to_string()))
    }

    fn truncated_feedback<T>(&mut self, actual: usize) -> Result<T, ViiperDs4SessionError> {
        self.fail_stream();
        Err(ViiperDs4SessionError::TruncatedFeedback {
            actual,
            expected: VIIPER_DS4_FEEDBACK_BYTES,
        })
    }

    fn fail_stream(&mut self) {
        self.shutdown_stream();
        self.state = ViiperDs4WorkerState::Failed;
    }

    fn shutdown_stream(&mut self) {
        if let Some(stream) = self.stream.take() {
            let _ = stream.shutdown(Shutdown::Both);
        }
    }
}

impl Drop for ViiperDs4Worker {
    fn drop(&mut self) {
        self.shutdown_stream();
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Ds4DeviceResponse {
    #[serde(rename = "busId")]
    bus_id: u32,
    #[serde(rename = "devId")]
    device_id: String,
    vid: String,
    pid: String,
    #[serde(rename = "type")]
    device_type: String,
    #[serde(rename = "deviceSpecific")]
    device_specific: Value,
}

fn decode_ds4_device(
    response: Vec<u8>,
    expected_bus_id: u32,
) -> Result<Ds4DeviceResponse, ViiperDs4SessionError> {
    let device: Ds4DeviceResponse = decode_response(&response).map_err(map_shared_session_error)?;
    if device.bus_id != expected_bus_id {
        return Err(ViiperDs4SessionError::ResponseMismatch("device bus ID"));
    }
    validate_device_id(&device.device_id).map_err(|_| ViiperDs4SessionError::InvalidDeviceId)?;
    if device.vid != DS4_VENDOR_ID {
        return Err(ViiperDs4SessionError::ResponseMismatch("vendor ID"));
    }
    if device.pid != DS4_PRODUCT_ID {
        return Err(ViiperDs4SessionError::ResponseMismatch("product ID"));
    }
    if device.device_type != DS4_TYPE {
        return Err(ViiperDs4SessionError::ResponseMismatch("device type"));
    }
    if !device.device_specific.is_object() {
        return Err(ViiperDs4SessionError::ResponseMismatch(
            "device-specific metadata",
        ));
    }
    Ok(device)
}

fn motion_header(motion: &DataEnvelope<ImuSampleV1>) -> InputFrameHeader {
    InputFrameHeader {
        stream_id: motion.stream_id,
        stream_epoch: motion.stream_epoch,
        sequence: motion.sequence,
        source_timestamp_nanos: motion.source_timestamp_nanos,
    }
}

fn map_shared_session_error(error: ViiperSessionError) -> ViiperDs4SessionError {
    match error {
        ViiperSessionError::Client(error) => ViiperDs4SessionError::Client(error),
        ViiperSessionError::Input(error) => ViiperDs4SessionError::Input(error),
        ViiperSessionError::InvalidBusId => ViiperDs4SessionError::InvalidBusId,
        ViiperSessionError::InvalidDeviceId => ViiperDs4SessionError::InvalidDeviceId,
        ViiperSessionError::ResponseMismatch(detail) => {
            ViiperDs4SessionError::ResponseMismatch(detail)
        }
        ViiperSessionError::StreamConnectTimedOut => {
            ViiperDs4SessionError::StreamConnect("timed out".to_owned())
        }
        ViiperSessionError::StreamConnectFailed(error) => {
            ViiperDs4SessionError::StreamConnect(error)
        }
        ViiperSessionError::StreamConfigurationFailed(error) => {
            ViiperDs4SessionError::StreamConfiguration(error)
        }
        ViiperSessionError::StreamWriteFailed(error) => ViiperDs4SessionError::StreamWrite(error),
        ViiperSessionError::StreamReadFailed(error) => ViiperDs4SessionError::StreamRead(error),
        ViiperSessionError::StreamPeerClosed => ViiperDs4SessionError::StreamPeerClosed,
        ViiperSessionError::TruncatedFeedback { actual, expected } => {
            ViiperDs4SessionError::TruncatedFeedback { actual, expected }
        }
        ViiperSessionError::Projection(_) | ViiperSessionError::WorkerNotRunning(_) => {
            ViiperDs4SessionError::ResponseMismatch("unexpected shared Xbox session state")
        }
    }
}
