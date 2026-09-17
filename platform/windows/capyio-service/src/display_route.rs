use std::collections::HashMap;

use capyio_core::RouteId;
use capyio_windows_display::{
    IDD_CONTROL_MAX_MONITORS, IddConnectedMonitor, IddMonitorConnectRequest, IddMonitorCoordinator,
    IddMonitorIdentity, IddMonitorMode,
};

/// Service-internal projection request produced only after the outer Runtime
/// has authenticated the peer and authorized the Route. This value is not an
/// authentication token and must not be exposed as an interactive pipe command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DisplayRouteProjectionSpec {
    pub route_id: RouteId,
    pub sink_identity: IddMonitorIdentity,
    pub stream_epoch: u64,
    pub mode: IddMonitorMode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DisplayRouteProjectionSnapshot {
    pub spec: DisplayRouteProjectionSpec,
    pub monitor: IddConnectedMonitor,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DisplayRouteProjectionError {
    InvalidSpec,
    RouteConflict,
    CapabilityAlreadyProjected,
    CapacityExceeded,
    StaleEpoch,
    FutureEpoch,
    Projection(String),
}

impl std::fmt::Display for DisplayRouteProjectionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSpec => formatter.write_str("display projection spec is invalid"),
            Self::RouteConflict => {
                formatter.write_str("display Route is already bound to another projection")
            }
            Self::CapabilityAlreadyProjected => formatter
                .write_str("display Sink Capability is already owned by another active Route"),
            Self::CapacityExceeded => {
                formatter.write_str("display Route projection capacity is exhausted")
            }
            Self::StaleEpoch => formatter.write_str("display projection epoch is stale"),
            Self::FutureEpoch => formatter.write_str("display projection epoch is not active"),
            Self::Projection(_) => formatter.write_str("Windows display projection failed"),
        }
    }
}

impl std::error::Error for DisplayRouteProjectionError {}

pub trait DisplayProjectionControl {
    fn prepare_monitor(
        &mut self,
        request: IddMonitorConnectRequest,
    ) -> Result<IddConnectedMonitor, String>;

    fn disconnect_monitor(&mut self, identity: IddMonitorIdentity) -> Result<bool, String>;
}

impl DisplayProjectionControl for IddMonitorCoordinator {
    fn prepare_monitor(
        &mut self,
        request: IddMonitorConnectRequest,
    ) -> Result<IddConnectedMonitor, String> {
        self.prepare(request).map_err(|error| error.to_string())
    }

    fn disconnect_monitor(&mut self, identity: IddMonitorIdentity) -> Result<bool, String> {
        self.disconnect(identity).map_err(|error| error.to_string())
    }
}

pub struct DisplayRouteProjectionRuntime<P> {
    projection: P,
    active: HashMap<RouteId, DisplayRouteProjectionSnapshot>,
}

impl<P: DisplayProjectionControl> DisplayRouteProjectionRuntime<P> {
    pub fn new(projection: P) -> Self {
        Self {
            projection,
            active: HashMap::new(),
        }
    }

    /// Applies a Route event only after the caller has authenticated the peer
    /// and authorized the exact Route-to-Sink-Capability relationship.
    pub fn activate_authorized(
        &mut self,
        spec: DisplayRouteProjectionSpec,
    ) -> Result<DisplayRouteProjectionSnapshot, DisplayRouteProjectionError> {
        validate_spec(spec)?;
        if let Some(existing) = self.active.get_mut(&spec.route_id) {
            if existing.spec.sink_identity != spec.sink_identity || existing.spec.mode != spec.mode
            {
                return Err(DisplayRouteProjectionError::RouteConflict);
            }
            if spec.stream_epoch < existing.spec.stream_epoch {
                return Err(DisplayRouteProjectionError::StaleEpoch);
            }
            existing.spec.stream_epoch = spec.stream_epoch;
            return Ok(*existing);
        }
        if self
            .active
            .values()
            .any(|active| active.spec.sink_identity == spec.sink_identity)
        {
            return Err(DisplayRouteProjectionError::CapabilityAlreadyProjected);
        }
        if self.active.len() >= IDD_CONTROL_MAX_MONITORS as usize {
            return Err(DisplayRouteProjectionError::CapacityExceeded);
        }

        let monitor = self
            .projection
            .prepare_monitor(IddMonitorConnectRequest {
                identity: spec.sink_identity,
                preferred_mode: spec.mode,
            })
            .map_err(DisplayRouteProjectionError::Projection)?;
        let snapshot = DisplayRouteProjectionSnapshot { spec, monitor };
        self.active.insert(spec.route_id, snapshot);
        Ok(snapshot)
    }

