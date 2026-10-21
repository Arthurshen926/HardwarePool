//! Generation-bound JNI owner; byte reservation precedes JNI copies.
use super::*;
use crate::camera_job::{self, CameraJob, FAILED, READY, STOPPED};
use crate::camera_sender::{PreparedCameraSender, source_authority};
use capyio_core::{RouteId, RouteState};
use std::sync::Arc;

static NEXT_CAMERA_JOB: AtomicU64 = AtomicU64::new(1);
// One camera generation globally, including stopped-but-not-yet-exited workers.
static JOB: OnceLock<Mutex<Option<Arc<CameraJob>>>> = OnceLock::new();
fn jobs() -> &'static Mutex<Option<Arc<CameraJob>>> {
    JOB.get_or_init(|| Mutex::new(None))
}
static OFFER: OnceLock<Mutex<Option<(u64, PreparedCameraSender)>>> = OnceLock::new();
fn offers() -> &'static Mutex<Option<(u64, PreparedCameraSender)>> {
    OFFER.get_or_init(|| Mutex::new(None))
}
pub(super) fn cache_offer(
    runtime: u64,
    host: &capyio_runtime::NodeRuntime,
    link: &capyio_control_link::CatalogControlLink,
    route: RouteId,
    offer: capyio_protocol::SecureCameraDataOffer,
    identity: (capyio_control_tls::ControlTlsClientIdentity, String),
    now: u64,
) -> Result<(), String> {
    let prepared =
        PreparedCameraSender::new(host, link, route, offer, identity.0, identity.1, now)?;
    let mut cached = offers()
        .lock()
        .map_err(|_| "camera offer registry poisoned")?;
    if cached
        .as_ref()
        .is_some_and(|(owner, old)| *owner != runtime || old.binding != prepared.binding)
    {
        return Err("another camera offer is owned".into());
    }
    *cached = Some((runtime, prepared));
    Ok(())
}
fn find(runtime: u64, generation: u64) -> Result<Arc<CameraJob>, String> {
    jobs()
        .lock()
        .map_err(|_| "camera registry poisoned")?
        .as_ref()
        .filter(|j| j.runtime == runtime && j.generation == generation)
        .cloned()
        .ok_or("stale camera generation".into())
}
pub(super) fn stop_runtime(runtime: u64, route: Option<RouteId>) {
    if let Ok(mut cached) = offers().lock()
        && cached.as_ref().is_some_and(|(owner, p)| {
            *owner == runtime && route.is_none_or(|id| id == p.binding.route_id)
        })
    {
        *cached = None;
    }
    if let Ok(jobs) = jobs().lock()
        && let Some(job) = jobs.as_ref()
        && job.runtime == runtime
        && route.is_none_or(|id| id == job.binding.route_id)
    {
        job.stop();
    }
}
pub(super) fn failed_for_route(runtime: u64, route: RouteId) -> bool {
    jobs().lock().is_ok_and(|jobs| {
        jobs.as_ref().is_some_and(|job| {
            job.runtime == runtime && job.binding.route_id == route && job.state() == FAILED
        })
    })
}
pub(super) fn ready_for_route(runtime: u64, route: RouteId) -> bool {
    jobs().lock().is_ok_and(|jobs| {
        jobs.as_ref().is_some_and(|j| {
            j.runtime == runtime && j.binding.route_id == route && j.state() == READY
        })
    })
}
/// Caller already owns a brief Runtime guard; this never locks Runtime again.
pub(super) fn reconcile(runtime: &capyio_runtime::NodeRuntime) {
    let job = jobs().lock().ok().and_then(|jobs| jobs.clone());
    if let Some(job) = job
        && runtime.snapshot().local_node.id == job.binding.source.node_id
    {
        let valid = camera_job::now().ok().is_some_and(|now| {
            runtime.route(job.binding.route_id).is_ok_and(|route| {
                source_authority(runtime, route, now).is_some()
                    && capyio_control_tls::camera::CameraTlsBinding::from_route(
                        job.binding.session,
                        route,
                        job.binding.stream_id,
                        now,
                    )
                    .is_ok_and(|binding| binding == job.binding)
            })
        });
        if !valid {
            stop_runtime(job.runtime, Some(job.binding.route_id));
        }
    }
    // Cached preparation must also be revoked before Start has created a job.
    if let Ok(mut cached) = offers().lock()
        && let Some((_, prepared)) = cached.as_ref()
        && runtime.snapshot().local_node.id == prepared.binding.source.node_id
        && camera_job::now().map_or(true, |now| {
            prepared.validate_snapshot(runtime, now).is_err()
        })
    {
        *cached = None;
    }
}

