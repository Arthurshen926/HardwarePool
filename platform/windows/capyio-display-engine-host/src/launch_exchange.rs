use capyio_display_stream_adapter::{
    ValidatedDisplayStreamSession,
    launch_record::{
        GAMESTREAM_LAUNCH_ACK_BYTES, GAMESTREAM_LAUNCH_RECORD_BYTES, GameStreamLaunchBinding,
        GameStreamLaunchSecrets, encode_gamestream_launch_ack, encode_gamestream_launch_record,
    },
};
use capyio_display_tls::AuthenticatedGameStreamLaunchServer;

use crate::{AuthenticatedEngineLaunch, DisplayEngineLoadError};
use capyio_control_tls::display::{DisplayLaunchTlsBinding, DisplayLaunchTlsChannel};

fn matches_paired_route(
    peer: DisplayLaunchTlsBinding,
    session: &ValidatedDisplayStreamSession,
    binding: GameStreamLaunchBinding,
) -> bool {
    peer.session.session_id.as_uuid().as_bytes() == &binding.session_id
        && peer.route_id.as_uuid().as_bytes() == &binding.route_id
        && peer.stream_id.as_uuid().as_bytes() == &binding.stream_id
        && peer.epoch == binding.stream_epoch
        && peer.sink.capability_id.as_uuid().as_bytes() == &session.request().display_capability_id
}

/// Reuses a node-pinned, finite-grant Route channel. Construction of a launch
/// still requires the receiver's exact ACK; no raw key bypass is exposed.
pub fn exchange_paired_node_engine_launch(
    channel: &mut DisplayLaunchTlsChannel,
    session: &ValidatedDisplayStreamSession,
    binding: GameStreamLaunchBinding,
    secrets: GameStreamLaunchSecrets,
) -> Result<AuthenticatedEngineLaunch, DisplayEngineLoadError> {
    if !matches_paired_route(channel.binding(), session, binding) {
        channel.close();
        return Err(DisplayEngineLoadError::LaunchTransport(
            "display peer Route differs from engine binding".into(),
        ));
    }
    let result = exchange_then_build(session, binding, secrets, |record, acknowledgement| {
        channel
            .send_record(record)
            .map_err(|error| DisplayEngineLoadError::LaunchTransport(error.to_string()))?;
        let received = channel
            .receive_record()
            .map_err(|error| DisplayEngineLoadError::LaunchTransport(error.to_string()))?;
        if received.as_slice() != acknowledgement {
            return Err(DisplayEngineLoadError::LaunchTransport(
                "display peer acknowledgement mismatch".into(),
            ));
        }
        Ok(())
    });
    // One authorized launch per channel; a reconnect needs a fresh handshake.
    channel.close();
    result
}

struct SensitiveLaunchRecord([u8; GAMESTREAM_LAUNCH_RECORD_BYTES]);

/// One-use launch held by the service while its authorized node process
/// relays the record over the already authenticated peer control connection.
pub struct PendingServiceEngineLaunch {
    record: SensitiveLaunchRecord,
    acknowledgement: [u8; GAMESTREAM_LAUNCH_ACK_BYTES],
    launch: AuthenticatedEngineLaunch,
}
impl PendingServiceEngineLaunch {
    pub fn prepare(
        session: &ValidatedDisplayStreamSession,
        binding: GameStreamLaunchBinding,
        secrets: GameStreamLaunchSecrets,
    ) -> Result<Self, DisplayEngineLoadError> {
        let record = SensitiveLaunchRecord(
            encode_gamestream_launch_record(session, binding, &secrets)
                .map_err(|e| DisplayEngineLoadError::LaunchRecord(e.to_string()))?,
        );
        let acknowledgement = encode_gamestream_launch_ack(binding, secrets.launch_session_id)
            .map_err(|e| DisplayEngineLoadError::LaunchRecord(e.to_string()))?;
        let launch = AuthenticatedEngineLaunch::new(
            session.request().adapter_instance_id,
            session.request().display_capability_id,
            secrets.launch_session_id,
            secrets.key,
            secrets.key_id,
            secrets.ping_payload,
            secrets.control_connect_data,
        )?;
        launch.validate_for(session)?;
        Ok(Self {
            record,
            acknowledgement,
            launch,
        })
    }
    pub fn record(&self) -> &[u8; GAMESTREAM_LAUNCH_RECORD_BYTES] {
        &self.record.0
    }
    pub fn commit(
        self,
        acknowledgement: &[u8],
    ) -> Result<AuthenticatedEngineLaunch, DisplayEngineLoadError> {
        if acknowledgement != self.acknowledgement {
            return Err(DisplayEngineLoadError::LaunchTransport(
                "display acknowledgement mismatch".into(),
            ));
        }
        Ok(self.launch)
    }
}

