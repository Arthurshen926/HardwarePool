//! Service-owned handoff: control prepares a listener; the foreground Java
//! touchpad worker takes it and owns all record I/O. UI supplies no endpoints.
use super::*;
use capyio_control_tls::touchpad::{
    TouchpadTlsBinding, TouchpadTlsChannel, TouchpadTlsListener, TouchpadTlsStopHandle,
};
use capyio_core::{RouteId, RouteState, SessionState};
use capyio_protocol::TouchpadDataOffer;
use std::{
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

static NEXT_JOB: AtomicU64 = AtomicU64::new(1);
static JOBS: OnceLock<Mutex<BTreeMap<u64, Job>>> = OnceLock::new();
struct Job {
    runtime: u64,
    binding: TouchpadTlsBinding,
    listener: Option<TouchpadTlsListener>,
    stopped: Arc<AtomicBool>,
    stop: Option<TouchpadTlsStopHandle>,
}
thread_local! {
    static CHANNELS: RefCell<BTreeMap<u64, TouchpadTlsChannel>> = const { RefCell::new(BTreeMap::new()) };
}
fn jobs() -> &'static Mutex<BTreeMap<u64, Job>> {
    JOBS.get_or_init(|| Mutex::new(BTreeMap::new()))
}
fn now() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|t| t.as_millis() as u64)
        .map_err(|e| e.to_string())
}

pub(super) fn prepare(
    runtime: u64,
    binding: TouchpadTlsBinding,
    listener: TouchpadTlsListener,
) -> Result<TouchpadDataOffer, String> {
    validate(runtime, binding)?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let mut jobs = jobs()
        .lock()
        .map_err(|_| "touchpad job registry poisoned")?;
    if jobs.len() >= MAX_CATALOG_RUNTIMES || jobs.values().any(|job| job.runtime == runtime) {
        return Err("a touchpad data connection is already owned by this Node".to_owned());
    }
    let id = NEXT_JOB
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
            n.checked_add(1).filter(|n| *n <= i64::MAX as u64)
        })
        .map_err(|_| "touchpad job identities exhausted")?;
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
    Ok(TouchpadDataOffer {
        port,
        stream_id: binding.stream_id,
    })
}

fn validate(runtime: u64, binding: TouchpadTlsBinding) -> Result<(), String> {
    let mut runtimes = catalog_runtimes()
        .lock()
        .map_err(|_| "Runtime registry poisoned")?;
    let host = runtimes
        .get_mut(&runtime)
        .ok_or("touchpad Runtime is absent")?;
    let now = now()?;
    host.runtime.expire_port_leases(now);
    let route = host
        .runtime
        .route(binding.route_id)
        .map_err(|e| e.to_string())?;
    if host
        .runtime
        .session(binding.session.session_id)
        .map_err(|e| e.to_string())?
        .state
        != SessionState::Ready
        || host
            .runtime
            .route_port_lease_claim(route.id, route.source)
            .is_none_or(|lease| lease.expires_at_ms <= now)
        || TouchpadTlsBinding::from_route(binding.session, route, binding.stream_id, now)
            .map_err(|e| e.to_string())?
            != binding
    {
        return Err("touchpad Route/Session/lease is no longer authorized".to_owned());
    }
    Ok(())
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
            // A taken job keeps its cancellation reason until its Java worker
            // closes it. Untaken listeners have no worker and can be removed now.
            job.listener.is_none()
        });
    }
}

