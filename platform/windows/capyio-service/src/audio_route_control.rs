//! ADR 1501: opt-in SID-scoped handoff, separate from the all-IU legacy pipe.
//! Caller metadata is NOT cryptographic evidence: the configured principal's
//! Node Runtime owns consent, authenticated peer admission and Port leases.
use crate::{
    BrokerServiceRuntime, ServiceBrokerProcess,
    camera_control::camera_pipe_sddl,
    local_pipe::{NamedPipe, try_invoke, wake},
};
use capyio_audio::{AudioMediaStreamBinding, AudioStreamSpec};
use capyio_core::{Route, RouteId, Session, SessionId, StreamId};
use capyio_native_audio_lan::{NativeLanRouteBinding, NativeSpeakerSupervisorStatus};
use serde::{Deserialize, Serialize};
use std::{
    net::SocketAddrV4,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[cfg(not(test))]
const PIPE: &str = r"\\.\pipe\CapyIO.Audio.Route.Control.v1";
#[cfg(test)]
const PIPE: &str = r"\\.\pipe\CapyIO.Audio.Route.Control.v1.test";
const OWNER_BUDGET: Duration = Duration::from_secs(15);

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AudioRouteDirection {
    Microphone,
    Speaker,
}
impl AudioRouteDirection {
    fn index(self) -> usize {
        match self {
            Self::Microphone => 0,
            Self::Speaker => 1,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AudioRouteKey {
    pub direction: AudioRouteDirection,
    pub session_id: SessionId,
    pub route_id: RouteId,
    pub stream_id: StreamId,
    pub epoch: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AudioRouteStart {
    pub direction: AudioRouteDirection,
    pub route: Route,
    pub session: Session,
    pub media: AudioMediaStreamBinding,
    pub local: SocketAddrV4,
    pub peer: SocketAddrV4,
}
impl AudioRouteStart {
    pub fn key(&self) -> AudioRouteKey {
        AudioRouteKey {
            direction: self.direction,
            session_id: self.media.session_id,
            route_id: self.media.route_id,
            stream_id: self.media.stream_id,
            epoch: self.media.stream_epoch,
        }
    }
    fn validate(&self, now_ms: u64) -> Result<NativeLanRouteBinding, String> {
        let binding = NativeLanRouteBinding::from_live_context(
            self.media.clone(),
            &self.route,
            &self.session,
            now_ms,
        )
        .map_err(|_| "CAPY.AUDIO_ROUTE.INVALID_BINDING")?;
        let (source, sink, preset) = match self.direction {
            AudioRouteDirection::Microphone => (
                self.session.remote_node_id,
                self.session.local_node_id,
                AudioStreamSpec::voice_interactive(),
            ),
            AudioRouteDirection::Speaker => (
                self.session.local_node_id,
                self.session.remote_node_id,
                AudioStreamSpec::media_balanced(),
            ),
        };
        if self.route.source.node_id != source
            || self.route.sink.node_id != sink
            || source == sink
            || self.media.selected_spec != preset
        {
            return Err("CAPY.AUDIO_ROUTE.DIRECTION_OR_PRESET".into());
        }
        let args = match self.direction {
            AudioRouteDirection::Microphone => {
                capyio_native_audio_lan::NativeMicrophoneLaunch::route_arguments(
                    self.local, self.peer, &binding,
                )
            }
            AudioRouteDirection::Speaker => {
                capyio_native_audio_lan::NativeMicrophoneLaunch::speaker_route_arguments(
                    self.local, self.peer, &binding,
                )
            }
        }
        .map_err(|_| "CAPY.AUDIO_ROUTE.INVALID_ENDPOINT")?;
        match self.direction {
            AudioRouteDirection::Microphone => {
                capyio_native_audio_lan::NativeMicrophoneLaunch::parse(&args, now_ms)
            }
            AudioRouteDirection::Speaker => {
                capyio_native_audio_lan::NativeMicrophoneLaunch::parse_speaker(&args, now_ms)
            }
        }
        .map_err(|_| "CAPY.AUDIO_ROUTE.INVALID_ENDPOINT")?;
        Ok(binding)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    tag = "operation",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum AudioRouteOperation {
    /// Repeating exactly the same validated Start renews the 15-second owner
    /// lease, but never changes the worker's original finite grant deadline.
    Start(Box<AudioRouteStart>),
    Status(AudioRouteKey),
    Stop(AudioRouteKey),
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    schema_version: u8,
    request_id: u64,
    command: AudioRouteOperation,
}
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AudioRouteSnapshot {
    pub key: AudioRouteKey,
    /// Readiness only, never proof of received or rendered audio.
    pub worker_ready: bool,
    pub owner_lease_remaining_ms: u64,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Response {
    schema_version: u8,
    request_id: u64,
    snapshot: Option<AudioRouteSnapshot>,
    problem_code: Option<String>,
}

#[derive(Default)]
pub struct AudioRouteServiceClient {
    next_id: AtomicU64,
}
impl AudioRouteServiceClient {
    pub fn invoke(&self, command: AudioRouteOperation) -> Result<AudioRouteSnapshot, String> {
        let id = self
            .next_id
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |id| id.checked_add(1))
            .map_err(|_| "CAPY.AUDIO_ROUTE.REQUEST_EXHAUSTED")?
            + 1;
        let response: Response = try_invoke(
            PIPE,
            &Request {
                schema_version: 1,
                request_id: id,
                command,
            },
        )?;
        if response.schema_version != 1 || response.request_id != id {
            return Err("CAPY.AUDIO_ROUTE.INVALID_RESPONSE".into());
        }
        if let Some(problem) = response.problem_code {
            return Err(problem);
        }
        response
            .snapshot
            .ok_or_else(|| "CAPY.AUDIO_ROUTE.INVALID_RESPONSE".into())
    }
}

type Runtime = Arc<Mutex<BrokerServiceRuntime<ServiceBrokerProcess>>>;
struct Owner {
    spec: AudioRouteStart,
    deadline: Instant,
    grant_deadline: Instant,
}
#[derive(Default)]
struct Owners {
    slots: [Option<Owner>; 2],
}
impl Owners {
    fn stop(
        &mut self,
        process: &mut crate::NativeAudioSupervisor,
        index: usize,
    ) -> Result<(), String> {
        if self.slots[index].is_none() {
            return Ok(());
        }
        match index {
            0 => process.microphone.stop(),
            _ => process.speaker.stop(),
        }
        .map_err(|_| "CAPY.AUDIO_ROUTE.STOP_FAILED")?;
        self.slots[index] = None;
        process.route_owned[index] = false;
        Ok(())
    }
    fn poll(&mut self, process: &mut crate::NativeAudioSupervisor, now_ms: u64, now: Instant) {
        for index in 0..2 {
            let expired = self.slots[index].as_ref().is_some_and(|owner| {
                now >= owner.deadline
                    || now >= owner.grant_deadline
                    || owner.spec.validate(now_ms).is_err()
            });
            if expired {
                let _ = self.stop(process, index);
            }
        }
    }
    fn invoke(
        &mut self,
        process: &mut crate::NativeAudioSupervisor,
        command: AudioRouteOperation,
        now_ms: u64,
        now: Instant,
    ) -> Result<AudioRouteSnapshot, String> {
        self.poll(process, now_ms, now);
        let key = match &command {
            AudioRouteOperation::Start(spec) => spec.key(),
            AudioRouteOperation::Status(key) | AudioRouteOperation::Stop(key) => key.clone(),
        };
        let index = key.direction.index();
        if let Some(owner) = &self.slots[index]
            && owner.spec.key() != key
        {
            return Err("CAPY.AUDIO_ROUTE.BUSY".into());
        }
        match command {
            AudioRouteOperation::Start(spec) => {
                let binding = spec.validate(now_ms)?;
                if let Some(owner) = &mut self.slots[index] {
                    if owner.spec != *spec {
                        return Err("CAPY.AUDIO_ROUTE.BINDING_CHANGED".into());
                    }
                    owner.deadline = now + OWNER_BUDGET;
                } else {
                    let grant_deadline = now
                        .checked_add(Duration::from_millis(binding.expires_at_ms() - now_ms))
                        .ok_or("CAPY.AUDIO_ROUTE.INVALID_DEADLINE")?;
                    let status = match index {
                        0 => process.microphone.status(),
                        _ => process.speaker.status(),
                    }
                    .map_err(|_| "CAPY.AUDIO_ROUTE.STATUS_FAILED")?;
                    if matches!(status, NativeSpeakerSupervisorStatus::Running { .. }) {
                        return Err("CAPY.AUDIO_ROUTE.LEGACY_BUSY".into());
                    }
                    match index {
                        0 => process.microphone.start_route_at(
                            spec.local,
                            spec.peer,
                            &binding,
                            &spec.route,
                            &spec.session,
                            now_ms,
                        ),
                        _ => process.speaker.start_route_at(
                            spec.local,
                            spec.peer,
                            &binding,
                            &spec.route,
                            &spec.session,
                            now_ms,
                        ),
                    }
                    .map_err(|_| "CAPY.AUDIO_ROUTE.START_FAILED")?;
                    process.route_owned[index] = true;
                    self.slots[index] = Some(Owner {
                        spec: *spec,
                        deadline: now + OWNER_BUDGET,
                        grant_deadline,
                    });
                }
            }
            AudioRouteOperation::Stop(_) => self.stop(process, index)?,
            AudioRouteOperation::Status(_) => {}
        }
        let ready = if self.slots[index].is_some() {
            let status = match index {
                0 => process.microphone.status(),
                _ => process.speaker.status(),
            }
            .map_err(|_| "CAPY.AUDIO_ROUTE.STATUS_FAILED")?;
            matches!(status, NativeSpeakerSupervisorStatus::Running { .. })
        } else {
            false
        };
        if !ready {
            self.stop(process, index)?;
        }
        Ok(AudioRouteSnapshot {
            key,
            worker_ready: ready,
            owner_lease_remaining_ms: self.slots[index].as_ref().map_or(0, |owner| {
                owner
                    .deadline
                    .min(owner.grant_deadline)
                    .saturating_duration_since(now)
                    .as_millis() as u64
            }),
        })
    }
}

fn wall_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

/// Service-owned, opt-in listener. The main service loop must call poll every
/// 250 ms; requests never contain a SID or executable. Drop stops both owners.
pub struct ControlledAudioRoutes {
    runtime: Runtime,
    owners: Arc<Mutex<Owners>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<Result<(), String>>>,
}
impl ControlledAudioRoutes {
    pub fn open(runtime: Runtime, allowed_sid: &str) -> Result<Self, String> {
        let sddl = camera_pipe_sddl(allowed_sid)?;
        if !matches!(
            runtime
                .lock()
                .map_err(|_| "CAPY.AUDIO_ROUTE.STATE_UNAVAILABLE")?
                .process,
            ServiceBrokerProcess::NativeAudio(_)
        ) {
            return Err("CAPY.AUDIO_ROUTE.NATIVE_AUDIO_REQUIRED".into());
        }
        let owners = Arc::new(Mutex::new(Owners::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let mut result = Self {
            runtime: runtime.clone(),
            owners: owners.clone(),
            stop: stop.clone(),
            thread: None,
        };
        let (ready, ready_rx) = mpsc::sync_channel(1);
        result.thread = Some(
            thread::Builder::new()
                .name("capyio-audio-route-control".into())
                .spawn(move || {
                    let pipe = match NamedPipe::create_exclusive(PIPE, &sddl) {
                        Ok(pipe) => pipe,
                        Err(_) => {
                            let _ = ready.send(false);
                            return Err("CAPY.AUDIO_ROUTE.PIPE_OPEN_FAILED".into());
                        }
                    };
                    let _ = ready.send(true);
                    while !stop.load(Ordering::Acquire) {
                        if !pipe.wait_for_client(&stop)? {
                            break;
                        }
                        if let Ok(request) = pipe.read_json::<Request>() {
                            let outcome = (|| {
                                if stop.load(Ordering::Acquire)
                                    || request.schema_version != 1
                                    || request.request_id == 0
                                {
                                    return Err("CAPY.AUDIO_ROUTE.INVALID_REQUEST".into());
                                }
                                let mut owners = owners
                                    .lock()
                                    .map_err(|_| "CAPY.AUDIO_ROUTE.STATE_UNAVAILABLE")?;
                                let mut runtime = runtime
                                    .lock()
                                    .map_err(|_| "CAPY.AUDIO_ROUTE.STATE_UNAVAILABLE")?;
                                let ServiceBrokerProcess::NativeAudio(process) =
                                    &mut runtime.process
                                else {
                                    return Err("CAPY.AUDIO_ROUTE.NATIVE_AUDIO_REQUIRED".into());
                                };
                                owners.invoke(process, request.command, wall_ms(), Instant::now())
                            })();
                            let (snapshot, problem_code) = match outcome {
                                Ok(snapshot) => (Some(snapshot), None),
                                Err(code) => (None, Some(code)),
                            };
                            let _ = pipe.write_json(&Response {
                                schema_version: 1,
                                request_id: request.request_id,
                                snapshot,
                                problem_code,
                            });
                        }
                        pipe.disconnect();
                    }
                    Ok(())
                })
                .map_err(|_| "CAPY.AUDIO_ROUTE.PIPE_SPAWN_FAILED")?,
        );
        if ready_rx.recv_timeout(Duration::from_secs(2)) != Ok(true) {
            return Err("CAPY.AUDIO_ROUTE.PIPE_OPEN_FAILED".into());
        }
        Ok(result)
    }
    pub fn poll(&mut self) {
        if self.thread.as_ref().is_some_and(JoinHandle::is_finished) {
            self.stop.store(true, Ordering::Release);
        }
        let mut owners = self
            .owners
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut runtime = self
            .runtime
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let ServiceBrokerProcess::NativeAudio(process) = &mut runtime.process {
            if self.stop.load(Ordering::Acquire) {
                for index in 0..2 {
                    let _ = owners.stop(process, index);
                }
            } else {
                owners.poll(process, wall_ms(), Instant::now());
            }
        }
    }
    pub fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Release);
        wake(PIPE);
        if let Some(worker) = self.thread.take() {
            let _ = worker.join();
        }
        self.poll();
    }
}
impl Drop for ControlledAudioRoutes {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BrokerProcess, NativeAudioSupervisor, ServiceConfig};
    use capyio_core::*;

    fn spec(direction: AudioRouteDirection) -> AudioRouteStart {
        let session = Session::new(NodeId::new(), NodeId::new());
        let endpoint = |node_id| PortRef {
            node_id,
            capability_id: CapabilityId::new(),
            port_id: PortId::new(),
        };
        let mut route = Route {
            id: RouteId::new(),
            session_id: session.id,
            source: endpoint(session.remote_node_id),
            sink: endpoint(session.local_node_id),
            profile: ProfileId::audio_frames_v1(),
            backend: RouteBackend::AdapterManaged,
            compatible_formats: vec![],
            compatible_qos_modes: Default::default(),
            selected_format: None,
            selected_qos_mode: None,
            state: RouteState::Starting,
            authorization: AuthorizationState::Authorized {
                expires_at_ms: Some(60_000),
            },
            epoch: 7,
            diagnostic_ids: vec![],
        };
        if direction == AudioRouteDirection::Speaker {
            std::mem::swap(&mut route.source, &mut route.sink);
        }
        let media = AudioMediaStreamBinding {
            session_id: session.id,
            route_id: route.id,
            stream_id: StreamId::new(),
            stream_epoch: 7,
            selected_spec: match direction {
                AudioRouteDirection::Microphone => AudioStreamSpec::voice_interactive(),
                AudioRouteDirection::Speaker => AudioStreamSpec::media_balanced(),
            },
        };
        AudioRouteStart {
            direction,
            route,
            session,
            media,
            local: "100.64.0.1:46001".parse().unwrap(),
            peer: "100.64.0.2:46000".parse().unwrap(),
        }
    }
    fn process() -> NativeAudioSupervisor {
        let config = ServiceConfig::parse([
            "service",
            "--mode",
            "native-speaker",
            "--broker",
            "unused-speaker.exe",
            "--bind-ip",
            "100.64.0.1",
            "--port",
            "46001",
            "--peer-ip",
            "100.64.0.2",
            "--peer-port",
            "46000",
            "--microphone-broker",
            "unused-mic.exe",
            "--microphone-bind-ip",
            "100.64.0.1",
            "--microphone-port",
            "46101",
            "--microphone-peer-ip",
            "100.64.0.2",
            "--microphone-peer-port",
            "46100",
        ])
        .unwrap();
        let ServiceBrokerProcess::NativeAudio(process) = config.supervisor().unwrap() else {
            panic!("native audio");
        };
        process
    }
    #[test]
    fn directional_presets_finite_grants_and_identity_are_required() {
        for direction in [
            AudioRouteDirection::Microphone,
            AudioRouteDirection::Speaker,
        ] {
            let mut value = spec(direction);
            assert!(value.validate(1).is_ok());
            assert!(value.validate(60_000).is_err());
            value.route.epoch += 1;
            assert!(value.validate(1).is_err());
            value.route.epoch -= 1;
            value.session.state = SessionState::Suspended;
            assert!(value.validate(1).is_err());
            value.session.state = SessionState::Ready;
            value.direction = if direction == AudioRouteDirection::Microphone {
                AudioRouteDirection::Speaker
            } else {
                AudioRouteDirection::Microphone
            };
            assert!(value.validate(1).is_err());
            value.direction = direction;
            value.route.authorization = AuthorizationState::Authorized {
                expires_at_ms: None,
            };
            assert!(value.validate(1).is_err());
        }
    }
    #[test]
    fn owned_direction_cannot_be_stopped_by_other_key_or_legacy_pipe() {
        let mut process = process();
        let mic = spec(AudioRouteDirection::Microphone);
        let speaker = spec(AudioRouteDirection::Speaker);
        let now = Instant::now();
        let mut owners = Owners {
            slots: [
                Some(Owner {
                    spec: mic.clone(),
                    deadline: now + OWNER_BUDGET,
                    grant_deadline: now + Duration::from_secs(60),
                }),
                Some(Owner {
                    spec: speaker.clone(),
                    deadline: now + OWNER_BUDGET,
                    grant_deadline: now + Duration::from_secs(60),
                }),
            ],
        };
        process.route_owned = [true, true];
        let mut wrong = mic.key();
        wrong.epoch += 1;
        assert_eq!(
            owners
                .invoke(&mut process, AudioRouteOperation::Stop(wrong), 1, now)
                .unwrap_err(),
            "CAPY.AUDIO_ROUTE.BUSY"
        );
        let mut wrapper = ServiceBrokerProcess::NativeAudio(process);
        assert_eq!(wrapper.start().unwrap_err(), "CAPY.AUDIO_ROUTE.OWNED");
        assert_eq!(
            wrapper.native_microphone(Some(false)).unwrap_err(),
            "CAPY.AUDIO_ROUTE.OWNED"
        );
        let ServiceBrokerProcess::NativeAudio(mut process) = wrapper else {
            unreachable!()
        };
        owners
            .invoke(&mut process, AudioRouteOperation::Stop(mic.key()), 1, now)
            .unwrap();
        assert!(owners.slots[0].is_none());
        assert!(owners.slots[1].is_some());
        assert_eq!(process.route_owned, [false, true]);
        owners
            .invoke(&mut process, AudioRouteOperation::Stop(mic.key()), 1, now)
            .unwrap();
        owners.poll(&mut process, 2, now + OWNER_BUDGET);
        assert!(owners.slots[1].is_none());
        assert_eq!(process.route_owned, [false, false]);
    }
    #[test]
    fn requests_are_closed_and_principal_acl_does_not_include_interactive_users() {
        let request = Request {
            schema_version: 1,
            request_id: 1,
            command: AudioRouteOperation::Start(Box::new(spec(AudioRouteDirection::Microphone))),
        };
        let mut json = serde_json::to_value(&request).unwrap();
        assert!(serde_json::to_vec(&json).unwrap().len() < 4096);
        json["executable"] = "not-allowed".into();
        assert!(serde_json::from_value::<Request>(json).is_err());
        let acl = camera_pipe_sddl("S-1-5-21-100-200-300-1001").unwrap();
        assert!(!acl.contains(";;;IU)"));
        assert!(camera_pipe_sddl("S-1-5-21-1);(A;;GA;;;IU)").is_err());
    }
}
