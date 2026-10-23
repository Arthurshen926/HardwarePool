//! Bilateral consent foundation. The explicit marked Linux production dispatcher
//! and portable supervisor use this gate; unmarked legacy routes do not.
//! A catalog marker never substitutes for either peer's fresh local approval.
#![allow(dead_code)]
use crate::{
    AndroidCatalogRuntimeV1, stereo_admission::EndpointAuthority,
    stereo_negotiation::StereoConsentStamp,
};
use capyio_control_link::{CatalogControlLink, StereoConsentControlIdentity, StereoConsentReceipt};
use capyio_control_tls::{CertificateFingerprint, ControlTlsBinding};
use capyio_core::*;
use capyio_protocol::{SECURE_AUDIO_STEREO_FORMAT, ValidatedRouteStatus};
use capyio_runtime::NodeRuntime;
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const MAX_REQUESTS: usize = 16;
const CONSENT_MS: u64 = 60_000;

#[derive(Clone, Copy)]
struct ClockSample {
    wall: u64,
    instant: Instant,
}
impl ClockSample {
    fn now() -> Result<Self, String> {
        let instant = Instant::now();
        Ok(Self {
            wall: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_millis()
                .try_into()
                .map_err(|_| "stereo clock overflow")?,
            instant,
        })
    }
}

#[derive(Clone, Debug)]
struct ControlAuthority {
    generation: u64,
    binding: ControlTlsBinding,
    certificate: CertificateFingerprint,
    connection: StereoConsentControlIdentity,
    live: Arc<AtomicBool>,
}
impl ControlAuthority {
    fn check(&self, link: &CatalogControlLink) -> Result<(), String> {
        link.validate_stereo_consent_control_identity(&self.connection)
            .map_err(|e| e.to_string())?;
        if !self.live.load(Ordering::Acquire)
            || link.tls_binding() != self.binding
            || link
                .authenticated_peer_certificate()
                .map_err(|e| e.to_string())?
                != self.certificate
        {
            return Err("stereo current control authority changed".into());
        }
        Ok(())
    }
}

#[derive(Debug)]
struct Pending {
    original: Route,
    session: Session,
    endpoints: [EndpointAuthority; 2],
    selection_revision: u64,
    control: ControlAuthority,
    request_wall_end: u64,
    request_end: Instant,
    last_wall: u64,
    last_instant: Instant,
    receipt: Option<Arc<StereoConsentReceipt>>,
}
impl Pending {
    fn check(
        &mut self,
        runtime: &NodeRuntime,
        current: &ControlAuthority,
        clock: ClockSample,
    ) -> Result<(), String> {
        if !self.control.connection.is_live()
            || !self.control.live.load(Ordering::Acquire)
            || !Arc::ptr_eq(&self.control.live, &current.live)
            || current.generation != self.control.generation
            || clock.wall < self.last_wall
            || clock.instant < self.last_instant
            || clock.wall >= self.request_wall_end
            || clock.instant >= self.request_end
            || runtime.route(self.original.id).map_err(|e| e.to_string())? != &self.original
            || runtime
                .session(self.session.id)
                .map_err(|e| e.to_string())?
                != &self.session
            || self.session.state != SessionState::Ready
            || crate::secure_speaker_policy::selection(runtime, &self.original)?
                != self.selection_revision
        {
            return Err("stereo pending authority changed or expired".into());
        }
        for endpoint in &self.endpoints {
            endpoint
                .check(runtime, &self.original, false)
                .map_err(|e| e.to_string())?;
        }
        self.last_wall = clock.wall;
        self.last_instant = clock.instant;
        if let Some(receipt) = &self.receipt
            && (!receipt.is_live()
                || clock.wall < receipt.received_wall_ms()
                || clock.instant < receipt.received_instant()
                || clock.instant >= receipt.monotonic_deadline())
        {
            return Err("stereo original peer receipt changed or expired".into());
        }
        Ok(())
    }
}

#[derive(Debug)]
enum Entry {
    Pending(Box<Pending>),
    Consumed {
        receipt: Arc<StereoConsentReceipt>,
        prepared: Box<Route>,
    },
    Retired,
}
#[derive(Debug, Default)]
struct State {
    control: Option<ControlAuthority>,
    entries: BTreeMap<RouteId, Entry>,
}
/// Host clones share ALL authority transitions and tombstones. They never clone
/// a fresh receipt or obtain a second independently consumable approval.
#[derive(Clone, Debug, Default)]
pub(crate) struct StereoConsentGate(Arc<Mutex<State>>);

