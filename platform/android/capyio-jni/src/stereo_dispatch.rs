//! Explicit production admission seam for a marked Linux stereo Source.
//! The authenticated catalog marker selects a protocol, never grants consent.
//! Unmarked legacy routes are not migrated to the bilateral path.
use crate::AndroidCatalogRuntimeV1;
use capyio_control_link::CatalogControlLink;
use capyio_core::*;
use capyio_protocol::{SECURE_AUDIO_STEREO_FORMAT, ValidatedRouteStatus};

pub const BILATERAL_SOURCE_MARKER: &str = "capyio.secure_stereo.bilateral_consent";
pub const BILATERAL_SOURCE_VERSION: &str = "v1";

impl AndroidCatalogRuntimeV1 {
    pub(crate) fn resolve_dispatched_stereo_request(
        &mut self,
        id: RouteId,
        allow: bool,
    ) -> Result<(), String> {
        let result =
            self.resolve_bilateral_stereo_consent(id, allow, std::time::Duration::from_secs(60));
        // An early click is neither queued nor terminal. All actually resolved
        // denial/failure lineages are terminal, including before any grant.
        if !allow
            || (result.is_err()
                && self
                    .runtime
                    .route(id)
                    .is_ok_and(|route| route.authorization != AuthorizationState::Pending))
        {
            self.bilateral_stereo.invalidate(id);
            self.runtime
                .retire_route_authorization(id)
                .map_err(|e| e.to_string())?;
        }
        result
    }
    /// Called on decoded authenticated RouteStatus before generic audio import.
    /// `None` is exclusively an unmarked route; malformed opt-ins fail closed.
    /// A marked Session is dedicated to this single attempt. Local consent is
    /// still a fresh visible `resolve_touchpad_request` action after peer receipt.
    pub fn receive_bilateral_stereo_status(
        &mut self,
        link: &mut CatalogControlLink,
        status: &ValidatedRouteStatus,
        generation: u64,
    ) -> Result<Option<Route>, String> {
        let id = status.route.id;
        if self.stereo_dispatch_routes.contains(&id) {
            let result = self.receive_stereo_consent(link, status);
            if result.is_err() {
                self.bilateral_stereo.invalidate(id);
                self.runtime
                    .retire_route_authorization(id)
                    .map_err(|e| e.to_string())?;
            }
            result?;
            return self
                .runtime
                .route(id)
                .cloned()
                .map(Some)
                .map_err(|e| e.to_string());
        }
        if self.bilateral_stereo.owns_session(link.session_id()) {
            return Err("bilateral dedicated Session already owns an attempt".into());
        }
        let snapshot = self.runtime.snapshot();
        let source = snapshot
            .peers
            .iter()
            .find(|p| p.id == status.route.source.node_id);
        let capability =
            source.and_then(|p| p.capabilities.get(&status.route.source.capability_id));
        let Some(marker) = capability.and_then(|c| c.metadata.get(BILATERAL_SOURCE_MARKER)) else {
            return Ok(None);
        };
        let source = source.ok_or("bilateral source Node absent")?;
        let capability = capability.ok_or("bilateral source Capability absent")?;
        let port = capability
            .ports
            .get(&status.route.source.port_id)
            .ok_or("bilateral source Port absent")?;
        if marker != BILATERAL_SOURCE_VERSION
            || source.platform != Platform::Linux
            || source.id != link.peer_node_id()
            || port.direction != PortDirection::Source
            || port.profile != ProfileId::audio_frames_v1()
            || port.formats != [FormatDescriptor::new(SECURE_AUDIO_STEREO_FORMAT)]
            || port.qos_modes != std::collections::BTreeSet::from([QosMode::Interactive])
            || port.interoperability_mode != InteroperabilityMode::AdapterManaged
            || status.route.state != RouteState::Draft
            || status.route.authorization != AuthorizationState::Pending
            || status.route.epoch != 0
            || status.route.session_id != link.session_id()
            || status.touchpad_data.is_some()
            || status.haptics_data.is_some()
            || status.native_audio_data.is_some()
            || status.camera_data.is_some()
            || status.secure_camera_data.is_some()
            || status.secure_audio_data.is_some()
            || snapshot
                .routes
                .iter()
                .any(|r| r.session_id == link.session_id())
        {
            return Err("invalid marked bilateral stereo Pending proposal".into());
        }
        crate::secure_speaker_policy::selection(&self.runtime, &status.route)?;
        // No grant or lease is minted by importing this exact Pending proposal.
        self.bind_stereo_consent_control(link, generation)?;
        let imported = link
            .apply_route_status(&mut self.runtime, status, false)
            .map_err(|e| e.to_string())?;
        let result = (|| {
            if imported != status.route {
                return Err("bilateral Pending import changed exact Route".into());
            }
            self.bilateral_stereo.register(id, link.session_id())?;
            self.stereo_dispatch_routes.insert(id);
            self.stage_stereo_consent(link, id)?;
            Ok(Some(imported))
        })();
        if result.is_err() {
            self.bilateral_stereo.invalidate(id);
            self.runtime
                .retire_route_authorization(id)
                .map_err(|e| e.to_string())?;
        }
        result
    }
}
