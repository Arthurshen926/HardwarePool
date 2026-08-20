use std::collections::{BTreeMap, VecDeque};

use hardwarepool_core::{
    AudioFormat, BindingState, CapabilityDescriptor, CapabilityId, NodeDescriptor, NodeId,
    ProjectionKind, Session, SessionId, SessionPhase,
};
use serde::{Deserialize, Serialize};

use crate::{
    HostOperation, HostOperationCompletion, OperationId, OperationRecord, OperationRegistry,
    OperationStatus, OperationUpdate, PeerSnapshot, RuntimeError, RuntimeEvent, RuntimeEventKind,
    RuntimeSnapshot,
};

const MAX_RETAINED_EVENTS: usize = 256;
const DEMO_LEASE_DURATION_MS: u64 = 60 * 60 * 1_000;

/// Runtime record for one known peer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PeerRecord {
    pub descriptor: NodeDescriptor,
    pub online: bool,
}

/// OS-independent owner of peers, sessions and deterministic lifecycle commands.
#[derive(Clone, Debug)]
pub struct NodeRuntime {
    local_node: NodeDescriptor,
    peers: BTreeMap<NodeId, PeerRecord>,
    sessions: BTreeMap<SessionId, Session>,
    operations: OperationRegistry,
    events: VecDeque<RuntimeEvent>,
    next_event_sequence: u64,
}

impl NodeRuntime {
    pub fn new(local_node: NodeDescriptor) -> Result<Self, RuntimeError> {
        local_node.validate()?;
        Ok(Self {
            local_node,
            peers: BTreeMap::new(),
            sessions: BTreeMap::new(),
            operations: OperationRegistry::default(),
            events: VecDeque::new(),
            next_event_sequence: 1,
        })
    }

    pub fn register_peer(
        &mut self,
        descriptor: NodeDescriptor,
        online: bool,
    ) -> Result<(), RuntimeError> {
        descriptor.validate()?;
        let peer_id = descriptor.id;
        let _previous = self
            .peers
            .insert(peer_id, PeerRecord { descriptor, online });
        self.emit(RuntimeEventKind::PeerRegistered { peer_id });
        Ok(())
    }

    pub fn open_session(&mut self, peer_id: NodeId) -> Result<SessionId, RuntimeError> {
        let peer = self
            .peers
            .get(&peer_id)
            .ok_or(RuntimeError::UnknownPeer(peer_id))?;
        if !peer.online {
            return Err(RuntimeError::PeerOffline(peer_id));
        }

        let session = Session::new(self.local_node.id, peer_id);
        let session_id = session.id;
        let previous = self.sessions.insert(session_id, session);
        debug_assert!(previous.is_none(), "fresh SessionId must be unique");
        self.emit(RuntimeEventKind::SessionOpened {
            session_id,
            peer_id,
        });
        Ok(session_id)
    }

