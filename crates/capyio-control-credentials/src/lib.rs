#![forbid(unsafe_code)]

//! In-memory and encoded representation of an already-issued control identity.
//! This crate does not issue certificates, enroll peers or persist plaintext.

use thiserror::Error;

pub const MAX_CONTROL_CREDENTIAL_BUNDLE_BYTES: usize = 65_536;
pub const MAX_CONTROL_CERTIFICATE_CHAIN_LENGTH: usize = 4;
const MAGIC: [u8; 8] = *b"CPYCRD01";
const HEADER_BYTES: usize = 12;

pub struct ControlCredentialBundle {
    certificate_chain_der: Vec<Vec<u8>>,
    private_key_der: Vec<u8>,
    peer_ca_der: Vec<u8>,
}

impl ControlCredentialBundle {
    pub fn new(
        certificate_chain_der: Vec<Vec<u8>>,
        private_key_der: Vec<u8>,
        peer_ca_der: Vec<u8>,
    ) -> Result<Self, ControlCredentialError> {
        let bundle = Self {
            certificate_chain_der,
            private_key_der,
            peer_ca_der,
        };
        bundle.validate()?;
        Ok(bundle)
    }

    pub fn encode(&self) -> Result<Vec<u8>, ControlCredentialError> {
        self.validate()?;
        let mut encoded = Vec::with_capacity(self.encoded_len()?);
        encoded.extend_from_slice(&MAGIC);
        encoded.push(
            u8::try_from(self.certificate_chain_der.len())
                .map_err(|_| ControlCredentialError::InvalidCertificateChain)?,
        );
        encoded.extend_from_slice(&[0; 3]);
        for certificate in &self.certificate_chain_der {
            append_component(&mut encoded, certificate)?;
        }
        append_component(&mut encoded, &self.private_key_der)?;
        append_component(&mut encoded, &self.peer_ca_der)?;
        if encoded.len() > MAX_CONTROL_CREDENTIAL_BUNDLE_BYTES {
            return Err(ControlCredentialError::BundleTooLarge);
        }
        Ok(encoded)
    }

    pub fn decode(encoded: &[u8]) -> Result<Self, ControlCredentialError> {
        if encoded.len() > MAX_CONTROL_CREDENTIAL_BUNDLE_BYTES {
            return Err(ControlCredentialError::BundleTooLarge);
        }
        if encoded.len() < HEADER_BYTES || encoded[..8] != MAGIC || encoded[9..12] != [0; 3] {
            return Err(ControlCredentialError::InvalidEncoding);
        }
        let chain_length = usize::from(encoded[8]);
        if !(1..=MAX_CONTROL_CERTIFICATE_CHAIN_LENGTH).contains(&chain_length) {
            return Err(ControlCredentialError::InvalidCertificateChain);
        }
        let mut cursor = HEADER_BYTES;
        // Build directly into the guarded value so any component parsed before
        // a later failure is cleared by Drop rather than released as plaintext.
        let mut bundle = Self {
            certificate_chain_der: Vec::with_capacity(chain_length),
            private_key_der: Vec::new(),
            peer_ca_der: Vec::new(),
        };
        for _ in 0..chain_length {
            bundle
                .certificate_chain_der
                .push(read_component(encoded, &mut cursor)?);
        }
        bundle.private_key_der = read_component(encoded, &mut cursor)?;
        bundle.peer_ca_der = read_component(encoded, &mut cursor)?;
        if cursor != encoded.len() {
            return Err(ControlCredentialError::InvalidEncoding);
        }
        bundle.validate()?;
        Ok(bundle)
    }

    pub fn into_parts(mut self) -> (Vec<Vec<u8>>, Vec<u8>, Vec<u8>) {
        (
            std::mem::take(&mut self.certificate_chain_der),
            std::mem::take(&mut self.private_key_der),
            std::mem::take(&mut self.peer_ca_der),
        )
    }

    fn validate(&self) -> Result<(), ControlCredentialError> {
        if !(1..=MAX_CONTROL_CERTIFICATE_CHAIN_LENGTH).contains(&self.certificate_chain_der.len())
            || self.certificate_chain_der.iter().any(Vec::is_empty)
        {
            return Err(ControlCredentialError::InvalidCertificateChain);
        }
        if self.private_key_der.is_empty() || self.peer_ca_der.is_empty() {
            return Err(ControlCredentialError::MissingSecretComponent);
        }
        self.encoded_len()?;
        Ok(())
    }

    fn encoded_len(&self) -> Result<usize, ControlCredentialError> {
        let mut length = HEADER_BYTES;
        for component in self
            .certificate_chain_der
            .iter()
            .chain([&self.private_key_der, &self.peer_ca_der])
        {
            let _ = u32::try_from(component.len())
                .map_err(|_| ControlCredentialError::BundleTooLarge)?;
            length = length
                .checked_add(4)
                .and_then(|value| value.checked_add(component.len()))
                .ok_or(ControlCredentialError::BundleTooLarge)?;
        }
        if length > MAX_CONTROL_CREDENTIAL_BUNDLE_BYTES {
            Err(ControlCredentialError::BundleTooLarge)
        } else {
            Ok(length)
        }
    }
}

impl Drop for ControlCredentialBundle {
    fn drop(&mut self) {
        for certificate in &mut self.certificate_chain_der {
            certificate.fill(0);
        }
        self.private_key_der.fill(0);
        self.peer_ca_der.fill(0);
    }
}

fn append_component(encoded: &mut Vec<u8>, component: &[u8]) -> Result<(), ControlCredentialError> {
    let length =
        u32::try_from(component.len()).map_err(|_| ControlCredentialError::BundleTooLarge)?;
    encoded.extend_from_slice(&length.to_be_bytes());
    encoded.extend_from_slice(component);
    Ok(())
}

fn read_component(encoded: &[u8], cursor: &mut usize) -> Result<Vec<u8>, ControlCredentialError> {
    let length_end = cursor
        .checked_add(4)
        .filter(|end| *end <= encoded.len())
        .ok_or(ControlCredentialError::InvalidEncoding)?;
    let length = usize::try_from(u32::from_be_bytes(
        encoded[*cursor..length_end]
            .try_into()
            .map_err(|_| ControlCredentialError::InvalidEncoding)?,
    ))
    .map_err(|_| ControlCredentialError::InvalidEncoding)?;
    let component_end = length_end
        .checked_add(length)
        .filter(|end| *end <= encoded.len())
        .ok_or(ControlCredentialError::InvalidEncoding)?;
    if length == 0 {
        return Err(ControlCredentialError::MissingSecretComponent);
    }
    *cursor = component_end;
    Ok(encoded[length_end..component_end].to_vec())
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ControlCredentialError {
    #[error("control credential certificate chain is invalid")]
    InvalidCertificateChain,
    #[error("control credential secret component is missing")]
    MissingSecretComponent,
    #[error("control credential encoding is invalid")]
    InvalidEncoding,
    #[error("control credential bundle exceeds its bound")]
    BundleTooLarge,
}
