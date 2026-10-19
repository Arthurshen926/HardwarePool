use std::collections::BTreeMap;

use capyio_core::{NodeId, PortRef, RouteId, SessionId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const MAX_PORT_LEASE_POLICIES: usize = 512;
pub const MAX_PORT_LEASE_CLAIMS: usize = 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum PortLeasePolicy {
    Exclusive,
    Shared { max_routes: u16 },
}

impl PortLeasePolicy {
    fn capacity(self) -> Result<usize, PortLeaseError> {
        match self {
            Self::Exclusive => Ok(1),
            Self::Shared { max_routes: 0 } => Err(PortLeaseError::InvalidSharedCapacity),
            Self::Shared { max_routes } => Ok(usize::from(max_routes)),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PortLeaseClaim {
    pub port: PortRef,
    pub route_id: RouteId,
    pub session_id: SessionId,
    pub expires_at_ms: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LeaseClaimOutcome {
    Acquired,
    Refreshed,
}

#[derive(Clone, Debug)]
pub struct PortLeaseRegistry {
    local_node_id: NodeId,
    policies: BTreeMap<PortRef, PortLeasePolicy>,
    claims: BTreeMap<PortRef, BTreeMap<RouteId, PortLeaseClaim>>,
}

impl PortLeaseRegistry {
    #[must_use]
    pub fn new(local_node_id: NodeId) -> Self {
        Self {
            local_node_id,
            policies: BTreeMap::new(),
            claims: BTreeMap::new(),
        }
    }

    pub fn set_policy(
        &mut self,
        port: PortRef,
        policy: PortLeasePolicy,
    ) -> Result<(), PortLeaseError> {
        if port.node_id != self.local_node_id {
            return Err(PortLeaseError::NotLocalPort(port));
        }
        policy.capacity()?;
        if self
            .claims
            .get(&port)
            .is_some_and(|claims| !claims.is_empty())
            && self.policies.get(&port) != Some(&policy)
        {
            return Err(PortLeaseError::PolicyInUse(port));
        }
        if !self.policies.contains_key(&port) && self.policies.len() >= MAX_PORT_LEASE_POLICIES {
            return Err(PortLeaseError::PolicyCapacityReached);
        }
        self.policies.insert(port, policy);
        Ok(())
    }

    pub fn claim(
        &mut self,
        claim: PortLeaseClaim,
        now_ms: u64,
    ) -> Result<LeaseClaimOutcome, PortLeaseError> {
        if claim.port.node_id != self.local_node_id {
            return Err(PortLeaseError::NotLocalPort(claim.port));
        }
        if claim.expires_at_ms <= now_ms {
            return Err(PortLeaseError::InvalidExpiry);
        }
        self.expire(now_ms);
        let policy = *self
            .policies
            .get(&claim.port)
            .ok_or(PortLeaseError::PolicyMissing(claim.port))?;
        if let Some(existing) = self
            .claims
            .get(&claim.port)
            .and_then(|claims| claims.get(&claim.route_id))
        {
            if existing.session_id != claim.session_id {
                return Err(PortLeaseError::RouteSessionMismatch);
            }
            self.claims
                .get_mut(&claim.port)
                .expect("existing claim map")
                .insert(claim.route_id, claim);
            return Ok(LeaseClaimOutcome::Refreshed);
        }
        let total_claims = self.claims.values().map(BTreeMap::len).sum::<usize>();
        if total_claims >= MAX_PORT_LEASE_CLAIMS {
            return Err(PortLeaseError::ClaimCapacityReached);
        }
        let claims = self.claims.entry(claim.port).or_default();
        if claims.len() >= policy.capacity()? {
            return Err(PortLeaseError::Conflict(claim.port));
        }
        claims.insert(claim.route_id, claim);
        Ok(LeaseClaimOutcome::Acquired)
    }

    pub fn expire(&mut self, now_ms: u64) -> usize {
        let before = self.claims.values().map(BTreeMap::len).sum::<usize>();
        self.claims.retain(|_, claims| {
            claims.retain(|_, claim| claim.expires_at_ms > now_ms);
            !claims.is_empty()
        });
        before - self.claims.values().map(BTreeMap::len).sum::<usize>()
    }

    pub fn release_route(&mut self, route_id: RouteId) -> usize {
        let before = self.claims.values().map(BTreeMap::len).sum::<usize>();
        self.claims.retain(|_, claims| {
            claims.remove(&route_id);
            !claims.is_empty()
        });
        before - self.claims.values().map(BTreeMap::len).sum::<usize>()
    }

    pub fn release(
        &mut self,
        port: PortRef,
        route_id: RouteId,
        session_id: SessionId,
    ) -> Result<bool, PortLeaseError> {
        let Some(existing) = self
            .claims
            .get(&port)
            .and_then(|claims| claims.get(&route_id))
        else {
            return Ok(false);
        };
        if existing.session_id != session_id {
            return Err(PortLeaseError::RouteSessionMismatch);
        }
        let claims = self.claims.get_mut(&port).expect("existing claim map");
        claims.remove(&route_id);
        if claims.is_empty() {
            self.claims.remove(&port);
        }
        Ok(true)
    }

    #[must_use]
    pub fn policy(&self, port: PortRef) -> Option<PortLeasePolicy> {
        self.policies.get(&port).copied()
    }

    #[must_use]
    pub fn has_current_claim(&self, port: PortRef, route_id: RouteId, now_ms: u64) -> bool {
        self.claims
            .get(&port)
            .and_then(|claims| claims.get(&route_id))
            .is_some_and(|claim| claim.expires_at_ms > now_ms)
    }

    pub fn claims(&self) -> impl Iterator<Item = &PortLeaseClaim> {
        self.claims.values().flat_map(|claims| claims.values())
    }

    #[must_use]
    pub fn claim_for(&self, port: PortRef, route_id: RouteId) -> Option<&PortLeaseClaim> {
        self.claims
            .get(&port)
            .and_then(|claims| claims.get(&route_id))
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum PortLeaseError {
    #[error("lease Port {0:?} is not owned by the local Node")]
    NotLocalPort(PortRef),
    #[error("shared lease capacity must be non-zero")]
    InvalidSharedCapacity,
    #[error("lease policy for Port {0:?} is in use")]
    PolicyInUse(PortRef),
    #[error("lease policy capacity reached")]
    PolicyCapacityReached,
    #[error("lease policy is missing for Port {0:?}")]
    PolicyMissing(PortRef),
    #[error("lease expiry must be later than the current time")]
    InvalidExpiry,
    #[error("Route lease refresh changed Session identity")]
    RouteSessionMismatch,
    #[error("lease claim capacity reached")]
    ClaimCapacityReached,
    #[error("lease conflict on Port {0:?}")]
    Conflict(PortRef),
}

#[cfg(test)]
mod tests {
    use capyio_core::{CapabilityId, PortId};

    use super::*;

    fn test_port(node_id: NodeId) -> PortRef {
        PortRef {
            node_id,
            capability_id: CapabilityId::new(),
            port_id: PortId::new(),
        }
    }

    fn claim(
        port: PortRef,
        route_id: RouteId,
        session_id: SessionId,
        expires_at_ms: u64,
    ) -> PortLeaseClaim {
        PortLeaseClaim {
            port,
            route_id,
            session_id,
            expires_at_ms,
        }
    }

    #[test]
    fn exclusive_policy_rejects_another_route_and_refreshes_same_identity() {
        let node_id = NodeId::new();
        let port = test_port(node_id);
        let mut leases = PortLeaseRegistry::new(node_id);
        leases
            .set_policy(port, PortLeasePolicy::Exclusive)
            .expect("policy");
        let first_route = RouteId::new();
        let first_session = SessionId::new();
        assert_eq!(
            leases.claim(claim(port, first_route, first_session, 20), 10),
            Ok(LeaseClaimOutcome::Acquired)
        );
        assert_eq!(
            leases.claim(claim(port, first_route, first_session, 30), 11),
            Ok(LeaseClaimOutcome::Refreshed)
        );
        assert_eq!(
            leases.claim(claim(port, first_route, SessionId::new(), 40), 12),
            Err(PortLeaseError::RouteSessionMismatch)
        );
        assert_eq!(
            leases.claim(claim(port, RouteId::new(), SessionId::new(), 40), 12),
            Err(PortLeaseError::Conflict(port))
        );
        assert!(leases.has_current_claim(port, first_route, 29));
        assert!(!leases.has_current_claim(port, first_route, 30));
    }

    #[test]
    fn shared_policy_has_an_exact_bound_and_release_is_route_scoped() {
        let node_id = NodeId::new();
        let port = test_port(node_id);
        let mut leases = PortLeaseRegistry::new(node_id);
        leases
            .set_policy(port, PortLeasePolicy::Shared { max_routes: 2 })
            .expect("policy");
        let first = RouteId::new();
        let second = RouteId::new();
        leases
            .claim(claim(port, first, SessionId::new(), 20), 10)
            .expect("first");
        leases
            .claim(claim(port, second, SessionId::new(), 20), 10)
            .expect("second");
        assert_eq!(
            leases.claim(claim(port, RouteId::new(), SessionId::new(), 20), 10),
            Err(PortLeaseError::Conflict(port))
        );
        assert_eq!(leases.release_route(first), 1);
        assert_eq!(leases.claims().count(), 1);
        assert_eq!(leases.release_route(first), 0);
    }

    #[test]
    fn exact_release_requires_the_claim_session() {
        let node_id = NodeId::new();
        let port = test_port(node_id);
        let route_id = RouteId::new();
        let session_id = SessionId::new();
        let mut leases = PortLeaseRegistry::new(node_id);
        leases
            .set_policy(port, PortLeasePolicy::Exclusive)
            .expect("policy");
        leases
            .claim(claim(port, route_id, session_id, 20), 10)
            .expect("claim");

        assert_eq!(
            leases.release(port, route_id, SessionId::new()),
            Err(PortLeaseError::RouteSessionMismatch)
        );
        assert!(leases.claim_for(port, route_id).is_some());
        assert_eq!(leases.release(port, route_id, session_id), Ok(true));
        assert_eq!(leases.release(port, route_id, session_id), Ok(false));
    }

    #[test]
    fn owner_scope_expiry_and_policy_changes_fail_closed() {
        let node_id = NodeId::new();
        let port = test_port(node_id);
        let mut leases = PortLeaseRegistry::new(node_id);
        assert_eq!(
            leases.set_policy(port, PortLeasePolicy::Shared { max_routes: 0 }),
            Err(PortLeaseError::InvalidSharedCapacity)
        );
        let foreign = test_port(NodeId::new());
        assert_eq!(
            leases.set_policy(foreign, PortLeasePolicy::Exclusive),
            Err(PortLeaseError::NotLocalPort(foreign))
        );
        leases
            .set_policy(port, PortLeasePolicy::Exclusive)
            .expect("policy");
        assert_eq!(
            leases.claim(claim(port, RouteId::new(), SessionId::new(), 10), 10),
            Err(PortLeaseError::InvalidExpiry)
        );
        let route = RouteId::new();
        leases
            .claim(claim(port, route, SessionId::new(), 20), 10)
            .expect("claim");
        assert_eq!(
            leases.set_policy(port, PortLeasePolicy::Shared { max_routes: 2 }),
            Err(PortLeaseError::PolicyInUse(port))
        );
        assert_eq!(leases.expire(20), 1);
        assert_eq!(leases.claims().count(), 0);
        leases
            .set_policy(port, PortLeasePolicy::Shared { max_routes: 2 })
            .expect("unused policy changes");
    }

    #[test]
    fn policy_and_claim_storage_have_fixed_global_bounds() {
        let node_id = NodeId::new();
        let mut leases = PortLeaseRegistry::new(node_id);
        let mut ports = Vec::new();
        for _ in 0..MAX_PORT_LEASE_POLICIES {
            let port = test_port(node_id);
            leases
                .set_policy(port, PortLeasePolicy::Exclusive)
                .expect("bounded policy");
            ports.push(port);
        }
        assert_eq!(
            leases.set_policy(test_port(node_id), PortLeasePolicy::Exclusive),
            Err(PortLeaseError::PolicyCapacityReached)
        );

        let shared = ports[0];
        leases.policies.insert(
            shared,
            PortLeasePolicy::Shared {
                max_routes: u16::MAX,
            },
        );
        for _ in 0..MAX_PORT_LEASE_CLAIMS {
            leases
                .claim(claim(shared, RouteId::new(), SessionId::new(), 20), 10)
                .expect("bounded claim");
        }
        assert_eq!(
            leases.claim(claim(shared, RouteId::new(), SessionId::new(), 20), 10),
            Err(PortLeaseError::ClaimCapacityReached)
        );
    }
}
