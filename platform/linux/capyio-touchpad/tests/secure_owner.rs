//! Software-only acceptance: real loopback mTLS and Android policy, fake device writes.
use capyio_android_host::{
    AndroidMotionAction, AndroidMotionSample, AndroidPointerSample, AndroidToolType,
    AndroidTouchSurface, AndroidTouchpadCaptureSession,
};
use capyio_control_tls::{
    CertificateFingerprint, ControlTlsBinding, ControlTlsClientIdentity, ControlTlsEndpoint,
    ControlTlsServerIdentity,
    touchpad::{TouchpadTlsBinding, TouchpadTlsChannel, TouchpadTlsListener},
};
use capyio_core::{
    FormatDescriptor, InteroperabilityMode, NodeDescriptor, Platform, PortDirection, PortRef,
    ProfileId, QosMode, RouteBackend, RouteState, StreamId,
};
use capyio_input::TouchpadFrame;
use capyio_linux_touchpad::{
    EventBatch, InputEvent, RuntimeTouchpadWorker, TouchpadEventTransport,
    linux_touchpad_descriptor,
};
use capyio_remote_touchpad_adapter::{
    PrivateTouchpadPacketCodecV1, PrivateTouchpadTransportCodecV1,
    secure_receiver::{touchpad_record_binding, touchpad_tls_stream},
};
use capyio_runtime::{NodeRuntime, PortLeasePolicy};
use rcgen::{
    BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose,
};
use std::{
    io,
    net::TcpListener,
    sync::{Arc, Condvar, Mutex, mpsc},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
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
fn touchpad_port(node: &NodeDescriptor, direction: PortDirection) -> PortRef {
    node.capabilities
        .values()
        .find_map(|cap| {
            cap.ports.values().find_map(|port| {
                (port.profile == ProfileId::touchpad_frames_v1() && port.direction == direction)
                    .then_some(PortRef {
                        node_id: node.id,
                        capability_id: cap.id,
                        port_id: port.id,
                    })
            })
        })
        .unwrap()
}
fn runtime_fixture(lifetime_ms: u64) -> (Arc<Mutex<NodeRuntime>>, TouchpadTlsBinding) {
    let mut local = capyio_testkit::windows_node();
    local.platform = Platform::Linux;
    let mut peer = capyio_testkit::android_node();
    for node in [&mut local, &mut peer] {
        for adapter in node.adapter_instances.values_mut() {
            adapter
                .supported_route_modes
                .insert(RouteBackend::AdapterManaged);
        }
        for cap in node.capabilities.values_mut() {
            for port in cap.ports.values_mut() {
                port.interoperability_mode = InteroperabilityMode::AdapterManaged;
            }
        }
    }
    let source = touchpad_port(&peer, PortDirection::Source);
    let sink = touchpad_port(&local, PortDirection::Sink);
    let peer_id = peer.id;
    let local_id = local.id;
    let mut runtime = NodeRuntime::new(local).unwrap();
    runtime.register_peer(peer, true).unwrap();
    let session = runtime.open_session(peer_id).unwrap();
    let route = runtime
        .create_route(session, source, sink, RouteBackend::AdapterManaged)
        .unwrap();
    runtime
        .set_local_port_lease_policy(sink, PortLeasePolicy::Exclusive)
        .unwrap();
    let now = now();
    runtime
        .authorize_route(route, Some(now + lifetime_ms))
        .unwrap();
    runtime
        .acquire_route_port_lease(route, sink, now + lifetime_ms, now)
        .unwrap();
    runtime
        .prepare_route(
            route,
            Some(FormatDescriptor::new("touchpad-frame-v1")),
            QosMode::Interactive,
            now,
        )
        .unwrap();
    runtime.begin_route_start(route, now).unwrap();
    let binding = TouchpadTlsBinding::from_route(
        ControlTlsBinding::new(session, peer_id, local_id).unwrap(),
        runtime.route(route).unwrap(),
        StreamId::new(),
        now,
    )
    .unwrap();
    (Arc::new(Mutex::new(runtime)), binding)
}

#[derive(Default)]
struct Trace {
    opens: usize,
    batches: Vec<Vec<InputEvent>>,
    closes: usize,
    fail_next: bool,
}
struct FakeDevice {
    trace: Arc<Mutex<Trace>>,
    closed: bool,
}
impl TouchpadEventTransport for FakeDevice {
    fn submit(&mut self, batch: &EventBatch) -> io::Result<()> {
        assert!(!self.closed, "no writes after native close");
        let mut trace = self.trace.lock().unwrap();
        if std::mem::take(&mut trace.fail_next) {
            return Err(io::Error::other("simulated uncertain native write"));
        }
        trace.batches.push(batch.as_slice().to_vec());
        Ok(())
    }
    fn close(&mut self) -> io::Result<()> {
        if !self.closed {
            self.closed = true;
            self.trace.lock().unwrap().closes += 1;
        }
        Ok(())
    }
}
fn open_fake(trace: Arc<Mutex<Trace>>) -> impl FnOnce(u64) -> io::Result<FakeDevice> + Send {
    move |deadline| {
        assert!(deadline > now());
        trace.lock().unwrap().opens += 1;
        Ok(FakeDevice {
            trace,
            closed: false,
        })
    }
}
fn wait_for(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !predicate() {
        assert!(
            Instant::now() < deadline,
            "condition did not complete within bounded software test"
        );
        thread::sleep(Duration::from_millis(2));
    }
}
fn terminal(worker: &mut RuntimeTouchpadWorker) -> io::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match worker.poll() {
            Ok(true) => {
                assert!(Instant::now() < deadline);
                thread::sleep(Duration::from_millis(2));
            }
            Ok(false) => return Ok(()),
            Err(e) => return Err(e),
        }
    }
}
fn capture(binding: TouchpadTlsBinding) -> AndroidTouchpadCaptureSession {
    AndroidTouchpadCaptureSession::new(
        touchpad_tls_stream(binding),
        AndroidTouchSurface {
            width_px: 1200,
            height_px: 700,
            descriptor: linux_touchpad_descriptor(),
        },
        0,
    )
    .unwrap()
}
fn finger(id: u32, x: f32, y: f32) -> AndroidPointerSample {
    AndroidPointerSample {
        pointer_id: id,
        tool_type: AndroidToolType::Finger,
        x_px: x,
        y_px: y,
        pressure: Some(0.5),
    }
}
fn motion(
    action: AndroidMotionAction,
    pointers: Vec<AndroidPointerSample>,
    nanos: u64,
) -> AndroidMotionSample {
    AndroidMotionSample {
        action,
        pointers,
        event_time_nanos: nanos,
    }
}
fn send_frame(sender: &mut TouchpadTlsChannel, binding: TouchpadTlsBinding, frame: &TouchpadFrame) {
    let packet = PrivateTouchpadPacketCodecV1::new(
        touchpad_tls_stream(binding),
        linux_touchpad_descriptor(),
    )
    .unwrap()
    .encode(frame)
    .unwrap();
    sender
        .send_record(
            PrivateTouchpadTransportCodecV1::new(touchpad_record_binding(binding))
                .encode_data(&packet)
                .unwrap()
                .as_bytes(),
        )
        .unwrap();
}
fn ack(sender: &mut TouchpadTlsChannel, binding: TouchpadTlsBinding, sequence: u64) {
    PrivateTouchpadTransportCodecV1::new(touchpad_record_binding(binding))
        .validate_ack(&sender.receive_record().unwrap(), sequence)
        .unwrap();
}
fn has(batch: &[InputEvent], event_type: u16, code: u16, value: i32) -> bool {
    batch.iter().any(|event| {
        *event
            == InputEvent {
                event_type,
                code,
                value,
            }
    })
}

