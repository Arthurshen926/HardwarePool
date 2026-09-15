use std::str::FromStr;

use capyio_core::{MessageId, NodeDescriptor, SessionId};
use prost::Message;

use crate::{PROTOCOL_MAJOR, PROTOCOL_MINOR, ProtocolError, v1};

/// Defensive bootstrap limit for one control-plane envelope.
pub const MAX_CONTROL_ENVELOPE_BYTES: usize = 1024 * 1024;
/// Maximum Adapter descriptors carried by one Node catalog.
pub const MAX_CATALOG_ADAPTERS: usize = 64;
/// Maximum Capability descriptors carried by one Node catalog.
pub const MAX_CATALOG_CAPABILITIES: usize = 256;
/// Maximum Ports carried by one Capability descriptor.
pub const MAX_CATALOG_PORTS_PER_CAPABILITY: usize = 32;
/// Maximum format alternatives carried by one Port descriptor.
pub const MAX_CATALOG_FORMATS_PER_PORT: usize = 32;
/// Maximum metadata or format-parameter entries in one descriptor.
pub const MAX_CATALOG_METADATA_ENTRIES: usize = 64;
/// Maximum protocol versions advertised by one Node.
pub const MAX_CATALOG_PROTOCOL_VERSIONS: usize = 16;
/// Maximum UTF-8 byte length of one catalog string field.
pub const MAX_CATALOG_STRING_BYTES: usize = 2 * 1024;
const MAX_CATALOG_ENUM_LIST_ENTRIES: usize = 16;

/// A structurally and semantically validated catalog control message. Peer
/// authentication and replay admission remain transport/host responsibilities.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedCatalogSnapshot {
    pub message_id: MessageId,
    pub session_id: SessionId,
    pub node: NodeDescriptor,
}

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

