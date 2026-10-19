use std::{
    collections::BTreeSet,
    net::TcpListener,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::Duration,
};

use capyio_android_jni::{
    AndroidCatalogRuntimeV1, AndroidControlDialError, authenticate_android_control_dial,
    exchange_dialed_android_catalog,
};
use capyio_control_credentials::ControlCredentialBundle;
use capyio_control_link::CatalogControlLink;
use capyio_control_tls::{
    AuthenticatedControlServer, ControlTlsEndpoint, ControlTlsNodeBinding, ControlTlsServerIdentity,
};
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
fn android_dials_pinned_windows_and_exchanges_catalog_without_a_route() {
    let mut android = AndroidCatalogRuntimeV1::new(ANDROID_FIXTURE).unwrap();
    let android_node = android.snapshot().local_node;
    let windows = windows_node();
    let (server_identity, android_bundle, windows_fingerprint) = dual_identities();
    let directory = paired_windows_directory(android_node.id, windows.id, windows_fingerprint);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = AuthenticatedControlServer::from_listener_fresh_session(
        listener,
        server_identity,
        ControlTlsNodeBinding::new(windows.id, android_node.id).unwrap(),
        Duration::from_secs(2),
    )
    .unwrap();
    let android_name = android_node.display_name.clone();
    let windows_worker = thread::spawn(move || {
        let channel = server.accept().unwrap();
        let mut runtime = NodeRuntime::new(windows).unwrap();
        let mut link =
            CatalogControlLink::admit_accepted_peer(&mut runtime, channel, android_name).unwrap();
        let incoming = link.receive_catalog().unwrap();
        link.apply_catalog(&mut runtime, incoming).unwrap();
        let outgoing = link.prepare_catalog(&runtime, &BTreeSet::new()).unwrap();
        link.send_catalog(outgoing).unwrap();
        let snapshot = runtime.snapshot();
        (link, runtime, snapshot)
    });
    let link = android
        .dial_paired_catalog_exchange(
            ControlTlsEndpoint {
                address,
                io_timeout: Duration::from_secs(2),
            },
            &directory.encode_json().unwrap(),
            &android_bundle.encode().unwrap(),
            &BTreeSet::new(),
        )
        .unwrap();
    let (mut windows_link, mut windows_runtime, windows_snapshot) = windows_worker.join().unwrap();
    assert_eq!(windows_snapshot.peers[0].id, android_node.id);
    assert_eq!(
        android.snapshot().peers[0].id,
        windows_snapshot.local_node.id
    );
    assert_eq!(android.snapshot().sessions.len(), 1);
    assert!(android.snapshot().routes.is_empty());
    windows_link
        .close_and_reconcile(&mut windows_runtime)
        .unwrap();
    // The Android Runtime remains owned by the caller while its live link is returned.
    assert!(!link.is_closed());
    android.close_control_link(link).unwrap();
    assert!(
        android
            .snapshot()
            .sessions
            .iter()
            .all(|session| session.state == capyio_core::SessionState::Closed)
    );
}

#[test]
fn android_dial_rejects_unpaired_and_invalid_credentials_before_network() {
    let mut android = AndroidCatalogRuntimeV1::new(ANDROID_FIXTURE).unwrap();
    let local = android.snapshot().local_node.id;
    let empty = PairedPeerDirectory::new(local).unwrap();
    let endpoint = ControlTlsEndpoint {
        address: "127.0.0.1:9".parse().unwrap(),
        io_timeout: Duration::from_secs(1),
    };
    assert!(matches!(
        android.dial_paired_catalog_exchange(
            endpoint,
            &empty.encode_json().unwrap(),
            &[],
            &BTreeSet::new(),
        ),
        Err(AndroidControlDialError::NoActivePeer)
    ));
    let directory = paired_windows_directory(
        local,
        windows_node().id,
        CertificateFingerprint::new([7; 32]).unwrap(),
    );
    assert!(matches!(
        android.dial_paired_catalog_exchange(
            endpoint,
            &directory.encode_json().unwrap(),
            &[],
            &BTreeSet::new(),
        ),
        Err(AndroidControlDialError::Credential(_))
    ));
    assert!(android.snapshot().sessions.is_empty());
}

#[test]
fn android_dial_rejects_wrong_windows_leaf_without_admitting_a_session() {
    let mut android = AndroidCatalogRuntimeV1::new(ANDROID_FIXTURE).unwrap();
    let windows = windows_node();
    let (server_identity, android_bundle, _) = dual_identities();
    let directory = paired_windows_directory(
        android.snapshot().local_node.id,
        windows.id,
        CertificateFingerprint::new([9; 32]).unwrap(),
    );
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = AuthenticatedControlServer::from_listener_fresh_session(
        listener,
        server_identity,
        ControlTlsNodeBinding::new(windows.id, android.snapshot().local_node.id).unwrap(),
        Duration::from_secs(2),
    )
    .unwrap();
    let worker = thread::spawn(move || server.accept());
    assert!(matches!(
        android.dial_paired_catalog_exchange(
            ControlTlsEndpoint {
                address,
                io_timeout: Duration::from_secs(2)
            },
            &directory.encode_json().unwrap(),
            &android_bundle.encode().unwrap(),
            &BTreeSet::new(),
        ),
        Err(AndroidControlDialError::Transport(_))
    ));
    let _ = worker.join().unwrap();
    assert!(android.snapshot().sessions.is_empty());
}

