//! Exact selected secure speaker JNI owner; AudioTrack remains a separate platform owner.
//! Runtime/control preparation and fresh commits are separate from network I/O.
use super::*;
use crate::{
    secure_stereo_receiver::{MAX_STEREO_OPERATION, SecureStereoSinkAdmission},
    stereo_admission::EndpointAuthority,
    stereo_job::{FAILED, PENDING, STOPPED, StereoAdmissionGate, StereoReceiveJob},
    stereo_negotiation::{StereoNegotiationAction, StereoSinkNegotiation, StereoSinkSnapshot},
};
use capyio_control_tls::{ControlTlsBinding, ControlTlsServerIdentity};
use capyio_core::{Route, RouteId, RouteState, Session, StreamId};
use jni::objects::JLongArray;
use std::sync::atomic::{AtomicBool, AtomicI32};
use std::time::Instant;

static NEXT_CONTROL: AtomicU64 = AtomicU64::new(1);
static NEXT_JOB: AtomicU64 = AtomicU64::new(1);
static JOB: OnceLock<Mutex<Option<Arc<StereoEntry>>>> = OnceLock::new();
static TERMINAL: OnceLock<Mutex<Option<Terminal>>> = OnceLock::new();
#[derive(Clone, Copy)]
struct Terminal {
    runtime: u64,
    generation: u64,
    state: i32,
    counters: [i64; 3],
}
fn jobs() -> &'static Mutex<Option<Arc<StereoEntry>>> {
    JOB.get_or_init(|| Mutex::new(None))
}
// Poison is never cleared. Recover only enough ownership to revoke admission;
// the registry guard is gone before any independent shutdown path can run.
fn entry_snapshot() -> Option<Arc<StereoEntry>> {
    let (entry, poisoned) = match jobs().lock() {
        Ok(registry) => (registry.clone(), false),
        Err(error) => (error.into_inner().clone(), true),
    };
    if poisoned && let Some(entry) = &entry {
        entry.reject();
    }
    entry
}
fn terminal() -> &'static Mutex<Option<Terminal>> {
    TERMINAL.get_or_init(|| Mutex::new(None))
}
fn allocate(counter: &AtomicU64) -> Result<u64, String> {
    counter
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
            n.checked_add(1).filter(|v| *v <= i64::MAX as u64)
        })
        .map_err(|_| "stereo generation space exhausted".into())
}
pub(super) fn next_control_generation() -> Result<u64, String> {
    allocate(&NEXT_CONTROL)
}
fn now_ms() -> Result<u64, String> {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "stereo wall clock unavailable")?
            .as_millis(),
    )
    .map_err(|_| "stereo clock overflow".into())
}

/// Derived only from the material used by this exact admitted channel. The live
/// token is invalidated before disconnect/replacement becomes observable.
pub(super) struct ControlMediaIdentity {
    generation: u64,
    binding: ControlTlsBinding,
    identity: ControlTlsServerIdentity,
    live: Arc<AtomicBool>,
}
impl ControlMediaIdentity {
    pub(super) fn new(
        generation: u64,
        link: &CatalogControlLink,
        identity: ControlTlsServerIdentity,
    ) -> Result<Self, String> {
        if generation == 0
            || identity.expected_peer_certificate
                != link
                    .authenticated_peer_certificate()
                    .map_err(|e| e.to_string())?
        {
            return Err("stereo identity does not match current authenticated control peer".into());
        }
        Ok(Self {
            generation,
            binding: link.tls_binding(),
            identity,
            live: Arc::new(AtomicBool::new(true)),
        })
    }
    fn validate(&self, link: &CatalogControlLink, generation: u64) -> Result<(), String> {
        if !self.live.load(Ordering::Acquire)
            || self.generation != generation
            || self.binding != link.tls_binding()
            || link.is_closed()
            || self.identity.expected_peer_certificate
                != link
                    .authenticated_peer_certificate()
                    .map_err(|e| e.to_string())?
        {
            return Err("stereo current control identity changed".into());
        }
        Ok(())
    }
    pub(super) fn invalidate(&self) {
        self.live.store(false, Ordering::Release);
    }
}
struct StereoEntry {
    runtime: u64,
    generation: u64,
    control_generation: u64,
    control_live: Arc<AtomicBool>,
    // Retained independently of Runtime lookup, including registry failure.
    bilateral: bool,
    cleanup_failed: AtomicBool,
    route: RouteId,
    expected: Mutex<Route>,
    session: Session,
    endpoints: [EndpointAuthority; 2],
    gate: StereoAdmissionGate,
    state: AtomicI32,
    pending_finished: AtomicBool,
    pending_end: Instant,
    job: OnceLock<Arc<StereoReceiveJob>>,
}
impl StereoEntry {
    fn reject(&self) {
        self.gate.invalidate();
        let _ = self
            .state
            .compare_exchange(PENDING, FAILED, Ordering::AcqRel, Ordering::Acquire);
        if let Some(job) = self.job.get() {
            job.reject_callback();
        } else {
            self.pending_finished.store(true, Ordering::Release);
        }
    }
    fn live(&self) -> bool {
        if !self.control_live.load(Ordering::Acquire)
            || (self.job.get().is_none() && Instant::now() >= self.pending_end)
        {
            self.reject();
        }
        self.gate.is_open()
    }
    fn state(&self) -> i32 {
        if self.cleanup_failed.load(Ordering::Acquire) {
            return FAILED;
        }
        self.live();
        self.job
            .get()
            .map_or_else(|| self.state.load(Ordering::Acquire), |job| job.state())
    }
    fn counters(&self) -> [i64; 3] {
        self.job.get().map_or([0; 3], |job| {
            let m = job.metrics();
            [
                m.records_admitted as i64,
                m.records_accepted as i64,
                m.pcm_bytes_accepted as i64,
            ]
        })
    }
    fn stop(&self) {
        self.state.store(STOPPED, Ordering::Release);
        self.gate.invalidate();
        if let Some(job) = self.job.get() {
            job.stop();
        } else {
            self.pending_finished.store(true, Ordering::Release);
        }
    }
}
pub(super) struct PendingStereo {
    entry: Arc<StereoEntry>,
    negotiation: Option<StereoSinkNegotiation>,
}
impl PendingStereo {
    pub(super) fn cancel(&mut self) {
        self.entry.reject();
        if let Some(negotiation) = &mut self.negotiation {
            negotiation.terminate();
        }
        self.entry.pending_finished.store(true, Ordering::Release);
    }
}

