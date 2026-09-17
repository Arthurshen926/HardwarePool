//! Versioned, data-only boundary for probing and starting a native video engine.
//!
//! This module does not load a library or call foreign code. The C-compatible
//! records let a later bridge validate an engine before it receives any Route
//! identity or starts capture, networking or a virtual display.

use std::{fmt, mem::size_of};

use crate::{DynamicRange, EnginePorts, ValidatedDisplayStreamSession, VideoCodec};

pub const ENGINE_ABI_V2: u32 = 2;
pub const ENGINE_AUTHENTICATED_START_ABI_V1: u32 = 1;
pub const CAPTURE_OUTPUT_UTF8_CAPACITY: usize = 64;
pub const MAX_REPORTED_ENGINE_SESSIONS: u32 = 256;
pub const MIN_STOP_TIMEOUT_MILLIS: u32 = 100;
pub const MAX_STOP_TIMEOUT_MILLIS: u32 = 10_000;

pub const FEATURE_CAPTURE: u64 = 1 << 0;
pub const FEATURE_HARDWARE_ENCODE: u64 = 1 << 1;
pub const FEATURE_PACING: u64 = 1 << 2;
pub const FEATURE_PACKET_RECOVERY: u64 = 1 << 3;
pub const FEATURE_VIDEO_CONTROL: u64 = 1 << 4;
pub const FEATURE_HDR10: u64 = 1 << 5;
pub const FEATURE_HEVC: u64 = 1 << 6;
pub const FEATURE_AV1: u64 = 1 << 7;

pub const FEATURE_AUDIO: u64 = 1 << 32;
pub const FEATURE_INPUT: u64 = 1 << 33;
pub const FEATURE_GAMEPAD: u64 = 1 << 34;
pub const FEATURE_DISCOVERY: u64 = 1 << 35;
pub const FEATURE_UPNP: u64 = 1 << 36;
pub const FEATURE_GLOBAL_TOPOLOGY: u64 = 1 << 37;
pub const FEATURE_FOREIGN_MONITOR_CLEANUP: u64 = 1 << 38;

pub const REQUIRED_VIDEO_FEATURES: u64 = FEATURE_CAPTURE
    | FEATURE_HARDWARE_ENCODE
    | FEATURE_PACING
    | FEATURE_PACKET_RECOVERY
    | FEATURE_VIDEO_CONTROL;

pub const FORBIDDEN_DISPLAY_ONLY_FEATURES: u64 = FEATURE_AUDIO
    | FEATURE_INPUT
    | FEATURE_GAMEPAD
    | FEATURE_DISCOVERY
    | FEATURE_UPNP
    | FEATURE_GLOBAL_TOPOLOGY
    | FEATURE_FOREIGN_MONITOR_CLEANUP;

pub const KNOWN_ENGINE_FEATURES: u64 = REQUIRED_VIDEO_FEATURES
    | FEATURE_HDR10
    | FEATURE_HEVC
    | FEATURE_AV1
    | FORBIDDEN_DISPLAY_ONLY_FEATURES;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct EngineProbeV2 {
    pub struct_size: u32,
    pub abi_version: u32,
    pub enabled_features: u64,
    pub max_concurrent_sessions: u32,
    pub reserved: u32,
}

impl EngineProbeV2 {
    #[must_use]
    pub const fn display_only_baseline(max_concurrent_sessions: u32) -> Self {
        Self {
            struct_size: size_of::<Self>() as u32,
            abi_version: ENGINE_ABI_V2,
            enabled_features: REQUIRED_VIDEO_FEATURES,
            max_concurrent_sessions,
            reserved: 0,
        }
    }

