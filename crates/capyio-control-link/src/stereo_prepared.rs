//! Dormant reverse Prepared evidence for the bilateral stereo Source supervisor.
//! This cache never grants Runtime authority or changes generic status admission.
//! Lifecycle traffic after handoff may retire this prelaunch receipt; it is still
//! delivered normally to the one-shot Source owner and never mints a new receipt.
use super::*;
use capyio_control_tls::CertificateFingerprint;
use capyio_core::{FormatDescriptor, ProfileId, QosMode, RouteBackend};
use capyio_protocol::SECURE_AUDIO_STEREO_FORMAT;
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const MAX_RECEIPTS: usize = 16;

/// Opaque, non-Clone evidence minted only by authenticated control decoding.
/// This proves receipt, not local consent or current catalog/Route admission.
///
/// ```compile_fail
/// use capyio_control_link::StereoPreparedReceipt;
/// fn duplicate(receipt: StereoPreparedReceipt) { let _ = receipt.clone(); }
/// ```
#[derive(Debug)]
pub struct StereoPreparedReceipt {
    instance_id: MessageId,
    binding: ControlTlsBinding,
    certificate: CertificateFingerprint,
    status: ValidatedRouteStatus,
    wall: u64,
    instant: Instant,
    end: Instant,
    live: Arc<AtomicBool>,
}
impl StereoPreparedReceipt {
    pub fn status(&self) -> &ValidatedRouteStatus {
        &self.status
    }
    pub fn binding(&self) -> ControlTlsBinding {
        self.binding
    }
    pub fn peer_certificate(&self) -> CertificateFingerprint {
        self.certificate
    }
    pub fn received_wall_ms(&self) -> u64 {
        self.wall
    }
    pub fn received_instant(&self) -> Instant {
        self.instant
    }
    pub fn monotonic_deadline(&self) -> Instant {
        self.end
    }
    /// Revoked synchronously by conflict, receipt overflow, close or link drop.
    /// The consumer must additionally validate its current Runtime/generation.
    pub fn is_live(&self) -> bool {
        self.live.load(Ordering::Acquire)
    }
}

