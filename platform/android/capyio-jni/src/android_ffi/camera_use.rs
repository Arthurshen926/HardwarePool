use super::*;
use capyio_core::*;
use capyio_protocol::CameraDataOffer;

const SINK_CAP: &str = "d41251b3-63ed-4e83-807e-4c4ab9b20e21";
const SINK_PORT: &str = "d41251b3-63ed-4e83-807e-4c4ab9b20e31";

fn local(host: &AndroidCatalogRuntimeV1, r: &Route) -> bool {
    let node = host.runtime.snapshot().local_node;
    r.source.node_id == node.id
        && r.profile == ProfileId::video_frames_v1()
        && node
            .capabilities
            .get(&r.source.capability_id)
            .is_some_and(|c| c.class == CapabilityClass::Camera)
}
fn authority(host: &AndroidCatalogRuntimeV1, r: &Route, now: u64) -> Option<u64> {
    crate::camera_sender::source_authority(&host.runtime, r, now)
}
pub(super) fn fail(host: &mut AndroidCatalogRuntimeV1, id: RouteId) -> Result<(), String> {
    host.runtime
        .report_route_offline(
            id,
            Problem {
                id: ProblemId::new(),
                code: "CAPY.ANDROID.CAMERA_ROUTE_FAILED".into(),
                category: ProblemCategory::Adapter,
                severity: ProblemSeverity::Error,
                retryable: true,
                related_node: None,
                related_adapter: None,
                related_route: Some(id),
                human_message: "相机发送失败或授权已结束".into(),
                technical_detail: None,
            },
        )
        .map_err(|e| e.to_string())?;
    host.audio_status_pending.insert(id);
    Ok(())
}
pub(super) fn receive_status(
    host: &mut AndroidCatalogRuntimeV1,
    peer: &Route,
    offer: Option<CameraDataOffer>,
    secure_offer: Option<capyio_protocol::SecureCameraDataOffer>,
    control_binding: capyio_control_tls::ControlTlsBinding,
    address: String,
    now: u64,
) -> Result<(), String> {
    if !local(host, peer) {
        return Ok(());
    }
    if host.camera_use.as_ref().is_some_and(|j| {
        j.route_id != peer.id
            && host.runtime.route(j.route_id).is_ok_and(|r| {
                !matches!(
                    r.state,
                    RouteState::Stopped | RouteState::Offline | RouteState::Failed
                ) && r.authorization != AuthorizationState::Denied
            })
    }) {
        // The independent Source lease denies concurrent camera use; don't replace its owner.
        return Ok(());
    }
    if host
        .camera_use
        .as_ref()
        .is_none_or(|j| j.route_id != peer.id)
    {
        host.camera_use = Some(crate::AndroidCameraUse {
            route_id: peer.id,
            offer: None,
            secure_transport: false,
            secure_offer: None,
            secure_binding: None,
            secure_generation: crate::camera_job::GenerationClaim::default(),
            local_intent: false,
        });
    }
    if let Some(secure) = secure_offer {
        let route = host.runtime.route(peer.id).map_err(|e| e.to_string())?;
        secure.validate(route).map_err(|e| e.to_string())?;
        if authority(host, route, now).is_none() {
            return Err("secure camera offer lacks current Source authority".into());
        }
        // Keep the secure endpoint native-only; no Java Socket fallback.
        let binding = capyio_control_tls::camera::CameraTlsBinding::from_route(
            control_binding,
            route,
            secure.stream_id,
            now,
        )
        .map_err(|e| e.to_string())?;
        let job = host.camera_use.as_mut().ok_or("camera job missing")?;
        if job
            .secure_offer
            .is_some_and(|(old, epoch)| old != secure || epoch != peer.epoch)
        {
            return Err("secure camera offer changed within Route".into());
        }
        job.secure_offer = Some((secure, peer.epoch));
        job.secure_binding = Some(binding);
        job.offer = None;
        job.secure_transport = true;
    }
    if let Some(offer) = offer {
        if host
            .camera_use
            .as_ref()
            .is_some_and(|job| job.secure_transport)
        {
            return Err("secure camera Route cannot downgrade to plaintext".into());
        }
        let r = host.runtime.route(peer.id).map_err(|e| e.to_string())?;
        if authority(host, r, now).is_none()
            || offer.port != 38173
            || peer.epoch == 0
            || peer.epoch > i32::MAX as u64
        {
            return Err("camera offer lacks current Source authority".into());
        }
        let job = host.camera_use.as_mut().ok_or("camera job missing")?;
        if job
            .offer
            .as_ref()
            .is_some_and(|(old, _, epoch)| *old != offer || *epoch != peer.epoch)
        {
            return Err("camera offer changed within Route".into());
        }
        job.offer = Some((offer, address, peer.epoch));
    }
    let r = host
        .runtime
        .route(peer.id)
        .map_err(|e| e.to_string())?
        .clone();
    if peer.authorization == AuthorizationState::Denied
        && r.authorization == AuthorizationState::Pending
    {
        host.runtime
            .deny_pending_route(r.id)
            .map_err(|e| e.to_string())?;
    }
    if matches!(
        peer.state,
        RouteState::Stopped | RouteState::Failed | RouteState::Offline
    ) && matches!(
        r.state,
        RouteState::Prepared | RouteState::Starting | RouteState::Active
    ) {
        host.runtime
            .begin_route_stop(r.id)
            .map_err(|e| e.to_string())?;
        host.runtime.stop_route(r.id).map_err(|e| e.to_string())?;
    } else if matches!(
        peer.state,
        RouteState::Stopped | RouteState::Failed | RouteState::Offline
    ) && r.state == RouteState::Draft
        && matches!(r.authorization, AuthorizationState::Authorized { .. })
    {
        fail(host, r.id)?;
    }
    secure_camera::reconcile(&host.runtime);
    Ok(())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeCatalogRuntime_nativeCameraUse(
    mut env: EnvUnowned<'_>,
    _class: JClass<'_>,
    handle: jlong,
    action: JString<'_>,
    peer: JString<'_>,
) -> jbyteArray {
    env.with_env(|env|->jni::errors::Result<_>{
        let result=(||->Result<Vec<u8>,String>{
            let action=string(env,action)?;let peer=string(env,peer)?;
            let mut runtimes=catalog_runtimes().lock().map_err(|_|"Runtime registry poisoned")?;
            let host=runtimes.get_mut(&catalog_handle(handle)?).ok_or("Runtime absent")?;
            let now=SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e|e.to_string())?.as_millis()as u64;
            host.runtime.expire_port_leases(now);
            if action=="stop" || action=="failed" { secure_camera::stop_runtime(handle as u64,None); }
            if action=="peers" {
                let s=host.runtime.snapshot();
                let peers:Vec<_>=s.peers.iter().filter(|p|s.sessions.iter().any(|session|session.remote_node_id==p.id&&session.state==SessionState::Ready)
                    &&p.capabilities.values().any(|c|c.id.to_string()==SINK_CAP&&c.ports.values().any(|p|p.id.to_string()==SINK_PORT&&p.direction==PortDirection::Sink&&p.profile==ProfileId::video_frames_v1())))
                    .map(|p|serde_json::json!({"nodeId":p.id.to_string(),"displayName":p.display_name})).collect();
                return serde_json::to_vec(&peers).map_err(|e|e.to_string());
            }
            if let Some(job)=host.camera_use.clone(){
                let r=host.runtime.route(job.route_id).map_err(|e|e.to_string())?.clone();
                if matches!(r.state,RouteState::Starting|RouteState::Active)&&(authority(host,&r,now).is_none()||secure_camera::failed_for_route(handle as u64,r.id)){secure_camera::stop_runtime(handle as u64,Some(r.id));fail(host,r.id)?;}
                if action=="stop" {
                    if r.state==RouteState::Draft&&r.authorization==AuthorizationState::Pending{host.runtime.deny_pending_route(r.id).map_err(|e|e.to_string())?;}
                    else if matches!(r.state,RouteState::Prepared|RouteState::Starting|RouteState::Active){host.runtime.begin_route_stop(r.id).map_err(|e|e.to_string())?;host.runtime.stop_route(r.id).map_err(|e|e.to_string())?;}
                    host.audio_status_pending.insert(r.id);
                }else if action=="active"&&!job.secure_transport&&r.state==RouteState::Starting&&authority(host,&r,now).is_some(){host.runtime.activate_route(r.id).map_err(|e|e.to_string())?;host.audio_status_pending.insert(r.id);}
                else if action=="failed"&&matches!(r.state,RouteState::Draft|RouteState::Prepared|RouteState::Starting|RouteState::Active){fail(host,r.id)?;}
            }
            if action=="start" {
                if host.camera_use.as_ref().is_some_and(|j|host.runtime.route(j.route_id).is_ok_and(|r|!matches!(r.state,RouteState::Stopped|RouteState::Failed|RouteState::Offline)&&r.authorization!=AuthorizationState::Denied)){return Err("camera already in use".into())}
                let s=host.runtime.snapshot();let session=s.sessions.iter().find(|s|s.remote_node_id.to_string()==peer&&s.state==SessionState::Ready).ok_or("peer not ready")?;
                let cap=s.local_node.capabilities.values().find(|c|c.class==CapabilityClass::Camera).ok_or("camera unavailable")?;
                let port=cap.ports.values().find(|p|p.direction==PortDirection::Source&&p.profile==ProfileId::video_frames_v1()).ok_or("camera Source absent")?;
                let source=PortRef{node_id:s.local_node.id,capability_id:cap.id,port_id:port.id};
                let sink=PortRef{node_id:session.remote_node_id,capability_id:SINK_CAP.parse().map_err(|_|"invalidcap")?,port_id:SINK_PORT.parse().map_err(|_|"invalidport")?};
                let id=host.runtime.create_route(session.id,source,sink,RouteBackend::AdapterManaged).map_err(|e|e.to_string())?;
                host.camera_use=Some(crate::AndroidCameraUse{route_id:id,offer:None,secure_transport:false,secure_offer:None,secure_binding:None,secure_generation:crate::camera_job::GenerationClaim::default(),local_intent:true});host.audio_status_pending.insert(id);
            }
            let output=if let Some(job)=&host.camera_use {
                let r=host.runtime.route(job.route_id).map_err(|e|e.to_string())?;let expiry=authority(host,r,now);
                let secure_ready=secure_camera::ready_for_route(handle as u64,r.id);
                let should_run=expiry.is_some()&&matches!(r.state,RouteState::Starting|RouteState::Active)&&if job.secure_transport {secure_ready&&job.secure_offer.is_some_and(|(_,epoch)|epoch==r.epoch)}else{job.offer.as_ref().is_some_and(|(_,_,epoch)|*epoch==r.epoch)};
                let s=host.runtime.snapshot();
                serde_json::json!({"routeId":r.id.to_string(),"peerNodeId":r.sink.node_id.to_string(),"status":if r.authorization==AuthorizationState::Denied{"idle"}else{match r.state{RouteState::Draft=>"awaiting_consent",RouteState::Prepared|RouteState::Starting=>"starting",RouteState::Active=>"active",RouteState::Stopping=>"stopping",RouteState::Stopped=>"idle",_=>"failed"}},
                    "shouldRun":should_run,"secureTransport":job.secure_transport,"secureCapturePending":job.secure_transport&&!secure_ready,"secureConfig":job.secure_offer.map(|(o,epoch)|serde_json::json!({"streamId":o.stream_id.to_string(),"epoch":epoch})),"expiresAtMs":expiry,"config":job.offer.as_ref().map(|(o,host,epoch)|serde_json::json!({"host":host,"port":o.port,"streamId":o.stream_id.to_string(),"epoch":epoch})),
                    "error":s.problems.iter().rev().find(|p|p.related_route==Some(r.id)).map(|p|format!("{}: {}",p.code,p.human_message))})
            }else{serde_json::json!({"status":"idle"})};
            serde_json::to_vec(&output).map_err(|e|e.to_string())
        })();
        Ok(match result{Ok(bytes)=>byte_array(env,&bytes),Err(e)=>{throw(env,"java/lang/IllegalStateException",e);ptr::null_mut()}})
    }).resolve::<ThrowRuntimeExAndDefault>()
}