    pub fn validate(self) -> Result<ValidatedEngineProbe, EngineAbiError> {
        if self.struct_size != size_of::<Self>() as u32 {
            return Err(EngineAbiError::UnexpectedStructSize);
        }
        if self.abi_version != ENGINE_ABI_V2 {
            return Err(EngineAbiError::UnsupportedAbiVersion);
        }
        if self.reserved != 0 {
            return Err(EngineAbiError::ReservedFieldNotZero);
        }
        if self.enabled_features & REQUIRED_VIDEO_FEATURES != REQUIRED_VIDEO_FEATURES {
            return Err(EngineAbiError::MissingRequiredVideoFeature);
        }
        if self.enabled_features & FORBIDDEN_DISPLAY_ONLY_FEATURES != 0 {
            return Err(EngineAbiError::ForbiddenSubsystemEnabled);
        }
        if self.enabled_features & !KNOWN_ENGINE_FEATURES != 0 {
            return Err(EngineAbiError::UnknownFeatureEnabled);
        }
        if !(1..=MAX_REPORTED_ENGINE_SESSIONS).contains(&self.max_concurrent_sessions) {
            return Err(EngineAbiError::InvalidSessionCapacity);
        }
        Ok(ValidatedEngineProbe(self))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValidatedEngineProbe(EngineProbeV2);

impl ValidatedEngineProbe {
    #[must_use]
    pub const fn enabled_features(self) -> u64 {
        self.0.enabled_features
    }

    #[must_use]
    pub const fn max_concurrent_sessions(self) -> u32 {
        self.0.max_concurrent_sessions
    }

    pub fn supports(self, session: &ValidatedDisplayStreamSession) -> Result<(), EngineAbiError> {
        let required = match (session.request().codec, session.request().dynamic_range) {
            (VideoCodec::H264, DynamicRange::Sdr8) => 0,
            (VideoCodec::Hevc, DynamicRange::Sdr8) => FEATURE_HEVC,
            (VideoCodec::Av1, DynamicRange::Sdr8) => FEATURE_AV1,
            (VideoCodec::Hevc, DynamicRange::Hdr10) => FEATURE_HEVC | FEATURE_HDR10,
            (VideoCodec::Av1, DynamicRange::Hdr10) => FEATURE_AV1 | FEATURE_HDR10,
            (VideoCodec::H264, DynamicRange::Hdr10) => {
                return Err(EngineAbiError::UnsupportedSessionFeatures);
            }
        };
        if self.0.enabled_features & required == required {
            Ok(())
        } else {
            Err(EngineAbiError::UnsupportedSessionFeatures)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct EngineStartV2 {
    pub struct_size: u32,
    pub abi_version: u32,
    pub adapter_instance_id: [u8; 16],
    pub display_capability_id: [u8; 16],
    pub capture_output_utf8: [u8; CAPTURE_OUTPUT_UTF8_CAPACITY],
    pub width: u32,
    pub height: u32,
    pub refresh_millihertz: u32,
    pub codec: u32,
    pub dynamic_range: u32,
    pub bitrate_kbps: u32,
    pub https_port: u16,
    pub video_port: u16,
    pub control_port: u16,
    pub rtsp_port: u16,
}

impl EngineStartV2 {
    pub fn new(
        probe: ValidatedEngineProbe,
        session: &ValidatedDisplayStreamSession,
        capture_output: WindowsCaptureOutput,
    ) -> Result<Self, EngineAbiError> {
        probe.supports(session)?;
        let request = session.request();
        let EnginePorts {
            https,
            video,
            control,
            rtsp,
        } = session.ports();
        Ok(Self {
            struct_size: size_of::<Self>() as u32,
            abi_version: ENGINE_ABI_V2,
            adapter_instance_id: request.adapter_instance_id,
            display_capability_id: request.display_capability_id,
            capture_output_utf8: capture_output.0,
            width: request.mode.width,
            height: request.mode.height,
            refresh_millihertz: request.mode.refresh_millihertz,
            codec: match request.codec {
                VideoCodec::H264 => 1,
                VideoCodec::Hevc => 2,
                VideoCodec::Av1 => 3,
            },
            dynamic_range: match request.dynamic_range {
                DynamicRange::Sdr8 => 1,
                DynamicRange::Hdr10 => 2,
            },
            bitrate_kbps: request.bitrate_kbps,
            https_port: https,
            video_port: video,
            control_port: control,
            rtsp_port: rtsp,
        })
    }
}

/// Raw one-shot authenticated-start extension record. It deliberately does
/// not implement `Copy` or `Debug`; the Windows loader owns and wipes it.
#[repr(C)]
pub struct EngineLaunchMaterialV1 {
    pub struct_size: u32,
    pub abi_version: u32,
    pub launch_session_id: u32,
    pub key: [u8; 16],
    pub key_id: u32,
    pub ping_payload: [u8; 8],
    pub control_connect_data: u32,
    pub reserved: [u32; 4],
}

impl EngineLaunchMaterialV1 {
    #[must_use]
    pub const fn new(
        launch_session_id: u32,
        key: [u8; 16],
        key_id: u32,
        ping_payload: [u8; 8],
        control_connect_data: u32,
    ) -> Self {
        Self {
            struct_size: size_of::<Self>() as u32,
            abi_version: ENGINE_AUTHENTICATED_START_ABI_V1,
            launch_session_id,
            key,
            key_id,
            ping_payload,
            control_connect_data,
            reserved: [0; 4],
        }
    }

    pub fn validate(&self) -> Result<(), EngineAbiError> {
        if self.struct_size != size_of::<Self>() as u32 {
            return Err(EngineAbiError::UnexpectedStructSize);
        }
        if self.abi_version != ENGINE_AUTHENTICATED_START_ABI_V1 {
            return Err(EngineAbiError::UnsupportedAbiVersion);
        }
        if self.launch_session_id == 0 || self.key == [0; 16] {
            return Err(EngineAbiError::InvalidLaunchMaterial);
        }
        if self.reserved != [0; 4] {
            return Err(EngineAbiError::ReservedFieldNotZero);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowsCaptureOutput([u8; CAPTURE_OUTPUT_UTF8_CAPACITY]);

impl WindowsCaptureOutput {
    pub fn parse(value: &str) -> Result<Self, EngineAbiError> {
        const PREFIX: &str = r"\\.\DISPLAY";
        let suffix = value
            .strip_prefix(PREFIX)
            .ok_or(EngineAbiError::InvalidCaptureOutput)?;
        if suffix.is_empty()
            || suffix.starts_with('0')
            || !suffix.bytes().all(|byte| byte.is_ascii_digit())
            || value.len() >= CAPTURE_OUTPUT_UTF8_CAPACITY
        {
            return Err(EngineAbiError::InvalidCaptureOutput);
        }
        let mut bytes = [0; CAPTURE_OUTPUT_UTF8_CAPACITY];
        bytes[..value.len()].copy_from_slice(value.as_bytes());
        Ok(Self(bytes))
    }

    #[must_use]
    pub const fn as_abi_bytes(self) -> [u8; CAPTURE_OUTPUT_UTF8_CAPACITY] {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub enum EngineCallStatus {
    Ok = 0,
    InvalidArgument = -1,
    InvalidState = -2,
    CapacityExhausted = -3,
    Unsupported = -4,
    InternalFailure = -5,
    Timeout = -6,
}

impl EngineCallStatus {
    pub fn from_raw(value: i32) -> Result<Self, EngineAbiError> {
        match value {
            0 => Ok(Self::Ok),
            -1 => Ok(Self::InvalidArgument),
            -2 => Ok(Self::InvalidState),
            -3 => Ok(Self::CapacityExhausted),
            -4 => Ok(Self::Unsupported),
            -5 => Ok(Self::InternalFailure),
            -6 => Ok(Self::Timeout),
            _ => Err(EngineAbiError::UnknownStatusCode),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum EngineEventKind {
    Started = 1,
    FirstFrame = 2,
    KeyFrameRequested = 3,
    CongestionChanged = 4,
    Stopped = 5,
    Fatal = 6,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct EngineEventV2 {
    pub struct_size: u32,
    pub abi_version: u32,
    pub kind: u32,
    pub status: i32,
    pub adapter_instance_id: [u8; 16],
    pub display_capability_id: [u8; 16],
    pub sequence: u64,
    pub value: u64,
}

impl EngineEventV2 {
    pub fn validate(&self) -> Result<(EngineEventKind, EngineCallStatus), EngineAbiError> {
        if self.struct_size != size_of::<Self>() as u32 {
            return Err(EngineAbiError::UnexpectedStructSize);
        }
        if self.abi_version != ENGINE_ABI_V2 {
            return Err(EngineAbiError::UnsupportedAbiVersion);
        }
        if self.adapter_instance_id == [0; 16] || self.display_capability_id == [0; 16] {
            return Err(EngineAbiError::NilEventIdentity);
        }
        let kind = match self.kind {
            1 => EngineEventKind::Started,
            2 => EngineEventKind::FirstFrame,
            3 => EngineEventKind::KeyFrameRequested,
            4 => EngineEventKind::CongestionChanged,
            5 => EngineEventKind::Stopped,
            6 => EngineEventKind::Fatal,
            _ => return Err(EngineAbiError::UnknownEventKind),
        };
        Ok((kind, EngineCallStatus::from_raw(self.status)?))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EngineAbiError {
    UnexpectedStructSize,
    UnsupportedAbiVersion,
    ReservedFieldNotZero,
    MissingRequiredVideoFeature,
    ForbiddenSubsystemEnabled,
    UnknownFeatureEnabled,
    InvalidSessionCapacity,
    UnsupportedSessionFeatures,
    UnknownStatusCode,
    UnknownEventKind,
    NilEventIdentity,
    InvalidCaptureOutput,
    InvalidLaunchMaterial,
}

impl fmt::Display for EngineAbiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::UnexpectedStructSize => "engine ABI record size does not match version 1",
            Self::UnsupportedAbiVersion => "engine ABI version is unsupported",
            Self::ReservedFieldNotZero => "engine ABI reserved field must be zero",
            Self::MissingRequiredVideoFeature => "engine omits a required video feature",
            Self::ForbiddenSubsystemEnabled => "engine enables a forbidden display-only subsystem",
            Self::UnknownFeatureEnabled => "engine enables an unknown feature bit",
            Self::InvalidSessionCapacity => "engine session capacity is outside bounds",
            Self::UnsupportedSessionFeatures => {
                "engine cannot satisfy the requested codec or range"
            }
            Self::UnknownStatusCode => "engine returned an unknown status code",
            Self::UnknownEventKind => "engine emitted an unknown event kind",
            Self::NilEventIdentity => "engine event identity must be nonzero",
            Self::InvalidCaptureOutput => {
                "engine capture output must be a canonical bounded Windows DXGI name"
            }
            Self::InvalidLaunchMaterial => {
                "authenticated engine launch material is incomplete or invalid"
            }
        })
    }
}

impl std::error::Error for EngineAbiError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        DisplayMode, DisplayOnlyPolicy, DisplayStreamSessionRequest, ValidatedDisplayStreamSession,
    };

    fn session(codec: VideoCodec, dynamic_range: DynamicRange) -> ValidatedDisplayStreamSession {
        ValidatedDisplayStreamSession::new(DisplayStreamSessionRequest {
            adapter_instance_id: [1; 16],
            display_capability_id: [2; 16],
            mode: DisplayMode {
                width: 2_640,
                height: 1_216,
                refresh_millihertz: 120_000,
            },
            codec,
            dynamic_range,
            bitrate_kbps: 45_000,
            port_base: 50_000,
            policy: DisplayOnlyPolicy::CAPYIO,
        })
        .unwrap()
    }

    #[test]
    fn abi_layout_and_start_record_are_fixed() {
        assert_eq!(size_of::<EngineProbeV2>(), 24);
        assert_eq!(size_of::<EngineStartV2>(), 136);
        assert_eq!(size_of::<EngineLaunchMaterialV1>(), 60);
        let probe = EngineProbeV2 {
            enabled_features: REQUIRED_VIDEO_FEATURES | FEATURE_HEVC | FEATURE_HDR10,
            ..EngineProbeV2::display_only_baseline(1)
        }
        .validate()
        .unwrap();
        let output = WindowsCaptureOutput::parse(r"\\.\DISPLAY2").unwrap();
        let start = EngineStartV2::new(
            probe,
            &session(VideoCodec::Hevc, DynamicRange::Hdr10),
            output,
        )
        .unwrap();
        assert_eq!(start.adapter_instance_id, [1; 16]);
        assert_eq!(start.display_capability_id, [2; 16]);
        assert_eq!(&start.capture_output_utf8[..12], br"\\.\DISPLAY2");
        assert_eq!((start.codec, start.dynamic_range), (2, 2));
        assert_eq!(start.video_port, 50_009);
    }

    #[test]
    fn authenticated_launch_material_is_fixed_and_fails_closed() {
        let mut material = EngineLaunchMaterialV1::new(7, [0x42; 16], 3, [4; 8], 5);
        assert!(material.validate().is_ok());
        material.key = [0; 16];
        assert!(matches!(
            material.validate(),
            Err(EngineAbiError::InvalidLaunchMaterial)
        ));
        material.key = [0x42; 16];
        material.reserved[0] = 1;
        assert!(matches!(
            material.validate(),
            Err(EngineAbiError::ReservedFieldNotZero)
        ));
    }

    #[test]
    fn probe_rejects_abi_drift_missing_video_and_foreign_subsystems() {
        let mut probe = EngineProbeV2::display_only_baseline(1);
        probe.struct_size -= 1;
        assert_eq!(probe.validate(), Err(EngineAbiError::UnexpectedStructSize));

        probe = EngineProbeV2::display_only_baseline(1);
        probe.enabled_features &= !FEATURE_PACKET_RECOVERY;
        assert_eq!(
            probe.validate(),
            Err(EngineAbiError::MissingRequiredVideoFeature)
        );

        probe = EngineProbeV2::display_only_baseline(1);
        probe.enabled_features |= FEATURE_AUDIO;
        assert_eq!(
            probe.validate(),
            Err(EngineAbiError::ForbiddenSubsystemEnabled)
        );

        probe = EngineProbeV2::display_only_baseline(1);
        probe.enabled_features |= 1 << 63;
        assert_eq!(probe.validate(), Err(EngineAbiError::UnknownFeatureEnabled));
    }

    #[test]
    fn probe_rejects_zero_capacity_and_missing_requested_codec() {
        assert_eq!(
            EngineProbeV2::display_only_baseline(0).validate(),
            Err(EngineAbiError::InvalidSessionCapacity)
        );
        assert_eq!(
            EngineProbeV2::display_only_baseline(MAX_REPORTED_ENGINE_SESSIONS + 1).validate(),
            Err(EngineAbiError::InvalidSessionCapacity)
        );
        let probe = EngineProbeV2::display_only_baseline(1).validate().unwrap();
        assert_eq!(
            EngineStartV2::new(
                probe,
                &session(VideoCodec::Av1, DynamicRange::Sdr8),
                WindowsCaptureOutput::parse(r"\\.\DISPLAY2").unwrap(),
            ),
            Err(EngineAbiError::UnsupportedSessionFeatures)
        );
    }

    #[test]
    fn capture_output_is_exact_bounded_and_canonical() {
        let output = WindowsCaptureOutput::parse(r"\\.\DISPLAY12").unwrap();
        assert_eq!(&output.as_abi_bytes()[..13], br"\\.\DISPLAY12");
        for invalid in [
            "",
            r"\\.\display2",
            r"\\.\DISPLAY0",
            r"\\.\DISPLAY02",
            r"DISPLAY2",
            r"\\.\DISPLAY2-extra",
        ] {
            assert_eq!(
                WindowsCaptureOutput::parse(invalid),
                Err(EngineAbiError::InvalidCaptureOutput)
            );
        }
    }

    #[test]
    fn event_record_is_fixed_bounded_and_closed_to_unknown_values() {
        assert_eq!(size_of::<EngineEventV2>(), 64);
        let mut event = EngineEventV2 {
            struct_size: size_of::<EngineEventV2>() as u32,
            abi_version: ENGINE_ABI_V2,
            kind: EngineEventKind::FirstFrame as u32,
            status: EngineCallStatus::Ok as i32,
            adapter_instance_id: [1; 16],
            display_capability_id: [2; 16],
            sequence: 1,
            value: 16_667,
        };
        assert_eq!(
            event.validate(),
            Ok((EngineEventKind::FirstFrame, EngineCallStatus::Ok))
        );

        event.kind = 99;
        assert_eq!(event.validate(), Err(EngineAbiError::UnknownEventKind));
        event.kind = EngineEventKind::Fatal as u32;
        event.status = -99;
        assert_eq!(event.validate(), Err(EngineAbiError::UnknownStatusCode));
    }
}
