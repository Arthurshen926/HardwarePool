use std::time::Instant;

#[cfg(debug_assertions)]
use std::io::{self, Write};
#[cfg(debug_assertions)]
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
#[cfg(debug_assertions)]
use std::thread;
#[cfg(debug_assertions)]
use std::time::Duration;

use capyio_core::StreamId;
use capyio_data_plane::{
    DataEnvelope, ImuAccuracy, ImuCalibration, ImuCoordinateFrame, ImuSampleV1,
    ImuSensorMetadataV1, ImuUnitsV1,
};
#[cfg(debug_assertions)]
use capyio_dsu_adapter::{DSU_PROTOCOL_VERSION, crc32_ieee};
use capyio_dsu_adapter::{
    DsuImuWorker, DsuImuWorkerConfig, DsuImuWorkerStats, DsuLoopbackConfig, DsuNeutralOutcome,
    DsuSubmitOutcome,
};
use capyio_input::{
    DpadState, GamepadButton, GamepadControlUpdate, GamepadControls, GamepadStateComposer,
    GamepadStick, GamepadTrigger, SignedAxis, StickState, TriggerValue,
};
use capyio_runtime::NodeRuntime;
#[cfg(debug_assertions)]
use capyio_testkit::DemoLab;
#[cfg(debug_assertions)]
use capyio_viiper_adapter::{
    MAX_VIIPER_MANAGEMENT_RESPONSE_BYTES, ViiperAutoAttachDisabled, ViiperDs4ControlsMapping,
    ViiperDs4MotionMapping, ViiperLoopbackClient, ViiperLoopbackConfig, ViiperXbox360Mapping,
};
use serde::{Deserialize, Serialize};

use crate::android_gamepad::{AndroidGamepadListener, AndroidReceiverSnapshot, lan_ipv4_hint};
use crate::windows_gamepad_host::{UiWindowsGamepadProjection, WindowsGamepadHost};

const GAMEPAD_PROFILE: &str = "capyio.input.gamepad-state/1";
const DSU_SERVER_ID: u32 = 0x4341_5059;

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum UiGamepadButton {
    South,
    East,
    West,
    North,
    LeftShoulder,
    RightShoulder,
    LeftStick,
    RightStick,
    Select,
    Start,
    Guide,
    Touchpad,
    Paddle1,
    Paddle2,
    Paddle3,
    Paddle4,
}

impl UiGamepadButton {
    const ALL: [Self; 16] = [
        Self::South,
        Self::East,
        Self::West,
        Self::North,
        Self::LeftShoulder,
        Self::RightShoulder,
        Self::LeftStick,
        Self::RightStick,
        Self::Select,
        Self::Start,
        Self::Guide,
        Self::Touchpad,
        Self::Paddle1,
        Self::Paddle2,
        Self::Paddle3,
        Self::Paddle4,
    ];

