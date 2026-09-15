use std::{error::Error, fmt};

use capyio_core::{AdapterInstanceId, CapabilityId, NodeId, PortId};
use sha1::{Digest, Sha1};
use uuid::Uuid;

const MAX_PLATFORM_KEY_BYTES: usize = 96;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AndroidCatalogIdentityError {
    InvalidPlatformKey { field: &'static str },
}

impl fmt::Display for AndroidCatalogIdentityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPlatformKey { field } => {
                write!(formatter, "{field} must be canonical lowercase ASCII")
            }
        }
    }
}

impl Error for AndroidCatalogIdentityError {}

pub fn android_adapter_id(
    node_id: NodeId,
    capability_key: &str,
) -> Result<AdapterInstanceId, AndroidCatalogIdentityError> {
    platform_key(capability_key, "capability key")?;
    Ok(AdapterInstanceId::from_uuid(named_id(
        node_id,
        &format!("adapter/{capability_key}"),
    )))
}

pub fn android_capability_id(
    node_id: NodeId,
    capability_key: &str,
) -> Result<CapabilityId, AndroidCatalogIdentityError> {
    platform_key(capability_key, "capability key")?;
    Ok(CapabilityId::from_uuid(named_id(
        node_id,
        &format!("capability/{capability_key}"),
    )))
}

pub fn android_port_id(
    node_id: NodeId,
    capability_key: &str,
    port_key: &str,
) -> Result<PortId, AndroidCatalogIdentityError> {
    platform_key(capability_key, "capability key")?;
    platform_key(port_key, "port key")?;
    Ok(PortId::from_uuid(named_id(
        node_id,
        &format!("port/{capability_key}/{port_key}"),
    )))
}

fn named_id(node_id: NodeId, name: &str) -> Uuid {
    let mut digest = Sha1::new();
    digest.update(node_id.as_uuid().as_bytes());
    digest.update(name.as_bytes());
    let hash = digest.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&hash[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn platform_key(value: &str, field: &'static str) -> Result<(), AndroidCatalogIdentityError> {
    if value.is_empty()
        || value.len() > MAX_PLATFORM_KEY_BYTES
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
    {
        return Err(AndroidCatalogIdentityError::InvalidPlatformKey { field });
    }
    Ok(())
}
