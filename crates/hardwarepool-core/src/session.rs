use std::collections::{BTreeMap, btree_map::Entry};

use serde::{Deserialize, Serialize};

use crate::{
    AudioFormat, BindingId, CapabilityDescriptor, CapabilityId, CoreError, NodeId, ProjectionId,
    ProjectionKind, SessionId, StreamId,
};

/// Overall control-session phase. Capability bindings remain independently stateful.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionPhase {
    Ready,
    Suspended,
    Closing,
    Closed,
    Failed,
}

/// Lifecycle of one Capability-to-Projection binding.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingState {
    Requested,
    Authorized,
    Negotiated,
    Starting,
    Active,
    Suspended,
    Stopping,
    Stopped,
    Rejected,
    Offline,
    Failed,
}

impl BindingState {
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Stopped | Self::Rejected | Self::Failed)
    }
}

/// State and negotiated configuration for one independently controlled capability.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CapabilityBinding {
    pub id: BindingId,
    pub capability_id: CapabilityId,
    pub projection_id: ProjectionId,
    pub projection_kind: ProjectionKind,
    pub stream_id: StreamId,
    pub state: BindingState,
    pub selected_audio_format: Option<AudioFormat>,
    pub lease_expires_at_ms: Option<u64>,
    pub stream_epoch: u32,
    pub last_error: Option<String>,
}

/// Deterministic per-peer session state machine.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub local_node_id: NodeId,
    pub remote_node_id: NodeId,
    pub phase: SessionPhase,
    pub bindings: BTreeMap<CapabilityId, CapabilityBinding>,
}

impl Session {
    #[must_use]
    pub fn new(local_node_id: NodeId, remote_node_id: NodeId) -> Self {
        Self {
            id: SessionId::new(),
            local_node_id,
            remote_node_id,
            phase: SessionPhase::Ready,
            bindings: BTreeMap::new(),
        }
    }

    pub fn request_binding(
        &mut self,
        capability: &CapabilityDescriptor,
        projection_kind: ProjectionKind,
    ) -> Result<BindingId, CoreError> {
        self.require_ready("request_binding")?;

        if !capability.supports_projection(projection_kind) {
            return Err(CoreError::UnsupportedProjection {
                capability_id: capability.id,
                mapping: projection_kind,
            });
        }

        capability.validate()?;

        let binding = CapabilityBinding {
            id: BindingId::new(),
            capability_id: capability.id,
            projection_id: ProjectionId::new(),
            projection_kind,
            stream_id: StreamId::new(),
            state: BindingState::Requested,
            selected_audio_format: None,
            lease_expires_at_ms: None,
            stream_epoch: 0,
            last_error: None,
        };
        let id = binding.id;
        match self.bindings.entry(capability.id) {
            Entry::Occupied(mut entry) if entry.get().state.is_terminal() => {
                let _previous = entry.insert(binding);
                Ok(id)
            }
            Entry::Occupied(_) => Err(CoreError::BindingAlreadyExists(capability.id)),
            Entry::Vacant(entry) => {
                entry.insert(binding);
                Ok(id)
            }
        }
    }

    pub fn authorize(
        &mut self,
        capability_id: CapabilityId,
        issued_at_ms: u64,
        expires_at_ms: u64,
    ) -> Result<(), CoreError> {
        if expires_at_ms <= issued_at_ms {
            return Err(CoreError::InvalidLease);
        }
        let binding = self.binding_mut(capability_id)?;
        require_binding_state(binding.state, &[BindingState::Requested], "authorize")?;
        binding.lease_expires_at_ms = Some(expires_at_ms);
        binding.state = BindingState::Authorized;
        Ok(())
    }

    pub fn reject(
        &mut self,
        capability_id: CapabilityId,
        reason: impl Into<String>,
    ) -> Result<(), CoreError> {
        let binding = self.binding_mut(capability_id)?;
        require_binding_state(binding.state, &[BindingState::Requested], "reject")?;
        binding.last_error = Some(reason.into());
        binding.state = BindingState::Rejected;
        Ok(())
    }

    pub fn negotiate_audio(
        &mut self,
        capability: &CapabilityDescriptor,
        selected_format: AudioFormat,
        now_ms: u64,
    ) -> Result<(), CoreError> {
        selected_format.validate()?;
        let spec = capability
            .audio_spec()
            .ok_or(CoreError::NotAudioCapability(capability.id))?;
        if !spec.supports_format(&selected_format) {
            return Err(CoreError::UnsupportedAudioFormat(capability.id));
        }

        let binding = self.binding_mut(capability.id)?;
        require_binding_state(
            binding.state,
            &[BindingState::Authorized],
            "negotiate_audio",
        )?;
        ensure_lease(binding, now_ms)?;
        binding.selected_audio_format = Some(selected_format);
        binding.state = BindingState::Negotiated;
        Ok(())
    }