#[test]
fn android_capture_policy_over_real_mtls_reaches_production_projector_and_stops() {
    let (runtime, binding) = runtime_fixture(20_000);
    let (mut sender, channel) = channel_pair(binding);
    let trace = Arc::new(Mutex::new(Trace::default()));
    let mut worker = RuntimeTouchpadWorker::start_with_transport(
        runtime.clone(),
        channel,
        open_fake(trace.clone()),
    )
    .unwrap();
    assert_eq!(trace.lock().unwrap().opens, 0, "no device before Hello");
    sender
        .send_record(
            PrivateTouchpadTransportCodecV1::new(touchpad_record_binding(binding))
                .encode_hello()
                .as_bytes(),
        )
        .unwrap();
    wait_for(|| {
        if worker.is_finished() {
            panic!("owner exited before open: {:?}", worker.poll());
        }
        trace.lock().unwrap().opens == 1
    });
    assert_eq!(
        runtime
            .lock()
            .unwrap()
            .route(binding.route_id)
            .unwrap()
            .state,
        RouteState::Starting,
        "opening alone is not Active"
    );
    let mut capture = capture(binding);
    send_frame(&mut sender, binding, &capture.start(1).unwrap());
    ack(&mut sender, binding, 0);
    for count in 1..=5 {
        let action = if count == 1 {
            AndroidMotionAction::Down
        } else {
            AndroidMotionAction::PointerDown {
                action_index: count - 1,
            }
        };
        let points = (0..count)
            .map(|i| finger(i as u32, 100.0 + i as f32 * 50.0, 100.0))
            .collect();
        let frame = capture
            .map_motion(&motion(action, points, count as u64 + 1))
            .unwrap();
        send_frame(&mut sender, binding, &frame);
        ack(&mut sender, binding, count as u64);
    }
    // Actual Android recommended policy attenuates 3+ finger movement by 0.7.
    let frame = capture
        .map_motion(&motion(
            AndroidMotionAction::Move,
            (0..5)
                .map(|i| finger(i, 110.0 + i as f32 * 50.0, 100.0))
                .collect(),
            7,
        ))
        .unwrap();
    assert_eq!(frame.contacts[0].position.x_himetric, 1070);
    send_frame(&mut sender, binding, &frame);
    ack(&mut sender, binding, 6);
    assert_eq!(
        runtime
            .lock()
            .unwrap()
            .route(binding.route_id)
            .unwrap()
            .state,
        RouteState::Active
    );
    {
        let trace = trace.lock().unwrap();
        assert_eq!(trace.opens, 1);
        assert_eq!(trace.batches.len(), 7);
        let five = &trace.batches[5];
        assert!(has(five, 1, 0x148, 1)); // BTN_TOOL_QUINTTAP
        assert!(has(five, 1, 0x14a, 1)); // BTN_TOUCH
        assert!(has(five, 3, 0x3a, 32768));
        assert!(has(&trace.batches[6], 3, 0x35, 1070));
        assert!(trace.batches.iter().all(|batch| batch.last()
            == Some(&InputEvent {
                event_type: 0,
                code: 0,
                value: 0
            })));
    }
    // Stop interrupts a blocking TLS read. Cleanup evidence, not a synthetic
    // successful network close, is the assertion below.
    let _ = worker.stop();
    let trace = trace.lock().unwrap();
    assert_eq!(trace.closes, 1);
    let release = trace.batches.last().unwrap();
    assert!(has(release, 1, 0x14a, 0));
    assert_eq!(
        release
            .iter()
            .filter(|e| e.code == 0x39 && e.value == -1)
            .count(),
        5
    );
}

