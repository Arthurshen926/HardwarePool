//! Service-owned actuator worker. The control thread owns grants/listeners;
//! one Java worker owns record I/O and acknowledges only after Sink execution.
use super::*;
use capyio_control_tls::haptics::{
    HapticsRecord, HapticsSequence, HapticsTlsBinding, HapticsTlsChannel, HapticsTlsListener,
    HapticsTlsStopHandle,
};
use capyio_core::{RouteId, RouteState, SessionState};
use std::sync::{Arc, atomic::AtomicBool};

static JOBS: OnceLock<Mutex<BTreeMap<u64, Job>>> = OnceLock::new();
static NEXT: AtomicU64 = AtomicU64::new(1);
struct Job {
    runtime: u64,
    binding: HapticsTlsBinding,
    listener: Option<HapticsTlsListener>,
    stopped: Arc<AtomicBool>,
    stop: Option<HapticsTlsStopHandle>,
}
struct Channel {
    io: HapticsTlsChannel,
    sequence: HapticsSequence,
    pending: Option<u64>,
}
thread_local! { static CHANNELS: RefCell<BTreeMap<u64, Channel>> = const { RefCell::new(BTreeMap::new()) }; }
fn jobs() -> &'static Mutex<BTreeMap<u64, Job>> {
    JOBS.get_or_init(|| Mutex::new(BTreeMap::new()))
}
fn now() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|t| t.as_millis() as u64)
        .map_err(|e| e.to_string())
}

fn validate(runtime: u64, binding: HapticsTlsBinding) -> Result<(), String> {
    let mut runtimes = catalog_runtimes()
        .lock()
        .map_err(|_| "Runtime registry poisoned")?;
    let host = runtimes.get_mut(&runtime).ok_or("haptics Runtime absent")?;
    let now = now()?;
    host.runtime.expire_port_leases(now);
    let route = host
        .runtime
        .route(binding.route_id)
        .map_err(|e| e.to_string())?;
    if host
        .runtime
        .session(route.session_id)
        .map_err(|e| e.to_string())?
        .state
        != SessionState::Ready
        || host
            .runtime
            .route_port_lease_claim(route.id, route.sink)
            .is_none_or(|lease| lease.expires_at_ms <= now)
        || HapticsTlsBinding::from_route(binding.session, route, binding.stream_id, now)
            .map_err(|e| e.to_string())?
            != binding
    {
        return Err("haptics Route/Session/Sink lease is no longer authorized".into());
    }
    Ok(())
}

pub(super) fn prepare(
    runtime: u64,
    binding: HapticsTlsBinding,
    listener: HapticsTlsListener,
) -> Result<capyio_protocol::HapticsDataOffer, String> {
    validate(runtime, binding)?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let mut jobs = jobs().lock().map_err(|_| "haptics registry poisoned")?;
    if jobs.len() >= MAX_CATALOG_RUNTIMES || jobs.values().any(|job| job.runtime == runtime) {
        return Err("vibrator already has a data worker".into());
    }
    let id = NEXT
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| {
            id.checked_add(1).filter(|next| *next <= i64::MAX as u64)
        })
        .map_err(|_| "haptics handles exhausted")?;
    jobs.insert(
        id,
        Job {
            runtime,
            binding,
            listener: Some(listener),
            stopped: Arc::new(AtomicBool::new(false)),
            stop: None,
        },
    );
    Ok(capyio_protocol::HapticsDataOffer {
        port,
        stream_id: binding.stream_id,
    })
}

pub(super) fn stop_runtime(runtime: u64, route: Option<RouteId>) {
    if let Ok(mut jobs) = jobs().lock() {
        jobs.retain(|_, job| {
            if job.runtime != runtime || route.is_some_and(|id| id != job.binding.route_id) {
                return true;
            }
            job.stopped.store(true, Ordering::Release);
            if let Some(stop) = &job.stop {
                stop.stop();
            }
            job.listener.is_none()
        });
    }
}

