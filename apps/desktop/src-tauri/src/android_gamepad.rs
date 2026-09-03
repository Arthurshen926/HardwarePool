use std::{
    io,
    net::{Ipv4Addr, SocketAddrV4, UdpSocket},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use capyio_core::StreamId;
use capyio_data_plane::{
    DataEnvelope, ImuAccuracy, ImuCalibration, ImuComponentTimestampsV1, ImuCoordinateFrame,
    ImuSampleV1, ImuSensorMetadataV1, ImuUnitsV1,
};
use capyio_dsu_adapter::{DsuGamepadWorkerSender, DsuImuWorkerSender, DsuSubmitOutcome};
use capyio_input::{
    DpadState, GamepadButton, GamepadButtons, GamepadControls, GamepadState, InputFrameHeader,
    SignedAxis, StickState, TriggerValue,
};
use serde::Deserialize;

const MAX_PACKET_BYTES: usize = 2_048;
const MAX_SESSION_BYTES: usize = 64;
const PEER_TIMEOUT: Duration = Duration::from_millis(350);
const RECEIVE_POLL: Duration = Duration::from_millis(40);

#[derive(Clone, Debug)]
pub(crate) struct AndroidAcceptedState {
    pub(crate) controls: GamepadState,
    pub(crate) motion: DataEnvelope<ImuSampleV1>,
    pub(crate) remote_sequence: u64,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct AndroidReceiverSnapshot {
    pub(crate) accepted_packets: u64,
    pub(crate) rejected_packets: u64,
    pub(crate) replayed_packets: u64,
    pub(crate) peer_timeouts: u64,
    pub(crate) projection_queue_full: u64,
    pub(crate) peer_connected: bool,
    pub(crate) packet_age_millis: Option<u64>,
    pub(crate) latest: Option<AndroidAcceptedState>,
    pub(crate) last_event: &'static str,
}

#[derive(Clone, Debug)]
pub(crate) enum AndroidProjectionEvent {
    State(Box<AndroidAcceptedState>),
    UpstreamOffline,
}

#[derive(Clone)]
enum ProjectionSenders {
    Dsu {
        motion: DsuImuWorkerSender,
        controls: DsuGamepadWorkerSender,
    },
    PairedState(SyncSender<AndroidProjectionEvent>),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AndroidControllerPacket {
    version: u8,
    token: String,
    session: String,
    sequence: u64,
    timestamp_nanos: u64,
    buttons: u32,
    dpad_x: i8,
    dpad_y: i8,
    left_x: i32,
    left_y: i32,
    right_x: i32,
    right_y: i32,
    left_trigger: u32,
    right_trigger: u32,
    acceleration: [f64; 3],
    angular_velocity: [f64; 3],
    acceleration_timestamp_nanos: u64,
    angular_velocity_timestamp_nanos: u64,
}

struct ValidatedPacket {
    session: String,
    remote_sequence: u64,
    timestamp_nanos: u64,
    controls: GamepadControls,
    acceleration: [f64; 3],
    angular_velocity: [f64; 3],
    acceleration_timestamp_nanos: u64,
    angular_velocity_timestamp_nanos: u64,
}

#[derive(Clone, Copy)]
struct MotionFrame {
    timestamp_nanos: u64,
    acceleration: [f64; 3],
    angular_velocity: [f64; 3],
    acceleration_timestamp_nanos: u64,
    angular_velocity_timestamp_nanos: u64,
}

struct ReceiverState {
    snapshot: AndroidReceiverSnapshot,
    controls_stream_id: StreamId,
    motion_stream_id: StreamId,
    stream_epoch: u64,
    next_controls_sequence: u64,
    next_motion_sequence: u64,
}

impl ReceiverState {
    fn new() -> Self {
        Self {
            snapshot: AndroidReceiverSnapshot {
                last_event: "listening",
                ..AndroidReceiverSnapshot::default()
            },
            controls_stream_id: StreamId::new(),
            motion_stream_id: StreamId::new(),
            stream_epoch: 1,
            next_controls_sequence: 0,
            next_motion_sequence: 0,
        }
    }

    fn controls_anchor(&self) -> GamepadState {
        GamepadState {
            header: InputFrameHeader {
                stream_id: self.controls_stream_id,
                stream_epoch: self.stream_epoch,
                sequence: self.next_controls_sequence,
                source_timestamp_nanos: 1,
            },
            controls: GamepadControls::neutral(),
        }
    }

    fn motion_anchor(&self) -> DataEnvelope<ImuSampleV1> {
        motion_envelope(
            self.motion_stream_id,
            self.stream_epoch,
            self.next_motion_sequence,
            MotionFrame {
                timestamp_nanos: 1,
                acceleration: [0.0, 0.0, 9.806_65],
                angular_velocity: [0.0; 3],
                acceleration_timestamp_nanos: 1,
                angular_velocity_timestamp_nanos: 1,
            },
        )
    }

    fn accept(&mut self, packet: ValidatedPacket) -> AndroidAcceptedState {
        let controls = GamepadState {
            header: InputFrameHeader {
                stream_id: self.controls_stream_id,
                stream_epoch: self.stream_epoch,
                sequence: self.next_controls_sequence,
                source_timestamp_nanos: packet.timestamp_nanos,
            },
            controls: packet.controls,
        };
        let motion = motion_envelope(
            self.motion_stream_id,
            self.stream_epoch,
            self.next_motion_sequence,
            MotionFrame {
                timestamp_nanos: packet.timestamp_nanos,
                acceleration: packet.acceleration,
                angular_velocity: packet.angular_velocity,
                acceleration_timestamp_nanos: packet.acceleration_timestamp_nanos,
                angular_velocity_timestamp_nanos: packet.angular_velocity_timestamp_nanos,
            },
        );
        self.next_controls_sequence = self.next_controls_sequence.saturating_add(1);
        self.next_motion_sequence = self.next_motion_sequence.saturating_add(1);
        AndroidAcceptedState {
            controls,
            motion,
            remote_sequence: packet.remote_sequence,
        }
    }

    fn neutralize(&mut self, timestamp_nanos: u64) -> AndroidAcceptedState {
        let remote_sequence = self
            .snapshot
            .latest
            .as_ref()
            .map_or(0, |latest| latest.remote_sequence);
        let controls = GamepadState {
            header: InputFrameHeader {
                stream_id: self.controls_stream_id,
                stream_epoch: self.stream_epoch,
                sequence: self.next_controls_sequence,
                source_timestamp_nanos: timestamp_nanos,
            },
            controls: GamepadControls::neutral(),
        };
        let motion = motion_envelope(
            self.motion_stream_id,
            self.stream_epoch,
            self.next_motion_sequence,
            MotionFrame {
                timestamp_nanos,
                acceleration: [0.0, 0.0, 9.806_65],
                angular_velocity: [0.0; 3],
                acceleration_timestamp_nanos: timestamp_nanos,
                angular_velocity_timestamp_nanos: timestamp_nanos,
            },
        );
        self.next_controls_sequence = self.next_controls_sequence.saturating_add(1);
        self.next_motion_sequence = self.next_motion_sequence.saturating_add(1);
        let neutral = AndroidAcceptedState {
            controls,
            motion,
            remote_sequence,
        };
        self.snapshot.latest = Some(neutral.clone());
        neutral
    }
}

pub(crate) struct AndroidGamepadListener {
    local_address: SocketAddrV4,
    token: String,
    state: Arc<Mutex<ReceiverState>>,
    projection: Arc<Mutex<Option<ProjectionSenders>>>,
    stopped: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

#[must_use]
pub(crate) fn lan_ipv4_hint() -> Option<String> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((Ipv4Addr::new(192, 0, 2, 1), 9)).ok()?;
    match socket.local_addr().ok()? {
        std::net::SocketAddr::V4(address) if !address.ip().is_unspecified() => {
            Some(address.ip().to_string())
        }
        _ => None,
    }
}

impl AndroidGamepadListener {
    pub(crate) fn start(port: u16, token: String) -> Result<Self, String> {
        if token.len() < 8
            || token.len() > 64
            || !token.bytes().all(|value| value.is_ascii_hexdigit())
        {
            return Err(
                "Android controller pairing token must contain 8..=64 hexadecimal characters"
                    .to_owned(),
            );
        }
        let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port))
            .map_err(|error| format!("failed to bind Android controller UDP listener: {error}"))?;
        socket
            .set_read_timeout(Some(RECEIVE_POLL))
            .map_err(|error| format!("failed to configure Android controller listener: {error}"))?;
        let local_address = match socket
            .local_addr()
            .map_err(|error| format!("failed to inspect Android controller listener: {error}"))?
        {
            std::net::SocketAddr::V4(address) => address,
            std::net::SocketAddr::V6(_) => {
                return Err("Android controller listener unexpectedly bound IPv6".to_owned());
            }
        };
        let state = Arc::new(Mutex::new(ReceiverState::new()));
        let projection = Arc::new(Mutex::new(None::<ProjectionSenders>));
        let stopped = Arc::new(AtomicBool::new(false));
        let thread_state = Arc::clone(&state);
        let thread_projection = Arc::clone(&projection);
        let thread_stopped = Arc::clone(&stopped);
        let expected_token = token.clone();
        let thread = thread::Builder::new()
            .name("capyio-android-gamepad".to_owned())
            .spawn(move || {
                run_listener(
                    socket,
                    &expected_token,
                    &thread_state,
                    &thread_projection,
                    &thread_stopped,
                );
            })
            .map_err(|error| format!("failed to start Android controller listener: {error}"))?;
        Ok(Self {
            local_address,
            token,
            state,
            projection,
            stopped,
            thread: Some(thread),
        })
    }

    #[must_use]
    pub(crate) const fn local_address(&self) -> SocketAddrV4 {
        self.local_address
    }

    #[must_use]
    pub(crate) fn token(&self) -> &str {
        &self.token
    }

    pub(crate) fn snapshot(&self) -> AndroidReceiverSnapshot {
        self.state
            .lock()
            .map(|state| {
                let mut snapshot = state.snapshot.clone();
                snapshot.packet_age_millis = state.snapshot.packet_age_millis;
                snapshot
            })
            .unwrap_or_else(|_| AndroidReceiverSnapshot {
                last_event: "state_lock_poisoned",
                ..AndroidReceiverSnapshot::default()
            })
    }

    pub(crate) fn anchors(&self) -> Result<(DataEnvelope<ImuSampleV1>, GamepadState), String> {
        self.state
            .lock()
            .map(|state| (state.motion_anchor(), state.controls_anchor()))
            .map_err(|_| "Android controller state lock poisoned".to_owned())
    }

    pub(crate) fn set_projection(
        &self,
        motion: Option<DsuImuWorkerSender>,
        controls: Option<DsuGamepadWorkerSender>,
    ) -> Result<(), String> {
        let mut slot = self
            .projection
            .lock()
            .map_err(|_| "Android controller projection lock poisoned".to_owned())?;
        *slot = match (motion, controls) {
            (Some(motion), Some(controls)) => Some(ProjectionSenders::Dsu { motion, controls }),
            (None, None) => None,
            _ => {
                return Err(
                    "Android controller projection requires both motion and controls senders"
                        .to_owned(),
                );
            }
        };
        Ok(())
    }

    /// Connects the complete-state stream to a bounded host-owned projection
    /// Worker. The UDP receiver never waits for VIIPER, USB/IP or Runtime I/O.
    pub(crate) fn set_paired_projection(
        &self,
        sender: Option<SyncSender<AndroidProjectionEvent>>,
    ) -> Result<(), String> {
        let mut slot = self
            .projection
            .lock()
            .map_err(|_| "Android controller projection lock poisoned".to_owned())?;
        *slot = sender.map(ProjectionSenders::PairedState);
        Ok(())
    }

    pub(crate) fn stop(&mut self) -> Result<AndroidReceiverSnapshot, String> {
        if self.stopped.swap(true, Ordering::AcqRel) {
            return Ok(self.snapshot());
        }
        if let Ok(mut projection) = self.projection.lock()
            && let Some(senders) = projection.take()
        {
            request_projection_offline(&senders);
        }
        if let Some(thread) = self.thread.take() {
            thread
                .join()
                .map_err(|_| "Android controller listener thread panicked".to_owned())?;
        }
        Ok(self.snapshot())
    }
}

impl Drop for AndroidGamepadListener {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn run_listener(
    socket: UdpSocket,
    expected_token: &str,
    state: &Arc<Mutex<ReceiverState>>,
    projection: &Arc<Mutex<Option<ProjectionSenders>>>,
    stopped: &Arc<AtomicBool>,
) {
    let started = Instant::now();
    let mut buffer = [0_u8; MAX_PACKET_BYTES + 1];
    let mut last_session = None::<String>;
    let mut last_remote_sequence = None::<u64>;
    let mut last_received = None::<Instant>;
    let mut timeout_neutralized = false;
    while !stopped.load(Ordering::Acquire) {
        match socket.recv_from(&mut buffer) {
            Ok((length, _peer)) => {
                if length > MAX_PACKET_BYTES {
                    reject(state, "packet_too_large");
                    continue;
                }
                let packet = match decode_packet(&buffer[..length], expected_token) {
                    Ok(packet) => packet,
                    Err(_) => {
                        reject(state, "packet_rejected");
                        continue;
                    }
                };
                if last_session.as_deref() == Some(packet.session.as_str())
                    && last_remote_sequence
                        .is_some_and(|sequence| packet.remote_sequence <= sequence)
                {
                    if let Ok(mut guard) = state.lock() {
                        guard.snapshot.replayed_packets =
                            guard.snapshot.replayed_packets.saturating_add(1);
                        guard.snapshot.last_event = "replay_rejected";
                    }
                    continue;
                }
                if last_session.as_deref() != Some(packet.session.as_str()) {
                    last_session = Some(packet.session.clone());
                }
                last_remote_sequence = Some(packet.remote_sequence);
                let accepted = {
                    let Ok(mut guard) = state.lock() else {
                        break;
                    };
                    let accepted = guard.accept(packet);
                    guard.snapshot.accepted_packets =
                        guard.snapshot.accepted_packets.saturating_add(1);
                    guard.snapshot.peer_connected = true;
                    guard.snapshot.packet_age_millis = Some(0);
                    guard.snapshot.latest = Some(accepted.clone());
                    guard.snapshot.last_event = "packet_accepted";
                    accepted
                };
                last_received = Some(Instant::now());
                timeout_neutralized = false;
                if let Some(senders) = projection.lock().ok().and_then(|slot| slot.clone())
                    && let Some(event) = submit_projection_state(&senders, accepted)
                    && let Ok(mut guard) = state.lock()
                {
                    guard.snapshot.projection_queue_full =
                        guard.snapshot.projection_queue_full.saturating_add(1);
                    guard.snapshot.last_event = event;
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(_) => {
                if let Ok(mut guard) = state.lock() {
                    guard.snapshot.last_event = "socket_failed";
                }
                break;
            }
        }
        if let Some(received) = last_received {
            let age = received.elapsed();
            if let Ok(mut guard) = state.lock() {
                guard.snapshot.packet_age_millis =
                    Some(u64::try_from(age.as_millis()).unwrap_or(u64::MAX));
            }
            if age >= PEER_TIMEOUT && !timeout_neutralized {
                let neutral = if let Ok(mut guard) = state.lock() {
                    let timestamp = u64::try_from(started.elapsed().as_nanos())
                        .unwrap_or(u64::MAX)
                        .saturating_add(1);
                    let neutral = guard.neutralize(timestamp);
                    guard.snapshot.peer_connected = false;
                    guard.snapshot.peer_timeouts = guard.snapshot.peer_timeouts.saturating_add(1);
                    guard.snapshot.last_event = "peer_timeout_neutral";
                    Some(neutral)
                } else {
                    None
                };
                if let Some(neutral) = neutral
                    && let Some(senders) = projection.lock().ok().and_then(|slot| slot.clone())
                    && let Some(event) = submit_projection_state(&senders, neutral)
                    && let Ok(mut guard) = state.lock()
                {
                    guard.snapshot.projection_queue_full =
                        guard.snapshot.projection_queue_full.saturating_add(1);
                    guard.snapshot.last_event = event;
                }
                timeout_neutralized = true;
            }
        }
    }
}

fn submit_projection_state(
    projection: &ProjectionSenders,
    accepted: AndroidAcceptedState,
) -> Option<&'static str> {
    match projection {
        ProjectionSenders::Dsu { motion, controls } => {
            let motion = motion.try_submit(accepted.motion);
            let controls = controls.try_submit(accepted.controls);
            (matches!(motion, DsuSubmitOutcome::QueueFull)
                || matches!(controls, DsuSubmitOutcome::QueueFull))
            .then_some("projection_queue_full")
        }
        ProjectionSenders::PairedState(sender) => {
            match sender.try_send(AndroidProjectionEvent::State(Box::new(accepted))) {
                Ok(()) => None,
                Err(TrySendError::Full(_)) => Some("projection_queue_full"),
                Err(TrySendError::Disconnected(_)) => Some("projection_worker_stopped"),
            }
        }
    }
}

fn request_projection_offline(projection: &ProjectionSenders) {
    match projection {
        ProjectionSenders::Dsu { controls, .. } => {
            let _ = controls.request_neutral();
        }
        ProjectionSenders::PairedState(sender) => {
            let _ = sender.try_send(AndroidProjectionEvent::UpstreamOffline);
        }
    }
}

fn reject(state: &Arc<Mutex<ReceiverState>>, event: &'static str) {
    if let Ok(mut guard) = state.lock() {
        guard.snapshot.rejected_packets = guard.snapshot.rejected_packets.saturating_add(1);
        guard.snapshot.last_event = event;
    }
}

fn decode_packet(bytes: &[u8], expected_token: &str) -> Result<ValidatedPacket, String> {
    let packet: AndroidControllerPacket =
        serde_json::from_slice(bytes).map_err(|_| "invalid JSON packet".to_owned())?;
    if packet.version != 1 {
        return Err("unsupported Android controller packet version".to_owned());
    }
    if !tokens_equal(packet.token.as_bytes(), expected_token.as_bytes()) {
        return Err("pairing token mismatch".to_owned());
    }
    if packet.session.is_empty()
        || packet.session.len() > MAX_SESSION_BYTES
        || !packet
            .session
            .bytes()
            .all(|value| value.is_ascii_hexdigit() || value == b'-')
    {
        return Err("invalid Android controller session".to_owned());
    }
    if packet.timestamp_nanos == 0
        || packet.acceleration_timestamp_nanos == 0
        || packet.angular_velocity_timestamp_nanos == 0
    {
        return Err("Android controller timestamps must be positive".to_owned());
    }
    let mut buttons = GamepadButtons::empty();
    for button in [
        GamepadButton::South,
        GamepadButton::East,
        GamepadButton::West,
        GamepadButton::North,
        GamepadButton::LeftShoulder,
        GamepadButton::RightShoulder,
        GamepadButton::LeftStick,
        GamepadButton::RightStick,
        GamepadButton::Select,
        GamepadButton::Start,
        GamepadButton::Guide,
        GamepadButton::Touchpad,
        GamepadButton::Paddle1,
        GamepadButton::Paddle2,
        GamepadButton::Paddle3,
        GamepadButton::Paddle4,
    ] {
        if packet.buttons & (1_u32 << button as u8) != 0 {
            buttons = buttons.with(button);
        }
    }
    if packet.buttons & !0xffff != 0 {
        return Err("Android controller button mask has unknown bits".to_owned());
    }
    let axis = |value: i32| {
        let narrowed =
            i16::try_from(value).map_err(|_| "gamepad axis is outside i16".to_owned())?;
        SignedAxis::new(narrowed).map_err(|error| error.to_string())
    };
    let trigger = |value: u32| {
        u16::try_from(value)
            .map(TriggerValue::new)
            .map_err(|_| "gamepad trigger is outside u16".to_owned())
    };
    let controls = GamepadControls {
        buttons,
        dpad: DpadState {
            x: packet.dpad_x,
            y: packet.dpad_y,
        },
        left_stick: StickState {
            x: axis(packet.left_x)?,
            y: axis(packet.left_y)?,
        },
        right_stick: StickState {
            x: axis(packet.right_x)?,
            y: axis(packet.right_y)?,
        },
        left_trigger: trigger(packet.left_trigger)?,
        right_trigger: trigger(packet.right_trigger)?,
    };
    controls.validate().map_err(|error| error.to_string())?;
    for value in packet
        .acceleration
        .iter()
        .chain(packet.angular_velocity.iter())
    {
        if !value.is_finite() {
            return Err("Android controller IMU axes must be finite".to_owned());
        }
    }
    Ok(ValidatedPacket {
        session: packet.session,
        remote_sequence: packet.sequence,
        timestamp_nanos: packet.timestamp_nanos,
        controls,
        acceleration: packet.acceleration,
        angular_velocity: packet.angular_velocity,
        acceleration_timestamp_nanos: packet.acceleration_timestamp_nanos,
        angular_velocity_timestamp_nanos: packet.angular_velocity_timestamp_nanos,
    })
}

fn motion_envelope(
    stream_id: StreamId,
    stream_epoch: u64,
    sequence: u64,
    frame: MotionFrame,
) -> DataEnvelope<ImuSampleV1> {
    DataEnvelope {
        profile: ImuSampleV1::profile(),
        stream_id,
        stream_epoch,
        sequence,
        source_timestamp_nanos: frame.timestamp_nanos,
        receive_timestamp_nanos: frame.timestamp_nanos,
        clock_domain_id: "android.sensor.elapsed_realtime".to_owned(),
        payload: ImuSampleV1 {
            acceleration: frame.acceleration,
            angular_velocity: frame.angular_velocity,
            magnetic_field: None,
            units: ImuUnitsV1::default(),
            coordinate_frame: ImuCoordinateFrame::AndroidDeviceXRightYUpZOut,
            accuracy: ImuAccuracy::Unreliable,
            calibration: ImuCalibration::FactoryCalibrated,
            sensor: ImuSensorMetadataV1 {
                sensor_name: "Android Controller Lab accelerometer + gyroscope".to_owned(),
                vendor: "Android SensorManager".to_owned(),
                version: 1,
                android_sensor_type: None,
            },
            component_timestamps: Some(ImuComponentTimestampsV1 {
                acceleration_nanos: frame.acceleration_timestamp_nanos,
                angular_velocity_nanos: frame.angular_velocity_timestamp_nanos,
                magnetic_field_nanos: None,
            }),
        },
    }
}

fn tokens_equal(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::UdpSocket;
    use std::sync::mpsc;

    const TOKEN: &str = "1234abcd";

    fn packet(sequence: u64, buttons: u32) -> String {
        format!(
            "{{\"version\":1,\"token\":\"{TOKEN}\",\"session\":\"abc-123\",\"sequence\":{sequence},\"timestampNanos\":100,\"buttons\":{buttons},\"dpadX\":0,\"dpadY\":0,\"leftX\":0,\"leftY\":0,\"rightX\":0,\"rightY\":0,\"leftTrigger\":0,\"rightTrigger\":0,\"acceleration\":[0.0,0.0,9.8],\"angularVelocity\":[0.1,0.2,0.3],\"accelerationTimestampNanos\":90,\"angularVelocityTimestampNanos\":95}}"
        )
    }

    fn loopback_target(listener: &AndroidGamepadListener) -> SocketAddrV4 {
        SocketAddrV4::new(Ipv4Addr::LOCALHOST, listener.local_address().port())
    }

    #[test]
    fn packet_decoder_rejects_unknown_fields_and_reserved_axis() {
        assert!(decode_packet(packet(0, 1).as_bytes(), TOKEN).is_ok());
        let unknown = packet(0, 0).replace("{\"version\":1", "{\"extra\":1,\"version\":1");
        assert!(decode_packet(unknown.as_bytes(), TOKEN).is_err());
        let reserved = packet(0, 0).replace("\"leftX\":0", "\"leftX\":-32768");
        assert!(decode_packet(reserved.as_bytes(), TOKEN).is_err());
        assert!(decode_packet(packet(0, 0).as_bytes(), "deadbeef").is_err());
    }

    #[test]
    fn listener_accepts_packet_and_rejects_replay() {
        let mut listener = AndroidGamepadListener::start(0, TOKEN.to_owned()).expect("listener");
        let sender = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).expect("sender");
        sender
            .send_to(packet(7, 1).as_bytes(), loopback_target(&listener))
            .expect("first packet");
        sender
            .send_to(packet(7, 0).as_bytes(), loopback_target(&listener))
            .expect("replay packet");
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let snapshot = listener.snapshot();
            if snapshot.accepted_packets == 1 && snapshot.replayed_packets == 1 {
                let latest = snapshot.latest.expect("accepted state");
                assert!(
                    latest
                        .controls
                        .controls
                        .buttons
                        .contains(GamepadButton::South)
                );
                assert_eq!(latest.remote_sequence, 7);
                break;
            }
            assert!(
                Instant::now() < deadline,
                "listener did not process packets"
            );
            thread::yield_now();
        }
        listener.stop().expect("stop listener");
    }

    #[test]
    fn peer_timeout_requests_neutral_state() {
        let mut listener = AndroidGamepadListener::start(0, TOKEN.to_owned()).expect("listener");
        let sender = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).expect("sender");
        sender
            .send_to(packet(1, 1).as_bytes(), loopback_target(&listener))
            .expect("packet");
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let snapshot = listener.snapshot();
            if snapshot.peer_timeouts == 1 {
                assert!(!snapshot.peer_connected);
                assert_eq!(
                    snapshot.latest.expect("latest").controls.controls,
                    GamepadControls::neutral()
                );
                break;
            }
            assert!(Instant::now() < deadline, "peer did not time out");
            thread::sleep(Duration::from_millis(10));
        }
        listener.stop().expect("stop listener");
    }

    #[test]
    fn paired_projection_timeout_sends_complete_neutral_without_offlining_route() {
        let mut listener = AndroidGamepadListener::start(0, TOKEN.to_owned()).expect("listener");
        let (projection, events) = mpsc::sync_channel(4);
        listener
            .set_paired_projection(Some(projection))
            .expect("paired projection");
        UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))
            .expect("sender")
            .send_to(packet(3, 11).as_bytes(), loopback_target(&listener))
            .expect("packet");

        assert!(matches!(
            events.recv_timeout(Duration::from_secs(2)),
            Ok(AndroidProjectionEvent::State(_))
        ));
        let neutral = events
            .recv_timeout(Duration::from_secs(2))
            .expect("timeout neutral event");
        let AndroidProjectionEvent::State(neutral) = neutral else {
            panic!("timeout must not offline the paired projection");
        };
        assert_eq!(neutral.controls.controls, GamepadControls::neutral());
        assert_eq!(neutral.motion.payload.angular_velocity, [0.0; 3]);
        assert!(events.try_recv().is_err());

        listener.stop().expect("stop listener");
        assert!(matches!(
            events.recv_timeout(Duration::from_secs(2)),
            Ok(AndroidProjectionEvent::UpstreamOffline)
        ));
    }

    #[test]
    fn paired_projection_delivers_complete_state_then_explicit_offline() {
        let mut listener = AndroidGamepadListener::start(0, TOKEN.to_owned()).expect("listener");
        let (projection, events) = mpsc::sync_channel(2);
        listener
            .set_paired_projection(Some(projection))
            .expect("paired projection");
        let sender = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).expect("sender");
        sender
            .send_to(packet(9, 1).as_bytes(), loopback_target(&listener))
            .expect("packet");

        let event = events
            .recv_timeout(Duration::from_secs(2))
            .expect("state event");
        let AndroidProjectionEvent::State(accepted) = event else {
            panic!("expected complete state before offline");
        };
        assert_eq!(accepted.remote_sequence, 9);
        assert_eq!(
            accepted.controls.header.sequence, accepted.motion.sequence,
            "one Android packet remains one paired projection unit"
        );
        assert!(
            accepted
                .controls
                .controls
                .buttons
                .contains(GamepadButton::South)
        );
        assert_eq!(accepted.motion.payload.angular_velocity, [0.1, 0.2, 0.3]);

        let timeout = events
            .recv_timeout(Duration::from_secs(2))
            .expect("timeout neutral event");
        assert!(matches!(timeout, AndroidProjectionEvent::State(_)));
        listener.stop().expect("stop listener");
        assert!(matches!(
            events.recv_timeout(Duration::from_secs(2)),
            Ok(AndroidProjectionEvent::UpstreamOffline)
        ));
    }
}
