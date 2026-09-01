#![forbid(unsafe_code)]

//! Bounded local-LAN reference backend for CapyIO audio media packets.
//!
//! This crate is an explicitly insecure, `AdapterManaged` lab mechanism. It
//! executes only on media worker threads: platform audio callbacks hand off to
//! fixed-capacity queues and never call these sockets directly.

mod codec;
mod endpoint;
mod error;
mod lab;
mod reassembly;
mod supervisor;

use capyio_audio::{
    AudioTransportBackendContract, AudioTransportEncodingSupport, AudioTransportInteroperability,
    AudioTransportMediaAccess, AudioTransportMetadataFidelity, AudioTransportSecurity,
};

pub use codec::{
    MAX_NATIVE_LAN_DATAGRAM_BYTES, MAX_NATIVE_LAN_FRAGMENT_PAYLOAD_BYTES, MAX_NATIVE_LAN_FRAGMENTS,
    MAX_NATIVE_LAN_PACKET_PAYLOAD_BYTES, NATIVE_LAN_BACKEND_ID, NATIVE_LAN_HEADER_BYTES,
    NATIVE_LAN_WIRE_VERSION, NativeLanFragment, decode_native_lan_fragment,
    encode_native_lan_fragment, native_lan_fragment_count,
};
pub use endpoint::{
    NativeLanEndpointConfig, NativeLanEndpointMetrics, NativeLanReceiveOutcome,
    NativeLanUdpEndpoint,
};
pub use error::NativeLanError;
pub use lab::{
    NATIVE_MICROPHONE_LAB_ROUTE_ID, NATIVE_MICROPHONE_LAB_SESSION_ID,
    NATIVE_MICROPHONE_LAB_STREAM_EPOCH, NATIVE_MICROPHONE_LAB_STREAM_ID,
    NATIVE_SPEAKER_LAB_ROUTE_ID, NATIVE_SPEAKER_LAB_SESSION_ID, NATIVE_SPEAKER_LAB_STREAM_EPOCH,
    NATIVE_SPEAKER_LAB_STREAM_ID, microphone_lab_binding, speaker_lab_binding,
};
pub use reassembly::{
    MAX_NATIVE_LAN_INFLIGHT_PACKETS, NativeLanReassembler, NativeLanReassemblyOutcome,
    NativeLanReassemblyStats,
};
pub use supervisor::{
    NativeMicrophoneSupervisor, NativeSpeakerSupervisor, NativeSpeakerSupervisorError,
    NativeSpeakerSupervisorLimits, NativeSpeakerSupervisorStatus,
};

#[must_use]
pub const fn native_lan_backend_contract() -> AudioTransportBackendContract {
    AudioTransportBackendContract {
        backend_id: NATIVE_LAN_BACKEND_ID,
        interoperability: AudioTransportInteroperability::AdapterManaged,
        media_access: AudioTransportMediaAccess::FullPacket,
        encodings: AudioTransportEncodingSupport {
            pcm: true,
            opus: true,
        },
        metadata: AudioTransportMetadataFidelity::exact(),
        security: AudioTransportSecurity {
            peer_authenticated: false,
            confidentiality: false,
            integrity: false,
            replay_protection: false,
            downgrade_binding: false,
        },
    }
}