fn stop_job(id: u64) {
    let job = jobs().lock().ok().and_then(|mut jobs| jobs.remove(&id));
    if let Some(job) = job {
        job.stopped.store(true, Ordering::Release);
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

fn was_stopped(id: u64) -> bool {
    jobs()
        .lock()
        .ok()
        .and_then(|jobs| jobs.get(&id).map(|job| job.stopped.load(Ordering::Acquire)))
        .unwrap_or(false)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeTouchpadData_nativeWasStopped(
    mut env: EnvUnowned<'_>,
    _class: JClass<'_>,
    id: jlong,
) -> jboolean {
    env.with_env(|_| -> jni::errors::Result<_> {
        Ok(catch_unwind(AssertUnwindSafe(|| was_stopped(id as u64))).unwrap_or(false))
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

fn open(id: u64) -> Result<jlong, String> {
    let (runtime, binding, listener, stopped) = {
        let mut jobs = jobs()
            .lock()
            .map_err(|_| "touchpad job registry poisoned")?;
        let job = jobs.get_mut(&id).ok_or("touchpad request was cancelled")?;
        (
            job.runtime,
            job.binding,
            job.listener
                .take()
                .ok_or("touchpad connection already taken")?,
            job.stopped.clone(),
        )
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    let channel = loop {
        if stopped.load(Ordering::Acquire) || Instant::now() >= deadline {
            return Err("touchpad connection cancelled or timed out".to_owned());
        }
        validate(runtime, binding)?;
        match listener.try_accept() {
            Ok(Some(channel)) => break channel,
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(error) => return Err(error.to_string()),
        }
    };
    validate(runtime, binding)?;
    let stop = channel.stop_handle().map_err(|e| e.to_string())?;
    {
        let mut jobs = jobs()
            .lock()
            .map_err(|_| "touchpad job registry poisoned")?;
        jobs.get_mut(&id)
            .ok_or("touchpad request was cancelled")?
            .stop = Some(stop);
    }
    let packet_config = AndroidTouchpadBridgeConfigV1 {
        stream_id: binding.stream_id.to_string(),
        stream_epoch: binding.epoch,
        clock_domain_id: "android.elapsed-realtime-nanos".to_owned(),
        // Shared UI/TouchpadContactSnapshot coordinates are normalized to 10,000,
        // not the 16-bit HID range. The packet encoder performs HID scaling.
        width_px: 10_000,
        height_px: 10_000,
        width_himetric: 12_000,
        height_himetric: 7_000,
        max_contacts: 5,
        reports_pressure: true,
        first_sequence: 0,
    };
    let route_config = AndroidTouchpadRouteConfigV1 {
        route_id: binding.route_id.to_string(),
        session_id: binding.session.session_id.to_string(),
        source_node_id: binding.source.node_id.to_string(),
        source_capability_id: binding.source.capability_id.to_string(),
        source_port_id: binding.source.port_id.to_string(),
        sink_node_id: binding.sink.node_id.to_string(),
        sink_capability_id: binding.sink.capability_id.to_string(),
        sink_port_id: binding.sink.port_id.to_string(),
        authorization_expires_at_ms: Some(binding.expires_at_ms),
    };
    let session = AndroidTouchpadRecordSessionV1::new(packet_config, route_config)
        .map_err(|e| e.to_string())?;
    let handle = NEXT_RECORD_HANDLE.with(|next| {
        let handle = next.get();
        let successor = handle
            .checked_add(1)
            .filter(|n| *n <= i64::MAX as u64)
            .ok_or("record handles exhausted")?;
        next.set(successor);
        Ok::<_, String>(handle)
    })?;
    RECORD_SESSIONS.with_borrow_mut(|sessions| {
        sessions.insert(handle, session);
    });
    CHANNELS.with_borrow_mut(|channels| {
        channels.insert(id, channel);
    });
    Ok(handle as jlong)
}

fn send(id: u64, bytes: &[u8]) -> Result<(), String> {
    let (runtime, binding) = {
        let jobs = jobs()
            .lock()
            .map_err(|_| "touchpad job registry poisoned")?;
        let job = jobs.get(&id).ok_or("touchpad connection cancelled")?;
        (job.runtime, job.binding)
    };
    validate(runtime, binding)?;
    CHANNELS.with_borrow_mut(|channels| {
        let channel = channels
            .get_mut(&id)
            .ok_or("touchpad worker has no channel")?;
        channel.send_record(bytes).map_err(|e| e.to_string())?;
        if bytes.get(5) == Some(&2) {
            let ack = channel.receive_record().map_err(|e| e.to_string())?;
            let codec = capyio_remote_touchpad_adapter::PrivateTouchpadTransportCodecV1::new(
                capyio_remote_touchpad_adapter::PrivateTouchpadRouteBinding {
                    route_id: binding.route_id,
                    session_id: binding.session.session_id,
                    source: binding.source,
                    sink: binding.sink,
                    route_epoch: binding.epoch,
                    authorization_expires_at_ms: Some(binding.expires_at_ms),
                },
            );
            let sequence = u64::from_le_bytes(
                bytes
                    .get(16..24)
                    .ok_or("invalid record")?
                    .try_into()
                    .map_err(|_| "invalid sequence")?,
            );
            codec
                .validate_ack(&ack, sequence)
                .map_err(|e| e.to_string())?;
        }
        Ok::<_, String>(())
    })?;
    validate(runtime, binding)?;
    if bytes.get(5) == Some(&2) {
        let mut runtimes = catalog_runtimes()
            .lock()
            .map_err(|_| "Runtime registry poisoned")?;
        let host = runtimes.get_mut(&runtime).ok_or("Runtime removed")?;
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
    }
    Ok(())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeTouchpadData_nativePending(
    mut env: EnvUnowned<'_>,
    _class: JClass<'_>,
    runtime: jlong,
) -> jlong {
    env.with_env(|_| -> jni::errors::Result<_> {
        Ok(catch_unwind(AssertUnwindSafe(|| {
            jobs()
                .lock()
                .ok()
                .and_then(|jobs| {
                    jobs.iter()
                        .find(|(_, job)| job.runtime == runtime as u64 && job.listener.is_some())
                        .map(|(id, _)| *id as jlong)
                })
                .unwrap_or(0)
        }))
        .unwrap_or(0))
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeTouchpadData_nativeOpen(
    mut unowned: EnvUnowned<'_>,
    _class: JClass<'_>,
    id: jlong,
) -> jlong {
    unowned
        .with_env(|env| -> jni::errors::Result<_> {
            let result = catch_unwind(AssertUnwindSafe(|| open(catalog_handle(id)?)));
            Ok(match result {
                Ok(Ok(handle)) => handle,
                _ => {
                    stop_job(id as u64);
                    throw(
                        env,
                        "java/lang/IllegalStateException",
                        "secure touchpad opening failed",
                    );
                    0
                }
            })
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeTouchpadData_nativeSend(
    mut unowned: EnvUnowned<'_>,
    _class: JClass<'_>,
    id: jlong,
    record: JByteArray<'_>,
) -> jboolean {
    unowned
        .with_env(|env| -> jni::errors::Result<_> {
            let result = catch_unwind(AssertUnwindSafe(|| -> Result<(), String> {
                if record.is_null() || record.len(env).map_err(|e| e.to_string())? > 176 {
                    return Err("invalid touchpad record bound".to_owned());
                }
                let bytes = env.convert_byte_array(&record).map_err(|e| e.to_string())?;
                send(catalog_handle(id)?, &bytes)
            }));
            if matches!(result, Ok(Ok(()))) {
                Ok(true)
            } else {
                if !was_stopped(id as u64) {
                    stop_job(id as u64);
                }
                Ok(false)
            }
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeTouchpadData_nativeStop(
    mut env: EnvUnowned<'_>,
    _class: JClass<'_>,
    id: jlong,
) {
    env.with_env(|_| -> jni::errors::Result<_> {
        let _ = catch_unwind(AssertUnwindSafe(|| stop_job(id as u64)));
        Ok(())
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeTouchpadData_nativeClose(
    mut env: EnvUnowned<'_>,
    _class: JClass<'_>,
    id: jlong,
) {
    env.with_env(|_| -> jni::errors::Result<_> {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            CHANNELS.with_borrow_mut(|channels| {
                channels.remove(&(id as u64));
            });
            stop_job(id as u64);
        }));
        Ok(())
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}
