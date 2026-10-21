//! Bounded handoff of validated control statuses to capability coordinators.
//! Each capability keeps its own inbox; Runtime still validates Route/epoch state.
use capyio_core::{RouteId, SessionId};
use capyio_protocol::ValidatedRouteStatus;
use std::{collections::BTreeMap, sync::Mutex};

pub(super) struct RouteStatusInbox {
    pending: Mutex<BTreeMap<(SessionId, RouteId), ValidatedRouteStatus>>,
}

impl RouteStatusInbox {
    pub const fn new() -> Self {
        Self {
            pending: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn defer(&self, status: &ValidatedRouteStatus) -> bool {
        let key = (status.session_id, status.route.id);
        if let Ok(mut pending) = self.pending.lock()
            && (pending.len() < 32 || pending.contains_key(&key))
        {
            pending.insert(key, status.clone());
            return true;
        }
        false
    }

    pub fn drain(&self, session: SessionId) -> Vec<ValidatedRouteStatus> {
        let Ok(mut pending) = self.pending.lock() else {
            return vec![];
        };
        let keys = pending
            .keys()
            .filter(|(owner, _)| *owner == session)
            .copied()
            .collect::<Vec<_>>();
        keys.into_iter()
            .filter_map(|key| pending.remove(&key))
            .collect()
    }

    pub fn remove(&self, session: SessionId, route: RouteId) {
        if let Ok(mut pending) = self.pending.lock() {
            pending.remove(&(session, route));
        }
    }

    pub fn remove_session(&self, session: SessionId) {
        if let Ok(mut pending) = self.pending.lock() {
            pending.retain(|(owner, _), _| *owner != session);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DesktopNodeState;
    use capyio_core::MessageId;

    #[test]
    fn bounded_handoff_preserves_session_isolation_and_latest_status() {
        let node = DesktopNodeState::demo().unwrap();
        let mut status = ValidatedRouteStatus {
            message_id: MessageId::new(),
            session_id: SessionId::new(),
            route: node
                .runtime
                .route(node.demo_routes.phone_microphone_to_windows)
                .unwrap()
                .clone(),
            touchpad_data: None,
            haptics_data: None,
            native_audio_data: None,
            camera_data: None,
            secure_camera_data: None,
        };
        let inbox = RouteStatusInbox::new();
        let first_session = status.session_id;
        let first_route = status.route.id;
        assert!(inbox.defer(&status));
        status.session_id = SessionId::new();
        for _ in 0..31 {
            status.route.id = RouteId::new();
            assert!(inbox.defer(&status));
        }
        status.message_id = MessageId::new();
        assert!(inbox.defer(&status)); // Replacing the same key works at capacity.
        let latest_message = status.message_id;
        let latest_route = status.route.id;
        status.route.id = RouteId::new();
        assert!(!inbox.defer(&status));
        let first = inbox.drain(first_session);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].route.id, first_route);
        let remaining = inbox.drain(status.session_id);
        assert_eq!(remaining.len(), 31);
        assert_eq!(
            remaining
                .iter()
                .find(|s| s.route.id == latest_route)
                .unwrap()
                .message_id,
            latest_message
        );
        assert!(inbox.drain(status.session_id).is_empty());

        assert!(inbox.defer(&status));
        inbox.remove(first_session, status.route.id);
        assert_eq!(inbox.drain(status.session_id).len(), 1);
        assert!(inbox.defer(&status));
        inbox.remove(status.session_id, status.route.id);
        assert!(inbox.drain(status.session_id).is_empty());
        assert!(inbox.defer(&status));
        inbox.remove_session(first_session);
        assert_eq!(inbox.drain(status.session_id).len(), 1);
        assert!(inbox.defer(&status));
        inbox.remove_session(status.session_id);
        assert!(inbox.drain(status.session_id).is_empty());
    }
}
