//! One-shot native mono capture worker. Capture admission never waits for I/O or
//! a Runtime/job mutex, and Java can supply only one exact 10 ms PCM payload.
use crate::audio_sender::PreparedAudioSender;
use capyio_audio::{AudioMediaPacket, AudioMediaStreamBinding, AudioStreamSpec};
use capyio_control_tls::audio::{AudioTlsBinding, AudioTlsStopHandle};
use capyio_native_audio_lan::{
    SECURE_MONO_PCM_PAYLOAD_BYTES, SECURE_MONO_PCM_RECORD_BYTES, SECURE_MONO_PCM_SAMPLES,
    SecureMonoPcmAdmission,
};
use capyio_protocol::SecureAudioSessionProfile;
use capyio_runtime::NodeRuntime;
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicBool, AtomicI32, AtomicUsize, Ordering},
    mpsc::{self, Receiver, SyncSender},
};
use std::{
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub(crate) const PENDING: i32 = 0;
pub(crate) const READY: i32 = 1;
pub(crate) const FAILED: i32 = 2;
pub(crate) const STOPPED: i32 = 3;
/// At least one canonical PCM record completed TLS write and fresh revalidation.
pub(crate) const STREAMING: i32 = 4;
const RECORD_SLOTS: usize = 2;
pub(crate) type Pcm = [u8; SECURE_MONO_PCM_PAYLOAD_BYTES];
pub(crate) type Snapshot = Arc<dyn Fn() -> Result<NodeRuntime, String> + Send + Sync>;
pub(crate) type Preparation = Box<dyn FnOnce() -> Result<PreparedAudioSender, String> + Send>;
pub(crate) type Failure = Arc<dyn Fn() + Send + Sync>;

/// Runtime owns this durable claim, not the reapable worker registry. Failed or
/// stopped generations cannot restart a consumed Route, even after reaping.
#[derive(Clone, Debug, Default)]
pub(crate) struct GenerationClaim {
    consumed: bool,
}
impl GenerationClaim {
    pub(crate) fn reserve(&mut self) -> Result<(), String> {
        if self.consumed {
            return Err("audio Route generation already consumed".into());
        }
        self.consumed = true;
        Ok(())
    }
}

/// Reserve before copying Java bytes. The two slots include outstanding permits,
/// queued PCM and the worker's current PCM, rather than only queued messages.
/// Encoding has additional fixed 960-byte packet / 1064-byte wire staging.
pub(crate) struct RecordPermit {
    job: Arc<AudioJob>,
}
impl Drop for RecordPermit {
    fn drop(&mut self) {
        self.job.reserved.fetch_sub(1, Ordering::AcqRel);
    }
}
pub(crate) struct Record {
    pcm: Pcm,
    _permit: RecordPermit,
}
pub(crate) struct AudioJob {
    pub(crate) runtime: u64,
    pub(crate) generation: u64,
    pub(crate) binding: AudioTlsBinding,
    pub(crate) profile: SecureAudioSessionProfile,
    state: AtomicI32,
    stop_handle: OnceLock<AudioTlsStopHandle>,
    reserved: AtomicUsize,
    queue: SyncSender<Record>,
    worker: Mutex<Option<JoinHandle<()>>>,
    finished: AtomicBool,
    lease_deadline_ms: u64,
    lease_deadline: Instant,
    preparation_deadline: Instant,
}
impl AudioJob {
    pub(crate) fn pending(
        runtime: u64,
        generation: u64,
        binding: AudioTlsBinding,
        profile: SecureAudioSessionProfile,
        lease_deadline_ms: u64,
        preparation_deadline: Instant,
    ) -> (Arc<Self>, Receiver<Record>) {
        let (queue, receiver) = mpsc::sync_channel(RECORD_SLOTS);
        let lifetime =
            Duration::from_millis(lease_deadline_ms.saturating_sub(now().unwrap_or(u64::MAX)))
                .min(profile.max_duration());
        (
            Arc::new(Self {
                runtime,
                generation,
                binding,
                profile,
                state: AtomicI32::new(PENDING),
                stop_handle: OnceLock::new(),
                reserved: AtomicUsize::new(0),
                queue,
                worker: Mutex::new(None),
                finished: AtomicBool::new(false),
                lease_deadline_ms,
                preparation_deadline,
                lease_deadline: Instant::now()
                    .checked_add(lifetime)
                    .unwrap_or_else(Instant::now)
                    .min(preparation_deadline),
            }),
            receiver,
        )
    }
    pub(crate) fn state(&self) -> i32 {
        if Instant::now() >= self.lease_deadline
            || now().map_or(true, |now| now >= self.lease_deadline_ms)
        {
            self.fail();
        }
        self.state.load(Ordering::Acquire)
    }
    pub(crate) fn finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }
    /// The completion marker precedes the worker's final return. Registry
    /// retirement must instead observe actual thread completion before joining
    /// and replacing a generation. Contention is conservatively not finished.
    pub(crate) fn worker_exited(&self) -> bool {
        self.worker.try_lock().is_ok_and(|worker| {
            worker
                .as_ref()
                .map_or_else(|| self.finished(), JoinHandle::is_finished)
        })
    }
    pub(crate) fn stop(&self) {
        self.terminate(STOPPED);
    }
    pub(crate) fn fail(&self) {
        self.terminate(FAILED);
    }
    fn terminate(&self, terminal: i32) {
        // First terminal result wins. A late expiry cannot turn explicit Stop
        // into failure, and worker cleanup cannot hide an earlier failure.
        let _ = self
            .state
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |state| {
                matches!(state, PENDING | READY | STREAMING).then_some(terminal)
            });
        if let Some(stop) = self.stop_handle.get() {
            stop.stop();
        }
    }
    fn install_stop(&self, stop: AudioTlsStopHandle) -> Result<(), String> {
        if let Err(stop) = self.stop_handle.set(stop) {
            stop.stop();
            return Err("audio connection Stop handle already installed".into());
        }
        // Covers Stop racing either side of installation. No status lock is
        // needed, including when TLS has not yet completed its binding ACK.
        if !matches!(self.state(), PENDING | READY | STREAMING) {
            self.stop_handle.get().unwrap().stop();
        }
        Ok(())
    }
    pub(crate) fn reserve(self: &Arc<Self>, length: usize) -> Result<RecordPermit, String> {
        if length != SECURE_MONO_PCM_PAYLOAD_BYTES {
            self.fail();
            return Err("audio requires exactly 960 PCM bytes".into());
        }
        if !matches!(self.state(), READY | STREAMING) {
            return Err("audio generation is not ready".into());
        }
        self.reserved
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < RECORD_SLOTS).then_some(count + 1)
            })
            .map_err(|_| {
                self.fail();
                "audio PCM queue full".to_owned()
            })?;
        let permit = RecordPermit {
            job: Arc::clone(self),
        };
        if !matches!(self.state(), READY | STREAMING) {
            return Err("audio generation stopped".into());
        }
        Ok(permit)
    }
    pub(crate) fn submit(&self, permit: RecordPermit, pcm: Pcm) -> Result<(), String> {
        // Identity, not just an externally visible generation integer, prevents
        // a permit from being substituted across Runtime owners or Routes.
        if !std::ptr::eq(Arc::as_ptr(&permit.job), self)
            || !matches!(self.state(), READY | STREAMING)
        {
            return Err("stale audio generation".into());
        }
        self.queue
            .try_send(Record {
                pcm,
                _permit: permit,
            })
            .map_err(|_| {
                self.fail();
                "audio PCM queue unavailable".into()
            })
    }
    pub(crate) fn spawn(
        self: &Arc<Self>,
        receiver: Receiver<Record>,
        prepared: Preparation,
        snapshot: Snapshot,
        failure: Failure,
    ) -> Result<(), String> {
        // Prevent a short-lived worker's completion marker being observed with
        // an empty handle slot before spawn has installed its actual handle.
        // This lock never spans Runtime, control or media I/O.
        let mut worker = self
            .worker
            .lock()
            .map_err(|_| "audio worker registry poisoned")?;
        if worker.is_some() || self.finished() {
            return Err("audio worker already spawned".into());
        }
        let job = Arc::clone(self);
        let handle = thread::Builder::new()
            .name("CapyIO-secure-audio".into())
            .spawn(move || {
                if job.run(receiver, prepared, snapshot).is_err() {
                    job.fail();
                }
                if job.state() == FAILED {
                    failure();
                }
                job.stop();
                job.finished.store(true, Ordering::Release);
            })
            .map_err(|error| {
                self.fail();
                self.finished.store(true, Ordering::Release);
                error.to_string()
            })?;
        *worker = Some(handle);
        Ok(())
    }
    fn run(
        &self,
        receiver: Receiver<Record>,
        prepared: Preparation,
        snapshot: Snapshot,
    ) -> Result<(), String> {
        if self.state() != PENDING {
            return Ok(());
        }
        let prepared = prepared()?;
        if prepared.binding() != self.binding
            || prepared.profile() != self.profile
            || prepared.lease_deadline() != self.lease_deadline_ms
            || prepared.monotonic_deadline() != self.preparation_deadline
        {
            return Err("audio job selection differs from preparation".into());
        }
        prepared.validate_snapshot(&snapshot()?, now()?)?;
        if self.state() != PENDING {
            return Ok(());
        }
        let mut sender = prepared.connect_with_stop(|stop| {
            if self.install_stop(stop).is_err() {
                self.fail();
            }
        })?;
        sender.check(&snapshot()?, now()?)?;
        if self
            .state
            .compare_exchange(PENDING, READY, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            sender.stop();
            return Ok(());
        }
        let mut timeline = PcmTimeline::new(self.binding)?;
        while matches!(self.state(), READY | STREAMING) {
            sender.check(&snapshot()?, now()?)?;
            match receiver.recv_timeout(Duration::from_millis(10)) {
                Ok(record) => {
                    if !matches!(self.state(), READY | STREAMING) {
                        break;
                    }
                    let wire = timeline.encode(record.pcm)?;
                    sender.send_record_with_snapshot(|| snapshot(), &wire, now()?)?;
                    let _ = self.state.compare_exchange(
                        READY,
                        STREAMING,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    );
                    // record's permit stays live through both fresh snapshots
                    // and the write, so in-flight PCM counts toward the bound.
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        sender.stop();
        Ok(())
    }
    /// Invoke outside Service/Runtime/registry locks. An early completion marker
    /// alone never authorizes joining: only an actually exited handle is taken,
    /// and the worker mutex is released before join. No detach or self-join.
    pub(crate) fn reap(&self) {
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
                self.fail();
            }
            self.finished.store(true, Ordering::Release);
        }
    }
}

