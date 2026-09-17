#![deny(unsafe_op_in_unsafe_fn)]
#![deny(unsafe_code)]

//! Windows virtual-monitor handoff plus consent-gated capture fallback.
//!
//! The platform-neutral Source can be tested with a fake backend. The Windows
//! module is the only place that may open the system picker or touch WGC/D3D11.

mod error;
mod idd_control;
mod idd_handoff;
mod nv12;
mod source;
mod sudovda_control;

#[cfg(windows)]
#[allow(unsafe_code)]
mod idd_mapping;

#[cfg(windows)]
#[allow(unsafe_code)]
mod idd_device_control;

#[cfg(windows)]
#[allow(unsafe_code)]
mod sudovda_device_control;

#[cfg(windows)]
mod idd_coordinator;

#[cfg(windows)]
#[allow(unsafe_code)]
mod display_config;

#[cfg(windows)]
#[allow(unsafe_code)]
mod engine_output;

#[cfg(windows)]
#[allow(unsafe_code)]
mod display_session;

#[cfg(windows)]
#[allow(unsafe_code)]
mod mf_encoder;

#[cfg(windows)]
mod mf_pipeline;

#[cfg(windows)]
#[allow(unsafe_code)]
pub mod wgc;

#[cfg(windows)]
pub use display_config::{
    CapyIoDisplayPath, DisplayPathCategory, DisplayTopologyApplyResult, DisplayTopologyError,
    PhysicalDisplaySuppressionLease, PhysicalDisplaySuppressionSummary, WindowsDisplayPath,
    WindowsDisplayTopologyLease, activate_capyio_extended_path, inspect_capyio_display_path,
    inspect_windows_display_topology, preview_physical_display_suppression,
    suppress_active_physical_displays, validate_capyio_extended_path,
};
#[cfg(windows)]
pub use display_session::{
    DisplaySessionContext, DisplaySessionError, InteractiveDisplaySessionId,
    VerifiedInteractiveDisplaySession, verify_current_process_display_session,
};
#[cfg(windows)]
pub use engine_output::{
    DisplayOutputBindingError, VerifiedWindowsDisplayOutput, activate_owned_windows_display_output,
    configure_process_display_dpi_awareness, verify_owned_windows_display_output,
    verify_static_capyio_display_output,
};
pub use error::WindowsDisplayError;
pub use idd_control::{
    IDD_CONNECT_REQUEST_BYTES, IDD_CONTROL_ABI_VERSION, IDD_CONTROL_MAGIC,
    IDD_CONTROL_MAX_MONITORS, IDD_CONTROL_MAX_REFRESH_MILLIHZ, IDD_DISCONNECT_REQUEST_BYTES,
    IDD_MONITOR_INVENTORY_RESPONSE_BYTES, IOCTL_CAPYIO_CONNECT_MONITOR,
    IOCTL_CAPYIO_DISCONNECT_MONITOR, IOCTL_CAPYIO_ENUMERATE_MONITORS, IOCTL_CAPYIO_QUERY_ADAPTER,
    IddControlError, IddMonitorConnectRequest, IddMonitorDisconnectRequest, IddMonitorMode,
};
#[cfg(windows)]
pub use idd_coordinator::{IddCoordinatorError, IddCoordinatorInventory, IddMonitorCoordinator};
#[cfg(windows)]
pub use idd_device_control::{
    IddAdapterStatus, IddConnectedMonitor, IddDeviceControlError, IddDeviceController,
};
pub use idd_handoff::{
    IDD_HANDOFF_ABI_VERSION, IDD_HANDOFF_HEADER_BYTES, IDD_HANDOFF_MAGIC, IDD_MAX_MONITORS,
    IddFrameHeader, IddHandoffError, IddMonitorIdentity,
};
#[cfg(windows)]
pub use idd_mapping::{IddFrameBackend, IddFrameBrokerMapping, IddFrameSnapshot};
#[cfg(windows)]
pub use mf_encoder::{
    MediaFoundationH264Encoder, MediaFoundationH264EncoderConfig, MfEncoderError,
};
#[cfg(windows)]
pub use mf_pipeline::{MediaFoundationDisplayPipeline, MfDisplayPipelineError};
pub use nv12::{BgraToNv12Error, bgra8_to_nv12};
pub use source::{
    CaptureBackendPoll, CapturePumpOutcome, CaptureSize, CaptureStopOutcome, CapturedBgraFrame,
    MirrorCaptureBackend, MirrorCaptureMetrics, MirrorCaptureSource, MirrorCaptureState,
};
pub use sudovda_control::{
    IOCTL_SUDOVDA_ADD_VIRTUAL_DISPLAY, IOCTL_SUDOVDA_DRIVER_PING,
    IOCTL_SUDOVDA_GET_PROTOCOL_VERSION, IOCTL_SUDOVDA_REMOVE_VIRTUAL_DISPLAY,
    SUDOVDA_ADD_REQUEST_BYTES, SUDOVDA_ADD_RESPONSE_BYTES, SUDOVDA_PROTOCOL_RESPONSE_BYTES,
    SUDOVDA_REMOVE_REQUEST_BYTES, SudoVdaAddRequest, SudoVdaAddResponse, SudoVdaControlError,
    SudoVdaProtocolVersion, encode_sudovda_remove,
};
#[cfg(windows)]
pub use sudovda_device_control::{
    SudoVdaConnectedMonitor, SudoVdaDeviceControlError, SudoVdaMonitorLease,
};
