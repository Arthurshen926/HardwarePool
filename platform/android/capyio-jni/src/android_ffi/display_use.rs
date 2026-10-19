use super::*;
use crate::display_catalog::*;
use capyio_core::*;
use capyio_protocol::v1::{
    DisplaySessionAction as Action, DisplaySessionControl as Message, DisplayStreamCapabilities,
    DisplayStreamSelection,
};

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn message(job: &DisplayUse, action: Action) -> Message {
    Message {
        operation_id: job.operation.clone(),
        route_id: job.route.clone(),
        epoch: job.epoch,
        action: action as i32,
        automatic: action == Action::Start && job.automatic,
        ..Default::default()
    }
}
fn stop_route(host: &mut AndroidCatalogRuntimeV1) -> Result<(), String> {
    let Ok(id) = host.display_use.route.parse::<RouteId>() else {
        return Ok(());
    };
    let route = host.runtime.route(id).map_err(|e| e.to_string())?.clone();
    if matches!(
        route.state,
        RouteState::Prepared | RouteState::Starting | RouteState::Active
    ) {
        host.runtime
            .begin_route_stop(id)
            .map_err(|e| e.to_string())?;
        host.runtime.stop_route(id).map_err(|e| e.to_string())?;
    } else if route.state == RouteState::Draft && route.authorization == AuthorizationState::Pending
    {
        host.runtime
            .deny_pending_route(id)
            .map_err(|e| e.to_string())?;
    }
    host.audio_status_pending.insert(id);
    Ok(())
}
pub(super) fn is_display(route: &Route) -> bool {
    route.sink.capability_id.to_string() == SINK_CAP
        && route.sink.port_id.to_string() == SINK_PORT
        && route.source.capability_id.to_string() == SOURCE_CAP
        && route.profile == ProfileId::video_frames_v1()
}
pub(super) fn receive(
    host: &mut AndroidCatalogRuntimeV1,
    link: &CatalogControlLink,
    mut msg: Message,
    address: IpAddr,
) -> Result<(), String> {
    let action = Action::try_from(msg.action).map_err(|_| "display action")?;
    if action == Action::Start {
        if host.display_use.operation == msg.operation_id && !host.display_use.route.is_empty() {
            return Ok(());
        }
        if host.display_use.operation == msg.operation_id
            && host.display_use.selection != msg.selection
        {
            return Err("display selection changed during startup".into());
        }
        if !matches!(host.display_use.status.as_str(), "" | "idle" | "failed")
            && host.display_use.operation != msg.operation_id
        {
            return Err("display already in use".into());
        }
        let local = host.display_use.operation == msg.operation_id && host.display_use.local_intent;
        host.display_use = DisplayUse {
            peer: link.peer_node_id().to_string(),
            operation: msg.operation_id,
            route: msg.route_id,
            selection: msg.selection,
            status: "awaiting_consent".into(),
            local_intent: local,
            probe: true,
            automatic: msg.automatic,
            ..Default::default()
        };
        return Ok(());
    }
    let job = &host.display_use;
    if job.operation != msg.operation_id
        || job.peer != link.peer_node_id().to_string()
        || job.route != msg.route_id
    {
        return Err("display operation mismatch".into());
    }
    match action {
        Action::Ready => {
            if msg.epoch != job.epoch || job.epoch == 0 || job.status != "starting" {
                return Err("display ready mismatch".into());
            }
            host.display_use.ready = true;
        }
        Action::Stop | Action::Failed => {
            stop_route(host)?;
            host.display_use.launch.fill(0);
            host.display_use.launch.clear();
            host.display_use.status = if action == Action::Stop {
                "idle"
            } else {
                "failed"
            }
            .into();
            host.display_use.error = msg.problem_code;
        }
        Action::Launch => {
            let id: RouteId = msg.route_id.parse().map_err(|_| "display route")?;
            let route = host.runtime.route(id).map_err(|e| e.to_string())?.clone();
            if !is_display(&route)
                || route.session_id != link.session_id()
                || route.source.node_id != link.peer_node_id()
                || route.sink.node_id != host.runtime.snapshot().local_node.id
            {
                return Err("display route identity".into());
            }
            let AuthorizationState::Authorized {
                expires_at_ms: Some(expiry),
            } = route.authorization
            else {
                return Err("display not authorized".into());
            };
            if expiry <= now() {
                return Err("display grant expired".into());
            }
            let bytes = &msg.launch_record;
            if bytes.len() != 176 {
                return Err("display launch size".into());
            }
            let uuid_at = |offset: usize| -> String {
                let b = &bytes[offset..offset + 16];
                format!(
                    "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
                    b[0],
                    b[1],
                    b[2],
                    b[3],
                    b[4],
                    b[5],
                    b[6],
                    b[7],
                    b[8],
                    b[9],
                    b[10],
                    b[11],
                    b[12],
                    b[13],
                    b[14],
                    b[15]
                )
            };
            let u32_at = |o: usize| u32::from_be_bytes(bytes[o..o + 4].try_into().unwrap());
            let epoch = u64::from_be_bytes(bytes[96..104].try_into().unwrap());
            let selection = job.selection.as_ref().ok_or("display selection absent")?;
            let mode = selection.mode.as_ref().ok_or("display mode absent")?;
            let IpAddr::V4(ip) = address else {
                return Err("display IPv4 required".into());
            };
            if uuid_at(32) != route.sink.capability_id.to_string()
                || uuid_at(48) != route.session_id.to_string()
                || uuid_at(64) != msg.route_id
                || epoch != msg.epoch
                || epoch == 0
                || bytes[104..108] != ip.octets()
                || u32_at(112) != mode.width
                || u32_at(116) != mode.height
                || u32_at(120) != mode.refresh_millihertz
                || u32_at(132) != selection.bitrate_kbps
                || u32_at(124) != 1
                || u32_at(128) != 1
            {
                return Err("display launch differs from negotiated route".into());
            }
            if route.state == RouteState::Draft {
                host.runtime
                    .prepare_route(
                        id,
                        route.compatible_formats.first().cloned(),
                        QosMode::Interactive,
                        now(),
                    )
                    .map_err(|e| e.to_string())?;
            }
            if host.runtime.route(id).map_err(|e| e.to_string())?.state == RouteState::Prepared {
                host.runtime
                    .begin_route_start(id, now())
                    .map_err(|e| e.to_string())?;
            }
            if host.runtime.route(id).map_err(|e| e.to_string())?.epoch != epoch {
                return Err("display epoch mismatch".into());
            }
            host.display_use.epoch = epoch;
            host.display_use.launch = std::mem::take(&mut msg.launch_record);
            host.display_use.status = "starting".into();
        }
        _ => return Err("unexpected display action at receiver".into()),
    }
    Ok(())
}
pub(super) fn route_status(host: &mut AndroidCatalogRuntimeV1, peer: &Route) -> Result<(), String> {
    if !is_display(peer) {
        return Ok(());
    }
    if host.display_use.route.is_empty() && host.display_use.peer == peer.source.node_id.to_string()
    {
        host.display_use.route = peer.id.to_string();
    }
    if host.display_use.route != peer.id.to_string() {
        return Ok(());
    }
    if let AuthorizationState::Authorized {
        expires_at_ms: Some(expiry),
    } = peer.authorization
    {
        host.display_use.peer_deadline = Some(expiry);
    }
    let local = host
        .runtime
        .route(peer.id)
        .map_err(|e| e.to_string())?
        .clone();
    if matches!(local.state, RouteState::Starting | RouteState::Active) && peer.epoch == local.epoch
    {
        if let (
            AuthorizationState::Authorized {
                expires_at_ms: Some(previous),
            },
            AuthorizationState::Authorized {
                expires_at_ms: Some(next),
            },
        ) = (local.authorization, peer.authorization)
        {
            if next > previous && next <= now().saturating_add(600_000) {
                host.runtime
                    .renew_route_authorization(peer.id, next, now())
                    .map_err(|e| e.to_string())?;
                host.audio_status_pending.insert(peer.id);
            }
        }
    }
    if host.display_use.local_intent
        && host
            .runtime
            .route(peer.id)
            .map_err(|e| e.to_string())?
            .authorization
            == AuthorizationState::Pending
    {
        if let AuthorizationState::Authorized {
            expires_at_ms: Some(expiry),
        } = peer.authorization
        {
            host.enqueue_touchpad_request(peer.id, now())?;
            host.touchpad_consent.resolve_bounded(
                &mut host.runtime,
                peer.id,
                true,
                now(),
                Some(expiry),
            )?;
            host.audio_status_pending.insert(peer.id);
        }
    }
    if matches!(
        peer.state,
        RouteState::Stopped | RouteState::Offline | RouteState::Failed
    ) || peer.authorization == AuthorizationState::Denied
    {
        stop_route(host)?;
        host.display_use.status = "idle".into();
        host.display_use.launch.fill(0);
        host.display_use.launch.clear();
    }
    Ok(())
}
pub(super) fn publish(
    host: &mut AndroidCatalogRuntimeV1,
    link: &mut CatalogControlLink,
) -> Result<(), String> {
    if host.display_use.peer != link.peer_node_id().to_string() {
        return Ok(());
    }
    for mut msg in std::mem::take(&mut host.display_use.pending) {
        if msg.route_id.is_empty() {
            msg.route_id = host.display_use.route.clone();
        }
        link.send_display_session_control(msg)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub(super) fn receive_failure(
    host: &mut AndroidCatalogRuntimeV1,
    operation: String,
    route: String,
    error: String,
) {
    if host.display_use.operation == operation {
        let _ = stop_route(host);
        host.display_use.launch.fill(0);
        host.display_use.launch.clear();
        host.display_use.status = "failed".into();
        host.display_use.error = error;
    }
    if host.display_use.pending.len() < 16 {
        host.display_use.pending.push(Message {
            operation_id: operation,
            route_id: route,
            action: Action::Failed as i32,
            problem_code: "CAPY.DISPLAY.RECEIVER_REJECTED".into(),
            ..Default::default()
        });
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeCatalogRuntime_nativeDisplayUse(
    mut env: EnvUnowned<'_>,
    _class: JClass<'_>,
    handle: jlong,
    action: JString<'_>,
    argument: JString<'_>,
) -> jbyteArray {
    env.with_env(|env|->jni::errors::Result<_>{let result=(||->Result<Vec<u8>,String>{
        let action=string(env,action)?;let argument=string(env,argument)?;
        let mut runtimes=catalog_runtimes().lock().map_err(|_|"Runtime registry poisoned")?;
        let host=runtimes.get_mut(&catalog_handle(handle)?).ok_or("Runtime absent")?;
        if action=="peers" {let s=host.runtime.snapshot();let peers:Vec<_>=s.peers.iter().filter(|p|s.sessions.iter().any(|session|session.remote_node_id==p.id&&session.state==SessionState::Ready)).map(|p|serde_json::json!({"nodeId":p.id.to_string(),"displayName":p.display_name,"displayAvailable":p.capabilities.keys().any(|id|id.to_string()==SOURCE_CAP)})).collect();return serde_json::to_vec(&peers).map_err(|e|e.to_string());}
        if action=="start" {
            if !matches!(host.display_use.status.as_str(),""|"idle"|"failed"){return Err("display already in use".into());}
            let value:serde_json::Value=serde_json::from_str(&argument).map_err(|e|e.to_string())?;
            let peer=value["peerNodeId"].as_str().ok_or("display peer absent")?.to_string();
            if !host.runtime.snapshot().sessions.iter().any(|s|s.remote_node_id.to_string()==peer&&s.state==SessionState::Ready){return Err("display peer not connected".into());}
            let n=|key:&str|->Result<u32,String>{value[key].as_u64().and_then(|v|u32::try_from(v).ok()).ok_or(format!("invalid {key}"))};
            let selection=DisplayStreamSelection{mode:Some(capyio_protocol::v1::DisplayStreamMode{width:n("width")?,height:n("height")?,refresh_millihertz:n("refreshMillihertz")?}),bitrate_kbps:n("bitrateKbps")?};
            host.display_use=DisplayUse::local_start(peer,selection,value["automatic"].as_bool().unwrap_or(false));
            let mut msg=message(&host.display_use,Action::Start);msg.selection=host.display_use.selection.clone();host.display_use.pending.push(msg);
        } else if action=="supported" {
            host.display_use.probe=false;
            if host.display_use.automatic {
                let value:serde_json::Value=serde_json::from_str(&argument).map_err(|e|e.to_string())?;
                let n=|key:&str|->Result<u32,String>{value[key].as_u64().and_then(|v|u32::try_from(v).ok()).ok_or(format!("invalid {key}"))};
                let width=n("width")?;let height=n("height")?;let refresh=n("refreshMillihertz")?;let bitrate=n("bitrateKbps")?;
                if width<2||height<2||width>2560||height>1600||width%2!=0||height%2!=0||!matches!(refresh,30_000|60_000)||!(5000..=40000).contains(&bitrate){return Err("unsupported automatic display selection".into());}
                host.display_use.selection=Some(DisplayStreamSelection{mode:Some(capyio_protocol::v1::DisplayStreamMode{width,height,refresh_millihertz:refresh}),bitrate_kbps:bitrate});
            }
            let selection=host.display_use.selection.clone().ok_or("display selection absent")?;
            let mut msg=message(&host.display_use,Action::Capabilities);
            msg.selection=Some(selection.clone());
            msg.capabilities=Some(DisplayStreamCapabilities{modes:selection.mode.into_iter().collect(),minimum_bitrate_kbps:selection.bitrate_kbps,maximum_bitrate_kbps:selection.bitrate_kbps,h264_sdr:true});host.display_use.pending.push(msg);
        } else if action=="stop"||action=="failed" {
            if !host.display_use.operation.is_empty(){let mut msg=message(&host.display_use,if action=="stop"{Action::Stop}else{Action::Failed});if action=="failed" {msg.problem_code="CAPY.DISPLAY.RECEIVER_FAILED".into();}host.display_use.pending.push(msg);}
            host.display_use.launch.fill(0);host.display_use.launch.clear();host.display_use.status=if action=="stop"{"idle"}else{"failed"}.into();host.display_use.error=argument;
            stop_route(host)?;
        } else if action=="presented" {
            let frames=argument.parse::<u64>().map_err(|_|"invalid frame count")?;let mut msg=message(&host.display_use,Action::Presented);msg.presented_frames=frames;host.display_use.pending.push(msg);host.display_use.status="active".into();
            if let Ok(id)=host.display_use.route.parse::<RouteId>() {if host.runtime.route(id).is_ok_and(|r|r.state==RouteState::Starting){host.runtime.activate_route(id).map_err(|e|e.to_string())?;host.audio_status_pending.insert(id);}}
        }
        if !matches!(host.display_use.status.as_str(),""|"idle"|"failed") {
            if let Ok(id)=host.display_use.route.parse::<RouteId>() {if let Ok(r)=host.runtime.route(id) {let valid=host.runtime.session(r.session_id).is_ok_and(|s|s.state==SessionState::Ready)&& !matches!(r.state,RouteState::Stopped|RouteState::Offline|RouteState::Failed)&& r.authorization!=AuthorizationState::Denied && !matches!(r.authorization,AuthorizationState::Authorized{expires_at_ms:Some(e)} if e<=now());if !valid {host.display_use.status="failed".into();host.display_use.error="display_session_ended".into();host.display_use.launch.fill(0);host.display_use.launch.clear();}}}
        }
        let job=&host.display_use;let mode=job.selection.as_ref().and_then(|s|s.mode.as_ref());
        serde_json::to_vec(&serde_json::json!({"status":if job.status.is_empty(){"idle"}else{&job.status},"operationId":job.operation,"peerNodeId":job.peer,"routeId":job.route,"error":job.error,"probe":job.probe,"automatic":job.automatic,"hasLaunch":!job.launch.is_empty(),"ready":job.ready,"width":mode.map(|m|m.width),"height":mode.map(|m|m.height),"refreshMillihertz":mode.map(|m|m.refresh_millihertz),"bitrateKbps":job.selection.as_ref().map(|s|s.bitrate_kbps)})).map_err(|e|e.to_string())
    })();Ok(match result{Ok(bytes)=>byte_array(env,&bytes),Err(e)=>{throw(env,"java/lang/IllegalStateException",e);ptr::null_mut()}})}).resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeCatalogRuntime_nativeDisplayRecord(
    mut env: EnvUnowned<'_>,
    _class: JClass<'_>,
    handle: jlong,
    ack: JByteArray<'_>,
) -> jbyteArray {
    env.with_env(|env| -> jni::errors::Result<_> {
        let result = (|| -> Result<Vec<u8>, String> {
            let mut runtimes = catalog_runtimes()
                .lock()
                .map_err(|_| "Runtime registry poisoned")?;
            let host = runtimes
                .get_mut(&catalog_handle(handle)?)
                .ok_or("Runtime absent")?;
            if ack.is_null() {
                return Ok(std::mem::take(&mut host.display_use.launch));
            }
            let bytes = env.convert_byte_array(&ack).map_err(|e| e.to_string())?;
            if bytes.len() != 64 {
                return Err("display ACK size".into());
            }
            let mut msg = message(&host.display_use, Action::Ack);
            msg.acknowledgement = bytes;
            host.display_use.pending.push(msg);
            Ok(Vec::new())
        })();
        Ok(match result {
            Ok(mut bytes) => {
                let result = byte_array(env, &bytes);
                bytes.fill(0);
                result
            }
            Err(e) => {
                throw(env, "java/lang/IllegalStateException", e);
                ptr::null_mut()
            }
        })
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}
