use capyio_display_tls::{AuthenticatedDisplaySender, AuthenticatedDisplayServer};
use capyio_display_transport::DisplayTransportBinding;
use capyio_windows_display::{IddMonitorIdentity, IddMonitorMode};

use crate::{
    DisplayProjectionControl, DisplayRouteProjectionError, DisplayRouteProjectionRuntime,
    DisplayRouteProjectionSnapshot, DisplayRouteProjectionSpec,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DisplaySessionWorkerError {
    Authentication(String),
    InvalidSelectedMode,
    Projection(DisplayRouteProjectionError),
    Session(String),
    Cleanup(DisplayRouteProjectionError),
    SessionAndCleanup {
        session: String,
        cleanup: DisplayRouteProjectionError,
    },
}

impl std::fmt::Display for DisplaySessionWorkerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Authentication(_) => {
                formatter.write_str("display receiver authentication failed")
            }
            Self::InvalidSelectedMode => formatter
                .write_str("authenticated display selection has no exact Windows monitor mode"),
            Self::Projection(error) => error.fmt(formatter),
            Self::Session(_) => formatter.write_str("authenticated display session failed"),
            Self::Cleanup(error) => write!(formatter, "display session cleanup failed: {error}"),
            Self::SessionAndCleanup { cleanup, .. } => write!(
                formatter,
                "authenticated display session and cleanup failed: {cleanup}"
            ),
        }
    }
}

impl std::error::Error for DisplaySessionWorkerError {}

/// Serial service-owned bridge from an mTLS-authenticated display connection
/// to one authorized Route projection. The caller supplies the Sink Capability
/// identity from the Runtime authorization decision, never from network bytes.
pub struct AuthenticatedDisplayProjectionWorker<P> {
    routes: DisplayRouteProjectionRuntime<P>,
}

impl<P: DisplayProjectionControl> AuthenticatedDisplayProjectionWorker<P> {
    pub fn new(projection: P) -> Self {
        Self {
            routes: DisplayRouteProjectionRuntime::new(projection),
        }
    }

    /// Accepts one mutually authenticated and exactly Route-bound TLS session,
    /// projects its authorized Sink Capability, runs the bounded media owner,
    /// then attempts exact-epoch departure on every session outcome.
    pub fn accept_and_run<F>(
        &mut self,
        server: &AuthenticatedDisplayServer,
        sink_identity: IddMonitorIdentity,
        serve: F,
    ) -> Result<(), DisplaySessionWorkerError>
    where
        F: FnOnce(
            &mut AuthenticatedDisplaySender,
            &mut P,
            DisplayRouteProjectionSnapshot,
        ) -> Result<(), String>,
    {
        let mut sender = server
            .accept()
            .map_err(|error| DisplaySessionWorkerError::Authentication(error.to_string()))?;
        self.run_authenticated(sink_identity, &mut sender, serve)
    }

    /// Polls a server configured with `set_nonblocking(true)`. `Ok(false)`
    /// means that no receiver was waiting and no projection was changed.
    pub fn try_accept_and_run<F>(
        &mut self,
        server: &AuthenticatedDisplayServer,
        sink_identity: IddMonitorIdentity,
        serve: F,
    ) -> Result<bool, DisplaySessionWorkerError>
    where
        F: FnOnce(
            &mut AuthenticatedDisplaySender,
            &mut P,
            DisplayRouteProjectionSnapshot,
        ) -> Result<(), String>,
    {
        let Some(mut sender) = server
            .try_accept()
            .map_err(|error| DisplaySessionWorkerError::Authentication(error.to_string()))?
        else {
            return Ok(false);
        };
        self.run_authenticated(sink_identity, &mut sender, serve)?;
        Ok(true)
    }

    fn run_authenticated<F>(
        &mut self,
        sink_identity: IddMonitorIdentity,
        sender: &mut AuthenticatedDisplaySender,
        serve: F,
    ) -> Result<(), DisplaySessionWorkerError>
    where
        F: FnOnce(
            &mut AuthenticatedDisplaySender,
            &mut P,
            DisplayRouteProjectionSnapshot,
        ) -> Result<(), String>,
    {
        let binding = sender.binding().clone();
        self.run_bound_session(sink_identity, &binding, |projection, snapshot| {
            serve(sender, projection, snapshot)
        })
    }

