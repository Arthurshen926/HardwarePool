#![forbid(unsafe_code)]

//! Display-only contract for a future Vibepollo-derived streaming Adapter.
//!
//! This crate intentionally contains no upstream source, codec, socket,
//! platform API or driver call. It fixes the coexistence and ownership policy
//! that a later in-process C++ engine boundary must satisfy.

use std::fmt;

pub mod engine_abi;
pub mod launch_record;
pub mod lifecycle;

pub const SUNSHINE_DEFAULT_PORT_BASE: u16 = 47_989;
pub const MAX_WIDTH: u32 = 8_192;
pub const MAX_HEIGHT: u32 = 8_192;
pub const MAX_REFRESH_MILLIHERTZ: u32 = 500_000;
pub const MAX_BITRATE_KBPS: u32 = 200_000;

const VIDEO_PORT_OFFSET: u16 = 9;
const CONTROL_PORT_OFFSET: u16 = 10;
const RTSP_PORT_OFFSET: u16 = 21;
const HTTPS_PORT_OFFSET: u16 = 5;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VideoCodec {
    H264,
    Hevc,
    Av1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DynamicRange {
    Sdr8,
    Hdr10,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DisplayMode {
    pub width: u32,
    pub height: u32,
    pub refresh_millihertz: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DisplayOnlyPolicy {
    pub audio_enabled: bool,
    pub input_enabled: bool,
    pub discovery_enabled: bool,
    pub upnp_enabled: bool,
    pub may_replace_global_topology: bool,
    pub may_clean_foreign_monitors: bool,
}

impl DisplayOnlyPolicy {
    pub const CAPYIO: Self = Self {
        audio_enabled: false,
        input_enabled: false,
        discovery_enabled: false,
        upnp_enabled: false,
        may_replace_global_topology: false,
        may_clean_foreign_monitors: false,
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EnginePorts {
    pub https: u16,
    pub video: u16,
    pub control: u16,
    pub rtsp: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DisplayStreamSessionRequest {
    /// Stable CapyIO Adapter instance identity, never a process-global default.
    pub adapter_instance_id: [u8; 16],
    /// Stable Display Sink Capability identity used as virtual-monitor owner.
    pub display_capability_id: [u8; 16],
    pub mode: DisplayMode,
    pub codec: VideoCodec,
    pub dynamic_range: DynamicRange,
    pub bitrate_kbps: u32,
    /// GameStream-compatible port base selected for this CapyIO instance.
    pub port_base: u16,
    pub policy: DisplayOnlyPolicy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValidatedDisplayStreamSession {
    request: DisplayStreamSessionRequest,
    ports: EnginePorts,
}

impl ValidatedDisplayStreamSession {
    pub fn new(request: DisplayStreamSessionRequest) -> Result<Self, SessionContractError> {
        validate_identity(request.adapter_instance_id, "adapter instance")?;
        validate_identity(request.display_capability_id, "display capability")?;
        if request.mode.width == 0
            || request.mode.height == 0
            || request.mode.width > MAX_WIDTH
            || request.mode.height > MAX_HEIGHT
        {
            return Err(SessionContractError::InvalidMode);
        }
        if !(1_000..=MAX_REFRESH_MILLIHERTZ).contains(&request.mode.refresh_millihertz) {
            return Err(SessionContractError::InvalidRefreshRate);
        }
        if !(500..=MAX_BITRATE_KBPS).contains(&request.bitrate_kbps) {
            return Err(SessionContractError::InvalidBitrate);
        }
        if request.dynamic_range == DynamicRange::Hdr10 && request.codec == VideoCodec::H264 {
            return Err(SessionContractError::HdrRequiresTenBitCodec);
        }
        if request.policy != DisplayOnlyPolicy::CAPYIO {
            return Err(SessionContractError::UnsafePolicy);
        }
        if request.port_base < 1_024 {
            return Err(SessionContractError::PrivilegedPortNamespace);
        }
        if request.port_base == SUNSHINE_DEFAULT_PORT_BASE {
            return Err(SessionContractError::ForeignDefaultPortNamespace);
        }
        let ports = EnginePorts {
            https: request
                .port_base
                .checked_sub(HTTPS_PORT_OFFSET)
                .ok_or(SessionContractError::PortNamespaceOverflow)?,
            video: add_port_offset(request.port_base, VIDEO_PORT_OFFSET)?,
            control: add_port_offset(request.port_base, CONTROL_PORT_OFFSET)?,
            rtsp: add_port_offset(request.port_base, RTSP_PORT_OFFSET)?,
        };
        Ok(Self { request, ports })
    }

    #[must_use]
    pub const fn request(&self) -> &DisplayStreamSessionRequest {
        &self.request
    }

    #[must_use]
    pub const fn ports(&self) -> EnginePorts {
        self.ports
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionContractError {
    NilIdentity(&'static str),
    InvalidMode,
    InvalidRefreshRate,
    InvalidBitrate,
    HdrRequiresTenBitCodec,
    UnsafePolicy,
    PrivilegedPortNamespace,
    ForeignDefaultPortNamespace,
    PortNamespaceOverflow,
}

impl fmt::Display for SessionContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NilIdentity(name) => write!(formatter, "{name} identity must be nonzero"),
            Self::InvalidMode => formatter.write_str("display dimensions are outside bounds"),
            Self::InvalidRefreshRate => {
                formatter.write_str("display refresh rate is outside bounds")
            }
            Self::InvalidBitrate => formatter.write_str("display bitrate is outside bounds"),
            Self::HdrRequiresTenBitCodec => {
                formatter.write_str("HDR10 requires HEVC or AV1 in this contract")
            }
            Self::UnsafePolicy => {
                formatter.write_str("display-only coexistence policy was weakened")
            }
            Self::PrivilegedPortNamespace => {
                formatter.write_str("display port namespace must not use privileged ports")
            }
            Self::ForeignDefaultPortNamespace => {
                formatter.write_str("CapyIO must not claim Sunshine's default port namespace")
            }
            Self::PortNamespaceOverflow => formatter.write_str("display port namespace overflows"),
        }
    }
}

impl std::error::Error for SessionContractError {}

fn validate_identity(value: [u8; 16], name: &'static str) -> Result<(), SessionContractError> {
    if value == [0; 16] {
        Err(SessionContractError::NilIdentity(name))
    } else {
        Ok(())
    }
}

fn add_port_offset(base: u16, offset: u16) -> Result<u16, SessionContractError> {
    base.checked_add(offset)
        .filter(|port| *port >= 1_024)
        .ok_or(SessionContractError::PortNamespaceOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> DisplayStreamSessionRequest {
        DisplayStreamSessionRequest {
            adapter_instance_id: [1; 16],
            display_capability_id: [2; 16],
            mode: DisplayMode {
                width: 2_640,
                height: 1_216,
                refresh_millihertz: 120_000,
            },
            codec: VideoCodec::Hevc,
            dynamic_range: DynamicRange::Hdr10,
            bitrate_kbps: 45_000,
            port_base: 50_000,
            policy: DisplayOnlyPolicy::CAPYIO,
        }
    }

    #[test]
    fn validates_native_high_refresh_display_only_session() {
        let session = ValidatedDisplayStreamSession::new(request()).unwrap();
        assert_eq!(session.ports().https, 49_995);
        assert_eq!(session.ports().video, 50_009);
        assert_eq!(session.ports().control, 50_010);
        assert_eq!(session.ports().rtsp, 50_021);
        assert_eq!(session.request().mode.width, 2_640);
    }

    #[test]
    fn rejects_foreign_default_port_namespace_and_overflow() {
        let mut candidate = request();
        candidate.port_base = SUNSHINE_DEFAULT_PORT_BASE;
        assert_eq!(
            ValidatedDisplayStreamSession::new(candidate),
            Err(SessionContractError::ForeignDefaultPortNamespace)
        );

        candidate = request();
        candidate.port_base = 1_023;
        assert_eq!(
            ValidatedDisplayStreamSession::new(candidate),
            Err(SessionContractError::PrivilegedPortNamespace)
        );

        candidate = request();
        candidate.port_base = u16::MAX;
        assert_eq!(
            ValidatedDisplayStreamSession::new(candidate),
            Err(SessionContractError::PortNamespaceOverflow)
        );
    }

    #[test]
    fn rejects_nil_ownership_and_unsafe_global_behavior() {
        let mut candidate = request();
        candidate.display_capability_id = [0; 16];
        assert_eq!(
            ValidatedDisplayStreamSession::new(candidate),
            Err(SessionContractError::NilIdentity("display capability"))
        );

        candidate = request();
        candidate.policy.may_clean_foreign_monitors = true;
        assert_eq!(
            ValidatedDisplayStreamSession::new(candidate),
            Err(SessionContractError::UnsafePolicy)
        );
    }

    #[test]
    fn rejects_hdr_over_h264_and_out_of_range_quality() {
        let mut candidate = request();
        candidate.codec = VideoCodec::H264;
        assert_eq!(
            ValidatedDisplayStreamSession::new(candidate),
            Err(SessionContractError::HdrRequiresTenBitCodec)
        );

        candidate = request();
        candidate.mode.refresh_millihertz = MAX_REFRESH_MILLIHERTZ + 1;
        assert_eq!(
            ValidatedDisplayStreamSession::new(candidate),
            Err(SessionContractError::InvalidRefreshRate)
        );

        candidate = request();
        candidate.bitrate_kbps = MAX_BITRATE_KBPS + 1;
        assert_eq!(
            ValidatedDisplayStreamSession::new(candidate),
            Err(SessionContractError::InvalidBitrate)
        );
    }
}
