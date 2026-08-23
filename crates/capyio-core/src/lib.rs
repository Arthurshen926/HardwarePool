#![forbid(unsafe_code)]

//! Operating-system independent domain model for CapyIO.
//!
//! This crate intentionally contains no sockets, codecs, UI framework, platform SDK,
//! or driver integration. It defines stable capability semantics and deterministic
//! lifecycle transitions that can be shared by every host implementation.

pub mod audio;
pub mod capability;
pub mod error;
pub mod ids;
pub mod session;

pub use audio::{
    AudioBundleSpec, AudioCapabilitySpec, AudioFormat, AudioProcessingSupport, AudioQosMode,
    AudioSampleFormat, ChannelLayout,
};
pub use capability::{
    Availability, CapabilityDescriptor, CapabilityDetails, CapabilityKind, LocalRole,
    NodeDescriptor, NodeRole, OpaqueCapabilitySpec, PermissionRequirement, Platform, ProfileId,
    ProjectionKind, StreamRole,
};
pub use error::CoreError;
pub use ids::{BindingId, CapabilityId, MessageId, NodeId, ProjectionId, SessionId, StreamId};
pub use session::{BindingState, CapabilityBinding, Session, SessionPhase};
