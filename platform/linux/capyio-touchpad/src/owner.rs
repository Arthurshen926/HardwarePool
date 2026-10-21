//! Authenticated Linux touchpad owner. Only worker threads perform TLS/helper
//! I/O. A separate watchdog enforces revocation, finite lifetime and idle even
//! when a peer stalls inside a TLS record or the caller stops polling.
use crate::session::{TouchpadTransportStop, wall_ms};
use crate::{LinuxTouchpadSinkFactory, TouchpadEventTransport};
use capyio_control_tls::touchpad::{TouchpadTlsBinding, TouchpadTlsChannel, TouchpadTlsStopHandle};
use capyio_core::{
    Availability, FormatDescriptor, Platform, PortDirection, Problem, ProblemCategory, ProblemId,
    ProblemSeverity, QosMode, RouteState, SessionState,
};
use capyio_remote_touchpad_adapter::secure_receiver::{
    SecureTouchpadReceiver, TouchpadSinkAdmission, touchpad_tls_descriptor, touchpad_tls_stream,
};
use capyio_runtime::NodeRuntime;
use std::{
    io,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
fn error(value: impl ToString) -> io::Error {
    io::Error::other(value.to_string())
}
const IDLE: Duration = Duration::from_secs(2);

fn lock_runtime(
    runtime: &Mutex<NodeRuntime>,
) -> io::Result<std::sync::MutexGuard<'_, NodeRuntime>> {
    let until = Instant::now() + Duration::from_millis(100);
    loop {
        match runtime.try_lock() {
            Ok(runtime) => return Ok(runtime),
            Err(std::sync::TryLockError::Poisoned(_)) => {
                return Err(error("touchpad Runtime poisoned"));
            }
            Err(std::sync::TryLockError::WouldBlock) if Instant::now() < until => {
                thread::sleep(Duration::from_millis(1))
            }
            Err(_) => return Err(error("touchpad Runtime lock deadline")),
        }
    }
}

type HelperStop = Arc<Mutex<Option<Arc<dyn TouchpadTransportStop>>>>;
fn admission(
    runtime: &Mutex<NodeRuntime>,
    binding: TouchpadTlsBinding,
    deadline_ms: u64,
    wait_for_runtime: bool,
) -> io::Result<TouchpadSinkAdmission> {
    // Contention cannot suspend a security watchdog indefinitely. A short
    // conflicting control update may terminate this epoch; it never grants it.
    let runtime = if wait_for_runtime {
        lock_runtime(runtime)?
    } else {
        runtime.try_lock().map_err(|e| match e {
            std::sync::TryLockError::WouldBlock => {
                io::Error::new(io::ErrorKind::WouldBlock, "touchpad Runtime busy")
            }
            std::sync::TryLockError::Poisoned(_) => error("touchpad Runtime poisoned"),
        })?
    };
    admission_locked(&runtime, binding, deadline_ms)
}
fn admission_locked(
    runtime: &NodeRuntime,
    binding: TouchpadTlsBinding,
    deadline_ms: u64,
) -> io::Result<TouchpadSinkAdmission> {
    let now = wall_ms()?;
    if now >= deadline_ms {
        return Err(error("touchpad owner deadline expired"));
    }
    let route = runtime.route(binding.route_id).map_err(error)?;
    let session = runtime.session(binding.session.session_id).map_err(error)?;
    let snapshot = runtime.snapshot();
    if snapshot.local_node.platform != Platform::Linux
        || snapshot.local_node.id != binding.sink.node_id
        || session.state != SessionState::Ready
        || session.local_node_id != binding.sink.node_id
        || session.remote_node_id != binding.source.node_id
        || TouchpadTlsBinding::from_route(binding.session, route, binding.stream_id, now)
            .map_err(error)?
            != binding
        || route.selected_format != Some(FormatDescriptor::new("touchpad-frame-v1"))
        || route.selected_qos_mode != Some(QosMode::Interactive)
    {
        return Err(error(
            "touchpad Runtime Session/Route/format binding mismatch",
        ));
    }
    for (node, endpoint, direction) in [
        (&snapshot.local_node, binding.sink, PortDirection::Sink),
        (
            snapshot
                .peers
                .iter()
                .find(|peer| peer.id == binding.source.node_id)
                .ok_or_else(|| error("touchpad peer absent"))?,
            binding.source,
            PortDirection::Source,
        ),
    ] {
        let capability = node
            .capabilities
            .get(&endpoint.capability_id)
            .ok_or_else(|| error("touchpad capability absent"))?;
        let port = capability
            .ports
            .get(&endpoint.port_id)
            .ok_or_else(|| error("touchpad port absent"))?;
        if capability.availability != Availability::Available
            || port.availability != Availability::Available
            || port.direction != direction
            || port.profile != route.profile
            || !port
                .formats
                .contains(&FormatDescriptor::new("touchpad-frame-v1"))
        {
            return Err(error("touchpad endpoint unavailable or incompatible"));
        }
    }
    let lease = runtime
        .route_port_lease_claim(route.id, binding.sink)
        .ok_or_else(|| error("touchpad Sink lease absent"))?;
    if lease.session_id != binding.session.session_id
        || lease.expires_at_ms <= now
        || lease.expires_at_ms > binding.expires_at_ms
    {
        return Err(error("touchpad Sink lease mismatch or expiry"));
    }
    Ok(TouchpadSinkAdmission {
        route: route.clone(),
        local_lease_expires_at_ms: lease.expires_at_ms,
        session_ready: true,
    })
}
fn stop_all(stop: &TouchpadTlsStopHandle, helper: &HelperStop) {
    stop.stop();
    if let Ok(helper) = helper.lock()
        && let Some(helper) = helper.as_ref()
    {
        helper.stop();
    }
}

