//! Dormant production-library handshake over real pinned TLS, not shared grants.
//! No capture helper, Android SDK, physical playback, or default dispatcher.
#![cfg(target_os = "linux")]
#[path = "support/bilateral_fixture.rs"]
mod fixture;

use capyio_android_jni::secure_stereo_receiver::SecureStereoSinkAdmission;
use capyio_android_jni::{AndroidCatalogRuntimeV1, stereo_supervisor::StereoSinkSupervisor};
use capyio_control_link::CatalogControlLink;
use capyio_control_tls::{
    AuthenticatedControlClientChannel, AuthenticatedControlServer, ControlTlsBinding,
    ControlTlsClientIdentity, ControlTlsEndpoint, ControlTlsServerIdentity,
};
use capyio_core::*;
use capyio_linux_audio::{
    SecureStereoSourceHandshake, SecureStereoSourceIntent, SecureStereoSourceOwner,
};
use capyio_protocol::SECURE_AUDIO_STEREO_FORMAT;
use capyio_runtime::NodeRuntime;
use std::{
    collections::BTreeSet,
    net::TcpListener,
    thread,
    time::{Duration, Instant},
};

fn catalog(revision: u64) -> String {
    let mut value: serde_json::Value = serde_json::from_str(include_str!(
        "../../../android/fixtures/android-core-catalog-v1.json"
    ))
    .unwrap();
    let cap = value["capabilities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|cap| cap["class"] == "speaker")
        .unwrap();
    cap["speaker_selection"] =
        serde_json::json!({"profile":"secure_stereo_tls13_v1","revision":revision});
    cap["ports"][0]["formats"] = serde_json::json!([SECURE_AUDIO_STEREO_FORMAT]);
    value.to_string()
}
struct Identities {
    source: ControlTlsClientIdentity,
    sink: ControlTlsServerIdentity,
}
struct Peers {
    host: AndroidCatalogRuntimeV1,
    source: NodeRuntime,
    sink_link: CatalogControlLink,
    source_link: CatalogControlLink,
    source_port: PortRef,
    sink_port: PortRef,
    identity: Identities,
    binding: ControlTlsBinding,
}
impl Peers {
    fn new(android_accepts: bool) -> Self {
        let host = AndroidCatalogRuntimeV1::new(&catalog(1)).unwrap();
        let android = host.snapshot().local_node;
        let (_, linux, source_port, _) = fixture::nodes();
        let cap = android
            .capabilities
            .values()
            .find(|cap| cap.class == CapabilityClass::Speaker)
            .unwrap();
        let sink_port = PortRef {
            node_id: android.id,
            capability_id: cap.id,
            port_id: *cap.ports.keys().next().unwrap(),
        };
        let binding = if android_accepts {
            ControlTlsBinding::new(SessionId::new(), android.id, linux.id)
        } else {
            ControlTlsBinding::new(SessionId::new(), linux.id, android.id)
        }
        .unwrap();
        let mut source = NodeRuntime::new(linux).unwrap();
        let (android_server, android_client, linux_server, linux_client) = fixture::credentials();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = AuthenticatedControlServer::from_listener(
            listener,
            if android_accepts {
                android_server.clone()
            } else {
                linux_server
            },
            binding,
            Duration::from_secs(2),
        )
        .unwrap();
        let accepting = thread::spawn(move || server.accept().unwrap());
        let dialed = AuthenticatedControlClientChannel::connect(
            ControlTlsEndpoint {
                address,
                io_timeout: Duration::from_secs(2),
            },
            "localhost",
            if android_accepts {
                linux_client.clone()
            } else {
                android_client
            },
            binding,
        )
        .unwrap();
        let accepted = accepting.join().unwrap();
        let selected = BTreeSet::from([source_port.capability_id]);
        let (host, sink_link, source_link) = if android_accepts {
            let worker = thread::spawn(move || {
                let mut host = host;
                let link = host
                    .admit_accepted_catalog_exchange(accepted, "Linux".into(), &BTreeSet::new())
                    .unwrap();
                (host, link)
            });
            let mut link =
                CatalogControlLink::admit_dialed_peer(&mut source, dialed, "Android".into())
                    .unwrap();
            link.send_catalog(link.prepare_catalog(&source, &selected).unwrap())
                .unwrap();
            let incoming = link.receive_catalog().unwrap();
            link.apply_catalog(&mut source, incoming).unwrap();
            let (host, sink) = worker.join().unwrap();
            (host, sink, link)
        } else {
            let worker = thread::spawn(move || {
                let mut host = host;
                let link = host
                    .admit_dialed_catalog_exchange(dialed, "Linux".into(), &BTreeSet::new())
                    .unwrap();
                (host, link)
            });
            let mut link =
                CatalogControlLink::admit_accepted_peer(&mut source, accepted, "Android".into())
                    .unwrap();
            let incoming = link.receive_catalog().unwrap();
            link.apply_catalog(&mut source, incoming).unwrap();
            link.send_catalog(link.prepare_catalog(&source, &selected).unwrap())
                .unwrap();
            let (host, sink) = worker.join().unwrap();
            (host, sink, link)
        };
        assert!(host.snapshot().routes.is_empty());
        assert!(source.snapshot().routes.is_empty());
        Self {
            host,
            source,
            sink_link,
            source_link,
            source_port,
            sink_port,
            identity: Identities {
                source: linux_client,
                sink: android_server,
            },
            binding,
        }
    }
    fn handshake_with_lifetime(
        self,
        receiver_origin: bool,
        source_lifetime: Duration,
    ) -> (
        SecureStereoSourceHandshake,
        StereoSinkSupervisor,
        Identities,
        ControlTlsBinding,
        PortRef,
        PortRef,
    ) {
        let mut sink = StereoSinkSupervisor::new(self.host, self.sink_link, 17).unwrap();
        let intent = SecureStereoSourceIntent {
            source: self.source_port,
            sink: self.sink_port,
            pipewire_target: "explicit-test-source".into(),
            lifetime: source_lifetime,
        };
        let mut source = if receiver_origin {
            sink.propose(self.source_port, self.sink_port).unwrap();
            SecureStereoSourceHandshake::await_proposal(self.source, self.source_link, intent)
                .unwrap()
        } else {
            SecureStereoSourceHandshake::propose(self.source, self.source_link, intent).unwrap()
        };
        let until = Instant::now() + Duration::from_secs(2);
        while source.route_id().is_none()
            || sink
                .host_handle()
                .lock()
                .unwrap()
                .snapshot()
                .routes
                .is_empty()
        {
            assert!(Instant::now() < until, "Pending import deadline");
            source.poll().unwrap();
            sink.poll().unwrap();
        }
        (
            source,
            sink,
            self.identity,
            self.binding,
            self.source_port,
            self.sink_port,
        )
    }
}
fn admit(binding: ControlTlsBinding, route_id: RouteId) -> SecureStereoSinkAdmission {
    SecureStereoSinkAdmission {
        expected: binding,
        route_id,
        stream_id: StreamId::new(),
        local_address: "127.0.0.1".parse().unwrap(),
        lifetime: Duration::from_secs(60),
    }
}
fn prepare_pair(
    android_accepts: bool,
    receiver_origin: bool,
) -> (
    SecureStereoSourceHandshake,
    StereoSinkSupervisor,
    Identities,
    RouteId,
    PortRef,
    PortRef,
) {
    prepare_pair_with_limits(
        android_accepts,
        receiver_origin,
        Duration::from_secs(60),
        Duration::from_secs(60),
    )
}
fn prepare_pair_with_limits(
    android_accepts: bool,
    receiver_origin: bool,
    source_lifetime: Duration,
    sink_lifetime: Duration,
) -> (
    SecureStereoSourceHandshake,
    StereoSinkSupervisor,
    Identities,
    RouteId,
    PortRef,
    PortRef,
) {
    let (mut source, mut sink, identity, binding, source_port, sink_port) =
        Peers::new(android_accepts).handshake_with_lifetime(receiver_origin, source_lifetime);
    let route_id = source.route_id().unwrap();
    assert_eq!(
        source.runtime().route(route_id).unwrap().authorization,
        AuthorizationState::Pending
    );
    assert_eq!(
        sink.host_handle().lock().unwrap().snapshot().routes[0].authorization,
        AuthorizationState::Pending
    );
    // Early local approval cannot be queued to take effect after peer approval.
    assert!(
        sink.approve(route_id, true, admit(binding, route_id))
            .is_err()
    );
    source.approve(true).unwrap();
    let original_end = source.approved_end().unwrap();
    let initial_expiry = source.runtime().route(route_id).unwrap().authorization;
    let until = Instant::now() + Duration::from_secs(2);
    loop {
        assert!(
            Instant::now() < until,
            "Authorized Draft reception deadline"
        );
        if sink.poll().unwrap().is_some() {
            break;
        }
    }
    let mut admission = admit(binding, route_id);
    admission.lifetime = sink_lifetime;
    sink.approve(route_id, true, admission).unwrap();
    while source.runtime().route(route_id).unwrap().state != RouteState::Prepared {
        assert!(Instant::now() < until, "Prepared reception deadline");
        source.poll().unwrap();
    }
    let prepared = source.runtime().route(route_id).unwrap().clone();
    let sink_prepared = sink.host_handle().lock().unwrap().snapshot().routes[0].clone();
    assert_eq!(prepared, sink_prepared);
    assert_eq!(prepared.epoch, 0);
    assert_eq!(source.approved_end(), Some(original_end));
    let (
        AuthorizationState::Authorized {
            expires_at_ms: Some(initial),
        },
        AuthorizationState::Authorized {
            expires_at_ms: Some(final_expiry),
        },
    ) = (initial_expiry, prepared.authorization)
    else {
        panic!("finite independent grants")
    };
    assert!(final_expiry <= initial);
    assert!(
        source
            .runtime()
            .route_port_lease_claim(route_id, source_port)
            .is_none()
    );
    (source, sink, identity, route_id, source_port, sink_port)
}
fn run_handshake(
    android_accepts: bool,
    receiver_origin: bool,
) -> (
    SecureStereoSourceOwner,
    StereoSinkSupervisor,
    RouteId,
    PortRef,
    PortRef,
) {
    let (source, mut sink, identity, route_id, source_port, sink_port) =
        prepare_pair(android_accepts, receiver_origin);
    let mut owner = source
        .into_owner(identity.source, "localhost".into())
        .unwrap();
    assert!(
        owner
            .runtime()
            .route_port_lease_claim(route_id, source_port)
            .is_none()
    );
    assert_eq!(owner.helper_pid(), None);
    assert_eq!(owner.selected_stream_id(), None);
    let until = Instant::now() + Duration::from_secs(2);
    loop {
        assert!(Instant::now() < until, "crossed Acquire/Start deadline");
        owner.poll().unwrap();
        sink.poll().unwrap();
        if owner.runtime().route(route_id).unwrap().state == RouteState::Starting
            && sink.host_handle().lock().unwrap().snapshot().routes[0].state == RouteState::Starting
        {
            break;
        }
    }
    assert_eq!(owner.runtime().route(route_id).unwrap().epoch, 1);
    assert_eq!(
        sink.host_handle().lock().unwrap().snapshot().routes[0].epoch,
        1
    );
    assert!(
        owner
            .runtime()
            .route_port_lease_claim(route_id, source_port)
            .is_some()
    );
    assert_eq!(owner.helper_pid(), None);
    assert_eq!(owner.packets_sent(), 0);
    (owner, sink, route_id, source_port, sink_port)
}
#[test]
fn source_proposes_android_accepts() {
    let _ = run_handshake(true, false);
}
#[test]
fn source_proposes_android_dials() {
    let _ = run_handshake(false, false);
}
#[test]
fn receiver_proposes_android_accepts() {
    let _ = run_handshake(true, true);
}
#[test]
fn receiver_proposes_android_dials() {
    let _ = run_handshake(false, true);
}

#[test]
fn constructor_failure_returns_terminal_authoritative_runtime() {
    let (source, _sink, identity, id, source_port, _) = prepare_pair(true, false);
    let failure = match source.into_owner(identity.source, String::new()) {
        Err(failure) => failure,
        Ok(_) => panic!("empty server name admitted"),
    };
    let route = failure.runtime.route(id).unwrap();
    assert_eq!(route.state, RouteState::Failed);
    assert_eq!(route.authorization, AuthorizationState::Revoked);
    assert!(
        failure
            .runtime
            .route_port_lease_claim(id, source_port)
            .is_none()
    );
    assert!(
        failure.cleanup_errors.is_empty(),
        "{:?}",
        failure.cleanup_errors
    );
    let mut runtime = failure.runtime;
    assert!(runtime.authorize_route(id, Some(u64::MAX)).is_err());
    assert!(
        runtime
            .prepare_route(
                id,
                Some(FormatDescriptor::new(SECURE_AUDIO_STEREO_FORMAT)),
                QosMode::Interactive,
                1
            )
            .is_err()
    );
}
#[test]
fn host_clone_selection_change_after_stamp_prepare_retires_live_supervisor() {
    let (source, mut sink, _identity, id, _, _) = prepare_pair(true, false);
    let mut cloned = sink.host_handle().lock().unwrap().clone();
    cloned.replace_catalog(&catalog(2)).unwrap();
    assert!(sink.poll().is_err());
    let route = sink
        .host_handle()
        .lock()
        .unwrap()
        .snapshot()
        .routes
        .into_iter()
        .find(|r| r.id == id)
        .unwrap();
    assert_eq!(route.state, RouteState::Failed);
    assert_eq!(route.authorization, AuthorizationState::Revoked);
    drop(source);
}
#[test]
fn host_clone_selection_change_after_finish_retires_original_runtime() {
    let (_owner, mut sink, id, _, _) = run_handshake(true, false);
    let mut cloned = sink.host_handle().lock().unwrap().clone();
    cloned.replace_catalog(&catalog(2)).unwrap();
    assert!(sink.poll().is_err());
    let route = sink
        .host_handle()
        .lock()
        .unwrap()
        .snapshot()
        .routes
        .into_iter()
        .find(|r| r.id == id)
        .unwrap();
    assert_eq!(route.state, RouteState::Failed);
    assert_eq!(route.authorization, AuthorizationState::Revoked);
}
#[test]
fn owner_stop_terminally_retires_b2_grant() {
    let (mut owner, _sink, id, source_port, _) = run_handshake(false, true);
    owner.stop().unwrap();
    let route = owner.runtime().route(id).unwrap();
    assert_eq!(route.state, RouteState::Failed);
    assert_eq!(route.authorization, AuthorizationState::Revoked);
    assert!(
        owner
            .runtime()
            .route_port_lease_claim(id, source_port)
            .is_none()
    );
    assert!(
        owner
            .runtime_mut()
            .authorize_route(id, Some(u64::MAX))
            .is_err()
    );
}

#[test]
fn shorter_source_grant_is_exact_before_prepared() {
    let (source, _sink, _identity, id, _, _) =
        prepare_pair_with_limits(true, false, Duration::from_secs(5), Duration::from_secs(60));
    let AuthorizationState::Authorized {
        expires_at_ms: Some(expiry),
    } = source.runtime().route(id).unwrap().authorization
    else {
        panic!("finite")
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    assert!(expiry > now && expiry <= now + 5000);
}
#[test]
fn shorter_receiver_grant_is_exact_before_prepared() {
    let (source, _sink, _identity, id, _, _) =
        prepare_pair_with_limits(false, true, Duration::from_secs(60), Duration::from_secs(5));
    let AuthorizationState::Authorized {
        expires_at_ms: Some(expiry),
    } = source.runtime().route(id).unwrap().authorization
    else {
        panic!("finite")
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    assert!(expiry > now && expiry <= now + 5000);
}
#[test]
fn delayed_handoff_cannot_restart_original_source_consent() {
    let (source, _sink, identity, id, _, _) =
        prepare_pair_with_limits(true, false, Duration::from_secs(3), Duration::from_secs(60));
    let end = source.approved_end().unwrap();
    thread::sleep(end.saturating_duration_since(Instant::now()) + Duration::from_millis(5));
    let failure = match source.into_owner(identity.source, "localhost".into()) {
        Err(f) => f,
        Ok(_) => panic!("expired original cap admitted"),
    };
    assert_eq!(
        failure.runtime.route(id).unwrap().authorization,
        AuthorizationState::Revoked
    );
    assert_eq!(failure.runtime.route(id).unwrap().state, RouteState::Failed);
}

#[test]
fn bilateral_media_offer_and_clone_revocation_reach_actual_job() {
    let (source, mut sink, identity, id, source_port, _) = prepare_pair(true, false);
    let mut rejected_media_identity = identity.source;
    rejected_media_identity.private_key_der = vec![0];
    let mut owner = source
        .into_owner(rejected_media_identity, "localhost".into())
        .unwrap();
    let until = Instant::now() + Duration::from_secs(2);
    while sink.host_handle().lock().unwrap().snapshot().routes[0].state != RouteState::Starting {
        assert!(Instant::now() < until);
        owner.poll().unwrap();
        sink.poll().unwrap();
    }
    let job = sink.start_media(identity.sink, 7, 23).unwrap();
    assert_eq!(job.snapshot().route().epoch, 1);
    // Decode the real Starting/offer. The intentionally invalid local media
    // key prevents Bound and therefore prevents any capture helper opening.
    while owner.selected_stream_id().is_none() {
        assert!(Instant::now() < until);
        let _ = owner.poll();
    }
    assert_eq!(owner.selected_stream_id(), Some(job.offer().stream_id));
    assert_eq!(owner.helper_pid(), None);
    assert!(job.gate().is_open());
    let mut cloned = sink.host_handle().lock().unwrap().clone();
    cloned.replace_catalog(&catalog(2)).unwrap();
    assert!(!job.gate().is_open());
    assert!(job.try_read(23, &mut [0; 1920]).is_err());
    assert_eq!(job.accepted_bytes(), 0);
    assert!(sink.poll().is_err());
    assert_eq!(
        sink.host_handle().lock().unwrap().snapshot().routes[0].authorization,
        AuthorizationState::Revoked
    );
    owner.stop().unwrap();
    assert!(
        owner
            .runtime()
            .route_port_lease_claim(id, source_port)
            .is_none()
    );
    assert_eq!(owner.helper_pid(), None);
    let until = Instant::now() + Duration::from_secs(2);
    while !job.worker_exited() {
        assert!(Instant::now() < until);
        thread::sleep(Duration::from_millis(1));
    }
    job.reap_finished();
}

/// Real data TLS and synthetic PCM, with independently approved Runtime grants.
/// No PipeWire helper, Android SDK, AudioTrack or physical playback.
#[test]
fn bilateral_real_pcm_permit_cannot_ack_after_clone_revocation() {
    use capyio_android_jni::stereo_job::{BOUND, PENDING};
    use capyio_audio::{AudioMediaPacket, AudioMediaStreamBinding, AudioStreamSpec};
    use capyio_control_tls::audio_stereo::StereoAudioTlsChannel;
    use capyio_native_audio_lan::SecureStereoPcmAdmission;
    let (source, mut sink, identity, _id, _, _) = prepare_pair(false, true);
    let mut owner = source
        .into_owner(identity.source.clone(), "localhost".into())
        .unwrap();
    let until = Instant::now() + Duration::from_secs(2);
    while sink.host_handle().lock().unwrap().snapshot().routes[0].state != RouteState::Starting {
        assert!(Instant::now() < until);
        owner.poll().unwrap();
        sink.poll().unwrap();
    }
    let job = sink.start_media(identity.sink, 7, 23).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    // Independent synthetic sender uses actual negotiated job binding/pins.
    let mut media = StereoAudioTlsChannel::connect_until(
        ControlTlsEndpoint {
            address: job.listener_address(),
            io_timeout: Duration::from_millis(250),
        },
        "localhost",
        identity.source,
        job.snapshot().binding(now).unwrap(),
        job.operation_deadline(),
    )
    .unwrap();
    while job.state() == PENDING {
        assert!(Instant::now() < until);
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(job.state(), BOUND);
    job.enable().unwrap();
    let route = job.snapshot().route();
    let binding = AudioMediaStreamBinding {
        session_id: route.session_id,
        route_id: route.id,
        stream_id: job.offer().stream_id,
        stream_epoch: route.epoch as u32,
        selected_spec: AudioStreamSpec::media_balanced(),
    };
    let pcm: Vec<u8> = (0..480).flat_map(|_| [0x12, 0x34, 0x56, 0x78]).collect();
    let mut encoder = SecureStereoPcmAdmission::new(binding.clone()).unwrap();
    let record = encoder
        .encode_record(&AudioMediaPacket {
            stream_id: binding.stream_id,
            stream_epoch: binding.stream_epoch,
            sequence: 0,
            source_timestamp_micros: 100,
            first_sample_index: 0,
            sample_count: 480,
            discontinuity: false,
            payload: pcm.clone(),
        })
        .unwrap();
    media.send_record(&record).unwrap();
    let mut bytes = [0; 1920];
    let permit = loop {
        assert!(Instant::now() < until);
        if let Some(p) = job.try_read(23, &mut bytes).unwrap() {
            break p;
        }
        thread::sleep(Duration::from_millis(1));
    };
    assert_eq!(bytes.as_slice(), pcm.as_slice());
    assert_eq!(job.accepted_bytes(), 0);
    let mut cloned = sink.host_handle().lock().unwrap().clone();
    cloned.replace_catalog(&catalog(2)).unwrap();
    assert!(
        job.acknowledge(23, permit.token, permit.offset, permit.length)
            .is_err()
    );
    assert_eq!(job.accepted_bytes(), 0);
    assert!(sink.poll().is_err());
    assert_eq!(
        sink.host_handle().lock().unwrap().snapshot().routes[0].state,
        RouteState::Failed
    );
    owner.stop().unwrap();
    assert_eq!(owner.helper_pid(), None);
    let until = Instant::now() + Duration::from_secs(2);
    while !job.worker_exited() {
        assert!(Instant::now() < until);
        thread::sleep(Duration::from_millis(1));
    }
    job.reap_finished();
}
