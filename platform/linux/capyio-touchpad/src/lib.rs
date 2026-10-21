#![forbid(unsafe_code)]
//! Owned Type-B contact projection, never relative mouse gesture emulation.
//! Native creation requires an admitted authenticated receiver and explicit grant.
mod projection;
pub use projection::*;
mod owner;
mod session;
#[cfg(all(target_os = "linux", feature = "native-uinput"))]
mod transport;
pub use owner::RuntimeTouchpadWorker;
pub use session::{
    LinuxTouchpadSession, LinuxTouchpadSinkFactory, TouchpadEventTransport, TouchpadTransportStop,
};
