//! Native-only secure mono generation owner. No endpoint or credential DTO.
use super::*;
use crate::audio_job::{self, AudioJob, FAILED, STOPPED, STREAMING};
use crate::audio_sender::PreparedAudioSender;
use capyio_core::{RouteId, RouteState};
use std::sync::Arc;

static NEXT_JOB: AtomicU64 = AtomicU64::new(1);
// One global microphone worker, including cancelled workers not yet exited.
static JOB: OnceLock<Mutex<Option<Arc<AudioJob>>>> = OnceLock::new();
static OFFER: OnceLock<Mutex<Option<(u64, PreparedAudioSender)>>> = OnceLock::new();
// Bounded tombstone: late State calls remain terminal after the worker is reaped.
static TERMINAL: OnceLock<Mutex<Option<(u64, u64, i32)>>> = OnceLock::new();
fn terminal() -> &'static Mutex<Option<(u64, u64, i32)>> {
    TERMINAL.get_or_init(|| Mutex::new(None))
}
/// Called only before obtaining a Runtime/control registry, never from capture.
/// A finished-only join cannot wait for media progress. Without later host polls
/// one completed handle may remain retained; this is not a hard scheduling SLA.
pub(super) fn reap_finished(runtime: Option<u64>) {
    let job = jobs().lock().ok().and_then(|mut registry| {
        if registry
            .as_ref()
            .is_some_and(|job| runtime.is_none_or(|id| id == job.runtime) && job.worker_exited())
        {
            registry.take()
        } else {
            None
        }
    });
    if let Some(job) = job {
        job.reap(); // No registry or Runtime guard is retained across join.
        let state = job.state();
        // Retire only this preparation; a newer Route is never cancelled.
        stop_runtime(job.runtime, Some(job.binding.route_id));
        if let Ok(mut terminal) = terminal().lock() {
            *terminal = Some((job.runtime, job.generation, state));
        }
        if let Ok(mut registry) = catalog_runtimes().lock()
            && let Some(host) = registry.get_mut(&job.runtime)
        {
            if let Some(use_) = host.secure_audio_uses.get_mut(&job.binding.route_id) {
                use_.terminal = Some(state);
            }
            if host
                .runtime
                .route(job.binding.route_id)
                .is_ok_and(|r| matches!(r.state, RouteState::Starting | RouteState::Active))
            {
                if state == FAILED {
                    let _ = crate::audio_negotiation::fail(host, job.binding.route_id);
                } else {
                    let _ = host.runtime.begin_route_stop(job.binding.route_id);
                    let _ = host.runtime.stop_route(job.binding.route_id);
                    host.audio_status_pending.insert(job.binding.route_id);
                }
            }
        }
    }
}
fn jobs() -> &'static Mutex<Option<Arc<AudioJob>>> {
    JOB.get_or_init(|| Mutex::new(None))
}
fn offers() -> &'static Mutex<Option<(u64, PreparedAudioSender)>> {
    OFFER.get_or_init(|| Mutex::new(None))
}