    pub fn begin_start(
        &mut self,
        capability_id: CapabilityId,
        now_ms: u64,
    ) -> Result<(), CoreError> {
        self.require_ready("begin_start")?;
        let binding = self.binding_mut(capability_id)?;
        require_binding_state(
            binding.state,
            &[BindingState::Negotiated, BindingState::Stopped],
            "begin_start",
        )?;
        ensure_lease(binding, now_ms)?;
        binding.stream_epoch = binding.stream_epoch.saturating_add(1);
        binding.stream_id = StreamId::new();
        binding.last_error = None;
        binding.state = BindingState::Starting;
        Ok(())
    }

    pub fn mark_active(&mut self, capability_id: CapabilityId) -> Result<(), CoreError> {
        let binding = self.binding_mut(capability_id)?;
        require_binding_state(binding.state, &[BindingState::Starting], "mark_active")?;
        binding.state = BindingState::Active;
        Ok(())
    }

    pub fn suspend_binding(&mut self, capability_id: CapabilityId) -> Result<(), CoreError> {
        let binding = self.binding_mut(capability_id)?;
        require_binding_state(binding.state, &[BindingState::Active], "suspend_binding")?;
        binding.state = BindingState::Suspended;
        Ok(())
    }

    pub fn resume_binding(
        &mut self,
        capability_id: CapabilityId,
        now_ms: u64,
    ) -> Result<(), CoreError> {
        self.require_ready("resume_binding")?;
        let binding = self.binding_mut(capability_id)?;
        require_binding_state(binding.state, &[BindingState::Suspended], "resume_binding")?;
        ensure_lease(binding, now_ms)?;
        binding.stream_epoch = binding.stream_epoch.saturating_add(1);
        binding.stream_id = StreamId::new();
        binding.state = BindingState::Starting;
        Ok(())
    }

    pub fn begin_stop(&mut self, capability_id: CapabilityId) -> Result<(), CoreError> {
        let binding = self.binding_mut(capability_id)?;
        require_binding_state(
            binding.state,
            &[
                BindingState::Starting,
                BindingState::Active,
                BindingState::Suspended,
                BindingState::Offline,
            ],
            "begin_stop",
        )?;
        binding.state = BindingState::Stopping;
        Ok(())
    }

    pub fn mark_stopped(&mut self, capability_id: CapabilityId) -> Result<(), CoreError> {
        let binding = self.binding_mut(capability_id)?;
        require_binding_state(binding.state, &[BindingState::Stopping], "mark_stopped")?;
        binding.state = BindingState::Stopped;
        Ok(())
    }

    /// Cancels a binding before it becomes active.
    pub fn cancel_binding(&mut self, capability_id: CapabilityId) -> Result<(), CoreError> {
        let binding = self.binding_mut(capability_id)?;
        require_binding_state(
            binding.state,
            &[
                BindingState::Requested,
                BindingState::Authorized,
                BindingState::Negotiated,
            ],
            "cancel_binding",
        )?;
        binding.state = BindingState::Stopped;
        binding.selected_audio_format = None;
        Ok(())
    }

    pub fn fail_binding(
        &mut self,
        capability_id: CapabilityId,
        reason: impl Into<String>,
    ) -> Result<(), CoreError> {
        let binding = self.binding_mut(capability_id)?;
        binding.last_error = Some(reason.into());
        binding.state = BindingState::Failed;
        Ok(())
    }

    /// Marks all non-terminal live bindings offline after peer loss.
    pub fn mark_remote_offline(&mut self) -> Result<(), CoreError> {
        match self.phase {
            SessionPhase::Ready | SessionPhase::Suspended => {}
            phase => {
                return Err(CoreError::InvalidSessionTransition {
                    from: phase,
                    action: "mark_remote_offline",
                });
            }
        }

        self.phase = SessionPhase::Suspended;
        for binding in self.bindings.values_mut() {
            if matches!(
                binding.state,
                BindingState::Authorized
                    | BindingState::Negotiated
                    | BindingState::Starting
                    | BindingState::Active
                    | BindingState::Suspended
                    | BindingState::Stopping
            ) {
                binding.state = BindingState::Offline;
                binding.stream_id = StreamId::new();
            }
        }
        Ok(())
    }

    /// Restores the control session. Offline streams require an explicit fresh start.
    pub fn mark_remote_online(&mut self) -> Result<(), CoreError> {
        if self.phase != SessionPhase::Suspended {
            return Err(CoreError::InvalidSessionTransition {
                from: self.phase,
                action: "mark_remote_online",
            });
        }
        self.phase = SessionPhase::Ready;
        for binding in self.bindings.values_mut() {
            if binding.state == BindingState::Offline {
                binding.state = BindingState::Stopped;
            }
        }
        Ok(())
    }

    pub fn begin_close(&mut self) -> Result<(), CoreError> {
        match self.phase {
            SessionPhase::Ready | SessionPhase::Suspended | SessionPhase::Failed => {
                self.phase = SessionPhase::Closing;
                Ok(())
            }
            phase => Err(CoreError::InvalidSessionTransition {
                from: phase,
                action: "begin_close",
            }),
        }
    }