/// Shared authoritative mutation boundary: atomics only after the brief lookup.
/// Never perform socket shutdown or join while the Runtime guard is retained.
pub(super) fn reconcile(runtime: &capyio_runtime::NodeRuntime) {
    let entry = entry_snapshot();
    let Some(entry) = entry else {
        return;
    };
    if runtime.snapshot().local_node.id != entry.session.local_node_id
        || runtime.session(entry.session.id).is_err()
    {
        return;
    }
    // Guard Drop additionally selects the exact handle, including a removed
    // Session. This pre-publication helper must not invalidate another Runtime
    // merely because both were initialized with the same local Node identity.
    reconcile_entry(&entry, runtime);
}
/// Called while the sole authoritative registry guard is already held. There
/// is no Runtime registry reacquisition. JOB is held only for an Arc clone;
/// all paths that obtain Runtime elsewhere release JOB before doing so.
pub(super) fn reconcile_registry(
    runtimes: &BTreeMap<u64, AndroidCatalogRuntimeV1>,
    poisoned: bool,
) {
    let entry = entry_snapshot();
    let Some(entry) = entry else {
        return;
    };
    let Some(host) = runtimes.get(&entry.runtime).filter(|_| !poisoned) else {
        entry.reject();
        return;
    };
    reconcile_entry(&entry, &host.runtime);
}
fn reconcile_entry(entry: &StereoEntry, runtime: &capyio_runtime::NodeRuntime) {
    if !entry.live() {
        return;
    }
    let valid = now_ms().ok().is_some_and(|now| {
        if let Some(job) = entry.job.get() {
            job.reconcile(runtime, entry.control_generation, now)
                .is_ok()
        } else {
            entry.expected.lock().ok().is_some_and(|expected| {
                runtime.route(entry.route).is_ok_and(|r| r == &*expected)
                    && runtime
                        .session(entry.session.id)
                        .is_ok_and(|s| s == &entry.session)
                    && entry
                        .endpoints
                        .iter()
                        .all(|endpoint| endpoint.check(runtime, &expected, true).is_ok())
                    && crate::stereo_admission::expiry(&expected).is_ok_and(|end| now < end)
            })
        }
    });
    if !valid {
        entry.reject();
    }
}
pub(super) fn stop_runtime(runtime: u64, route: Option<RouteId>) {
    let entry =
        entry_snapshot().filter(|e| e.runtime == runtime && route.is_none_or(|id| id == e.route));
    if let Some(entry) = entry {
        entry.stop();
    }
}
/// Called before obtaining Runtime/control guards. Only an actually exited
/// worker is joined; the single terminal result and consumed host Route survive.
pub(super) fn reap_finished(runtime: Option<u64>) {
    let entry = {
        let Ok(mut registry) = jobs().lock() else {
            return;
        };
        if registry.as_ref().is_some_and(|e| {
            runtime.is_none_or(|r| r == e.runtime)
                && e.job.get().map_or_else(
                    || e.pending_finished.load(Ordering::Acquire),
                    |j| j.worker_exited(),
                )
        }) {
            registry.take()
        } else {
            None
        }
    };
    let Some(entry) = entry else {
        return;
    };
    if let Some(job) = entry.job.get() {
        job.reap_finished();
    }
    let mut cleanup_failed = false;
    match catalog_runtimes().lock() {
        Ok(mut registry) => {
            if let Some(host) = registry.get_mut(&entry.runtime) {
                if entry.bilateral {
                    host.bilateral_stereo.invalidate(entry.route);
                    cleanup_failed = host
                        .runtime
                        .retire_route_authorization(entry.route)
                        .is_err();
                    host.stereo_consents.remove(&entry.route);
                    host.audio_status_pending.insert(entry.route);
                } else if host
                    .runtime
                    .route(entry.route)
                    .is_ok_and(|r| matches!(r.state, RouteState::Prepared | RouteState::Starting))
                {
                    if entry.state() == FAILED {
                        let _ = crate::audio_negotiation::fail(host, entry.route);
                    } else {
                        let _ = host.runtime.begin_route_stop(entry.route);
                        let _ = host.runtime.stop_route(entry.route);
                    }
                }
                reconcile(&host.runtime);
            } else {
                cleanup_failed = entry.bilateral;
            }
        }
        Err(_) => cleanup_failed = entry.bilateral,
    }
    if cleanup_failed {
        // Failure to establish Runtime retirement is not successful Stop. This
        // latch overrides a completed job's STOPPED and survives later stop().
        entry.cleanup_failed.store(true, Ordering::Release);
        entry.gate.invalidate();
        entry.reject();
    }
    // Publish only after cleanup; do not preserve an earlier worker STOPPED
    // when its authoritative retirement failed or could not be observed.
    if let Ok(mut last) = terminal().lock() {
        *last = Some(Terminal {
            runtime: entry.runtime,
            generation: entry.generation,
            state: entry.state(),
            counters: entry.counters(),
        });
    }
}
fn lookup(runtime: jlong, generation: jlong) -> Result<Arc<StereoEntry>, &'static str> {
    if runtime <= 0 || generation <= 0 {
        return Err("invalid");
    }
    let registry = match jobs().try_lock() {
        Ok(registry) => registry,
        Err(std::sync::TryLockError::WouldBlock) => return Err("busy"),
        Err(std::sync::TryLockError::Poisoned(error)) => {
            if let Some(entry) = error.get_ref().as_ref() {
                entry.reject();
            }
            return Err("terminal");
        }
    };
    registry
        .as_ref()
        .filter(|e| e.runtime == runtime as u64 && e.generation == generation as u64)
        .cloned()
        .ok_or("stale")
}
pub(super) fn owns_live_route(runtime: u64, route: RouteId) -> bool {
    entry_snapshot()
        .is_some_and(|entry| entry.runtime == runtime && entry.route == route && entry.live())
}

