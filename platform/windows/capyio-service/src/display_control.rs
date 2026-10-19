//! Narrow, requester-scoped control for the service-owned display host.
use crate::{
    DisplayExecutionSnapshot, DisplayExecutionState, DisplayServiceRuntime,
    camera_control::camera_pipe_sddl,
    local_pipe::{NamedPipe, try_invoke, wake},
};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

#[cfg(not(test))]
const PIPE: &str = r"\\.\pipe\CapyIO.Display.Control.v1";
#[cfg(test)]
const PIPE: &str = r"\\.\pipe\CapyIO.Display.Control.v1.test";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DisplayControlOperation {
    Status,
    Start,
    Stop,
    StopPeer {
        generation: u64,
    },
    PreparePeer(crate::DisplayPeerLaunchRequest),
    CommitPeer {
        generation: u64,
        acknowledgement: Vec<u8>,
    },
    RenewPeer {
        generation: u64,
        expires_at_ms: u64,
    },
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    schema_version: u8,
    request_id: u64,
    operation: DisplayControlOperation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    settings: Option<crate::display_host::DisplaySessionSettings>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DisplayControlSnapshot {
    pub schema_version: u8,
    pub generation: u64,
    pub state: DisplayExecutionState,
    pub problem_code: Option<String>,
}
impl From<DisplayExecutionSnapshot> for DisplayControlSnapshot {
    fn from(value: DisplayExecutionSnapshot) -> Self {
        Self {
            schema_version: value.schema_version,
            generation: value.generation,
            state: value.state,
            problem_code: value.problem_code.map(str::to_owned),
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Response {
    schema_version: u8,
    request_id: u64,
    snapshot: Option<DisplayControlSnapshot>,
    problem_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    prepared: Option<crate::DisplayPeerLaunchPrepared>,
}

pub struct DisplayServiceClient {
    next_id: AtomicU64,
}
impl Default for DisplayServiceClient {
    fn default() -> Self {
        Self {
            next_id: AtomicU64::new(1),
        }
    }
}
impl DisplayServiceClient {
    pub fn stop_peer(&self, generation: u64) -> Result<DisplayControlSnapshot, String> {
        self.invoke(DisplayControlOperation::StopPeer { generation })
    }
    pub fn renew_peer(
        &self,
        generation: u64,
        expires_at_ms: u64,
    ) -> Result<DisplayControlSnapshot, String> {
        self.invoke(DisplayControlOperation::RenewPeer {
            generation,
            expires_at_ms,
        })
    }
    pub fn prepare_peer(
        &self,
        request: crate::DisplayPeerLaunchRequest,
    ) -> Result<crate::DisplayPeerLaunchPrepared, String> {
        let id = self.next_id.fetch_add(1, Ordering::AcqRel);
        let mut response: Response = try_invoke(
            PIPE,
            &Request {
                schema_version: 1,
                request_id: id,
                operation: DisplayControlOperation::PreparePeer(request),
                settings: None,
            },
        )
        .map_err(|_| "CAPY.DISPLAY.CONTROL_UNAVAILABLE")?;
        let prepared = response.prepared.take();
        validate_response(response, id)?;
        prepared
            .filter(|p| p.launch_record.len() == 176)
            .ok_or_else(|| "CAPY.DISPLAY.CONTROL_RESPONSE_INVALID".into())
    }
    pub fn commit_peer(
        &self,
        generation: u64,
        acknowledgement: Vec<u8>,
    ) -> Result<DisplayControlSnapshot, String> {
        self.invoke(DisplayControlOperation::CommitPeer {
            generation,
            acknowledgement,
        })
    }
    pub fn invoke(
        &self,
        operation: DisplayControlOperation,
    ) -> Result<DisplayControlSnapshot, String> {
        self.invoke_with_settings(operation, None)
    }

    pub fn start_with_settings(
        &self,
        settings: crate::display_host::DisplaySessionSettings,
    ) -> Result<DisplayControlSnapshot, String> {
        settings.validate().map_err(str::to_owned)?;
        self.invoke_with_settings(DisplayControlOperation::Start, Some(settings))
    }

    fn invoke_with_settings(
        &self,
        operation: DisplayControlOperation,
        settings: Option<crate::display_host::DisplaySessionSettings>,
    ) -> Result<DisplayControlSnapshot, String> {
        let request_id = self
            .next_id
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |id| id.checked_add(1))
            .map_err(|_| "CAPY.DISPLAY.REQUEST_ID_EXHAUSTED")?;
        let response: Response = try_invoke(
            PIPE,
            &Request {
                schema_version: 1,
                request_id,
                operation,
                settings,
            },
        )
        .map_err(|_| "CAPY.DISPLAY.CONTROL_UNAVAILABLE")?;
        validate_response(response, request_id)
    }
}

fn validate_response(
    response: Response,
    request_id: u64,
) -> Result<DisplayControlSnapshot, String> {
    if response.schema_version != 1 || response.request_id != request_id {
        return Err("CAPY.DISPLAY.CONTROL_RESPONSE_INVALID".into());
    }
    if response.problem_code.is_some() {
        return Err("CAPY.DISPLAY.CONTROL_REJECTED".into());
    }
    let snapshot = response
        .snapshot
        .ok_or("CAPY.DISPLAY.CONTROL_RESPONSE_INVALID")?;
    if snapshot.schema_version != 1
        || snapshot.problem_code.as_ref().is_some_and(|code| {
            code.len() > 128
                || !code.starts_with("CAPY.DISPLAY.")
                || !code
                    .bytes()
                    .all(|byte| byte.is_ascii_uppercase() || byte == b'.' || byte == b'_')
        })
    {
        return Err("CAPY.DISPLAY.CONTROL_RESPONSE_INVALID".into());
    }
    Ok(snapshot)
}

fn dispatch(
    runtime: &mut DisplayServiceRuntime,
    path: &std::path::Path,
    request: Request,
) -> Response {
    let mut response = Response {
        prepared: None,
        schema_version: 1,
        request_id: request.request_id,
        snapshot: None,
        problem_code: None,
    };
    if request.schema_version != 1 || request.request_id == 0 {
        response.problem_code = Some("CAPY.DISPLAY.CONTROL_REQUEST_INVALID".into());
        return response;
    }
    runtime.poll();
    if request
        .settings
        .is_some_and(|settings| settings.validate().is_err())
        || (request.settings.is_some()
            && !matches!(request.operation, DisplayControlOperation::Start))
    {
        response.problem_code = Some("CAPY.DISPLAY.SETTINGS_INVALID".into());
        return response;
    }
    let result = match request.operation {
        DisplayControlOperation::StopPeer { generation } => runtime.stop_peer(generation),
        DisplayControlOperation::RenewPeer {
            generation,
            expires_at_ms,
        } => runtime.renew_peer(generation, expires_at_ms),
        DisplayControlOperation::PreparePeer(peer) => {
            runtime.prepare_peer(path.to_owned(), peer).map(|prepared| {
                response.prepared = Some(prepared);
            })
        }
        DisplayControlOperation::CommitPeer {
            generation,
            acknowledgement,
        } => runtime.commit_peer(generation, acknowledgement),
        DisplayControlOperation::Status => Ok(()),
        DisplayControlOperation::Stop => {
            runtime.request_stop();
            Ok(())
        }
        DisplayControlOperation::Start => match runtime.snapshot().state {
            DisplayExecutionState::Starting | DisplayExecutionState::Running
                if request.settings.is_none() =>
            {
                Ok(())
            }
            DisplayExecutionState::Starting | DisplayExecutionState::Running => {
                Err("CAPY.DISPLAY.HOST_BUSY")
            }
            DisplayExecutionState::Stopping => Err("CAPY.DISPLAY.HOST_BUSY"),
            _ => runtime.start_with_settings(path.to_owned(), request.settings),
        },
    };
    response.snapshot = Some(runtime.snapshot().into());
    response.problem_code = result.err().map(str::to_owned);
    response
}

/// Owns optional control listener and display runtime independently of the UI.
/// With a requesting SID the host waits for explicit Start; without it, preserves
/// ADR 1426's trusted-launch opt-in and exposes no control pipe.
pub struct ControlledDisplayRuntime {
    runtime: Arc<Mutex<DisplayServiceRuntime>>,
    control_stop: Arc<AtomicBool>,
    control: Option<JoinHandle<Result<(), String>>>,
    control_problem: Option<&'static str>,
}
impl ControlledDisplayRuntime {
    pub fn open(path: PathBuf, requesting_sid: Option<&str>) -> Result<Self, String> {
        let runtime = Arc::new(Mutex::new(DisplayServiceRuntime::default()));
        let mut owner = Self {
            runtime: Arc::clone(&runtime),
            control_stop: Arc::new(AtomicBool::new(false)),
            control: None,
            control_problem: None,
        };
        let Some(sid) = requesting_sid else {
            runtime
                .lock()
                .map_err(|_| "CAPY.DISPLAY.STATE_UNAVAILABLE")?
                .start(path)?;
            return Ok(owner);
        };
        // Reuse the Windows-validated requester SID ACL already used by camera.
        let sddl = camera_pipe_sddl(sid).map_err(|_| "CAPY.DISPLAY.CONTROL_USER_INVALID")?;
        let stop = Arc::clone(&owner.control_stop);
        let (ready, received) = mpsc::sync_channel(1);
        owner.control = Some(
            thread::Builder::new()
                .name("capyio-display-control".into())
                .spawn(move || {
                    let pipe = match NamedPipe::create_exclusive(PIPE, &sddl) {
                        Ok(pipe) => pipe,
                        Err(_) => {
                            let _ = ready.send(false);
                            return Err("CAPY.DISPLAY.CONTROL_OPEN_FAILED".into());
                        }
                    };
                    let _ = ready.send(true);
                    while !stop.load(Ordering::Acquire) {
                        if !pipe.wait_for_client(&stop)? || stop.load(Ordering::Acquire) {
                            break;
                        }
                        let request = pipe.read_json::<Request>();
                        if stop.load(Ordering::Acquire) {
                            pipe.disconnect();
                            break;
                        }
                        if let Ok(request) = request {
                            let mut locked = runtime
                                .lock()
                                .map_err(|_| "CAPY.DISPLAY.STATE_UNAVAILABLE")?;
                            if stop.load(Ordering::Acquire) {
                                pipe.disconnect();
                                break;
                            }
                            let response = dispatch(&mut locked, &path, request);
                            drop(locked);
                            let _ = pipe.write_json(&response);
                        }
                        pipe.disconnect();
                    }
                    Ok(())
                })
                .map_err(|_| "CAPY.DISPLAY.CONTROL_SPAWN_FAILED")?,
        );
        if received.recv_timeout(Duration::from_secs(2)) != Ok(true) {
            return Err("CAPY.DISPLAY.CONTROL_OPEN_FAILED".into());
        }
        Ok(owner)
    }

    pub fn snapshot(&self) -> DisplayControlSnapshot {
        let mut snapshot: DisplayControlSnapshot = self
            .runtime
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .snapshot()
            .into();
        if let Some(problem) = self.control_problem {
            snapshot.problem_code = Some(problem.into());
        }
        snapshot
    }
    pub fn poll(&mut self) {
        self.runtime
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .poll();
        if self.control.as_ref().is_some_and(JoinHandle::is_finished) {
            let _ = self.control.take().unwrap().join();
            self.control_problem = Some("CAPY.DISPLAY.CONTROL_EXITED");
            self.request_stop();
        }
    }
    pub fn request_stop(&mut self) {
        self.control_stop.store(true, Ordering::Release);
        self.runtime
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .request_stop();
    }
    pub fn shutdown(&mut self) -> DisplayControlSnapshot {
        self.request_stop();
        if let Some(control) = self.control.take() {
            wake(PIPE);
            if !matches!(control.join(), Ok(Ok(()))) {
                self.control_problem = Some("CAPY.DISPLAY.CONTROL_EXITED");
            }
        }
        self.runtime
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .shutdown();
        self.snapshot()
    }
}
impl Drop for ControlledDisplayRuntime {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn start_is_idempotent_and_stop_does_not_wait_for_native_cleanup() {
        let mut runtime = DisplayServiceRuntime::default();
        let (entered, observed) = mpsc::sync_channel(1);
        let (release, blocked) = mpsc::sync_channel(1);
        runtime
            .start_with(move || {
                Ok(move |stop: Arc<AtomicBool>| {
                    entered.send(()).unwrap();
                    blocked.recv_timeout(Duration::from_secs(3)).unwrap();
                    assert!(stop.load(Ordering::Acquire));
                    Ok(())
                })
            })
            .unwrap();
        observed.recv_timeout(Duration::from_secs(3)).unwrap();
        let path = std::path::Path::new("never-read");
        let request = |operation| Request {
            schema_version: 1,
            request_id: 1,
            operation,
            settings: None,
        };
        let start = dispatch(&mut runtime, path, request(DisplayControlOperation::Start));
        assert_eq!(start.snapshot.unwrap().generation, 1);
        assert!(start.problem_code.is_none());
        let stop = dispatch(&mut runtime, path, request(DisplayControlOperation::Stop));
        assert_eq!(
            stop.snapshot.unwrap().state,
            DisplayExecutionState::Stopping
        );
        let restart = dispatch(&mut runtime, path, request(DisplayControlOperation::Start));
        assert_eq!(
            restart.problem_code.as_deref(),
            Some("CAPY.DISPLAY.HOST_BUSY")
        );
        release.send(()).unwrap();
        assert_eq!(runtime.shutdown().state, DisplayExecutionState::Stopped);
    }

    #[test]
    fn controller_sid_requires_display_configuration_and_rejects_injection() {
        let base = vec![
            "service",
            "--broker",
            "x",
            "--bind-ip",
            "127.0.0.1",
            "--port",
            "40101",
        ];
        for sid in [
            "S-1-5-21-1-2-3-1001",
            "S-1-5-4)(A;;GA;;;WD",
            "",
            "not-a-sid",
        ] {
            let mut args = base.clone();
            args.extend(["--display-control-user-sid", sid]);
            assert!(crate::ServiceConfig::parse(args).is_err());
        }
        assert!(camera_pipe_sddl("S-1-5-4)(A;;GA;;;WD").is_err());
    }
    #[test]
    fn closed_requests_reject_launch_authority_and_unknown_operations() {
        for value in [
            r#"{"schemaVersion":1,"requestId":1,"operation":"start","path":"bad"}"#,
            r#"{"schemaVersion":1,"requestId":1,"operation":"shell"}"#,
        ] {
            assert!(serde_json::from_str::<Request>(value).is_err());
        }
        let mut runtime = DisplayServiceRuntime::default();
        for (version, id) in [(2, 1), (1, 0)] {
            let response = dispatch(
                &mut runtime,
                std::path::Path::new("unused"),
                Request {
                    schema_version: version,
                    request_id: id,
                    operation: DisplayControlOperation::Start,
                    settings: None,
                },
            );
            assert!(response.problem_code.is_some());
            assert_eq!(runtime.snapshot().generation, 0);
        }
    }
    #[test]
    fn settings_are_only_accepted_for_start_and_never_mutate_an_active_session() {
        let settings = crate::display_host::DisplaySessionSettings {
            width: 1280,
            height: 720,
            refresh_millihertz: 30_000,
            bitrate_kbps: 8_000,
        };
        let mut runtime = DisplayServiceRuntime::default();
        let response = dispatch(
            &mut runtime,
            std::path::Path::new("unused"),
            Request {
                schema_version: 1,
                request_id: 1,
                operation: DisplayControlOperation::Status,
                settings: Some(settings),
            },
        );
        assert_eq!(
            response.problem_code.as_deref(),
            Some("CAPY.DISPLAY.SETTINGS_INVALID")
        );
        assert_eq!(runtime.snapshot().generation, 0);
        runtime
            .start_with(|| {
                Ok(|stop: Arc<AtomicBool>| {
                    while !stop.load(Ordering::Acquire) {
                        thread::sleep(Duration::from_millis(1));
                    }
                    Ok(())
                })
            })
            .unwrap();
        let response = dispatch(
            &mut runtime,
            std::path::Path::new("unused"),
            Request {
                schema_version: 1,
                request_id: 2,
                operation: DisplayControlOperation::Start,
                settings: Some(settings),
            },
        );
        assert_eq!(
            response.problem_code.as_deref(),
            Some("CAPY.DISPLAY.HOST_BUSY")
        );
        assert_eq!(runtime.snapshot().generation, 1);
        runtime.request_stop();
    }

    #[test]
    fn response_correlation_and_private_problem_bounds_fail_closed() {
        let mut response = Response {
            prepared: None,
            schema_version: 1,
            request_id: 2,
            snapshot: Some(DisplayServiceRuntime::default().snapshot().into()),
            problem_code: None,
        };
        assert!(validate_response(response, 1).is_err());
        response = Response {
            prepared: None,
            schema_version: 1,
            request_id: 1,
            snapshot: Some(DisplayServiceRuntime::default().snapshot().into()),
            problem_code: None,
        };
        response.snapshot.as_mut().unwrap().problem_code = Some("C:\\private\\secret".into());
        assert!(validate_response(response, 1).is_err());
    }
    #[test]
    fn requester_pipe_roundtrip_stays_stopped_until_explicit_start() {
        let sid = crate::current_process_user_sid().unwrap();
        let path = std::env::temp_dir().join(format!("capyio-missing-{}.json", std::process::id()));
        assert!(!path.exists());
        let mut owner = ControlledDisplayRuntime::open(path, Some(&sid)).unwrap();
        assert!(
            ControlledDisplayRuntime::open(
                std::env::temp_dir().join("unused-display.json"),
                Some(&sid)
            )
            .is_err()
        );
        let client = DisplayServiceClient::default();
        assert_eq!(
            client
                .invoke(DisplayControlOperation::Status)
                .unwrap()
                .generation,
            0
        );
        assert_eq!(
            client.invoke(DisplayControlOperation::Stop).unwrap().state,
            DisplayExecutionState::Stopped
        );
        client.invoke(DisplayControlOperation::Start).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        loop {
            owner.poll();
            let snapshot = client.invoke(DisplayControlOperation::Status).unwrap();
            if snapshot.state == DisplayExecutionState::Failed {
                assert_eq!(
                    snapshot.problem_code.as_deref(),
                    Some("CAPY.DISPLAY.CONFIGURATION_INVALID")
                );
                break;
            }
            assert!(std::time::Instant::now() < deadline);
        }
        assert_eq!(owner.shutdown().state, DisplayExecutionState::Failed);
    }
}
