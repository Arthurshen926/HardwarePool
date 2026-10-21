use crate::{CameraBinding, error};
use capyio_core::{
    AuthorizationState, Availability, Platform, PortDirection, RouteBackend, RouteState,
    SessionState,
};
use capyio_runtime::NodeRuntime;
use capyio_video::{VideoStreamSpec, video_frames_profile};
use std::io;

pub(crate) struct Guard {
    pub binding: CameraBinding,
    pub deadline_ms: u64,
    grant_ms: u64,
    last_ms: u64,
}
impl Guard {
    pub fn new(runtime: &NodeRuntime, binding: CameraBinding, now: u64) -> io::Result<Self> {
        let route = runtime.route(binding.route_id).map_err(error)?;
        if route.state != RouteState::Starting {
            return Err(error("camera requires an explicitly Starting Route"));
        }
        let AuthorizationState::Authorized {
            expires_at_ms: Some(grant_ms),
        } = route.authorization
        else {
            return Err(error("finite camera grant required"));
        };
        let lease = runtime
            .route_port_lease_claim(route.id, binding.local_sink)
            .ok_or_else(|| error("camera Sink lease missing"))?;
        let mut guard = Self {
            binding,
            deadline_ms: grant_ms.min(lease.expires_at_ms),
            grant_ms,
            last_ms: now,
        };
        guard.check(runtime, now)?;
        Ok(guard)
    }
    pub fn check(&mut self, runtime: &NodeRuntime, now: u64) -> io::Result<()> {
        let b = &self.binding;
        if now < self.last_ms
            || now >= self.deadline_ms
            || b.stream_epoch == 0
            || b.stream_id.as_uuid().is_nil()
            || b.session_id.as_uuid().is_nil()
            || b.route_id.as_uuid().is_nil()
        {
            return Err(error("expired, regressed or invalid camera binding"));
        }
        self.last_ms = now;
        let route = runtime.route(b.route_id).map_err(error)?;
        let session = runtime.session(b.session_id).map_err(error)?;
        let snapshot = runtime.snapshot();
        if snapshot.local_node.platform != Platform::Linux
            || snapshot.local_node.id != session.local_node_id
            || session.state != SessionState::Ready
            || session.local_node_id == session.remote_node_id
            || route.session_id != b.session_id
            || route.epoch != b.stream_epoch
            || route.sink != b.local_sink
            || route.sink.node_id != session.local_node_id
            || route.source.node_id != session.remote_node_id
            || route.profile != video_frames_profile()
            || route.backend != RouteBackend::AdapterManaged
            || route.selected_format
                != Some(VideoStreamSpec::camera_720p30_nv12().format_descriptor())
            || route.selected_qos_mode != Some(capyio_core::QosMode::Interactive)
            || !matches!(route.state, RouteState::Starting | RouteState::Active)
            || route.authorization
                != (AuthorizationState::Authorized {
                    expires_at_ms: Some(self.grant_ms),
                })
        {
            return Err(error(
                "camera Runtime Session/Route/format/epoch no longer matches",
            ));
        }
        let cap = snapshot
            .local_node
            .capabilities
            .get(&b.local_sink.capability_id)
            .ok_or_else(|| error("camera Sink capability missing"))?;
        let port = cap
            .ports
            .get(&b.local_sink.port_id)
            .ok_or_else(|| error("camera Sink port missing"))?;
        if port.direction != PortDirection::Sink
            || cap.availability != Availability::Available
            || port.availability != Availability::Available
            || port.profile != video_frames_profile()
            || !port
                .formats
                .contains(&VideoStreamSpec::camera_720p30_nv12().format_descriptor())
        {
            return Err(error("camera destination is not a local Sink"));
        }
        let peer = snapshot
            .peers
            .iter()
            .find(|n| n.id == session.remote_node_id)
            .ok_or_else(|| error("camera peer catalog missing"))?;
        let source_cap = peer
            .capabilities
            .get(&route.source.capability_id)
            .ok_or_else(|| error("camera Source capability missing"))?;
        let source = source_cap
            .ports
            .get(&route.source.port_id)
            .ok_or_else(|| error("camera Source port missing"))?;
        if source.direction != PortDirection::Source
            || source_cap.availability != Availability::Available
            || source.availability != Availability::Available
            || source.profile != video_frames_profile()
            || !source
                .formats
                .contains(&VideoStreamSpec::camera_720p30_nv12().format_descriptor())
        {
            return Err(error(
                "camera Source no longer matches available negotiated catalog",
            ));
        }
        let lease = runtime
            .route_port_lease_claim(route.id, b.local_sink)
            .ok_or_else(|| error("camera Sink lease missing"))?;
        if lease.session_id != b.session_id
            || lease.expires_at_ms <= now
            || lease.expires_at_ms > self.grant_ms
        {
            return Err(error("camera Sink lease expired or mismatched"));
        }
        Ok(())
    }
    pub fn owns(&self, runtime: &NodeRuntime) -> bool {
        runtime.route(self.binding.route_id).is_ok_and(|r| {
            r.session_id == self.binding.session_id
                && r.epoch == self.binding.stream_epoch
                && r.sink == self.binding.local_sink
        })
    }
}