#[test]
fn missing_runtime_lease_and_mismatched_hello_never_open_device() {
    for bad_hello in [false, true] {
        let (runtime, binding) = runtime_fixture(10_000);
        let (mut sender, channel) = channel_pair(binding);
        let trace = Arc::new(Mutex::new(Trace::default()));
        if !bad_hello {
            runtime
                .lock()
                .unwrap()
                .release_route_port_lease(
                    binding.route_id,
                    binding.sink,
                    binding.session.session_id,
                )
                .unwrap();
            assert!(
                RuntimeTouchpadWorker::start_with_transport(
                    runtime,
                    channel,
                    open_fake(trace.clone())
                )
                .is_err()
            );
        } else {
            let mut worker = RuntimeTouchpadWorker::start_with_transport(
                runtime,
                channel,
                open_fake(trace.clone()),
            )
            .unwrap();
            let mut wrong = touchpad_record_binding(binding);
            wrong.route_epoch += 1;
            sender
                .send_record(
                    PrivateTouchpadTransportCodecV1::new(wrong)
                        .encode_hello()
                        .as_bytes(),
                )
                .unwrap();
            assert!(terminal(&mut worker).is_err());
        }
        assert_eq!(trace.lock().unwrap().opens, 0);
    }
}

#[test]
fn revoked_lease_eof_and_idle_deadline_release_active_contacts() {
    for cause in 0..3 {
        let (runtime, binding) = runtime_fixture(10_000);
        let (mut sender, channel) = channel_pair(binding);
        let trace = Arc::new(Mutex::new(Trace::default()));
        let mut worker = RuntimeTouchpadWorker::start_with_transport(
            runtime.clone(),
            channel,
            open_fake(trace.clone()),
        )
        .unwrap();
        sender
            .send_record(
                PrivateTouchpadTransportCodecV1::new(touchpad_record_binding(binding))
                    .encode_hello()
                    .as_bytes(),
            )
            .unwrap();
        let mut capture = capture(binding);
        send_frame(&mut sender, binding, &capture.start(1).unwrap());
        ack(&mut sender, binding, 0);
        send_frame(
            &mut sender,
            binding,
            &capture
                .map_motion(&motion(
                    AndroidMotionAction::Down,
                    vec![finger(1, 500.0, 300.0)],
                    2,
                ))
                .unwrap(),
        );
        ack(&mut sender, binding, 1);
        match cause {
            0 => {
                runtime
                    .lock()
                    .unwrap()
                    .release_route_port_lease(
                        binding.route_id,
                        binding.sink,
                        binding.session.session_id,
                    )
                    .unwrap();
            }
            1 => {
                drop(sender);
            }
            _ => {} // independent owner idle deadline, with no further receive call from this test
        }
        assert!(terminal(&mut worker).is_err());
        let trace = trace.lock().unwrap();
        assert_eq!(trace.closes, 1);
        assert!(has(trace.batches.last().unwrap(), 3, 0x39, -1));
    }
}