    pub fn mark_closed(&mut self) -> Result<(), CoreError> {
        if self.phase != SessionPhase::Closing {
            return Err(CoreError::InvalidSessionTransition {
                from: self.phase,
                action: "mark_closed",
            });
        }
        self.phase = SessionPhase::Closed;
        for binding in self.bindings.values_mut() {
            if !binding.state.is_terminal() {
                binding.state = BindingState::Stopped;
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn active_binding_count(&self) -> usize {
        self.bindings
            .values()
            .filter(|binding| binding.state == BindingState::Active)
            .count()
    }

    pub fn binding(&self, capability_id: CapabilityId) -> Result<&CapabilityBinding, CoreError> {
        self.bindings
            .get(&capability_id)
            .ok_or(CoreError::UnknownCapability(capability_id))
    }

    fn binding_mut(
        &mut self,
        capability_id: CapabilityId,
    ) -> Result<&mut CapabilityBinding, CoreError> {
        self.bindings
            .get_mut(&capability_id)
            .ok_or(CoreError::UnknownCapability(capability_id))
    }

    fn require_ready(&self, action: &'static str) -> Result<(), CoreError> {
        if self.phase == SessionPhase::Ready {
            Ok(())
        } else {
            Err(CoreError::InvalidSessionTransition {
                from: self.phase,
                action,
            })
        }
    }
}

fn require_binding_state(
    actual: BindingState,
    allowed: &[BindingState],
    action: &'static str,
) -> Result<(), CoreError> {
    if allowed.contains(&actual) {
        Ok(())
    } else {
        Err(CoreError::InvalidBindingTransition {
            from: actual,
            action,
        })
    }
}

fn ensure_lease(binding: &CapabilityBinding, now_ms: u64) -> Result<(), CoreError> {
    match binding.lease_expires_at_ms {
        Some(expiry) if expiry > now_ms => Ok(()),
        Some(_) => Err(CoreError::LeaseExpired),
        None => Err(CoreError::InvalidLease),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::{
        AudioCapabilitySpec, AudioProcessingSupport, AudioQosMode, Availability, CapabilityDetails,
        CapabilityKind, LocalRole, PermissionRequirement, ProfileId, StreamRole,
    };

    fn speaker() -> CapabilityDescriptor {
        CapabilityDescriptor {
            id: CapabilityId::new(),
            display_name: "Phone speaker".to_owned(),
            profile: ProfileId::audio_render_v1(),
            kind: CapabilityKind::AudioRender,
            local_role: LocalRole::Render,
            stream_role: StreamRole::Consumer,
            supported_projections: BTreeSet::from([
                ProjectionKind::ApplicationStream,
                ProjectionKind::SystemRenderEndpoint,
            ]),
            permission_requirement: PermissionRequirement::UserConfirmation,
            availability: Availability::Available,
            details: CapabilityDetails::Audio(AudioCapabilitySpec {
                formats: vec![AudioFormat::speaker_baseline()],
                qos_modes: vec![AudioQosMode::MediaPlayback],
                processing: AudioProcessingSupport::default(),
                supports_volume_control: true,
                supports_mute: true,
            }),
        }
    }

    fn activate(session: &mut Session, capability: &CapabilityDescriptor) {
        session
            .request_binding(capability, ProjectionKind::SystemRenderEndpoint)
            .expect("request");
        session
            .authorize(capability.id, 10, 10_000)
            .expect("authorize");
        session
            .negotiate_audio(capability, AudioFormat::speaker_baseline(), 20)
            .expect("negotiate");
        session.begin_start(capability.id, 30).expect("start");
        session.mark_active(capability.id).expect("active");
    }

    #[test]
    fn complete_audio_binding_lifecycle() {
        let capability = speaker();
        let mut session = Session::new(NodeId::new(), NodeId::new());
        activate(&mut session, &capability);
        assert_eq!(session.active_binding_count(), 1);

        session.begin_stop(capability.id).expect("begin stop");
        session.mark_stopped(capability.id).expect("stopped");
        assert_eq!(session.active_binding_count(), 0);
    }

    #[test]
    fn expired_lease_prevents_start() {
        let capability = speaker();
        let mut session = Session::new(NodeId::new(), NodeId::new());
        session
            .request_binding(&capability, ProjectionKind::SystemRenderEndpoint)
            .expect("request");
        session.authorize(capability.id, 10, 20).expect("authorize");
        session
            .negotiate_audio(&capability, AudioFormat::speaker_baseline(), 15)
            .expect("negotiate");

        assert_eq!(
            session.begin_start(capability.id, 20),
            Err(CoreError::LeaseExpired)
        );
    }

    #[test]
    fn remote_reconnect_requires_fresh_stream_epoch() {
        let capability = speaker();
        let mut session = Session::new(NodeId::new(), NodeId::new());
        activate(&mut session, &capability);
        let old_stream = session.binding(capability.id).expect("binding").stream_id;

        session.mark_remote_offline().expect("offline");
        session.mark_remote_online().expect("online");
        assert_eq!(
            session.binding(capability.id).expect("binding").state,
            BindingState::Stopped
        );

        session.begin_start(capability.id, 100).expect("restart");
        let new_stream = session.binding(capability.id).expect("binding").stream_id;
        assert_ne!(old_stream, new_stream);
    }
}
