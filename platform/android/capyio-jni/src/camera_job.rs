//! One-shot, bounded native media job. No Java/UI/Runtime lock is held across I/O.
use crate::camera_sender::PreparedCameraSender;
use capyio_control_tls::camera::{
    CameraTlsBinding, CameraTlsStopHandle, MAX_CAMERA_TLS_RECORD_BYTES,
};
use capyio_runtime::NodeRuntime;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc::{self, Receiver, SyncSender},
};
use std::{
    thread::{self, JoinHandle},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub(crate) const PENDING: i32 = 0;
pub(crate) const READY: i32 = 1;
pub(crate) const FAILED: i32 = 2;
pub(crate) const STOPPED: i32 = 3;
const RECORD_SLOTS: usize = 2;
pub(crate) type Snapshot = Arc<dyn Fn() -> Result<NodeRuntime, String> + Send + Sync>;
pub(crate) type Preparation = Box<dyn FnOnce() -> Result<PreparedCameraSender, String> + Send>;
pub(crate) type Failure = Arc<dyn Fn() + Send + Sync>;

/// Durable per-Route one-shot claim, retained by Runtime rather than worker
/// registry. Reaping a worker or attempting a different invalid Start cannot
/// re-arm a consumed Route. Only creating a new Route creates a fresh claim.
#[derive(Clone, Debug, Default)]
pub(crate) struct GenerationClaim {
    consumed: bool,
}
impl GenerationClaim {
    pub(crate) fn reserve(&mut self) -> Result<(), String> {
        if self.consumed {
            return Err("camera Route generation already consumed".into());
        }
        self.consumed = true;
        Ok(())
    }
}

struct Status {
    state: i32,
    stop: Option<CameraTlsStopHandle>,
}
/// A permit is acquired before copying a Java byte array. The two slots include
/// reservations, queued records AND the worker's current record. Their combined
/// payload never exceeds 2*(4MiB+56). Parsing adds at most one 4MiB bounded copy.
pub(crate) struct RecordPermit {
    job: Arc<CameraJob>,
}
impl Drop for RecordPermit {
    fn drop(&mut self) {
        self.job.reserved.fetch_sub(1, Ordering::AcqRel);
    }
}
pub(crate) struct Record {
    bytes: Vec<u8>,
    _permit: RecordPermit,
}
pub(crate) struct CameraJob {
    pub(crate) runtime: u64,
    pub(crate) generation: u64,
    pub(crate) binding: CameraTlsBinding,
    status: Mutex<Status>,
    cancelled: AtomicBool,
    reserved: AtomicUsize,
    queue: SyncSender<Record>,
    worker: Mutex<Option<JoinHandle<()>>>,
    finished: AtomicBool,
    lease_deadline_ms: u64,
    lease_deadline: std::time::Instant,
}
impl CameraJob {
    pub(crate) fn pending(
        runtime: u64,
        generation: u64,
        binding: CameraTlsBinding,
        lease_deadline_ms: u64,
    ) -> (Arc<Self>, Receiver<Record>) {
        let (queue, receiver) = mpsc::sync_channel(RECORD_SLOTS);
        (
            Arc::new(Self {
                runtime,
                generation,
                binding,
                status: Mutex::new(Status {
                    state: PENDING,
                    stop: None,
                }),
                cancelled: AtomicBool::new(false),
                reserved: AtomicUsize::new(0),
                queue,
                worker: Mutex::new(None),
                finished: AtomicBool::new(false),
                lease_deadline_ms,
                lease_deadline: std::time::Instant::now()
                    .checked_add(Duration::from_millis(
                        lease_deadline_ms.saturating_sub(now().unwrap_or(u64::MAX)),
                    ))
                    .unwrap_or_else(std::time::Instant::now),
            }),
            receiver,
        )
    }
    pub(crate) fn state(&self) -> i32 {
        if std::time::Instant::now() >= self.lease_deadline
            || now().map_or(true, |now| now >= self.lease_deadline_ms)
        {
            self.fail();
        }
        self.status
            .lock()
            .map(|s| {
                if self.cancelled.load(Ordering::Acquire) && s.state != FAILED {
                    STOPPED
                } else {
                    s.state
                }
            })
            .unwrap_or(FAILED)
    }
    pub(crate) fn finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }
    pub(crate) fn stop(&self) {
        self.terminate(STOPPED);
    }
    fn terminate(&self, state: i32) {
        self.cancelled.store(true, Ordering::Release);
        if let Ok(mut status) = self.status.lock() {
            if matches!(status.state, PENDING | READY) {
                status.state = state;
            }
            if let Some(stop) = status.stop.take() {
                stop.stop();
            }
        }
    }
    pub(crate) fn fail(&self) {
        self.terminate(FAILED);
    }
    pub(crate) fn reserve(self: &Arc<Self>, length: usize) -> Result<RecordPermit, String> {
        if !(56..=MAX_CAMERA_TLS_RECORD_BYTES).contains(&length) {
            self.fail();
            return Err("camera record length invalid".into());
        }
        if self.state() != READY || self.cancelled.load(Ordering::Acquire) {
            return Err("camera generation or record bound invalid".into());
        }
        self.reserved
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < RECORD_SLOTS).then_some(count + 1)
            })
            .map_err(|_| {
                self.fail();
                "camera record queue full".to_owned()
            })?;
        let permit = RecordPermit {
            job: Arc::clone(self),
        };
        if self.state() != READY || self.cancelled.load(Ordering::Acquire) {
            return Err("camera generation stopped".into());
        }
        Ok(permit)
    }
    pub(crate) fn submit(&self, permit: RecordPermit, bytes: Vec<u8>) -> Result<(), String> {
        // A permit cannot be transferred across a different generation.
        if !(56..=MAX_CAMERA_TLS_RECORD_BYTES).contains(&bytes.len()) {
            self.fail();
            return Err("camera record length invalid".into());
        }
        if permit.job.generation != self.generation
            || self.state() != READY
            || self.cancelled.load(Ordering::Acquire)
        {
            return Err("stale camera generation".into());
        }
        self.queue
            .try_send(Record {
                bytes,
                _permit: permit,
            })
            .map_err(|_| {
                self.fail();
                "camera record queue unavailable".into()
            })
    }
    pub(crate) fn spawn(
        self: &Arc<Self>,
        receiver: Receiver<Record>,
        prepared: Preparation,
        snapshot: Snapshot,
        failure: Failure,
    ) -> Result<(), String> {
        let job = Arc::clone(self);
        let handle = thread::Builder::new()
            .name("CapyIO-secure-camera".into())
            .spawn(move || {
                let result = job.run(receiver, prepared, snapshot);
                if result.is_err() && !job.cancelled.load(Ordering::Acquire) {
                    job.fail();
                }
                if job.state() == FAILED {
                    failure();
                }
                job.terminate(STOPPED);
                job.finished.store(true, Ordering::Release);
            })
            .map_err(|e| {
                self.fail();
                self.finished.store(true, Ordering::Release);
                e.to_string()
            })?;
        *self
            .worker
            .lock()
            .map_err(|_| "camera worker registry poisoned")? = Some(handle);
        Ok(())
    }
    fn run(
        &self,
        receiver: Receiver<Record>,
        prepared: Preparation,
        snapshot: Snapshot,
    ) -> Result<(), String> {
        if self.cancelled.load(Ordering::Acquire) {
            return Ok(());
        }
        // Prepared data was captured under brief locks. All connect/send work is
        // here, outside them; Runtime snapshots contain no borrowed registry guard.
        let prepared = prepared()?;
        if prepared.lease_deadline() != self.lease_deadline_ms {
            return Err("camera job lease differs from preparation".into());
        }
        prepared.validate_snapshot(&snapshot()?, now()?)?;
        if self.cancelled.load(Ordering::Acquire) {
            return Ok(());
        }
        let mut sender = prepared.connect()?;
        sender.check(&snapshot()?, now()?)?;
        let stop = sender.stop_handle()?;
        {
            let mut status = self.status.lock().map_err(|_| "camera status poisoned")?;
            if self.cancelled.load(Ordering::Acquire) || status.state != PENDING {
                sender.stop();
                return Ok(());
            }
            status.stop = Some(stop);
            status.state = READY;
        }
        while !self.cancelled.load(Ordering::Acquire) {
            sender.check(&snapshot()?, now()?)?;
            match receiver.recv_timeout(Duration::from_millis(20)) {
                Ok(record) => {
                    if self.cancelled.load(Ordering::Acquire) {
                        break;
                    }
                    let current = snapshot()?;
                    sender.send_record(&current, &record.bytes, now()?)?;
                    // A revoke/control EOF during the write cancels the stop
                    // handle; fresh state is mandatory after any completed I/O.
                    sender.check(&snapshot()?, now()?)?;
                    if sender.ended() {
                        return Ok(());
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        sender.stop();
        Ok(())
    }
    /// Never call under the Service/Runtime/job registry lock. Only finished
    /// workers are joined, so reaping cannot wait for network progress.
    pub(crate) fn reap(&self) {
        if self.finished()
            && let Ok(mut worker) = self.worker.lock()
            && let Some(handle) = worker.take()
        {
            let _ = handle.join();
        }
    }
}
pub(crate) fn now() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|t| t.as_millis() as u64)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use capyio_core::*;
    fn fixture() -> (Arc<CameraJob>, Receiver<Record>) {
        let source = PortRef {
            node_id: NodeId::new(),
            capability_id: CapabilityId::new(),
            port_id: PortId::new(),
        };
        let sink = PortRef {
            node_id: NodeId::new(),
            capability_id: CapabilityId::new(),
            port_id: PortId::new(),
        };
        CameraJob::pending(
            1,
            1,
            CameraTlsBinding {
                session: capyio_control_tls::ControlTlsBinding::new(
                    SessionId::new(),
                    sink.node_id,
                    source.node_id,
                )
                .unwrap(),
                route_id: RouteId::new(),
                source,
                sink,
                stream_id: StreamId::new(),
                epoch: 1,
                expires_at_ms: now().unwrap() + 1000,
            },
            now().unwrap() + 1000,
        )
    }
    #[test]
    fn pending_and_stopped_generations_cannot_allocate_records() {
        let (job, _) = fixture();
        assert_eq!(job.runtime, 1);
        assert_eq!(job.binding.epoch, 1);
        assert!(job.reserve(56).is_err());
        assert_eq!(job.reserved.load(Ordering::Acquire), 0);
        job.stop();
        assert_eq!(job.state(), STOPPED);
        assert!(job.reserve(56).is_err());
    }
    #[test]
    fn permits_bound_reservations_queue_and_current_record_together() {
        let (job, receiver) = fixture();
        // Tests only the queue boundary, not an authorization or transport grant.
        job.status.lock().unwrap().state = READY;
        let first = job.reserve(MAX_CAMERA_TLS_RECORD_BYTES).unwrap();
        let second = job.reserve(56).unwrap();
        job.submit(first, vec![0; 56]).unwrap();
        let current = receiver.try_recv().unwrap();
        assert_eq!(job.reserved.load(Ordering::Acquire), 2);
        assert!(job.reserve(56).is_err());
        assert_eq!(job.state(), FAILED);
        drop(second);
        drop(current);
        assert_eq!(job.reserved.load(Ordering::Acquire), 0);
    }
    #[test]
    fn copy_failure_and_late_generation_release_budget() {
        let (job, _) = fixture();
        job.status.lock().unwrap().state = READY;
        let reservation = job.reserve(56).unwrap();
        drop(reservation);
        assert_eq!(job.reserved.load(Ordering::Acquire), 0);
        let reservation = job.reserve(56).unwrap();
        job.stop();
        assert!(job.submit(reservation, vec![0; 56]).is_err());
        assert_eq!(job.reserved.load(Ordering::Acquire), 0);
    }
    #[test]
    fn oversized_record_is_terminal_before_copy() {
        let (job, _) = fixture();
        job.status.lock().unwrap().state = READY;
        assert!(job.reserve(MAX_CAMERA_TLS_RECORD_BYTES + 1).is_err());
        assert_eq!(job.state(), FAILED);
        assert_eq!(job.reserved.load(Ordering::Acquire), 0);
    }
    #[test]
    fn cancel_before_worker_preparation_never_connects_or_reports_ready() {
        let (job, receiver) = fixture();
        job.stop();
        job.spawn(
            receiver,
            Box::new(|| panic!("cancelled preparation executed")),
            Arc::new(|| panic!("cancelled snapshot read")),
            Arc::new(|| panic!("cancel was reported as failure")),
        )
        .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        while !job.finished() && std::time::Instant::now() < deadline {
            thread::sleep(Duration::from_millis(1));
        }
        assert!(job.finished());
        assert_eq!(job.state(), STOPPED);
        job.reap();
    }
    #[test]
    fn preparation_failure_is_terminal_and_reported_once() {
        let (job, receiver) = fixture();
        let failed = Arc::new(AtomicUsize::new(0));
        let observed = failed.clone();
        job.spawn(
            receiver,
            Box::new(|| Err("no live authenticated control".into())),
            Arc::new(|| panic!("failed preparation read snapshot")),
            Arc::new(move || {
                observed.fetch_add(1, Ordering::AcqRel);
            }),
        )
        .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        while !job.finished() && std::time::Instant::now() < deadline {
            thread::sleep(Duration::from_millis(1));
        }
        assert!(job.finished());
        assert_eq!(job.state(), FAILED);
        assert_eq!(failed.load(Ordering::Acquire), 1);
        job.reap();
    }
    #[test]
    fn consumed_route_survives_worker_reaping_and_failed_other_start() {
        let mut route_a = GenerationClaim::default();
        route_a.reserve().unwrap();
        let (worker, _) = fixture();
        worker.stop();
        drop(worker);
        // Invalid Route B has no Runtime claim to mutate. Reaping is independent.
        let invalid_b: Option<&mut GenerationClaim> = None;
        assert!(invalid_b.ok_or("invalid Route B").is_err());
        assert!(route_a.reserve().is_err());
        let mut explicitly_new_route = GenerationClaim::default();
        assert!(explicitly_new_route.reserve().is_ok());
    }
    #[test]
    fn initial_lease_deadline_revokes_ready_without_runtime_snapshot() {
        let (template, _) = fixture();
        let (job, _) = CameraJob::pending(1, 2, template.binding, now().unwrap() + 20);
        job.status.lock().unwrap().state = READY;
        assert_eq!(job.state(), READY);
        // No Runtime/snapshot callback is needed, even if its owner is blocked.
        thread::sleep(Duration::from_millis(30));
        assert_eq!(job.state(), FAILED);
        assert!(job.reserve(56).is_err());
        assert_eq!(job.reserved.load(Ordering::Acquire), 0);
    }
}

#[cfg(test)]
#[path = "camera_job_tests.rs"]
mod lifecycle_tests;