pub(super) fn launch_ready(host: &AndroidCatalogRuntimeV1, route: &Route, now: u64) -> bool {
    route.state == RouteState::Prepared
        && crate::secure_speaker_policy::selection(&host.runtime, route).is_ok()
        && host
            .stereo_consents
            .get(&route.id)
            .is_some_and(|stamp| std::time::Instant::now() < stamp.monotonic_deadline())
        && crate::stereo_admission::expiry(route).is_ok_and(|end| now < end)
}

fn start(runtime: u64, route: RouteId) -> Result<u64, String> {
    let result = start_inner(runtime, route);
    if result.is_err() {
        let mut registry = catalog_runtimes()
            .lock()
            .map_err(|_| "Runtime registry poisoned")?;
        if let Some(host) = registry.get_mut(&runtime)
            && host.stereo_dispatch_routes.contains(&route)
        {
            host.bilateral_stereo.invalidate(route);
            host.runtime
                .retire_route_authorization(route)
                .map_err(|e| e.to_string())?;
            host.stereo_consents.remove(&route);
            host.audio_status_pending.insert(route);
        }
    }
    result
}
fn start_inner(runtime: u64, route: RouteId) -> Result<u64, String> {
    reap_finished(None);
    let session = {
        let registry = catalog_runtimes()
            .lock()
            .map_err(|_| "Runtime registry poisoned")?;
        let host = registry.get(&runtime).ok_or("Runtime absent")?;
        let route = host.runtime.route(route).map_err(|e| e.to_string())?;
        if !launch_ready(host, route, now_ms()?) {
            return Err("stereo Start requires already Prepared local consent".into());
        }
        route.session_id
    };
    let candidates: Vec<_> = control_listeners()
        .lock()
        .map_err(|_| "control registry poisoned")?
        .values()
        .cloned()
        .collect();
    for candidate in candidates {
        let mut owner = candidate
            .lock()
            .map_err(|_| "control connection poisoned")?;
        if owner.closed
            || owner.runtime_handle != Some(runtime)
            || !owner
                .link
                .as_ref()
                .is_some_and(|link| link.session_id() == session)
        {
            continue;
        }
        if owner
            .stereo_pending
            .as_ref()
            .is_some_and(|p| matches!(p.entry.state(), FAILED | STOPPED))
        {
            if let Some(pending) = &mut owner.stereo_pending {
                pending.cancel();
            }
            owner.stereo_pending = None;
        }
        if owner.stereo_pending.is_some() {
            return Err("stereo generation already owns this control link".into());
        }
        let generation = owner.control_generation;
        let identity = owner
            .stereo_identity
            .as_ref()
            .ok_or("current stereo identity absent")?;
        let link = owner.link.as_ref().ok_or("current control link absent")?;
        identity.validate(link, generation)?;
        let local_address = link
            .authenticated_local_addr()
            .map_err(|e| e.to_string())?
            .ip();
        if !local_address.is_ipv4()
            || local_address.is_unspecified()
            || local_address.is_multicast()
        {
            return Err("stereo requires current native IPv4 channel address".into());
        }
        let now = now_ms()?;
        let (mut negotiation, request, entry) = {
            let mut registry = catalog_runtimes()
                .lock()
                .map_err(|_| "Runtime registry poisoned")?;
            let host = registry.get_mut(&runtime).ok_or("Runtime absent")?;
            let mut slot = jobs().lock().map_err(|_| "stereo registry poisoned")?;
            if slot.is_some() {
                return Err("previous stereo generation has not exited".into());
            }
            if !launch_ready(
                host,
                host.runtime.route(route).map_err(|e| e.to_string())?,
                now,
            ) {
                return Err("secure speaker selection or original consent changed".into());
            }
            let stamp = host
                .stereo_consents
                .remove(&route)
                .ok_or("original local stereo consent stamp absent")?;
            let admission = SecureStereoSinkAdmission {
                expected: link.tls_binding(),
                route_id: route,
                stream_id: StreamId::new(),
                local_address,
                lifetime: Duration::from_secs(60),
            };
            let (negotiation, request) = stamp.prepare(
                &host.runtime,
                link,
                admission,
                generation,
                now,
                Instant::now(),
            )?;
            host.stereo_generations
                .entry(route)
                .or_default()
                .reserve()
                .map_err(str::to_owned)?;
            let id = allocate(&NEXT_JOB)?;
            let original = negotiation.route().clone();
            let entry = Arc::new(StereoEntry {
                runtime,
                generation: id,
                control_generation: generation,
                control_live: identity.live.clone(),
                bilateral: host.stereo_dispatch_routes.contains(&route),
                cleanup_failed: AtomicBool::new(false),
                route,
                session: host
                    .runtime
                    .session(original.session_id)
                    .map_err(|e| e.to_string())?
                    .clone(),
                endpoints: [
                    EndpointAuthority::capture(&host.runtime, original.source)
                        .map_err(|e| e.to_string())?,
                    EndpointAuthority::capture(&host.runtime, original.sink)
                        .map_err(|e| e.to_string())?,
                ],
                expected: Mutex::new(original),
                gate: if host.stereo_dispatch_routes.contains(&route) {
                    host.bilateral_stereo.gate(route)?
                } else {
                    StereoAdmissionGate::new()
                },
                state: AtomicI32::new(PENDING),
                pending_finished: AtomicBool::new(false),
                pending_end: negotiation.operation_deadline(),
                job: OnceLock::new(),
            });
            if host.stereo_dispatch_routes.contains(&route) {
                host.audio_status_pending.remove(&route);
            }
            *slot = Some(entry.clone());
            (negotiation, request, entry)
        };
        let result = (|| {
            let link = owner.link.as_mut().ok_or("control link disappeared")?;
            // The receiver's immutable Prepared must precede Source Acquire.
            // Legacy Stage A keeps its original publication contract.
            let bilateral = catalog_runtimes()
                .lock()
                .map_err(|_| "Runtime registry poisoned")?
                .get(&runtime)
                .ok_or("Runtime absent")?
                .stereo_dispatch_routes
                .contains(&route);
            if bilateral {
                link.send_route_status_until(negotiation.route(), negotiation.operation_deadline())
                    .map_err(|e| e.to_string())?;
            }
            let sent = link
                .send_prepared_port_lease_request_until(request, negotiation.operation_deadline())
                .map_err(|e| e.to_string())?;
            let registry = catalog_runtimes()
                .lock()
                .map_err(|_| "Runtime registry poisoned")?;
            let host = registry.get(&runtime).ok_or("Runtime absent")?;
            negotiation.validate(&host.runtime, link, generation, now_ms()?, Instant::now())?;
            if !entry.live() {
                return Err("stereo Start cancelled".into());
            }
            negotiation.commit_source_request(sent)
        })();
        if let Err(error) = result {
            entry.reject();
            entry.pending_finished.store(true, Ordering::Release);
            return Err(error);
        }
        let id = entry.generation;
        owner.stereo_pending = Some(PendingStereo {
            entry,
            negotiation: Some(negotiation),
        });
        return Ok(id);
    }
    Err("no current authenticated control link for stereo Runtime".into())
}