#[test]
fn dialed_slow_catalog_releases_runtime_and_retains_a_newer_publication_revision() {
    let android = Arc::new(Mutex::new(
        AndroidCatalogRuntimeV1::new(ANDROID_FIXTURE).unwrap(),
    ));
    let android_node = android.lock().unwrap().snapshot().local_node;
    let windows = windows_node();
    let (server_identity, android_bundle, windows_fingerprint) = dual_identities();
    let directory = paired_windows_directory(android_node.id, windows.id, windows_fingerprint);
    let socket = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = socket.local_addr().unwrap();
    let server = AuthenticatedControlServer::from_listener_fresh_session(
        socket,
        server_identity,
        ControlTlsNodeBinding::new(windows.id, android_node.id).unwrap(),
        Duration::from_secs(2),
    )
    .unwrap();
    let (waiting_tx, waiting_rx) = mpsc::channel();
    let (continue_tx, continue_rx) = mpsc::channel();
    let windows_worker = thread::spawn(move || {
        let channel = server.accept().unwrap();
        let mut runtime = NodeRuntime::new(windows).unwrap();
        let mut link = CatalogControlLink::admit_accepted_peer(
            &mut runtime,
            channel,
            android_node.display_name,
        )
        .unwrap();
        let incoming = link.receive_catalog().unwrap();
        link.apply_catalog(&mut runtime, incoming).unwrap();
        waiting_tx.send(()).unwrap();
        continue_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let outgoing = link.prepare_catalog(&runtime, &BTreeSet::new()).unwrap();
        link.send_catalog(outgoing).unwrap();
        (link, runtime)
    });
    let worker_runtime = Arc::clone(&android);
    let android_worker = thread::spawn(move || {
        let (channel, display_name) = authenticate_android_control_dial(
            android_node.id,
            ControlTlsEndpoint {
                address,
                io_timeout: Duration::from_secs(2),
            },
            &directory.encode_json().unwrap(),
            &android_bundle.encode().unwrap(),
        )
        .unwrap();
        exchange_dialed_android_catalog(channel, display_name, || {
            worker_runtime.lock().map_err(|error| error.to_string())
        })
        .unwrap()
    });
    waiting_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    {
        let mut runtime = android
            .try_lock()
            .expect("network wait must not hold Runtime mutex");
        runtime
            .replace_catalog(&ANDROID_FIXTURE.replace("CapyIO Android", "Updated Android"))
            .unwrap();
    }
    continue_tx.send(()).unwrap();
    let (link, published_revision) = android_worker.join().unwrap();
    let (_windows_link, windows_runtime) = windows_worker.join().unwrap();
    assert_eq!(
        windows_runtime.snapshot().peers[0].display_name,
        "CapyIO Android"
    );
    assert_eq!(published_revision, 0);
    // The control worker must publish revision 1 on a later heartbeat; marking
    // the current revision as already sent here would silently lose this update.
    assert_eq!(android.lock().unwrap().publication_revision(), 1);
    android.lock().unwrap().close_control_link(link).unwrap();
}

fn paired_windows_directory(
    local: capyio_core::NodeId,
    windows: capyio_core::NodeId,
    fingerprint: CertificateFingerprint,
) -> PairedPeerDirectory {
    let mut directory = PairedPeerDirectory::new(local).unwrap();
    directory
        .add(
            PairedPeerRecord::new(
                windows,
                "Windows",
                "localhost",
                CredentialHandle::new(format!("android-keystore:{}", "a".repeat(64))).unwrap(),
                fingerprint,
                1,
            )
            .unwrap(),
        )
        .unwrap();
    directory
}

fn dual_identities() -> (
    ControlTlsServerIdentity,
    ControlCredentialBundle,
    CertificateFingerprint,
) {
    let mut ca_params = CertificateParams::new(Vec::<String>::new()).unwrap();
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyCertSign,
    ];
    let ca_key = KeyPair::generate().unwrap();
    let ca = ca_params.self_signed(&ca_key).unwrap().der().to_vec();
    let issuer = Issuer::new(ca_params, ca_key);
    let leaf = |name: &str| {
        let mut params = CertificateParams::new(vec![name.to_owned()]).unwrap();
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![
            ExtendedKeyUsagePurpose::ClientAuth,
            ExtendedKeyUsagePurpose::ServerAuth,
        ];
        let key = KeyPair::generate().unwrap();
        (
            params.signed_by(&key, &issuer).unwrap().der().to_vec(),
            key.serialize_der(),
        )
    };
    let (windows_cert, windows_key) = leaf("localhost");
    let (android_cert, android_key) = leaf("android.capyio.test");
    let windows_fingerprint = CertificateFingerprint::from_certificate_der(&windows_cert).unwrap();
    let android_fingerprint = CertificateFingerprint::from_certificate_der(&android_cert).unwrap();
    let windows_bundle =
        ControlCredentialBundle::new(vec![windows_cert, ca.clone()], windows_key, ca.clone())
            .unwrap();
    let android_bundle =
        ControlCredentialBundle::new(vec![android_cert, ca.clone()], android_key, ca).unwrap();
    (
        ControlTlsServerIdentity::from_bundle(android_fingerprint, windows_bundle),
        android_bundle,
        windows_fingerprint,
    )
}
