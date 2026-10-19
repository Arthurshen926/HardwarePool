//! Shared port-lease orchestration for the existing native audio and camera routes.
use super::{Arc, CatalogControlLink, DesktopNodeState, Mutex, control_exchange};
use capyio_core::{PortRef, RouteId};
use capyio_protocol::{PortLeaseAction, PortLeaseOutcome};
use capyio_runtime::{NodeRuntime, PortLeasePolicy};

pub(super) struct RoutePortLeases {
    pub route: RouteId,
    pub local: PortRef,
    pub remote: PortRef,
}

impl RoutePortLeases {
    pub fn acquire_remote(
        &self,
        link: &mut CatalogControlLink,
        node: &Arc<Mutex<DesktopNodeState>>,
        ttl: u64,
        rejected: &str,
    ) -> Result<(), String> {
        self.request_remote(link, node, PortLeaseAction::Acquire, ttl, rejected)
    }

    pub fn acquire_local(
        &self,
        runtime: &mut NodeRuntime,
        now: u64,
        ttl: u64,
    ) -> Result<(), String> {
        runtime
            .set_local_port_lease_policy(self.local, PortLeasePolicy::Exclusive)
            .map_err(|e| e.to_string())?;
        runtime
            .acquire_route_port_lease(self.route, self.local, now + ttl, now)
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn renew(
        &self,
        link: &mut CatalogControlLink,
        node: &Arc<Mutex<DesktopNodeState>>,
        now: u64,
        ttl: u64,
        rejected: &str,
    ) -> Result<(), String> {
        self.request_remote(link, node, PortLeaseAction::Renew, ttl, rejected)?;
        node.lock()
            .map_err(|_| "Node lock poisoned")?
            .runtime
            .acquire_route_port_lease(self.route, self.local, now + ttl, now)
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn request_remote(
        &self,
        link: &mut CatalogControlLink,
        node: &Arc<Mutex<DesktopNodeState>>,
        action: PortLeaseAction,
        ttl: u64,
        rejected: &str,
    ) -> Result<(), String> {
        let request = link
            .send_port_lease_request(self.route, self.remote, action, ttl)
            .map_err(|e| e.to_string())?;
        let response =
            control_exchange::receive_lease(link, node, request, self.route, self.remote)?;
        let accepted = response.outcome == PortLeaseOutcome::Renewed
            || (action == PortLeaseAction::Acquire
                && response.outcome == PortLeaseOutcome::Acquired);
        if !accepted {
            return Err(rejected.into());
        }
        Ok(())
    }
}
