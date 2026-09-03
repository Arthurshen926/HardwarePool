//! Versioned Android composition boundary for private touchpad packets.
//!
//! The public Rust DTO deliberately contains only primitives that map to a
//! narrow JNI surface. Android framework objects never enter the core crates.

#![deny(unsafe_op_in_unsafe_fn)]

use std::{error::Error, fmt, str::FromStr};

use capyio_android_host::{
    AndroidMotionAction, AndroidMotionSample, AndroidPointerSample, AndroidToolType,
    AndroidTouchSurface, AndroidTouchpadCaptureError, AndroidTouchpadCaptureSession,
};
use capyio_core::{CapabilityId, NodeId, PortId, PortRef, RouteId, SessionId, StreamId};
use capyio_input::{
    InputStreamDescriptor, TouchpadButtonType, TouchpadDescriptor, TouchpadPhysicalSize,
};
use capyio_remote_touchpad_adapter::{
    PrivateTouchpadPacketSource, PrivateTouchpadPacketSourceError, PrivateTouchpadPacketV1,
    PrivateTouchpadRouteBinding, PrivateTouchpadTransportCodecV1,
    PrivateTouchpadTransportRecordError, PrivateTouchpadTransportRecordV1,
};

#[cfg(target_os = "android")]
mod android_ffi;

pub const ANDROID_TOUCHPAD_JNI_CONTRACT_VERSION: u32 = 1;
pub const ANDROID_ACTION_DOWN: i32 = 0;
pub const ANDROID_ACTION_UP: i32 = 1;
pub const ANDROID_ACTION_MOVE: i32 = 2;
pub const ANDROID_ACTION_CANCEL: i32 = 3;
pub const ANDROID_ACTION_POINTER_DOWN: i32 = 5;
pub const ANDROID_ACTION_POINTER_UP: i32 = 6;
pub const ANDROID_TOOL_TYPE_FINGER: i32 = 1;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AndroidTouchpadBridgeConfigV1 {
    pub stream_id: String,
    pub stream_epoch: u64,
    pub clock_domain_id: String,
    pub width_px: u32,
    pub height_px: u32,
    pub width_himetric: u32,
    pub height_himetric: u32,
    pub max_contacts: u8,
    pub reports_pressure: bool,
    pub first_sequence: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AndroidTouchpadRouteConfigV1 {
    pub route_id: String,
    pub session_id: String,
    pub source_node_id: String,
    pub source_capability_id: String,
    pub source_port_id: String,
    pub sink_node_id: String,
    pub sink_capability_id: String,
    pub sink_port_id: String,
    pub authorization_expires_at_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AndroidMotionDtoV1<'a> {
    pub event_time_nanos: u64,
    pub action: i32,
    pub action_index: usize,
    pub pointer_ids: &'a [i32],
    pub tool_types: &'a [i32],
    pub x_px: &'a [f32],
    pub y_px: &'a [f32],
    /// Negative values mean absent; non-negative values carry Android pressure.
    pub pressure: &'a [f32],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AndroidTouchpadBridgeError {
    InvalidConfig(String),
    InvalidMotion(String),
    Capture(AndroidTouchpadCaptureError),
    Packet(PrivateTouchpadPacketSourceError),
    Transport(PrivateTouchpadTransportRecordError),
}

impl fmt::Display for AndroidTouchpadBridgeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig(message) => write!(formatter, "invalid bridge config: {message}"),
            Self::InvalidMotion(message) => write!(formatter, "invalid motion DTO: {message}"),
            Self::Capture(error) => error.fmt(formatter),
            Self::Packet(error) => error.fmt(formatter),
            Self::Transport(error) => error.fmt(formatter),
        }
    }
}

impl Error for AndroidTouchpadBridgeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Capture(error) => Some(error),
            Self::Packet(error) => Some(error),
            Self::Transport(error) => Some(error),
            _ => None,
        }
    }
}

impl From<AndroidTouchpadCaptureError> for AndroidTouchpadBridgeError {
    fn from(value: AndroidTouchpadCaptureError) -> Self {
        Self::Capture(value)
    }
}

impl From<PrivateTouchpadPacketSourceError> for AndroidTouchpadBridgeError {
    fn from(value: PrivateTouchpadPacketSourceError) -> Self {
        Self::Packet(value)
    }
}

