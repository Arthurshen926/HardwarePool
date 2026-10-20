#![cfg(target_os = "linux")]
//! Hosted loopback-only supervision checks. All identities and catalogs are
//! ephemeral test fixtures. No media offer is returned, so no UDP worker,
//! PipeWire process, credentials file, or physical device is opened.
use capyio_control_link::{CatalogControlLink, ReceivedControlMessage};
use capyio_control_tls::{
    AuthenticatedControlClientChannel, AuthenticatedControlServer, ControlTlsBinding,
    ControlTlsClientIdentity, ControlTlsEndpoint, ControlTlsServerIdentity,
};
use capyio_core::{
    FormatDescriptor, InteroperabilityMode, Platform, QosMode, RouteBackend, RouteId, RouteState,
    SessionState, StreamId,
};
use capyio_linux_audio::{AuthenticatedPlaybackOwner, PlaybackAdmission, PlaybackTarget};
use capyio_linux_host::{LinuxPlaybackHost, PlaybackExit, PlaybackStopHandle};
use capyio_protocol::{
    NativeAudioDataOffer, NativeAudioEndpoint, NativeAudioPreset, RouteCommandAction,
};
use capyio_runtime::{NodeRuntime, PortLeasePolicy, RuntimeSnapshot};
use std::{
    io,
    net::TcpListener,
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
        .try_into()
        .unwrap()
}

fn connected() -> (
    LinuxPlaybackHost,
    PlaybackStopHandle,
    CatalogControlLink,
    RouteId,
) {
    // Adapt the existing DemoLab catalogs only inside this test, preserving the
    // real production admission checks, local grant and exclusive Sink lease.
    let lab = capyio_testkit::DemoLab::new().unwrap();
    let original = lab
        .runtime
        .route(lab.routes.phone_microphone_to_windows)
        .unwrap()
        .clone();
    let snapshot = lab.runtime.snapshot();
    let mut local = snapshot.local_node;
    local.platform = Platform::Linux;
    let mut peer = snapshot.peers[0].clone();
    for node in [&mut local, &mut peer] {
        for adapter in node.adapter_instances.values_mut() {
            adapter
                .supported_route_modes
                .insert(RouteBackend::AdapterManaged);
        }
        for capability in node.capabilities.values_mut() {
            for port in capability.ports.values_mut() {
                port.interoperability_mode = InteroperabilityMode::AdapterManaged;
            }
        }
    }
    let peer_id = peer.id;
    let mut runtime = NodeRuntime::new(local).unwrap();
    runtime.register_peer(peer, true).unwrap();
    let session = runtime.open_session(peer_id).unwrap();
    let route = runtime
        .create_route(
            session,
            original.source,
            original.sink,
            RouteBackend::AdapterManaged,
        )
        .unwrap();
    let now = now_ms();
    runtime
        .set_local_port_lease_policy(original.sink, PortLeasePolicy::Exclusive)
        .unwrap();
    runtime.authorize_route(route, Some(now + 60_000)).unwrap();
    runtime
        .acquire_route_port_lease(route, original.sink, now + 30_000, now)
        .unwrap();
    runtime
        .prepare_route(
            route,
            Some(FormatDescriptor::new("pcm-s16le-48000-mono")),
            QosMode::Interactive,
            now,
        )
        .unwrap();
    runtime.begin_route_start(route, now).unwrap();
    let expected =
        ControlTlsBinding::new(session, original.sink.node_id, original.source.node_id).unwrap();
    let admission = PlaybackAdmission {
        expected,
        route_id: route,
        local_offer: NativeAudioDataOffer {
            port: 40211,
            stream_id: StreamId::new(),
            endpoint: NativeAudioEndpoint::Sink,
            preset: NativeAudioPreset::PcmS16le48000Mono,
        },
    };
    let target = PlaybackTarget {
        local: "127.0.0.1:40211".parse().unwrap(),
        duration: Duration::from_secs(30),
        pipewire_target: "synthetic.test.never.opened".into(),
        endpoint_mode: capyio_linux_audio::EndpointMode::Playback,
    };
    let (server_identity, client_identity) = identities();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let timeout = Duration::from_secs(2);
    let server =
        AuthenticatedControlServer::from_listener(listener, server_identity, expected, timeout)
            .unwrap();
    let accepting = thread::spawn(move || server.accept().unwrap());
    let client = AuthenticatedControlClientChannel::connect(
        ControlTlsEndpoint {
            address,
            io_timeout: timeout,
        },
        "localhost",
        client_identity,
        expected,
    )
    .unwrap();
    let accepted = accepting.join().unwrap();
    let owner =
        AuthenticatedPlaybackOwner::from_accepted_trusted_lab(runtime, accepted, admission, target)
            .unwrap();
    let (host, stop) = LinuxPlaybackHost::from_admitted_trusted_lab(owner);
    (host, stop, CatalogControlLink::from_dialed(client), route)
}