    pub fn deactivate(
        &mut self,
        route_id: RouteId,
        stream_epoch: u64,
    ) -> Result<bool, DisplayRouteProjectionError> {
        let Some(active) = self.active.get(&route_id).copied() else {
            return Ok(false);
        };
        if stream_epoch < active.spec.stream_epoch {
            return Err(DisplayRouteProjectionError::StaleEpoch);
        }
        if stream_epoch > active.spec.stream_epoch {
            return Err(DisplayRouteProjectionError::FutureEpoch);
        }
        self.projection
            .disconnect_monitor(active.spec.sink_identity)
            .map_err(DisplayRouteProjectionError::Projection)?;
        self.active.remove(&route_id);
        Ok(true)
    }

    pub fn deactivate_all(&mut self) -> Result<(), DisplayRouteProjectionError> {
        let mut routes: Vec<_> = self.active.keys().copied().collect();
        routes.sort_unstable();
        for route_id in routes {
            let epoch = self
                .active
                .get(&route_id)
                .expect("collected active Route remains present")
                .spec
                .stream_epoch;
            self.deactivate(route_id, epoch)?;
        }
        Ok(())
    }

    #[must_use]
    pub fn snapshots(&self) -> Vec<DisplayRouteProjectionSnapshot> {
        let mut snapshots: Vec<_> = self.active.values().copied().collect();
        snapshots.sort_unstable_by_key(|snapshot| snapshot.spec.route_id);
        snapshots
    }

    #[must_use]
    pub fn projection(&self) -> &P {
        &self.projection
    }

    pub(crate) fn projection_mut(&mut self) -> &mut P {
        &mut self.projection
    }
}

