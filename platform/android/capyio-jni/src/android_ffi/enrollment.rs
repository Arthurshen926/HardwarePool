use super::*;
use capyio_core::NodeId;
use capyio_enrollment::EnrollmentManager;
use jni::{
    jni_sig, jni_str,
    objects::{JObject, JValue},
};
use serde::Deserialize;

static MANAGER: OnceLock<Mutex<EnrollmentManager>> = OnceLock::new();
fn manager() -> &'static Mutex<EnrollmentManager> {
    MANAGER.get_or_init(|| Mutex::new(EnrollmentManager::default()))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    action: String,
    host: Option<String>,
    port: Option<u16>,
    pin: Option<String>,
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidFilelessPairing_nativeAction(
    mut unowned: EnvUnowned<'_>,
    _class: JClass<'_>,
    local_id: JString<'_>,
    name: JString<'_>,
    directory: JByteArray<'_>,
    request: JString<'_>,
) -> jbyteArray {
    unowned
        .with_env(|env| -> jni::errors::Result<_> {
            let result = (|| -> Result<Vec<u8>, String> {
                let request = string(env, request)?;
                if request.len() > 1024 {
                    return Err("enrollment request too large".into());
                }
                let request: Request = serde_json::from_str(&request).map_err(|e| e.to_string())?;
                let mut manager = manager().lock().map_err(|_| "enrollment lock poisoned")?;
                match request.action.as_str() {
                    "approve" => manager.approve()?,
                    "cancel" => manager.cancel(),
                    "discover" => {
                        let local = NodeId::from_str(&string(env, local_id)?).map_err(|e| e.to_string())?;
                        let interface = request.host.ok_or("请选择本机网络")?.parse().map_err(|_| "局域网发现需要 IPv4 地址")?;
                        manager.discover(local, interface)?;
                    }
                    "welcome" | "join" => {
                        let local =
                            NodeId::from_str(&string(env, local_id)?).map_err(|e| e.to_string())?;
                        let name = string(env, name)?;
                        let encoded = pairing_directory_bytes(env, &directory)?;
                    let existing =
                        capyio_pairing::PairedPeerDirectory::decode_json(local, &encoded)
                            .map_err(|e| e.to_string())?;
                    if existing.peers().any(|peer| matches!(peer.state, capyio_pairing::PairingState::Active)) {
                        return Err("此 Android 节点已有有效配对，请直接连接原设备；首次配对不会覆盖现有证书。".into());
                    }
                        let paired = existing.peers().map(|p| p.node_id).collect();
                        let host: IpAddr = request
                            .host
                            .ok_or("missing IP")?
                            .parse()
                            .map_err(|_| "invalid IP")?;
                        let port = request.port.filter(|p| *p != 0).ok_or("invalid port")?;
                        if request.action == "welcome" {
                            manager.welcome(
                                local,
                                name,
                                host,
                                port,
                                paired,
                                "android-keystore:",
                            )?;
                        } else {
                            manager.join(
                                local,
                                name,
                                SocketAddr::new(host, port),
                                request.pin.ok_or("missing PIN")?,
                                paired,
                                "android-keystore:",
                            )?;
                        }
                        manager.defer_control_ready();
                    }
                    _ => return Err("unknown enrollment action".into()),
                }
                serde_json::to_vec(&manager.status()).map_err(|e| e.to_string())
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
pub extern "system" fn Java_dev_capyio_android_AndroidFilelessPairing_nativeControlReady(
    _env: EnvUnowned<'_>,
    _class: JClass<'_>,
    port: jint,
) {
    if let Ok(manager) = manager().lock() {
        manager.publish_control_port(u16::try_from(port).ok().and_then(std::num::NonZeroU16::new));
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_capyio_android_AndroidFilelessPairing_nativePoll(
    mut unowned: EnvUnowned<'_>,
    _class: JClass<'_>,
    owner: JObject<'_>,
) -> jbyteArray {
    unowned
        .with_env(|env| -> jni::errors::Result<_> {
            let result = (|| -> Result<Vec<u8>, String> {
                let manager = manager().lock().map_err(|_| "enrollment lock poisoned")?;
                manager.stage_with(|bytes| {
                    let array = env
                        .byte_array_from_slice(bytes)
                        .map_err(|e| e.to_string())?;
                    let saved = env
                        .call_method(
                            &owner,
                            jni_str!("stageCandidate"),
                            jni_sig!("([B)Z"),
                            &[JValue::Object(array.as_ref())],
                        )
                        .map_err(|e| e.to_string())?
                        .z()
                        .map_err(|e| e.to_string())?;
                    if saved {
                        Ok(())
                    } else {
                        Err("无法保存本机配对恢复记录；现有配对未替换".into())
                    }
                })?;
                manager.persist_with(|_| {
                    let saved = env
                        .call_method(&owner, jni_str!("commitCandidate"), jni_sig!("()Z"), &[])
                        .map_err(|e| e.to_string())?
                        .z()
                        .map_err(|e| e.to_string())?;
                    if saved {
                        Ok(())
                    } else {
                        Err("本机配对提交未完成；请保留恢复记录".into())
                    }
                })?;
                serde_json::to_vec(&manager.status()).map_err(|e| e.to_string())
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
