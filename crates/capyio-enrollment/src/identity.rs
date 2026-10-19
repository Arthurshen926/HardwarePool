use capyio_control_credentials::ControlCredentialBundle;
use capyio_core::NodeId;
use capyio_pairing::CertificateFingerprint;
use rcgen::{
    BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose,
};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeerHello {
    pub version: u8,
    pub node_id: NodeId,
    pub name: String,
    pub nonce: [u8; 32],
    pub leaf: Vec<u8>,
    pub ca: Vec<u8>,
}

impl PeerHello {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 3
            || self.node_id.as_uuid().is_nil()
            || self.name.trim().is_empty()
            || self.name.len() > 128
            || self.name.chars().any(char::is_control)
            || self.leaf.is_empty()
            || self.leaf.len() > 4096
            || self.ca.is_empty()
            || self.ca.len() > 4096
        {
            return Err("invalid enrollment identity".into());
        }
        Ok(())
    }
    pub fn server_name(&self) -> String {
        format!("node-{}.capyio.local", self.node_id)
    }
    pub fn fingerprint(&self) -> Result<CertificateFingerprint, String> {
        CertificateFingerprint::from_certificate_der(&self.leaf)
            .ok_or_else(|| "invalid enrollment certificate".into())
    }
}

pub struct LocalIdentity {
    pub hello: PeerHello,
    key: Vec<u8>,
}

impl LocalIdentity {
    pub fn generate(node_id: NodeId, name: String) -> Result<Self, String> {
        let mut ca_params =
            CertificateParams::new(Vec::<String>::new()).map_err(|e| e.to_string())?;
        ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        ca_params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::DigitalSignature,
        ];
        let ca_key = KeyPair::generate().map_err(|e| e.to_string())?;
        let ca = ca_params
            .self_signed(&ca_key)
            .map_err(|e| e.to_string())?
            .der()
            .to_vec();
        let issuer = Issuer::new(ca_params, ca_key);
        let mut params = CertificateParams::new(vec![format!("node-{node_id}.capyio.local")])
            .map_err(|e| e.to_string())?;
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![
            ExtendedKeyUsagePurpose::ClientAuth,
            ExtendedKeyUsagePurpose::ServerAuth,
        ];
        let key = KeyPair::generate().map_err(|e| e.to_string())?;
        let leaf = params
            .signed_by(&key, &issuer)
            .map_err(|e| e.to_string())?
            .der()
            .to_vec();
        let mut nonce = [0; 32];
        SystemRandom::new()
            .fill(&mut nonce)
            .map_err(|_| "OS random source failed")?;
        let hello = PeerHello {
            version: 3,
            node_id,
            name,
            nonce,
            leaf,
            ca,
        };
        hello.validate()?;
        Ok(Self {
            hello,
            key: key.serialize_der(),
        })
    }

    pub fn credential_for(&self, peer: &PeerHello) -> Result<ControlCredentialBundle, String> {
        peer.validate()?;
        ControlCredentialBundle::new(
            vec![self.hello.leaf.clone(), self.hello.ca.clone()],
            self.key.clone(),
            peer.ca.clone(),
        )
        .map_err(|e| e.to_string())
    }
}

impl Drop for LocalIdentity {
    fn drop(&mut self) {
        self.key.fill(0);
    }
}
