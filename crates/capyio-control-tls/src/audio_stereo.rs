//! Dedicated pinned TLS stereo audio transport. Each u16-prefixed record contains
//! exactly two canonical CPYA fragments (1200 + 928 bytes), one 1920-byte PCM frame.
//! Fragments are admitted atomically by SecureStereoPcmAdmission. The owner
//! must separately freeze the selected format and complete AudioStreamSpec/QoS:
//! those are not carried in the existing 208-byte route binding.
//! Socket-based inherited tests are hosted-only, like the camera TLS suite.
use crate::route_tls::route_tls;
route_tls!(
    StereoAudioTlsBinding,
    StereoAudioTlsListener,
    StereoAudioTlsChannel,
    StereoAudioTlsStopHandle,
    AUDIO_STEREO_TLS_ALPN,
    MAX_AUDIO_STEREO_TLS_RECORD_BYTES,
    b"capyio-audio-stereo/1",
    b"CPYAST01",
    "capyio.audio.frames",
    2128,
    2128,
    u16,
    true
);

#[cfg(test)]
mod hosted_writer_tests {
    use super::*;
    use capyio_core::{CapabilityId, PortId};
    use std::{sync::mpsc, thread};

    fn pending_binding() -> StereoAudioTlsBinding {
        let session =
            ControlTlsBinding::new(SessionId::new(), NodeId::new(), NodeId::new()).unwrap();
        StereoAudioTlsBinding {
            session,
            route_id: RouteId::new(),
            source: PortRef {
                node_id: session.client_node_id,
                capability_id: CapabilityId::new(),
                port_id: PortId::new(),
            },
            sink: PortRef {
                node_id: session.server_node_id,
                capability_id: CapabilityId::new(),
                port_id: PortId::new(),
            },
            stream_id: StreamId::new(),
            epoch: 1,
            expires_at_ms: now_ms().unwrap() + 10_000,
        }
    }