fn exact_pending(runtime: &NodeRuntime, id: RouteId) -> Result<Route, String> {
    let route = runtime.route(id).map_err(|e| e.to_string())?.clone();
    let exact = FormatDescriptor::new(SECURE_AUDIO_STEREO_FORMAT);
    let snapshot = runtime.snapshot();
    let source = snapshot
        .peers
        .iter()
        .find(|node| node.id == route.source.node_id)
        .ok_or("stereo Source absent")?;
    if !StereoConsentStamp::is_exact_stereo_sink(runtime, id)
        || source.platform != Platform::Linux
        || source
            .capabilities
            .get(&route.source.capability_id)
            .is_none_or(|cap| cap.class != CapabilityClass::Custom("system_audio_capture".into()))
        || route.state != RouteState::Draft
        || route.authorization != AuthorizationState::Pending
        || route.epoch != 0
        || route.selected_format.is_some()
        || route.selected_qos_mode.is_some()
        || route.compatible_formats != [exact]
        || route.compatible_qos_modes != std::collections::BTreeSet::from([QosMode::Interactive])
    {
        return Err("stereo requires the exact untouched Pending Draft".into());
    }
    crate::secure_speaker_policy::selection(runtime, &route)?;
    Ok(route)
}

fn peer_matches(original: &Route, status: &ValidatedRouteStatus, now: u64) -> Result<u64, String> {
    let AuthorizationState::Authorized {
        expires_at_ms: Some(end),
    } = status.route.authorization
    else {
        return Err("stereo finite peer consent absent".into());
    };
    let mut expected = original.clone();
    expected.authorization = status.route.authorization;
    if status.session_id != original.session_id
        || status.route != expected
        || end <= now
        || status.touchpad_data.is_some()
        || status.haptics_data.is_some()
        || status.native_audio_data.is_some()
        || status.camera_data.is_some()
        || status.secure_camera_data.is_some()
        || status.secure_audio_data.is_some()
    {
        return Err("stereo peer consent is not the exact no-offer Draft".into());
    }
    Ok(end)
}

fn retire(host: &mut AndroidCatalogRuntimeV1, route: RouteId) -> Result<(), String> {
    host.bilateral_stereo.invalidate(route);
    host.stereo_consents.remove(&route);
    host.audio_status_pending.insert(route);
    let current = host
        .runtime
        .route(route)
        .map_err(|e| e.to_string())?
        .clone();
    if current.state == RouteState::Draft
        && matches!(
            current.authorization,
            AuthorizationState::Pending | AuthorizationState::Denied
        )
    {
        return host
            .runtime
            .deny_pending_route(route)
            .map_err(|e| e.to_string());
    }
    // Terminal per-route revocation is required: both Offline and Stopped can
    // otherwise reuse the old Authorized grant through ordinary Runtime APIs.
    host.runtime
        .retire_route_authorization(route)
        .map_err(|e| e.to_string())
}

impl AndroidCatalogRuntimeV1 {
    /// Dormant admission hook for the authoritative control supervisor. A
    /// replacement invalidates the old lineage before any new context exists.
    pub(crate) fn bind_stereo_consent_control(
        &mut self,
        link: &CatalogControlLink,
        generation: u64,
    ) -> Result<(), String> {
        let gate = self.stereo_consent_gate.clone();
        let mut state = gate.0.lock().map_err(|_| "stereo consent state poisoned")?;
        if let Some(old) = &state.control {
            old.live.store(false, Ordering::Release);
            for (route, entry) in &mut state.entries {
                *entry = Entry::Retired;
                retire(self, *route)?;
            }
            return Err("stereo original control cannot be rebound".into());
        }
        if generation == 0 {
            return Err("stereo requires a new positive control generation".into());
        }
        let session = self
            .runtime
            .session(link.session_id())
            .map_err(|e| e.to_string())?;
        if session.state != SessionState::Ready || session.remote_node_id != link.peer_node_id() {
            return Err("stereo current Session absent".into());
        }
        let certificate = link
            .authenticated_peer_certificate()
            .map_err(|e| e.to_string())?;
        state.control = Some(ControlAuthority {
            generation,
            binding: link.tls_binding(),
            certificate,
            connection: link
                .stereo_consent_control_identity()
                .map_err(|e| e.to_string())?,
            live: Arc::new(AtomicBool::new(true)),
        });
        Ok(())
    }

    pub(crate) fn invalidate_stereo_consent_control(&mut self) -> Result<(), String> {
        let gate = self.stereo_consent_gate.clone();
        let mut state = gate.0.lock().map_err(|_| "stereo consent state poisoned")?;
        if let Some(control) = &state.control {
            control.live.store(false, Ordering::Release);
        }
        for (route, entry) in &mut state.entries {
            *entry = Entry::Retired;
            retire(self, *route)?;
        }
        Ok(())
    }

