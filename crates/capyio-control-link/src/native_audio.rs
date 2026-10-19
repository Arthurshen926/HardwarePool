use capyio_core::{AuthorizationState, FormatDescriptor, RouteId, SessionId, StreamId};
use capyio_protocol::{
    NativeAudioDataOffer, NativeAudioEndpoint, NativeAudioPreset, ValidatedRouteStatus,
};
use capyio_runtime::NodeRuntime;

use crate::{CatalogControlLink, CatalogLinkError};

/// Ephemeral preparation result. Not an authorization token: the owner must
/// revalidate before starting/restarting and supervise lease loss/disconnect.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeAudioWorkerPreparation {
    pub session_id: SessionId,
    pub route_id: RouteId,
    pub stream_id: StreamId,
    pub epoch: u32,
    pub local_port: u16,
    pub peer_port: u16,
    pub local_endpoint: NativeAudioEndpoint,
    pub preset: NativeAudioPreset,
    pub authorization_expires_at_ms: u64,
    pub local_lease_expires_at_ms: u64,
}

impl CatalogControlLink {
    /// Check a fresh runtime snapshot and reciprocal offers before opening a
    /// worker. This neither sends data nor marks a Route Active. Peer IP must
    /// still be taken from this authenticated connection, never offer metadata.
    pub fn prepare_native_audio_worker(
        &self,
        runtime: &NodeRuntime,
        route_id: RouteId,
        local: NativeAudioDataOffer,
        remote: &ValidatedRouteStatus,
        trusted_lab_confirmed: bool,
        now_ms: u64,
    ) -> Result<NativeAudioWorkerPreparation, CatalogLinkError> {
        self.validate_runtime(runtime)?;
        let reject = CatalogLinkError::NativeAudioNotReady;
        if remote.route.id != route_id {
            return Err(reject("audio reply belongs to another Route"));
        }
        if !trusted_lab_confirmed {
            return Err(reject("native UDP requires explicit trusted-lab policy"));
        }
        let peer = remote
            .native_audio_data
            .as_deref()
            .copied()
            .ok_or_else(|| reject("peer did not return an audio endpoint"))?;
        let route = runtime.route(remote.route.id)?;
        for (matches, reason) in [
            (
                route.session_id == self.session_id()
                    && remote.session_id == self.session_id()
                    && remote.route.session_id == route.session_id,
                "audio offer session mismatch",
            ),
            (
                route.source == remote.route.source,
                "audio offer Source mismatch",
            ),
            (route.sink == remote.route.sink, "audio offer Sink mismatch"),
            (
                route.profile == remote.route.profile,
                "audio offer profile mismatch",
            ),
            (
                route.backend == remote.route.backend,
                "audio offer backend mismatch",
            ),
            (
                route.epoch == remote.route.epoch,
                "audio offer epoch mismatch",
            ),
            (
                route.authorization == remote.route.authorization,
                "audio offer finite grant mismatch",
            ),
            (
                local.stream_id == peer.stream_id,
                "audio offer Stream mismatch",
            ),
            (
                local.endpoint != peer.endpoint,
                "audio offer endpoint direction mismatch",
            ),
            (local.preset == peer.preset, "audio offer preset mismatch"),
            (
                local.owner(route) == self.local_node_id && peer.owner(route) == self.peer_node_id,
                "audio offer owner mismatch",
            ),
        ] {
            if !matches {
                return Err(reject(reason));
            }
        }
        // Reuse the closed protocol contract for profile/state/backend/bounds.
        capyio_protocol::encode_route_status_with_native_audio_data(
            route.session_id,
            route,
            local,
        )?;
        capyio_protocol::encode_route_status_with_native_audio_data(
            route.session_id,
            &remote.route,
            peer,
        )?;
        let exact_format = Some(FormatDescriptor::new(local.preset.format_name()));
        if route.selected_format != exact_format
            || remote.route.selected_format != exact_format
            || route.selected_qos_mode.is_none()
            || route.selected_qos_mode != remote.route.selected_qos_mode
        {
            return Err(reject("audio format or QoS was not negotiated exactly"));
        }
        let AuthorizationState::Authorized {
            expires_at_ms: Some(expiry),
        } = route.authorization
        else {
            return Err(reject("audio grant must be finite"));
        };
        if expiry <= now_ms {
            return Err(reject("audio grant expired"));
        }
        let port = match local.endpoint {
            NativeAudioEndpoint::Source => route.source,
            NativeAudioEndpoint::Sink => route.sink,
        };
        let claim = runtime
            .route_port_lease_claim(route.id, port)
            .ok_or_else(|| reject("local audio Port lease is missing"))?;
        if claim.session_id != route.session_id
            || claim.route_id != route.id
            || claim.port != port
            || claim.expires_at_ms <= now_ms
            || claim.expires_at_ms > expiry
        {
            return Err(reject("local audio Port lease is invalid or expired"));
        }
        Ok(NativeAudioWorkerPreparation {
            session_id: route.session_id,
            route_id: route.id,
            stream_id: local.stream_id,
            epoch: route.epoch as u32,
            local_port: local.port,
            peer_port: peer.port,
            local_endpoint: local.endpoint,
            preset: local.preset,
            authorization_expires_at_ms: expiry,
            local_lease_expires_at_ms: claim.expires_at_ms,
        })
    }
}
