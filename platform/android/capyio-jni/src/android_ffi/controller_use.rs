//! Service-owned controller lifecycle. Receiver advertisements are configuration,
//! never authorization to start hardware.
use super::*;
use capyio_core::{
    AuthorizationState, CapabilityClass, PortDirection, PortRef, ProfileId, RouteBackend,
    RouteState, SessionState,
};

fn now() -> Result<u64, String> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis() as u64)
}

fn authorized(host: &AndroidCatalogRuntimeV1, route: &capyio_core::Route, now: u64) -> bool {
    host.runtime
        .session(route.session_id)
        .is_ok_and(|s| s.state == SessionState::Ready)
        && matches!(route.authorization, AuthorizationState::Authorized { expires_at_ms: Some(expiry) } if expiry > now)
        && host
            .runtime
            .route_port_lease_claim(route.id, route.source)
            .is_some_and(|lease| lease.expires_at_ms > now)
}

pub(super) fn fail(
    host: &mut AndroidCatalogRuntimeV1,
    id: capyio_core::RouteId,
    reason: &str,
) -> Result<(), String> {
    host.runtime
        .report_route_offline(
            id,
            capyio_core::Problem {
                id: capyio_core::ProblemId::new(),
                code: "CAPY.ANDROID.CONTROLLER_UNAVAILABLE".into(),
                category: capyio_core::ProblemCategory::Adapter,
                severity: capyio_core::ProblemSeverity::Error,
                retryable: true,
                related_node: None,
                related_adapter: None,
                related_route: Some(id),
                human_message: reason.into(),
                technical_detail: None,
            },
        )
        .map_err(|e| e.to_string())?;
    host.controller_status_pending = Some(id);
    Ok(())
}