    /// Activates an audio projection through request, authorization, negotiation and start.
    ///
    /// This convenience method is for the deterministic bootstrap UI/demo. Production hosts
    /// will drive these transitions from authenticated peer messages and platform completions.
    pub fn activate_audio_projection(
        &mut self,
        session_id: SessionId,
        capability_id: CapabilityId,
        projection_kind: ProjectionKind,
        now_ms: u64,
    ) -> Result<(), RuntimeError> {
        let (peer_id, phase) = {
            let session = self.session(session_id)?;
            (session.remote_node_id, session.phase)
        };
        if phase != SessionPhase::Ready {
            return Err(hardwarepool_core::CoreError::InvalidSessionTransition {
                from: phase,
                action: "activate_audio_projection",
            }
            .into());
        }

        let peer = self
            .peers
            .get(&peer_id)
            .ok_or(RuntimeError::UnknownPeer(peer_id))?;
        if !peer.online {
            return Err(RuntimeError::PeerOffline(peer_id));
        }
        let capability = peer
            .descriptor
            .capabilities
            .get(&capability_id)
            .cloned()
            .ok_or(RuntimeError::CapabilityNotAdvertised {
                peer_id,
                capability_id,
            })?;
        let selected_format = first_audio_format(&capability)?;

        {
            let session = self.session_mut(session_id)?;
            let state = session
                .bindings
                .get(&capability_id)
                .map(|binding| binding.state);

            match state {
                None | Some(BindingState::Rejected | BindingState::Failed) => {
                    session.request_binding(&capability, projection_kind)?;
                }
                Some(BindingState::Active) => return Ok(()),
                Some(BindingState::Offline) => return Err(RuntimeError::PeerOffline(peer_id)),
                _ => {}
            }

            let state = session.binding(capability_id)?.state;
            if state == BindingState::Requested {
                session.authorize(
                    capability_id,
                    now_ms,
                    now_ms.saturating_add(DEMO_LEASE_DURATION_MS),
                )?;
            }

            let state = session.binding(capability_id)?.state;
            if state == BindingState::Authorized {
                session.negotiate_audio(&capability, selected_format, now_ms)?;
            }

            let state = session.binding(capability_id)?.state;
            match state {
                BindingState::Negotiated | BindingState::Stopped => {
                    session.begin_start(capability_id, now_ms)?;
                    session.mark_active(capability_id)?;
                }
                BindingState::Suspended => {
                    session.resume_binding(capability_id, now_ms)?;
                    session.mark_active(capability_id)?;
                }
                BindingState::Starting => session.mark_active(capability_id)?,
                BindingState::Stopping => {
                    session.mark_stopped(capability_id)?;
                    session.begin_start(capability_id, now_ms)?;
                    session.mark_active(capability_id)?;
                }
                BindingState::Active => {}
                other => {
                    return Err(hardwarepool_core::CoreError::InvalidBindingTransition {
                        from: other,
                        action: "activate_audio_projection",
                    }
                    .into());
                }
            }
        }

        self.emit_binding_state(session_id, capability_id)?;
        Ok(())
    }

    pub fn deactivate_projection(
        &mut self,
        session_id: SessionId,
        capability_id: CapabilityId,
    ) -> Result<(), RuntimeError> {
        {
            let session = self.session_mut(session_id)?;
            let state = session.binding(capability_id)?.state;
            match state {
                BindingState::Active
                | BindingState::Starting
                | BindingState::Suspended
                | BindingState::Offline => {
                    session.begin_stop(capability_id)?;
                    session.mark_stopped(capability_id)?;
                }
                BindingState::Requested | BindingState::Authorized | BindingState::Negotiated => {
                    session.cancel_binding(capability_id)?;
                }
                BindingState::Stopping => session.mark_stopped(capability_id)?,
                BindingState::Stopped | BindingState::Rejected | BindingState::Failed => {
                    return Ok(());
                }
            }
        }
        self.emit_binding_state(session_id, capability_id)?;
        Ok(())
    }

    pub fn set_peer_online(&mut self, peer_id: NodeId, online: bool) -> Result<(), RuntimeError> {
        let peer = self
            .peers
            .get_mut(&peer_id)
            .ok_or(RuntimeError::UnknownPeer(peer_id))?;
        if peer.online == online {
            return Ok(());
        }
        peer.online = online;

        let affected_sessions: Vec<SessionId> = self
            .sessions
            .values()
            .filter(|session| session.remote_node_id == peer_id)
            .map(|session| session.id)
            .collect();

        for session_id in affected_sessions {
            let phase = {
                let session = self.session_mut(session_id)?;
                if online {
                    if session.phase == SessionPhase::Suspended {
                        session.mark_remote_online()?;
                    }
                } else if matches!(session.phase, SessionPhase::Ready | SessionPhase::Suspended) {
                    session.mark_remote_offline()?;
                }
                session.phase
            };
            self.emit(RuntimeEventKind::SessionPhaseChanged { session_id, phase });
        }

        self.emit(RuntimeEventKind::PeerOnlineChanged { peer_id, online });
        Ok(())
    }

