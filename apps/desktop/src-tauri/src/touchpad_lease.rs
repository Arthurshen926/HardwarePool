use super::{
    Arc, CatalogControlLink, DesktopNodeState, Mutex, PortLeaseAction, PortLeaseOutcome, PortRef,
    RouteId, RouteState, control_exchange, unix_now_ms,
};
use capyio_core::AuthorizationState;
use capyio_protocol::{MAX_PORT_LEASE_TTL_MS, MIN_PORT_LEASE_TTL_MS};
use std::time::Duration;

pub(super) const RENEW_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Debug)]
pub(super) enum RenewError {
    Lease(String),
    Link(String),
}
impl From<String> for RenewError {
    fn from(error: String) -> Self {
        Self::Lease(error)
    }
}
impl From<&str> for RenewError {
    fn from(error: &str) -> Self {
        Self::Lease(error.into())
    }
}
impl From<RenewError> for String {
    fn from(error: RenewError) -> Self {
        match error {
            RenewError::Lease(message) | RenewError::Link(message) => message,
        }
    }
}

pub(super) fn ttl(authorization: &AuthorizationState, now_ms: u64) -> Result<u64, String> {
    let AuthorizationState::Authorized {
        expires_at_ms: Some(expiry),
    } = authorization
    else {
        return Err("触控板缺少有效的本次使用授权".into());
    };
    let remaining = expiry.saturating_sub(now_ms).saturating_sub(1_000);
    if remaining < MIN_PORT_LEASE_TTL_MS {
        return Err("触控板本次使用授权已到期，请重新申请".into());
    }
    Ok(remaining.min(MAX_PORT_LEASE_TTL_MS))
}

pub(super) fn sink_expiry(authorization_expiry: u64, now_ms: u64) -> Result<u64, String> {
    let duration = ttl(
        &AuthorizationState::Authorized {
            expires_at_ms: Some(authorization_expiry),
        },
        now_ms,
    )?;
    now_ms
        .checked_add(duration)
        .ok_or_else(|| "touchpad lease clock overflow".into())
}

pub(super) fn renew(
    link: &mut CatalogControlLink,
    node: &Arc<Mutex<DesktopNodeState>>,
    route_id: RouteId,
    source: PortRef,
) -> Result<(), RenewError> {
    let route = {
        let state = node.lock().map_err(|_| "Node state lock poisoned")?;
        let route = state
            .runtime
            .route(route_id)
            .map_err(|e| e.to_string())?
            .clone();
        if route.session_id != link.session_id()
            || (route.source != source && route.sink != source)
            || source.node_id != link.peer_node_id()
            || route.state != RouteState::Active
        {
            return Err("触控板连接已结束；不会自动恢复授权".into());
        }
        require_live_claim(&state.runtime, &route, unix_now_ms()?)?;
        route
    };
    let request = link
        .send_port_lease_request(
            route_id,
            source,
            PortLeaseAction::Renew,
            ttl(&route.authorization, unix_now_ms()?)?,
        )
        .map_err(|e| RenewError::Link(e.to_string()))?;
    let response = control_exchange::receive_lease(link, node, request, route_id, source)
        .map_err(RenewError::Link)?;
    let mut state = node.lock().map_err(|_| "Node state lock poisoned")?;
    let now = unix_now_ms()?;
    if response.outcome != PortLeaseOutcome::Renewed
        || response.expires_at_ms.is_none_or(|expiry| expiry <= now)
    {
        return Err("对端未续租触控板；输入已停止，请重新申请".into());
    }
    // I/O elapsed time must not resurrect a locally expired claim.
    require_live_claim(&state.runtime, &route, now)?;
    let expiry = now
        .checked_add(ttl(&route.authorization, now)?)
        .ok_or("lease clock overflow")?;
    let local = local_port(&state.runtime, &route)?;
    state
        .runtime
        .acquire_route_port_lease(route_id, local, expiry, now)
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn require_live_claim(
    runtime: &capyio_runtime::NodeRuntime,
    route: &capyio_core::Route,
    now: u64,
) -> Result<(), String> {
    let current = runtime.route(route.id).map_err(|e| e.to_string())?;
    if current.state != RouteState::Active
        || current.session_id != route.session_id
        || current.authorization != route.authorization
        || !runtime
            .session(route.session_id)
            .is_ok_and(|session| session.state == capyio_core::SessionState::Ready)
    {
        return Err("触控板会话或授权已变化，不能续租".into());
    }
    if runtime
        .route_port_lease_claim(route.id, local_port(runtime, route)?)
        .is_none_or(|claim| claim.session_id != route.session_id || claim.expires_at_ms <= now)
    {
        return Err("触控板本机占用已结束；不会自动重新取得占用".into());
    }
    Ok(())
}

fn local_port(
    runtime: &capyio_runtime::NodeRuntime,
    route: &capyio_core::Route,
) -> Result<PortRef, String> {
    let local = runtime.snapshot().local_node.id;
    if route.source.node_id == local {
        Ok(route.source)
    } else if route.sink.node_id == local {
        Ok(route.sink)
    } else {
        Err("Route has no local endpoint".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_claims_continue_across_five_minutes_but_never_extend_consent() {
        let start = 1_800_000_000_000;
        let grant = start + 8 * 60 * 60 * 1_000;
        assert_eq!(sink_expiry(grant, start).unwrap(), start + 300_000);
        assert_eq!(
            sink_expiry(grant, start + 360_000).unwrap(),
            start + 660_000
        );
        assert_eq!(sink_expiry(grant, grant - 10_000).unwrap(), grant - 1_000);
        assert!(sink_expiry(grant, grant - 1_999).is_err());
        assert!(sink_expiry(grant, grant).is_err());
        assert!(ttl(&AuthorizationState::Pending, start).is_err());
        assert!(
            ttl(
                &AuthorizationState::Authorized {
                    expires_at_ms: None
                },
                start
            )
            .is_err()
        );
    }

    #[test]
    fn renewal_requires_the_same_live_claim_and_never_resurrects_stopped_input() {
        use capyio_runtime::PortLeasePolicy;
        let mut lab = capyio_testkit::DemoLab::new().unwrap();
        let id = lab.routes.phone_microphone_to_windows;
        lab.runtime
            .authorize_route(id, Some(8 * 60 * 60 * 1000))
            .unwrap();
        lab.runtime.set_route_active(id, true, 100).unwrap();
        let route = lab.runtime.route(id).unwrap().clone();
        lab.runtime
            .set_local_port_lease_policy(route.sink, PortLeasePolicy::Exclusive)
            .unwrap();
        assert!(require_live_claim(&lab.runtime, &route, 100).is_err());
        lab.runtime
            .acquire_route_port_lease(id, route.sink, 300_100, 100)
            .unwrap();
        assert!(require_live_claim(&lab.runtime, &route, 100).is_ok());
        let mut unrelated = route.clone();
        unrelated.id = lab.routes.phone_camera_to_panel;
        assert!(require_live_claim(&lab.runtime, &unrelated, 100).is_err());
        for now in (60_100..=420_100).step_by(60_000) {
            require_live_claim(&lab.runtime, &route, now).unwrap();
            lab.runtime
                .acquire_route_port_lease(id, route.sink, now + 300_000, now)
                .unwrap();
        }
        assert!(require_live_claim(&lab.runtime, &route, 720_100).is_err());
        lab.runtime.set_route_active(id, false, 430_100).unwrap();
        assert!(require_live_claim(&lab.runtime, &route, 430_101).is_err());
    }
}