    #[test]
    fn ipc_stereo_audio_stop_at_registration_sends_no_tls_or_binding_bytes() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let (_, identity) = crate::tests::test_identities();
        let mut registrations = 0;
        let result = StereoAudioTlsChannel::connect_until_with_stop(
            ControlTlsEndpoint {
                address: listener.local_addr().unwrap(),
                io_timeout: Duration::from_secs(2),
            },
            "localhost",
            identity,
            pending_binding(),
            Instant::now() + Duration::from_secs(3),
            |stop| {
                registrations += 1;
                stop.stop();
            },
        );
        assert!(result.is_err());
        assert_eq!(registrations, 1);
        let (mut peer, _) = listener.accept().unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
        assert_eq!(peer.read(&mut [0; 1]).unwrap(), 0);
    }

    #[test]
    fn ipc_stereo_audio_registered_stop_interrupts_actual_pending_tls_handshake() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (_, identity) = crate::tests::test_identities();
        let binding = pending_binding();
        let (registered_tx, registered_rx) = mpsc::sync_channel(1);
        let (finished_tx, finished_rx) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            let result = StereoAudioTlsChannel::connect_until_with_stop(
                ControlTlsEndpoint {
                    address,
                    io_timeout: Duration::from_secs(3),
                },
                "localhost",
                identity,
                binding,
                Instant::now() + Duration::from_secs(4),
                |stop| registered_tx.send(stop).unwrap(),
            );
            finished_tx.send(result.is_err()).unwrap();
        });
        let (mut peer, _) = listener.accept().unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let stop = registered_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        // Establish that the real TLS ClientHello has begun, not just that a
        // pending worker was cancelled before entering any socket operation.
        assert_eq!(peer.read(&mut [0; 1]).unwrap(), 1);
        let before = Instant::now();
        stop.stop();
        assert!(finished_rx.recv_timeout(Duration::from_secs(1)).unwrap());
        worker.join().unwrap();
        assert!(before.elapsed() < Duration::from_secs(1));
    }

    /// Hosted-only real TLS: no local execution in restricted environments.
    /// A fixed attempt count and one reused record keep test work/allocation
    /// bounded even on a host with unexpectedly large kernel socket buffers.
    #[test]
    fn ipc_stereo_audio_blocked_writer_preserves_absolute_deadline_and_is_terminal() {
        blocked_writer(false);
    }

    #[test]
    fn ipc_stereo_audio_stop_interrupts_verified_saturated_tls_writer() {
        blocked_writer(true);
    }

    fn blocked_writer(cancel: bool) {
        let session =
            ControlTlsBinding::new(SessionId::new(), NodeId::new(), NodeId::new()).unwrap();
        let expected = StereoAudioTlsBinding {
            session,
            route_id: RouteId::new(),
            source: PortRef {
                node_id: session.client_node_id,
                capability_id: CapabilityId::new(),
                port_id: PortId::new(),
            },
            sink: PortRef {
                node_id: session.server_node_id,
                capability_id: CapabilityId::new(),
                port_id: PortId::new(),
            },
            stream_id: StreamId::new(),
            epoch: 1,
            expires_at_ms: now_ms().unwrap() + 10_000,
        };
        let (identity, client) = crate::tests::test_identities();
        let lifetime = Instant::now() + Duration::from_secs(5);
        let server = StereoAudioTlsListener::from_listener_until(
            TcpListener::bind("127.0.0.1:0").unwrap(),
            identity,
            expected,
            Duration::from_secs(2),
            lifetime,
        )
        .unwrap();
        let address = server.local_addr().unwrap();
        let (ready_tx, ready_rx) = mpsc::sync_channel(0);
        let (release_tx, release_rx) = mpsc::sync_channel(0);
        let receiver = thread::spawn(move || {
            let end = Instant::now() + Duration::from_secs(2);
            let mut channel = loop {
                if let Some(channel) = server.try_accept_until(end).unwrap() {
                    break channel;
                }
                thread::sleep(Duration::from_millis(2));
            };
            ready_tx.send(()).unwrap();
            // Complete the binding but deliberately consume no application data.
            // Even an assertion failure/disconnected sender releases this owner.
            let _ = release_rx.recv_timeout(Duration::from_secs(3));
            channel.close();
        });
        let mut sender = StereoAudioTlsChannel::connect_until(
            ControlTlsEndpoint {
                address,
                io_timeout: Duration::from_secs(2),
            },
            "localhost",
            client,
            expected,
            lifetime,
        )
        .unwrap();
        ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        // Handshake writes are not evidence of blocked application PCM writes.
        sender
            .io
            .socket()
            .observed_write_backpressure
            .store(false, Ordering::Release);
        let before = Instant::now();
        let operation_end = before
            + if cancel {
                Duration::from_secs(2)
            } else {
                Duration::from_millis(500)
            };
        let stopper = if cancel {
            let stop = sender.stop_handle().unwrap();
            let observed = sender.io.socket().observed_write_backpressure.clone();
            Some(thread::spawn(move || {
                let end = Instant::now() + Duration::from_secs(1);
                while !observed.load(Ordering::Acquire) && Instant::now() < end {
                    thread::sleep(Duration::from_millis(1));
                }
                let established = observed.load(Ordering::Acquire);
                let at = Instant::now();
                stop.stop();
                (established, at)
            }))
        } else {
            None
        };
        let record = [0x5a; MAX_AUDIO_STEREO_TLS_RECORD_BYTES];
        let mut failure = None;
        for _ in 0..65_536 {
            if let Err(error) = sender.send_record_until(&record, operation_end) {
                failure = Some(error);
                break;
            }
        }
        let failed_at = Instant::now();
        let elapsed = failed_at.duration_since(before);
        let backpressure = sender
            .io
            .socket()
            .observed_write_backpressure
            .load(Ordering::Acquire);
        let terminal = matches!(
            sender.send_record_until(&record, operation_end),
            Err(ControlTlsError::ChannelFailed)
        );
        let _ = release_tx.send(());
        receiver.join().unwrap();
        assert!(
            backpressure,
            "test must establish actual socket write saturation"
        );
        if let Some(stopper) = stopper {
            let (established, stopped_at) = stopper.join().unwrap();
            assert!(established, "Stop must follow actual write backpressure");
            assert!(failed_at >= stopped_at);
            assert!(failed_at.duration_since(stopped_at) < Duration::from_secs(1));
            assert!(matches!(failure, Some(ControlTlsError::Io(ref error))
                if matches!(error.kind(), std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::BrokenPipe)));
        } else {
            assert!(
                matches!(failure, Some(ControlTlsError::Io(ref error)) if error.kind() == std::io::ErrorKind::TimedOut)
            );
        }
        assert!(
            elapsed < Duration::from_secs(2),
            "saturation must not renew the operation budget"
        );
        assert!(
            terminal,
            "timed-out partial TLS writes must never be retried"
        );
    }
}