fn metadata(id: u64) -> Result<(u64, HapticsTlsBinding), String> {
    let jobs = jobs().lock().map_err(|_| "haptics registry poisoned")?;
    let job = jobs.get(&id).ok_or("haptics worker absent")?;
    if job.stopped.load(Ordering::Acquire) {
        return Err("haptics stopped".into());
    }
    Ok((job.runtime, job.binding))
}
fn close(id: u64) {
    CHANNELS.with_borrow_mut(|channels| {
        channels.remove(&id);
    });
    let job = jobs().lock().ok().and_then(|mut jobs| jobs.remove(&id));
    if let Some(job) = job {
        if let Some(stop) = job.stop {
            stop.stop();
        }
        if let Ok(mut runtimes) = catalog_runtimes().lock()
            && let Some(host) = runtimes.get_mut(&job.runtime)
        {
            let _ = host.runtime.set_route_active(
                job.binding.route_id,
                false,
                now().unwrap_or(u64::MAX),
            );
        }
    }
}
fn open(id: u64) -> Result<(), String> {
    let (runtime, binding) = metadata(id)?;
    let listener = jobs()
        .lock()
        .map_err(|_| "haptics registry poisoned")?
        .get_mut(&id)
        .and_then(|job| job.listener.take())
        .ok_or("haptics listener already taken")?;
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        metadata(id)?;
        validate(runtime, binding)?;
        if std::time::Instant::now() >= deadline {
            return Err("haptics accept timed out".into());
        }
        if let Some(channel) = listener.try_accept().map_err(|e| e.to_string())? {
            let stop = channel.stop_handle().map_err(|e| e.to_string())?;
            jobs()
                .lock()
                .map_err(|_| "haptics registry poisoned")?
                .get_mut(&id)
                .ok_or("haptics stopped")?
                .stop = Some(stop);
            CHANNELS.with_borrow_mut(|channels| {
                channels.insert(
                    id,
                    Channel {
                        io: channel,
                        sequence: HapticsSequence::default(),
                        pending: None,
                    },
                );
            });
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn receive(id: u64) -> Result<Vec<u8>, String> {
    let (runtime, binding) = metadata(id)?;
    validate(runtime, binding)?;
    let bytes = CHANNELS.with_borrow_mut(|channels| -> Result<Vec<u8>, String> {
        let channel = channels.get_mut(&id).ok_or("haptics channel absent")?;
        if channel.pending.is_some() {
            return Err("previous haptics command not acknowledged".into());
        }
        let bytes = channel.io.receive_record().map_err(|e| e.to_string())?;
        let record = HapticsRecord::decode(&bytes).map_err(|e| e.to_string())?;
        channel.sequence.accept(record).map_err(|e| e.to_string())?;
        channel.pending = Some(record.sequence());
        Ok(bytes)
    })?;
    metadata(id)?;
    validate(runtime, binding)?;
    Ok(bytes)
}
fn acknowledge(id: u64) -> Result<(), String> {
    let (runtime, binding) = metadata(id)?;
    validate(runtime, binding)?;
    CHANNELS.with_borrow_mut(|channels| -> Result<(), String> {
        let channel = channels.get_mut(&id).ok_or("haptics channel absent")?;
        let sequence = channel
            .pending
            .take()
            .ok_or("no executed haptics command")?;
        channel
            .io
            .send_record(
                &HapticsRecord::Ack { sequence }
                    .encode()
                    .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())
    })?;
    let mut runtimes = catalog_runtimes()
        .lock()
        .map_err(|_| "Runtime registry poisoned")?;
    let host = runtimes.get_mut(&runtime).ok_or("haptics Runtime absent")?;
    if host
        .runtime
        .route(binding.route_id)
        .map_err(|e| e.to_string())?
        .state
        == RouteState::Starting
    {
        host.runtime
            .activate_route(binding.route_id)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeHapticsData_nativePending(
    mut env: EnvUnowned<'_>,
    _: JClass<'_>,
    runtime: jlong,
) -> jlong {
    env.with_env(|_| -> jni::errors::Result<_> {
        Ok(jobs()
            .lock()
            .ok()
            .and_then(|jobs| {
                jobs.iter()
                    .find(|(_, job)| {
                        job.runtime == runtime as u64
                            && job.listener.is_some()
                            && !job.stopped.load(Ordering::Acquire)
                    })
                    .map(|(id, _)| *id as jlong)
            })
            .unwrap_or(0))
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeHapticsData_nativeOpen(
    mut env: EnvUnowned<'_>,
    _: JClass<'_>,
    id: jlong,
) -> jboolean {
    env.with_env(|_| -> jni::errors::Result<_> { Ok(open(id as u64).is_ok()) })
        .resolve::<ThrowRuntimeExAndDefault>()
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeHapticsData_nativeReceive(
    mut env: EnvUnowned<'_>,
    _: JClass<'_>,
    id: jlong,
) -> jbyteArray {
    env.with_env(|env| -> jni::errors::Result<_> {
        Ok(match receive(id as u64) {
            Ok(bytes) => byte_array(env, &bytes),
            Err(_) => ptr::null_mut(),
        })
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeHapticsData_nativeAcknowledge(
    mut env: EnvUnowned<'_>,
    _: JClass<'_>,
    id: jlong,
) -> jboolean {
    env.with_env(|_| -> jni::errors::Result<_> { Ok(acknowledge(id as u64).is_ok()) })
        .resolve::<ThrowRuntimeExAndDefault>()
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeHapticsData_nativeClose(
    mut env: EnvUnowned<'_>,
    _: JClass<'_>,
    id: jlong,
) {
    env.with_env(|_| -> jni::errors::Result<_> {
        close(id as u64);
        Ok(())
    })
    .resolve::<ThrowRuntimeExAndDefault>();
}