type Completion = (io::Result<PlaybackExit>, RuntimeSnapshot, bool);
fn run(mut host: LinuxPlaybackHost) -> (JoinHandle<()>, Receiver<Completion>) {
    let (sender, receiver) = mpsc::channel();
    let worker = thread::spawn(move || {
        let result = host.run();
        let _ = sender.send((result, host.snapshot(), host.metrics().is_none()));
    });
    (worker, receiver)
}
fn receive_offer(peer: &mut CatalogControlLink, route: RouteId) {
    let ReceivedControlMessage::RouteStatus(status) = peer.receive_control_message().unwrap()
    else {
        panic!("expected authenticated local playback offer");
    };
    assert_eq!(status.route.id, route);
    assert_eq!(status.route.state, RouteState::Starting);
    let offer = status.native_audio_data.unwrap();
    assert_eq!(offer.endpoint, NativeAudioEndpoint::Sink);
    assert_eq!(offer.preset, NativeAudioPreset::PcmS16le48000Mono);
    assert_eq!(offer.port, 40211);
}
fn finished(
    worker: JoinHandle<()>,
    done: Receiver<Completion>,
    route: RouteId,
) -> io::Result<PlaybackExit> {
    let (result, snapshot, unopened) = done
        .recv_timeout(Duration::from_secs(3))
        .expect("supervisor must terminate promptly");
    worker.join().unwrap();
    assert!(unopened, "no reciprocal offer may open a media worker");
    assert!(snapshot.port_leases.is_empty());
    assert_eq!(snapshot.sessions.len(), 1);
    assert_eq!(snapshot.sessions[0].state, SessionState::Closed);
    assert!(
        snapshot.routes.iter().any(|item| item.id == route
            && matches!(item.state, RouteState::Stopped | RouteState::Offline))
    );
    result
}

#[test]
fn ipc_host_idle_control_survives_read_deadline_then_local_stop_closes_session() {
    let (host, stop, mut peer, route) = connected();
    let (worker, done) = run(host);
    receive_offer(&mut peer, route);
    // Longer than the owner's one-second blocking receive deadline. No incoming
    // control or media is necessary to keep the polling supervisor alive.
    assert!(matches!(
        done.recv_timeout(Duration::from_millis(1_100)),
        Err(RecvTimeoutError::Timeout)
    ));
    let started = Instant::now();
    for _ in 0..100 {
        stop.request_stop();
    }
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "Stop must not wait for a peer read"
    );
    assert_eq!(
        finished(worker, done, route).unwrap(),
        PlaybackExit::LocalStop
    );
}

#[test]
fn ipc_host_last_stop_handle_drop_closes_session() {
    let (host, stop, mut peer, route) = connected();
    let clone = stop.clone();
    let (worker, done) = run(host);
    receive_offer(&mut peer, route);
    drop(stop);
    assert!(matches!(
        done.recv_timeout(Duration::from_millis(50)),
        Err(RecvTimeoutError::Timeout)
    ));
    drop(clone);
    assert_eq!(
        finished(worker, done, route).unwrap(),
        PlaybackExit::LifecycleOwnerGone
    );
}

#[test]
fn ipc_host_authenticated_peer_stop_closes_session() {
    let (host, _stop, mut peer, route) = connected();
    let (worker, done) = run(host);
    receive_offer(&mut peer, route);
    peer.send_route_command(route, RouteCommandAction::Stop)
        .unwrap();
    assert_eq!(
        finished(worker, done, route).unwrap(),
        PlaybackExit::PeerStop
    );
}

#[test]
fn ipc_host_control_disconnect_fails_closed() {
    let (host, _stop, mut peer, route) = connected();
    let (worker, done) = run(host);
    receive_offer(&mut peer, route);
    drop(peer);
    assert!(finished(worker, done, route).is_err());
}

fn identities() -> (ControlTlsServerIdentity, ControlTlsClientIdentity) {
    use rcgen::{
        BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
        KeyUsagePurpose,
    };
    let mut params = CertificateParams::new(Vec::<String>::new()).unwrap();
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
    ];
    let key = KeyPair::generate().unwrap();
    let ca = params.self_signed(&key).unwrap().der().to_vec();
    let issuer = Issuer::new(params, key);
    let leaf = |name: &str, usage| {
        let mut params = CertificateParams::new(vec![name.into()]).unwrap();
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![usage];
        let key = KeyPair::generate().unwrap();
        (
            params.signed_by(&key, &issuer).unwrap().der().to_vec(),
            key.serialize_der(),
        )
    };
    let (server, server_key) = leaf("localhost", ExtendedKeyUsagePurpose::ServerAuth);
    let (client, client_key) = leaf("capyio-test-client", ExtendedKeyUsagePurpose::ClientAuth);
    let server_pin =
        capyio_control_tls::CertificateFingerprint::from_certificate_der(&server).unwrap();
    let client_pin =
        capyio_control_tls::CertificateFingerprint::from_certificate_der(&client).unwrap();
    (
        ControlTlsServerIdentity {
            expected_peer_certificate: client_pin,
            certificate_chain_der: vec![server, ca.clone()],
            private_key_der: server_key,
            client_ca_der: ca.clone(),
        },
        ControlTlsClientIdentity {
            expected_peer_certificate: server_pin,
            certificate_chain_der: vec![client, ca.clone()],
            private_key_der: client_key,
            server_ca_der: ca,
        },
    )
}
