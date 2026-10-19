#![cfg(feature = "tls")]

use capyio_control_tls::{
    CertificateFingerprint, ControlTlsBinding, ControlTlsClientIdentity, ControlTlsEndpoint,
    ControlTlsServerIdentity,
    touchpad::{TouchpadTlsBinding, TouchpadTlsChannel, TouchpadTlsListener},
};
use capyio_core::{AuthorizationState, ProfileId, Route, RouteBackend, RouteState, StreamId};
use capyio_input::{
    InputFrameHeader, InputStreamDescriptor, TouchpadButtonState, TouchpadDescriptor,
    TouchpadFrame, TouchpadFrameKind,
};
use capyio_remote_touchpad_adapter::{
    PrivateTouchpadPacketCodecV1, PrivateTouchpadSink, PrivateTouchpadSinkFactory,
    PrivateTouchpadTransportCodecV1,
    secure_receiver::{
        SecureTouchpadReceiver, TouchpadSinkAdmission, touchpad_record_binding,
        touchpad_tls_descriptor, touchpad_tls_stream,
    },
};
use capyio_testkit::DemoLab;
use rcgen::{
    BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose,
};
use std::{
    convert::Infallible,
    net::TcpListener,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn identities() -> (ControlTlsServerIdentity, ControlTlsClientIdentity) {
    let mut params = CertificateParams::new(Vec::<String>::new()).unwrap();
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyCertSign,
    ];
    let key = KeyPair::generate().unwrap();
    let ca = params.self_signed(&key).unwrap().der().to_vec();
    let issuer = Issuer::new(params, key);
    let leaf = |name: &str, usage| {
        let mut params = CertificateParams::new(vec![name.to_owned()]).unwrap();
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![usage];
        let key = KeyPair::generate().unwrap();
        (
            params.signed_by(&key, &issuer).unwrap().der().to_vec(),
            key.serialize_der(),
        )
    };
    let (server, server_key) = leaf("localhost", ExtendedKeyUsagePurpose::ServerAuth);
    let (client, client_key) = leaf("client", ExtendedKeyUsagePurpose::ClientAuth);
    (
        ControlTlsServerIdentity {
            expected_peer_certificate: CertificateFingerprint::from_certificate_der(&client)
                .unwrap(),
            certificate_chain_der: vec![server.clone(), ca.clone()],
            private_key_der: server_key,
            client_ca_der: ca.clone(),
        },
        ControlTlsClientIdentity {
            expected_peer_certificate: CertificateFingerprint::from_certificate_der(&server)
                .unwrap(),
            certificate_chain_der: vec![client, ca.clone()],
            private_key_der: client_key,
            server_ca_der: ca,
        },
    )
}

#[derive(Default)]
struct Counts {
    opens: usize,
    frames: usize,
    closes: usize,
}
struct Sink(Arc<Mutex<Counts>>);
impl PrivateTouchpadSink for Sink {
    type Error = Infallible;
    fn submit_frame(&mut self, _: &TouchpadFrame) -> Result<(), Self::Error> {
        self.0.lock().unwrap().frames += 1;
        Ok(())
    }
    fn advance_epoch(&mut self, _: u64, _: u64) -> Result<(), Self::Error> {
        Ok(())
    }
    fn close(&mut self) -> Result<(), Self::Error> {
        self.0.lock().unwrap().closes += 1;
        Ok(())
    }
}
struct Factory(Arc<Mutex<Counts>>);
impl PrivateTouchpadSinkFactory for Factory {
    type Sink = Sink;
    type Error = Infallible;
    fn open(
        &mut self,
        _: &InputStreamDescriptor,
        _: TouchpadDescriptor,
        _: u64,
    ) -> Result<Sink, Infallible> {
        self.0.lock().unwrap().opens += 1;
        Ok(Sink(self.0.clone()))
    }
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
fn route() -> Route {
    let lab = DemoLab::new().unwrap();
    let mut route = lab
        .runtime
        .route(lab.routes.phone_microphone_to_windows)
        .unwrap()
        .clone();
    route.profile = ProfileId::touchpad_frames_v1();
    route.backend = RouteBackend::AdapterManaged;
    route.authorization = AuthorizationState::Authorized {
        expires_at_ms: Some(now() + 60_000),
    };
    route.state = RouteState::Active;
    route.epoch = 1;
    route
}
fn channel_pair(binding: TouchpadTlsBinding) -> (TouchpadTlsChannel, TouchpadTlsChannel) {
    let (server_identity, client_identity) = identities();
    let listener = TouchpadTlsListener::from_listener(
        TcpListener::bind("127.0.0.1:0").unwrap(),
        server_identity,
        binding,
        Duration::from_secs(2),
    )
    .unwrap();
    let address = listener.local_addr().unwrap();
    let worker = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(channel) = listener.try_accept().unwrap() {
                return channel;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(2));
        }
    });
    let client = TouchpadTlsChannel::connect(
        ControlTlsEndpoint {
            address,
            io_timeout: Duration::from_secs(2),
        },
        "localhost",
        client_identity,
        binding,
    )
    .unwrap();
    (worker.join().unwrap(), client)
}
fn binding(route: &Route) -> TouchpadTlsBinding {
    TouchpadTlsBinding::from_route(
        ControlTlsBinding::new(route.session_id, route.source.node_id, route.sink.node_id).unwrap(),
        route,
        StreamId::new(),
        now(),
    )
    .unwrap()
}
fn admission(route: &Route, binding: TouchpadTlsBinding) -> Result<TouchpadSinkAdmission, String> {
    Ok(TouchpadSinkAdmission {
        route: route.clone(),
        local_lease_expires_at_ms: binding.expires_at_ms - 1,
        session_ready: true,
    })
}

