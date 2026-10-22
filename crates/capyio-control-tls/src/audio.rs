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

    /// Hosted-only real TLS: no local execution in restricted environments.
    /// A fixed attempt count and one reused record keep test work/allocation
    /// bounded even on a host with unexpectedly large kernel socket buffers.
    #[test]
    fn ipc_audio_blocked_writer_preserves_absolute_deadline_and_is_terminal() {
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
        let before = Instant::now();
        let operation_end = before + Duration::from_millis(500);
        let record = [0x5a; MAX_AUDIO_TLS_RECORD_BYTES];
        let mut failure = None;
        for _ in 0..65_536 {
            if let Err(error) = sender.send_record_until(&record, operation_end) {
                failure = Some(error);
                break;
            }
        }
        let elapsed = before.elapsed();
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
        assert!(
            matches!(failure, Some(ControlTlsError::Io(ref error)) if error.kind() == std::io::ErrorKind::TimedOut)
        );
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