pub(super) fn handles(pending: Option<&PendingStereo>, message: &ReceivedControlMessage) -> bool {
    let Some(pending) = pending else {
        return false;
    };
    let route = pending.entry.route;
    let session = pending.entry.session.id;
    match message {
        ReceivedControlMessage::PortLeaseResponse(response) => {
            pending.negotiation.as_ref().is_some_and(|n| {
                response.request_message_id == n.source_request()
                    && response.route_id == route
                    && response.session_id == session
            })
        }
        ReceivedControlMessage::PortLeaseRequest(request) => {
            request.request().route_id == route && request.request().session_id == session
        }
        ReceivedControlMessage::RouteCommand(command) => {
            command.route_id == route && command.session_id == session
        }
        ReceivedControlMessage::RouteStatus(status) => {
            status.route.id == route && status.session_id == session
        }
        _ => false,
    }
}
/// Validate under Runtime before each control poll; an idle poll does not renew
/// or reset the original 250 ms negotiation deadline.
pub(super) fn before_poll(
    pending: &mut PendingStereo,
    link: &CatalogControlLink,
    generation: u64,
) -> Result<Instant, String> {
    if !pending.entry.live() {
        return Err("stereo admission ended".into());
    }
    let registry = catalog_runtimes()
        .lock()
        .map_err(|_| "Runtime registry poisoned")?;
    let host = registry
        .get(&pending.entry.runtime)
        .ok_or("Runtime absent")?;
    if let Some(negotiation) = &mut pending.negotiation {
        negotiation.validate(&host.runtime, link, generation, now_ms()?, Instant::now())?;
        Ok(negotiation.operation_deadline())
    } else {
        let job = pending.entry.job.get().ok_or("stereo job absent")?;
        job.reconcile(&host.runtime, generation, now_ms()?)?;
        Ok(job
            .snapshot()
            .monotonic_deadline()
            .min(Instant::now() + MAX_STEREO_OPERATION))
    }
}
pub(super) fn receive(
    pending: &mut PendingStereo,
    link: &mut CatalogControlLink,
    identity: &ControlMediaIdentity,
    generation: u64,
    message: ReceivedControlMessage,
) -> Result<(), String> {
    identity.validate(link, generation)?;
    let entry = pending.entry.clone();
    if !entry.live() {
        return Err("stereo admission ended".into());
    }
    let Some(negotiation) = &mut pending.negotiation else {
        match message {
            ReceivedControlMessage::RouteCommand(command)
                if command.action == capyio_protocol::RouteCommandAction::Stop =>
            {
                entry.stop();
                let route = {
                    let mut registry = catalog_runtimes()
                        .lock()
                        .map_err(|_| "Runtime registry poisoned")?;
                    let host = registry.get_mut(&entry.runtime).ok_or("Runtime absent")?;
                    host.runtime
                        .begin_route_stop(entry.route)
                        .map_err(|e| e.to_string())?;
                    host.runtime
                        .stop_route(entry.route)
                        .map_err(|e| e.to_string())?;
                    reconcile(&host.runtime);
                    host.runtime
                        .route(entry.route)
                        .map_err(|e| e.to_string())?
                        .clone()
                };
                return link
                    .send_route_status_until(&route, Instant::now() + MAX_STEREO_OPERATION)
                    .map_err(|e| e.to_string());
            }
            ReceivedControlMessage::RouteStatus(status)
                if entry
                    .job
                    .get()
                    .is_some_and(|j| status.route == *j.snapshot().route())
                    && status.secure_audio_data.is_none()
                    && status.native_audio_data.is_none()
                    && status.camera_data.is_none()
                    && status.secure_camera_data.is_none()
                    && status.touchpad_data.is_none()
                    && status.haptics_data.is_none() =>
            {
                return Ok(());
            }
            _ => {
                entry.reject();
                return Err("unexpected stereo control mutation or renewal".into());
            }
        }
    };
    let action = {
        let mut registry = catalog_runtimes()
            .lock()
            .map_err(|_| "Runtime registry poisoned")?;
        let host = registry.get_mut(&entry.runtime).ok_or("Runtime absent")?;
        let result = negotiation.accept_control(
            &mut host.runtime,
            link,
            message,
            generation,
            now_ms()?,
            Instant::now(),
        );
        reconcile(&host.runtime);
        result?
    };
    match action {
        StereoNegotiationAction::SendLeaseResponse(response) => {
            link.send_prepared_port_lease_response_until(
                response,
                negotiation.operation_deadline(),
            )
            .map_err(|e| e.to_string())?;
            let registry = catalog_runtimes()
                .lock()
                .map_err(|_| "Runtime registry poisoned")?;
            let host = registry.get(&entry.runtime).ok_or("Runtime absent")?;
            negotiation.validate(&host.runtime, link, generation, now_ms()?, Instant::now())?;
            if !entry.live() {
                return Err("stereo response cancelled".into());
            }
            negotiation.commit_sink_response()?;
        }
        StereoNegotiationAction::PeerStop => {
            entry.stop();
            pending.cancel();
            return Err("peer stopped stereo negotiation".into());
        }
        StereoNegotiationAction::None => {}
    }
    if !negotiation.ready() {
        return Ok(());
    }
    let snapshot = {
        let mut registry = catalog_runtimes()
            .lock()
            .map_err(|_| "Runtime registry poisoned")?;
        let host = registry.get_mut(&entry.runtime).ok_or("Runtime absent")?;
        if !entry.live() {
            return Err("stereo Start cancelled".into());
        }
        let snapshot = negotiation.finish(
            &mut host.runtime,
            link,
            generation,
            now_ms()?,
            Instant::now(),
        )?;
        *entry
            .expected
            .lock()
            .map_err(|_| "stereo authority poisoned")? = snapshot.route().clone();
        reconcile(&host.runtime);
        snapshot
    };
    pending.negotiation = None;
    let authority_entry = Arc::downgrade(&entry);
    let authority = Arc::new(move |snapshot: &StereoSinkSnapshot| {
        let authority_entry = authority_entry
            .upgrade()
            .ok_or("stereo owner was released")?;
        let deadline = snapshot
            .monotonic_deadline()
            .min(Instant::now() + MAX_STEREO_OPERATION);
        loop {
            if !authority_entry.live() || Instant::now() >= deadline {
                return Err("stereo authority check cancelled or timed out".into());
            }
            match catalog_runtimes().try_lock() {
                Ok(registry) => {
                    let host = registry
                        .get(&authority_entry.runtime)
                        .ok_or("Runtime absent")?;
                    snapshot.validate(
                        &host.runtime,
                        authority_entry.control_generation,
                        now_ms()?,
                        Instant::now(),
                    )?;
                    return if authority_entry.live() {
                        Ok(())
                    } else {
                        Err("stereo control generation ended".into())
                    };
                }
                Err(std::sync::TryLockError::WouldBlock) => {
                    std::thread::sleep(Duration::from_millis(1))
                }
                Err(_) => return Err("Runtime registry poisoned".into()),
            }
        }
    });
    let failure_entry = Arc::downgrade(&entry);
    let failure = Arc::new(move || {
        if let Some(entry) = failure_entry.upgrade() {
            entry.reject();
        }
    });
    // The per-link protocol mutex may serialize this work; Runtime and global
    // registries are released. Stop needs neither this mutex nor control I/O.
    let job = StereoReceiveJob::start(
        entry.runtime,
        entry.generation,
        snapshot,
        identity.identity.clone(),
        generation,
        entry.gate.clone(),
        authority,
        failure,
    )?;
    entry
        .job
        .set(job.clone())
        .map_err(|_| "stereo job installed twice")?;
    entry.pending_finished.store(true, Ordering::Release);
    if !entry.live() {
        job.stop();
        return Err("stereo media setup cancelled".into());
    }
    link.send_secure_stereo_audio_data_offer_until(
        job.snapshot().route(),
        job.offer(),
        now_ms()?,
        job.operation_deadline(),
    )
    .map_err(|e| {
        entry.reject();
        e.to_string()
    })?;
    job.offer_committed()
}