fn validate_spec(spec: DisplayRouteProjectionSpec) -> Result<(), DisplayRouteProjectionError> {
    if spec.stream_epoch == 0 || spec.mode.validate().is_err() {
        return Err(DisplayRouteProjectionError::InvalidSpec);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use capyio_core::ids::CapabilityId;

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
                connector_index: self.prepared.len() as u32 - 1,
                target_id: 10 + self.prepared.len() as u32,
                adapter_luid_low: 42,
                adapter_luid_high: -3,
            })
        }

        fn disconnect_monitor(&mut self, identity: IddMonitorIdentity) -> Result<bool, String> {
            self.disconnected.push(identity);
            if self.disconnect_error {
                return Err("injected disconnect failure".to_owned());
            }
            Ok(true)
        }
    }

    fn route(index: u8) -> RouteId {
        format!("{index:02x}000000-0000-4000-8000-000000000001")
            .parse()
            .unwrap()
    }

    fn identity(index: u8) -> IddMonitorIdentity {
        IddMonitorIdentity::new(
            format!("{index:02x}112233-4455-6677-8899-aabbccddeeff")
                .parse::<CapabilityId>()
                .unwrap(),
        )
        .unwrap()
    }

    fn spec(route_index: u8, identity_index: u8, epoch: u64) -> DisplayRouteProjectionSpec {
        DisplayRouteProjectionSpec {
            route_id: route(route_index),
            sink_identity: identity(identity_index),
            stream_epoch: epoch,
            mode: IddMonitorMode {
                width: 1980,
                height: 912,
                refresh_millihz: 60_000,
            },
        }
    }

    #[test]
    fn activation_is_idempotent_and_newer_epoch_reuses_the_monitor() {
        let mut runtime = DisplayRouteProjectionRuntime::new(FakeProjection::default());
        let first = runtime.activate_authorized(spec(1, 1, 7)).unwrap();
        assert_eq!(runtime.activate_authorized(spec(1, 1, 7)).unwrap(), first);
        let advanced = runtime.activate_authorized(spec(1, 1, 8)).unwrap();
        assert_eq!(advanced.spec.stream_epoch, 8);
        assert_eq!(runtime.projection.prepared.len(), 1);
        assert_eq!(runtime.snapshots(), vec![advanced]);
    }

    #[test]
    fn invalid_spec_and_ninth_route_fail_before_projection() {
        let mut runtime = DisplayRouteProjectionRuntime::new(FakeProjection::default());
        assert_eq!(
            runtime.activate_authorized(spec(1, 1, 0)),
            Err(DisplayRouteProjectionError::InvalidSpec)
        );
        for index in 0..IDD_CONTROL_MAX_MONITORS as u8 {
            runtime
                .activate_authorized(spec(index + 1, index + 1, 1))
                .unwrap();
        }
        assert_eq!(
            runtime.activate_authorized(spec(20, 20, 1)),
            Err(DisplayRouteProjectionError::CapacityExceeded)
        );
        assert_eq!(runtime.projection.prepared.len(), 8);
    }

    #[test]
    fn stale_epoch_and_route_or_capability_rebinding_fail_closed() {
        let mut runtime = DisplayRouteProjectionRuntime::new(FakeProjection::default());
        runtime.activate_authorized(spec(1, 1, 7)).unwrap();
        assert_eq!(
            runtime.activate_authorized(spec(1, 1, 6)),
            Err(DisplayRouteProjectionError::StaleEpoch)
        );
        assert_eq!(
            runtime.activate_authorized(spec(1, 2, 8)),
            Err(DisplayRouteProjectionError::RouteConflict)
        );
        assert_eq!(
            runtime.activate_authorized(spec(2, 1, 1)),
            Err(DisplayRouteProjectionError::CapabilityAlreadyProjected)
        );
        assert_eq!(runtime.projection.prepared.len(), 1);
    }

    #[test]
    fn two_routes_prepare_two_independent_capabilities() {
        let mut runtime = DisplayRouteProjectionRuntime::new(FakeProjection::default());
        runtime.activate_authorized(spec(1, 1, 1)).unwrap();
        runtime.activate_authorized(spec(2, 2, 1)).unwrap();
        assert_eq!(runtime.projection.prepared.len(), 2);
        assert_eq!(runtime.snapshots().len(), 2);
    }

    #[test]
    fn stale_or_future_deactivation_cannot_remove_the_active_epoch() {
        let mut runtime = DisplayRouteProjectionRuntime::new(FakeProjection::default());
        runtime.activate_authorized(spec(1, 1, 7)).unwrap();
        assert_eq!(
            runtime.deactivate(route(1), 6),
            Err(DisplayRouteProjectionError::StaleEpoch)
        );
        assert_eq!(
            runtime.deactivate(route(1), 8),
            Err(DisplayRouteProjectionError::FutureEpoch)
        );
        assert!(runtime.projection.disconnected.is_empty());
        assert_eq!(runtime.snapshots().len(), 1);
    }

    #[test]
    fn disconnect_failure_retains_route_and_success_removes_it() {
        let mut runtime = DisplayRouteProjectionRuntime::new(FakeProjection::default());
        runtime.activate_authorized(spec(1, 1, 7)).unwrap();
        runtime.projection.disconnect_error = true;
        assert!(matches!(
            runtime.deactivate(route(1), 7),
            Err(DisplayRouteProjectionError::Projection(_))
        ));
        assert_eq!(runtime.snapshots().len(), 1);

        runtime.projection.disconnect_error = false;
        assert!(runtime.deactivate(route(1), 7).unwrap());
        assert!(runtime.snapshots().is_empty());
        assert!(!runtime.deactivate(route(1), 7).unwrap());
    }

    #[test]
    fn explicit_deactivate_all_cleans_every_route() {
        let mut runtime = DisplayRouteProjectionRuntime::new(FakeProjection::default());
        runtime.activate_authorized(spec(1, 1, 1)).unwrap();
        runtime.activate_authorized(spec(2, 2, 1)).unwrap();
        runtime.deactivate_all().unwrap();
        assert!(runtime.snapshots().is_empty());
        assert_eq!(runtime.projection.disconnected.len(), 2);
    }
}