#[test]
fn tls_records_reach_sink_and_cancel_on_route_loss() {
    let route = route();
    let binding = binding(&route);
    let (mut sender, channel) = channel_pair(binding);
    let codec = PrivateTouchpadTransportCodecV1::new(touchpad_record_binding(binding));
    sender.send_record(codec.encode_hello().as_bytes()).unwrap();
    let counts = Arc::new(Mutex::new(Counts::default()));
    let mut receiver = SecureTouchpadReceiver::accept(channel, Factory(counts.clone()), || {
        admission(&route, binding)
    })
    .unwrap();
    let stream = touchpad_tls_stream(binding);
    let packet_codec =
        PrivateTouchpadPacketCodecV1::new(stream.clone(), touchpad_tls_descriptor()).unwrap();
    let frame = TouchpadFrame {
        header: InputFrameHeader {
            stream_id: stream.stream_id,
            stream_epoch: stream.stream_epoch,
            sequence: 0,
            source_timestamp_nanos: 1,
        },
        kind: TouchpadFrameKind::CancelAll,
        button: TouchpadButtonState::Released,
        contacts: vec![],
    };
    sender
        .send_record(
            codec
                .encode_data(&packet_codec.encode(&frame).unwrap())
                .unwrap()
                .as_bytes(),
        )
        .unwrap();
    assert!(
        receiver
            .receive_next(|| admission(&route, binding))
            .unwrap()
    );
    codec
        .validate_ack(&sender.receive_record().unwrap(), 0)
        .unwrap();
    let mut offline = route.clone();
    offline.state = RouteState::Offline;
    assert!(
        receiver
            .receive_next(|| admission(&offline, binding))
            .is_err()
    );
    assert_eq!(receiver.frames(), 1);
    drop(receiver);
    let counts = counts.lock().unwrap();
    assert_eq!((counts.opens, counts.frames, counts.closes), (1, 1, 1));
}

#[test]
fn stale_grant_lease_session_and_hello_cannot_open_native_sink() {
    for case in 0..4 {
        let route = route();
        let binding = binding(&route);
        let (mut sender, channel) = channel_pair(binding);
        let mut hello_binding = touchpad_record_binding(binding);
        if case == 3 {
            hello_binding.route_epoch += 1;
        }
        sender
            .send_record(
                PrivateTouchpadTransportCodecV1::new(hello_binding)
                    .encode_hello()
                    .as_bytes(),
            )
            .unwrap();
        let counts = Arc::new(Mutex::new(Counts::default()));
        assert!(
            SecureTouchpadReceiver::accept(channel, Factory(counts.clone()), || {
                let mut admission = admission(&route, binding)?;
                match case {
                    0 => admission.route.authorization = AuthorizationState::Denied,
                    1 => admission.local_lease_expires_at_ms = 1,
                    2 => admission.session_ready = false,
                    _ => {}
                }
                Ok(admission)
            })
            .is_err()
        );
        assert_eq!(counts.lock().unwrap().opens, 0);
    }
}

#[test]
fn explicit_stop_unblocks_receiver_and_closes_native_sink_once() {
    let route = route();
    let binding = binding(&route);
    let (mut sender, channel) = channel_pair(binding);
    sender
        .send_record(
            PrivateTouchpadTransportCodecV1::new(touchpad_record_binding(binding))
                .encode_hello()
                .as_bytes(),
        )
        .unwrap();
    let counts = Arc::new(Mutex::new(Counts::default()));
    let mut receiver = SecureTouchpadReceiver::accept(channel, Factory(counts.clone()), || {
        admission(&route, binding)
    })
    .unwrap();
    let stop = receiver.stop_handle().unwrap();
    let worker = thread::spawn(move || {
        assert!(
            receiver
                .receive_next(|| admission(&route, binding))
                .is_err()
        );
    });
    thread::sleep(Duration::from_millis(30));
    let started = Instant::now();
    stop.stop();
    worker.join().unwrap();
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(counts.lock().unwrap().closes, 1);
}