    const fn semantic(self) -> GamepadButton {
        match self {
            Self::South => GamepadButton::South,
            Self::East => GamepadButton::East,
            Self::West => GamepadButton::West,
            Self::North => GamepadButton::North,
            Self::LeftShoulder => GamepadButton::LeftShoulder,
            Self::RightShoulder => GamepadButton::RightShoulder,
            Self::LeftStick => GamepadButton::LeftStick,
            Self::RightStick => GamepadButton::RightStick,
            Self::Select => GamepadButton::Select,
            Self::Start => GamepadButton::Start,
            Self::Guide => GamepadButton::Guide,
            Self::Touchpad => GamepadButton::Touchpad,
            Self::Paddle1 => GamepadButton::Paddle1,
            Self::Paddle2 => GamepadButton::Paddle2,
            Self::Paddle3 => GamepadButton::Paddle3,
            Self::Paddle4 => GamepadButton::Paddle4,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::South => "south",
            Self::East => "east",
            Self::West => "west",
            Self::North => "north",
            Self::LeftShoulder => "left_shoulder",
            Self::RightShoulder => "right_shoulder",
            Self::LeftStick => "left_stick",
            Self::RightStick => "right_stick",
            Self::Select => "select",
            Self::Start => "start",
            Self::Guide => "guide",
            Self::Touchpad => "touchpad",
            Self::Paddle1 => "paddle1",
            Self::Paddle2 => "paddle2",
            Self::Paddle3 => "paddle3",
            Self::Paddle4 => "paddle4",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum UiGamepadStick {
    Left,
    Right,
}

impl UiGamepadStick {
    const fn semantic(self) -> GamepadStick {
        match self {
            Self::Left => GamepadStick::Left,
            Self::Right => GamepadStick::Right,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum UiGamepadTrigger {
    Left,
    Right,
}

impl UiGamepadTrigger {
    const fn semantic(self) -> GamepadTrigger {
        match self {
            Self::Left => GamepadTrigger::Left,
            Self::Right => GamepadTrigger::Right,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum UpdateGamepadRequest {
    Button {
        button: UiGamepadButton,
        pressed: bool,
    },
    Dpad {
        x: i8,
        y: i8,
    },
    Stick {
        stick: UiGamepadStick,
        x: i16,
        y: i16,
    },
    Trigger {
        trigger: UiGamepadTrigger,
        value: u16,
    },
    Reset,
}

impl UpdateGamepadRequest {
    fn semantic(&self) -> Result<GamepadControlUpdate, String> {
        match *self {
            Self::Button { button, pressed } => Ok(GamepadControlUpdate::Button {
                button: button.semantic(),
                pressed,
            }),
            Self::Dpad { x, y } => {
                let state = DpadState { x, y };
                state.validate().map_err(|error| error.to_string())?;
                Ok(GamepadControlUpdate::Dpad(state))
            }
            Self::Stick { stick, x, y } => Ok(GamepadControlUpdate::Stick {
                stick: stick.semantic(),
                state: StickState {
                    x: SignedAxis::new(x).map_err(|error| error.to_string())?,
                    y: SignedAxis::new(y).map_err(|error| error.to_string())?,
                },
            }),
            Self::Trigger { trigger, value } => Ok(GamepadControlUpdate::Trigger {
                trigger: trigger.semantic(),
                value: TriggerValue::new(value),
            }),
            Self::Reset => Ok(GamepadControlUpdate::Reset),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UiGamepadState {
    schema_version: u8,
    source: &'static str,
    simulated: bool,
    profile: &'static str,
    stream_epoch: u64,
    sequence: Option<u64>,
    source_timestamp_nanos: Option<u64>,
    pressed_buttons: Vec<&'static str>,
    dpad: UiDpadState,
    left_stick: UiStickState,
    right_stick: UiStickState,
    left_trigger: u16,
    right_trigger: u16,
    last_update: &'static str,
    dsu_projection: UiDsuProjection,
    windows_projection: UiWindowsGamepadProjection,
    android_input: UiAndroidInput,
    motion: UiGamepadMotion,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiGamepadMotion {
    source: &'static str,
    source_timestamp_nanos: Option<u64>,
    acceleration: [f64; 3],
    angular_velocity: [f64; 3],
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiAndroidInput {
    supported: bool,
    status: &'static str,
    endpoint: Option<String>,
    lan_host_hint: Option<String>,
    pairing_token: Option<String>,
    peer_connected: bool,
    accepted_packets: u64,
    rejected_packets: u64,
    replayed_packets: u64,
    peer_timeouts: u64,
    projection_queue_full: u64,
    packet_age_millis: Option<u64>,
    remote_sequence: Option<u64>,
    last_event: &'static str,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiDsuProjection {
    supported: bool,
    status: &'static str,
    endpoint: Option<String>,
    mode: UiDsuMode,
    last_submit: &'static str,
    controls_submitted: u64,
    controls_accepted: u64,
    controls_queue_full: u64,
    controls_neutral_resets: u64,
    active_subscribers: u64,
    pad_packets_sent: u64,
    packet_send_errors: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum UiDsuMode {
    MotionOnly,
    MotionAndControls,
}

#[derive(Clone, Copy, Debug, Serialize)]
struct UiDpadState {
    x: i8,
    y: i8,
}

#[derive(Clone, Copy, Debug, Serialize)]
struct UiStickState {
    x: i16,
    y: i16,
}

pub(crate) struct GamepadLab {
    composer: GamepadStateComposer,
    started: Instant,
    last_sequence: Option<u64>,
    last_timestamp_nanos: Option<u64>,
    last_update: &'static str,
    dsu_worker: Option<DsuImuWorker>,
    last_dsu_stats: DsuImuWorkerStats,
    dsu_status: &'static str,
    dsu_endpoint: Option<String>,
    dsu_mode: UiDsuMode,
    last_dsu_submit: &'static str,
    android_listener: Option<AndroidGamepadListener>,
    last_android_snapshot: AndroidReceiverSnapshot,
    android_status: &'static str,
    android_endpoint: Option<String>,
    windows_host: WindowsGamepadHost,
}

impl GamepadLab {
    pub(crate) fn new() -> Result<Self, String> {
        Self::new_with_windows_host(WindowsGamepadHost::new())
    }

    pub(crate) fn new_with_windows_host(windows_host: WindowsGamepadHost) -> Result<Self, String> {
        Ok(Self {
            composer: GamepadStateComposer::new(StreamId::new(), 1, 0)
                .map_err(|error| error.to_string())?,
            started: Instant::now(),
            last_sequence: None,
            last_timestamp_nanos: None,
            last_update: "neutral.initial",
            dsu_worker: None,
            last_dsu_stats: DsuImuWorkerStats::default(),
            dsu_status: "idle",
            dsu_endpoint: None,
            dsu_mode: UiDsuMode::MotionAndControls,
            last_dsu_submit: "not_started",
            android_listener: None,
            last_android_snapshot: AndroidReceiverSnapshot::default(),
            android_status: "idle",
            android_endpoint: None,
            windows_host,
        })
    }

    pub(crate) fn snapshot(&self) -> UiGamepadState {
        if let Some(listener) = self.android_listener.as_ref() {
            let android = listener.snapshot();
            if let Some(latest) = android.latest.as_ref() {
                return self.to_ui(
                    latest.controls.controls,
                    "android_touch",
                    false,
                    latest.controls.header.stream_epoch,
                    Some(latest.controls.header.sequence),
                    Some(latest.controls.header.source_timestamp_nanos),
                    android.last_event,
                    &android,
                    Some(&latest.motion),
                );
            }
            return self.to_ui(
                GamepadControls::neutral(),
                "android_touch",
                false,
                1,
                None,
                None,
                android.last_event,
                &android,
                None,
            );
        }
        self.to_ui(
            self.composer.controls(),
            "desktop_simulator",
            true,
            self.composer.stream_epoch(),
            self.last_sequence,
            self.last_timestamp_nanos,
            self.last_update,
            &self.last_android_snapshot,
            None,
        )
    }

    pub(crate) fn apply(
        &mut self,
        request: UpdateGamepadRequest,
    ) -> Result<UiGamepadState, String> {
        if self.android_listener.is_some() {
            return Err(
                "desktop simulator is disabled while Android controller input is listening"
                    .to_owned(),
            );
        }
        let update = request.semantic()?;
        let timestamp = u64::try_from(self.started.elapsed().as_nanos())
            .unwrap_or(u64::MAX)
            .saturating_add(1);
        let state = self
            .composer
            .apply(update, timestamp)
            .map_err(|error| error.to_string())?;
        self.last_sequence = Some(state.header.sequence);
        self.last_timestamp_nanos = Some(state.header.source_timestamp_nanos);
        self.last_update = match request {
            UpdateGamepadRequest::Reset => "reset.neutral",
            _ => "control.update",
        };
        if let Some(worker) = self.dsu_worker.as_ref() {
            self.last_dsu_submit = match worker.controls_sender() {
                Some(sender) => match sender.try_submit(state) {
                    DsuSubmitOutcome::Submitted => "submitted",
                    DsuSubmitOutcome::QueueFull => "queue_full_neutral_requested",
                    DsuSubmitOutcome::Stopped => "worker_stopped",
                },
                None => "motion_only_controls_not_projected",
            };
        }
        Ok(self.snapshot())
    }

    pub(crate) fn apply_windows_preflight(
        &mut self,
        projection: UiWindowsGamepadProjection,
    ) -> UiGamepadState {
        self.windows_host.apply_read_only_preflight(projection);
        self.snapshot()
    }

    pub(crate) fn start_android(&mut self, port: u16) -> Result<UiGamepadState, String> {
        if self.android_listener.is_some() {
            return Err("Android controller input is already listening".to_owned());
        }
        if self.dsu_worker.is_some() {
            return Err("stop DSU before changing the controller input stream".to_owned());
        }
        if port == 0 {
            return Err("Android controller UDP port must be 1..=65535".to_owned());
        }
        let token = generate_pairing_token();
        let mut listener = AndroidGamepadListener::start(port, token)?;
        if let Err(error) = listener.set_paired_projection(Some(self.windows_host.input_sender())) {
            let _ = listener.stop();
            return Err(error);
        }
        self.android_endpoint = Some(listener.local_address().to_string());
        self.android_status = "listening";
        self.android_listener = Some(listener);
        Ok(self.snapshot())
    }

    pub(crate) fn stop_android(&mut self) -> Result<UiGamepadState, String> {
        if self.dsu_worker.is_some() {
            return Err("stop DSU before stopping Android controller input".to_owned());
        }
        let Some(mut listener) = self.android_listener.take() else {
            self.android_status = "stopped";
            return Ok(self.snapshot());
        };
        self.last_android_snapshot = listener.stop()?;
        self.android_status = "stopped";
        Ok(self.snapshot())
    }

    pub(crate) fn start_windows_projection(
        &mut self,
        runtime: &mut NodeRuntime,
        enable_xinput_companion: bool,
    ) -> Result<UiGamepadState, String> {
        if self.windows_host.is_active() {
            return Err("Windows DS4 projection is already active".to_owned());
        }
        if self.dsu_worker.is_some() {
            return Err("stop DSU before starting the Windows DS4 projection".to_owned());
        }
        let listener = self.android_listener.as_ref().ok_or_else(|| {
            "start Android controller input before starting the Windows DS4 projection".to_owned()
        })?;
        let (motion, controls) = listener.anchors()?;
        self.windows_host.start(
            runtime,
            controls,
            motion,
            self.next_timestamp() / 1_000_000,
            enable_xinput_companion,
        )?;
        Ok(self.snapshot())
    }

    pub(crate) fn stop_windows_projection(
        &mut self,
        runtime: &mut NodeRuntime,
    ) -> Result<UiGamepadState, String> {
        self.windows_host.stop(runtime)?;
        Ok(self.snapshot())
    }

    pub(crate) fn poll_windows_projection(&mut self, runtime: &mut NodeRuntime) {
        self.windows_host.poll(runtime);
    }

    /// The debug-only direct projection Gates consume accepted snapshots
    /// themselves and must not leave the production host ingress queue armed.
    fn detach_android_host_projection_for_direct_gate(&self) -> Result<(), String> {
        self.android_listener
            .as_ref()
            .ok_or_else(|| "Android controller input is not listening".to_owned())?
            .set_paired_projection(None)
    }

    pub(crate) fn start_dsu(&mut self, port: u16) -> Result<UiGamepadState, String> {
        self.start_dsu_mode(port, UiDsuMode::MotionAndControls)
    }

    pub(crate) fn start_dsu_mode(
        &mut self,
        port: u16,
        mode: UiDsuMode,
    ) -> Result<UiGamepadState, String> {
        if self.dsu_worker.is_some() {
            return Err("DSU gamepad projection is already running".to_owned());
        }
        let android_source = self.android_listener.is_some();
        let (motion, anchor) = if let Some(listener) = self.android_listener.as_ref() {
            listener.anchors()?
        } else {
            let reset_timestamp = self.next_timestamp();
            let reset = self
                .composer
                .apply(GamepadControlUpdate::Reset, reset_timestamp)
                .map_err(|error| error.to_string())?;
            self.last_sequence = Some(reset.header.sequence);
            self.last_timestamp_nanos = Some(reset.header.source_timestamp_nanos);
            self.last_update = "reset.before_dsu_start";
            let anchor = self
                .composer
                .anchor(self.next_timestamp())
                .map_err(|error| error.to_string())?;
            (stationary_motion_envelope(), anchor)
        };
        let config = DsuImuWorkerConfig::new(DsuLoopbackConfig::local_lab(port, DSU_SERVER_ID));
        let mut worker = match mode {
            UiDsuMode::MotionOnly => DsuImuWorker::start(config, &motion),
            UiDsuMode::MotionAndControls => {
                DsuImuWorker::start_with_controls(config, &motion, &anchor)
            }
        }
        .map_err(|error| error.to_string())?;
        let endpoint = worker.local_address().to_string();
        if let Some(listener) = self.android_listener.as_ref() {
            if let Err(error) =
                listener.set_projection(Some(worker.sender()), worker.controls_sender())
            {
                let _ = worker.stop();
                return Err(error);
            }
        } else if worker.sender().try_submit(motion) != DsuSubmitOutcome::Submitted {
            let _ = worker.stop();
            return Err("DSU gamepad projection rejected its stationary motion anchor".to_owned());
        }
        self.dsu_endpoint = Some(endpoint);
        self.dsu_mode = mode;
        self.dsu_status = "active";
        self.last_dsu_submit = if android_source {
            "waiting_for_android_packet"
        } else {
            "stationary_motion_seeded"
        };
        self.dsu_worker = Some(worker);
        Ok(self.snapshot())
    }

    pub(crate) fn stop_dsu(&mut self) -> Result<UiGamepadState, String> {
        let Some(mut worker) = self.dsu_worker.take() else {
            self.dsu_status = "stopped";
            return Ok(self.snapshot());
        };
        if let Some(listener) = self.android_listener.as_ref() {
            listener.set_projection(None, None)?;
            listener.set_paired_projection(Some(self.windows_host.input_sender()))?;
        }
        self.last_dsu_submit = match worker.controls_sender() {
            Some(sender) => match sender.request_neutral() {
                DsuNeutralOutcome::Requested => "neutral_requested",
                DsuNeutralOutcome::Stopped => "worker_stopped",
            },
            None => "motion_only_stopped",
        };
        worker.stop().map_err(|error| error.to_string())?;
        self.last_dsu_stats = worker.stats();
        self.dsu_status = "stopped";
        Ok(self.snapshot())
    }

    fn next_timestamp(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_nanos())
            .unwrap_or(u64::MAX)
            .saturating_add(1)
    }

    #[allow(clippy::too_many_arguments)]
    fn to_ui(
        &self,
        controls: GamepadControls,
        source: &'static str,
        simulated: bool,
        stream_epoch: u64,
        sequence: Option<u64>,
        source_timestamp_nanos: Option<u64>,
        last_update: &'static str,
        android: &AndroidReceiverSnapshot,
        motion: Option<&DataEnvelope<ImuSampleV1>>,
    ) -> UiGamepadState {
        let pressed_buttons = UiGamepadButton::ALL
            .into_iter()
            .filter(|button| controls.buttons.contains(button.semantic()))
            .map(UiGamepadButton::label)
            .collect();
        let stats = self
            .dsu_worker
            .as_ref()
            .map(DsuImuWorker::stats)
            .unwrap_or(self.last_dsu_stats);
        UiGamepadState {
            schema_version: 1,
            source,
            simulated,
            profile: GAMEPAD_PROFILE,
            stream_epoch,
            sequence,
            source_timestamp_nanos,
            pressed_buttons,
            dpad: UiDpadState {
                x: controls.dpad.x,
                y: controls.dpad.y,
            },
            left_stick: UiStickState {
                x: controls.left_stick.x.get(),
                y: controls.left_stick.y.get(),
            },
            right_stick: UiStickState {
                x: controls.right_stick.x.get(),
                y: controls.right_stick.y.get(),
            },
            left_trigger: controls.left_trigger.get(),
            right_trigger: controls.right_trigger.get(),
            last_update,
            dsu_projection: UiDsuProjection {
                supported: true,
                status: if self
                    .dsu_worker
                    .as_ref()
                    .is_some_and(|worker| worker.stats().stopped)
                {
                    "failed"
                } else {
                    self.dsu_status
                },
                endpoint: self.dsu_endpoint.clone(),
                mode: self.dsu_mode,
                last_submit: self.last_dsu_submit,
                controls_submitted: stats.controls_submitted,
                controls_accepted: stats.controls_accepted,
                controls_queue_full: stats.controls_queue_full,
                controls_neutral_resets: stats.controls_neutral_resets,
                active_subscribers: stats.active_subscribers,
                pad_packets_sent: stats.dsu_pad_packets_sent,
                packet_send_errors: stats.dsu_pad_packet_send_errors,
            },
            windows_projection: self.windows_host.snapshot(),
            android_input: UiAndroidInput {
                supported: true,
                status: if self.android_listener.is_some() {
                    if android.peer_connected {
                        "connected"
                    } else {
                        "listening"
                    }
                } else {
                    self.android_status
                },
                endpoint: self.android_endpoint.clone(),
                lan_host_hint: lan_ipv4_hint(),
                pairing_token: self
                    .android_listener
                    .as_ref()
                    .map(|listener| listener.token().to_owned()),
                peer_connected: android.peer_connected,
                accepted_packets: android.accepted_packets,
                rejected_packets: android.rejected_packets,
                replayed_packets: android.replayed_packets,
                peer_timeouts: android.peer_timeouts,
                projection_queue_full: android.projection_queue_full,
                packet_age_millis: android.packet_age_millis,
                remote_sequence: android.latest.as_ref().map(|latest| latest.remote_sequence),
                last_event: android.last_event,
            },
            motion: UiGamepadMotion {
                source: if motion.is_some() {
                    "android_sensors"
                } else {
                    "stationary_fixture"
                },
                source_timestamp_nanos: motion.map(|sample| sample.source_timestamp_nanos),
                acceleration: motion
                    .map(|sample| sample.payload.acceleration)
                    .unwrap_or([0.0, 0.0, 9.806_65]),
                angular_velocity: motion
                    .map(|sample| sample.payload.angular_velocity)
                    .unwrap_or([0.0; 3]),
            },
        }
    }
}

fn generate_pairing_token() -> String {
    StreamId::new()
        .to_string()
        .chars()
        .filter(char::is_ascii_hexdigit)
        .take(12)
        .collect()
}

fn stationary_motion_envelope() -> DataEnvelope<ImuSampleV1> {
    DataEnvelope {
        profile: ImuSampleV1::profile(),
        stream_id: StreamId::new(),
        stream_epoch: 1,
        sequence: 0,
        source_timestamp_nanos: 1,
        receive_timestamp_nanos: 1,
        clock_domain_id: "capyio.desktop.gamepad_simulator".to_owned(),
        payload: ImuSampleV1 {
            acceleration: [0.0, 0.0, 9.806_65],
            angular_velocity: [0.0, 0.0, 0.0],
            magnetic_field: None,
            units: ImuUnitsV1::default(),
            coordinate_frame: ImuCoordinateFrame::AndroidDeviceXRightYUpZOut,
            accuracy: ImuAccuracy::High,
            calibration: ImuCalibration::RuntimeCalibrated,
            sensor: ImuSensorMetadataV1 {
                sensor_name: "CapyIO stationary gamepad fixture".to_owned(),
                vendor: "CapyIO".to_owned(),
                version: 1,
                android_sensor_type: None,
            },
            component_timestamps: None,
        },
    }
}

#[cfg(debug_assertions)]
const DSU_PAD_DATA_MESSAGE: u32 = 0x10_0002;
#[cfg(debug_assertions)]
const DSU_PAD_DATA_BYTES: usize = 100;

#[cfg(debug_assertions)]
fn gate_write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

#[cfg(debug_assertions)]
fn gate_write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

#[cfg(debug_assertions)]
fn gate_read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("u32 slice"))
}

#[cfg(debug_assertions)]
fn gate_read_f32(bytes: &[u8], offset: usize) -> f32 {
    f32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("f32 slice"))
}

#[cfg(debug_assertions)]
fn gate_dsu_pad_request(client_id: u32) -> [u8; 28] {
    let mut packet = [0_u8; 28];
    packet[..4].copy_from_slice(b"DSUC");
    gate_write_u16(&mut packet, 4, DSU_PROTOCOL_VERSION);
    gate_write_u16(&mut packet, 6, 12);
    gate_write_u32(&mut packet, 12, client_id);
    gate_write_u32(&mut packet, 16, DSU_PAD_DATA_MESSAGE);
    packet[20] = 1;
    packet[21] = 0;
    let checksum = crc32_ieee(&packet);
    gate_write_u32(&mut packet, 8, checksum);
    packet
}

#[cfg(debug_assertions)]
fn gate_receive_dsu_pad(socket: &UdpSocket, target: SocketAddr) -> io::Result<[u8; 100]> {
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut packet = [0_u8; DSU_PAD_DATA_BYTES];
    loop {
        match socket.recv_from(&mut packet) {
            Ok((bytes, source))
                if bytes == DSU_PAD_DATA_BYTES
                    && source == target
                    && &packet[..4] == b"DSUS"
                    && gate_read_u32(&packet, 16) == DSU_PAD_DATA_MESSAGE =>
            {
                return Ok(packet);
            }
            Ok(_) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::ConnectionReset
                ) => {}
            Err(error) => return Err(error),
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "timed out waiting for a DSU pad-data packet",
            ));
        }
    }
}

#[cfg(debug_assertions)]
fn gate_dsu_packet_has_non_neutral_controls(packet: &[u8; 100]) -> bool {
    packet[36..=38].iter().any(|value| *value != 0)
        || packet[40..=43].iter().any(|value| *value != 128)
        || packet[54] != 0
        || packet[55] != 0
}

#[cfg(debug_assertions)]
fn gate_dsu_packet_has_finite_imu(packet: &[u8; 100]) -> bool {
    let acceleration = [
        gate_read_f32(packet, 76),
        gate_read_f32(packet, 80),
        gate_read_f32(packet, 84),
    ];
    let angular_velocity = [
        gate_read_f32(packet, 88),
        gate_read_f32(packet, 92),
        gate_read_f32(packet, 96),
    ];
    acceleration
        .into_iter()
        .chain(angular_velocity)
        .all(f32::is_finite)
        && acceleration.into_iter().any(|value| value.abs() > 0.01)
}

/// Runs the explicitly operator-assisted, debug-only physical gamepad gate.
///
/// The caller must configure the printed Android endpoint/token, then exercise
/// at least one touch control. The gate deliberately replaces a DSU subscriber
/// mid-stream to cover Windows UDP connection-reset recovery.
#[cfg(debug_assertions)]
pub(crate) fn run_physical_gamepad_gate(android_port: u16, dsu_port: u16) -> Result<(), String> {
    let mut lab = GamepadLab::new()?;
    let listening = lab.start_android(android_port)?;
    let token = listening
        .android_input
        .pairing_token
        .ok_or_else(|| "physical gate listener did not issue a pairing token".to_owned())?;
    let dsu = lab.start_dsu(dsu_port)?;
    let target: SocketAddr = dsu
        .dsu_projection
        .endpoint
        .ok_or_else(|| "physical gate DSU endpoint is missing".to_owned())?
        .parse()
        .map_err(|error| format!("invalid physical gate DSU endpoint: {error}"))?;

    println!("CAPYIO_PHYSICAL_ANDROID_PORT={android_port}");
    println!("CAPYIO_PHYSICAL_DSU_ENDPOINT={target}");
    println!("CAPYIO_PHYSICAL_PAIRING_TOKEN={token}");
    println!("CAPYIO_PHYSICAL_GAMEPAD_READY");
    io::stdout()
        .flush()
        .map_err(|error| format!("could not flush physical gate instructions: {error}"))?;

    let first_subscriber =
        UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|error| error.to_string())?;
    first_subscriber
        .set_read_timeout(Some(Duration::from_millis(100)))
        .map_err(|error| error.to_string())?;
    let first_deadline = Instant::now() + Duration::from_secs(30);
    let mut first_registration_due = Instant::now();
    let mut first_packet = None;
    while Instant::now() < first_deadline {
        if Instant::now() >= first_registration_due {
            first_subscriber
                .send_to(&gate_dsu_pad_request(0x4341_5001), target)
                .map_err(|error| error.to_string())?;
            first_registration_due = Instant::now() + Duration::from_secs(2);
        }
        if let Ok(packet) = gate_receive_dsu_pad(&first_subscriber, target) {
            first_packet = Some(packet);
            break;
        }
    }
    let Some(first_packet) = first_packet else {
        let snapshot = lab.snapshot();
        let stats = lab
            .dsu_worker
            .as_ref()
            .map(DsuImuWorker::stats)
            .unwrap_or_default();
        let _ = lab.stop_dsu();
        let _ = lab.stop_android();
        return Err(format!(
            "first DSU subscriber received no packet; Android accepted={} rejected={} last_event={} DSU datagrams={} malformed={} subscriptions={} samples={}/{} controls={}/{} packets={}",
            snapshot.android_input.accepted_packets,
            snapshot.android_input.rejected_packets,
            snapshot.android_input.last_event,
            stats.dsu_datagrams_received,
            stats.malformed_dsu_datagrams,
            stats.active_subscribers,
            stats.samples_accepted,
            stats.samples_submitted,
            stats.controls_accepted,
            stats.controls_submitted,
            stats.dsu_pad_packets_sent,
        ));
    };
    let first_packet_number = gate_read_u32(&first_packet, 32);
    drop(first_subscriber);

    std::thread::sleep(Duration::from_millis(150));

    let second_subscriber =
        UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|error| error.to_string())?;
    second_subscriber
        .set_read_timeout(Some(Duration::from_millis(100)))
        .map_err(|error| error.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(45);
    let mut second_registration_due = Instant::now();
    let mut received = 0_u32;
    let mut last_packet_number = first_packet_number;
    let mut saw_non_neutral_controls = false;
    let mut saw_finite_imu = false;
    while Instant::now() < deadline {
        if Instant::now() >= second_registration_due {
            second_subscriber
                .send_to(&gate_dsu_pad_request(0x4341_5002), target)
                .map_err(|error| error.to_string())?;
            second_registration_due = Instant::now() + Duration::from_secs(2);
        }
        if let Ok(packet) = gate_receive_dsu_pad(&second_subscriber, target) {
            received += 1;
            last_packet_number = gate_read_u32(&packet, 32);
            saw_non_neutral_controls |= gate_dsu_packet_has_non_neutral_controls(&packet);
            saw_finite_imu |= gate_dsu_packet_has_finite_imu(&packet);
        }

        let snapshot = lab.snapshot();
        if snapshot.android_input.accepted_packets > 0
            && snapshot.source == "android_touch"
            && saw_non_neutral_controls
            && saw_finite_imu
        {
            break;
        }
    }

    let snapshot = lab.snapshot();
    println!(
        "CAPYIO_PHYSICAL_RESULT accepted={} rejected={} replayed={} timeouts={} dsu_packets={} packet_numbers={}..={} non_neutral={} finite_imu={} source={} last_event={}",
        snapshot.android_input.accepted_packets,
        snapshot.android_input.rejected_packets,
        snapshot.android_input.replayed_packets,
        snapshot.android_input.peer_timeouts,
        received,
        first_packet_number,
        last_packet_number,
        saw_non_neutral_controls,
        saw_finite_imu,
        snapshot.source,
        snapshot.android_input.last_event,
    );

    lab.stop_dsu()?;
    lab.stop_android()?;

    if snapshot.android_input.accepted_packets == 0 {
        return Err("no valid Android gamepad packet was accepted".to_owned());
    }
    if snapshot.source != "android_touch" {
        return Err(format!(
            "expected android_touch source, observed {}",
            snapshot.source
        ));
    }
    if received == 0 {
        return Err("reconnected DSU subscriber received no data".to_owned());
    }
    if !saw_finite_imu {
        return Err("DSU subscriber observed no finite IMU data".to_owned());
    }
    if !saw_non_neutral_controls {
        return Err("DSU subscriber observed no pressed button, moved stick or trigger".to_owned());
    }
    Ok(())
}

/// Runs the Runtime-owned Android -> DS4 -> exact USB/IP attachment Gate.
/// The fixed Windows host configuration performs no driver installation or
/// global detach; cleanup is limited to the port owned by this controller.
#[cfg(debug_assertions)]
pub(crate) fn run_windows_ds4_runtime_gate(
    android_port: u16,
    hold_seconds: u64,
) -> Result<(), String> {
    run_windows_ds4_runtime_gate_with_mode(android_port, hold_seconds, true)
}

/// Runs the Runtime-owned Android -> DS4 Gate without publishing the optional
/// direct-XInput compatibility device. This is the default product topology
/// used by WGI, RawGameController, Steam and browser consumers.
#[cfg(debug_assertions)]
pub(crate) fn run_windows_ds4_only_runtime_gate(
    android_port: u16,
    hold_seconds: u64,
) -> Result<(), String> {
    run_windows_ds4_runtime_gate_with_mode(android_port, hold_seconds, false)
}

#[cfg(debug_assertions)]
fn run_windows_ds4_runtime_gate_with_mode(
    android_port: u16,
    hold_seconds: u64,
    enable_xinput_companion: bool,
) -> Result<(), String> {
    if !(5..=300).contains(&hold_seconds) {
        return Err("Windows DS4 Runtime Gate hold must be 5..=300 seconds".to_owned());
    }
    let mut runtime_lab = DemoLab::new().map_err(|error| error.to_string())?;
    let host = WindowsGamepadHost::install(&mut runtime_lab.runtime, runtime_lab.session_id)?;
    let mut lab = GamepadLab::new_with_windows_host(host)?;
    let listening = lab.start_android(android_port)?;
    let token = listening
        .android_input
        .pairing_token
        .ok_or_else(|| "Windows DS4 Runtime Gate did not issue a pairing token".to_owned())?;
    println!(
        "CAPYIO_PHYSICAL_ANDROID_ENDPOINT={}",
        listening
            .android_input
            .endpoint
            .as_deref()
            .unwrap_or("unavailable")
    );
    println!(
        "CAPYIO_PHYSICAL_LAN_HOST_HINT={}",
        listening
            .android_input
            .lan_host_hint
            .as_deref()
            .unwrap_or("unavailable")
    );
    println!("CAPYIO_PHYSICAL_PAIRING_TOKEN={token}");

    println!(
        "CAPYIO_WINDOWS_GAMEPAD_MODE={}",
        if enable_xinput_companion {
            "ds4+xinput-companion"
        } else {
            "ds4-only"
        }
    );
    lab.start_windows_projection(&mut runtime_lab.runtime, enable_xinput_companion)?;
    let deadline = Instant::now() + Duration::from_secs(hold_seconds);
    while Instant::now() < deadline {
        lab.poll_windows_projection(&mut runtime_lab.runtime);
        thread::sleep(Duration::from_millis(4));
    }
    let projection = lab.snapshot().windows_projection;
    let stop = lab
        .stop_windows_projection(&mut runtime_lab.runtime)
        .map(|_| ());
    let android_stop = lab.stop_android().map(|_| ());
    stop?;
    android_stop?;
    if projection.status != "active" {
        return Err(format!(
            "Windows DS4 projection ended with status {} ({})",
            projection.status, projection.last_event
        ));
    }
    if projection.input_packets == 0 || projection.last_remote_sequence.is_none() {
        return Err("Windows DS4 projection accepted no physical Android packets".to_owned());
    }
    if projection.non_neutral_packets == 0 {
        return Err("Windows DS4 projection observed no non-neutral phone control".to_owned());
    }
    if enable_xinput_companion && (!projection.xinput_ready || projection.xinput_packets == 0) {
        return Err("Windows Xbox compatibility companion accepted no phone controls".to_owned());
    }
    if !enable_xinput_companion && (projection.xinput_ready || projection.xinput_packets != 0) {
        return Err("DS4-only projection unexpectedly activated the XInput companion".to_owned());
    }
    println!(
        "CAPYIO_WINDOWS_DS4_INPUT_PACKETS={}",
        projection.input_packets
    );
    println!(
        "CAPYIO_WINDOWS_DS4_NON_NEUTRAL_PACKETS={}",
        projection.non_neutral_packets
    );
    println!(
        "CAPYIO_WINDOWS_XINPUT_PACKETS={}",
        projection.xinput_packets
    );
    println!(
        "CAPYIO_WINDOWS_DS4_REMOTE_SEQUENCE={}",
        projection.last_remote_sequence.unwrap_or_default()
    );
    println!(
        "CAPYIO_WINDOWS_DS4_OWNED_PORT={}",
        projection
            .owned_usbip_port
            .map_or_else(|| "none".to_owned(), |port| port.to_string())
    );
    Ok(())
}

/// Runs the debug-only physical Android -> DSU + VIIPER Xbox 360 gate.
///
/// The external VIIPER v0.7.0 process must already be bound to the explicit
/// loopback management port with both automatic attachment modes disabled.
/// This gate owns exactly one VIIPER bus, begins with neutral state, forwards
/// complete accepted Android states and removes the bus after a final neutral.
#[cfg(debug_assertions)]
pub(crate) fn run_physical_viiper_gamepad_gate(
    android_port: u16,
    dsu_port: u16,
    viiper_port: u16,
    hold_seconds: u64,
) -> Result<(), String> {
    if android_port == 0 || dsu_port == 0 || viiper_port == 0 {
        return Err("Android, DSU and VIIPER ports must be non-zero".to_owned());
    }
    if !(5..=300).contains(&hold_seconds) {
        return Err("VIIPER physical gate hold time must be within 5..=300 seconds".to_owned());
    }

    let mut lab = GamepadLab::new()?;
    let listening = lab.start_android(android_port)?;
    let token = listening
        .android_input
        .pairing_token
        .ok_or_else(|| "VIIPER physical gate listener did not issue a pairing token".to_owned())?;
    let dsu = lab.start_dsu(dsu_port)?;
    let dsu_endpoint = dsu
        .dsu_projection
        .endpoint
        .ok_or_else(|| "VIIPER physical gate DSU endpoint is missing".to_owned())?;
    let (_, anchor) = lab
        .android_listener
        .as_ref()
        .ok_or_else(|| "VIIPER physical gate Android listener is missing".to_owned())?
        .anchors()?;
    let viiper_address = SocketAddr::from((Ipv4Addr::LOCALHOST, viiper_port));
    let viiper_config = ViiperLoopbackConfig::new(
        viiper_address,
        Duration::from_secs(2),
        Duration::from_secs(2),
        MAX_VIIPER_MANAGEMENT_RESPONSE_BYTES,
    )
    .map_err(|error| error.to_string())?;
    let viiper = ViiperLoopbackClient::new(viiper_config);
    let probe = viiper.probe().map_err(|error| error.to_string())?;
    let mut worker = match viiper.open_xbox360(
        ViiperAutoAttachDisabled::confirmed_by_caller(),
        anchor,
        ViiperXbox360Mapping::preserve(),
    ) {
        Ok(worker) => worker,
        Err(error) => {
            let _ = lab.stop_dsu();
            let _ = lab.stop_android();
            return Err(error.to_string());
        }
    };

    println!("CAPYIO_PHYSICAL_ANDROID_PORT={android_port}");
    println!("CAPYIO_PHYSICAL_DSU_ENDPOINT={dsu_endpoint}");
    println!("CAPYIO_PHYSICAL_PAIRING_TOKEN={token}");
    println!(
        "CAPYIO_PHYSICAL_VIIPER_SERVER={}:{}",
        probe.server(),
        probe.version()
    );
    println!("CAPYIO_PHYSICAL_VIIPER_API={viiper_address}");
    println!("CAPYIO_PHYSICAL_VIIPER_BUS_ID={}", worker.bus_id());
    println!("CAPYIO_PHYSICAL_VIIPER_DEVICE_ID={}", worker.device_id());
    println!("CAPYIO_PHYSICAL_VIIPER_GAMEPAD_READY");
    io::stdout()
        .flush()
        .map_err(|error| format!("could not flush VIIPER physical gate instructions: {error}"))?;

    let deadline = Instant::now() + Duration::from_secs(hold_seconds);
    let mut last_submitted_sequence = None;
    let mut states_submitted = 0_u64;
    let mut saw_non_neutral_controls = false;
    let mut saw_finite_imu = false;
    let mut primary_error = None;
    while Instant::now() < deadline {
        let snapshot = lab
            .android_listener
            .as_ref()
            .map(AndroidGamepadListener::snapshot)
            .unwrap_or_default();
        if let Some(latest) = snapshot.latest.as_ref() {
            let sequence = latest.controls.header.sequence;
            if last_submitted_sequence != Some(sequence) {
                saw_non_neutral_controls |= latest.controls.controls != GamepadControls::neutral();
                saw_finite_imu |= latest
                    .motion
                    .payload
                    .acceleration
                    .into_iter()
                    .chain(latest.motion.payload.angular_velocity)
                    .all(f64::is_finite);
                match worker.submit(latest.controls) {
                    Ok(_) => {
                        last_submitted_sequence = Some(sequence);
                        states_submitted = states_submitted.saturating_add(1);
                    }
                    Err(error) => {
                        primary_error = Some(format!("VIIPER state submission failed: {error}"));
                        break;
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_millis(4));
    }

    let snapshot = lab.snapshot();
    println!(
        "CAPYIO_PHYSICAL_VIIPER_RESULT accepted={} rejected={} replayed={} timeouts={} viiper_states={} non_neutral={} finite_imu={} source={} last_event={}",
        snapshot.android_input.accepted_packets,
        snapshot.android_input.rejected_packets,
        snapshot.android_input.replayed_packets,
        snapshot.android_input.peer_timeouts,
        states_submitted,
        saw_non_neutral_controls,
        saw_finite_imu,
        snapshot.source,
        snapshot.android_input.last_event,
    );

    let worker_cleanup = worker.stop().err().map(|error| error.to_string());
    let dsu_cleanup = lab.stop_dsu().err();
    let android_cleanup = lab.stop_android().err();
    if let Some(error) = primary_error
        .or_else(|| worker_cleanup.map(|error| format!("VIIPER cleanup failed: {error}")))
        .or_else(|| dsu_cleanup.map(|error| format!("DSU cleanup failed: {error}")))
        .or_else(|| android_cleanup.map(|error| format!("Android cleanup failed: {error}")))
    {
        return Err(error);
    }
    if snapshot.android_input.accepted_packets == 0 || states_submitted == 0 {
        return Err("no valid Android gamepad state reached VIIPER".to_owned());
    }
    if !saw_finite_imu {
        return Err("the paired Android stream contained no finite IMU data".to_owned());
    }
    if !saw_non_neutral_controls {
        return Err("VIIPER received no pressed button, moved stick or trigger".to_owned());
    }
    Ok(())
}

/// Runs the debug-only physical Android touch+IMU -> native DS4 VIIPER gate.
///
/// The Gate creates exactly one `dualshock4` export and keeps it alive for the
/// bounded hold interval. A separately authorized usbip-win2 `--once` attach
/// may target the printed bus/device identity while this process is running.
#[cfg(debug_assertions)]
pub(crate) fn run_physical_ds4_gamepad_gate(
    android_port: u16,
    viiper_port: u16,
    hold_seconds: u64,
) -> Result<(), String> {
    if android_port == 0 || viiper_port == 0 {
        return Err("Android and VIIPER ports must be non-zero".to_owned());
    }
    if !(5..=300).contains(&hold_seconds) {
        return Err("DS4 physical gate hold time must be within 5..=300 seconds".to_owned());
    }

    let mut lab = GamepadLab::new()?;
    let listening = lab.start_android(android_port)?;
    let token = listening
        .android_input
        .pairing_token
        .ok_or_else(|| "DS4 physical gate listener did not issue a pairing token".to_owned())?;
    let (motion_anchor, controls_anchor) = lab
        .android_listener
        .as_ref()
        .ok_or_else(|| "DS4 physical gate Android listener is missing".to_owned())?
        .anchors()?;
    lab.detach_android_host_projection_for_direct_gate()?;
    let viiper_address = SocketAddr::from((Ipv4Addr::LOCALHOST, viiper_port));
    let viiper_config = ViiperLoopbackConfig::new(
        viiper_address,
        Duration::from_secs(2),
        Duration::from_secs(2),
        MAX_VIIPER_MANAGEMENT_RESPONSE_BYTES,
    )
    .map_err(|error| error.to_string())?;
    let viiper = ViiperLoopbackClient::new(viiper_config);
    let probe = viiper.probe().map_err(|error| error.to_string())?;
    let mut worker = match viiper.open_dualshock4(
        ViiperAutoAttachDisabled::confirmed_by_caller(),
        controls_anchor,
        &motion_anchor,
        ViiperDs4ControlsMapping::gamepad_y_up(),
        ViiperDs4MotionMapping::identity(),
    ) {
        Ok(worker) => worker,
        Err(error) => {
            let _ = lab.stop_android();
            return Err(error.to_string());
        }
    };

    println!("CAPYIO_PHYSICAL_ANDROID_PORT={android_port}");
    println!("CAPYIO_PHYSICAL_PAIRING_TOKEN={token}");
    println!(
        "CAPYIO_PHYSICAL_VIIPER_SERVER={}:{}",
        probe.server(),
        probe.version()
    );
    println!("CAPYIO_PHYSICAL_DS4_VIIPER_API={viiper_address}");
    println!("CAPYIO_PHYSICAL_DS4_BUS_ID={}", worker.bus_id());
    println!("CAPYIO_PHYSICAL_DS4_DEVICE_ID={}", worker.device_id());
    println!(
        "CAPYIO_PHYSICAL_DS4_USBIP_BUS={}-{}",
        worker.bus_id(),
        worker.device_id()
    );
    println!("CAPYIO_PHYSICAL_DS4_READY");
    io::stdout()
        .flush()
        .map_err(|error| format!("could not flush DS4 physical gate instructions: {error}"))?;

    let deadline = Instant::now() + Duration::from_secs(hold_seconds);
    let mut last_pair = None;
    let mut states_submitted = 0_u64;
    let mut saw_non_neutral_controls = false;
    let mut saw_finite_imu = false;
    let mut primary_error = None;
    let mut continuity = DirectGateContinuity::default();
    while Instant::now() < deadline {
        let snapshot = lab
            .android_listener
            .as_ref()
            .map(AndroidGamepadListener::snapshot)
            .unwrap_or_default();
        if continuity.observe(&snapshot)
            && let Err(error) = worker.request_safe_state()
        {
            primary_error = Some(format!(
                "VIIPER DS4 neutral request after Android timeout failed: {error}"
            ));
            break;
        }
        if snapshot.peer_connected
            && let Some(latest) = snapshot.latest.as_ref()
        {
            let pair = (latest.controls.header.sequence, latest.motion.sequence);
            if last_pair != Some(pair) {
                saw_non_neutral_controls |= latest.controls.controls != GamepadControls::neutral();
                saw_finite_imu |= latest
                    .motion
                    .payload
                    .acceleration
                    .into_iter()
                    .chain(latest.motion.payload.angular_velocity)
                    .all(f64::is_finite);
                match worker.submit(latest.controls, &latest.motion) {
                    Ok(_) => {
                        last_pair = Some(pair);
                        states_submitted = states_submitted.saturating_add(1);
                    }
                    Err(error) => {
                        primary_error =
                            Some(format!("VIIPER DS4 state submission failed: {error}"));
                        break;
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_millis(4));
    }

    let snapshot = lab.snapshot();
    println!(
        "CAPYIO_PHYSICAL_DS4_RESULT accepted={} rejected={} replayed={} timeouts={} recovered_timeouts={} ds4_states={} non_neutral={} finite_imu={} source={} last_event={}",
        snapshot.android_input.accepted_packets,
        snapshot.android_input.rejected_packets,
        snapshot.android_input.replayed_packets,
        snapshot.android_input.peer_timeouts,
        continuity.recovered_timeouts,
        states_submitted,
        saw_non_neutral_controls,
        saw_finite_imu,
        snapshot.source,
        snapshot.android_input.last_event,
    );

    let worker_cleanup = worker.stop().err().map(|error| error.to_string());
    let android_cleanup = lab.stop_android().err();
    if let Some(error) = primary_error
        .or_else(|| worker_cleanup.map(|error| format!("VIIPER DS4 cleanup failed: {error}")))
        .or_else(|| android_cleanup.map(|error| format!("Android cleanup failed: {error}")))
    {
        return Err(error);
    }
    if snapshot.android_input.accepted_packets == 0 || states_submitted == 0 {
        return Err("no valid Android controls+IMU state reached the DS4 session".to_owned());
    }
    if !continuity.latest_timeout_recovered() {
        return Err(
            "Android DS4 source did not recover after its latest neutralizing timeout".to_owned(),
        );
    }
    if !saw_finite_imu {
        return Err("the DS4 session received no finite Android IMU data".to_owned());
    }
    if !saw_non_neutral_controls {
        return Err(
            "the DS4 session received no pressed button, moved stick or trigger".to_owned(),
        );
    }
    Ok(())
}

#[derive(Default)]
struct DirectGateContinuity {
    observed_timeouts: u64,
    accepted_at_latest_timeout: u64,
    recovered_timeouts: u64,
}

impl DirectGateContinuity {
    fn observe(&mut self, snapshot: &AndroidReceiverSnapshot) -> bool {
        let new_timeout = snapshot.peer_timeouts > self.observed_timeouts;
        if new_timeout {
            self.observed_timeouts = snapshot.peer_timeouts;
            self.accepted_at_latest_timeout = snapshot.accepted_packets;
        } else if self.recovered_timeouts < self.observed_timeouts
            && snapshot.peer_connected
            && snapshot.accepted_packets > self.accepted_at_latest_timeout
        {
            self.recovered_timeouts = self.observed_timeouts;
        }
        new_timeout
    }

    fn latest_timeout_recovered(&self) -> bool {
        self.recovered_timeouts == self.observed_timeouts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, UdpSocket};
    use std::sync::Mutex;

    static ANDROID_PORT_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn simulator_composes_complete_snapshots_and_resets_neutral() {
        let mut lab = GamepadLab::new().expect("gamepad lab");
        let neutral = lab.snapshot();
        assert!(neutral.sequence.is_none());
        assert!(neutral.pressed_buttons.is_empty());
        assert_eq!(neutral.windows_projection.status, "host_gate_required");
        assert_eq!(
            neutral.windows_projection.device_identity,
            "DualShock 4 · 054c:09cc · native motion"
        );
        assert!(neutral.windows_projection.bus_id.is_none());
        assert!(neutral.windows_projection.owned_usbip_port.is_none());

        let south = lab
            .apply(UpdateGamepadRequest::Button {
                button: UiGamepadButton::South,
                pressed: true,
            })
            .expect("press south");
        assert_eq!(south.sequence, Some(0));
        assert_eq!(south.pressed_buttons, vec!["south"]);

        let moved = lab
            .apply(UpdateGamepadRequest::Stick {
                stick: UiGamepadStick::Left,
                x: 12_345,
                y: -23_456,
            })
            .expect("move stick");
        assert_eq!(moved.sequence, Some(1));
        assert_eq!(moved.left_stick.x, 12_345);
        assert_eq!(moved.left_stick.y, -23_456);
        assert_eq!(moved.pressed_buttons, vec!["south"]);

        let reset = lab
            .apply(UpdateGamepadRequest::Reset)
            .expect("reset controls");
        assert_eq!(reset.sequence, Some(2));
        assert!(reset.pressed_buttons.is_empty());
        assert_eq!(reset.left_stick.x, 0);
        assert_eq!(reset.left_stick.y, 0);
        assert_eq!(reset.left_trigger, 0);
    }

    #[test]
    fn simulator_can_feed_the_bounded_loopback_dsu_worker() {
        let mut lab = GamepadLab::new().expect("gamepad lab");
        let started = lab.start_dsu(0).expect("start ephemeral DSU lab");
        assert_eq!(started.dsu_projection.status, "active");
        assert!(started.dsu_projection.endpoint.is_some());

        lab.apply(UpdateGamepadRequest::Button {
            button: UiGamepadButton::South,
            pressed: true,
        })
        .expect("submit controls");
        let deadline = Instant::now() + std::time::Duration::from_secs(2);
        loop {
            let snapshot = lab.snapshot();
            if snapshot.dsu_projection.controls_accepted >= 1 {
                assert_eq!(snapshot.dsu_projection.controls_submitted, 1);
                assert_eq!(snapshot.dsu_projection.controls_queue_full, 0);
                break;
            }
            assert!(
                Instant::now() < deadline,
                "DSU worker did not accept controls"
            );
            std::thread::yield_now();
        }
        let stopped = lab.stop_dsu().expect("stop DSU lab");
        assert_eq!(stopped.dsu_projection.status, "stopped");
    }

    #[test]
    fn motion_only_dsu_does_not_project_simulated_controls() {
        let mut lab = GamepadLab::new().expect("gamepad lab");
        let started = lab
            .start_dsu_mode(0, UiDsuMode::MotionOnly)
            .expect("start motion-only DSU lab");
        assert_eq!(started.dsu_projection.mode, UiDsuMode::MotionOnly);
        lab.apply(UpdateGamepadRequest::Button {
            button: UiGamepadButton::South,
            pressed: true,
        })
        .expect("update local controls without DSU projection");
        let snapshot = lab.snapshot();
        assert_eq!(snapshot.dsu_projection.controls_submitted, 0);
        assert_eq!(snapshot.dsu_projection.controls_accepted, 0);
        assert_eq!(
            snapshot.dsu_projection.last_submit,
            "motion_only_controls_not_projected"
        );
        lab.stop_dsu().expect("stop motion-only DSU lab");
    }

    #[test]
    fn invalid_control_does_not_consume_sequence() {
        let mut lab = GamepadLab::new().expect("gamepad lab");
        assert!(
            lab.apply(UpdateGamepadRequest::Dpad { x: 2, y: 0 })
                .is_err()
        );
        assert_eq!(lab.snapshot().sequence, None);
        assert_eq!(lab.composer.next_sequence(), Some(0));

        assert!(
            lab.apply(UpdateGamepadRequest::Stick {
                stick: UiGamepadStick::Right,
                x: i16::MIN,
                y: 0,
            })
            .is_err()
        );
        assert_eq!(lab.composer.next_sequence(), Some(0));
    }

    #[test]
    fn android_complete_state_feeds_the_same_dsu_worker() {
        let _port_guard = ANDROID_PORT_TEST_LOCK.lock().expect("Android port lock");
        let probe = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).expect("reserve UDP port");
        let port = probe.local_addr().expect("probe address").port();
        drop(probe);
        let mut lab = GamepadLab::new().expect("gamepad lab");
        let listening = lab.start_android(port).expect("start Android input");
        let token = listening
            .android_input
            .pairing_token
            .expect("pairing token");
        lab.start_dsu(0).expect("start DSU");
        let packet = format!(
            "{{\"version\":1,\"token\":\"{token}\",\"session\":\"abc-def\",\"sequence\":0,\"timestampNanos\":100,\"buttons\":1,\"dpadX\":0,\"dpadY\":0,\"leftX\":12000,\"leftY\":-9000,\"rightX\":0,\"rightY\":0,\"leftTrigger\":1000,\"rightTrigger\":2000,\"acceleration\":[0.0,1.0,9.7],\"angularVelocity\":[0.1,0.2,0.3],\"accelerationTimestampNanos\":90,\"angularVelocityTimestampNanos\":95}}"
        );
        let sender = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).expect("packet sender");
        sender
            .send_to(packet.as_bytes(), (Ipv4Addr::LOCALHOST, port))
            .expect("send Android packet");
        let deadline = Instant::now() + std::time::Duration::from_secs(2);
        loop {
            let snapshot = lab.snapshot();
            if snapshot.android_input.accepted_packets == 1
                && snapshot.dsu_projection.controls_accepted >= 1
            {
                assert_eq!(snapshot.source, "android_touch");
                assert!(!snapshot.simulated);
                assert_eq!(snapshot.pressed_buttons, vec!["south"]);
                assert_eq!(snapshot.left_stick.x, 12_000);
                assert_eq!(snapshot.motion.angular_velocity, [0.1, 0.2, 0.3]);
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Android packet did not reach DSU: accepted={} rejected={} last={} controls={}/{}",
                snapshot.android_input.accepted_packets,
                snapshot.android_input.rejected_packets,
                snapshot.android_input.last_event,
                snapshot.dsu_projection.controls_accepted,
                snapshot.dsu_projection.controls_submitted,
            );
            std::thread::yield_now();
        }
        lab.stop_dsu().expect("stop DSU");
        lab.stop_android().expect("stop Android input");
    }

    #[test]
    fn android_complete_state_reaches_bounded_windows_projection_ingress() {
        let _port_guard = ANDROID_PORT_TEST_LOCK.lock().expect("Android port lock");
        let probe = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).expect("reserve UDP port");
        let port = probe.local_addr().expect("probe address").port();
        drop(probe);
        let mut lab = GamepadLab::new().expect("gamepad lab");
        let listening = lab.start_android(port).expect("start Android input");
        let token = listening
            .android_input
            .pairing_token
            .expect("pairing token");
        let packet = format!(
            "{{\"version\":1,\"token\":\"{token}\",\"session\":\"d54-f17\",\"sequence\":17,\"timestampNanos\":100,\"buttons\":1,\"dpadX\":0,\"dpadY\":0,\"leftX\":0,\"leftY\":0,\"rightX\":0,\"rightY\":0,\"leftTrigger\":0,\"rightTrigger\":0,\"acceleration\":[0.0,1.0,9.7],\"angularVelocity\":[0.1,0.2,0.3],\"accelerationTimestampNanos\":90,\"angularVelocityTimestampNanos\":95}}"
        );
        UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))
            .expect("packet sender")
            .send_to(packet.as_bytes(), (Ipv4Addr::LOCALHOST, port))
            .expect("send Android packet");
        let deadline = Instant::now() + std::time::Duration::from_secs(2);
        loop {
            let snapshot = lab.snapshot();
            if snapshot.windows_projection.input_packets >= 1 {
                assert_eq!(snapshot.windows_projection.last_remote_sequence, Some(17));
                assert_eq!(snapshot.windows_projection.input_offline_events, 0);
                break;
            }
            if Instant::now() >= deadline {
                panic!(
                    "complete state did not reach Windows projection ingress: accepted={} rejected={} last={}",
                    snapshot.android_input.accepted_packets,
                    snapshot.android_input.rejected_packets,
                    snapshot.android_input.last_event
                );
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        lab.stop_android().expect("stop Android input");
        assert_eq!(lab.snapshot().windows_projection.input_offline_events, 1);
    }

    #[test]
    fn direct_gate_detaches_the_unused_windows_projection_ingress() {
        let _port_guard = ANDROID_PORT_TEST_LOCK.lock().expect("Android port lock");
        let probe = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).expect("reserve UDP port");
        let port = probe.local_addr().expect("probe address").port();
        drop(probe);
        let mut lab = GamepadLab::new().expect("gamepad lab");
        let listening = lab.start_android(port).expect("start Android input");
        let token = listening
            .android_input
            .pairing_token
            .expect("pairing token");
        lab.detach_android_host_projection_for_direct_gate()
            .expect("detach unused host ingress");
        let packet = format!(
            "{{\"version\":1,\"token\":\"{token}\",\"session\":\"d1ec7-54c\",\"sequence\":0,\"timestampNanos\":100,\"buttons\":1,\"dpadX\":0,\"dpadY\":0,\"leftX\":0,\"leftY\":0,\"rightX\":0,\"rightY\":0,\"leftTrigger\":0,\"rightTrigger\":0,\"acceleration\":[0.0,1.0,9.7],\"angularVelocity\":[0.1,0.2,0.3],\"accelerationTimestampNanos\":90,\"angularVelocityTimestampNanos\":95}}"
        );
        UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))
            .expect("packet sender")
            .send_to(packet.as_bytes(), (Ipv4Addr::LOCALHOST, port))
            .expect("send Android packet");
        let deadline = Instant::now() + std::time::Duration::from_secs(2);
        loop {
            let snapshot = lab.snapshot();
            if snapshot.android_input.accepted_packets == 1 {
                assert_eq!(snapshot.windows_projection.input_packets, 0);
                assert_eq!(snapshot.android_input.projection_queue_full, 0);
                break;
            }
            assert!(
                Instant::now() < deadline,
                "direct Gate packet was not accepted"
            );
            std::thread::yield_now();
        }
        lab.stop_android().expect("stop Android input");
        assert_eq!(lab.snapshot().windows_projection.input_offline_events, 0);
    }

    #[test]
    fn direct_gate_neutralizes_each_timeout_and_requires_a_later_packet() {
        let mut continuity = DirectGateContinuity::default();
        let mut snapshot = AndroidReceiverSnapshot {
            accepted_packets: 8,
            peer_connected: true,
            ..AndroidReceiverSnapshot::default()
        };
        assert!(!continuity.observe(&snapshot));
        assert!(continuity.latest_timeout_recovered());

        snapshot.peer_timeouts = 1;
        snapshot.peer_connected = false;
        assert!(continuity.observe(&snapshot));
        assert!(!continuity.latest_timeout_recovered());
        assert!(!continuity.observe(&snapshot));

        snapshot.accepted_packets = 9;
        snapshot.peer_connected = true;
        assert!(!continuity.observe(&snapshot));
        assert!(continuity.latest_timeout_recovered());
        assert_eq!(continuity.recovered_timeouts, 1);

        snapshot.peer_timeouts = 2;
        snapshot.peer_connected = false;
        assert!(continuity.observe(&snapshot));
        assert!(!continuity.latest_timeout_recovered());
    }

    /// Operator-assisted physical-device gate. Run this exact ignored test,
    /// copy the printed token/ports into the Android Controller Lab, press a
    /// control, and move the device before the deadline. The second subscriber
    /// is intentional: on Windows it exercises recovery from the ICMP reset
    /// produced after the first UDP subscriber disappears.
    #[test]
    #[ignore = "requires an explicitly authorized physical Android device on the local network"]
    fn physical_android_touch_and_imu_reach_a_reconnected_dsu_subscriber() {
        let android_port = std::env::var("CAPYIO_PHYSICAL_ANDROID_PORT")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(31_581);
        let dsu_port = std::env::var("CAPYIO_PHYSICAL_DSU_PORT")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(26_761);
        run_physical_gamepad_gate(android_port, dsu_port).expect("physical gamepad gate");
    }
}
