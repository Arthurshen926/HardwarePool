use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

use jni::{
    Env, EnvUnowned,
    errors::ThrowRuntimeExAndDefault,
    objects::{JClass, JFloatArray, JIntArray, JString},
    strings::JNIString,
    sys::{jboolean, jbyteArray, jint, jlong},
};

use crate::{
    AndroidCatalogRuntimeV1, AndroidMotionDtoV1, AndroidTouchpadBridgeConfigV1,
    AndroidTouchpadPacketSessionV1, AndroidTouchpadRecordSessionV1, AndroidTouchpadRouteConfigV1,
};

static NEXT_CATALOG_HANDLE: AtomicU64 = AtomicU64::new(1);
static CATALOG_RUNTIMES: OnceLock<Mutex<BTreeMap<u64, AndroidCatalogRuntimeV1>>> = OnceLock::new();
const MAX_CATALOG_RUNTIMES: usize = 4;

fn catalog_runtimes() -> &'static Mutex<BTreeMap<u64, AndroidCatalogRuntimeV1>> {
    CATALOG_RUNTIMES.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn catalog_handle(value: jlong) -> Result<u64, String> {
    let handle = positive_u64(value, "catalog Runtime handle")?;
    if handle == 0 {
        return Err("catalog Runtime handle must be positive".to_owned());
    }
    Ok(handle)
}

fn allocate_catalog_handle() -> Result<u64, String> {
    NEXT_CATALOG_HANDLE
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |handle| {
            handle
                .checked_add(1)
                .filter(|next| *next <= i64::MAX as u64)
        })
        .map_err(|_| "native catalog Runtime handle space exhausted".to_owned())
}

thread_local! {
    static NEXT_HANDLE: Cell<u64> = const { Cell::new(1) };
    static SESSIONS: RefCell<BTreeMap<u64, AndroidTouchpadPacketSessionV1>> =
        const { RefCell::new(BTreeMap::new()) };
    static NEXT_RECORD_HANDLE: Cell<u64> = const { Cell::new(1) };
    static RECORD_SESSIONS: RefCell<BTreeMap<u64, AndroidTouchpadRecordSessionV1>> =
        const { RefCell::new(BTreeMap::new()) };
}

fn with_record_session(
    env: &mut Env<'_>,
    handle: jlong,
    operation: impl FnOnce(
        &mut AndroidTouchpadRecordSessionV1,
    ) -> Result<
        Option<capyio_remote_touchpad_adapter::PrivateTouchpadTransportRecordV1>,
        String,
    >,
) -> jbyteArray {
    let handle = match positive_u64(handle, "handle") {
        Ok(handle) if handle != 0 => handle,
        _ => {
            throw(
                env,
                "java/lang/IllegalArgumentException",
                "invalid record session handle",
            );
            return ptr::null_mut();
        }
    };
    match catch_unwind(AssertUnwindSafe(|| {
        RECORD_SESSIONS.with_borrow_mut(|sessions| {
            let session = sessions
                .get_mut(&handle)
                .ok_or_else(|| "unknown record session handle".to_owned())?;
            operation(session)
        })
    })) {
        Ok(Ok(Some(record))) => byte_array(env, record.as_bytes()),
        Ok(Ok(None)) => ptr::null_mut(),
        Ok(Err(error)) => {
            throw(env, "java/lang/IllegalStateException", error);
            ptr::null_mut()
        }
        Err(_) => {
            throw(
                env,
                "java/lang/IllegalStateException",
                "native record bridge panicked",
            );
            ptr::null_mut()
        }
    }
}

fn throw(env: &mut Env<'_>, class: &str, message: impl ToString) {
    let _ = env.throw_new(JNIString::new(class), JNIString::new(message.to_string()));
}

fn string(_env: &mut Env<'_>, value: JString<'_>) -> Result<String, String> {
    if value.is_null() {
        Err("string argument must not be null".to_owned())
    } else {
        Ok(value.to_string())
    }
}

