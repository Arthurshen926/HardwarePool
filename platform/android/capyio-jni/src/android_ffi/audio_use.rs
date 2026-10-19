use super::*;
use capyio_core::*;
use capyio_protocol::{NativeAudioDataOffer, NativeAudioEndpoint, NativeAudioPreset};

fn local_port(host: &AndroidCatalogRuntimeV1, route: &Route) -> Option<(PortRef, &'static str)> {
    let local = host.runtime.snapshot().local_node;
    for (port, class, direction) in [
        (route.source, CapabilityClass::Microphone, "microphone"),
        (route.sink, CapabilityClass::Speaker, "speaker"),
    ] {
        if port.node_id == local.id
            && local
                .capabilities
                .get(&port.capability_id)
                .is_some_and(|cap| cap.class == class)
            && route.profile == ProfileId::audio_frames_v1()
        {
            return Some((port, direction));
        }
    }
    None
}

fn authority(host: &AndroidCatalogRuntimeV1, route: &Route, now: u64) -> Option<u64> {
    let (port, _) = local_port(host, route)?;
    let AuthorizationState::Authorized {
        expires_at_ms: Some(expiry),
    } = route.authorization
    else {
        return None;
    };
    (expiry > now
        && host
            .runtime
            .session(route.session_id)
            .is_ok_and(|s| s.state == SessionState::Ready)
        && host
            .runtime
            .route_port_lease_claim(route.id, port)
            .is_some_and(|lease| lease.expires_at_ms > now))
    .then_some(expiry)
}

pub(super) fn receive_offer(
    host: &mut AndroidCatalogRuntimeV1,
    peer: &Route,
    offer: NativeAudioDataOffer,
    address: String,
    now: u64,
) -> Result<(), String> {
    let local = host.runtime.route(peer.id).map_err(|e| e.to_string())?;
    let (_, direction) = local_port(host, local).ok_or("audio offer is not for local hardware")?;
    if authority(host, local, now).is_none()
        || offer.owner(peer) == local.source.node_id && direction == "microphone"
        || offer.owner(peer) == local.sink.node_id && direction == "speaker"
        || offer.preset
            != if direction == "microphone" {
                NativeAudioPreset::PcmS16le48000Mono
            } else {
                NativeAudioPreset::PcmS16le48000Stereo
            }
    {
        return Err("audio offer lacks current local authority or preset".into());
    }
    if let Some(existing) = host.audio_uses.get(&peer.id) {
        if existing.offer != offer || existing.peer_route.epoch != peer.epoch {
            return Err("audio offer changed within one Route epoch".into());
        }
        return Ok(());
    }
    host.audio_uses.retain(|id, _| {
        host.runtime.route(*id).is_ok_and(|r| {
            !matches!(
                r.state,
                RouteState::Stopped | RouteState::Offline | RouteState::Failed
            )
        })
    });
    if host.audio_uses.len() >= 2 {
        return Err("audio hardware is busy".into());
    }
    host.audio_uses.insert(
        peer.id,
        crate::AndroidAudioUse {
            peer_route: peer.clone(),
            offer,
            peer_address: address,
            local_port: None,
            offer_pending: false,
            observed_state: local.state,
        },
    );
    Ok(())
}

pub(super) fn remote_status(
    host: &mut AndroidCatalogRuntimeV1,
    peer: &Route,
) -> Result<(), String> {
    if local_port(host, peer).is_none() {
        return Ok(());
    }
    let local = host
        .runtime
        .route(peer.id)
        .map_err(|e| e.to_string())?
        .clone();
    if peer.authorization == AuthorizationState::Denied
        && local.authorization == AuthorizationState::Pending
    {
        host.runtime
            .deny_pending_route(peer.id)
            .map_err(|e| e.to_string())?;
    }
    if matches!(
        peer.state,
        RouteState::Stopped | RouteState::Offline | RouteState::Failed
    ) && matches!(
        local.state,
        RouteState::Prepared | RouteState::Starting | RouteState::Active
    ) {
        host.runtime
            .begin_route_stop(peer.id)
            .map_err(|e| e.to_string())?;
        host.runtime
            .stop_route(peer.id)
            .map_err(|e| e.to_string())?;
    } else if matches!(
        peer.state,
        RouteState::Stopped | RouteState::Offline | RouteState::Failed
    ) && local.state == RouteState::Draft
        && matches!(local.authorization, AuthorizationState::Authorized { .. })
    {
        // A receiver may terminate after the offer but before Start. Draft is
        // not stoppable in core; terminate this authorized preparation explicitly.
        fail(host, peer.id)?;
    }
    Ok(())
}

