use super::*;
use crate::audio_negotiation::{authority, fail, fail_message, local_port};
use crate::audio_selection::{
    ADMISSION_UNSUPPORTED, AudioDirection, select_endpoint, supports_inbound_draft,
};
use capyio_core::*;

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeCatalogRuntime_nativeAudioUse(
    mut env: EnvUnowned<'_>,
    _class: JClass<'_>,
    handle: jlong,
    action: JString<'_>,
    id: JString<'_>,
    peer: JString<'_>,
    direction: JString<'_>,
    port: jint,
) -> jbyteArray {
    env.with_env(|env|->jni::errors::Result<_>{
        let result=(||->Result<Vec<u8>,String>{
            secure_audio::reap_finished(Some(catalog_handle(handle)?));
            let action=string(env,action)?;let id=string(env,id)?;let peer=string(env,peer)?;let direction=string(env,direction)?;
            let mut registry=catalog_runtimes().lock().map_err(|_|"Runtime registry poisoned")?;
            let host=registry.get_mut(&catalog_handle(handle)?).ok_or("unknown Runtime")?;
            let now=SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e|e.to_string())?.as_millis() as u64;
            host.runtime.expire_port_leases(now);
            if action=="peers" {
                let snapshot=host.runtime.snapshot();
                let peers:Vec<_>=snapshot.peers.iter().filter(|peer|snapshot.sessions.iter().any(|s|s.remote_node_id==peer.id&&s.state==SessionState::Ready)).filter_map(|peer|{
                    let mut directions = Vec::new();
                    let mut unavailable_directions = Vec::new();
                    for direction in [AudioDirection::Microphone, AudioDirection::Speaker] {
                        if let Ok(endpoint) = select_endpoint(peer, direction, false) {
                            if supports_inbound_draft(peer, endpoint, direction)
                                && select_endpoint(&snapshot.local_node, direction, true).is_ok() {
                                directions.push(direction.name());
                            } else {
                                unavailable_directions.push(serde_json::json!({"direction":direction.name(),"reason":ADMISSION_UNSUPPORTED}));
                            }
                        }
                    }
                    if crate::audio_selection::select_secure_endpoint(&snapshot.local_node,true).is_ok()
                        && crate::audio_selection::select_secure_endpoint(peer,false).is_ok() {
                        directions.push("microphone-secure");
                    }
                    (!directions.is_empty() || !unavailable_directions.is_empty()).then(||serde_json::json!({"nodeId":peer.id.to_string(),"displayName":peer.display_name,"directions":directions,"unavailableDirections":unavailable_directions}))
                }).collect();
                return serde_json::to_vec(&peers).map_err(|e|e.to_string());
            }
            secure_audio::reconcile(&host.runtime);
            let secure_ids:Vec<_>=host.secure_audio_uses.keys().copied().collect();
            for id in secure_ids {
                if (secure_audio::failed_for_route(catalog_handle(handle)?,id)
                    || host.secure_audio_uses.get(&id).is_some_and(|job|job.terminal==Some(crate::audio_job::FAILED)))
                    && host.runtime.route(id).is_ok_and(|r|matches!(r.state,RouteState::Starting|RouteState::Active)) { fail(host,id)?; }
            }
            let running:Vec<_>=host.audio_uses.keys().copied().collect();
            for id in running {
                let route=host.runtime.route(id).map_err(|e|e.to_string())?.clone();
                if matches!(route.state,RouteState::Starting|RouteState::Active)&&authority(host,&route,now).is_none(){fail(host,id)?;}
                let state=host.runtime.route(id).map_err(|e|e.to_string())?.state;
                let job=host.audio_uses.get_mut(&id).ok_or("audio job missing")?;
                if state!=job.observed_state {job.observed_state=state;host.audio_status_pending.insert(id);}
            }
            if action=="start" {
                let peer = peer.parse().map_err(|_|"invalid peer Node")?;
                host.begin_audio_use(peer, &direction)?;
            } else if action=="prepared" {
                let id:RouteId=id.parse().map_err(|_|"invalid audio Route")?;
                host.prepare_audio_endpoint(id, u16::try_from(port).ok().filter(|p|*p>0).ok_or("invalid audio port")?)?;
            } else if action=="active" || action=="failed" || action=="busy" {
                let id:RouteId=id.parse().map_err(|_|"invalid audio Route")?;
                let route=host.runtime.route(id).map_err(|e|e.to_string())?;
                if action=="active" && route.state==RouteState::Starting && authority(host,route,now).is_some()
                    && (!crate::secure_audio_negotiation::proposed(route) || secure_audio::streaming_for_route(catalog_handle(handle)?,id)) {
                    host.runtime.activate_route(id).map_err(|e|e.to_string())?;host.audio_status_pending.insert(id);
                } else if action=="busy" && matches!(route.state,RouteState::Draft|RouteState::Prepared|RouteState::Starting|RouteState::Active){fail_message(host,id,"本机音频正由另一项使用（可能是旧版高级入口）占用。请先停止原会话，再从跨设备音频开始；不会自动接管或停止已有音频。")?;}
                else if action=="failed" && matches!(route.state,RouteState::Draft|RouteState::Prepared|RouteState::Starting|RouteState::Active){fail(host,id)?;}
            } else if action=="stop" {
                if direction=="microphone-secure" { secure_audio::stop_runtime(catalog_handle(handle)?,None); }
                host.stop_audio_direction(&direction)?;
            }
            secure_audio::reconcile(&host.runtime);
            let snapshot=host.runtime.snapshot();
            let mut output=Vec::new();
            for route in &snapshot.routes {
                let Some((_,legacy_direction))=local_port(host,route)else{continue};
                let secure=crate::secure_audio_negotiation::proposed(route);
                let direction=crate::secure_audio_negotiation::direction(route,legacy_direction);
                let secure_job=host.secure_audio_uses.get(&route.id);
                let expires=authority(host,route,now);
                let job=host.audio_uses.get(&route.id);
                let live=expires.is_some() && !matches!(route.state,RouteState::Stopped|RouteState::Failed|RouteState::Offline) && route.authorization!=AuthorizationState::Denied
                    && job.is_none_or(|job|!matches!(route.state,RouteState::Starting|RouteState::Active)||route.epoch==job.peer_route.epoch);
                output.push(serde_json::json!({"routeId":route.id.to_string(),"sessionId":route.session_id.to_string(),"peerNodeId":if legacy_direction=="microphone"{route.sink.node_id}else{route.source.node_id}.to_string(),"direction":direction,
                    "status":if route.authorization==AuthorizationState::Denied{"idle"}else{match route.state{RouteState::Draft=>if route.authorization==AuthorizationState::Pending{"awaiting_consent"}else{"starting"},RouteState::Prepared|RouteState::Starting=>"starting",RouteState::Active=>"active",RouteState::Stopping=>"stopping",RouteState::Stopped=>"idle",_=>"failed"}},
                    "endpointPrepared":if secure{secure_job.is_some()}else{job.is_some_and(|job|job.local_port.is_some())},
                    "waitingMedia":direction=="speaker"&&route.state==RouteState::Starting,
                    "message":if direction=="speaker"&&route.state==RouteState::Starting{Some("扬声器已就绪，等待对端发送音频；尚未收到媒体帧")}else{None},
                    "shouldPrepare":live&&if secure{secure_job.is_some()}else{job.is_some()},
                    "shouldRun":live&&if secure{secure_job.is_some()&&matches!(route.state,RouteState::Starting|RouteState::Active)}else{host.audio_should_run(route.id,now)},"expiresAtMs":expires,
                    "error":snapshot.problems.iter().rev().find(|p|p.related_route==Some(route.id)).map(|p|format!("{}: {}",p.code,p.human_message)),
                    "config":if secure{Some(crate::secure_audio_negotiation::config())}else{job.map(crate::audio_negotiation::legacy_audio_config)}}));
            }
            serde_json::to_vec(&output).map_err(|e|e.to_string())
        })();
        Ok(match result{Ok(bytes)=>byte_array(env,&bytes),Err(error)=>{throw(env,"java/lang/IllegalStateException",error);ptr::null_mut()}})
    }).resolve::<ThrowRuntimeExAndDefault>()
}