#[test]
fn failed_native_commit_never_acknowledges_the_frame() {
    let (runtime, binding) = runtime_fixture(10_000);
    let (mut sender, channel) = channel_pair(binding);
    let trace = Arc::new(Mutex::new(Trace::default()));
    let mut worker =
        RuntimeTouchpadWorker::start_with_transport(runtime, channel, open_fake(trace.clone()))
            .unwrap();
    sender
        .send_record(
            PrivateTouchpadTransportCodecV1::new(touchpad_record_binding(binding))
                .encode_hello()
                .as_bytes(),
        )
        .unwrap();
    let mut capture = capture(binding);
    send_frame(&mut sender, binding, &capture.start(1).unwrap());
    ack(&mut sender, binding, 0);
    trace.lock().unwrap().fail_next = true;
    send_frame(
        &mut sender,
        binding,
        &capture
            .map_motion(&motion(
                AndroidMotionAction::Down,
                vec![finger(1, 500.0, 300.0)],
                2,
            ))
            .unwrap(),
    );
    assert!(
        sender.receive_record().is_err(),
        "failed native batch must not receive an ACK"
    );
    assert!(terminal(&mut worker).is_err());
    assert_eq!(trace.lock().unwrap().closes, 1);
}

struct GatedDevice {
    device: FakeDevice,
    gate: Arc<(Mutex<(bool, bool)>, Condvar)>,
    first: bool,
}
impl TouchpadEventTransport for GatedDevice {
    fn submit(&mut self, batch: &EventBatch) -> io::Result<()> {
        if self.first {
            self.first = false;
            let (lock, wake) = &*self.gate;
            let mut state = lock.lock().unwrap();
            state.0 = true;
            wake.notify_all();
            let (state, timeout) = wake
                .wait_timeout_while(state, Duration::from_secs(1), |state| !state.1)
                .unwrap();
            if timeout.timed_out() || !state.1 {
                return Err(io::Error::other("test commit gate expired"));
            }
        }
        self.device.submit(batch)
    }
    fn close(&mut self) -> io::Result<()> {
        self.device.close()
    }
}
#[test]
fn ack_waits_for_complete_backend_commit_not_queue_admission() {
    let (runtime, binding) = runtime_fixture(10_000);
    let (mut sender, channel) = channel_pair(binding);
    let trace = Arc::new(Mutex::new(Trace::default()));
    let gate = Arc::new((Mutex::new((false, false)), Condvar::new()));
    let device_gate = gate.clone();
    let device_trace = trace.clone();
    let mut worker = RuntimeTouchpadWorker::start_with_transport(runtime, channel, move |_| {
        device_trace.lock().unwrap().opens += 1;
        Ok(GatedDevice {
            device: FakeDevice {
                trace: device_trace,
                closed: false,
            },
            gate: device_gate,
            first: true,
        })
    })
    .unwrap();
    sender
        .send_record(
            PrivateTouchpadTransportCodecV1::new(touchpad_record_binding(binding))
                .encode_hello()
                .as_bytes(),
        )
        .unwrap();
    send_frame(&mut sender, binding, &capture(binding).start(1).unwrap());
    wait_for(|| gate.0.lock().unwrap().0);
    let (completed, result) = mpsc::channel();
    let reader = thread::spawn(move || {
        let record = sender.receive_record();
        completed.send(record).unwrap();
        sender
    });
    thread::sleep(Duration::from_millis(30));
    assert!(
        matches!(result.try_recv(), Err(mpsc::TryRecvError::Empty)),
        "no ACK while commit is blocked"
    );
    assert!(trace.lock().unwrap().batches.is_empty());
    gate.0.lock().unwrap().1 = true;
    gate.1.notify_all();
    let record = result
        .recv_timeout(Duration::from_secs(2))
        .unwrap()
        .unwrap();
    PrivateTouchpadTransportCodecV1::new(touchpad_record_binding(binding))
        .validate_ack(&record, 0)
        .unwrap();
    assert_eq!(trace.lock().unwrap().batches.len(), 1);
    let _sender = reader.join().unwrap();
    // Stop interrupts a blocking TLS read. Cleanup evidence, not a synthetic
    // successful network close, is the assertion below.
    let _ = worker.stop();
}