/// Java cannot inject CPYA headers, epochs, sequence, sample index or clocks.
/// Relative PTS starts at zero and derives solely from this worker's timeline.
struct PcmTimeline {
    admission: SecureMonoPcmAdmission,
    sequence: u64,
}
impl PcmTimeline {
    fn new(binding: AudioTlsBinding) -> Result<Self, String> {
        Ok(Self {
            admission: SecureMonoPcmAdmission::new(AudioMediaStreamBinding {
                session_id: binding.session.session_id,
                route_id: binding.route_id,
                stream_id: binding.stream_id,
                stream_epoch: u32::try_from(binding.epoch).map_err(|_| "audio epoch overflow")?,
                selected_spec: AudioStreamSpec::voice_interactive(),
            })
            .map_err(|error| error.to_string())?,
            sequence: 0,
        })
    }
    fn encode(&mut self, pcm: Pcm) -> Result<[u8; SECURE_MONO_PCM_RECORD_BYTES], String> {
        let next = self
            .sequence
            .checked_add(1)
            .ok_or("audio sequence overflow")?;
        // Validate the next sample/PTS too, matching canonical admission's rule
        // that a packet without a representable continuation is not admitted.
        next.checked_mul(10_000).ok_or("audio timestamp overflow")?;
        next.checked_mul(u64::from(SECURE_MONO_PCM_SAMPLES))
            .ok_or("audio sample overflow")?;
        let binding = self.admission.binding();
        let packet = AudioMediaPacket {
            stream_id: binding.stream_id,
            stream_epoch: binding.stream_epoch,
            sequence: self.sequence,
            source_timestamp_micros: self.sequence * 10_000,
            first_sample_index: self.sequence * u64::from(SECURE_MONO_PCM_SAMPLES),
            sample_count: SECURE_MONO_PCM_SAMPLES,
            discontinuity: false,
            payload: pcm.to_vec(),
        };
        let wire = self
            .admission
            .encode_record(&packet)
            .map_err(|error| error.to_string())?;
        self.sequence = next;
        Ok(wire)
    }
}
pub(crate) fn now() -> Result<u64, String> {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_millis(),
    )
    .map_err(|error| error.to_string())
}

#[cfg(test)]
#[path = "audio_job_tests.rs"]
mod tests;