impl Drop for SensitiveLaunchRecord {
    fn drop(&mut self) {
        self.0.fill(0);
        std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
        std::hint::black_box(&self.0);
    }
}

pub fn exchange_authenticated_engine_launch(
    server: &AuthenticatedGameStreamLaunchServer,
    session: &ValidatedDisplayStreamSession,
    binding: GameStreamLaunchBinding,
    secrets: GameStreamLaunchSecrets,
) -> Result<AuthenticatedEngineLaunch, DisplayEngineLoadError> {
    exchange_then_build(session, binding, secrets, |record, acknowledgement| {
        server
            .exchange(record, acknowledgement)
            .map_err(|error| DisplayEngineLoadError::LaunchTransport(error.to_string()))
    })
}

/// Nonblocking-listener variant used by the service event loop. `Ok(None)`
/// means no authenticated transport was waiting and consumes no launch owner.
pub fn try_exchange_authenticated_engine_launch(
    server: &AuthenticatedGameStreamLaunchServer,
    session: &ValidatedDisplayStreamSession,
    binding: GameStreamLaunchBinding,
    secrets: GameStreamLaunchSecrets,
) -> Result<Option<AuthenticatedEngineLaunch>, DisplayEngineLoadError> {
    let mut accepted = false;
    let launch = exchange_then_build(session, binding, secrets, |record, acknowledgement| {
        accepted = server
            .try_exchange(record, acknowledgement)
            .map_err(|error| DisplayEngineLoadError::LaunchTransport(error.to_string()))?;
        Ok(())
    })?;
    if accepted { Ok(Some(launch)) } else { Ok(None) }
}