#[test]
fn finite_grant_expiry_releases_contacts_without_new_packets() {
    let (runtime, binding) = runtime_fixture(700);
    let (mut sender, channel) = channel_pair(binding);
    let trace = Arc::new(Mutex::new(Trace::default()));
    let mut worker =
        RuntimeTouchpadWorker::start_with_transport(runtime, channel, open_fake(trace.clone()))
            .unwrap();
    sender
        .send_record(
            PrivateTouchpadTransportCodecV1::new(touchpad_record_binding(binding))
                .encode_hello()
                .as_bytes(),
        )
        .unwrap();
    let mut capture = capture(binding);
    send_frame(&mut sender, binding, &capture.start(1).unwrap());
    ack(&mut sender, binding, 0);
    send_frame(
        &mut sender,
        binding,
        &capture
            .map_motion(&motion(
                AndroidMotionAction::Down,
                vec![finger(1, 500.0, 300.0)],
                2,
            ))
            .unwrap(),
    );
    ack(&mut sender, binding, 1);
    assert!(terminal(&mut worker).is_err());
    assert!(now() >= binding.expires_at_ms);
    let trace = trace.lock().unwrap();
    assert_eq!(trace.closes, 1);
    assert!(has(trace.batches.last().unwrap(), 3, 0x39, -1));
}

#[test]
fn revoke_during_blocked_commit_never_acknowledges_and_closes() {
    let (runtime, binding) = runtime_fixture(10_000);
    let (mut sender, channel) = channel_pair(binding);
    let trace = Arc::new(Mutex::new(Trace::default()));
    let gate = Arc::new((Mutex::new((false, false)), Condvar::new()));
    let device_gate = gate.clone();
    let device_trace = trace.clone();
    let mut worker =
        RuntimeTouchpadWorker::start_with_transport(runtime.clone(), channel, move |_| {
            device_trace.lock().unwrap().opens += 1;
            Ok(GatedDevice {
                device: FakeDevice {
                    trace: device_trace,
                    closed: false,
                },
                gate: device_gate,
                first: true,
            })
        })
        .unwrap();
    sender
        .send_record(
            PrivateTouchpadTransportCodecV1::new(touchpad_record_binding(binding))
                .encode_hello()
                .as_bytes(),
        )
        .unwrap();
    send_frame(&mut sender, binding, &capture(binding).start(1).unwrap());
    wait_for(|| gate.0.lock().unwrap().0);
    runtime
        .lock()
        .unwrap()
        .release_route_port_lease(binding.route_id, binding.sink, binding.session.session_id)
        .unwrap();
    gate.0.lock().unwrap().1 = true;
    gate.1.notify_all();
    assert!(
        sender.receive_record().is_err(),
        "revocation while committing must prevent acknowledgement"
    );
    assert!(terminal(&mut worker).is_err());
    assert_eq!(trace.lock().unwrap().closes, 1);
}