fn positive_u64(value: jlong, label: &str) -> Result<u64, String> {
    u64::try_from(value).map_err(|_| format!("{label} must fit inside a non-negative signed long"))
}

fn positive_u32(value: jint, label: &str) -> Result<u32, String> {
    u32::try_from(value).map_err(|_| format!("{label} must be non-negative"))
}

fn int_array(env: &mut Env<'_>, value: &JIntArray<'_>) -> Result<Vec<i32>, String> {
    let len = value.len(env).map_err(|error| error.to_string())?;
    let mut values = vec![0; len];
    value
        .get_region(env, 0, &mut values)
        .map_err(|error| error.to_string())?;
    Ok(values)
}

fn float_array(env: &mut Env<'_>, value: &JFloatArray<'_>) -> Result<Vec<f32>, String> {
    let len = value.len(env).map_err(|error| error.to_string())?;
    let mut values = vec![0.0; len];
    value
        .get_region(env, 0, &mut values)
        .map_err(|error| error.to_string())?;
    Ok(values)
}

fn byte_array(env: &mut Env<'_>, bytes: &[u8]) -> jbyteArray {
    match env.byte_array_from_slice(bytes) {
        Ok(array) => array.into_raw(),
        Err(error) => {
            throw(env, "java/lang/IllegalStateException", error);
            ptr::null_mut()
        }
    }
}