pub(super) fn cache_offer(
    runtime: u64,
    host: &mut AndroidCatalogRuntimeV1,
    link: &mut capyio_control_link::CatalogControlLink,
    route: RouteId,
    offer: capyio_protocol::SecureAudioDataOffer,
    identity: (capyio_control_tls::ControlTlsClientIdentity, String),
    now: u64,
) -> Result<(), String> {
    // The authenticated receipt is consumed exactly once, before admission.
    let prepared = PreparedAudioSender::new(
        &host.runtime,
        link,
        route,
        offer,
        identity.0,
        identity.1,
        now,
    )?;
    let binding = prepared.binding();
    if host.secure_audio_uses.contains_key(&route) {
        return Err("secure audio Route already admitted".into());
    }
    let mut cached = offers()
        .lock()
        .map_err(|_| "audio offer registry poisoned")?;
    if cached.is_some() {
        return Err("previous secure audio preparation has not ended".into());
    }
    host.secure_audio_uses.insert(
        route,
        crate::secure_audio_negotiation::SecureAudioUse {
            binding,
            generation: audio_job::GenerationClaim::default(),
            terminal: None,
        },
    );
    *cached = Some((runtime, prepared));
    Ok(())
}
fn find(runtime: u64, generation: u64) -> Result<Arc<AudioJob>, String> {
    // Capture may never wait behind control/Start/Stop registry work.
    jobs()
        .try_lock()
        .map_err(|_| "audio registry unavailable")?
        .as_ref()
        .filter(|j| j.runtime == runtime && j.generation == generation)
        .cloned()
        .ok_or("stale audio generation".into())
}
pub(super) fn stop_runtime(runtime: u64, route: Option<RouteId>) {
    if let Ok(mut cached) = offers().lock()
        && cached.as_ref().is_some_and(|(owner, p)| {
            *owner == runtime && route.is_none_or(|id| id == p.binding().route_id)
        })
    {
        *cached = None;
    }
    let job = jobs().lock().ok().and_then(|registry| {
        registry
            .as_ref()
            .filter(|job| {
                job.runtime == runtime && route.is_none_or(|id| id == job.binding.route_id)
            })
            .cloned()
    });
    if let Some(job) = job {
        job.stop();
    }
}
fn route_job(runtime: u64, route: RouteId) -> Option<Arc<AudioJob>> {
    jobs().lock().ok().and_then(|registry| {
        registry
            .as_ref()
            .filter(|job| job.runtime == runtime && job.binding.route_id == route)
            .cloned()
    })
}
pub(super) fn failed_for_route(runtime: u64, route: RouteId) -> bool {
    route_job(runtime, route).is_some_and(|job| job.state() == FAILED)
}
pub(super) fn streaming_for_route(runtime: u64, route: RouteId) -> bool {
    route_job(runtime, route).is_some_and(|job| job.state() == STREAMING)
}
/// Runtime caller already holds its brief guard. No nested Runtime lock or I/O.
pub(super) fn reconcile(runtime: &capyio_runtime::NodeRuntime) {
    let owner = runtime.snapshot().local_node.id;
    let invalid = offers().lock().ok().and_then(|cached| {
        cached.as_ref().and_then(|(id, p)| {
            (p.binding().source.node_id == owner
                && audio_job::now().map_or(true, |n| p.validate_snapshot(runtime, n).is_err()))
            .then_some((*id, p.binding().route_id))
        })
    });
    if let Some((runtime, route)) = invalid {
        stop_runtime(runtime, Some(route));
    }
    let job = jobs().lock().ok().and_then(|j| j.clone());
    if let Some(job) = job
        && job.binding.source.node_id == owner
    {
        let valid = audio_job::now().ok().is_some_and(|now| {
            runtime.route(job.binding.route_id).is_ok_and(|r| {
                crate::audio_sender::source_authority(runtime, r, now).is_some()
                    && crate::secure_audio_negotiation::selected(r)
                    && capyio_control_tls::audio::AudioTlsBinding::from_route(
                        job.binding.session,
                        r,
                        job.binding.stream_id,
                        now,
                    )
                    .is_ok_and(|b| b == job.binding)
            })
        });
        if !valid {
            stop_runtime(job.runtime, Some(job.binding.route_id));
        }
    }
}
fn start(runtime: u64, route: RouteId) -> Result<u64, String> {
    reap_finished(None);
    {
        let registry = catalog_runtimes()
            .lock()
            .map_err(|_| "Runtime registry poisoned")?;
        reconcile(&registry.get(&runtime).ok_or("Runtime absent")?.runtime);
    }
    let existing = jobs()
        .lock()
        .map_err(|_| "audio registry poisoned")?
        .clone();
    if let Some(job) = existing {
        if job.runtime == runtime && job.binding.route_id == route {
            if matches!(job.state(), FAILED | STOPPED) {
                return Err("secure audio is one-shot; create a new Route".into());
            }
            return Ok(job.generation);
        }
        return Err("previous audio worker has not been reaped".into());
    }
    let prepared = {
        let cached = offers()
            .lock()
            .map_err(|_| "audio offer registry poisoned")?;
        cached
            .as_ref()
            .filter(|(id, p)| *id == runtime && p.binding().route_id == route)
            .map(|(_, p)| p.clone())
            .ok_or("authenticated secure audio offer absent")?
    };
    let binding = prepared.binding();
    let (job, receiver) = {
        let mut runtimes = catalog_runtimes()
            .lock()
            .map_err(|_| "Runtime registry poisoned")?;
        let host = runtimes.get_mut(&runtime).ok_or("Runtime absent")?;
        let cached = offers()
            .lock()
            .map_err(|_| "audio offer registry poisoned")?;
        if !cached
            .as_ref()
            .is_some_and(|(owner, p)| *owner == runtime && p.binding() == binding)
        {
            return Err("secure audio preparation was cancelled".into());
        }
        prepared.validate_snapshot(&host.runtime, audio_job::now()?)?;
        let mut registry = jobs().lock().map_err(|_| "audio registry poisoned")?;
        if registry.is_some() {
            return Err("previous secure audio worker has not exited".into());
        }
        let use_ = host
            .secure_audio_uses
            .get_mut(&route)
            .filter(|j| j.binding == binding)
            .ok_or("secure audio Route absent")?;
        use_.generation.reserve()?;
        let generation = NEXT_JOB
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                n.checked_add(1).filter(|n| *n <= i64::MAX as u64)
            })
            .map_err(|_| "audio generations exhausted")?;
        let (job, receiver) =
            AudioJob::pending(runtime, generation, binding, prepared.lease_deadline());
        *registry = Some(job.clone());
        (job, receiver)
    };
    let snapshot_job = job.clone();
    let snapshot = Arc::new(move || {
        let deadline = std::time::Instant::now() + Duration::from_millis(100);
        loop {
            if matches!(snapshot_job.state(), FAILED | STOPPED) {
                return Err("secure audio generation ended".into());
            }
            if std::time::Instant::now() >= deadline {
                return Err("audio Runtime snapshot timed out".into());
            }
            match catalog_runtimes().try_lock() {
                Ok(registry) => {
                    return Ok(registry
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
        if let Ok(mut registry) = catalog_runtimes().try_lock()
            && let Some(host) = registry.get_mut(&runtime)
            && host
                .runtime
                .route(route)
                .is_ok_and(|r| matches!(r.state, RouteState::Starting | RouteState::Active))
        {
            let _ = crate::audio_negotiation::fail(host, route);
        }
    });
    job.spawn(receiver, Box::new(move || Ok(prepared)), snapshot, failure)?;
    Ok(job.generation)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeCatalogRuntime_nativeAudioSecureStart(
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
                    .map_err(|_| "invalid audio Route")?,
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
                    "secure audio start failed",
                );
                0
            }
        })
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeCatalogRuntime_nativeAudioSecureState(
    mut env: EnvUnowned<'_>,
    _class: JClass<'_>,
    runtime: jlong,
    generation: jlong,
) -> jint {
    env.with_env(|_| -> jni::errors::Result<_> {
        let result = catch_unwind(AssertUnwindSafe(|| -> Result<i32, String> {
            let runtime = catalog_handle(runtime)?;
            let generation = catalog_handle(generation)?;
            if let Ok(job) = find(runtime, generation) {
                return Ok(job.state());
            }
            Ok(terminal()
                .try_lock()
                .ok()
                .and_then(|last| {
                    last.as_ref()
                        .filter(|(owner, id, _)| *owner == runtime && *id == generation)
                        .map(|(_, _, state)| *state)
                })
                .unwrap_or(STOPPED))
        }));
        Ok(match result {
            Ok(Ok(state)) => state,
            _ => STOPPED,
        })
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeCatalogRuntime_nativeAudioSecureRecord(
    mut env: EnvUnowned<'_>,
    _class: JClass<'_>,
    runtime: jlong,
    generation: jlong,
    pcm: JByteArray<'_>,
) -> jboolean {
    env.with_env(|env| -> jni::errors::Result<_> {
        let result = catch_unwind(AssertUnwindSafe(|| -> Result<(), String> {
            let job = find(catalog_handle(runtime)?, catalog_handle(generation)?)?;
            if pcm.is_null() {
                job.fail();
                return Err("null audio PCM".into());
            }
            let length = pcm.len(env).map_err(|e| {
                job.fail();
                e.to_string()
            })?;
            let permit = job.reserve(length)?;
            let mut signed = [0i8; 960];
            pcm.get_region(env, 0, &mut signed).map_err(|e| {
                job.fail();
                e.to_string()
            })?;
            job.submit(permit, signed.map(|byte| byte as u8))
        }));
        Ok(matches!(result, Ok(Ok(()))))
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeCatalogRuntime_nativeAudioSecureStop(
    mut env: EnvUnowned<'_>,
    _class: JClass<'_>,
    runtime: jlong,
    generation: jlong,
) {
    env.with_env(|_| -> jni::errors::Result<_> {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            if let (Ok(runtime), Ok(generation)) =
                (catalog_handle(runtime), catalog_handle(generation))
            {
                // Stop is a lifecycle operation, not a capture callback. It may
                // briefly synchronize with registry insertion, never with media
                // I/O, Runtime/control locks, or joining a worker. Unlike a
                // try-lookup, this cannot silently lose a concurrent Stop.
                let job = jobs().lock().ok().and_then(|registry| {
                    registry
                        .as_ref()
                        .filter(|job| job.runtime == runtime && job.generation == generation)
                        .cloned()
                });
                if let Some(job) = job {
                    job.stop();
                }
            }
        }));
        Ok(())
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}