#[test]
fn stop_or_changed_grant_before_hello_cannot_create_device() {
    for renew in [false, true] {
        let (runtime, binding) = runtime_fixture(10_000);
        let (mut sender, channel) = channel_pair(binding);
        let trace = Arc::new(Mutex::new(Trace::default()));
        let mut worker = RuntimeTouchpadWorker::start_with_transport(
            runtime.clone(),
            channel,
            open_fake(trace.clone()),
        )
        .unwrap();
        if renew {
            runtime
                .lock()
                .unwrap()
                .renew_route_authorization(binding.route_id, binding.expires_at_ms + 1_000, now())
                .unwrap();
            // A new local grant does not silently extend the pinned data epoch.
            let _ = sender.send_record(
                PrivateTouchpadTransportCodecV1::new(touchpad_record_binding(binding))
                    .encode_hello()
                    .as_bytes(),
            );
            assert!(terminal(&mut worker).is_err());
        } else {
            let started = Instant::now();
            let _ = worker.stop();
            assert!(started.elapsed() < Duration::from_secs(2));
            assert!(worker.is_finished());
        }
        assert_eq!(trace.lock().unwrap().opens, 0);
    }
}

#[test]
fn initial_update_cannot_acknowledge_or_activate_route() {
    let (runtime, binding) = runtime_fixture(10_000);
    let (mut sender, channel) = channel_pair(binding);
    let trace = Arc::new(Mutex::new(Trace::default()));
    let mut worker = RuntimeTouchpadWorker::start_with_transport(
        runtime.clone(),
        channel,
        open_fake(trace.clone()),
    )
    .unwrap();
    sender
        .send_record(
            PrivateTouchpadTransportCodecV1::new(touchpad_record_binding(binding))
                .encode_hello()
                .as_bytes(),
        )
        .unwrap();
    let mut capture = capture(binding);
    let _unsent_barrier = capture.start(1).unwrap();
    let mut update = capture
        .map_motion(&motion(
            AndroidMotionAction::Down,
            vec![finger(1, 500.0, 300.0)],
            2,
        ))
        .unwrap();
    update.header.sequence = 0;
    send_frame(&mut sender, binding, &update);
    assert!(
        sender.receive_record().is_err(),
        "uninitialized device must not acknowledge ordinary input"
    );
    assert!(terminal(&mut worker).is_err());
    assert_ne!(
        runtime
            .lock()
            .unwrap()
            .route(binding.route_id)
            .unwrap()
            .state,
        RouteState::Active
    );
    let trace = trace.lock().unwrap();
    assert_eq!(trace.opens, 1);
    assert_eq!(trace.closes, 1);
    assert!(trace.batches.iter().all(|batch| !has(batch, 1, 0x14a, 1)));
}

#[test]
fn runtime_lock_contention_cannot_suspend_shutdown_or_leave_contacts() {
    let (runtime, binding) = runtime_fixture(10_000);
    let (mut sender, channel) = channel_pair(binding);
    let trace = Arc::new(Mutex::new(Trace::default()));
    let mut worker = RuntimeTouchpadWorker::start_with_transport(
        runtime.clone(),
        channel,
        open_fake(trace.clone()),
    )
    .unwrap();
    sender
        .send_record(
            PrivateTouchpadTransportCodecV1::new(touchpad_record_binding(binding))
                .encode_hello()
                .as_bytes(),
        )
        .unwrap();
    let mut capture = capture(binding);
    send_frame(&mut sender, binding, &capture.start(1).unwrap());
    ack(&mut sender, binding, 0);
    send_frame(
        &mut sender,
        binding,
        &capture
            .map_motion(&motion(
                AndroidMotionAction::Down,
                vec![finger(1, 500.0, 300.0)],
                2,
            ))
            .unwrap(),
    );
    ack(&mut sender, binding, 1);
    let locked = runtime.lock().unwrap();
    thread::sleep(Duration::from_millis(150));
    let started = Instant::now();
    let _ = worker.stop();
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "shutdown must not await an unbounded Runtime lock"
    );
    assert!(worker.is_finished());
    let trace = trace.lock().unwrap();
    assert_eq!(trace.closes, 1);
    assert!(has(trace.batches.last().unwrap(), 3, 0x39, -1));
    drop(locked);
}