#[cfg(test)]
mod hosted_contract_tests {
    use super::*;
    use capyio_audio::{AudioMediaPacket, AudioMediaStreamBinding, AudioStreamSpec};
    use capyio_core::{CapabilityId, PortId};
    use capyio_native_audio_lan::{SECURE_STEREO_PCM_RECORD_BYTES, SecureStereoPcmAdmission};
    use std::{sync::mpsc, thread};

    fn binding() -> StereoAudioTlsBinding {
        let session =
            ControlTlsBinding::new(SessionId::new(), NodeId::new(), NodeId::new()).unwrap();
        StereoAudioTlsBinding {
            session,
            route_id: RouteId::new(),
            source: PortRef {
                node_id: session.client_node_id,
                capability_id: CapabilityId::new(),
                port_id: PortId::new(),
            },
            sink: PortRef {
                node_id: session.server_node_id,
                capability_id: CapabilityId::new(),
                port_id: PortId::new(),
            },
            stream_id: StreamId::new(),
            epoch: 1,
            expires_at_ms: now_ms().unwrap() + 10_000,
        }
    }

    fn media_binding(binding: StereoAudioTlsBinding) -> AudioMediaStreamBinding {
        AudioMediaStreamBinding {
            session_id: binding.session.session_id,
            route_id: binding.route_id,
            stream_id: binding.stream_id,
            stream_epoch: binding.epoch as u32,
            selected_spec: AudioStreamSpec::media_balanced(),
        }
    }

    fn packet(binding: StereoAudioTlsBinding, sequence: u64) -> AudioMediaPacket {
        AudioMediaPacket {
            stream_id: binding.stream_id,
            stream_epoch: binding.epoch as u32,
            sequence,
            source_timestamp_micros: 12345 + sequence * 10_000,
            first_sample_index: sequence * 480,
            sample_count: 480,
            discontinuity: false,
            payload: (0..480_i16)
                .flat_map(|sample| {
                    let left = sample.to_le_bytes();
                    let right = (-sample - 1000).to_le_bytes();
                    [left[0], left[1], right[0], right[1]]
                })
                .collect(),
        }
    }

    fn server(
        identity: ControlTlsServerIdentity,
        binding: StereoAudioTlsBinding,
    ) -> StereoAudioTlsListener {
        StereoAudioTlsListener::from_listener(
            TcpListener::bind("127.0.0.1:0").unwrap(),
            identity,
            binding,
            Duration::from_secs(2),
        )
        .unwrap()
    }

