use std::str::FromStr;

use hardwarepool_core::{MessageId, SessionId};
use prost::Message;

use crate::{PROTOCOL_MAJOR, PROTOCOL_MINOR, ProtocolError, v1};

/// Defensive bootstrap limit for one control-plane envelope.
pub const MAX_CONTROL_ENVELOPE_BYTES: usize = 1024 * 1024;

/// Encodes an already validated envelope.
#[must_use]
pub fn encode_envelope(envelope: &v1::Envelope) -> Vec<u8> {
    envelope.encode_to_vec()
}

/// Validates and encodes a control envelope.
pub fn encode_envelope_checked(envelope: &v1::Envelope) -> Result<Vec<u8>, ProtocolError> {
    validate_envelope(envelope)?;
    let bytes = encode_envelope(envelope);
    if bytes.len() > MAX_CONTROL_ENVELOPE_BYTES {
        return Err(ProtocolError::MessageTooLarge {
            limit: MAX_CONTROL_ENVELOPE_BYTES,
            actual: bytes.len(),
        });
    }
    Ok(bytes)
}

/// Decodes one bounded control message and validates required semantic fields.
pub fn decode_envelope(bytes: &[u8]) -> Result<v1::Envelope, ProtocolError> {
    if bytes.len() > MAX_CONTROL_ENVELOPE_BYTES {
        return Err(ProtocolError::MessageTooLarge {
            limit: MAX_CONTROL_ENVELOPE_BYTES,
            actual: bytes.len(),
        });
    }
    let envelope = v1::Envelope::decode(bytes)?;
    validate_envelope(&envelope)?;
    Ok(envelope)
}

/// Validates version, typed identifiers and the required payload.
pub fn validate_envelope(envelope: &v1::Envelope) -> Result<(), ProtocolError> {
    validate_envelope_version(envelope)?;
    MessageId::from_str(&envelope.message_id).map_err(|_| ProtocolError::InvalidId {
        field: "envelope.message_id",
        value: envelope.message_id.clone(),
    })?;
    if !envelope.session_id.is_empty() {
        SessionId::from_str(&envelope.session_id).map_err(|_| ProtocolError::InvalidId {
            field: "envelope.session_id",
            value: envelope.session_id.clone(),
        })?;
    }
    if envelope.payload.is_none() {
        return Err(ProtocolError::MissingField("envelope.payload"));
    }
    Ok(())
}

/// Rejects unsupported major protocol versions. Newer minor versions remain append-compatible.
pub fn validate_envelope_version(envelope: &v1::Envelope) -> Result<(), ProtocolError> {
    if envelope.protocol_major != PROTOCOL_MAJOR {
        return Err(ProtocolError::UnsupportedProtocolMajor {
            expected: PROTOCOL_MAJOR,
            actual: envelope.protocol_major,
        });
    }
    Ok(())
}

/// Creates a v1 envelope with a fresh typed message identifier.
#[must_use]
pub fn new_envelope(session_id: Option<SessionId>, payload: v1::envelope::Payload) -> v1::Envelope {
    v1::Envelope {
        protocol_major: PROTOCOL_MAJOR,
        protocol_minor: PROTOCOL_MINOR,
        message_id: MessageId::new().to_string(),
        session_id: session_id.map_or_else(String::new, |id| id.to_string()),
        payload: Some(payload),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error_payload() -> v1::envelope::Payload {
        v1::envelope::Payload::Error(v1::ErrorMessage {
            code: "test".to_owned(),
            category: "protocol".to_owned(),
            retryable: false,
            detail: String::new(),
            related_id: String::new(),
        })
    }

    #[test]
    fn missing_payload_is_rejected() {
        let mut envelope = new_envelope(None, error_payload());
        envelope.payload = None;
        assert!(matches!(
            validate_envelope(&envelope),
            Err(ProtocolError::MissingField("envelope.payload"))
        ));
    }

    #[test]
    fn invalid_message_id_is_rejected() {
        let mut envelope = new_envelope(None, error_payload());
        envelope.message_id = "not-a-uuid".to_owned();
        assert!(matches!(
            validate_envelope(&envelope),
            Err(ProtocolError::InvalidId {
                field: "envelope.message_id",
                ..
            })
        ));
    }

    #[test]
    fn oversized_input_is_rejected_before_decode() {
        let bytes = vec![0; MAX_CONTROL_ENVELOPE_BYTES + 1];
        assert!(matches!(
            decode_envelope(&bytes),
            Err(ProtocolError::MessageTooLarge { .. })
        ));
    }
}