/// Retire only media admission, not the reusable authenticated control Session.
/// Terminal mutation is reconciled before its outside-lock status publication.
pub(super) fn retire_terminal(owner: &mut NativeControlListener) -> Result<(), String> {
    if !owner
        .stereo_pending
        .as_ref()
        .is_some_and(|pending| matches!(pending.entry.state(), FAILED | STOPPED))
    {
        return Ok(());
    }
    let mut pending = owner
        .stereo_pending
        .take()
        .ok_or("stereo pending disappeared")?;
    pending.cancel();
    let entry = pending.entry;
    let mut registry = catalog_runtimes()
        .lock()
        .map_err(|_| "Runtime registry poisoned")?;
    let host = registry.get_mut(&entry.runtime).ok_or("Runtime absent")?;
    if host.stereo_dispatch_routes.contains(&entry.route) {
        host.bilateral_stereo.invalidate(entry.route);
        host.runtime
            .retire_route_authorization(entry.route)
            .map_err(|e| e.to_string())?;
    } else if host
        .runtime
        .route(entry.route)
        .is_ok_and(|route| matches!(route.state, RouteState::Prepared | RouteState::Starting))
    {
        if entry.state() == FAILED {
            crate::audio_negotiation::fail(host, entry.route)?;
        } else {
            host.runtime
                .begin_route_stop(entry.route)
                .map_err(|e| e.to_string())?;
            host.runtime
                .stop_route(entry.route)
                .map_err(|e| e.to_string())?;
        }
    }
    host.audio_status_pending.insert(entry.route);
    reconcile(&host.runtime);
    Ok(())
}
/// Remove only explicit stereo statuses from the legacy publisher so all new
/// stereo control sends happen after releasing the authoritative Runtime guard.
pub(super) fn take_statuses(
    host: &mut AndroidCatalogRuntimeV1,
    session: capyio_core::SessionId,
) -> Vec<Route> {
    let local = host.runtime.snapshot().local_node.id;
    let routes: Vec<_> = host
        .audio_status_pending
        .iter()
        .filter_map(|id| host.runtime.route(*id).ok())
        .filter(|route| {
            route.session_id == session
                && route.sink.node_id == local
                && route.profile == capyio_core::ProfileId::audio_frames_v1()
                && (route.selected_format
                    == Some(capyio_core::FormatDescriptor::new(
                        capyio_protocol::SECURE_AUDIO_STEREO_FORMAT,
                    ))
                    || route.compatible_formats
                        == [capyio_core::FormatDescriptor::new(
                            capyio_protocol::SECURE_AUDIO_STEREO_FORMAT,
                        )])
        })
        .cloned()
        .collect();
    for route in &routes {
        host.audio_status_pending.remove(&route.id);
    }
    routes
        .into_iter()
        .filter(|route| {
            !(host.stereo_dispatch_routes.contains(&route.id)
                && route.state == RouteState::Prepared)
        })
        .collect()
}

