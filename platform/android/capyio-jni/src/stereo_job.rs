//! One-shot stereo receive worker. Only this worker touches TLS or the borrowed
//! Runtime callback. JNI read/ACK never acquire Runtime, control or registry locks.
//! Copying PCM out is not acceptance, playback, or physical audibility.
use crate::{
    secure_stereo_receiver::{MAX_STEREO_OPERATION, SecureStereoSinkMetrics},
    stereo_admission::PendingPcm,
    stereo_negotiation::StereoSinkSnapshot,
};
use capyio_audio::{AudioMediaStreamBinding, AudioStreamSpec};
use capyio_control_tls::{
    ControlTlsServerIdentity,
    audio_stereo::{StereoAudioTlsListener, StereoAudioTlsStopHandle},
};
use capyio_native_audio_lan::{
    SECURE_STEREO_PCM_PAYLOAD_BYTES, SecureStereoPcmAdmission, SecureStereoPcmRateBudget,
};
use capyio_protocol::{SecureAudioDataOffer, SecureAudioTransport};
use capyio_runtime::NodeRuntime;
use std::{
    net::{SocketAddr, TcpListener},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const PENDING: i32 = 0;
pub const BOUND: i32 = 1;
pub const FAILED: i32 = 2;
pub const STOPPED: i32 = 3;
/// Local readiness only. Never Route Active or a rendering/audibility assertion.
pub const ENABLED: i32 = 4;
// Process-wide positive IDs never repeat across jobs or generation reuse.
static NEXT_PERMIT: AtomicU64 = AtomicU64::new(1);
pub type StereoAuthority = Arc<dyn Fn(&StereoSinkSnapshot) -> Result<(), String> + Send + Sync>;
pub type StereoFailure = Arc<dyn Fn() + Send + Sync>;

/// Shared with the authoritative Runtime owner. Invalidate synchronously before
/// publishing a changed original authority. Closed generations can never reopen.
#[derive(Clone, Debug)]
pub struct StereoAdmissionGate(Arc<AtomicBool>);
impl Default for StereoAdmissionGate {
    fn default() -> Self {
        Self::new()
    }
}
impl StereoAdmissionGate {
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(true)))
    }
    pub fn invalidate(&self) {
        self.0.store(false, Ordering::Release);
    }
    pub fn is_open(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Keep this durable tombstone with the Runtime, not in the reapable job map.
#[derive(Clone, Debug, Default)]
pub struct StereoGenerationClaim {
    consumed: Arc<AtomicBool>,
}
impl StereoGenerationClaim {
    pub fn reserve(&mut self) -> Result<(), &'static str> {
        self.consumed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "stereo Route generation already consumed")?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PcmReadPermit {
    pub generation: u64,
    pub token: u64,
    pub offset: usize,
    pub length: usize,
}
#[derive(Default)]
struct PcmSlot {
    pending: Option<PendingPcm>,
    permit: Option<PcmReadPermit>,
    authorized: bool,
    ack: Option<(usize, bool)>,
}

pub struct StereoReceiveJob {
    pub runtime: u64,
    pub generation: u64,
    snapshot: StereoSinkSnapshot,
    gate: StereoAdmissionGate,
    authority: StereoAuthority,
    state: AtomicI32,
    offer_committed: AtomicBool,
    address: SocketAddr,
    accept_deadline: Instant,
    listener_stop: Option<StereoAudioTlsStopHandle>,
    channel_stop: OnceLock<StereoAudioTlsStopHandle>,
    slot: Mutex<PcmSlot>,
    worker: Mutex<Option<JoinHandle<()>>>,
    finished: AtomicBool,
    last_wall: AtomicU64,
    records_admitted: AtomicU64,
    records_accepted: AtomicU64,
    bytes_accepted: AtomicU64,
}
impl StereoReceiveJob {
    /// Called outside Runtime/global registry guards. A per-link serialization
    /// guard is permitted only if the authority callback never takes it. Identity must
    /// come from identity_control_generation, never a role-specific stale cache.
    /// Binding the listener does NOT enable accept: offer_committed is required.
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        runtime: u64,
        generation: u64,
        snapshot: StereoSinkSnapshot,
        identity: ControlTlsServerIdentity,
        identity_control_generation: u64,
        gate: StereoAdmissionGate,
        authority: StereoAuthority,
        failure: StereoFailure,
    ) -> Result<Arc<Self>, String> {
        if runtime == 0
            || generation == 0
            || generation > i64::MAX as u64
            || identity_control_generation != snapshot.control_generation
            || identity.expected_peer_certificate != snapshot.peer_certificate
            || !gate.is_open()
        {
            gate.invalidate();
            return Err("invalid exact stereo job identity or generation".into());
        }
        let result = (|| {
            authority(&snapshot)?;
            let now = now_ms()?;
            if now < snapshot.wall_floor
                || now >= snapshot.wall_deadline_ms()
                || Instant::now() >= snapshot.end
                || !gate.is_open()
            {
                return Err("stereo authority expired before listener".into());
            }
            let accept_deadline = snapshot.end.min(Instant::now() + MAX_STEREO_OPERATION);
            let binding = snapshot.binding(now)?;
            let listener = StereoAudioTlsListener::from_listener_until(
                TcpListener::bind((snapshot.admission.local_address, 0))
                    .map_err(|e| e.to_string())?,
                identity,
                binding,
                MAX_STEREO_OPERATION,
                snapshot.end,
            )
            .map_err(|e| e.to_string())?;
            let address = listener.local_addr().map_err(|e| e.to_string())?;
            let listener_stop = listener.stop_handle().map_err(|e| e.to_string())?;
            let job = Arc::new(Self {
                runtime,
                generation,
                snapshot,
                gate: gate.clone(),
                authority,
                state: AtomicI32::new(PENDING),
                offer_committed: AtomicBool::new(false),
                address,
                accept_deadline,
                listener_stop: Some(listener_stop),
                channel_stop: OnceLock::new(),
                slot: Mutex::new(PcmSlot::default()),
                worker: Mutex::new(None),
                finished: AtomicBool::new(false),
                last_wall: AtomicU64::new(now),
                records_admitted: AtomicU64::new(0),
                records_accepted: AtomicU64::new(0),
                bytes_accepted: AtomicU64::new(0),
            });
            let mut handle_slot = job
                .worker
                .lock()
                .map_err(|_| "stereo worker mutex poisoned")?;
            let worker_job = Arc::clone(&job);
            let handle = thread::Builder::new()
                .name("CapyIO-stereo-receive".into())
                .spawn(move || {
                    if worker_job.run(listener).is_err() {
                        worker_job.latch(FAILED);
                    }
                    worker_job.shutdown_transport();
                    if worker_job.state() == FAILED {
                        failure();
                    }
                    worker_job.finished.store(true, Ordering::Release);
                })
                .map_err(|e| {
                    job.latch(FAILED);
                    job.finished.store(true, Ordering::Release);
                    e.to_string()
                })?;
            *handle_slot = Some(handle);
            drop(handle_slot);
            Ok(job)
        })();
        if result.is_err() {
            gate.invalidate();
        }
        result
    }
    pub fn snapshot(&self) -> &StereoSinkSnapshot {
        &self.snapshot
    }
    pub fn gate(&self) -> StereoAdmissionGate {
        self.gate.clone()
    }
    pub fn listener_address(&self) -> SocketAddr {
        self.address
    }
    pub fn offer(&self) -> SecureAudioDataOffer {
        SecureAudioDataOffer {
            port: self.address.port(),
            stream_id: self.snapshot.admission.stream_id,
            transport: SecureAudioTransport::Tls13MtlsStereoV1,
        }
    }
    pub fn operation_deadline(&self) -> Instant {
        self.accept_deadline
    }
    /// After successful outside-lock publication, revalidate original authority.
    /// Failed publication must call Stop; no handshake/read is possible first.
    pub fn offer_committed(&self) -> Result<(), String> {
        let result = (|| {
            self.fresh()?;
            if self.state() != PENDING
                || Instant::now() >= self.accept_deadline
                || self
                    .offer_committed
                    .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                    .is_err()
            {
                return Err("stale or repeated stereo offer commit".into());
            }
            self.check_live().map_err(str::to_owned)
        })();
        if result.is_err() {
            self.latch(FAILED);
        }
        result
    }
    pub fn state(&self) -> i32 {
        if self.check_limits().is_err() {
            self.latch(FAILED);
        }
        self.state.load(Ordering::Acquire)
    }
    pub fn enable(&self) -> Result<(), &'static str> {
        self.check_live()?;
        if self
            .state
            .compare_exchange(BOUND, ENABLED, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            self.latch(FAILED);
            return Err("stereo enable requires one current Bound observation");
        }
        self.check_live()
    }
    /// No Runtime lock, network, joining, or heap allocation. One outstanding
    /// token at most. Busy/empty returns None and does not advance any timeline.
    pub fn try_read(
        &self,
        generation: u64,
        buffer: &mut [u8],
    ) -> Result<Option<PcmReadPermit>, &'static str> {
        if generation != self.generation || buffer.len() != SECURE_STEREO_PCM_PAYLOAD_BYTES {
            self.latch(FAILED);
            return Err("invalid stereo generation or PCM buffer");
        }
        self.check_live()?;
        if self.state.load(Ordering::Acquire) != ENABLED {
            return Ok(None);
        }
        let Ok(mut slot) = self.slot.try_lock() else {
            return Ok(None);
        };
        if !slot.authorized || slot.permit.is_some() || slot.ack.is_some() {
            return Ok(None);
        }
        let Some(pending) = slot.pending.as_mut() else {
            return Ok(None);
        };
        let offset = pending.offset;
        let bytes = pending.before_write_checked(Instant::now()).map_err(|_| {
            self.latch(FAILED);
            "stereo PCM deadline or attempts exhausted"
        })?;
        let length = bytes.len();
        buffer[..length].copy_from_slice(bytes);
        let token = NEXT_PERMIT
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |next| {
                next.checked_add(1).filter(|n| *n <= i64::MAX as u64)
            })
            .map_err(|_| {
                self.latch(FAILED);
                "stereo permit counter exhausted"
            })?;
        let permit = PcmReadPermit {
            generation,
            token,
            offset,
            length,
        };
        self.check_live()?;
        slot.permit = Some(permit);
        slot.authorized = false;
        Ok(Some(permit))
    }
    /// False is transient try-lock contention; that token has not been consumed.
    /// Retry only this ACK on false, never repeat the platform write.
    /// True consumes the exact token once, including a zero-progress ACK. Actual
    /// accepted bytes are published only after worker-side fresh revalidation.
    pub fn acknowledge(
        &self,
        generation: u64,
        token: u64,
        offset: usize,
        accepted: usize,
    ) -> Result<bool, &'static str> {
        if generation != self.generation || token == 0 {
            self.latch(FAILED);
            return Err("stale stereo ACK generation or token");
        }
        self.check_live()?;
        if self.state.load(Ordering::Acquire) != ENABLED {
            self.latch(FAILED);
            return Err("stereo ACK before enable");
        }
        let Ok(mut slot) = self.slot.try_lock() else {
            return Ok(false);
        };
        let Some(permit) = slot.permit else {
            self.latch(FAILED);
            return Err("replayed or absent stereo ACK permit");
        };
        if permit.generation != generation
            || permit.token != token
            || permit.offset != offset
            || accepted > permit.length
            || !accepted.is_multiple_of(4)
        {
            self.latch(FAILED);
            return Err("stale or invalid stereo ACK");
        }
        self.check_live()?;
        let result = slot
            .pending
            .as_mut()
            .ok_or_else(|| {
                self.latch(FAILED);
                "stereo PCM missing"
            })?
            .after_accept(accepted, Instant::now())
            .map_err(|_| {
                self.latch(FAILED);
                "stereo ACK exceeded original PCM deadline"
            })?;
        slot.permit = None;
        slot.ack = Some(result);
        self.check_live()?;
        Ok(true)
    }
    /// Safe under the Runtime guard: no job mutex, callback, socket or join.
    /// Call after mutations but before releasing their guard/publication. If
    /// authority is known to change, invalidate the gate before mutating instead.
    pub fn reconcile(
        &self,
        runtime: &NodeRuntime,
        control_generation: u64,
        now: u64,
    ) -> Result<(), String> {
        let result = self
            .snapshot
            .validate(runtime, control_generation, now, Instant::now());
        if result.is_err() {
            self.latch(FAILED);
        }
        result
    }
    pub fn metrics(&self) -> SecureStereoSinkMetrics {
        SecureStereoSinkMetrics {
            records_admitted: self.records_admitted.load(Ordering::Acquire),
            records_accepted: self.records_accepted.load(Ordering::Acquire),
            pcm_bytes_accepted: self.bytes_accepted.load(Ordering::Acquire),
        }
    }
    fn check_limits(&self) -> Result<(), &'static str> {
        let previous = self.last_wall.load(Ordering::Acquire);
        let now = wall_now()?;
        self.last_wall.fetch_max(now, Ordering::AcqRel);
        if !self.gate.is_open()
            || now < previous
            || now >= self.snapshot.wall_deadline_ms()
            || Instant::now() >= self.snapshot.end
        {
            return Err("stereo authority expired, changed or stopped");
        }
        Ok(())
    }
    fn check_live(&self) -> Result<(), &'static str> {
        if let Err(error) = self.check_limits() {
            self.latch(FAILED);
            return Err(error);
        }
        if matches!(self.state.load(Ordering::Acquire), FAILED | STOPPED) {
            return Err("stereo job terminal");
        }
        Ok(())
    }
    fn fresh(&self) -> Result<(), String> {
        self.check_live().map_err(str::to_owned)?;
        (self.authority)(&self.snapshot)?;
        self.check_live().map_err(str::to_owned)
    }
    fn latch(&self, terminal: i32) {
        // The terminal state itself closes fast-path admission before the gate
        // changes, so a racing worker cannot overwrite an explicit Stop.
        let _ = self
            .state
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |state| {
                matches!(state, PENDING | BOUND | ENABLED).then_some(terminal)
            });
        self.gate.invalidate();
    }
    fn shutdown_transport(&self) {
        if let Some(stop) = &self.listener_stop {
            stop.stop();
        }
        if let Some(stop) = self.channel_stop.get() {
            stop.stop();
        }
    }
    /// Admission closes before independent socket shutdown. Never joins.
    pub fn stop(&self) {
        self.latch(STOPPED);
        self.shutdown_transport();
    }
    pub fn fail(&self) {
        self.latch(FAILED);
        self.shutdown_transport();
    }
    /// JNI validation failure: atomics only, never socket I/O or allocation.
    pub fn reject_callback(&self) {
        self.latch(FAILED);
    }
    pub fn accepted_bytes(&self) -> u64 {
        self.bytes_accepted.load(Ordering::Acquire)
    }
    pub fn control_generation(&self) -> u64 {
        self.snapshot.control_generation
    }
    pub fn finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }
    pub fn worker_exited(&self) -> bool {
        self.worker.try_lock().is_ok_and(|worker| {
            worker
                .as_ref()
                .map_or_else(|| self.finished(), JoinHandle::is_finished)
        })
    }
    /// Invoke outside every registry/Runtime/control guard. Takes only an
    /// actually-finished handle, releases its mutex, then joins. No detachment.
    pub fn reap_finished(&self) {
        let handle = {
            let Ok(mut worker) = self.worker.try_lock() else {
                return;
            };
            if worker.as_ref().is_some_and(JoinHandle::is_finished) {
                worker.take()
            } else {
                None
            }
        };
        if let Some(handle) = handle {
            if handle.join().is_err() {
                self.latch(FAILED);
            }
            self.finished.store(true, Ordering::Release);
        }
    }
    fn run(&self, listener: StereoAudioTlsListener) -> Result<(), String> {
        // Publication must complete before handshake/Bound can become observable.
        while !self.offer_committed.load(Ordering::Acquire) {
            self.fresh()?;
            if Instant::now() >= self.accept_deadline {
                return Err("stereo offer commit deadline".into());
            }
            thread::sleep(Duration::from_millis(1));
        }
        let mut channel = loop {
            self.fresh()?;
            if Instant::now() >= self.accept_deadline {
                return Err("stereo accept deadline".into());
            }
            if let Some(channel) = listener
                .try_accept_until(self.accept_deadline)
                .map_err(|e| e.to_string())?
            {
                break channel;
            }
            thread::sleep(Duration::from_millis(1));
        };
        let stop = channel.stop_handle().map_err(|e| e.to_string())?;
        if let Err(stop) = self.channel_stop.set(stop) {
            stop.stop();
            return Err("stereo transport registered twice".into());
        }
        self.fresh()?;
        if Instant::now() >= self.accept_deadline {
            return Err("stereo binding deadline".into());
        }
        self.state
            .compare_exchange(PENDING, BOUND, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "stereo stopped before binding")?;
        let mut parser = SecureStereoPcmAdmission::new(AudioMediaStreamBinding {
            session_id: self.snapshot.original.session_id,
            route_id: self.snapshot.original.id,
            stream_id: self.snapshot.admission.stream_id,
            stream_epoch: u32::try_from(self.snapshot.original.epoch)
                .map_err(|_| "stereo epoch overflow")?,
            selected_spec: AudioStreamSpec::media_balanced(),
        })
        .map_err(|e| e.to_string())?;
        let mut rate = SecureStereoPcmRateBudget::new(self.snapshot.admission.lifetime)
            .map_err(|e| e.to_string())?;
        loop {
            self.fresh()?;
            if self.state.load(Ordering::Acquire) != ENABLED {
                thread::sleep(Duration::from_millis(1));
                continue;
            }
            // No catalog guard is held alongside this one-record slot lock.
            let needs_record = {
                let slot = self.slot.lock().map_err(|_| "stereo PCM slot poisoned")?;
                if slot
                    .pending
                    .as_ref()
                    .is_some_and(|p| Instant::now() >= p.deadline)
                {
                    return Err("stereo PCM ACK deadline".into());
                }
                slot.pending.is_none()
            };
            if needs_record {
                self.fresh()?;
                let deadline = self.snapshot.end.min(Instant::now() + MAX_STEREO_OPERATION);
                let wire = channel
                    .receive_record_until(deadline)
                    .map_err(|e| e.to_string())?;
                self.fresh()?;
                rate.admit(self.snapshot.started.elapsed())
                    .map_err(|e| e.to_string())?;
                let packet = parser.admit_record(&wire).map_err(|e| e.to_string())?;
                let pending = PendingPcm::new(packet.payload, Instant::now(), deadline)
                    .map_err(|e| e.to_string())?;
                self.fresh()?;
                let mut slot = self.slot.lock().map_err(|_| "stereo PCM slot poisoned")?;
                self.check_live().map_err(str::to_owned)?;
                slot.pending = Some(pending);
                slot.authorized = true;
                self.records_admitted.fetch_add(1, Ordering::AcqRel);
            } else {
                let ack = {
                    let mut slot = self.slot.lock().map_err(|_| "stereo PCM slot poisoned")?;
                    slot.ack.take()
                };
                if let Some((accepted, complete)) = ack {
                    // Accounting in ACK is not authority. Do not publish metrics,
                    // issue another permit, or read another record without this.
                    self.fresh()?;
                    let mut slot = self.slot.lock().map_err(|_| "stereo PCM slot poisoned")?;
                    self.check_live().map_err(str::to_owned)?;
                    if slot
                        .pending
                        .as_ref()
                        .is_none_or(|p| Instant::now() >= p.deadline)
                    {
                        return Err("stereo ACK validation deadline".into());
                    }
                    self.bytes_accepted
                        .fetch_add(accepted as u64, Ordering::AcqRel);
                    if complete {
                        slot.pending = None;
                        slot.authorized = false;
                        self.records_accepted.fetch_add(1, Ordering::AcqRel);
                    } else {
                        slot.authorized = true;
                    }
                } else {
                    thread::sleep(Duration::from_millis(1));
                }
            }
        }
    }
}
fn wall_now() -> Result<u64, &'static str> {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "stereo wall clock unavailable")?
            .as_millis(),
    )
    .map_err(|_| "stereo wall clock overflow")
}
fn now_ms() -> Result<u64, String> {
    wall_now().map_err(str::to_owned)
}

#[cfg(test)]
#[path = "stereo_job_tests.rs"]
mod tests;