pub struct RuntimeTouchpadWorker {
    stop: Arc<TouchpadTlsStopHandle>,
    helper: HelperStop,
    cancelled: Arc<AtomicBool>,
    done: Arc<AtomicBool>,
    worker: Option<JoinHandle<io::Result<()>>>,
    watchdog: Option<JoinHandle<()>>,
}
impl RuntimeTouchpadWorker {
    /// The supplied channel is already pinned mTLS. No listener, permission
    /// changes or device probes are performed. The injected opener is also the
    /// software-test boundary; native construction uses exactly this owner.
    pub fn start_with_transport<T, O>(
        runtime: Arc<Mutex<NodeRuntime>>,
        mut channel: TouchpadTlsChannel,
        opener: O,
    ) -> io::Result<Self>
    where
        T: TouchpadEventTransport + Send + 'static,
        O: FnOnce(u64) -> io::Result<T> + Send + 'static,
    {
        let binding = channel.binding();
        let now = wall_ms()?;
        let first = admission(&runtime, binding, binding.expires_at_ms, true)?;
        if first.route.state != RouteState::Starting {
            return Err(error("touchpad requires Starting Route"));
        }
        let deadline_ms = first
            .local_lease_expires_at_ms
            .min(binding.expires_at_ms)
            .min(now.saturating_add(300_000));
        let lifetime = Duration::from_millis(
            deadline_ms
                .checked_sub(now)
                .filter(|v| *v > 0)
                .ok_or_else(|| error("touchpad deadline expired"))?,
        );
        let started = Instant::now();
        channel.restrict_deadline(started + lifetime);
        let stop = Arc::new(channel.stop_handle().map_err(error)?);
        let helper: HelperStop = Arc::new(Mutex::new(None));
        let cancelled = Arc::new(AtomicBool::new(false));
        let done = Arc::new(AtomicBool::new(false));
        let last_frame = Arc::new(AtomicU64::new(0));
        let thread_runtime = Arc::clone(&runtime);
        let thread_helper = Arc::clone(&helper);
        let thread_cancelled = Arc::clone(&cancelled);
        let thread_done = Arc::clone(&done);
        let thread_last = Arc::clone(&last_frame);
        let worker = thread::Builder::new().name("capyio-linux-touchpad-tls".into()).spawn(move || {
            let run = || -> io::Result<()> {
                let open_runtime = Arc::clone(&thread_runtime);
                let open_cancelled = Arc::clone(&thread_cancelled);
                let factory = LinuxTouchpadSinkFactory::new(touchpad_tls_stream(binding), touchpad_tls_descriptor(), 0, deadline_ms, move |deadline| {
                    admission(&open_runtime, binding, deadline, true)?;
                    if open_cancelled.load(Ordering::Acquire) { return Err(error("touchpad cancelled before native open")); }
                    let mut transport = opener(deadline)?;
                    let setup = (|| {
                        let handle = transport.stop_handle()?;
                        *thread_helper.lock().map_err(|_| error("touchpad stop state poisoned"))? = handle;
                        admission(&open_runtime, binding, deadline, true)?;
                        if open_cancelled.load(Ordering::Acquire) { return Err(error("touchpad cancelled during native open")); }
                        Ok(())
                    })();
                    if let Err(e) = setup { let _ = transport.close(); return Err(e); }
                    Ok(transport)
                })?;
                let current = || admission(&thread_runtime, binding, deadline_ms, true).map_err(|e| e.to_string());
                let mut receiver = SecureTouchpadReceiver::accept(channel, factory, current).map_err(error)?;
                let mut active = false;
                loop {
                    if thread_cancelled.load(Ordering::Acquire) { return Err(error("touchpad cancelled")); }
                    if !receiver.receive_next(current).map_err(error)? { break; }
                    thread_last.store(started.elapsed().as_millis() as u64, Ordering::Release);
                    if !active {
                        let mut runtime = lock_runtime(&thread_runtime)?;
                        // Validate while holding the very guard used to mutate:
                        // the wait for this lock may cross revoke or expiry.
                        admission_locked(&runtime, binding, deadline_ms)?;
                        if thread_cancelled.load(Ordering::Acquire) { return Err(error("touchpad cancelled before activation")); }
                        runtime.activate_route(binding.route_id).map_err(error)?;
                        active = true;
                    }
                }
                receiver.close().map_err(error)
            };
            let mut result = run();
            let reconciled = (|| -> io::Result<()> {
                let mut runtime = lock_runtime(&thread_runtime)?;
                if !runtime.route(binding.route_id).is_ok_and(|route| route.session_id == binding.session.session_id && route.epoch == binding.epoch && route.sink == binding.sink) { return Ok(()); }
                if let Err(reason) = &result {
                    runtime.report_route_offline(binding.route_id, Problem {
                        id: ProblemId::new(), code: "CAPY.LINUX_TOUCHPAD.STOPPED".into(),
                        category: ProblemCategory::Transport, severity: ProblemSeverity::Error,
                        retryable: true, related_node: Some(binding.source.node_id),
                        related_adapter: None, related_route: Some(binding.route_id),
                        human_message: "Linux touchpad input stopped; a fresh authorized epoch is required".into(),
                        technical_detail: Some(reason.to_string().chars().take(1000).collect()),
                    }).map_err(error)?;
                } else {
                    runtime.set_route_active(binding.route_id, false, wall_ms().unwrap_or(u64::MAX)).map_err(error)?;
                }
                Ok(())
            })();
            if let Err(cleanup) = reconciled {
                result = Err(error(format!("{}; Runtime reconciliation: {cleanup}", result.err().map_or_else(|| "touchpad device closed".into(), |e| e.to_string()))));
            }
            thread_done.store(true, Ordering::Release);
            result
        }).map_err(error)?;
        let watch_stop = Arc::clone(&stop);
        let watch_helper = Arc::clone(&helper);
        let watch_cancelled = Arc::clone(&cancelled);
        let watch_done = Arc::clone(&done);
        let watchdog = thread::Builder::new()
            .name("capyio-touchpad-watchdog".into())
            .spawn(move || {
                let mut previous = now;
                let mut busy_since: Option<Instant> = None;
                while !watch_done.load(Ordering::Acquire)
                    && !watch_cancelled.load(Ordering::Acquire)
                {
                    let elapsed = started.elapsed();
                    let last = Duration::from_millis(last_frame.load(Ordering::Acquire));
                    let wall = wall_ms();
                    let invalid_clock = wall.as_ref().map_or(true, |value| *value < previous);
                    if let Ok(value) = wall {
                        previous = value;
                    }
                    let authority_failed = match admission(&runtime, binding, deadline_ms, false) {
                        Ok(_) => {
                            busy_since = None;
                            false
                        }
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                            busy_since.get_or_insert_with(Instant::now).elapsed()
                                >= Duration::from_millis(100)
                        }
                        Err(_) => true,
                    };
                    if invalid_clock
                        || elapsed >= lifetime
                        || elapsed.saturating_sub(last) >= IDLE
                        || authority_failed
                    {
                        watch_cancelled.store(true, Ordering::Release);
                        stop_all(&watch_stop, &watch_helper);
                        break;
                    }
                    thread::sleep(Duration::from_millis(10));
                }
            });
        let mut owned = Self {
            stop,
            helper,
            cancelled,
            done,
            worker: Some(worker),
            watchdog: None,
        };
        match watchdog {
            Ok(watchdog) => owned.watchdog = Some(watchdog),
            Err(e) => {
                let _ = owned.stop();
                return Err(e);
            }
        }
        Ok(owned)
    }
    #[cfg(all(target_os = "linux", feature = "native-uinput"))]
    pub fn start(
        runtime: Arc<Mutex<NodeRuntime>>,
        channel: TouchpadTlsChannel,
    ) -> io::Result<Self> {
        Self::start_with_transport(
            runtime,
            channel,
            crate::transport::OwnedTouchpadHelper::spawn,
        )
    }
    /// Nonblocking: no native or network I/O, no joins until already finished.
    pub fn poll(&mut self) -> io::Result<bool> {
        if self
            .worker
            .as_ref()
            .is_some_and(|worker| !worker.is_finished())
        {
            return Ok(true);
        }
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| error("touchpad worker panicked"))??;
        }
        if self
            .watchdog
            .as_ref()
            .is_some_and(|watchdog| watchdog.is_finished())
        {
            let _ = self.watchdog.take().unwrap().join();
        }
        Ok(false)
    }
    /// Signal both I/O channels before any join. Bounded cleanup reports a
    /// timeout rather than blocking an owner indefinitely.
    pub fn stop(&mut self) -> io::Result<()> {
        self.cancelled.store(true, Ordering::Release);
        stop_all(&self.stop, &self.helper);
        let deadline = Instant::now() + Duration::from_secs(2);
        while self
            .worker
            .as_ref()
            .is_some_and(|worker| !worker.is_finished())
            || self
                .watchdog
                .as_ref()
                .is_some_and(|watchdog| !watchdog.is_finished())
        {
            if Instant::now() >= deadline {
                return Err(error("touchpad worker shutdown deadline"));
            }
            thread::sleep(Duration::from_millis(1));
        }
        // Explicit stop can interrupt a record and therefore produce a worker
        // transport error; still return it instead of claiming release success.
        self.poll().map(|_| ())
    }
    pub fn is_finished(&self) -> bool {
        self.done.load(Ordering::Acquire)
    }
}
impl Drop for RuntimeTouchpadWorker {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
