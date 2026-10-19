use std::{
    collections::BTreeSet,
    io::Write,
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

use capyio_android_jni::{
    AndroidCatalogRuntimeV1, AndroidControlListenerError, AndroidControlListenerV1,
    exchange_accepted_android_catalog,
};
use capyio_control_credentials::ControlCredentialBundle;
use capyio_control_link::CatalogControlLink;
use capyio_control_tls::{
    AuthenticatedControlClientChannel, AuthenticatedControlServer, ControlTlsClientIdentity,
    ControlTlsEndpoint, ControlTlsNodeBinding, ControlTlsServerIdentity,
};
use capyio_core::{CapabilityId, NodeDescriptor, PortDirection, SessionId, SessionState};
use capyio_pairing::{
    CertificateFingerprint, CredentialHandle, PairedPeerDirectory, PairedPeerRecord,
};
use capyio_runtime::NodeRuntime;
use capyio_testkit::windows_node;
use rcgen::{
    BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose,
};

const ANDROID_FIXTURE: &str = include_str!("../../fixtures/android-core-catalog-v1.json");

#[test]
fn listener_requires_exactly_one_active_paired_peer() {
    let local = capyio_core::NodeId::new();
    let empty = PairedPeerDirectory::new(local).unwrap();
    assert!(matches!(
        AndroidControlListenerV1::from_listener(
            TcpListener::bind("127.0.0.1:0").unwrap(),
            &local.to_string(),
            &empty.encode_json().unwrap(),
            &[],
            Duration::from_secs(1),
        ),
        Err(AndroidControlListenerError::NoActivePeer)
    ));

    let mut multiple = PairedPeerDirectory::new(local).unwrap();
    for (index, byte) in [1_u8, 2].into_iter().enumerate() {
        multiple
            .add(
                PairedPeerRecord::new(
                    capyio_core::NodeId::new(),
                    format!("Peer {index}"),
                    format!("peer-{index}.capyio.test"),
                    CredentialHandle::new(format!(
                        "android-keystore:{}",
                        format!("{byte:x}").repeat(64)
                    ))
                    .unwrap(),
                    CertificateFingerprint::new([byte; 32]).unwrap(),
                    1,
                )
                .unwrap(),
            )
            .unwrap();
    }
    assert!(matches!(
        AndroidControlListenerV1::from_listener(
            TcpListener::bind("127.0.0.1:0").unwrap(),
            &local.to_string(),
            &multiple.encode_json().unwrap(),
            &[],
            Duration::from_secs(1),
        ),
        Err(AndroidControlListenerError::MultipleActivePeers)
    ));
}

#[test]
fn accepted_authenticated_peer_exchanges_catalogs_and_remains_owned() {
    let mut android = AndroidCatalogRuntimeV1::new(ANDROID_FIXTURE).unwrap();
    let android_node = android.snapshot().local_node;
    let windows_node = windows_node();
    let expected = ControlTlsNodeBinding::new(android_node.id, windows_node.id).unwrap();
    let (server_identity, client_identity, server_bundle) = identities();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut directory = PairedPeerDirectory::new(android_node.id).unwrap();
    directory
        .add(
            PairedPeerRecord::new(
                windows_node.id,
                windows_node.display_name.clone(),
                "localhost",
                CredentialHandle::new(format!("android-keystore:{}", "a".repeat(64))).unwrap(),
                server_identity.expected_peer_certificate,
                1,
            )
            .unwrap(),
        )
        .unwrap();
    let listener = AndroidControlListenerV1::from_listener(
        listener,
        &android_node.id.to_string(),
        &directory.encode_json().unwrap(),
        &server_bundle.encode().unwrap(),
        Duration::from_secs(2),
    )
    .unwrap();
    let endpoint = listener.local_addr().unwrap();

    // A closed probe and malformed TLS must consume only their own sockets;
    // the exact same listener must still accept the paired connection below.
    for bytes in [&[][..], &[0_u8, 1, 2, 3, 4][..]] {
        let mut probe = TcpStream::connect(endpoint).unwrap();
        probe.write_all(bytes).unwrap();
        drop(probe);
        assert!(listener.try_accept_and_exchange(&mut android).is_err());
        assert!(android.snapshot().sessions.is_empty());
    }

    let client_node = windows_node.clone();
    let android_display_name = android_node.display_name.clone();
    let worker = thread::spawn(move || {
        let channel = AuthenticatedControlClientChannel::connect_fresh_session(
            ControlTlsEndpoint {
                address: endpoint,
                io_timeout: Duration::from_secs(2),
            },
            "localhost",
            client_identity,
            expected,
        )
        .unwrap();
        let mut runtime = NodeRuntime::new(client_node.clone()).unwrap();
        let mut link =
            CatalogControlLink::admit_dialed_new_peer(&mut runtime, channel, android_display_name)
                .unwrap();
        let selected = source_capabilities(&client_node);
        let outgoing = link.prepare_catalog(&runtime, &selected).unwrap();
        link.send_catalog(outgoing).unwrap();
        let incoming = link.receive_catalog().unwrap();
        link.apply_catalog(&mut runtime, incoming).unwrap();
        (link, runtime)
    });

    let selected = source_capabilities(&android_node);
    let android_link = listener
        .accept_and_exchange(&mut android, &selected)
        .unwrap();
    let (windows_link, windows_runtime) = worker.join().unwrap();

    assert!(!android_link.is_closed());
    assert!(!windows_link.is_closed());
    let android_snapshot = android.snapshot();
    assert_eq!(android_snapshot.sessions.len(), 1);
    assert_eq!(android_snapshot.peers[0].id, windows_node.id);
    assert_eq!(
        android_snapshot.peers[0].capabilities,
        windows_node.capabilities
    );
    let windows_snapshot = windows_runtime.snapshot();
    assert_eq!(windows_snapshot.peers[0].id, android_node.id);
    assert_eq!(
        windows_snapshot.peers[0].capabilities,
        android_node.capabilities
    );
}

#[test]
fn invalid_first_catalog_closes_the_admitted_android_session() {
    let android = Mutex::new(AndroidCatalogRuntimeV1::new(ANDROID_FIXTURE).unwrap());
    let android_node = android.lock().unwrap().snapshot().local_node;
    let windows_node = windows_node();
    let expected = ControlTlsNodeBinding::new(android_node.id, windows_node.id).unwrap();
    let (server_identity, client_identity, _) = identities();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = listener.local_addr().unwrap();
    let server = AuthenticatedControlServer::from_listener_fresh_session(
        listener,
        server_identity,
        expected,
        Duration::from_secs(2),
    )
    .unwrap();
    let wrong_catalog =
        capyio_protocol::encode_catalog_snapshot(SessionId::new(), &windows_node).unwrap();
    let worker = thread::spawn(move || {
        let mut channel = AuthenticatedControlClientChannel::connect_fresh_session(
            ControlTlsEndpoint {
                address: endpoint,
                io_timeout: Duration::from_secs(2),
            },
            "localhost",
            client_identity,
            expected,
        )
        .unwrap();
        channel.send_envelope(&wrong_catalog).unwrap();
    });

    let channel = server.accept().unwrap();
    assert!(
        exchange_accepted_android_catalog(channel, windows_node.display_name, || android
            .lock()
            .map_err(|error| error.to_string()))
        .is_err()
    );
    worker.join().unwrap();
    let snapshot = android.lock().unwrap().snapshot();
    assert_eq!(snapshot.sessions.len(), 1);
    assert_eq!(snapshot.sessions[0].state, SessionState::Closed);
}

#[test]
fn accepted_slow_catalog_does_not_hold_runtime_or_lose_publication_updates() {
    let android = Arc::new(Mutex::new(
        AndroidCatalogRuntimeV1::new(ANDROID_FIXTURE).unwrap(),
    ));
    let android_node = android.lock().unwrap().snapshot().local_node;
    let windows = windows_node();
    let windows_name = windows.display_name.clone();
    let expected = ControlTlsNodeBinding::new(android_node.id, windows.id).unwrap();
    let (server_identity, client_identity, _) = identities();
    let socket = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = socket.local_addr().unwrap();
    let server = AuthenticatedControlServer::from_listener_fresh_session(
        socket,
        server_identity,
        expected,
        Duration::from_secs(2),
    )
    .unwrap();
    let (continue_tx, continue_rx) = mpsc::channel();
    let client = thread::spawn(move || {
        let channel = AuthenticatedControlClientChannel::connect_fresh_session(
            ControlTlsEndpoint {
                address: endpoint,
                io_timeout: Duration::from_secs(2),
            },
            "localhost",
            client_identity,
            expected,
        )
        .unwrap();
        let mut runtime = NodeRuntime::new(windows).unwrap();
        let mut link =
            CatalogControlLink::admit_dialed_peer(&mut runtime, channel, android_node.display_name)
                .unwrap();
        continue_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let outgoing = link.prepare_catalog(&runtime, &BTreeSet::new()).unwrap();
        link.send_catalog(outgoing).unwrap();
        let incoming = link.receive_catalog().unwrap();
        link.apply_catalog(&mut runtime, incoming).unwrap();
        (link, runtime)
    });
    let worker_runtime = Arc::clone(&android);
    let (accessed_tx, accessed_rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        let channel = server.accept().unwrap();
        exchange_accepted_android_catalog(channel, windows_name, || {
            let guard = worker_runtime.lock().map_err(|error| error.to_string())?;
            let _ = accessed_tx.send(());
            Ok(guard)
        })
        .unwrap()
    });
    accessed_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let deadline = Instant::now() + Duration::from_millis(500);
    loop {
        if let Ok(mut runtime) = android.try_lock() {
            runtime
                .replace_catalog(&ANDROID_FIXTURE.replace("CapyIO Android", "Updated Android"))
                .unwrap();
            break;
        }
        assert!(
            Instant::now() < deadline,
            "catalog wait held the Runtime mutex"
        );
        thread::sleep(Duration::from_millis(1));
    }
    continue_tx.send(()).unwrap();
    let (link, revision) = worker.join().unwrap();
    let (_client_link, client_runtime) = client.join().unwrap();
    assert_eq!(revision, 1);
    assert_eq!(
        client_runtime.snapshot().peers[0].display_name,
        "Updated Android"
    );
    assert_eq!(android.lock().unwrap().publication_revision(), revision);
    android.lock().unwrap().close_control_link(link).unwrap();
}

