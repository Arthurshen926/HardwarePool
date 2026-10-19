//! Preferences belong to the publishing Node, not to a dialing/listening role.
use capyio_protocol::v1::{ControllerSettingsRequest, ControllerSettingsState};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiControllerSettings {
    node_id: String,
    revision: u64,
    motion_enabled: bool,
    haptics_strength: u32,
    pending: bool,
    error: Option<String>,
}
struct Peer {
    session: String,
    state: Option<UiControllerSettings>,
    queued: Option<ControllerSettingsRequest>,
    pending: Option<(u64, Instant)>,
}
fn peers() -> &'static Mutex<BTreeMap<String, Peer>> {
    static PEERS: OnceLock<Mutex<BTreeMap<String, Peer>>> = OnceLock::new();
    PEERS.get_or_init(Mutex::default)
}
pub fn connected(node: &str, session: &str) {
    if let Ok(mut peers) = peers().lock() {
        peers.insert(
            node.to_owned(),
            Peer {
                session: session.to_owned(),
                state: None,
                queued: None,
                pending: None,
            },
        );
    }
}
pub fn disconnected(node: &str, session: &str) {
    if let Ok(mut peers) = peers().lock()
        && peers.get(node).is_some_and(|peer| peer.session == session)
    {
        peers.remove(node);
    }
}
pub fn outgoing(node: &str, session: &str) -> Option<ControllerSettingsRequest> {
    let mut peers = peers().lock().ok()?;
    let peer = peers.get_mut(node).filter(|peer| peer.session == session)?;
    peer.queued.take()
}
pub fn received(node: &str, session: &str, value: ControllerSettingsState) {
    let Ok(mut peers) = peers().lock() else {
        return;
    };
    let Some(peer) = peers.get_mut(node).filter(|peer| peer.session == session) else {
        return;
    };
    let acknowledged = peer.pending.is_some_and(|(id, _)| id == value.request_id);
    if acknowledged {
        peer.pending = None;
    }
    peer.state = Some(UiControllerSettings {
        node_id: node.to_owned(),
        revision: value.revision,
        motion_enabled: value.motion_enabled,
        haptics_strength: value.haptics_strength,
        pending: peer.pending.is_some(),
        error: if acknowledged && !value.error.is_empty() {
            Some(value.error)
        } else {
            None
        },
    });
}
pub fn get_remote_controller_settings(
    node_id: String,
) -> Result<Option<UiControllerSettings>, String> {
    let mut peers = peers()
        .lock()
        .map_err(|_| "controller settings lock poisoned")?;
    let Some(peer) = peers.get_mut(&node_id) else {
        return Ok(None);
    };
    if peer
        .pending
        .is_some_and(|(_, time)| time.elapsed() > Duration::from_secs(15))
    {
        peer.pending = None;
        peer.queued = None;
        if let Some(state) = peer.state.as_mut() {
            state.pending = false;
            state.error = Some("settings_ack_timeout".to_owned());
        }
    }
    Ok(peer.state.clone())
}
pub fn set_remote_controller_settings(
    node_id: String,
    expected_revision: u64,
    motion_enabled: bool,
    haptics_strength: u32,
) -> Result<(), String> {
    if haptics_strength > 100 {
        return Err("haptics strength must be 0..100".to_owned());
    }
    let mut peers = peers()
        .lock()
        .map_err(|_| "controller settings lock poisoned")?;
    let peer = peers.get_mut(&node_id).ok_or("peer is disconnected")?;
    let state = peer
        .state
        .as_mut()
        .ok_or("peer has not published controller settings")?;
    if state.revision != expected_revision {
        return Err("settings_revision_conflict".to_owned());
    }
    if peer.pending.is_some() {
        return Err("settings_busy".to_owned());
    }
    // Do not reuse request 1 after a desktop restart while the owner still
    // advertises an acknowledgement from the previous process/session.
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let clock_id = u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "system clock precedes epoch")?
            .as_micros(),
    )
    .map_err(|_| "settings request clock overflow")?;
    let previous = NEXT.fetch_max(clock_id, Ordering::Relaxed);
    let request_id = NEXT.fetch_add(1, Ordering::Relaxed).max(previous);
    peer.queued = Some(ControllerSettingsRequest {
        request_id,
        expected_revision,
        motion_enabled,
        haptics_strength,
    });
    peer.pending = Some((request_id, Instant::now()));
    state.pending = true;
    state.error = None;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn controller_settings_requires_applied_ack_and_clears_on_disconnect() {
        let node = "settings-test-node";
        connected(node, "session-1");
        assert!(
            get_remote_controller_settings(node.to_owned())
                .unwrap()
                .is_none()
        );
        received(
            node,
            "session-1",
            ControllerSettingsState {
                revision: 1,
                motion_enabled: true,
                haptics_strength: 70,
                request_id: 0,
                error: String::new(),
            },
        );
        assert!(set_remote_controller_settings(node.to_owned(), 2, false, 20).is_err());
        set_remote_controller_settings(node.to_owned(), 1, false, 20).unwrap();
        let pending = get_remote_controller_settings(node.to_owned())
            .unwrap()
            .unwrap();
        assert!(pending.pending && pending.motion_enabled && pending.haptics_strength == 70);
        let request = outgoing(node, "session-1").unwrap();
        received(
            node,
            "session-1",
            ControllerSettingsState {
                revision: 2,
                motion_enabled: false,
                haptics_strength: 20,
                request_id: request.request_id,
                error: String::new(),
            },
        );
        let applied = get_remote_controller_settings(node.to_owned())
            .unwrap()
            .unwrap();
        assert!(!applied.pending && !applied.motion_enabled && applied.haptics_strength == 20);
        disconnected(node, "session-1");
        assert!(
            get_remote_controller_settings(node.to_owned())
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn controller_settings_new_session_ignores_old_cleanup_and_state() {
        let node = "settings-reconnect-test-node";
        connected(node, "old");
        connected(node, "new");
        disconnected(node, "old");
        received(
            node,
            "old",
            ControllerSettingsState {
                revision: 9,
                motion_enabled: true,
                haptics_strength: 10,
                request_id: 0,
                error: String::new(),
            },
        );
        assert!(
            get_remote_controller_settings(node.to_owned())
                .unwrap()
                .is_none()
        );
        received(
            node,
            "new",
            ControllerSettingsState {
                revision: 10,
                motion_enabled: false,
                haptics_strength: 20,
                request_id: 0,
                error: String::new(),
            },
        );
        assert_eq!(
            get_remote_controller_settings(node.to_owned())
                .unwrap()
                .unwrap()
                .revision,
            10
        );
        disconnected(node, "new");
    }
}