/// Converts one complete Core Node catalog into a bounded, Session-bound
/// CatalogSnapshot envelope. The caller still owns Session authorization,
/// transport authentication and delivery.
pub fn encode_catalog_snapshot(
    session_id: SessionId,
    node: &NodeDescriptor,
) -> Result<Vec<u8>, ProtocolError> {
    let node = v1::NodeDescriptor::try_from(node)?;
    validate_catalog_bounds(&node)?;
    let envelope = new_envelope(
        Some(session_id),
        v1::envelope::Payload::CatalogSnapshot(v1::CatalogSnapshot { node: Some(node) }),
    );
    encode_envelope_checked(&envelope)
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

/// Decodes one complete CatalogSnapshot envelope and converts its public wire
/// descriptor into a validated Core value ready for host-side Session admission.
pub fn decode_catalog_snapshot(bytes: &[u8]) -> Result<ValidatedCatalogSnapshot, ProtocolError> {
    let envelope = decode_envelope(bytes)?;
    let message_id =
        MessageId::from_str(&envelope.message_id).map_err(|_| ProtocolError::InvalidId {
            field: "envelope.message_id",
            value: envelope.message_id.clone(),
        })?;
    if envelope.session_id.is_empty() {
        return Err(ProtocolError::MissingField("envelope.session_id"));
    }
    let session_id =
        SessionId::from_str(&envelope.session_id).map_err(|_| ProtocolError::InvalidId {
            field: "envelope.session_id",
            value: envelope.session_id.clone(),
        })?;
    let snapshot = match envelope.payload {
        Some(v1::envelope::Payload::CatalogSnapshot(snapshot)) => snapshot,
        _ => {
            return Err(ProtocolError::UnexpectedPayload {
                expected: "catalog_snapshot",
            });
        }
    };
    let node = snapshot
        .node
        .ok_or(ProtocolError::MissingField("catalog_snapshot.node"))?;
    validate_catalog_bounds(&node)?;
    let node = node.try_into()?;
    Ok(ValidatedCatalogSnapshot {
        message_id,
        session_id,
        node,
    })
}

fn validate_catalog_bounds(node: &v1::NodeDescriptor) -> Result<(), ProtocolError> {
    check_count(
        "node.protocol_versions",
        node.protocol_versions.len(),
        MAX_CATALOG_PROTOCOL_VERSIONS,
    )?;
    check_count(
        "node.adapter_instances",
        node.adapter_instances.len(),
        MAX_CATALOG_ADAPTERS,
    )?;
    check_count(
        "node.capabilities",
        node.capabilities.len(),
        MAX_CATALOG_CAPABILITIES,
    )?;
    check_string("node.id", &node.id)?;
    check_string("node.display_name", &node.display_name)?;
    check_string("node.platform_detail", &node.platform_detail)?;
    check_string("node.platform_version", &node.platform_version)?;
    check_string("node.runtime_version", &node.runtime_version)?;

    for adapter in &node.adapter_instances {
        check_count(
            "adapter.owned_capability_ids",
            adapter.owned_capability_ids.len(),
            MAX_CATALOG_CAPABILITIES,
        )?;
        check_count(
            "adapter.supported_route_modes",
            adapter.supported_route_modes.len(),
            MAX_CATALOG_ENUM_LIST_ENTRIES,
        )?;
        check_string("adapter.id", &adapter.id)?;
        check_string("adapter.adapter_type", &adapter.adapter_type)?;
        check_string("adapter.display_name", &adapter.display_name)?;
        check_string("adapter.version", &adapter.version)?;
        for capability_id in &adapter.owned_capability_ids {
            check_string("adapter.owned_capability_ids", capability_id)?;
        }
    }

    for capability in &node.capabilities {
        check_count(
            "capability.metadata",
            capability.metadata.len(),
            MAX_CATALOG_METADATA_ENTRIES,
        )?;
        check_count(
            "capability.ports",
            capability.ports.len(),
            MAX_CATALOG_PORTS_PER_CAPABILITY,
        )?;
        check_string("capability.id", &capability.id)?;
        check_string("capability.display_name", &capability.display_name)?;
        check_string(
            "capability.adapter_instance_id",
            &capability.adapter_instance_id,
        )?;
        check_string("capability.custom_class", &capability.custom_class)?;
        for (key, value) in &capability.metadata {
            check_string("capability.metadata key", key)?;
            check_string("capability.metadata value", value)?;
        }
        for port in &capability.ports {
            check_count(
                "port.formats",
                port.formats.len(),
                MAX_CATALOG_FORMATS_PER_PORT,
            )?;
            check_count(
                "port.qos_modes",
                port.qos_modes.len(),
                MAX_CATALOG_ENUM_LIST_ENTRIES,
            )?;
            check_string("port.id", &port.id)?;
            check_string("port.capability_id", &port.capability_id)?;
            check_string("port.display_name", &port.display_name)?;
            if let Some(profile) = &port.profile {
                check_string("port.profile.name", &profile.name)?;
            }
            if let Some(value) = &port.schema_id {
                check_string("port.schema_id", value)?;
            }
            if let Some(value) = &port.clock_domain {
                check_string("port.clock_domain", value)?;
            }
            for format in &port.formats {
                check_string("format.id", &format.id)?;
                check_count(
                    "format.parameters",
                    format.parameters.len(),
                    MAX_CATALOG_METADATA_ENTRIES,
                )?;
                for (key, value) in &format.parameters {
                    check_string("format.parameters key", key)?;
                    check_string("format.parameters value", value)?;
                }
            }
        }
    }
    Ok(())
}

fn check_count(field: &'static str, actual: usize, limit: usize) -> Result<(), ProtocolError> {
    if actual > limit {
        return Err(ProtocolError::CatalogCountLimit {
            field,
            limit,
            actual,
        });
    }
    Ok(())
}

fn check_string(field: &'static str, value: &str) -> Result<(), ProtocolError> {
    let actual = value.len();
    if actual > MAX_CATALOG_STRING_BYTES {
        return Err(ProtocolError::CatalogStringLimit {
            field,
            limit: MAX_CATALOG_STRING_BYTES,
            actual,
        });
    }
    Ok(())
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
    if let Some(v1::envelope::Payload::CatalogSnapshot(snapshot)) = &envelope.payload
        && let Some(node) = &snapshot.node
    {
        validate_catalog_bounds(node)?;
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