fn current(host: &AndroidCatalogRuntimeV1, now: u64) -> Option<serde_json::Value> {
    let route = host.runtime.route(host.controller_use?).ok()?;
    let session = host.runtime.session(route.session_id).ok()?;
    let mode = match route.sink.capability_id.to_string().as_str() {
        "d41251b3-63ed-4e83-807e-4c4ab9b20c21" => "dualshock4",
        "d41251b3-63ed-4e83-807e-4c4ab9b20c22" => "xbox360",
        "d41251b3-63ed-4e83-807e-4c4ab9b20c23" => "dsu",
        _ => return None,
    };
    let valid = matches!(route.state, RouteState::Starting | RouteState::Active)
        && authorized(host, route, now);
    let offer = host
        .controller_receiver
        .as_ref()
        .filter(|(id, offer)| *id == route.session_id && offer.port != 0);
    let problem = if route.diagnostic_ids.is_empty() {
        None
    } else {
        host.runtime
            .snapshot()
            .problems
            .into_iter()
            .rev()
            .find(|problem| problem.related_route == Some(route.id))
    };
    Some(serde_json::json!({
        "routeId": route.id.to_string(), "peerNodeId": session.remote_node_id.to_string(),
        "mode": mode, "state": format!("{:?}", route.state).to_lowercase(),
        "shouldRun": valid && offer.is_some(),
        "error": problem.map(|problem| format!("{}: {}", problem.code, problem.human_message)),
        "receiver": offer.map(|(_, offer)| serde_json::json!({"host":offer.host,"port":offer.port,"token":offer.token})),
    }))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeCatalogRuntime_nativeControllerUse(
    mut env: EnvUnowned<'_>,
    _class: JClass<'_>,
    handle: jlong,
    action: JString<'_>,
    peer: JString<'_>,
    mode: JString<'_>,
    completed: JString<'_>,
    ready: jboolean,
) -> jbyteArray {
    env.with_env(|env| -> jni::errors::Result<_> {
        let result = (|| -> Result<Vec<u8>, String> {
            let action = string(env, action)?;
            let peer = string(env, peer)?;
            let mode = string(env, mode)?;
            let completed = string(env, completed)?;
            let mut hosts = catalog_runtimes()
                .lock()
                .map_err(|_| "Runtime registry poisoned")?;
            let host = hosts
                .get_mut(&catalog_handle(handle)?)
                .ok_or("unknown Runtime")?;
            let now = now()?;
            host.runtime.expire_port_leases(now);
            if action == "peers" {
                let snapshot = host.runtime.snapshot();
                let peers: Vec<_> = snapshot
                    .peers
                    .iter()
                    .filter(|peer| {
                        snapshot.sessions.iter().any(|session| {
                            session.remote_node_id == peer.id
                                && session.state == SessionState::Ready
                        }) && peer.capabilities.values().any(|capability| {
                            capability.class == CapabilityClass::Gamepad
                                && matches!(
                                    capability.id.to_string().as_str(),
                                    "d41251b3-63ed-4e83-807e-4c4ab9b20c21"
                                        | "d41251b3-63ed-4e83-807e-4c4ab9b20c22"
                                        | "d41251b3-63ed-4e83-807e-4c4ab9b20c23"
                                )
                                && capability.ports.values().any(|port| {
                                    port.direction == PortDirection::Sink
                                        && port.profile == ProfileId::gamepad_state_v1()
                                })
                        })
                    })
                    .map(|peer| {
                        serde_json::json!({"nodeId":peer.id.to_string(),
                        "displayName":peer.display_name})
                    })
                    .collect();
                return serde_json::to_vec(&peers).map_err(|e| e.to_string());
            }
            match action.as_str() {
                "start" => {
                    if host
                        .controller_use
                        .and_then(|id| host.runtime.route(id).ok())
                        .is_some_and(|route| {
                            !matches!(
                                route.state,
                                RouteState::Stopped | RouteState::Failed | RouteState::Offline
                            )
                        })
                    {
                        return Err("stop current controller use before changing mode".into());
                    }
                    let snapshot = host.runtime.snapshot();
                    let session = snapshot
                        .sessions
                        .iter()
                        .find(|s| {
                            s.remote_node_id.to_string() == peer && s.state == SessionState::Ready
                        })
                        .ok_or("peer is not authenticated")?;
                    let source = snapshot
                        .local_node
                        .capabilities
                        .values()
                        .find(|c| c.class == CapabilityClass::Gamepad)
                        .and_then(|c| {
                            c.ports
                                .values()
                                .find(|p| {
                                    p.direction == PortDirection::Source
                                        && p.profile == ProfileId::gamepad_state_v1()
                                })
                                .map(|p| PortRef {
                                    node_id: snapshot.local_node.id,
                                    capability_id: c.id,
                                    port_id: p.id,
                                })
                        })
                        .ok_or("publish local gamepad first")?;
                    let suffix = match mode.as_str() {
                        "dualshock4" => ("21", "31"),
                        "xbox360" => ("22", "32"),
                        "dsu" => ("23", "33"),
                        _ => return Err("unknown controller mode".into()),
                    };
                    let sink = PortRef {
                        node_id: session.remote_node_id,
                        capability_id: format!("d41251b3-63ed-4e83-807e-4c4ab9b20c{}", suffix.0)
                            .parse()
                            .map_err(|_| "invalid sink")?,
                        port_id: format!("d41251b3-63ed-4e83-807e-4c4ab9b20c{}", suffix.1)
                            .parse()
                            .map_err(|_| "invalid port")?,
                    };
                    let id = host
                        .runtime
                        .create_route(session.id, source, sink, RouteBackend::AdapterManaged)
                        .map_err(|e| e.to_string())?;
                    host.controller_use = Some(id);
                    host.controller_local_intent = Some(id);
                    host.controller_status_pending = Some(id);
                }
                "stop" => {
                    if let Some(id) = host.controller_use {
                        let state = host.runtime.route(id).map_err(|e| e.to_string())?.state;
                        if state == RouteState::Draft {
                            let authorization = host
                                .runtime
                                .route(id)
                                .map_err(|e| e.to_string())?
                                .authorization;
                            if matches!(
                                authorization,
                                AuthorizationState::Pending | AuthorizationState::Denied
                            ) {
                                host.runtime
                                    .deny_pending_route(id)
                                    .map_err(|e| e.to_string())?;
                                // This ends a proposal, not a running hardware Route.
                                // Keep Denied on the wire so the peer removes its
                                // consent item, and release the local operation to idle.
                                host.controller_use = None;
                                host.controller_local_intent = None;
                                host.controller_observed_state = None;
                            } else {
                                fail(host, id, "手柄使用请求已取消")?;
                            }
                        } else if !matches!(
                            state,
                            RouteState::Stopped | RouteState::Failed | RouteState::Offline
                        ) {
                            if state != RouteState::Stopping {
                                host.runtime
                                    .begin_route_stop(id)
                                    .map_err(|e| e.to_string())?;
                            }
                            host.runtime.stop_route(id).map_err(|e| e.to_string())?;
                        }
                        host.controller_status_pending = Some(id);
                    }
                }
                "failed" => {
                    if let Some(id) = host.controller_use.filter(|id| id.to_string() == completed) {
                        fail(host, id, "手柄发送器未就绪或已停止")?;
                    }
                }
                "poll" => {
                    if let Some(id) = host.controller_use {
                        let route = host.runtime.route(id).map_err(|e| e.to_string())?;
                        if matches!(route.state, RouteState::Starting | RouteState::Active)
                            && !authorized(host, route, now)
                        {
                            fail(host, id, "连接、授权或资源租约已结束")?;
                        }
                    }
                    if let Some(id) = host.controller_use.filter(|id| id.to_string() == completed) {
                        if ready
                            && current(host, now).is_some_and(|v| v["shouldRun"] == true)
                            && host.runtime.route(id).map_err(|e| e.to_string())?.state
                                == RouteState::Starting
                        {
                            host.runtime.activate_route(id).map_err(|e| e.to_string())?;
                            host.controller_status_pending = Some(id);
                        }
                    }
                }
                _ => return Err("unknown controller use action".into()),
            }
            if let Some(id) = host.controller_use {
                let state = host.runtime.route(id).map_err(|e| e.to_string())?.state;
                if host.controller_observed_state != Some((id, state)) {
                    host.controller_observed_state = Some((id, state));
                    host.controller_status_pending = Some(id);
                }
            }
            Ok(current(host, now)
                .unwrap_or(serde_json::Value::Null)
                .to_string()
                .into_bytes())
        })();
        Ok(match result {
            Ok(bytes) => byte_array(env, &bytes),
            Err(error) => {
                throw(env, "java/lang/IllegalStateException", error);
                ptr::null_mut()
            }
        })
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}
