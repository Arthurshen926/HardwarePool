use capyio_control_credentials::ControlCredentialBundle;
use capyio_control_tls::{
    CertificateFingerprint, ControlTlsClientIdentity, ControlTlsServerIdentity,
};
use capyio_core::*;
use capyio_protocol::SECURE_AUDIO_STEREO_FORMAT;
use rcgen::{
    BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose,
};
pub(crate) fn nodes() -> (NodeDescriptor, NodeDescriptor, PortRef, PortRef) {
    let mut android = capyio_testkit::android_node();
    let mut linux = capyio_testkit::windows_node();
    linux.platform = Platform::Linux;
    for node in [&mut android, &mut linux] {
        for adapter in node.adapter_instances.values_mut() {
            adapter.state = AdapterState::Ready;
            adapter
                .supported_route_modes
                .insert(RouteBackend::AdapterManaged);
        }
        for capability in node.capabilities.values_mut() {
            capability.availability = Availability::Available;
            for port in capability.ports.values_mut() {
                port.availability = Availability::Available;
                port.interoperability_mode = InteroperabilityMode::AdapterManaged;
                if port.profile == ProfileId::audio_frames_v1() {
                    port.formats = vec![FormatDescriptor::new(SECURE_AUDIO_STEREO_FORMAT)];
                    port.qos_modes = std::collections::BTreeSet::from([QosMode::Interactive]);
                }
            }
        }
    }
    let capability = linux
        .capabilities
        .values()
        .find(|capability| {
            capability.class == CapabilityClass::Custom("system_audio_capture".into())
        })
        .unwrap();
    let source = PortRef {
        node_id: linux.id,
        capability_id: capability.id,
        port_id: *capability.ports.keys().next().unwrap(),
    };
    let capability = android
        .capabilities
        .values()
        .find(|capability| capability.class == CapabilityClass::Speaker)
        .unwrap();
    let sink = PortRef {
        node_id: android.id,
        capability_id: capability.id,
        port_id: *capability.ports.keys().next().unwrap(),
    };
    (android, linux, source, sink)
}
pub(crate) fn credentials() -> (
    ControlTlsServerIdentity,
    ControlTlsClientIdentity,
    ControlTlsServerIdentity,
    ControlTlsClientIdentity,
) {
    let mut ca = CertificateParams::new(Vec::<String>::new()).unwrap();
    ca.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyCertSign,
    ];
    let key = KeyPair::generate().unwrap();
    let root = ca.self_signed(&key).unwrap().der().to_vec();
    let issuer = Issuer::new(ca, key);
    let leaf = || {
        let mut params = CertificateParams::new(vec!["localhost".into()]).unwrap();
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![
            ExtendedKeyUsagePurpose::ClientAuth,
            ExtendedKeyUsagePurpose::ServerAuth,
        ];
        let key = KeyPair::generate().unwrap();
        let cert = params.signed_by(&key, &issuer).unwrap().der().to_vec();
        let pin = CertificateFingerprint::from_certificate_der(&cert).unwrap();
        (
            ControlCredentialBundle::new(
                vec![cert, root.clone()],
                key.serialize_der(),
                root.clone(),
            )
            .unwrap(),
            pin,
        )
    };
    let (android, android_pin) = leaf();
    let (linux, linux_pin) = leaf();
    let android = ControlTlsServerIdentity::from_bundle(linux_pin, android);
    let linux = ControlTlsServerIdentity::from_bundle(android_pin, linux);
    let client = |server: &ControlTlsServerIdentity| ControlTlsClientIdentity {
        expected_peer_certificate: server.expected_peer_certificate,
        certificate_chain_der: server.certificate_chain_der.clone(),
        private_key_der: server.private_key_der.clone(),
        server_ca_der: server.client_ca_der.clone(),
    };
    let android_client = client(&android);
    let linux_client = client(&linux);
    (android, android_client, linux, linux_client)
}