impl From<PrivateTouchpadTransportRecordError> for AndroidTouchpadBridgeError {
    fn from(value: PrivateTouchpadTransportRecordError) -> Self {
        Self::Transport(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AndroidTouchpadPacketSessionV1 {
    capture: AndroidTouchpadCaptureSession,
    source: PrivateTouchpadPacketSource,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AndroidTouchpadRecordSessionV1 {
    packets: AndroidTouchpadPacketSessionV1,
    transport: PrivateTouchpadTransportCodecV1,
}

impl AndroidTouchpadRecordSessionV1 {
    pub fn new(
        packet_config: AndroidTouchpadBridgeConfigV1,
        route_config: AndroidTouchpadRouteConfigV1,
    ) -> Result<Self, AndroidTouchpadBridgeError> {
        let epoch = packet_config.stream_epoch;
        let binding = PrivateTouchpadRouteBinding {
            route_id: parse_id::<RouteId>(&route_config.route_id, "route ID")?,
            session_id: parse_id::<SessionId>(&route_config.session_id, "session ID")?,
            source: parse_port(
                &route_config.source_node_id,
                &route_config.source_capability_id,
                &route_config.source_port_id,
                "source",
            )?,
            sink: parse_port(
                &route_config.sink_node_id,
                &route_config.sink_capability_id,
                &route_config.sink_port_id,
                "sink",
            )?,
            route_epoch: epoch,
            authorization_expires_at_ms: route_config.authorization_expires_at_ms,
        };
        Ok(Self {
            packets: AndroidTouchpadPacketSessionV1::new(packet_config)?,
            transport: PrivateTouchpadTransportCodecV1::new(binding),
        })
    }

    #[must_use]
    pub fn hello(&self) -> PrivateTouchpadTransportRecordV1 {
        self.transport.encode_hello()
    }

    pub fn start(
        &mut self,
        event_time_nanos: u64,
    ) -> Result<PrivateTouchpadTransportRecordV1, AndroidTouchpadBridgeError> {
        let packet = self.packets.start(event_time_nanos)?;
        Ok(self.transport.encode_data(&packet)?)
    }

    pub fn motion(
        &mut self,
        dto: AndroidMotionDtoV1<'_>,
    ) -> Result<PrivateTouchpadTransportRecordV1, AndroidTouchpadBridgeError> {
        let packet = self.packets.motion(dto)?;
        Ok(self.transport.encode_data(&packet)?)
    }

    pub fn stop(
        &mut self,
        event_time_nanos: u64,
    ) -> Result<Option<PrivateTouchpadTransportRecordV1>, AndroidTouchpadBridgeError> {
        self.packets
            .stop(event_time_nanos)?
            .map(|packet| self.transport.encode_data(&packet).map_err(Into::into))
            .transpose()
    }

    pub fn close(
        &mut self,
        event_time_nanos: u64,
    ) -> Result<Option<PrivateTouchpadTransportRecordV1>, AndroidTouchpadBridgeError> {
        self.packets
            .close(event_time_nanos)?
            .map(|packet| self.transport.encode_data(&packet).map_err(Into::into))
            .transpose()
    }

    #[must_use]
    pub fn close_record(&self) -> PrivateTouchpadTransportRecordV1 {
        self.transport.encode_close()
    }

    pub fn validate_ack(
        &self,
        bytes: &[u8],
        expected_sequence: u64,
    ) -> Result<(), AndroidTouchpadBridgeError> {
        Ok(self.transport.validate_ack(bytes, expected_sequence)?)
    }
}

fn parse_port(
    node: &str,
    capability: &str,
    port: &str,
    label: &str,
) -> Result<PortRef, AndroidTouchpadBridgeError> {
    Ok(PortRef {
        node_id: parse_id::<NodeId>(node, &format!("{label} node ID"))?,
        capability_id: parse_id::<CapabilityId>(capability, &format!("{label} capability ID"))?,
        port_id: parse_id::<PortId>(port, &format!("{label} port ID"))?,
    })
}

fn parse_id<T>(value: &str, label: &str) -> Result<T, AndroidTouchpadBridgeError>
where
    T: FromStr,
    T::Err: fmt::Display,
{
    value.parse().map_err(|error: T::Err| {
        AndroidTouchpadBridgeError::InvalidConfig(format!("{label}: {error}"))
    })
}

impl AndroidTouchpadPacketSessionV1 {
    pub fn new(config: AndroidTouchpadBridgeConfigV1) -> Result<Self, AndroidTouchpadBridgeError> {
        let stream_id = StreamId::from_str(&config.stream_id)
            .map_err(|error| AndroidTouchpadBridgeError::InvalidConfig(error.to_string()))?;
        let stream = InputStreamDescriptor {
            stream_id,
            stream_epoch: config.stream_epoch,
            clock_domain_id: config.clock_domain_id,
        };
        let descriptor = TouchpadDescriptor {
            physical_size: TouchpadPhysicalSize {
                width_himetric: config.width_himetric,
                height_himetric: config.height_himetric,
            },
            max_contacts: config.max_contacts,
            button_type: TouchpadButtonType::NonClickable,
            reports_contact_size: false,
            reports_pressure: config.reports_pressure,
        };
        let surface = AndroidTouchSurface {
            width_px: config.width_px,
            height_px: config.height_px,
            descriptor,
        };
        Ok(Self {
            capture: AndroidTouchpadCaptureSession::new(
                stream.clone(),
                surface,
                config.first_sequence,
            )?,
            source: PrivateTouchpadPacketSource::new(stream, descriptor, config.first_sequence)?,
        })
    }

    pub fn start(
        &mut self,
        event_time_nanos: u64,
    ) -> Result<PrivateTouchpadPacketV1, AndroidTouchpadBridgeError> {
        let frame = self.capture.start(event_time_nanos)?;
        Ok(self.source.encode(&frame)?)
    }

    pub fn motion(
        &mut self,
        dto: AndroidMotionDtoV1<'_>,
    ) -> Result<PrivateTouchpadPacketV1, AndroidTouchpadBridgeError> {
        let event = map_motion_dto(dto)?;
        let frame = self.capture.map_motion(&event)?;
        Ok(self.source.encode(&frame)?)
    }

    pub fn stop(
        &mut self,
        event_time_nanos: u64,
    ) -> Result<Option<PrivateTouchpadPacketV1>, AndroidTouchpadBridgeError> {
        self.capture
            .stop(event_time_nanos)?
            .map(|frame| self.source.encode(&frame).map_err(Into::into))
            .transpose()
    }

    pub fn close(
        &mut self,
        event_time_nanos: u64,
    ) -> Result<Option<PrivateTouchpadPacketV1>, AndroidTouchpadBridgeError> {
        let packet = self
            .capture
            .close(event_time_nanos)?
            .map(|frame| {
                self.source
                    .encode(&frame)
                    .map_err(AndroidTouchpadBridgeError::from)
            })
            .transpose()?;
        self.source.close()?;
        Ok(packet)
    }
}

fn map_motion_dto(
    dto: AndroidMotionDtoV1<'_>,
) -> Result<AndroidMotionSample, AndroidTouchpadBridgeError> {
    let count = dto.pointer_ids.len();
    if [
        dto.tool_types.len(),
        dto.x_px.len(),
        dto.y_px.len(),
        dto.pressure.len(),
    ]
    .into_iter()
    .any(|len| len != count)
    {
        return Err(AndroidTouchpadBridgeError::InvalidMotion(
            "all pointer arrays must have identical lengths".to_owned(),
        ));
    }
    let action = match dto.action {
        ANDROID_ACTION_DOWN => AndroidMotionAction::Down,
        ANDROID_ACTION_UP => AndroidMotionAction::Up {
            action_index: dto.action_index,
        },
        ANDROID_ACTION_MOVE => AndroidMotionAction::Move,
        ANDROID_ACTION_CANCEL => AndroidMotionAction::Cancel,
        ANDROID_ACTION_POINTER_DOWN => AndroidMotionAction::PointerDown {
            action_index: dto.action_index,
        },
        ANDROID_ACTION_POINTER_UP => AndroidMotionAction::PointerUp {
            action_index: dto.action_index,
        },
        actual => {
            return Err(AndroidTouchpadBridgeError::InvalidMotion(format!(
                "unsupported MotionEvent action {actual}"
            )));
        }
    };
    let pointers = (0..count)
        .map(|index| {
            let pointer_id = u32::try_from(dto.pointer_ids[index]).map_err(|_| {
                AndroidTouchpadBridgeError::InvalidMotion(
                    "pointer IDs must be non-negative".to_owned(),
                )
            })?;
            let tool_type = match dto.tool_types[index] {
                ANDROID_TOOL_TYPE_FINGER => AndroidToolType::Finger,
                2 => AndroidToolType::Stylus,
                3 => AndroidToolType::Mouse,
                4 => AndroidToolType::Eraser,
                _ => AndroidToolType::Unknown,
            };
            let pressure = (dto.pressure[index] >= 0.0).then_some(dto.pressure[index]);
            Ok(AndroidPointerSample {
                pointer_id,
                tool_type,
                x_px: dto.x_px[index],
                y_px: dto.y_px[index],
                pressure,
            })
        })
        .collect::<Result<Vec<_>, AndroidTouchpadBridgeError>>()?;
    Ok(AndroidMotionSample {
        event_time_nanos: dto.event_time_nanos,
        action,
        pointers,
    })
}