fn exchange_then_build<F>(
    session: &ValidatedDisplayStreamSession,
    binding: GameStreamLaunchBinding,
    secrets: GameStreamLaunchSecrets,
    exchange: F,
) -> Result<AuthenticatedEngineLaunch, DisplayEngineLoadError>
where
    F: FnOnce(
        &[u8; GAMESTREAM_LAUNCH_RECORD_BYTES],
        &[u8; GAMESTREAM_LAUNCH_ACK_BYTES],
    ) -> Result<(), DisplayEngineLoadError>,
{
    let record = SensitiveLaunchRecord(
        encode_gamestream_launch_record(session, binding, &secrets)
            .map_err(|error| DisplayEngineLoadError::LaunchRecord(error.to_string()))?,
    );
    let acknowledgement = encode_gamestream_launch_ack(binding, secrets.launch_session_id)
        .map_err(|error| DisplayEngineLoadError::LaunchRecord(error.to_string()))?;
    exchange(&record.0, &acknowledgement)?;
    let launch = AuthenticatedEngineLaunch::new(
        session.request().adapter_instance_id,
        session.request().display_capability_id,
        secrets.launch_session_id,
        secrets.key,
        secrets.key_id,
        secrets.ping_payload,
        secrets.control_connect_data,
    )?;
    launch.validate_for(session)?;
    Ok(launch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use capyio_display_stream_adapter::{
        DisplayMode, DisplayOnlyPolicy, DisplayStreamSessionRequest, DynamicRange, VideoCodec,
    };
    use std::net::Ipv4Addr;

    fn session() -> ValidatedDisplayStreamSession {
        ValidatedDisplayStreamSession::new(DisplayStreamSessionRequest {
            adapter_instance_id: [1; 16],
            display_capability_id: [2; 16],
            mode: DisplayMode {
                width: 1920,
                height: 1080,
                refresh_millihertz: 60000,
            },
            codec: VideoCodec::Hevc,
            dynamic_range: DynamicRange::Sdr8,
            bitrate_kbps: 30000,
            port_base: 50000,
            policy: DisplayOnlyPolicy::CAPYIO,
        })
        .unwrap()
    }

    fn binding() -> GameStreamLaunchBinding {
        GameStreamLaunchBinding {
            session_id: [3; 16],
            route_id: [4; 16],
            stream_id: [5; 16],
            stream_epoch: 6,
            host_ipv4: Ipv4Addr::LOCALHOST,
        }
    }

    fn secrets() -> GameStreamLaunchSecrets {
        GameStreamLaunchSecrets {
            launch_session_id: 7,
            key: [8; 16],
            key_id: 9,
            ping_payload: [10; 8],
            control_connect_data: 11,
        }
    }

    #[test]
    fn engine_launch_exists_only_after_exchange_success() {
        let session = session();
        let rejected = exchange_then_build(&session, binding(), secrets(), |_, _| {
            Err(DisplayEngineLoadError::LaunchTransport(
                "rejected".to_owned(),
            ))
        });
        assert!(matches!(
            rejected,
            Err(DisplayEngineLoadError::LaunchTransport(_))
        ));
        let accepted = exchange_then_build(&session, binding(), secrets(), |record, ack| {
            assert_eq!(&record[..8], b"CPYGS001");
            assert_eq!(&ack[..8], b"CPYGSA01");
            Ok(())
        })
        .unwrap();
        accepted.validate_for(&session).unwrap();
    }

    #[test]
    fn service_pending_launch_requires_exact_ack() {
        let pending =
            PendingServiceEngineLaunch::prepare(&session(), binding(), secrets()).unwrap();
        assert_eq!(pending.record().len(), 176);
        assert!(pending.commit(&[0; 64]).is_err());
        let pending =
            PendingServiceEngineLaunch::prepare(&session(), binding(), secrets()).unwrap();
        let ack = encode_gamestream_launch_ack(binding(), secrets().launch_session_id).unwrap();
        pending
            .commit(&ack)
            .unwrap()
            .validate_for(&session())
            .unwrap();
        let pending =
            PendingServiceEngineLaunch::prepare(&session(), binding(), secrets()).unwrap();
        let wrong = encode_gamestream_launch_ack(
            GameStreamLaunchBinding {
                stream_epoch: 7,
                ..binding()
            },
            secrets().launch_session_id,
        )
        .unwrap();
        assert!(pending.commit(&wrong).is_err());
    }

    #[test]
    fn node_launch_requires_matching_route_epoch_and_sink() {
        let binding = binding();
        let session = session();
        let peer = DisplayLaunchTlsBinding {
            session: capyio_control_tls::ControlTlsBinding::new(
                "03030303-0303-0303-0303-030303030303".parse().unwrap(),
                "11111111-1111-1111-1111-111111111111".parse().unwrap(),
                "22222222-2222-2222-2222-222222222222".parse().unwrap(),
            )
            .unwrap(),
            route_id: "04040404-0404-0404-0404-040404040404".parse().unwrap(),
            stream_id: "05050505-0505-0505-0505-050505050505".parse().unwrap(),
            source: capyio_core::PortRef {
                node_id: "11111111-1111-1111-1111-111111111111".parse().unwrap(),
                capability_id: "33333333-3333-3333-3333-333333333333".parse().unwrap(),
                port_id: "44444444-4444-4444-4444-444444444444".parse().unwrap(),
            },
            sink: capyio_core::PortRef {
                node_id: "22222222-2222-2222-2222-222222222222".parse().unwrap(),
                capability_id: "02020202-0202-0202-0202-020202020202".parse().unwrap(),
                port_id: "55555555-5555-5555-5555-555555555555".parse().unwrap(),
            },
            epoch: 6,
            expires_at_ms: u64::MAX,
        };
        assert!(matches_paired_route(peer, &session, binding));
        assert!(!matches_paired_route(
            DisplayLaunchTlsBinding { epoch: 7, ..peer },
            &session,
            binding
        ));
        let mut changed = peer;
        changed.sink.capability_id = "99999999-9999-9999-9999-999999999999".parse().unwrap();
        assert!(!matches_paired_route(changed, &session, binding));
    }
}