pub(super) fn publish(
    host: &mut AndroidCatalogRuntimeV1,
    link: &mut CatalogControlLink,
    now: u64,
) -> Result<(), String> {
    for id in std::mem::take(&mut host.audio_status_pending) {
        let route = host.runtime.route(id).map_err(|e| e.to_string())?;
        if route.session_id == link.session_id() {
            let selected = host.published_source_capabilities();
            link.send_catalog(
                link.prepare_catalog(&host.runtime, &selected)
                    .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            link.send_route_status(route).map_err(|e| e.to_string())?;
        } else {
            host.audio_status_pending.insert(id);
        }
    }
    let ids: Vec<_> = host
        .audio_uses
        .iter()
        .filter(|(_, job)| job.offer_pending)
        .map(|(id, _)| *id)
        .collect();
    for id in ids {
        let route = host.runtime.route(id).map_err(|e| e.to_string())?.clone();
        if route.session_id != link.session_id() || authority(host, &route, now).is_none() {
            continue;
        }
        let job = host.audio_uses.get_mut(&id).ok_or("audio job missing")?;
        let mut advertisement = job.peer_route.clone();
        advertisement.authorization = route.authorization;
        let offer = NativeAudioDataOffer {
            port: job.local_port.ok_or("audio port not prepared")?,
            stream_id: job.offer.stream_id,
            endpoint: if job.offer.endpoint == NativeAudioEndpoint::Source {
                NativeAudioEndpoint::Sink
            } else {
                NativeAudioEndpoint::Source
            },
            preset: job.offer.preset,
        };
        link.send_native_audio_data_offer(&advertisement, offer, now)
            .map_err(|e| e.to_string())?;
        job.offer_pending = false;
    }
    Ok(())
}

fn fail(host: &mut AndroidCatalogRuntimeV1, id: RouteId) -> Result<(), String> {
    fail_message(host, id, "音频硬件启动失败或授权已结束")
}

fn fail_message(
    host: &mut AndroidCatalogRuntimeV1,
    id: RouteId,
    message: &str,
) -> Result<(), String> {
    host.runtime
        .report_route_offline(
            id,
            Problem {
                id: ProblemId::new(),
                code: "CAPY.ANDROID.AUDIO_ROUTE_FAILED".into(),
                category: ProblemCategory::Adapter,
                severity: ProblemSeverity::Error,
                retryable: true,
                related_node: None,
                related_adapter: None,
                related_route: Some(id),
                human_message: message.into(),
                technical_detail: None,
            },
        )
        .map_err(|e| e.to_string())?;
    host.audio_status_pending.insert(id);
    Ok(())
}

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
            let action=string(env,action)?;let id=string(env,id)?;let peer=string(env,peer)?;let direction=string(env,direction)?;
            let mut registry=catalog_runtimes().lock().map_err(|_|"Runtime registry poisoned")?;
            let host=registry.get_mut(&catalog_handle(handle)?).ok_or("unknown Runtime")?;
            let now=SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e|e.to_string())?.as_millis() as u64;
            host.runtime.expire_port_leases(now);
            if action=="peers" {
                let snapshot=host.runtime.snapshot();
                let peers:Vec<_>=snapshot.peers.iter().filter(|peer|snapshot.sessions.iter().any(|s|s.remote_node_id==peer.id&&s.state==SessionState::Ready)).filter_map(|peer|{
                    let directions:Vec<_>=[("microphone","d41251b3-63ed-4e83-807e-4c4ab9b20d21",PortDirection::Sink),("speaker","d41251b3-63ed-4e83-807e-4c4ab9b20d22",PortDirection::Source)].into_iter()
                        .filter(|(_,id,direction)|peer.capabilities.values().any(|cap|cap.id.to_string()==*id&&cap.ports.values().any(|p|p.direction==*direction&&p.profile==ProfileId::audio_frames_v1())))
                        .map(|(direction,_,_)|direction).collect();
                    (!directions.is_empty()).then(||serde_json::json!({"nodeId":peer.id.to_string(),"displayName":peer.display_name,"directions":directions}))
                }).collect();
                return serde_json::to_vec(&peers).map_err(|e|e.to_string());
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
                let snapshot=host.runtime.snapshot();
                if snapshot.routes.iter().any(|r|local_port(host,r).is_some_and(|(_,d)|d==direction)
                    && !matches!(r.state,RouteState::Stopped|RouteState::Offline|RouteState::Failed)
                    && r.authorization!=AuthorizationState::Denied) {return Err("audio direction already has a pending or running Route".into());}
                let session=snapshot.sessions.iter().find(|s|s.remote_node_id.to_string()==peer && s.state==SessionState::Ready).ok_or("peer is not connected")?;
                let (class,suffix)=match direction.as_str(){"microphone"=>(CapabilityClass::Microphone,("21","31")),"speaker"=>(CapabilityClass::Speaker,("22","32")),_=>return Err("unknown audio direction".into())};
                let cap=snapshot.local_node.capabilities.values().find(|c|c.class==class).ok_or("audio hardware not published")?;
                let endpoint=cap.ports.values().find(|p|p.profile==ProfileId::audio_frames_v1()).ok_or("audio Port absent")?;
                let local=PortRef{node_id:snapshot.local_node.id,capability_id:cap.id,port_id:endpoint.id};
                let remote=PortRef{node_id:session.remote_node_id,capability_id:format!("d41251b3-63ed-4e83-807e-4c4ab9b20d{}",suffix.0).parse().map_err(|_|"invalid cap")?,port_id:format!("d41251b3-63ed-4e83-807e-4c4ab9b20d{}",suffix.1).parse().map_err(|_|"invalid port")?};
                let (source,sink)=if direction=="microphone"{(local,remote)}else{(remote,local)};
                let id=host.runtime.create_route(session.id,source,sink,RouteBackend::AdapterManaged).map_err(|e|e.to_string())?;
                host.audio_local_intent.insert(id);host.audio_status_pending.insert(id);
            } else if action=="prepared" {
                let id:RouteId=id.parse().map_err(|_|"invalid audio Route")?;
                let job=host.audio_uses.get_mut(&id).ok_or("audio offer absent")?;
                job.local_port=Some(u16::try_from(port).ok().filter(|p|*p>0).ok_or("invalid audio port")?);job.offer_pending=true;
            } else if action=="active" || action=="failed" || action=="busy" {
                let id:RouteId=id.parse().map_err(|_|"invalid audio Route")?;
                let route=host.runtime.route(id).map_err(|e|e.to_string())?;
                if action=="active" && route.state==RouteState::Starting && authority(host,route,now).is_some() {
                    host.runtime.activate_route(id).map_err(|e|e.to_string())?;host.audio_status_pending.insert(id);
                } else if action=="busy" && matches!(route.state,RouteState::Draft|RouteState::Prepared|RouteState::Starting|RouteState::Active){fail_message(host,id,"本机音频正由另一项使用（可能是旧版高级入口）占用。请先停止原会话，再从跨设备音频开始；不会自动接管或停止已有音频。")?;}
                else if action=="failed" && matches!(route.state,RouteState::Draft|RouteState::Prepared|RouteState::Starting|RouteState::Active){fail(host,id)?;}
            } else if action=="stop" {
                let ids:Vec<_>=host.runtime.snapshot().routes.iter().filter(|r|local_port(host,r).is_some_and(|(_,d)|d==direction)).map(|r|r.id).collect();
                for id in ids {let route=host.runtime.route(id).map_err(|e|e.to_string())?.clone();
                    if route.state==RouteState::Draft && route.authorization==AuthorizationState::Pending {host.runtime.deny_pending_route(id).map_err(|e|e.to_string())?;}
                    else if matches!(route.state,RouteState::Prepared|RouteState::Starting|RouteState::Active){host.runtime.begin_route_stop(id).map_err(|e|e.to_string())?;host.runtime.stop_route(id).map_err(|e|e.to_string())?;}
                    host.audio_status_pending.insert(id);
                }
            }
            let snapshot=host.runtime.snapshot();
            let mut output=Vec::new();
            for route in &snapshot.routes {
                let Some((_,direction))=local_port(host,route)else{continue};
                let expires=authority(host,route,now);
                let job=host.audio_uses.get(&route.id);
                let live=expires.is_some() && !matches!(route.state,RouteState::Stopped|RouteState::Failed|RouteState::Offline) && route.authorization!=AuthorizationState::Denied
                    && job.is_none_or(|job|!matches!(route.state,RouteState::Starting|RouteState::Active)||route.epoch==job.peer_route.epoch);
                output.push(serde_json::json!({"routeId":route.id.to_string(),"sessionId":route.session_id.to_string(),"peerNodeId":if direction=="microphone"{route.sink.node_id}else{route.source.node_id}.to_string(),"direction":direction,
                    "status":if route.authorization==AuthorizationState::Denied{"idle"}else{match route.state{RouteState::Draft=>if route.authorization==AuthorizationState::Pending{"awaiting_consent"}else{"starting"},RouteState::Prepared|RouteState::Starting=>"starting",RouteState::Active=>"active",RouteState::Stopping=>"stopping",RouteState::Stopped=>"idle",_=>"failed"}},
                    "endpointPrepared":job.is_some_and(|job|job.local_port.is_some()),
                    "waitingMedia":direction=="speaker"&&route.state==RouteState::Starting,
                    "message":if direction=="speaker"&&route.state==RouteState::Starting{Some("扬声器已就绪，等待 Windows 播放音频；尚未收到媒体帧")}else{None},
                    "shouldPrepare":live&&job.is_some(),"shouldRun":live&&matches!(route.state,RouteState::Starting|RouteState::Active),"expiresAtMs":expires,
                    "error":snapshot.problems.iter().rev().find(|p|p.related_route==Some(route.id)).map(|p|format!("{}: {}",p.code,p.human_message)),
                    "config":job.map(|job|serde_json::json!({"host":job.peer_address,"peerPort":job.offer.port,"streamId":job.offer.stream_id.to_string(),"epoch":job.peer_route.epoch}))}));
            }
            serde_json::to_vec(&output).map_err(|e|e.to_string())
        })();
        Ok(match result{Ok(bytes)=>byte_array(env,&bytes),Err(error)=>{throw(env,"java/lang/IllegalStateException",error);ptr::null_mut()}})
    }).resolve::<ThrowRuntimeExAndDefault>()
}