    fn accept(server: StereoAudioTlsListener) -> Result<StereoAudioTlsChannel, ControlTlsError> {
        let end = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(channel) = server.try_accept_until(end)? {
                return Ok(channel);
            }
            thread::sleep(Duration::from_millis(2));
        }
    }

    fn endpoint(address: SocketAddr) -> ControlTlsEndpoint {
        ControlTlsEndpoint {
            address,
            io_timeout: Duration::from_secs(2),
        }
    }

    fn mono_binding(binding: StereoAudioTlsBinding) -> crate::audio::AudioTlsBinding {
        crate::audio::AudioTlsBinding {
            session: binding.session,
            route_id: binding.route_id,
            source: binding.source,
            sink: binding.sink,
            stream_id: binding.stream_id,
            epoch: binding.epoch,
            expires_at_ms: binding.expires_at_ms,
        }
    }

    #[test]
    fn ipc_stereo_actual_tls_preserves_channels_then_replay_is_terminal() {
        for corruption in 0..4 {
            let binding = binding();
            let (identity, client) = crate::tests::test_identities();
            let server = server(identity, binding);
            let address = server.local_addr().unwrap();
            let receiver = thread::spawn(move || {
                let mut channel = accept(server).unwrap();
                let mut admission = SecureStereoPcmAdmission::new(media_binding(binding)).unwrap();
                for sequence in 0..4 {
                    let record = channel.receive_record().unwrap();
                    assert_eq!(record.len(), 2128);
                    assert_eq!(
                        admission.admit_record(&record).unwrap(),
                        packet(binding, sequence)
                    );
                }
                let invalid = channel.receive_record().unwrap();
                assert!(admission.admit_record(&invalid).is_err());
                assert!(admission.terminal());
                channel.close();
                assert!(matches!(
                    channel.receive_record(),
                    Err(ControlTlsError::ChannelFailed)
                ));
            });
            let mut sender =
                StereoAudioTlsChannel::connect(endpoint(address), "localhost", client, binding)
                    .unwrap();
            let mut encoder = SecureStereoPcmAdmission::new(media_binding(binding)).unwrap();
            let mut final_record = [0; SECURE_STEREO_PCM_RECORD_BYTES];
            for sequence in 0..4 {
                final_record = encoder.encode_record(&packet(binding, sequence)).unwrap();
                sender.send_record(&final_record).unwrap();
            }
            if corruption != 0 {
                final_record = encoder.encode_record(&packet(binding, 4)).unwrap();
                // Complete TLS records with a malformed second fragment must not
                // publish even the already-valid first fragment.
                match corruption {
                    1 => final_record[1208] ^= 1, // mismatched Session
                    2 => final_record[1268] ^= 1, // mismatched initial PTS byte
                    _ => final_record[1302] = 1,  // reserved second-header bits
                }
            }
            sender.send_record(&final_record).unwrap();
            receiver.join().unwrap();
        }
    }

    #[test]
    fn ipc_stereo_and_mono_alpn_cannot_cross_even_with_same_identity_and_binding() {
        let binding = binding();
        let (identity, client) = crate::tests::test_identities();
        let server = server(identity.clone(), binding);
        let address = server.local_addr().unwrap();
        let receiver = thread::spawn(move || accept(server));
        assert!(
            crate::audio::AudioTlsChannel::connect(
                endpoint(address),
                "localhost",
                client.clone(),
                mono_binding(binding)
            )
            .is_err()
        );
        assert!(receiver.join().unwrap().is_err());
        let server = crate::audio::AudioTlsListener::from_listener(
            TcpListener::bind("127.0.0.1:0").unwrap(),
            identity,
            mono_binding(binding),
            Duration::from_secs(2),
        )
        .unwrap();
        let address = server.local_addr().unwrap();
        let receiver = thread::spawn(move || {
            let end = Instant::now() + Duration::from_secs(3);
            loop {
                match server.try_accept_until(end) {
                    Err(error) => return error,
                    Ok(None) => thread::sleep(Duration::from_millis(2)),
                    Ok(Some(_)) => panic!("stereo client entered mono ALPN"),
                }
            }
        });
        assert!(
            StereoAudioTlsChannel::connect(endpoint(address), "localhost", client, binding)
                .is_err()
        );
        receiver.join().unwrap();
    }

    #[test]
    fn ipc_stereo_invalid_record_length_and_partial_pair_close_without_payload_admission() {
        for size in [1064_u16, 1200, 2127, 2128, 2129] {
            let binding = binding();
            let (identity, client) = crate::tests::test_identities();
            let server = server(identity, binding);
            let address = server.local_addr().unwrap();
            let (ready_tx, ready_rx) = mpsc::sync_channel(0);
            let receiver = thread::spawn(move || {
                let mut channel = accept(server).unwrap();
                ready_tx.send(()).unwrap();
                assert!(
                    channel
                        .receive_record_until(Instant::now() + Duration::from_millis(200))
                        .is_err()
                );
                assert!(matches!(
                    channel.receive_record(),
                    Err(ControlTlsError::ChannelFailed)
                ));
            });
            let mut sender =
                StereoAudioTlsChannel::connect(endpoint(address), "localhost", client, binding)
                    .unwrap();
            ready_rx.recv_timeout(Duration::from_secs(1)).unwrap();
            sender.io.write_all(&size.to_be_bytes()).unwrap();
            // A complete first CPYA fragment is still not a stereo record.
            if size == 2128 {
                let mut encoder = SecureStereoPcmAdmission::new(media_binding(binding)).unwrap();
                let record = encoder.encode_record(&packet(binding, 0)).unwrap();
                sender.io.write_all(&record[..1200]).unwrap();
            }
            sender.io.flush().unwrap();
            receiver.join().unwrap();
        }
    }

    #[test]
    fn pure_stereo_framing_alpn_magic_and_mono_contract_are_distinct() {
        assert_eq!(MAX_AUDIO_STEREO_TLS_RECORD_BYTES, 2128);
        assert_eq!(crate::audio::MAX_AUDIO_TLS_RECORD_BYTES, 1064);
        assert_ne!(AUDIO_STEREO_TLS_ALPN, crate::audio::AUDIO_TLS_ALPN);
        assert!(
            require_protocol(
                Some(rustls::ProtocolVersion::TLSv1_3),
                Some(crate::audio::AUDIO_TLS_ALPN)
            )
            .is_err()
        );
        let binding = binding();
        assert_eq!(&binding.encode(false)[..8], b"CPYAST01");
        assert_eq!(binding.encode(false).len(), 208);
    }
}