    pub(crate) fn stage_stereo_consent(
        &mut self,
        link: &CatalogControlLink,
        route: RouteId,
    ) -> Result<(), String> {
        self.stage_stereo_consent_at(link, route, ClockSample::now()?)
    }
    fn stage_stereo_consent_at(
        &mut self,
        link: &CatalogControlLink,
        route: RouteId,
        clock: ClockSample,
    ) -> Result<(), String> {
        let gate = self.stereo_consent_gate.clone();
        let mut state = gate.0.lock().map_err(|_| "stereo consent state poisoned")?;
        let control = state.control.clone().ok_or("stereo control absent")?;
        control.check(link)?;
        if state.entries.contains_key(&route) {
            return Err("stereo request already staged or retired".into());
        }
        if state.entries.len() >= MAX_REQUESTS {
            return Err("stereo consent request bound reached".into());
        }
        let original = exact_pending(&self.runtime, route)?;
        if original.session_id != link.session_id() {
            return Err("stereo request belongs to another Session".into());
        }
        let session = self
            .runtime
            .session(original.session_id)
            .map_err(|e| e.to_string())?
            .clone();
        let endpoints = [
            EndpointAuthority::capture(&self.runtime, original.source)
                .map_err(|e| e.to_string())?,
            EndpointAuthority::capture(&self.runtime, original.sink).map_err(|e| e.to_string())?,
        ];
        let selection_revision = crate::secure_speaker_policy::selection(&self.runtime, &original)?;
        self.touchpad_consent
            .enqueue(&mut self.runtime, route, clock.wall)?;
        let wall_end = self
            .touchpad_consent
            .pending_deadline(route)
            .ok_or("stereo pending request absent")?;
        let remaining = wall_end
            .checked_sub(clock.wall)
            .filter(|n| *n > 0 && *n <= CONSENT_MS)
            .ok_or("stereo pending request clock changed")?;
        state.entries.insert(
            route,
            Entry::Pending(Box::new(Pending {
                original,
                session,
                endpoints,
                selection_revision,
                control,
                request_wall_end: wall_end,
                request_end: clock
                    .instant
                    .checked_add(Duration::from_millis(remaining))
                    .ok_or("stereo deadline overflow")?,
                last_wall: clock.wall,
                last_instant: clock.instant,
                receipt: None,
            })),
        );
        Ok(())
    }

    pub(crate) fn receive_stereo_consent(
        &mut self,
        link: &mut CatalogControlLink,
        status: &ValidatedRouteStatus,
    ) -> Result<(), String> {
        self.receive_stereo_consent_at(link, status, ClockSample::now()?)
    }
    fn receive_stereo_consent_at(
        &mut self,
        link: &mut CatalogControlLink,
        status: &ValidatedRouteStatus,
        clock: ClockSample,
    ) -> Result<(), String> {
        let gate = self.stereo_consent_gate.clone();
        let mut state = gate.0.lock().map_err(|_| "stereo consent state poisoned")?;
        let current = state.control.clone().ok_or("stereo control absent")?;
        let id = status.route.id;
        let Some(entry) = state.entries.get_mut(&id) else {
            return Err("stereo request was not staged".into());
        };
        let result = (|| {
            current.check(link)?;
            if let Entry::Consumed { receipt, prepared } = entry {
                link.validate_stereo_consent_receipt(receipt)
                    .map_err(|e| e.to_string())?;
                let mut expected = receipt.status().clone();
                expected.message_id = status.message_id;
                if expected == *status
                    && self.runtime.route(id).map_err(|e| e.to_string())? == prepared.as_ref()
                {
                    return Ok(());
                }
                return Err("stereo completed consent status or Prepared authority changed".into());
            }
            let Entry::Pending(pending) = entry else {
                return Err("stereo request already retired".into());
            };
            pending.check(&self.runtime, &current, clock)?;
            if let Some(receipt) = &pending.receipt {
                link.validate_stereo_consent_receipt(receipt)
                    .map_err(|e| e.to_string())?;
                let mut expected = receipt.status().clone();
                expected.message_id = status.message_id;
                if expected != *status {
                    return Err("stereo peer receipt conflicts with original".into());
                }
                return Ok(());
            }
            let receipt = link
                .take_received_stereo_consent(status)
                .map_err(|e| e.to_string())?;
            link.validate_stereo_consent_receipt(&receipt)
                .map_err(|e| e.to_string())?;
            peer_matches(&pending.original, receipt.status(), clock.wall)?;
            if receipt.binding() != current.binding
                || receipt.peer_certificate() != current.certificate
                || receipt.received_wall_ms() > clock.wall
                || receipt.received_instant() > clock.instant
                || receipt.monotonic_deadline() <= clock.instant
            {
                return Err("stereo peer receipt identity or original deadline changed".into());
            }
            pending.receipt = Some(Arc::new(receipt));
            Ok(())
        })();
        if result.is_err() {
            *entry = Entry::Retired;
            retire(self, id)?;
        }
        result
    }

