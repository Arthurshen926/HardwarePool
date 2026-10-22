//! Dedicated pinned TLS mono audio transport. Each u16-prefixed record contains
//! exactly one canonical 104-byte CPYA header plus 960 bytes of PCM. The owner
//! must separately freeze the selected format and complete AudioStreamSpec/QoS:
//! those are not carried in the existing 208-byte route binding.
//! Socket-based inherited tests are hosted-only, like the camera TLS suite.
use crate::route_tls::route_tls;
route_tls!(
    AudioTlsBinding,
    AudioTlsListener,
    AudioTlsChannel,
    AudioTlsStopHandle,
    AUDIO_TLS_ALPN,
    MAX_AUDIO_TLS_RECORD_BYTES,
    b"capyio-audio/1",
    b"CPYAUD01",
    "capyio.audio.frames",
    1064,
    1064,
    u16,
    true
);

#[cfg(test)]
mod hosted_writer_tests {
    use super::*;
    use capyio_core::{CapabilityId, PortId};
    use std::{sync::mpsc, thread};

    fn pending_binding() -> AudioTlsBinding {
        let session =
            ControlTlsBinding::new(SessionId::new(), NodeId::new(), NodeId::new()).unwrap();
        AudioTlsBinding {
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
    fn ipc_audio_stop_at_registration_sends_no_tls_or_binding_bytes() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let (_, identity) = crate::tests::test_identities();
        let mut registrations = 0;
        let result = AudioTlsChannel::connect_until_with_stop(
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
    fn ipc_audio_registered_stop_interrupts_actual_pending_tls_handshake() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (_, identity) = crate::tests::test_identities();
        let binding = pending_binding();
        let (registered_tx, registered_rx) = mpsc::sync_channel(1);
        let (finished_tx, finished_rx) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            let result = AudioTlsChannel::connect_until_with_stop(
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
    fn ipc_audio_blocked_writer_preserves_absolute_deadline_and_is_terminal() {
        blocked_writer(false);
    }

    #[test]
    fn ipc_audio_stop_interrupts_verified_saturated_tls_writer() {
        blocked_writer(true);
    }

    fn blocked_writer(cancel: bool) {
        let session =
            ControlTlsBinding::new(SessionId::new(), NodeId::new(), NodeId::new()).unwrap();
        let expected = AudioTlsBinding {
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
        let server = AudioTlsListener::from_listener_until(
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
        let mut sender = AudioTlsChannel::connect_until(
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
        let record = [0x5a; MAX_AUDIO_TLS_RECORD_BYTES];
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