    #[must_use]
    pub fn snapshots(&self) -> Vec<DisplayRouteProjectionSnapshot> {
        self.routes.snapshots()
    }

    #[must_use]
    pub fn projection(&self) -> &P {
        self.routes.projection()
    }

    fn run_bound_session<F>(
        &mut self,
        sink_identity: IddMonitorIdentity,
        binding: &DisplayTransportBinding,
        serve: F,
    ) -> Result<(), DisplaySessionWorkerError>
    where
        F: FnOnce(&mut P, DisplayRouteProjectionSnapshot) -> Result<(), String>,
    {
        let snapshot = self
            .routes
            .activate_authorized(projection_spec(sink_identity, binding)?)
            .map_err(DisplaySessionWorkerError::Projection)?;
        let session = serve(self.routes.projection_mut(), snapshot);
        let cleanup = self
            .routes
            .deactivate(binding.route_id(), binding.stream_epoch());
        match (session, cleanup) {
            (Ok(()), Ok(_)) => Ok(()),
            (Err(session), Ok(_)) => Err(DisplaySessionWorkerError::Session(session)),
            (Ok(()), Err(cleanup)) => Err(DisplaySessionWorkerError::Cleanup(cleanup)),
            (Err(session), Err(cleanup)) => {
                Err(DisplaySessionWorkerError::SessionAndCleanup { session, cleanup })
            }
        }
    }
}

fn monitor_mode(
    binding: &DisplayTransportBinding,
) -> Result<IddMonitorMode, DisplaySessionWorkerError> {
    let selected = binding.selected();
    let numerator = u64::from(selected.decoded.frame_rate.numerator());
    let denominator = u64::from(selected.decoded.frame_rate.denominator());
    let scaled = numerator
        .checked_mul(1000)
        .ok_or(DisplaySessionWorkerError::InvalidSelectedMode)?;
    if denominator == 0 || !scaled.is_multiple_of(denominator) {
        return Err(DisplaySessionWorkerError::InvalidSelectedMode);
    }
    let refresh_millihz = u32::try_from(scaled / denominator)
        .map_err(|_| DisplaySessionWorkerError::InvalidSelectedMode)?;
    let mode = IddMonitorMode {
        width: selected.decoded.width,
        height: selected.decoded.height,
        refresh_millihz,
    };
    mode.validate()
        .map_err(|_| DisplaySessionWorkerError::InvalidSelectedMode)?;
    Ok(mode)
}

pub(crate) fn projection_spec(
    sink_identity: IddMonitorIdentity,
    binding: &DisplayTransportBinding,
) -> Result<DisplayRouteProjectionSpec, DisplaySessionWorkerError> {
    Ok(DisplayRouteProjectionSpec {
        route_id: binding.route_id(),
        sink_identity,
        stream_epoch: binding.stream_epoch(),
        mode: monitor_mode(binding)?,
    })
}

#[cfg(test)]
mod tests {
    use capyio_core::{RouteId, SessionId, StreamId, ids::CapabilityId};
    use capyio_display::DisplayStreamSpec;
    use capyio_display_transport::EncodedDisplayStreamSpec;
    use capyio_video::FrameRate;
    use capyio_windows_display::{
        IddConnectedMonitor, IddMonitorConnectRequest, IddMonitorIdentity,
    };

    use super::*;

    #[derive(Default)]
    struct FakeProjection {
        prepared: Vec<IddMonitorConnectRequest>,
        disconnected: Vec<IddMonitorIdentity>,
        disconnect_error: bool,
    }

    impl DisplayProjectionControl for FakeProjection {
        fn prepare_monitor(
            &mut self,
            request: IddMonitorConnectRequest,
        ) -> Result<IddConnectedMonitor, String> {
            self.prepared.push(request);
            Ok(IddConnectedMonitor {
                identity: request.identity,
                connector_index: 0,
                target_id: 9,
                adapter_luid_low: 8,
                adapter_luid_high: -7,
            })
        }

