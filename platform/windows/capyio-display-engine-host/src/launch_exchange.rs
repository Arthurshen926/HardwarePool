use capyio_display_stream_adapter::{
    ValidatedDisplayStreamSession,
    launch_record::{
        GAMESTREAM_LAUNCH_ACK_BYTES, GAMESTREAM_LAUNCH_RECORD_BYTES, GameStreamLaunchBinding,
        GameStreamLaunchSecrets, encode_gamestream_launch_ack, encode_gamestream_launch_record,
    },
};
use capyio_display_tls::AuthenticatedGameStreamLaunchServer;

use crate::{AuthenticatedEngineLaunch, DisplayEngineLoadError};

struct SensitiveLaunchRecord([u8; GAMESTREAM_LAUNCH_RECORD_BYTES]);

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
}