/// Invalidate first and always consume/close the owned link, even when Runtime
/// reconciliation fails. Callers still stop other media owners before reporting.
pub(super) fn disconnect(owner: &mut NativeControlListener) -> Result<(), String> {
    if let Some(identity) = owner.stereo_identity.take() {
        identity.invalidate();
    }
    if let Some(mut pending) = owner.stereo_pending.take() {
        pending.cancel();
    }
    owner.control_generation = 0;
    let mut errors = Vec::new();
    if let Some(mut link) = owner.link.take() {
        if let Some(runtime) = owner.runtime_handle {
            match catalog_runtimes().lock() {
                Ok(mut registry) => {
                    if let Some(host) = registry.get_mut(&runtime) {
                        if host.stereo_dispatch_routes.iter().any(|id| {
                            host.runtime
                                .route(*id)
                                .is_ok_and(|route| route.session_id == link.session_id())
                        }) {
                            if let Err(error) =
                                host.invalidate_bilateral_stereo_session(link.session_id())
                            {
                                errors.push(error.to_string());
                            }
                            if let Err(error) = host.invalidate_stereo_consent_control() {
                                errors.push(error);
                            }
                        }
                        if let Err(error) = link.close_and_reconcile(&mut host.runtime) {
                            errors.push(error.to_string());
                        }
                    } else {
                        errors.push("Runtime absent during disconnect".into());
                    }
                }
                Err(_) => errors.push("Runtime registry poisoned".into()),
            }
        }
        // No registry, retirement or reconciliation failure may strand TLS.
        link.close();
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}
pub(super) enum ControlPoll {
    Idle,
    Failed(String),
}
impl std::fmt::Display for ControlPoll {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => f.write_str("no stereo control message"),
            Self::Failed(error) => f.write_str(error),
        }
    }
}
pub(super) fn receive_message(
    pending: Option<&mut PendingStereo>,
    link: &mut CatalogControlLink,
    generation: u64,
) -> Result<ReceivedControlMessage, ControlPoll> {
    if let Some(pending) = pending {
        let deadline = before_poll(pending, link, generation).map_err(ControlPoll::Failed)?;
        link.poll_control_message_until(deadline)
            .map_err(|e| ControlPoll::Failed(e.to_string()))?
            .ok_or(ControlPoll::Idle)
    } else {
        link.receive_control_message()
            .map_err(|e| ControlPoll::Failed(e.to_string()))
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeCatalogRuntime_nativeStereoSecureStart(
    mut env: EnvUnowned<'_>,
    _class: JClass<'_>,
    runtime: jlong,
    route: JString<'_>,
) -> jlong {
    env.with_env(|env| -> jni::errors::Result<_> {
        let result = catch_unwind(AssertUnwindSafe(|| {
            start(
                catalog_handle(runtime)?,
                string(env, route)?
                    .parse()
                    .map_err(|_| "invalid stereo Route")?,
            )
        }));
        Ok(match result {
            Ok(Ok(id)) => id as jlong,
            Ok(Err(error)) => {
                throw(env, "java/lang/IllegalStateException", error);
                0
            }
            Err(_) => {
                throw(
                    env,
                    "java/lang/IllegalStateException",
                    "stereo Start panicked",
                );
                0
            }
        })
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}
/// States describe native setup only; counters are actual admitted/accepted
/// records and accepted PCM bytes. None of them claim Active or audibility.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeCatalogRuntime_nativeStereoSecureState(
    mut env: EnvUnowned<'_>,
    _class: JClass<'_>,
    runtime: jlong,
    generation: jlong,
    counters: JLongArray<'_>,
) -> jint {
    env.with_env(|env| -> jni::errors::Result<_> {
        let result = catch_unwind(AssertUnwindSafe(|| -> Result<i32, ()> {
            let entry = lookup(runtime, generation);
            if counters.is_null() || counters.len(env).ok() != Some(3) {
                if let Ok(entry) = &entry {
                    entry.reject();
                }
                return Err(());
            }
            let (state, values) = match entry {
                Ok(ref entry) => (entry.state(), entry.counters()),
                Err("busy") => (PENDING, [0; 3]),
                Err(_) => terminal()
                    .try_lock()
                    .ok()
                    .and_then(|last| {
                        last.filter(|last| {
                            last.runtime == runtime as u64 && last.generation == generation as u64
                        })
                    })
                    .map_or((STOPPED, [0; 3]), |last| (last.state, last.counters)),
            };
            if counters.set_region(env, 0, &values).is_err() {
                if let Ok(entry) = &entry {
                    entry.reject();
                }
                return Err(());
            }
            Ok(state)
        }));
        Ok(result.ok().and_then(Result::ok).unwrap_or(FAILED))
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeCatalogRuntime_nativeStereoSecureEnable(
    mut env: EnvUnowned<'_>,
    _class: JClass<'_>,
    runtime: jlong,
    generation: jlong,
) -> jboolean {
    env.with_env(|_| -> jni::errors::Result<_> {
        let result = catch_unwind(AssertUnwindSafe(|| {
            let entry = lookup(runtime, generation).ok()?;
            if !entry.live() {
                return None;
            }
            let Some(job) = entry.job.get() else {
                entry.reject();
                return None;
            };
            job.enable().ok()
        }));
        Ok(matches!(result, Ok(Some(()))))
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}
/// Read: 1 copied, 0 busy/empty, -1 terminal. Java owns both fixed arrays.
/// A copy is not acceptance. Metadata is token, logical frame offset, length.
/// Registry busy can precede validation; it admits no callback or PCM copy.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeCatalogRuntime_nativeStereoSecureRead(
    mut env: EnvUnowned<'_>,
    _class: JClass<'_>,
    runtime: jlong,
    generation: jlong,
    pcm: JByteArray<'_>,
    metadata: JLongArray<'_>,
) -> jint {
    env.with_env(|env| -> jni::errors::Result<_> {
        let result = catch_unwind(AssertUnwindSafe(|| -> Result<i32, ()> {
            let entry = match lookup(runtime, generation) {
                Ok(entry) => entry,
                Err("busy") => return Ok(0),
                Err(_) => return Err(()),
            };
            let valid = !pcm.is_null()
                && !metadata.is_null()
                && pcm.len(env).ok() == Some(1920)
                && metadata.len(env).ok() == Some(3);
            if !valid || !entry.live() {
                entry.reject();
                return Err(());
            }
            let Some(job) = entry.job.get() else {
                return Ok(0);
            };
            let mut bytes = [0u8; 1920];
            let Some(permit) = job
                .try_read(generation as u64, &mut bytes)
                .map_err(|_| ())?
            else {
                return Ok(0);
            };
            // jbyte is signed, with identical representation; this borrowed view
            // keeps exactly one fixed 1920-byte stack intermediate.
            let signed =
                unsafe { std::slice::from_raw_parts(bytes.as_ptr().cast::<i8>(), bytes.len()) };
            if pcm.set_region(env, 0, &signed[..permit.length]).is_err()
                || metadata
                    .set_region(
                        env,
                        0,
                        &[
                            permit.token as i64,
                            permit.offset as i64,
                            permit.length as i64,
                        ],
                    )
                    .is_err()
            {
                entry.reject();
                return Err(());
            }
            if !entry.live() {
                return Err(());
            }
            Ok(1)
        }));
        Ok(result.ok().and_then(Result::ok).unwrap_or(-1))
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}
/// ACK: 1 consumed, 0 busy (retry ONLY this ACK without writing again), -1 terminal.
/// Busy can precede validation and never consumes a token or accepts PCM.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeCatalogRuntime_nativeStereoSecureAck(
    mut env: EnvUnowned<'_>,
    _class: JClass<'_>,
    runtime: jlong,
    generation: jlong,
    token: jlong,
    offset: jint,
    accepted: jint,
) -> jint {
    env.with_env(|_| -> jni::errors::Result<_> {
        let result = catch_unwind(AssertUnwindSafe(|| -> Result<i32, ()> {
            let entry = match lookup(runtime, generation) {
                Ok(entry) => entry,
                Err("busy") => return Ok(0),
                Err(_) => return Err(()),
            };
            if token <= 0 || offset < 0 || accepted < 0 || !entry.live() {
                entry.reject();
                return Err(());
            }
            let Some(job) = entry.job.get() else {
                entry.reject();
                return Err(());
            };
            job.acknowledge(
                generation as u64,
                token as u64,
                offset as usize,
                accepted as usize,
            )
            .map(i32::from)
            .map_err(|_| ())
        }));
        Ok(result.ok().and_then(Result::ok).unwrap_or(-1))
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeCatalogRuntime_nativeStereoSecureStop(
    mut env: EnvUnowned<'_>,
    _class: JClass<'_>,
    runtime: jlong,
    generation: jlong,
) {
    env.with_env(|_| -> jni::errors::Result<_> {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            let entry = entry_snapshot()
                .filter(|e| e.runtime == runtime as u64 && e.generation == generation as u64);
            if let Some(entry) = entry {
                entry.stop();
            }
        }));
        Ok(())
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}
