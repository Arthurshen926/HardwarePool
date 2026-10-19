use crate::ProtocolError;
use capyio_core::{
    AuthorizationState, FormatDescriptor, ProfileId, Route, RouteBackend, RouteState, StreamId,
};

/// Plain CAVC controlled-lab endpoint, not an authenticated media transport.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CameraDataOffer {
    pub port: u16,
    pub stream_id: StreamId,
}
impl CameraDataOffer {
    pub fn validate(self, route: &Route) -> Result<(), ProtocolError> {
        if self.port != 38173
            || self.stream_id.as_uuid().is_nil()
            || route.epoch == 0
            || route.epoch > i32::MAX as u64
            || route.source.node_id == route.sink.node_id
            || route.profile != ProfileId::video_frames_v1()
            || route.backend != RouteBackend::AdapterManaged
            || route.selected_format != Some(FormatDescriptor::new("cavc-avc-access-unit-v1"))
            || !matches!(route.state, RouteState::Starting | RouteState::Active)
            || !matches!(route.authorization, AuthorizationState::Authorized { expires_at_ms: Some(expiry) } if (1..=9_007_199_254_740_991).contains(&expiry))
        {
            return Err(ProtocolError::InvalidRouteControl(
                "invalid controlled-lab camera offer",
            ));
        }
        Ok(())
    }
}