        fn disconnect_monitor(&mut self, identity: IddMonitorIdentity) -> Result<bool, String> {
            self.disconnected.push(identity);
            if self.disconnect_error {
                Err("injected disconnect failure".to_owned())
            } else {
                Ok(true)
            }
        }
    }

    fn id<T: std::str::FromStr>(value: &str) -> T
    where
        T::Err: std::fmt::Debug,
    {
        value.parse().unwrap()
    }

    fn identity() -> IddMonitorIdentity {
        IddMonitorIdentity::new(id::<CapabilityId>("10112233-4455-6677-8899-aabbccddeeff")).unwrap()
    }

    fn binding() -> DisplayTransportBinding {
        DisplayTransportBinding::new(
            id::<SessionId>("00000000-0000-4000-8000-000000000001"),
            id::<RouteId>("00000000-0000-4000-8000-000000000002"),
            id::<StreamId>("00000000-0000-4000-8000-000000000003"),
            4,
            EncodedDisplayStreamSpec::fixture_720p30_avc(),
        )
        .unwrap()
    }

    #[test]
    fn authenticated_binding_projects_before_media_and_cleans_up_afterward() {
        let mut worker = AuthenticatedDisplayProjectionWorker::new(FakeProjection::default());
        let binding = binding();
        worker
            .run_bound_session(identity(), &binding, |_, snapshot| {
                assert_eq!(snapshot.spec.route_id, binding.route_id());
                assert_eq!(snapshot.spec.stream_epoch, 4);
                assert_eq!(snapshot.spec.sink_identity, identity());
                assert_eq!(snapshot.spec.mode.width, 1280);
                assert_eq!(snapshot.spec.mode.height, 720);
                assert_eq!(snapshot.spec.mode.refresh_millihz, 30_000);
                Ok(())
            })
            .unwrap();
        assert!(worker.snapshots().is_empty());
        assert_eq!(worker.projection().prepared.len(), 1);
        assert_eq!(worker.projection().disconnected, vec![identity()]);
    }

    #[test]
    fn media_failure_still_disconnects_the_exact_projection() {
        let mut worker = AuthenticatedDisplayProjectionWorker::new(FakeProjection::default());
        assert_eq!(
            worker.run_bound_session(
                identity(),
                &binding(),
                |_, _| Err("media failed".to_owned())
            ),
            Err(DisplaySessionWorkerError::Session(
                "media failed".to_owned()
            ))
        );
        assert!(worker.snapshots().is_empty());
        assert_eq!(worker.projection().disconnected, vec![identity()]);
    }

    #[test]
    fn cleanup_failure_retains_projection_and_both_failures_are_visible() {
        let mut worker = AuthenticatedDisplayProjectionWorker::new(FakeProjection {
            disconnect_error: true,
            ..FakeProjection::default()
        });
        assert!(matches!(
            worker.run_bound_session(identity(), &binding(), |_, _| Err("media failed".to_owned())),
            Err(DisplaySessionWorkerError::SessionAndCleanup {
                session,
                cleanup: DisplayRouteProjectionError::Projection(_),
            }) if session == "media failed"
        ));
        assert_eq!(worker.snapshots().len(), 1);
    }

    #[test]
    fn non_millihertz_frame_rate_fails_before_monitor_projection() {
        let original = binding();
        let mut decoded = DisplayStreamSpec::fixture_720p30_bgra();
        decoded.frame_rate = FrameRate::new(30_000, 1001).unwrap();
        let selected = EncodedDisplayStreamSpec {
            decoded,
            ..original.selected().clone()
        };
        let fractional = DisplayTransportBinding::new(
            original.session_id(),
            original.route_id(),
            original.stream_id(),
            original.stream_epoch(),
            selected,
        )
        .unwrap();
        let mut worker = AuthenticatedDisplayProjectionWorker::new(FakeProjection::default());
        assert_eq!(
            worker.run_bound_session(identity(), &fractional, |_, _| Ok(())),
            Err(DisplaySessionWorkerError::InvalidSelectedMode)
        );
        assert!(worker.projection().prepared.is_empty());
    }
}