fn source_capabilities(node: &NodeDescriptor) -> BTreeSet<CapabilityId> {
    node.capabilities
        .iter()
        .filter(|(_, capability)| {
            capability
                .ports
                .values()
                .any(|port| port.direction == PortDirection::Source)
        })
        .map(|(id, _)| *id)
        .collect()
}

fn identities() -> (
    ControlTlsServerIdentity,
    ControlTlsClientIdentity,
    ControlCredentialBundle,
) {
    let ca_key = KeyPair::generate().unwrap();
    let mut ca_params = CertificateParams::new(vec!["CapyIO Test CA".to_owned()]).unwrap();
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::DigitalSignature,
    ];
    let ca = ca_params.self_signed(&ca_key).unwrap();
    let issuer = Issuer::from_params(&ca_params, &ca_key);

    let server_key = KeyPair::generate().unwrap();
    let mut server_params = CertificateParams::new(vec!["localhost".to_owned()]).unwrap();
    server_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    let server = server_params.signed_by(&server_key, &issuer).unwrap();

    let client_key = KeyPair::generate().unwrap();
    let mut client_params = CertificateParams::new(vec!["client.capyio.test".to_owned()]).unwrap();
    client_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
    let client = client_params.signed_by(&client_key, &issuer).unwrap();

    let ca_der = ca.der().to_vec();
    let server_der = server.der().to_vec();
    let client_der = client.der().to_vec();
    let server_bundle = ControlCredentialBundle::new(
        vec![server_der.clone()],
        server_key.serialize_der(),
        ca_der.clone(),
    )
    .unwrap();
    (
        ControlTlsServerIdentity {
            expected_peer_certificate: CertificateFingerprint::from_certificate_der(&client_der)
                .unwrap(),
            certificate_chain_der: vec![server_der.clone()],
            private_key_der: server_key.serialize_der(),
            client_ca_der: ca_der.clone(),
        },
        ControlTlsClientIdentity {
            expected_peer_certificate: CertificateFingerprint::from_certificate_der(&server_der)
                .unwrap(),
            certificate_chain_der: vec![client_der],
            private_key_der: client_key.serialize_der(),
            server_ca_der: ca_der,
        },
        server_bundle,
    )
}
