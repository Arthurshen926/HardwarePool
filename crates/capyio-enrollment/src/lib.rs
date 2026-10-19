#![forbid(unsafe_code)]
//! Explicitly opened enrollment. No discovery observation establishes trust.

mod discovery;
mod exchange;
mod identity;
mod manager;

pub use discovery::EnrollmentCandidate;
pub use exchange::{EnrollmentChannel, authenticate};
pub use identity::{LocalIdentity, PeerHello};
pub use manager::{EnrollmentManager, EnrollmentStatus};
