//! Enrollment orchestration owned by the host, including recovery.
use super::*;
use capyio_enrollment::EnrollmentStatus;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EnrollmentRequest {
    action: String,
    host: Option<String>,
    port: Option<u16>,
    pin: Option<String>,
}

impl DesktopHost {
    pub fn enrollment_action(
        &self,
        request: EnrollmentRequest,
    ) -> Result<EnrollmentStatus, String> {
        let mut manager = self
            .enrollment
            .lock()
            .map_err(|_| "enrollment lock poisoned")?;
        match request.action.as_str() {
            "approve" => {
                manager.approve()?;
                return Ok(manager.status());
            }
            "cancel" => {
                manager.cancel();
                return Ok(manager.status());
            }
            "welcome" | "join" | "recover" | "discover" => (),
            _ => return Err("unknown enrollment action".into()),
        }
        let (id, name) = {
            let node = self.node.lock().map_err(|_| "node lock poisoned")?;
            let snapshot = node.runtime.snapshot();
            (
                snapshot.local_node.id,
                snapshot.local_node.display_name.clone(),
            )
        };
        let store = self
            .control_plane
            .lock()
            .map_err(|_| "control lock poisoned")?
            .pairing_store
            .clone()
            .ok_or("pairing store unavailable")?;
        if request.action == "recover" {
            if !matches!(
                manager.status().phase.as_str(),
                "idle" | "failed" | "cancelled" | "complete"
            ) {
                return Err("请先结束当前配对，再恢复已保存的记录".into());
            }
            let peer = store
                .commit_pending_enrollment()?
                .ok_or("没有待恢复的配对记录")?;
            self.control_plane
                .lock()
                .map_err(|_| "control lock poisoned")?
                .attach_pairing_store(store)?;
            manager.note_local_recovery(peer.node_id, peer.display_name);
            return Ok(manager.status());
        }
        if store.pending_enrollment()?.is_some() {
            return Err("存在已确认的配对恢复记录，请先恢复，不要生成另一套证书".into());
        }
        let paired = store
            .load_directory()
            .map_err(|e| e.to_string())?
            .peers()
            .map(|p| p.node_id)
            .collect();
        let host: IpAddr = request
            .host
            .ok_or("missing IP address")?
            .parse()
            .map_err(|_| "请输入具体 IP 地址")?;
        if request.action == "discover" {
            let IpAddr::V4(interface) = host else {
                return Err("局域网发现需要 IPv4 地址".into());
            };
            return manager.discover(id, interface);
        }
        let port = request.port.filter(|p| *p != 0).ok_or("请输入配对端口")?;
        if request.action == "welcome" {
            manager.welcome(id, name, host, port, paired, "windows-dpapi:")
        } else {
            manager.join(
                id,
                name,
                SocketAddr::new(host, port),
                request.pin.ok_or("missing PIN")?,
                paired,
                "windows-dpapi:",
            )
        }
    }

    pub fn enrollment_status(&self) -> Result<EnrollmentStatus, String> {
        let manager = self
            .enrollment
            .lock()
            .map_err(|_| "enrollment lock poisoned")?;
        let mut status = manager.status();
        if matches!(
            status.phase.as_str(),
            "idle" | "failed" | "cancelled" | "complete"
        ) {
            let control = self
                .control_plane
                .lock()
                .map_err(|_| "control lock poisoned")?;
            if let Some(store) = &control.pairing_store
                && let Some(pending) = store.pending_enrollment()?
                && let Some(peer) = pending.peers().next()
            {
                status.phase = "recovery_pending".into();
                status.peer_node_id = Some(peer.node_id.to_string());
                status.peer_name = Some(peer.display_name.clone());
                status.message =
                "发现双方确认后保留的本机恢复记录。可继续使用原证书，恢复不会覆盖不同的现有配对。"
                    .into();
            }
        }
        Ok(status)
    }
}