fn start(runtime: u64, route: RouteId) -> Result<u64, String> {
    // Reap only completed work, outside every Service/Runtime/control registry lock.
    let old = {
        let mut jobs = jobs().lock().map_err(|_| "camera registry poisoned")?;
        if let Some(existing) = jobs.as_ref()
            && existing.runtime == runtime
            && existing.binding.route_id == route
        {
            if matches!(existing.state(), FAILED | STOPPED) {
                return Err(
                    "camera generation is one-shot; a new explicit Route is required".into(),
                );
            }
            return Ok(existing.generation);
        }
        if jobs.as_ref().is_some_and(|j| j.finished()) {
            jobs.take()
        } else {
            None
        }
    };
    if let Some(old) = old {
        old.reap();
    }
    let binding = {
        let mut runtimes = catalog_runtimes()
            .lock()
            .map_err(|_| "Runtime registry poisoned")?;
        let host = runtimes.get_mut(&runtime).ok_or("Runtime absent")?;
        let use_ = host
            .camera_use
            .as_ref()
            .filter(|j| j.route_id == route && j.secure_transport)
            .ok_or("secure camera route absent")?;
        let binding = use_.secure_binding.ok_or("secure camera binding absent")?;
        source_authority(
            &host.runtime,
            host.runtime.route(route).map_err(|e| e.to_string())?,
            camera_job::now()?,
        )
        .ok_or("Source authority absent")?;
        host.camera_use
            .as_mut()
            .ok_or("camera use absent")?
            .secure_generation
            .reserve()?;
        binding
    };
    let prepared = {
        let cached = offers()
            .lock()
            .map_err(|_| "camera offer registry poisoned")?;
        let (_, prepared) = cached
            .as_ref()
            .filter(|(owner, p)| *owner == runtime && p.binding == binding)
            .ok_or("current authenticated camera preparation absent")?;
        prepared.clone()
    };
    let lease_deadline = prepared.lease_deadline();
    let prepared = Box::new(move || Ok(prepared));
    let (job, receiver) = {
        let mut registry = jobs().lock().map_err(|_| "camera registry poisoned")?;
        if let Some(existing) = registry.as_ref() {
            if existing.runtime == runtime
                && existing.binding == binding
                && !matches!(existing.state(), FAILED | STOPPED)
            {
                return Ok(existing.generation);
            }
            return Err("previous camera generation has not exited".into());
        }
        let generation = NEXT_CAMERA_JOB
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                n.checked_add(1).filter(|n| *n <= i64::MAX as u64)
            })
            .map_err(|_| "camera generations exhausted")?;
        let (job, receiver) = CameraJob::pending(runtime, generation, binding, lease_deadline);
        *registry = Some(job.clone());
        (job, receiver)
    };
    let snapshot_job = job.clone();
    let snapshot = Arc::new(move || {
        let deadline = std::time::Instant::now() + Duration::from_millis(100);
        loop {
            if matches!(snapshot_job.state(), FAILED | STOPPED) {
                return Err("camera generation ended".into());
            }
            if std::time::Instant::now() >= deadline {
                return Err("camera Runtime snapshot timed out".into());
            }
            match catalog_runtimes().try_lock() {
                Ok(runtimes) => {
                    return Ok(runtimes
                        .get(&runtime)
                        .ok_or("Runtime absent")?
                        .runtime
                        .clone());
                }
                Err(std::sync::TryLockError::WouldBlock) => {
                    std::thread::sleep(Duration::from_millis(1))
                }
                Err(_) => return Err("Runtime registry poisoned".into()),
            }
        }
    });
    let failure = Arc::new(move || {
        stop_runtime(runtime, Some(route));
        if let Ok(mut runtimes) = catalog_runtimes().try_lock()
            && let Some(host) = runtimes.get_mut(&runtime)
            && host
                .runtime
                .route(route)
                .is_ok_and(|r| matches!(r.state, RouteState::Starting | RouteState::Active))
        {
            let _ = camera_use::fail(host, route);
        }
    });
    job.spawn(receiver, prepared, snapshot, failure)?;
    Ok(job.generation)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeCatalogRuntime_nativeCameraSecureStart(
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
                    .map_err(|_| "invalid camera Route")?,
            )
        }));
        Ok(match result {
            Ok(Ok(id)) => id as jlong,
            Ok(Err(e)) => {
                throw(env, "java/lang/IllegalStateException", e);
                0
            }
            Err(_) => {
                throw(
                    env,
                    "java/lang/IllegalStateException",
                    "camera start failed",
                );
                0
            }
        })
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeCatalogRuntime_nativeCameraSecureState(
    mut env: EnvUnowned<'_>,
    _class: JClass<'_>,
    runtime: jlong,
    generation: jlong,
) -> jint {
    env.with_env(|_env| -> jni::errors::Result<_> {
        let state = (|| -> Result<i32, String> {
            let runtime = catalog_handle(runtime)?;
            let job = find(runtime, catalog_handle(generation)?)?;
            if let Ok(runtimes) = catalog_runtimes().try_lock() {
                if let Some(host) = runtimes.get(&runtime) {
                    reconcile(&host.runtime);
                } else {
                    job.stop();
                }
            }
            Ok(job.state())
        })();
        Ok(state.unwrap_or(STOPPED))
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeCatalogRuntime_nativeCameraSecureRecord(
    mut env: EnvUnowned<'_>,
    _class: JClass<'_>,
    runtime: jlong,
    generation: jlong,
    record: JByteArray<'_>,
) -> jboolean {
    env.with_env(|env| -> jni::errors::Result<_> {
        let result = catch_unwind(AssertUnwindSafe(|| -> Result<(), String> {
            let job = find(catalog_handle(runtime)?, catalog_handle(generation)?)?;
            if record.is_null() {
                job.fail();
                return Err("null camera record".into());
            }
            let length = record.len(env).map_err(|e| e.to_string())?;
            let permit = job.reserve(length)?; // No JNI allocation before reservation.
            let bytes = env.convert_byte_array(&record).map_err(|e| {
                job.fail();
                e.to_string()
            })?;
            job.submit(permit, bytes)
        }));
        Ok(matches!(result, Ok(Ok(()))))
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeCatalogRuntime_nativeCameraSecureStop(
    mut env: EnvUnowned<'_>,
    _class: JClass<'_>,
    runtime: jlong,
    generation: jlong,
) {
    env.with_env(|_env| -> jni::errors::Result<_> {
        if let (Ok(runtime), Ok(generation)) = (catalog_handle(runtime), catalog_handle(generation))
            && let Ok(job) = find(runtime, generation)
        {
            stop_runtime(runtime, Some(job.binding.route_id));
        }
        Ok(())
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}