    /// A fresh call after receipt is mandatory. Early clicks leave no deferred
    /// approval; denial is available even before peer approval.
    pub(crate) fn resolve_stereo_consent(
        &mut self,
        route: RouteId,
        allow: bool,
    ) -> Result<(), String> {
        self.resolve_stereo_consent_at(route, allow, ClockSample::now()?)
    }
    fn resolve_stereo_consent_at(
        &mut self,
        route: RouteId,
        allow: bool,
        clock: ClockSample,
    ) -> Result<(), String> {
        self.resolve_stereo_consent_at_bound(route, allow, clock, CONSENT_MS)
    }
    pub(crate) fn resolve_bilateral_stereo_consent(
        &mut self,
        route: RouteId,
        allow: bool,
        lifetime: Duration,
    ) -> Result<(), String> {
        let millis: u64 = lifetime
            .as_millis()
            .try_into()
            .map_err(|_| "stereo lifetime overflow")?;
        if millis == 0 || millis > CONSENT_MS {
            return Err("invalid bilateral consent lifetime".into());
        }
        self.resolve_stereo_consent_at_bound(route, allow, ClockSample::now()?, millis)
    }
    fn resolve_stereo_consent_at_bound(
        &mut self,
        route: RouteId,
        allow: bool,
        clock: ClockSample,
        local_ms: u64,
    ) -> Result<(), String> {
        let gate = self.stereo_consent_gate.clone();
        let mut state = gate.0.lock().map_err(|_| "stereo consent state poisoned")?;
        let current = state.control.clone().ok_or("stereo control absent")?;
        let entry = state
            .entries
            .get_mut(&route)
            .ok_or("stereo request was not staged")?;
        // An early allow is not consumed and is never queued. No Runtime mutation.
        if allow && matches!(entry, Entry::Pending(p) if p.receipt.is_none()) {
            return Err(
                "stereo awaits authenticated peer consent; approve again afterwards".into(),
            );
        }
        if !matches!(entry, Entry::Pending(_)) {
            return Err("stereo request already retired".into());
        }
        // Retire the shared lineage before any Runtime mutation, including failed
        // preflight. Host clones and later callbacks cannot retry this attempt.
        let Entry::Pending(mut pending) = std::mem::replace(entry, Entry::Retired) else {
            return Err("stereo request already retired".into());
        };
        let result = (|| {
            pending.check(&self.runtime, &current, clock)?;
            if !allow {
                return self.touchpad_consent.resolve_bounded(
                    &mut self.runtime,
                    route,
                    false,
                    clock.wall,
                    None,
                );
            }
            let receipt = pending
                .receipt
                .as_ref()
                .ok_or("stereo peer consent absent")?
                .clone();
            let peer_end = peer_matches(&pending.original, receipt.status(), clock.wall)?;
            let local_end = clock
                .wall
                .checked_add(local_ms)
                .ok_or("stereo consent overflow")?;
            let end = clock
                .instant
                .checked_add(Duration::from_millis(local_ms))
                .ok_or("stereo consent overflow")?
                .min(receipt.monotonic_deadline());
            self.touchpad_consent.resolve_bounded(
                &mut self.runtime,
                route,
                true,
                clock.wall,
                Some(local_end.min(peer_end)),
            )?;
            #[cfg(test)]
            tests::fail_at(1)?;
            let stamp = StereoConsentStamp::capture_bounded(
                &self.runtime,
                route,
                clock.wall,
                clock.instant,
                crate::stereo_negotiation::BilateralConsentBound {
                    end,
                    receipt: receipt.clone(),
                    live_control: current.live.clone(),
                    control_generation: current.generation,
                },
            )?;
            #[cfg(test)]
            tests::fail_at(2)?;
            self.runtime
                .prepare_route(
                    route,
                    Some(FormatDescriptor::new(SECURE_AUDIO_STEREO_FORMAT)),
                    QosMode::Interactive,
                    clock.wall,
                )
                .map_err(|e| e.to_string())?;
            #[cfg(test)]
            tests::fail_at(3)?;
            self.stereo_consents.insert(route, stamp);
            self.stereo_generations.entry(route).or_default();
            self.audio_status_pending.insert(route);
            *entry = Entry::Consumed {
                receipt,
                prepared: Box::new(
                    self.runtime
                        .route(route)
                        .map_err(|e| e.to_string())?
                        .clone(),
                ),
            };
            Ok(())
        })();
        if result.is_err() {
            retire(self, route)?;
        }
        result
    }
}

#[cfg(test)]
#[path = "stereo_consent_tests.rs"]
mod tests;
