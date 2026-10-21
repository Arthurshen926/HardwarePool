use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    net::{IpAddr, SocketAddr},
    ops::{Deref, DerefMut},
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
    str::FromStr,
    sync::{
        Mutex, MutexGuard, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use jni::{
    Env, EnvUnowned,
    errors::ThrowRuntimeExAndDefault,
    objects::{JByteArray, JClass, JFloatArray, JIntArray, JString},
    strings::JNIString,
    sys::{jboolean, jbyteArray, jint, jlong},
};

use crate::{
    AndroidCatalogRuntimeV1, AndroidControlListenerV1, AndroidMotionDtoV1,
    AndroidTouchpadBridgeConfigV1, AndroidTouchpadPacketSessionV1, AndroidTouchpadRecordSessionV1,
    AndroidTouchpadRouteConfigV1, android_pairing_directory_active_credential_handles,
    android_pairing_directory_peer_count, authenticate_android_control_dial,
    bind_android_control_listener, canonicalize_android_pairing_directory,
    exchange_accepted_android_catalog, exchange_dialed_android_catalog,
};
use capyio_control_link::{CatalogControlLink, ReceivedControlMessage};
use capyio_control_tls::ControlTlsEndpoint;
use capyio_discovery::DiscoveryAnnouncement;
use capyio_discovery_udp::DiscoveryUdpAnnouncer;

mod audio_use;
mod camera_use;
mod controller_use;
mod display_use;
mod enrollment;
mod secure_haptics;
mod secure_touchpad;

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeControlCredential_nativeValidate(
    mut unowned_env: EnvUnowned<'_>,
    _class: JClass<'_>,
    encoded_bundle: JByteArray<'_>,
) -> jboolean {
    unowned_env
        .with_env(|env| -> jni::errors::Result<_> {
            let result = catch_unwind(AssertUnwindSafe(|| -> Result<(), String> {
                if encoded_bundle.is_null() {
                    return Err("control credential bytes must not be null".to_owned());
                }
                let length = encoded_bundle.len(env).map_err(|error| error.to_string())?;
                if length > capyio_control_credentials::MAX_CONTROL_CREDENTIAL_BUNDLE_BYTES {
                    return Err("control credential exceeds its bound".to_owned());
                }
                let encoded = env
                    .convert_byte_array(&encoded_bundle)
                    .map_err(|error| error.to_string())?;
                capyio_control_credentials::ControlCredentialBundle::decode(&encoded)
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            }));
            Ok(match result {
                Ok(Ok(())) => true,
                Ok(Err(error)) => {
                    throw(env, "java/lang/IllegalArgumentException", error);
                    false
                }
                Err(_) => {
                    throw(
                        env,
                        "java/lang/IllegalStateException",
                        "native control credential bridge panicked",
                    );
                    false
                }
            })
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

fn pairing_directory_bytes(
    env: &mut Env<'_>,
    directory_json: &JByteArray<'_>,
) -> Result<Vec<u8>, String> {
    if directory_json.is_null() {
        return Err("paired directory bytes must not be null".to_owned());
    }
    let length = directory_json.len(env).map_err(|error| error.to_string())?;
    if length > capyio_pairing::MAX_PAIRED_DIRECTORY_JSON_BYTES {
        return Err("paired directory exceeds its bound".to_owned());
    }
    env.convert_byte_array(directory_json)
        .map_err(|error| error.to_string())
}

fn pairing_invitation(
    env: &mut Env<'_>,
    local_node_id: JString<'_>,
    invitation_bytes: &JByteArray<'_>,
) -> Result<capyio_pairing::PairingInvitation, String> {
    if invitation_bytes.is_null() {
        return Err("pairing invitation bytes must not be null".to_owned());
    }
    let length = invitation_bytes
        .len(env)
        .map_err(|error| error.to_string())?;
    if length > capyio_pairing::MAX_PAIRING_INVITATION_BYTES {
        return Err("pairing invitation exceeds its bound".to_owned());
    }
    let target = capyio_core::NodeId::from_str(&string(env, local_node_id)?)
        .map_err(|_| "local Node ID is invalid".to_owned())?;
    let encoded = env
        .convert_byte_array(invitation_bytes)
        .map_err(|error| error.to_string())?;
    capyio_pairing::PairingInvitation::decode_for(target, &encoded)
        .map_err(|error| error.to_string())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativePairingDirectory_nativeCanonicalize(
    mut unowned_env: EnvUnowned<'_>,
    _class: JClass<'_>,
    local_node_id: JString<'_>,
    directory_json: JByteArray<'_>,
) -> jbyteArray {
    unowned_env
        .with_env(|env| -> jni::errors::Result<_> {
            let result = catch_unwind(AssertUnwindSafe(|| -> Result<Vec<u8>, String> {
                let json = pairing_directory_bytes(env, &directory_json)?;
                canonicalize_android_pairing_directory(&string(env, local_node_id)?, &json)
                    .map_err(|error| error.to_string())
            }));
            Ok(match result {
                Ok(Ok(bytes)) => byte_array(env, &bytes),
                Ok(Err(error)) => {
                    throw(env, "java/lang/IllegalArgumentException", error);
                    ptr::null_mut()
                }
                Err(_) => {
                    throw(
                        env,
                        "java/lang/IllegalStateException",
                        "native paired-directory bridge panicked",
                    );
                    ptr::null_mut()
                }
            })
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativePairingDirectory_nativePeerCount(
    mut unowned_env: EnvUnowned<'_>,
    _class: JClass<'_>,
    local_node_id: JString<'_>,
    directory_json: JByteArray<'_>,
) -> jint {
    unowned_env
        .with_env(|env| -> jni::errors::Result<_> {
            let result = catch_unwind(AssertUnwindSafe(|| -> Result<jint, String> {
                let json = pairing_directory_bytes(env, &directory_json)?;
                let count =
                    android_pairing_directory_peer_count(&string(env, local_node_id)?, &json)
                        .map_err(|error| error.to_string())?;
                i32::try_from(count).map_err(|_| "paired peer count exceeds jint".to_owned())
            }));
            Ok(match result {
                Ok(Ok(count)) => count,
                Ok(Err(error)) => {
                    throw(env, "java/lang/IllegalArgumentException", error);
                    -1
                }
                Err(_) => {
                    throw(
                        env,
                        "java/lang/IllegalStateException",
                        "native paired-directory bridge panicked",
                    );
                    -1
                }
            })
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativePairingDirectory_nativeActiveCredentialHandles(
    mut unowned_env: EnvUnowned<'_>,
    _class: JClass<'_>,
    local_node_id: JString<'_>,
    directory_json: JByteArray<'_>,
) -> jbyteArray {
    unowned_env
        .with_env(|env| -> jni::errors::Result<_> {
            let result = catch_unwind(AssertUnwindSafe(|| -> Result<Vec<u8>, String> {
                let json = pairing_directory_bytes(env, &directory_json)?;
                let handles = android_pairing_directory_active_credential_handles(
                    &string(env, local_node_id)?,
                    &json,
                )
                .map_err(|error| error.to_string())?;
                let encoded = handles.join("\n").into_bytes();
                if encoded.len() > capyio_pairing::MAX_PAIRED_DIRECTORY_JSON_BYTES {
                    return Err("active credential handles exceed their bound".to_owned());
                }
                Ok(encoded)
            }));
            Ok(match result {
                Ok(Ok(bytes)) => byte_array(env, &bytes),
                Ok(Err(error)) => {
                    throw(env, "java/lang/IllegalArgumentException", error);
                    ptr::null_mut()
                }
                Err(_) => {
                    throw(
                        env,
                        "java/lang/IllegalStateException",
                        "native active-credential projection panicked",
                    );
                    ptr::null_mut()
                }
            })
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativePairingDirectory_nativeInvitationDirectory(
    mut unowned_env: EnvUnowned<'_>,
    _class: JClass<'_>,
    local_node_id: JString<'_>,
    invitation_bytes: JByteArray<'_>,
) -> jbyteArray {
    unowned_env
        .with_env(|env| -> jni::errors::Result<_> {
            let result = catch_unwind(AssertUnwindSafe(|| {
                pairing_invitation(env, local_node_id, &invitation_bytes)
                    .map(|invitation| invitation.directory_json().to_vec())
            }));
            Ok(match result {
                Ok(Ok(bytes)) => byte_array(env, &bytes),
                Ok(Err(error)) => {
                    throw(env, "java/lang/IllegalArgumentException", error);
                    ptr::null_mut()
                }
                Err(_) => {
                    throw(
                        env,
                        "java/lang/IllegalStateException",
                        "native invitation bridge panicked",
                    );
                    ptr::null_mut()
                }
            })
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativePairingDirectory_nativeInvitationCredential(
    mut unowned_env: EnvUnowned<'_>,
    _class: JClass<'_>,
    local_node_id: JString<'_>,
    invitation_bytes: JByteArray<'_>,
) -> jbyteArray {
    unowned_env
        .with_env(|env| -> jni::errors::Result<_> {
            let result = catch_unwind(AssertUnwindSafe(|| {
                pairing_invitation(env, local_node_id, &invitation_bytes)
                    .map(|invitation| invitation.credential_bundle().to_vec())
            }));
            Ok(match result {
                Ok(Ok(bytes)) => byte_array(env, &bytes),
                Ok(Err(error)) => {
                    throw(env, "java/lang/IllegalArgumentException", error);
                    ptr::null_mut()
                }
                Err(_) => {
                    throw(
                        env,
                        "java/lang/IllegalStateException",
                        "native invitation bridge panicked",
                    );
                    ptr::null_mut()
                }
            })
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

static NEXT_CATALOG_HANDLE: AtomicU64 = AtomicU64::new(1);
static CATALOG_RUNTIMES: OnceLock<Mutex<BTreeMap<u64, AndroidCatalogRuntimeV1>>> = OnceLock::new();
static NEXT_CONTROL_LISTENER_HANDLE: AtomicU64 = AtomicU64::new(1);
static CONTROL_LISTENERS: OnceLock<Mutex<BTreeMap<u64, NativeControlListener>>> = OnceLock::new();
const MAX_CATALOG_RUNTIMES: usize = 4;
const MAX_CONTROL_LISTENERS: usize = 4;

struct NativeControlListener {
    peer_address: Option<IpAddr>,
    listener: AndroidControlListenerV1,
    link: Option<CatalogControlLink>,
    runtime_handle: Option<u64>,
    published_catalog_revision: u64,
    published_controller_settings: Option<capyio_protocol::v1::ControllerSettingsState>,
    dialed: bool,
    next_heartbeat_sequence: u64,
    pending_heartbeat: Option<u64>,
    announcer: DiscoveryUdpAnnouncer,
    announcement: DiscoveryAnnouncement,
}

fn control_listeners() -> &'static Mutex<BTreeMap<u64, NativeControlListener>> {
    CONTROL_LISTENERS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn catalog_runtimes() -> &'static Mutex<BTreeMap<u64, AndroidCatalogRuntimeV1>> {
    CATALOG_RUNTIMES.get_or_init(|| Mutex::new(BTreeMap::new()))
}

struct CatalogRuntimeGuard {
    registry: MutexGuard<'static, BTreeMap<u64, AndroidCatalogRuntimeV1>>,
    handle: u64,
}

impl Deref for CatalogRuntimeGuard {
    type Target = AndroidCatalogRuntimeV1;

    fn deref(&self) -> &Self::Target {
        self.registry
            .get(&self.handle)
            .expect("validated Runtime guard")
    }
}

impl DerefMut for CatalogRuntimeGuard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.registry
            .get_mut(&self.handle)
            .expect("validated Runtime guard")
    }
}

fn lock_catalog_runtime(handle: u64) -> Result<CatalogRuntimeGuard, String> {
    let registry = catalog_runtimes()
        .lock()
        .map_err(|_| "catalog Runtime registry poisoned")?;
    if !registry.contains_key(&handle) {
        return Err("unknown catalog Runtime handle".to_owned());
    }
    Ok(CatalogRuntimeGuard { registry, handle })
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

fn allocate_control_listener_handle() -> Result<u64, String> {
    NEXT_CONTROL_LISTENER_HANDLE
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |handle| {
            handle
                .checked_add(1)
                .filter(|next| *next <= i64::MAX as u64)
        })
        .map_err(|_| "native control listener handle space exhausted".to_owned())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeControlListener_nativeCreate(
    mut unowned_env: EnvUnowned<'_>,
    _class: JClass<'_>,
    local_node_id: JString<'_>,
    directory_json: JByteArray<'_>,
    encoded_credential: JByteArray<'_>,
) -> jlong {
    unowned_env
        .with_env(|env| -> jni::errors::Result<_> {
            let result = catch_unwind(AssertUnwindSafe(|| -> Result<jlong, String> {
                let directory = pairing_directory_bytes(env, &directory_json)?;
                if encoded_credential.is_null() {
                    return Err("control credential bytes must not be null".to_owned());
                }
                let credential_length = encoded_credential
                    .len(env)
                    .map_err(|error| error.to_string())?;
                if credential_length
                    > capyio_control_credentials::MAX_CONTROL_CREDENTIAL_BUNDLE_BYTES
                {
                    return Err("control credential exceeds its bound".to_owned());
                }
                let mut credential = env
                    .convert_byte_array(&encoded_credential)
                    .map_err(|error| error.to_string())?;
                let socket = bind_android_control_listener().map_err(|error| error.to_string())?;
                socket
                    .set_nonblocking(true)
                    .map_err(|error| error.to_string())?;
                let local_node_id = string(env, local_node_id)?;
                let listener = AndroidControlListenerV1::from_listener(
                    socket,
                    &local_node_id,
                    &directory,
                    &credential,
                    std::time::Duration::from_secs(5),
                )
                .map_err(|error| error.to_string());
                credential.fill(0);
                let listener = listener?;
                let node_id = capyio_core::NodeId::from_str(&local_node_id)
                    .map_err(|_| "local Node ID is invalid".to_owned())?;
                let random = capyio_core::NodeId::new().as_uuid().as_u128().to_be_bytes();
                let instance_nonce = u64::from_be_bytes(
                    random[..8]
                        .try_into()
                        .map_err(|_| "discovery nonce generation failed".to_owned())?,
                )
                .max(1);
                let announcement = DiscoveryAnnouncement::new(
                    node_id,
                    listener
                        .local_addr()
                        .map_err(|error| error.to_string())?
                        .port(),
                    instance_nonce,
                )
                .map_err(|error| error.to_string())?;
                let announcer =
                    DiscoveryUdpAnnouncer::bind_default().map_err(|error| error.to_string())?;
                let handle = allocate_control_listener_handle()?;
                let mut listeners = control_listeners()
                    .lock()
                    .map_err(|_| "native control listener registry is poisoned".to_owned())?;
                if listeners.len() >= MAX_CONTROL_LISTENERS {
                    return Err("native control listener registry is full".to_owned());
                }
                listeners.insert(
                    handle,
                    NativeControlListener {
                        listener,
                        link: None,
                        runtime_handle: None,
                        published_catalog_revision: 0,
                        published_controller_settings: None,
                        dialed: false,
                        next_heartbeat_sequence: 1,
                        peer_address: None,
                        pending_heartbeat: None,
                        announcer,
                        announcement,
                    },
                );
                i64::try_from(handle)
                    .map_err(|_| "native control listener handle exceeds signed long".to_owned())
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
                        "native control listener creation panicked",
                    );
                    0
                }
            })
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeControlListener_nativePort(
    mut unowned_env: EnvUnowned<'_>,
    _class: JClass<'_>,
    handle: jlong,
) -> jint {
    unowned_env
        .with_env(|env| -> jni::errors::Result<_> {
            let result = catch_unwind(AssertUnwindSafe(|| -> Result<jint, String> {
                let handle = catalog_handle(handle)?;
                let listeners = control_listeners()
                    .lock()
                    .map_err(|_| "native control listener registry is poisoned".to_owned())?;
                let port = listeners
                    .get(&handle)
                    .ok_or_else(|| "unknown native control listener handle".to_owned())?
                    .listener
                    .local_addr()
                    .map_err(|error| error.to_string())?
                    .port();
                Ok(i32::from(port))
            }));
            Ok(match result {
                Ok(Ok(port)) => port,
                Ok(Err(error)) => {
                    throw(env, "java/lang/IllegalStateException", error);
                    0
                }
                Err(_) => {
                    throw(
                        env,
                        "java/lang/IllegalStateException",
                        "native control listener port lookup panicked",
                    );
                    0
                }
            })
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeControlListener_nativeAnnounce(
    mut unowned_env: EnvUnowned<'_>,
    _class: JClass<'_>,
    handle: jlong,
) -> jboolean {
    unowned_env
        .with_env(|_env| -> jni::errors::Result<_> {
            let result = catch_unwind(AssertUnwindSafe(|| -> Result<(), String> {
                let handle = catalog_handle(handle)?;
                let listeners = control_listeners()
                    .lock()
                    .map_err(|_| "native control listener registry is poisoned".to_owned())?;
                let owner = listeners
                    .get(&handle)
                    .ok_or_else(|| "unknown native control listener handle".to_owned())?;
                owner
                    .announcer
                    .announce(owner.announcement)
                    .map_err(|error| error.to_string())
            }));
            Ok(matches!(result, Ok(Ok(()))))
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeControlListener_nativeTryAccept(
    mut unowned_env: EnvUnowned<'_>,
    _class: JClass<'_>,
    listener_handle: jlong,
    runtime_handle: jlong,
) -> jboolean {
    unowned_env
        .with_env(|env| -> jni::errors::Result<_> {
            let result = catch_unwind(AssertUnwindSafe(|| -> Result<bool, String> {
                let listener_handle = catalog_handle(listener_handle)?;
                let runtime_handle = catalog_handle(runtime_handle)?;
                let mut listeners = control_listeners()
                    .lock()
                    .map_err(|_| "native control listener registry is poisoned".to_owned())?;
                let owner = listeners
                    .get_mut(&listener_handle)
                    .ok_or_else(|| "unknown native control listener handle".to_owned())?;
                if owner.link.is_some() {
                    return Ok(true);
                }
                if let Some(channel) = owner
                    .listener
                    .try_accept_authenticated()
                    .map_err(|error| error.to_string())?
                {
                    owner.peer_address =
                        Some(channel.peer_addr().map_err(|error| error.to_string())?.ip());
                    let (link, revision) = exchange_accepted_android_catalog(
                        channel,
                        owner.listener.paired_display_name().to_owned(),
                        || lock_catalog_runtime(runtime_handle),
                    )?;
                    owner.link = Some(link);
                    owner.runtime_handle = Some(runtime_handle);
                    owner.published_catalog_revision = revision;
                    owner.dialed = false;
                    owner.pending_heartbeat = None;
                    Ok(true)
                } else {
                    Ok(false)
                }
            }));
            Ok(match result {
                Ok(Ok(connected)) => connected,
                Ok(Err(error)) => {
                    throw(env, "java/lang/IllegalStateException", error);
                    false
                }
                Err(_) => {
                    throw(
                        env,
                        "java/lang/IllegalStateException",
                        "native control listener accept panicked",
                    );
                    false
                }
            })
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeControlListener_nativeDial(
    mut unowned_env: EnvUnowned<'_>,
    _class: JClass<'_>,
    listener_handle: jlong,
    runtime_handle: jlong,
    host: JString<'_>,
    port: jint,
    directory_json: JByteArray<'_>,
    encoded_credential: JByteArray<'_>,
) -> jboolean {
    unowned_env
        .with_env(|env| -> jni::errors::Result<_> {
            let result = catch_unwind(AssertUnwindSafe(|| -> Result<(), String> {
                let listener_handle = catalog_handle(listener_handle)?;
                let runtime_handle = catalog_handle(runtime_handle)?;
                let host = string(env, host)?;
                let address = IpAddr::from_str(&host)
                    .map_err(|_| "control target must be a numeric IP".to_owned())?;
                let port = u16::try_from(port).map_err(|_| "control port is invalid".to_owned())?;
                if port == 0 || address.is_unspecified() || address.is_multicast() {
                    return Err("control endpoint is invalid".to_owned());
                }
                let directory = pairing_directory_bytes(env, &directory_json)?;
                if encoded_credential.is_null()
                    || encoded_credential
                        .len(env)
                        .map_err(|error| error.to_string())?
                        > capyio_control_credentials::MAX_CONTROL_CREDENTIAL_BUNDLE_BYTES
                {
                    return Err("control credential exceeds its bound".to_owned());
                }
                let mut credential = env
                    .convert_byte_array(&encoded_credential)
                    .map_err(|error| error.to_string())?;
                let exchange = (|| -> Result<(), String> {
                    let mut listeners = control_listeners()
                        .lock()
                        .map_err(|_| "control listener registry poisoned".to_owned())?;
                    let owner = listeners
                        .get_mut(&listener_handle)
                        .ok_or("unknown control listener")?;
                    if owner.link.is_some() {
                        return Err("a control peer is already connected".to_owned());
                    }
                    let local_id = lock_catalog_runtime(runtime_handle)?
                        .snapshot()
                        .local_node
                        .id;
                    let (channel, display_name) = authenticate_android_control_dial(
                        local_id,
                        ControlTlsEndpoint {
                            address: SocketAddr::new(address, port),
                            io_timeout: Duration::from_secs(5),
                        },
                        &directory,
                        &credential,
                    )
                    .map_err(|error| error.to_string())?;
                    let (link, revision) =
                        exchange_dialed_android_catalog(channel, display_name, || {
                            lock_catalog_runtime(runtime_handle)
                        })?;
                    owner.link = Some(link);
                    owner.runtime_handle = Some(runtime_handle);
                    owner.published_catalog_revision = revision;
                    owner.dialed = true;
                    owner.peer_address = Some(address);
                    owner.next_heartbeat_sequence = 1;
                    owner.pending_heartbeat = None;
                    Ok(())
                })();
                credential.fill(0);
                exchange
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
                        "native control dial panicked",
                    );
                    false
                }
            })
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeControlListener_nativeServiceConnection(
    mut unowned_env: EnvUnowned<'_>,
    _class: JClass<'_>,
    listener_handle: jlong,
    runtime_handle: jlong,
) -> jboolean {
    unowned_env
        .with_env(|env| -> jni::errors::Result<_> {
            let result = catch_unwind(AssertUnwindSafe(|| -> Result<bool, String> {
                let listener_handle = catalog_handle(listener_handle)?;
                let runtime_handle = catalog_handle(runtime_handle)?;
                let mut listeners = control_listeners()
                    .lock()
                    .map_err(|_| "native control listener registry is poisoned".to_owned())?;
                let owner = listeners
                    .get_mut(&listener_handle)
                    .ok_or_else(|| "unknown native control listener handle".to_owned())?;
                let now_ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|_| "system clock is before Unix epoch".to_owned())?
                    .as_millis() as u64;
                if let Some(runtime) = catalog_runtimes()
                    .lock()
                    .map_err(|_| "native catalog Runtime registry is poisoned".to_owned())?
                    .get_mut(&runtime_handle)
                {
                    runtime.runtime.expire_port_leases(now_ms);
                    runtime.expire_touchpad_requests(now_ms);
                }
                let settings = catalog_runtimes()
                    .lock()
                    .map_err(|_| "Runtime registry poisoned")?
                    .get(&runtime_handle)
                    .and_then(|runtime| {
                        runtime.controller_settings.clone().map(|mut state| {
                            let current_session = owner.link.as_ref().map(|link| link.session_id());
                            if !runtime.controller_settings_request_origin.is_some_and(
                                |(session, id)| {
                                    Some(session) == current_session && id == state.request_id
                                },
                            ) {
                                state.request_id = 0;
                                state.error.clear();
                            }
                            state
                        })
                    });
                if settings != owner.published_controller_settings {
                    if let (Some(link), Some(value)) = (owner.link.as_mut(), settings.as_ref()) {
                        link.send_controller_settings_state(value.clone())
                            .map_err(|error| error.to_string())?;
                        owner.published_controller_settings = settings;
                    }
                }
                if owner.dialed && owner.pending_heartbeat.is_none() {
                    let sequence = owner.next_heartbeat_sequence;
                    owner
                        .link
                        .as_mut()
                        .ok_or("dialed control link is absent")?
                        .send_heartbeat(sequence, false)
                        .map_err(|error| error.to_string())?;
                    owner.pending_heartbeat = Some(sequence);
                    owner.next_heartbeat_sequence = sequence
                        .checked_add(1)
                        .ok_or("control heartbeat sequence exhausted")?;
                }
                if let Some(link) = owner.link.as_mut() {
                    let mut runtimes = catalog_runtimes()
                        .lock()
                        .map_err(|_| "Runtime registry poisoned")?;
                    let host = runtimes.get_mut(&runtime_handle).ok_or("unknown Runtime")?;
                    host.publish_audio_control(link, now_ms)?;
                    display_use::publish(host, link)?;
                    if let Some(id) = host.controller_status_pending.take() {
                        if host.publication_revision() != owner.published_catalog_revision {
                            let selected = host.published_source_capabilities();
                            let catalog = link
                                .prepare_catalog(&host.runtime, &selected)
                                .map_err(|e| e.to_string())?;
                            link.send_catalog(catalog).map_err(|e| e.to_string())?;
                            owner.published_catalog_revision = host.publication_revision();
                        }
                        let route = host.runtime.route(id).map_err(|e| e.to_string())?;
                        if route.session_id == link.session_id() {
                            link.send_route_status(route).map_err(|e| e.to_string())?;
                        }
                    }
                }
                let service_result = match owner.link.as_mut() {
                    Some(link) => match link.receive_control_message() {
                        Ok(ReceivedControlMessage::DisplaySessionControl(message)) => {
                            let mut runtimes = catalog_runtimes()
                                .lock()
                                .map_err(|_| "Runtime registry poisoned")?;
                            let host =
                                runtimes.get_mut(&runtime_handle).ok_or("unknown Runtime")?;
                            let operation = message.operation_id.clone();
                            let route = message.route_id.clone();
                            if let Err(error) = display_use::receive(
                                host,
                                link,
                                message,
                                owner.peer_address.ok_or("authenticated peer absent")?,
                            ) {
                                display_use::receive_failure(host, operation, route, error);
                            }
                            Ok(())
                        }
                        Ok(ReceivedControlMessage::ControllerSettingsRequest(request)) => {
                            let mut runtimes = catalog_runtimes()
                                .lock()
                                .map_err(|_| "Runtime registry poisoned")?;
                            let runtime =
                                runtimes.get_mut(&runtime_handle).ok_or("unknown Runtime")?;
                            if runtime.controller_settings_request.is_none() {
                                runtime.controller_settings_request =
                                    Some((link.session_id(), request));
                            } else if let Some(mut state) = runtime.controller_settings.clone() {
                                state.request_id = request.request_id;
                                state.error = "settings_busy".to_owned();
                                link.send_controller_settings_state(state)
                                    .map_err(|error| error.to_string())?;
                            }
                            Ok(())
                        }
                        Ok(ReceivedControlMessage::ControllerSettingsState(_)) => Ok(()),
                        Ok(ReceivedControlMessage::ControllerReceiverOffer(mut offer)) => {
                            // Never let an offer redirect hardware payloads to another host.
                            offer.host = owner
                                .peer_address
                                .ok_or("authenticated peer address absent")?
                                .to_string();
                            let mut runtimes = catalog_runtimes()
                                .lock()
                                .map_err(|_| "native Runtime registry poisoned")?;
                            let runtime = runtimes
                                .get_mut(&runtime_handle)
                                .ok_or("unknown native Runtime")?;
                            runtime.controller_receiver = Some((link.session_id(), offer));
                            Ok(())
                        }
                        Ok(ReceivedControlMessage::Heartbeat(heartbeat))
                            if !heartbeat.acknowledgement =>
                        {
                            link.send_heartbeat(heartbeat.sequence, true)
                                .map_err(|error| error.to_string())?;
                            let publication = {
                                let runtimes = catalog_runtimes().lock().map_err(|_| {
                                    "native catalog Runtime registry is poisoned".to_owned()
                                })?;
                                let runtime = runtimes.get(&runtime_handle).ok_or_else(|| {
                                    "unknown native catalog Runtime handle".to_owned()
                                })?;
                                if runtime.publication_revision()
                                    == owner.published_catalog_revision
                                {
                                    None
                                } else {
                                    let local = runtime.runtime.snapshot().local_node;
                                    let selected = local
                                        .capabilities
                                        .iter()
                                        .filter(|(_, capability)| {
                                            capability.ports.values().any(|port| {
                                                port.direction == capyio_core::PortDirection::Source
                                            })
                                        })
                                        .map(|(id, _)| *id)
                                        .collect();
                                    Some((
                                        link.prepare_catalog(&runtime.runtime, &selected)
                                            .map_err(|error| error.to_string())?,
                                        runtime.publication_revision(),
                                    ))
                                }
                            };
                            if let Some((outgoing, revision)) = publication {
                                link.send_catalog(outgoing)
                                    .map_err(|error| error.to_string())?;
                                owner.published_catalog_revision = revision;
                            }
                            Ok(())
                        }
                        Ok(ReceivedControlMessage::Catalog(catalog)) => {
                            let mut runtimes = catalog_runtimes().lock().map_err(|_| {
                                "native catalog Runtime registry is poisoned".to_owned()
                            })?;
                            let runtime = runtimes.get_mut(&runtime_handle).ok_or_else(|| {
                                "unknown native catalog Runtime handle".to_owned()
                            })?;
                            link.apply_catalog(&mut runtime.runtime, catalog)
                                .map(|_| ())
                                .map_err(|error| error.to_string())
                        }
                        Ok(ReceivedControlMessage::RouteStatus(status)) => {
                            let route = {
                                let mut runtimes = catalog_runtimes().lock().map_err(|_| {
                                    "native catalog Runtime registry is poisoned".to_owned()
                                })?;
                                let runtime =
                                    runtimes.get_mut(&runtime_handle).ok_or_else(|| {
                                        "unknown native catalog Runtime handle".to_owned()
                                    })?;
                                if status.route.profile == capyio_core::ProfileId::audio_frames_v1()
                                {
                                    runtime.receive_audio_route_status(
                                        link,
                                        &status,
                                        &owner
                                            .peer_address
                                            .ok_or("authenticated peer absent")?
                                            .to_string(),
                                        now_ms,
                                    )?
                                } else {
                                    let confirm =
                                        runtime.resource_requires_confirmation(&status.route);
                                    let mut route = link
                                        .apply_route_status(&mut runtime.runtime, &status, !confirm)
                                        .map_err(|error| error.to_string())?;
                                    if let Some(offer) = status.native_audio_data.as_deref() {
                                        crate::audio_negotiation::receive_offer(
                                            runtime,
                                            &status.route,
                                            *offer,
                                            owner
                                                .peer_address
                                                .ok_or("authenticated peer absent")?
                                                .to_string(),
                                            now_ms,
                                        )?;
                                    }
                                    camera_use::receive_status(
                                        runtime,
                                        &status.route,
                                        status.camera_data,
                                        status.secure_camera_data,
                                        owner
                                            .peer_address
                                            .ok_or("authenticated peer absent")?
                                            .to_string(),
                                        now_ms,
                                    )?;
                                    display_use::route_status(runtime, &status.route)?;
                                    if display_use::is_display(&route) {
                                        route = runtime
                                            .runtime
                                            .route(route.id)
                                            .map_err(|e| e.to_string())?
                                            .clone();
                                    }
                                    if runtime.controller_use == Some(route.id)
                                        && status.route.authorization
                                            == capyio_core::AuthorizationState::Denied
                                    {
                                        controller_use::fail(
                                            runtime,
                                            route.id,
                                            "对方拒绝了手柄使用请求",
                                        )?;
                                    }
                                    if confirm
                                        && status.route.authorization
                                            != capyio_core::AuthorizationState::Denied
                                    {
                                        if runtime.audio_local_intent.contains(&route.id)
                                            || (display_use::is_display(&route)
                                                && runtime.display_use.local_intent)
                                        {
                                            // The local button already expresses consent. Wait for the
                                            // remote owner's finite grant before issuing our matching
                                            // grant; a Pending acknowledgement must not mint an
                                            // independent eight-hour grant ahead of Windows' reply.
                                            if route.authorization
                                                == capyio_core::AuthorizationState::Pending
                                                && let capyio_core::AuthorizationState::Authorized {
                                                    expires_at_ms: Some(deadline),
                                                } = status.route.authorization
                                            {
                                                runtime
                                                    .enqueue_touchpad_request(route.id, now_ms)?;
                                                runtime.touchpad_consent.resolve_bounded(
                                                    &mut runtime.runtime,
                                                    route.id,
                                                    true,
                                                    now_ms,
                                                    Some(deadline),
                                                )?;
                                                runtime.audio_status_pending.insert(route.id);
                                                route = runtime
                                                    .runtime
                                                    .route(route.id)
                                                    .map_err(|e| e.to_string())?
                                                    .clone();
                                            }
                                        } else {
                                            runtime.enqueue_touchpad_request(route.id, now_ms)?;
                                            if (runtime.controller_local_intent == Some(route.id)
                                                || runtime.camera_use.as_ref().is_some_and(
                                                    |use_| {
                                                        use_.route_id == route.id
                                                            && use_.local_intent
                                                    },
                                                ))
                                                && route.authorization
                                                    == capyio_core::AuthorizationState::Pending
                                            {
                                                runtime.resolve_touchpad_request(
                                                    route.id, true, now_ms,
                                                )?;
                                                route = runtime
                                                    .runtime
                                                    .route(route.id)
                                                    .map_err(|e| e.to_string())?
                                                    .clone();
                                            }
                                        }
                                    }
                                    if runtime.controller_use == Some(route.id)
                                        && matches!(
                                            status.route.state,
                                            capyio_core::RouteState::Stopped
                                                | capyio_core::RouteState::Failed
                                                | capyio_core::RouteState::Offline
                                        )
                                        && matches!(
                                            route.state,
                                            capyio_core::RouteState::Prepared
                                                | capyio_core::RouteState::Starting
                                                | capyio_core::RouteState::Active
                                        )
                                    {
                                        runtime
                                            .runtime
                                            .begin_route_stop(route.id)
                                            .map_err(|e| e.to_string())?;
                                        runtime
                                            .runtime
                                            .stop_route(route.id)
                                            .map_err(|e| e.to_string())?;
                                    }
                                    crate::audio_negotiation::remote_status(
                                        runtime,
                                        &status.route,
                                    )?;
                                    runtime
                                        .runtime
                                        .route(route.id)
                                        .map_err(|e| e.to_string())?
                                        .clone()
                                }
                            };
                            link.send_route_status(&route)
                                .map_err(|error| error.to_string())
                        }
                        Ok(ReceivedControlMessage::PortLeaseRequest(request)) => {
                            let mut runtimes = catalog_runtimes().lock().map_err(|_| {
                                "native catalog Runtime registry is poisoned".to_owned()
                            })?;
                            let runtime = runtimes.get_mut(&runtime_handle).ok_or_else(|| {
                                "unknown native catalog Runtime handle".to_owned()
                            })?;
                            // Receiving a request may have blocked across the
                            // lease deadline. Re-evaluate while owning Runtime
                            // so Renew cannot revive a lease using pre-read time.
                            let now_ms = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .map_err(|_| "system clock is before Unix epoch".to_owned())?
                                .as_millis() as u64;
                            if runtime.runtime.route(request.request().route_id).is_ok_and(
                                |route| route.profile == capyio_core::ProfileId::audio_frames_v1(),
                            ) {
                                runtime.answer_audio_port_lease(link, request, now_ms)
                            } else {
                                runtime.runtime.expire_port_leases(now_ms);
                                link.answer_port_lease_request(
                                    &mut runtime.runtime,
                                    request,
                                    now_ms,
                                )
                                .map_err(|error| error.to_string())
                            }
                        }
                        Ok(ReceivedControlMessage::RouteCommand(command)) => {
                            let (route, touchpad, haptics, gamepad) = {
                                let mut runtimes = catalog_runtimes().lock().map_err(|_| {
                                    "native catalog Runtime registry is poisoned".to_owned()
                                })?;
                                let runtime =
                                    runtimes.get_mut(&runtime_handle).ok_or_else(|| {
                                        "unknown native catalog Runtime handle".to_owned()
                                    })?;
                                let touchpad = runtime.touchpad_requires_confirmation(
                                    runtime
                                        .runtime
                                        .route(command.route_id)
                                        .map_err(|e| e.to_string())?,
                                );
                                let haptics = !touchpad
                                    && runtime
                                        .runtime
                                        .route(command.route_id)
                                        .map_err(|e| e.to_string())?
                                        .profile
                                        == capyio_core::ProfileId::haptics_feedback_v1()
                                    && runtime.resource_requires_confirmation(
                                        runtime
                                            .runtime
                                            .route(command.route_id)
                                            .map_err(|e| e.to_string())?,
                                    );
                                if touchpad
                                    && command.action == capyio_protocol::RouteCommandAction::Stop
                                    && runtime
                                        .runtime
                                        .route(command.route_id)
                                        .map_err(|e| e.to_string())?
                                        .session_id
                                        == link.session_id()
                                {
                                    // Publish the explicit cancellation reason before
                                    // changing Route state or waking the data worker.
                                    secure_touchpad::stop_runtime(
                                        runtime_handle,
                                        Some(command.route_id),
                                    );
                                }
                                let gamepad = runtime
                                    .runtime
                                    .route(command.route_id)
                                    .map_err(|e| e.to_string())?
                                    .profile
                                    == capyio_core::ProfileId::gamepad_state_v1()
                                    && runtime.resource_requires_confirmation(
                                        runtime
                                            .runtime
                                            .route(command.route_id)
                                            .map_err(|e| e.to_string())?,
                                    );
                                let audio = runtime
                                    .runtime
                                    .route(command.route_id)
                                    .map_err(|e| e.to_string())?
                                    .profile
                                    == capyio_core::ProfileId::audio_frames_v1();
                                let camera = runtime
                                    .runtime
                                    .route(command.route_id)
                                    .map_err(|e| e.to_string())?
                                    .profile
                                    == capyio_core::ProfileId::video_frames_v1();
                                let route = if audio {
                                    runtime
                                        .stage_audio_route_command(link, &command, now_ms)
                                        .map_err(|error| error.to_string())?
                                } else if touchpad || haptics || gamepad || camera {
                                    link.stage_route_command(&mut runtime.runtime, &command, now_ms)
                                        .map_err(|error| error.to_string())?
                                } else {
                                    link.apply_route_command(&mut runtime.runtime, &command, now_ms)
                                        .map_err(|error| error.to_string())?
                                };
                                (route, touchpad, haptics, gamepad)
                            };
                            if gamepad
                                && command.action == capyio_protocol::RouteCommandAction::Start
                            {
                                let mut runtimes = catalog_runtimes()
                                    .lock()
                                    .map_err(|_| "native catalog Runtime registry is poisoned")?;
                                let runtime =
                                    runtimes.get_mut(&runtime_handle).ok_or("unknown Runtime")?;
                                runtime.controller_use = Some(route.id);
                                drop(runtimes);
                                link.send_route_status(&route).map_err(|e| e.to_string())
                            } else if haptics
                                && command.action == capyio_protocol::RouteCommandAction::Start
                            {
                                let binding =
                                    capyio_control_tls::haptics::HapticsTlsBinding::from_route(
                                        link.tls_binding(),
                                        &route,
                                        capyio_core::StreamId::new(),
                                        now_ms,
                                    )
                                    .map_err(|e| e.to_string())?;
                                let listener = owner.listener.haptics_listener(binding)?;
                                let offer =
                                    secure_haptics::prepare(runtime_handle, binding, listener)?;
                                link.send_haptics_data_offer(&route, offer)
                                    .map_err(|e| e.to_string())
                            } else if touchpad
                                && command.action == capyio_protocol::RouteCommandAction::Start
                            {
                                let binding =
                                    capyio_control_tls::touchpad::TouchpadTlsBinding::from_route(
                                        link.tls_binding(),
                                        &route,
                                        capyio_core::StreamId::new(),
                                        now_ms,
                                    )
                                    .map_err(|e| e.to_string())?;
                                let listener = owner.listener.touchpad_listener(binding)?;
                                let offer =
                                    secure_touchpad::prepare(runtime_handle, binding, listener)?;
                                link.send_touchpad_data_offer(&route, offer)
                                    .map_err(|e| e.to_string())
                            } else {
                                if haptics {
                                    secure_haptics::stop_runtime(runtime_handle, Some(route.id));
                                }
                                if touchpad {
                                    secure_touchpad::stop_runtime(runtime_handle, Some(route.id));
                                }
                                link.send_route_status(&route)
                                    .map_err(|error| error.to_string())
                            }
                        }
                        Ok(ReceivedControlMessage::PortLeaseResponse(_)) => {
                            Err("dialing peer sent an unsolicited Port lease response".to_owned())
                        }
                        Ok(ReceivedControlMessage::Heartbeat(heartbeat))
                            if owner.dialed
                                && heartbeat.acknowledgement
                                && owner.pending_heartbeat == Some(heartbeat.sequence) =>
                        {
                            owner.pending_heartbeat = None;
                            Ok(())
                        }
                        Ok(ReceivedControlMessage::Heartbeat(_)) => {
                            Err("peer sent an unexpected heartbeat acknowledgement".to_owned())
                        }
                        Err(error) => Err(error.to_string()),
                    },
                    None => return Ok(false),
                };
                if service_result.is_ok() {
                    return Ok(true);
                }
                let failure = service_result.expect_err("checked error above");
                secure_touchpad::stop_runtime(runtime_handle, None);
                secure_haptics::stop_runtime(runtime_handle, None);
                if let Some(link) = owner.link.as_mut()
                    && let Some(runtime) = catalog_runtimes()
                        .lock()
                        .map_err(|_| "native catalog Runtime registry is poisoned".to_owned())?
                        .get_mut(&runtime_handle)
                {
                    let _ = link.close_and_reconcile(&mut runtime.runtime);
                }
                owner.link = None;
                owner.runtime_handle = None;
                owner.dialed = false;
                owner.pending_heartbeat = None;
                Err(failure)
            }));
            Ok(match result {
                Ok(Ok(connected)) => connected,
                Ok(Err(error)) => {
                    throw(env, "java/lang/IllegalStateException", error);
                    false
                }
                Err(_) => {
                    throw(
                        env,
                        "java/lang/IllegalStateException",
                        "native control service panicked",
                    );
                    false
                }
            })
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeControlListener_nativeDisconnect(
    mut unowned_env: EnvUnowned<'_>,
    _class: JClass<'_>,
    listener_handle: jlong,
    runtime_handle: jlong,
) {
    unowned_env
        .with_env(|env| -> jni::errors::Result<_> {
            let result = catch_unwind(AssertUnwindSafe(|| -> Result<(), String> {
                let listener_handle = catalog_handle(listener_handle)?;
                let runtime_handle = catalog_handle(runtime_handle)?;
                let mut listeners = control_listeners()
                    .lock()
                    .map_err(|_| "control listener registry poisoned")?;
                let owner = listeners
                    .get_mut(&listener_handle)
                    .ok_or("unknown control listener")?;
                if owner
                    .runtime_handle
                    .is_some_and(|handle| handle != runtime_handle)
                {
                    return Err("control Runtime binding mismatch".into());
                }
                secure_touchpad::stop_runtime(runtime_handle, None);
                secure_haptics::stop_runtime(runtime_handle, None);
                let link = owner.link.take();
                owner.runtime_handle = None;
                owner.published_catalog_revision = 0;
                owner.published_controller_settings = None;
                owner.dialed = false;
                owner.pending_heartbeat = None;
                if let Some(mut link) = link {
                    let mut runtimes = catalog_runtimes()
                        .lock()
                        .map_err(|_| "catalog Runtime registry poisoned")?;
                    let runtime = runtimes
                        .get_mut(&runtime_handle)
                        .ok_or("unknown catalog Runtime")?;
                    link.close_and_reconcile(&mut runtime.runtime)
                        .map_err(|e| e.to_string())?;
                }
                Ok(())
            }));
            if !matches!(result, Ok(Ok(()))) {
                throw(
                    env,
                    "java/lang/IllegalStateException",
                    "control connection cleanup failed",
                );
            }
            Ok(())
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeControlListener_nativeClose(
    mut unowned_env: EnvUnowned<'_>,
    _class: JClass<'_>,
    listener_handle: jlong,
    runtime_handle: jlong,
) {
    unowned_env
        .with_env(|env| -> jni::errors::Result<_> {
            let result = catch_unwind(AssertUnwindSafe(|| -> Result<(), String> {
                let listener_handle = catalog_handle(listener_handle)?;
                let runtime_handle = catalog_handle(runtime_handle)?;
                let mut owner = control_listeners()
                    .lock()
                    .map_err(|_| "native control listener registry is poisoned".to_owned())?
                    .remove(&listener_handle)
                    .ok_or_else(|| "unknown native control listener handle".to_owned())?;
                secure_touchpad::stop_runtime(runtime_handle, None);
                secure_haptics::stop_runtime(runtime_handle, None);
                if let Some(link) = owner.link.as_mut() {
                    if let Some(runtime) = catalog_runtimes()
                        .lock()
                        .map_err(|_| "native catalog Runtime registry is poisoned".to_owned())?
                        .get_mut(&runtime_handle)
                    {
                        link.close_and_reconcile(&mut runtime.runtime)
                            .map_err(|error| error.to_string())?;
                    }
                }
                Ok(())
            }));
            if let Ok(Err(error)) = result {
                throw(env, "java/lang/IllegalStateException", error);
            }
            Ok(())
        })
        .resolve::<ThrowRuntimeExAndDefault>()
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
                {
                    let mut runtimes = catalog_runtimes()
                        .lock()
                        .map_err(|_| "native catalog Runtime registry is poisoned".to_owned())?;
                    runtimes
                        .get_mut(&handle)
                        .ok_or_else(|| "unknown native catalog Runtime handle".to_owned())?
                        .replace_catalog(&json)
                        .map_err(|error| error.to_string())?;
                }
                // The control worker notices the new Runtime revision on its
                // next heartbeat. UI/lifecycle publication never waits for
                // the Listener mutex held during blocking socket I/O.
                Ok(())
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
pub extern "system" fn Java_dev_capyio_android_AndroidNativeCatalogRuntime_nativeControllerReceiver(
    mut unowned_env: EnvUnowned<'_>,
    _class: JClass<'_>,
    handle: jlong,
) -> jbyteArray {
    unowned_env
        .with_env(|env| -> jni::errors::Result<_> {
            let result = (|| -> Result<Vec<u8>, String> {
                let handle = catalog_handle(handle)?;
                let runtimes = catalog_runtimes()
                    .lock()
                    .map_err(|_| "native Runtime registry poisoned")?;
                let runtime = runtimes.get(&handle).ok_or("unknown native Runtime")?;
                let offer = runtime.controller_receiver.as_ref().filter(|(id, _)| {
                    runtime.runtime.snapshot().sessions.iter().any(|session| {
                        session.id == *id && session.state == capyio_core::SessionState::Ready
                    })
                });
                Ok(match offer {
                    Some((_, offer)) => serde_json::json!({
                        "host": offer.host, "port": offer.port, "token": offer.token
                    })
                    .to_string()
                    .into_bytes(),
                    None => b"null".to_vec(),
                })
            })();
            Ok(match result {
                Ok(bytes) => byte_array(env, &bytes),
                Err(error) => {
                    throw(env, "java/lang/IllegalStateException", error);
                    ptr::null_mut()
                }
            })
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeCatalogRuntime_nativeControllerSettings(
    mut unowned_env: EnvUnowned<'_>,
    _class: JClass<'_>,
    handle: jlong,
    revision: jlong,
    motion: jboolean,
    strength: jint,
    request_id: jlong,
    error: JString<'_>,
) -> jbyteArray {
    unowned_env.with_env(|env| -> jni::errors::Result<_> {
        let result = (|| -> Result<Vec<u8>, String> {
            let error = string(env, error)?;
            if revision <= 0 || !(0..=100).contains(&strength) || request_id < 0 || error.len() > 128 {
                return Err("invalid controller settings".to_owned());
            }
            let mut runtimes = catalog_runtimes().lock().map_err(|_| "Runtime registry poisoned")?;
            let runtime = runtimes.get_mut(&catalog_handle(handle)?).ok_or("unknown Runtime")?;
            runtime.controller_settings = Some(capyio_protocol::v1::ControllerSettingsState {
                revision: revision as u64, motion_enabled: motion, haptics_strength: strength as u32,
                request_id: request_id as u64, error,
            });
            let ready = runtime.runtime.snapshot().sessions;
            Ok(match runtime.controller_settings_request.take().filter(|(session,_)|
                ready.iter().any(|entry| entry.id == *session && entry.state == capyio_core::SessionState::Ready)) {
                Some((session,request)) => {
                    runtime.controller_settings_request_origin = Some((session, request.request_id));
                    serde_json::json!({"requestId":request.request_id,
                    "expectedRevision":request.expected_revision,"motionEnabled":request.motion_enabled,
                    "hapticsStrength":request.haptics_strength}).to_string().into_bytes()
                },
                None => b"null".to_vec(),
            })
        })();
        Ok(match result {
            Ok(bytes) => byte_array(env, &bytes),
            Err(error) => { throw(env, "java/lang/IllegalStateException", error); ptr::null_mut() }
        })
    }).resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeCatalogRuntime_nativeRemoteCatalog(
    mut env: EnvUnowned<'_>,
    _class: JClass<'_>,
    handle: jlong,
) -> jbyteArray {
    env.with_env(|env| -> jni::errors::Result<_> {
        let result = catch_unwind(AssertUnwindSafe(|| -> Result<Vec<u8>, String> {
            let runtimes = catalog_runtimes().lock().map_err(|_| "Runtime registry poisoned")?;
            let snapshot = runtimes.get(&catalog_handle(handle)?).ok_or("Runtime absent")?.snapshot();
            let mut nodes = Vec::new();
            let mut capabilities = Vec::new();
            for node in snapshot.peers.iter().filter(|node| snapshot.sessions.iter().any(|s|
                s.remote_node_id == node.id && s.state == capyio_core::SessionState::Ready)) {
                nodes.push(serde_json::json!({"id":node.id.to_string(),"displayName":node.display_name,
                    "platform":node.platform,"platformVersion":node.platform_version,"online":true,
                    "local":false,"capabilityCount":node.capabilities.len()}));
                for c in node.capabilities.values() {
                    let simulated = node.adapter_instances.get(&c.adapter_instance_id).is_some_and(|a|
                        matches!(a.adapter_type.as_str(), "capyio.windows.audio" | "capyio.windows.projection"));
                    let class = serde_json::to_value(&c.class).map_err(|e|e.to_string())?;
                    let ports: Vec<_> = c.ports.values().map(|p| serde_json::json!({
                        "id":p.id.to_string(),"displayName":p.display_name,"direction":p.direction,
                        "profile":format!("{}/{}",p.profile.name,p.profile.major),
                        "formats":p.formats.iter().map(|f|f.id.clone()).collect::<Vec<_>>()
                    })).collect();
                    capabilities.push(serde_json::json!({"id":c.id.to_string(),"nodeId":node.id.to_string(),
                        "nodeName":node.display_name,"displayName":c.display_name,
                        "capabilityClass":class.as_str().unwrap_or("custom"),"local":false,
                        "availability":c.availability,"permissionRequirement":c.permission_requirement,
                        "inventorySource":"authenticated_peer_catalog","simulated":simulated,
                        "selectableForSharing":false,"selectedForSharing":false,"ports":ports}));
                }
            }
            serde_json::to_vec(&serde_json::json!({"nodes":nodes,"capabilities":capabilities})).map_err(|e|e.to_string())
        }));
        Ok(match result { Ok(Ok(bytes)) => byte_array(env,&bytes), _ => {
            throw(env,"java/lang/IllegalStateException","cannot read authenticated remote catalog"); ptr::null_mut()
        }})
    }).resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeCatalogRuntime_nativeTouchpadRequests(
    mut unowned_env: EnvUnowned<'_>,
    _class: JClass<'_>,
    handle: jlong,
) -> jbyteArray {
    unowned_env
        .with_env(|env| -> jni::errors::Result<_> {
            let result = catch_unwind(AssertUnwindSafe(|| -> Result<Vec<u8>, String> {
                let handle = catalog_handle(handle)?;
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|e| e.to_string())?
                    .as_millis() as u64;
                let mut runtimes = catalog_runtimes()
                    .lock()
                    .map_err(|_| "native Runtime registry poisoned")?;
                let runtime = runtimes.get_mut(&handle).ok_or("unknown native Runtime")?;
                runtime.resource_requests_json(now)
            }));
            Ok(match result {
                Ok(Ok(bytes)) => byte_array(env, &bytes),
                _ => {
                    throw(
                        env,
                        "java/lang/IllegalStateException",
                        "cannot read resource requests",
                    );
                    ptr::null_mut()
                }
            })
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidNativeCatalogRuntime_nativeResolveTouchpadRequest(
    mut unowned_env: EnvUnowned<'_>,
    _class: JClass<'_>,
    handle: jlong,
    route_id: JString<'_>,
    allow: jboolean,
) -> jboolean {
    unowned_env
        .with_env(|env| -> jni::errors::Result<_> {
            let result = catch_unwind(AssertUnwindSafe(|| -> Result<(), String> {
                let handle = catalog_handle(handle)?;
                let route_text = string(env, route_id)?;
                if route_text.len() != 36 {
                    return Err("invalid Route ID".into());
                }
                let id = capyio_core::RouteId::from_str(&route_text).map_err(|e| e.to_string())?;
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|e| e.to_string())?
                    .as_millis() as u64;
                let mut runtimes = catalog_runtimes()
                    .lock()
                    .map_err(|_| "native Runtime registry poisoned")?;
                let runtime = runtimes.get_mut(&handle).ok_or("unknown native Runtime")?;
                runtime.resolve_touchpad_request(id, allow, now)
            }));
            Ok(matches!(result, Ok(Ok(()))))
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