struct Entry {
    original: ValidatedRouteStatus,
    receipt: Option<StereoPreparedReceipt>,
    live: Arc<AtomicBool>,
}
#[derive(Default)]
pub(super) struct ReceiptCache {
    entries: BTreeMap<RouteId, Entry>,
    exhausted: bool,
}
impl ReceiptCache {
    pub(super) fn invalidate(&mut self) {
        for entry in self.entries.values_mut() {
            entry.live.store(false, Ordering::Release);
            entry.receipt = None;
        }
        self.exhausted = true;
    }
    fn observe(
        &mut self,
        status: &ValidatedRouteStatus,
        instance_id: MessageId,
        binding: ControlTlsBinding,
        certificate: CertificateFingerprint,
        local: NodeId,
        peer: NodeId,
    ) {
        if self.exhausted {
            return;
        }
        if let Some(entry) = self.entries.get_mut(&status.route.id) {
            if !same_status(&entry.original, status) {
                entry.live.store(false, Ordering::Release);
                entry.receipt = None;
            }
            // Exact duplicates do not refresh either clocks or consumption.
            return;
        }
        if !candidate(status, binding, local, peer) {
            return;
        }
        if self.entries.len() == MAX_RECEIPTS {
            self.invalidate();
            return;
        }
        let live = Arc::new(AtomicBool::new(false));
        let receipt = sample_clock().and_then(|(wall, instant)| {
            let AuthorizationState::Authorized {
                expires_at_ms: Some(expiry),
            } = status.route.authorization
            else {
                return None;
            };
            let remaining = expiry
                .checked_sub(wall)
                .filter(|remaining| *remaining > 0)?;
            let end = instant.checked_add(Duration::from_millis(remaining))?;
            live.store(true, Ordering::Release);
            Some(StereoPreparedReceipt {
                instance_id,
                binding,
                certificate,
                status: status.clone(),
                wall,
                instant,
                end,
                live: Arc::clone(&live),
            })
        });
        // Expired/clock-failed first observations are tombstones, never rearmed.
        self.entries.insert(
            status.route.id,
            Entry {
                original: status.clone(),
                receipt,
                live,
            },
        );
    }
}
fn sample_clock() -> Option<(u64, Instant)> {
    // Capture monotonic first: scheduler delay between samples only shortens
    // inferred remaining authority, never adds it to the peer deadline.
    let instant = Instant::now();
    let wall = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_millis()
        .try_into()
        .ok()?;
    Some((wall, instant))
}
fn same_status(a: &ValidatedRouteStatus, b: &ValidatedRouteStatus) -> bool {
    let mut expected = a.clone();
    expected.message_id = b.message_id;
    expected == *b
}
fn candidate(
    status: &ValidatedRouteStatus,
    binding: ControlTlsBinding,
    local: NodeId,
    peer: NodeId,
) -> bool {
    let route = &status.route;
    status.session_id == binding.session_id
        && route.session_id == binding.session_id
        && route.source.node_id == local
        && route.sink.node_id == peer
        && route.profile == ProfileId::audio_frames_v1()
        && route.backend == RouteBackend::AdapterManaged
        && route.state == RouteState::Prepared
        && route.epoch == 0
        && matches!(
            route.authorization,
            AuthorizationState::Authorized {
                expires_at_ms: Some(_)
            }
        )
        && route.selected_format == Some(FormatDescriptor::new(SECURE_AUDIO_STEREO_FORMAT))
        && route.selected_qos_mode == Some(QosMode::Interactive)
        && route.compatible_formats == [FormatDescriptor::new(SECURE_AUDIO_STEREO_FORMAT)]
        && route.compatible_qos_modes == [QosMode::Interactive].into_iter().collect()
        && status.touchpad_data.is_none()
        && status.haptics_data.is_none()
        && status.native_audio_data.is_none()
        && status.camera_data.is_none()
        && status.secure_camera_data.is_none()
        && status.secure_audio_data.is_none()
}
impl CatalogControlLink {
    pub(super) fn observe_stereo_prepared(&mut self, status: &ValidatedRouteStatus) {
        let Ok(certificate) = self.authenticated_peer_certificate() else {
            self.stereo_prepared_receipts.invalidate();
            return;
        };
        self.stereo_prepared_receipts.observe(
            status,
            self.instance_id,
            self.binding,
            certificate,
            self.local_node_id,
            self.peer_node_id,
        );
    }
    /// Consume authenticated transport evidence, never a caller-created grant.
    /// A mismatching consume attempt retires the stored receipt. No retry/rearm.
    pub fn take_received_stereo_prepared(
        &mut self,
        status: &ValidatedRouteStatus,
    ) -> Result<StereoPreparedReceipt, CatalogLinkError> {
        if self.channel.is_none() {
            return Err(CatalogLinkError::Closed);
        }
        let entry = self
            .stereo_prepared_receipts
            .entries
            .get_mut(&status.route.id)
            .ok_or(CatalogLinkError::InvalidPeerRoute)?;
        if !same_status(&entry.original, status) {
            entry.live.store(false, Ordering::Release);
            entry.receipt = None;
            return Err(CatalogLinkError::InvalidPeerRoute);
        }
        let receipt = entry
            .receipt
            .take()
            .ok_or(CatalogLinkError::InvalidPeerRoute)?;
        self.validate_stereo_prepared_receipt(&receipt)?;
        Ok(receipt)
    }
    /// Checks the exact private connection instance, not merely equal TLS IDs.
    pub fn validate_stereo_prepared_receipt(
        &self,
        receipt: &StereoPreparedReceipt,
    ) -> Result<(), CatalogLinkError> {
        if self.channel.is_none() {
            return Err(CatalogLinkError::Closed);
        }
        if receipt.instance_id != self.instance_id
            || receipt.binding != self.binding
            || !receipt.is_live()
            || self.authenticated_peer_certificate()? != receipt.certificate
            || !self
                .stereo_prepared_receipts
                .entries
                .get(&receipt.status.route.id)
                .is_some_and(|entry| Arc::ptr_eq(&entry.live, &receipt.live))
        {
            return Err(CatalogLinkError::InvalidPeerRoute);
        }
        Ok(())
    }
}