fn with_session_packet(
    env: &mut Env<'_>,
    handle: jlong,
    operation: impl FnOnce(
        &mut AndroidTouchpadPacketSessionV1,
    ) -> Result<
        Option<capyio_remote_touchpad_adapter::PrivateTouchpadPacketV1>,
        String,
    >,
) -> jbyteArray {
    let handle = match positive_u64(handle, "handle") {
        Ok(handle) if handle != 0 => handle,
        _ => {
            throw(
                env,
                "java/lang/IllegalArgumentException",
                "invalid session handle",
            );
            return ptr::null_mut();
        }
    };
    match catch_unwind(AssertUnwindSafe(|| {
        SESSIONS.with_borrow_mut(|sessions| {
            let session = sessions
                .get_mut(&handle)
                .ok_or_else(|| "unknown session handle".to_owned())?;
            operation(session)
        })
    })) {
        Ok(Ok(Some(packet))) => byte_array(env, packet.as_bytes()),
        Ok(Ok(None)) => ptr::null_mut(),
        Ok(Err(error)) => {
            throw(env, "java/lang/IllegalStateException", error);
            ptr::null_mut()
        }
        Err(_) => {
            throw(
                env,
                "java/lang/IllegalStateException",
                "native touchpad bridge panicked",
            );
            ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeCatalogRuntime_nativeCreate(
    mut unowned_env: EnvUnowned<'_>,
    _class: JClass<'_>,
    catalog_json: JString<'_>,
) -> jlong {
    unowned_env
        .with_env(|env| -> jni::errors::Result<_> {
            let result = catch_unwind(AssertUnwindSafe(|| -> Result<jlong, String> {
                let runtime = AndroidCatalogRuntimeV1::new(&string(env, catalog_json)?)
                    .map_err(|error| error.to_string())?;
                let handle = allocate_catalog_handle()?;
                let mut runtimes = catalog_runtimes()
                    .lock()
                    .map_err(|_| "native catalog Runtime registry is poisoned".to_owned())?;
                if runtimes.len() >= MAX_CATALOG_RUNTIMES {
                    return Err("native catalog Runtime registry is full".to_owned());
                }
                runtimes.insert(handle, runtime);
                i64::try_from(handle)
                    .map_err(|_| "native catalog Runtime handle exceeds signed long".to_owned())
            }));
            Ok(match result {
                Ok(Ok(handle)) => handle,
                Ok(Err(error)) => {
                    throw(env, "java/lang/IllegalArgumentException", error);
                    0
                }
                Err(_) => {
                    throw(
                        env,
                        "java/lang/IllegalStateException",
                        "native catalog Runtime bridge panicked",
                    );
                    0
                }
            })
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeCatalogRuntime_nativeReplace(
    mut unowned_env: EnvUnowned<'_>,
    _class: JClass<'_>,
    handle: jlong,
    catalog_json: JString<'_>,
) -> jboolean {
    unowned_env
        .with_env(|env| -> jni::errors::Result<_> {
            let result = catch_unwind(AssertUnwindSafe(|| -> Result<(), String> {
                let handle = catalog_handle(handle)?;
                let json = string(env, catalog_json)?;
                let mut runtimes = catalog_runtimes()
                    .lock()
                    .map_err(|_| "native catalog Runtime registry is poisoned".to_owned())?;
                runtimes
                    .get_mut(&handle)
                    .ok_or_else(|| "unknown native catalog Runtime handle".to_owned())?
                    .replace_disconnected_catalog(&json)
                    .map_err(|error| error.to_string())
            }));
            Ok(match result {
                Ok(Ok(())) => true,
                Ok(Err(error)) => {
                    throw(env, "java/lang/IllegalStateException", error);
                    false
                }
                Err(_) => {
                    throw(
                        env,
                        "java/lang/IllegalStateException",
                        "native catalog Runtime bridge panicked",
                    );
                    false
                }
            })
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeCatalogRuntime_nativeCapabilityCount(
    mut unowned_env: EnvUnowned<'_>,
    _class: JClass<'_>,
    handle: jlong,
) -> jint {
    unowned_env
        .with_env(|env| -> jni::errors::Result<_> {
            let result = catch_unwind(AssertUnwindSafe(|| -> Result<jint, String> {
                let handle = catalog_handle(handle)?;
                let runtimes = catalog_runtimes()
                    .lock()
                    .map_err(|_| "native catalog Runtime registry is poisoned".to_owned())?;
                let count = runtimes
                    .get(&handle)
                    .ok_or_else(|| "unknown native catalog Runtime handle".to_owned())?
                    .snapshot()
                    .local_node
                    .capabilities
                    .len();
                i32::try_from(count).map_err(|_| "catalog capability count exceeds jint".to_owned())
            }));
            Ok(match result {
                Ok(Ok(count)) => count,
                Ok(Err(error)) => {
                    throw(env, "java/lang/IllegalStateException", error);
                    0
                }
                Err(_) => {
                    throw(
                        env,
                        "java/lang/IllegalStateException",
                        "native catalog Runtime bridge panicked",
                    );
                    0
                }
            })
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeCatalogRuntime_nativeClose(
    mut unowned_env: EnvUnowned<'_>,
    _class: JClass<'_>,
    handle: jlong,
) {
    unowned_env
        .with_env(|env| -> jni::errors::Result<_> {
            let result = catch_unwind(AssertUnwindSafe(|| -> Result<(), String> {
                let handle = catalog_handle(handle)?;
                let removed = catalog_runtimes()
                    .lock()
                    .map_err(|_| "native catalog Runtime registry is poisoned".to_owned())?
                    .remove(&handle);
                if removed.is_none() {
                    return Err("unknown native catalog Runtime handle".to_owned());
                }
                Ok(())
            }));
            match result {
                Ok(Ok(())) => {}
                Ok(Err(error)) => throw(env, "java/lang/IllegalStateException", error),
                Err(_) => throw(
                    env,
                    "java/lang/IllegalStateException",
                    "native catalog Runtime bridge panicked",
                ),
            }
            Ok(())
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_touchpad_NativeTouchpadBridge_nativeCreate(
    mut unowned_env: EnvUnowned<'_>,
    _class: JClass<'_>,
    stream_id: JString<'_>,
    stream_epoch: jlong,
    clock_domain_id: JString<'_>,
    width_px: jint,
    height_px: jint,
    width_himetric: jint,
    height_himetric: jint,
    max_contacts: jint,
    reports_pressure: jboolean,
    first_sequence: jlong,
) -> jlong {
    unowned_env
        .with_env(|env| -> jni::errors::Result<_> {
            let result = catch_unwind(AssertUnwindSafe(|| {
                let config = AndroidTouchpadBridgeConfigV1 {
                    stream_id: string(env, stream_id)?,
                    stream_epoch: positive_u64(stream_epoch, "stream epoch")?,
                    clock_domain_id: string(env, clock_domain_id)?,
                    width_px: positive_u32(width_px, "pixel width")?,
                    height_px: positive_u32(height_px, "pixel height")?,
                    width_himetric: positive_u32(width_himetric, "himetric width")?,
                    height_himetric: positive_u32(height_himetric, "himetric height")?,
                    max_contacts: u8::try_from(max_contacts)
                        .map_err(|_| "maximum contacts must fit inside u8".to_owned())?,
                    reports_pressure,
                    first_sequence: positive_u64(first_sequence, "first sequence")?,
                };
                let session =
                    AndroidTouchpadPacketSessionV1::new(config).map_err(|e| e.to_string())?;
                NEXT_HANDLE.with(|next| {
                    let handle = next.get();
                    let successor = handle
                        .checked_add(1)
                        .ok_or_else(|| "native touchpad handle space exhausted".to_owned())?;
                    SESSIONS.with_borrow_mut(|sessions| {
                        sessions.insert(handle, session);
                    });
                    next.set(successor);
                    i64::try_from(handle)
                        .map_err(|_| "native handle exceeds signed long".to_owned())
                })
            }));
            let handle = match result {
                Ok(Ok(handle)) => handle,
                Ok(Err(error)) => {
                    throw(env, "java/lang/IllegalArgumentException", error);
                    0
                }
                Err(_) => {
                    throw(
                        env,
                        "java/lang/IllegalStateException",
                        "native touchpad bridge panicked",
                    );
                    0
                }
            };
            Ok(handle)
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_touchpad_NativeTouchpadBridge_nativeStart(
    mut unowned_env: EnvUnowned<'_>,
    _class: JClass<'_>,
    handle: jlong,
    event_time_nanos: jlong,
) -> jbyteArray {
    unowned_env
        .with_env(|env| -> jni::errors::Result<_> {
            let timestamp = match positive_u64(event_time_nanos, "event timestamp") {
                Ok(timestamp) => timestamp,
                Err(error) => {
                    throw(env, "java/lang/IllegalArgumentException", error);
                    return Ok(ptr::null_mut());
                }
            };
            let packet = with_session_packet(env, handle, |session| {
                session
                    .start(timestamp)
                    .map(Some)
                    .map_err(|e| e.to_string())
            });
            Ok(packet)
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_touchpad_NativeTouchpadBridge_nativeMotion(
    mut unowned_env: EnvUnowned<'_>,
    _class: JClass<'_>,
    handle: jlong,
    event_time_nanos: jlong,
    action: jint,
    action_index: jint,
    pointer_ids: JIntArray<'_>,
    tool_types: JIntArray<'_>,
    x_px: JFloatArray<'_>,
    y_px: JFloatArray<'_>,
    pressure: JFloatArray<'_>,
) -> jbyteArray {
    unowned_env
        .with_env(|env| -> jni::errors::Result<_> {
            let timestamp = match positive_u64(event_time_nanos, "event timestamp") {
                Ok(timestamp) => timestamp,
                Err(error) => {
                    throw(env, "java/lang/IllegalArgumentException", error);
                    return Ok(ptr::null_mut());
                }
            };
            let action_index = match usize::try_from(action_index) {
                Ok(index) => index,
                Err(_) => {
                    throw(
                        env,
                        "java/lang/IllegalArgumentException",
                        "negative action index",
                    );
                    return Ok(ptr::null_mut());
                }
            };
            let arrays = (
                int_array(env, &pointer_ids),
                int_array(env, &tool_types),
                float_array(env, &x_px),
                float_array(env, &y_px),
                float_array(env, &pressure),
            );
            let (pointer_ids, tool_types, x_px, y_px, pressure) = match arrays {
                (Ok(a), Ok(b), Ok(c), Ok(d), Ok(e)) => (a, b, c, d, e),
                _ => {
                    throw(
                        env,
                        "java/lang/IllegalArgumentException",
                        "could not read pointer arrays",
                    );
                    return Ok(ptr::null_mut());
                }
            };
            let packet = with_session_packet(env, handle, |session| {
                session
                    .motion(AndroidMotionDtoV1 {
                        event_time_nanos: timestamp,
                        action,
                        action_index,
                        pointer_ids: &pointer_ids,
                        tool_types: &tool_types,
                        x_px: &x_px,
                        y_px: &y_px,
                        pressure: &pressure,
                    })
                    .map(Some)
                    .map_err(|e| e.to_string())
            });
            Ok(packet)
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_touchpad_NativeTouchpadBridge_nativeStop(
    mut unowned_env: EnvUnowned<'_>,
    _class: JClass<'_>,
    handle: jlong,
    event_time_nanos: jlong,
) -> jbyteArray {
    unowned_env
        .with_env(|env| -> jni::errors::Result<_> {
            let timestamp = match positive_u64(event_time_nanos, "event timestamp") {
                Ok(timestamp) => timestamp,
                Err(error) => {
                    throw(env, "java/lang/IllegalArgumentException", error);
                    return Ok(ptr::null_mut());
                }
            };
            let packet = with_session_packet(env, handle, |session| {
                session.stop(timestamp).map_err(|e| e.to_string())
            });
            Ok(packet)
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_touchpad_NativeTouchpadBridge_nativeClose(
    mut unowned_env: EnvUnowned<'_>,
    _class: JClass<'_>,
    handle: jlong,
    event_time_nanos: jlong,
) -> jbyteArray {
    unowned_env
        .with_env(|env| -> jni::errors::Result<_> {
            let timestamp = match positive_u64(event_time_nanos, "event timestamp") {
                Ok(timestamp) => timestamp,
                Err(error) => {
                    throw(env, "java/lang/IllegalArgumentException", error);
                    return Ok(ptr::null_mut());
                }
            };
            let packet = with_session_packet(env, handle, |session| {
                session.close(timestamp).map_err(|e| e.to_string())
            });
            if !env.exception_check() {
                SESSIONS.with_borrow_mut(|sessions| {
                    sessions.remove(&(handle as u64));
                });
            }
            Ok(packet)
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

#[allow(clippy::too_many_arguments)]
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_touchpad_NativeTouchpadRecordBridge_nativeRecordCreate(
    mut unowned_env: EnvUnowned<'_>,
    _class: JClass<'_>,
    stream_id: JString<'_>,
    stream_epoch: jlong,
    clock_domain_id: JString<'_>,
    width_px: jint,
    height_px: jint,
    width_himetric: jint,
    height_himetric: jint,
    max_contacts: jint,
    reports_pressure: jboolean,
    first_sequence: jlong,
    route_id: JString<'_>,
    session_id: JString<'_>,
    source_node_id: JString<'_>,
    source_capability_id: JString<'_>,
    source_port_id: JString<'_>,
    sink_node_id: JString<'_>,
    sink_capability_id: JString<'_>,
    sink_port_id: JString<'_>,
    authorization_expires_at_ms: jlong,
) -> jlong {
    unowned_env
        .with_env(|env| -> jni::errors::Result<_> {
            let result = catch_unwind(AssertUnwindSafe(|| {
                let packet_config = AndroidTouchpadBridgeConfigV1 {
                    stream_id: string(env, stream_id)?,
                    stream_epoch: positive_u64(stream_epoch, "stream epoch")?,
                    clock_domain_id: string(env, clock_domain_id)?,
                    width_px: positive_u32(width_px, "pixel width")?,
                    height_px: positive_u32(height_px, "pixel height")?,
                    width_himetric: positive_u32(width_himetric, "himetric width")?,
                    height_himetric: positive_u32(height_himetric, "himetric height")?,
                    max_contacts: u8::try_from(max_contacts)
                        .map_err(|_| "maximum contacts must fit inside u8".to_owned())?,
                    reports_pressure,
                    first_sequence: positive_u64(first_sequence, "first sequence")?,
                };
                let route_config = AndroidTouchpadRouteConfigV1 {
                    route_id: string(env, route_id)?,
                    session_id: string(env, session_id)?,
                    source_node_id: string(env, source_node_id)?,
                    source_capability_id: string(env, source_capability_id)?,
                    source_port_id: string(env, source_port_id)?,
                    sink_node_id: string(env, sink_node_id)?,
                    sink_capability_id: string(env, sink_capability_id)?,
                    sink_port_id: string(env, sink_port_id)?,
                    authorization_expires_at_ms: if authorization_expires_at_ms < 0 {
                        None
                    } else {
                        Some(authorization_expires_at_ms as u64)
                    },
                };
                let session = AndroidTouchpadRecordSessionV1::new(packet_config, route_config)
                    .map_err(|error| error.to_string())?;
                NEXT_RECORD_HANDLE.with(|next| {
                    let handle = next.get();
                    let successor = handle
                        .checked_add(1)
                        .ok_or_else(|| "native record handle space exhausted".to_owned())?;
                    RECORD_SESSIONS.with_borrow_mut(|sessions| {
                        sessions.insert(handle, session);
                    });
                    next.set(successor);
                    i64::try_from(handle)
                        .map_err(|_| "native record handle exceeds signed long".to_owned())
                })
            }));
            let handle = match result {
                Ok(Ok(handle)) => handle,
                Ok(Err(error)) => {
                    throw(env, "java/lang/IllegalArgumentException", error);
                    0
                }
                Err(_) => {
                    throw(
                        env,
                        "java/lang/IllegalStateException",
                        "native record bridge panicked",
                    );
                    0
                }
            };
            Ok(handle)
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_touchpad_NativeTouchpadRecordBridge_nativeRecordHello(
    mut unowned_env: EnvUnowned<'_>,
    _class: JClass<'_>,
    handle: jlong,
) -> jbyteArray {
    unowned_env
        .with_env(|env| -> jni::errors::Result<_> {
            Ok(with_record_session(env, handle, |session| {
                Ok(Some(session.hello()))
            }))
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_touchpad_NativeTouchpadRecordBridge_nativeRecordStart(
    mut unowned_env: EnvUnowned<'_>,
    _class: JClass<'_>,
    handle: jlong,
    event_time_nanos: jlong,
) -> jbyteArray {
    unowned_env
        .with_env(|env| -> jni::errors::Result<_> {
            let timestamp = match positive_u64(event_time_nanos, "event timestamp") {
                Ok(value) => value,
                Err(error) => {
                    throw(env, "java/lang/IllegalArgumentException", error);
                    return Ok(ptr::null_mut());
                }
            };
            Ok(with_record_session(env, handle, |session| {
                session
                    .start(timestamp)
                    .map(Some)
                    .map_err(|e| e.to_string())
            }))
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_touchpad_NativeTouchpadRecordBridge_nativeRecordMotion(
    mut unowned_env: EnvUnowned<'_>,
    _class: JClass<'_>,
    handle: jlong,
    event_time_nanos: jlong,
    action: jint,
    action_index: jint,
    pointer_ids: JIntArray<'_>,
    tool_types: JIntArray<'_>,
    x_px: JFloatArray<'_>,
    y_px: JFloatArray<'_>,
    pressure: JFloatArray<'_>,
) -> jbyteArray {
    unowned_env
        .with_env(|env| -> jni::errors::Result<_> {
            let timestamp = match positive_u64(event_time_nanos, "event timestamp") {
                Ok(value) => value,
                Err(error) => {
                    throw(env, "java/lang/IllegalArgumentException", error);
                    return Ok(ptr::null_mut());
                }
            };
            let action_index = match usize::try_from(action_index) {
                Ok(value) => value,
                Err(_) => {
                    throw(
                        env,
                        "java/lang/IllegalArgumentException",
                        "negative action index",
                    );
                    return Ok(ptr::null_mut());
                }
            };
            let arrays = (
                int_array(env, &pointer_ids),
                int_array(env, &tool_types),
                float_array(env, &x_px),
                float_array(env, &y_px),
                float_array(env, &pressure),
            );
            let (pointer_ids, tool_types, x_px, y_px, pressure) = match arrays {
                (Ok(a), Ok(b), Ok(c), Ok(d), Ok(e)) => (a, b, c, d, e),
                _ => {
                    throw(
                        env,
                        "java/lang/IllegalArgumentException",
                        "could not read pointer arrays",
                    );
                    return Ok(ptr::null_mut());
                }
            };
            Ok(with_record_session(env, handle, |session| {
                session
                    .motion(AndroidMotionDtoV1 {
                        event_time_nanos: timestamp,
                        action,
                        action_index,
                        pointer_ids: &pointer_ids,
                        tool_types: &tool_types,
                        x_px: &x_px,
                        y_px: &y_px,
                        pressure: &pressure,
                    })
                    .map(Some)
                    .map_err(|e| e.to_string())
            }))
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_touchpad_NativeTouchpadRecordBridge_nativeRecordStop(
    mut unowned_env: EnvUnowned<'_>,
    _class: JClass<'_>,
    handle: jlong,
    event_time_nanos: jlong,
) -> jbyteArray {
    unowned_env
        .with_env(|env| -> jni::errors::Result<_> {
            let timestamp = match positive_u64(event_time_nanos, "event timestamp") {
                Ok(value) => value,
                Err(error) => {
                    throw(env, "java/lang/IllegalArgumentException", error);
                    return Ok(ptr::null_mut());
                }
            };
            Ok(with_record_session(env, handle, |session| {
                session.stop(timestamp).map_err(|e| e.to_string())
            }))
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_touchpad_NativeTouchpadRecordBridge_nativeRecordClose(
    mut unowned_env: EnvUnowned<'_>,
    _class: JClass<'_>,
    handle: jlong,
    event_time_nanos: jlong,
) -> jbyteArray {
    unowned_env
        .with_env(|env| -> jni::errors::Result<_> {
            let timestamp = match positive_u64(event_time_nanos, "event timestamp") {
                Ok(value) => value,
                Err(error) => {
                    throw(env, "java/lang/IllegalArgumentException", error);
                    return Ok(ptr::null_mut());
                }
            };
            Ok(with_record_session(env, handle, |session| {
                session.close(timestamp).map_err(|e| e.to_string())
            }))
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_touchpad_NativeTouchpadRecordBridge_nativeRecordTakeClose(
    mut unowned_env: EnvUnowned<'_>,
    _class: JClass<'_>,
    handle: jlong,
) -> jbyteArray {
    unowned_env
        .with_env(|env| -> jni::errors::Result<_> {
            let bytes =
                with_record_session(env, handle, |session| Ok(Some(session.close_record())));
            if !env.exception_check() {
                RECORD_SESSIONS.with_borrow_mut(|sessions| {
                    sessions.remove(&(handle as u64));
                });
            }
            Ok(bytes)
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}