    /// Registers asynchronous host work without exposing mutable Core state to callbacks.
    pub fn begin_host_operation(
        &mut self,
        operation: HostOperation,
    ) -> Result<OperationId, RuntimeError> {
        let id = self.operations.begin(operation)?;
        self.emit(RuntimeEventKind::OperationChanged {
            operation_id: id,
            status: OperationStatus::Pending,
        });
        Ok(id)
    }

    /// Applies a typed host completion. The first terminal transition wins a race.
    pub fn complete_host_operation(
        &mut self,
        id: OperationId,
        completion: HostOperationCompletion,
    ) -> Result<OperationUpdate, RuntimeError> {
        let update = self.operations.complete(id, completion)?;
        self.emit_operation_update(id, update);
        Ok(update)
    }

    pub fn cancel_host_operation(
        &mut self,
        id: OperationId,
    ) -> Result<OperationUpdate, RuntimeError> {
        let update = self.operations.cancel(id)?;
        self.emit_operation_update(id, update);
        Ok(update)
    }

    pub fn dispose_host_operation(
        &mut self,
        id: OperationId,
    ) -> Result<OperationUpdate, RuntimeError> {
        let update = self.operations.dispose(id)?;
        self.emit_operation_update(id, update);
        Ok(update)
    }

    pub fn host_operation(&self, id: OperationId) -> Result<&OperationRecord, RuntimeError> {
        self.operations.record(id)
    }

    #[must_use]
    pub fn snapshot(&self) -> RuntimeSnapshot {
        RuntimeSnapshot {
            local_node: self.local_node.clone(),
            peers: self
                .peers
                .values()
                .cloned()
                .map(|peer| PeerSnapshot {
                    descriptor: peer.descriptor,
                    online: peer.online,
                })
                .collect(),
            sessions: self.sessions.values().cloned().collect(),
            operations: self.operations.records().cloned().collect(),
            events: self.events.iter().cloned().collect(),
        }
    }

    pub fn session(&self, session_id: SessionId) -> Result<&Session, RuntimeError> {
        self.sessions
            .get(&session_id)
            .ok_or(RuntimeError::UnknownSession(session_id))
    }

    fn session_mut(&mut self, session_id: SessionId) -> Result<&mut Session, RuntimeError> {
        self.sessions
            .get_mut(&session_id)
            .ok_or(RuntimeError::UnknownSession(session_id))
    }

    fn emit_binding_state(
        &mut self,
        session_id: SessionId,
        capability_id: CapabilityId,
    ) -> Result<(), RuntimeError> {
        let (projection_kind, state) = {
            let binding = self.session(session_id)?.binding(capability_id)?;
            (binding.projection_kind, binding.state)
        };
        self.emit(RuntimeEventKind::BindingChanged {
            session_id,
            capability_id,
            projection_kind,
            state,
        });
        Ok(())
    }

    fn emit_operation_update(&mut self, id: OperationId, update: OperationUpdate) {
        if let OperationUpdate::Applied(status) = update {
            self.emit(RuntimeEventKind::OperationChanged {
                operation_id: id,
                status,
            });
        }
    }

    fn emit(&mut self, kind: RuntimeEventKind) {
        let event = RuntimeEvent {
            sequence: self.next_event_sequence,
            kind,
        };
        self.next_event_sequence = self.next_event_sequence.saturating_add(1);
        self.events.push_back(event);
        while self.events.len() > MAX_RETAINED_EVENTS {
            let _discarded = self.events.pop_front();
        }
    }
}

fn first_audio_format(capability: &CapabilityDescriptor) -> Result<AudioFormat, RuntimeError> {
    capability
        .audio_spec()
        .and_then(|spec| spec.formats.first())
        .cloned()
        .ok_or_else(|| hardwarepool_core::CoreError::NotAudioCapability(capability.id).into())
}
