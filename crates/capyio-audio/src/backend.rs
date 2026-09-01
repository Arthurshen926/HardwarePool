use crate::AudioDataError;

pub const MAX_AUDIO_TRANSPORT_BACKEND_ID_BYTES: usize = 96;

/// How faithfully one field of the common media contract crosses a backend.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioTransportFieldFidelity {
    /// The field reaches the peer with the same semantics.
    Exact,
    /// The backend carries only a documented subset or private equivalent.
    Partial,
    /// The backend does not carry the field.
    Absent,
    /// The field may exist inside an external implementation but CapyIO cannot observe it.
    Opaque,
}

/// Media visibility available to the CapyIO Adapter implementation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioTransportMediaAccess {
    /// The backend directly consumes and produces the complete common packet.
    FullPacket,
    /// The backend consumes decoded PCM payload but strips common packet metadata.
    PcmPayloadOnly,
    /// An external process owns media and exposes no packet/payload boundary to CapyIO.
    OpaqueProcess,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioTransportInteroperability {
    StandardPort,
    AdapterManaged,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AudioTransportEncodingSupport {
    pub pcm: bool,
    pub opus: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioTransportMetadataFidelity {
    pub session_route_binding: AudioTransportFieldFidelity,
    pub stream_identity: AudioTransportFieldFidelity,
    pub stream_epoch: AudioTransportFieldFidelity,
    pub sequence: AudioTransportFieldFidelity,
    pub source_timestamp: AudioTransportFieldFidelity,
    pub sample_timeline: AudioTransportFieldFidelity,
    pub discontinuity: AudioTransportFieldFidelity,
    pub selected_stream_spec: AudioTransportFieldFidelity,
    pub payload: AudioTransportFieldFidelity,
}

impl AudioTransportMetadataFidelity {
    #[must_use]
    pub const fn exact() -> Self {
        Self {
            session_route_binding: AudioTransportFieldFidelity::Exact,
            stream_identity: AudioTransportFieldFidelity::Exact,
            stream_epoch: AudioTransportFieldFidelity::Exact,
            sequence: AudioTransportFieldFidelity::Exact,
            source_timestamp: AudioTransportFieldFidelity::Exact,
            sample_timeline: AudioTransportFieldFidelity::Exact,
            discontinuity: AudioTransportFieldFidelity::Exact,
            selected_stream_spec: AudioTransportFieldFidelity::Exact,
            payload: AudioTransportFieldFidelity::Exact,
        }
    }

    #[must_use]
    pub const fn is_exact(self) -> bool {
        matches!(
            self,
            Self {
                session_route_binding: AudioTransportFieldFidelity::Exact,
                stream_identity: AudioTransportFieldFidelity::Exact,
                stream_epoch: AudioTransportFieldFidelity::Exact,
                sequence: AudioTransportFieldFidelity::Exact,
                source_timestamp: AudioTransportFieldFidelity::Exact,
                sample_timeline: AudioTransportFieldFidelity::Exact,
                discontinuity: AudioTransportFieldFidelity::Exact,
                selected_stream_spec: AudioTransportFieldFidelity::Exact,
                payload: AudioTransportFieldFidelity::Exact,
            }
        )
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AudioTransportSecurity {
    pub peer_authenticated: bool,
    pub confidentiality: bool,
    pub integrity: bool,
    pub replay_protection: bool,
    pub downgrade_binding: bool,
}

impl AudioTransportSecurity {
    #[must_use]
    pub const fn production_baseline() -> Self {
        Self {
            peer_authenticated: true,
            confidentiality: true,
            integrity: true,
            replay_protection: true,
            downgrade_binding: true,
        }
    }

    #[must_use]
    pub const fn meets_production_baseline(self) -> bool {
        self.peer_authenticated
            && self.confidentiality
            && self.integrity
            && self.replay_protection
            && self.downgrade_binding
    }
}

/// Machine-checkable declaration for one concrete audio media backend.
///
/// This describes observable semantics; it does not execute a codec or transport.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioTransportBackendContract {
    pub backend_id: &'static str,
    pub interoperability: AudioTransportInteroperability,
    pub media_access: AudioTransportMediaAccess,
    pub encodings: AudioTransportEncodingSupport,
    pub metadata: AudioTransportMetadataFidelity,
    pub security: AudioTransportSecurity,
}

impl AudioTransportBackendContract {
    pub fn validate(self) -> Result<Self, AudioDataError> {
        if self.backend_id.is_empty()
            || self.backend_id.len() > MAX_AUDIO_TRANSPORT_BACKEND_ID_BYTES
            || !self.backend_id.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'-' | b'_' | b'/')
            })
        {
            return Err(AudioDataError::InvalidTransportBackendContract(
                "backend ID must be canonical lowercase ASCII and bounded".to_owned(),
            ));
        }
        if !self.encodings.pcm && !self.encodings.opus {
            return Err(AudioDataError::InvalidTransportBackendContract(
                "backend must declare at least one supported encoding".to_owned(),
            ));
        }

        match self.media_access {
            AudioTransportMediaAccess::FullPacket if !self.metadata.is_exact() => {
                return Err(AudioDataError::InvalidTransportBackendContract(
                    "full-packet access requires exact common metadata fidelity".to_owned(),
                ));
            }
            AudioTransportMediaAccess::PcmPayloadOnly
                if !self.encodings.pcm
                    || self.encodings.opus
                    || self.metadata.payload != AudioTransportFieldFidelity::Exact =>
            {
                return Err(AudioDataError::InvalidTransportBackendContract(
                    "PCM-payload access requires PCM-only support and exact payload fidelity"
                        .to_owned(),
                ));
            }
            AudioTransportMediaAccess::OpaqueProcess
                if self.metadata.payload != AudioTransportFieldFidelity::Opaque =>
            {
                return Err(AudioDataError::InvalidTransportBackendContract(
                    "opaque-process access requires opaque payload fidelity".to_owned(),
                ));
            }
            _ => {}
        }

        if self.interoperability == AudioTransportInteroperability::StandardPort
            && (self.media_access != AudioTransportMediaAccess::FullPacket
                || !self.metadata.is_exact())
        {
            return Err(AudioDataError::InvalidTransportBackendContract(
                "StandardPort audio requires full-packet access and exact metadata fidelity"
                    .to_owned(),
            ));
        }

        if (self.security.confidentiality
            || self.security.replay_protection
            || self.security.downgrade_binding)
            && (!self.security.peer_authenticated || !self.security.integrity)
        {
            return Err(AudioDataError::InvalidTransportBackendContract(
                "confidentiality/replay/downgrade claims require authenticated integrity"
                    .to_owned(),
            ));
        }
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn native_contract() -> AudioTransportBackendContract {
        AudioTransportBackendContract {
            backend_id: "dev.capyio.audio.native/1",
            interoperability: AudioTransportInteroperability::StandardPort,
            media_access: AudioTransportMediaAccess::FullPacket,
            encodings: AudioTransportEncodingSupport {
                pcm: true,
                opus: true,
            },
            metadata: AudioTransportMetadataFidelity::exact(),
            security: AudioTransportSecurity::production_baseline(),
        }
    }

    #[test]
    fn exact_native_contract_is_valid_and_production_capable() {
        let contract = native_contract().validate().expect("contract");
        assert!(contract.metadata.is_exact());
        assert!(contract.security.meets_production_baseline());
    }

    #[test]
    fn standard_port_cannot_hide_or_strip_common_packet_fields() {
        let mut invalid = native_contract();
        invalid.metadata.sequence = AudioTransportFieldFidelity::Absent;
        assert!(matches!(
            invalid.validate(),
            Err(AudioDataError::InvalidTransportBackendContract(_))
        ));

        let mut invalid = native_contract();
        invalid.media_access = AudioTransportMediaAccess::OpaqueProcess;
        invalid.metadata.payload = AudioTransportFieldFidelity::Opaque;
        assert!(matches!(
            invalid.validate(),
            Err(AudioDataError::InvalidTransportBackendContract(_))
        ));
    }

    #[test]
    fn malformed_identity_encoding_and_security_claims_fail_closed() {
        let mut invalid = native_contract();
        invalid.backend_id = "Not Canonical";
        assert!(invalid.validate().is_err());

        let mut invalid = native_contract();
        invalid.encodings = AudioTransportEncodingSupport::default();
        assert!(invalid.validate().is_err());

        let mut invalid = native_contract();
        invalid.security.peer_authenticated = false;
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn compatibility_access_modes_cannot_overstate_payload_visibility() {
        let mut metadata = AudioTransportMetadataFidelity::exact();
        metadata.session_route_binding = AudioTransportFieldFidelity::Absent;
        let payload_only = AudioTransportBackendContract {
            backend_id: "dev.capyio.compat.payload/1",
            interoperability: AudioTransportInteroperability::AdapterManaged,
            media_access: AudioTransportMediaAccess::PcmPayloadOnly,
            encodings: AudioTransportEncodingSupport {
                pcm: true,
                opus: false,
            },
            metadata,
            security: AudioTransportSecurity::default(),
        };
        payload_only.validate().expect("payload-only contract");

        let mut invalid = payload_only;
        invalid.encodings.opus = true;
        assert!(invalid.validate().is_err());

        let mut invalid = payload_only;
        invalid.media_access = AudioTransportMediaAccess::OpaqueProcess;
        assert!(invalid.validate().is_err());

        invalid.metadata.payload = AudioTransportFieldFidelity::Opaque;
        invalid.validate().expect("honest opaque contract");
    }
}
