#![forbid(unsafe_code)]

//! Windows user-mode input Projection composition boundary.
//!
//! The current slices compose Runtime-owned fixed-epoch IMU/gamepad Routes
//! with bounded loopback DSU, VIIPER and optional owned USB/IP Projection
//! workers. They never start or configure VIIPER, deploy/restart a driver,
//! inject through a platform API or perform reverse haptics routing.

mod dsu_route;
mod usbip_win2;
mod vigem_x360;
mod viiper_ds4_route;
mod viiper_route;

pub use dsu_route::{DsuImuRouteController, DsuImuRouteStatus};
pub use usbip_win2::{
    MAX_USBIP_COMMAND_TIMEOUT, MAX_USBIP_OUTPUT_BYTES, PINNED_USBIP_WIN2_VERSION,
    USBIP_DS4_PRODUCT_ID, USBIP_DS4_VENDOR_ID, USBIP_XBOX360_PRODUCT_ID, USBIP_XBOX360_VENDOR_ID,
    UsbipBusId, UsbipControllerKind, UsbipExportedDevice, UsbipOwnedAttachment, UsbipWin2Client,
    UsbipWin2Config, UsbipWin2DeploymentVerified, UsbipWin2Error,
};
pub use vigem_x360::{VigemX360Companion, VigemX360Error, VigemX360SidecarConfig};
pub use viiper_ds4_route::{ViiperDs4RouteController, ViiperDs4RouteEpochs, ViiperDs4RouteStatus};
pub use viiper_route::{ViiperGamepadRouteController, ViiperGamepadRouteStatus};

pub const IMPLEMENTATION_STATUS: &str = "capy-gamepad-007a-vigem-xinput-companion";
