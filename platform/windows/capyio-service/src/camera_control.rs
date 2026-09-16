use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};
use windows_sys::Win32::{
    Foundation::{CloseHandle, LocalFree},
    Security::{
        Authorization::{ConvertSidToStringSidW, ConvertStringSidToSidW},
        GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser,
    },
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};

use crate::local_pipe::{NamedPipe, invoke, try_invoke, wake};

#[cfg(not(test))]
pub const CAMERA_CONTROL_PIPE_NAME: &str = r"\\.\pipe\CapyIO.Camera.Control.v1";
#[cfg(test)]
pub const CAMERA_CONTROL_PIPE_NAME: &str = r"\\.\pipe\CapyIO.Camera.Control.v1.test";
const CONTROL_SCHEMA_VERSION: u8 = 1;
const PIPE_SDDL_PREFIX: &str = "D:P(A;;GA;;;SY)(A;;GA;;;BA)";

const START_FAILED: &str = "CAPY.CAMERA_BROKER.START_FAILED";
const NOT_READY: &str = "CAPY.CAMERA_BROKER.NOT_READY";
const ROLLBACK_FAILED: &str = "CAPY.CAMERA_BROKER.ROLLBACK_FAILED";
const STATUS_FAILED: &str = "CAPY.CAMERA_BROKER.STATUS_FAILED";
const EXITED: &str = "CAPY.CAMERA_BROKER.EXITED";
const STOP_FAILED: &str = "CAPY.CAMERA_BROKER.STOP_FAILED";
const STILL_RUNNING: &str = "CAPY.CAMERA_BROKER.STILL_RUNNING";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ControlOperation {
    Status,
    Start,
    Stop,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ControlRequest {
    schema_version: u8,
    request_id: u64,
    operation: ControlOperation,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ControlResponse {
    schema_version: u8,
    request_id: u64,
    success: bool,
    snapshot: Option<CameraBrokerSnapshot>,
    problem_code: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CameraBrokerState {
    Stopped,
    Active,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CameraBrokerSnapshot {
    pub state: CameraBrokerState,
    pub generation: u64,
    pub problem_code: Option<String>,
}

/// Owns the fixed camera projection lifecycle. Implementations must not accept
/// caller-provided executables, paths, pipe names or transport endpoints.
pub trait CameraBrokerProcess: Send {
    fn start(&mut self) -> Result<(), String>;
    fn running(&mut self) -> Result<bool, String>;
    fn stop(&mut self) -> Result<(), String>;
}

pub struct CameraBrokerRuntime<P> {
    process: P,
    state: CameraBrokerState,
    generation: u64,
    problem_code: Option<String>,
}

impl<P: CameraBrokerProcess> CameraBrokerRuntime<P> {
    pub fn new(process: P) -> Self {
        Self {
            process,
            state: CameraBrokerState::Stopped,
            generation: 0,
            problem_code: None,
        }
    }

    pub fn snapshot(&self) -> CameraBrokerSnapshot {
        CameraBrokerSnapshot {
            state: self.state,
            generation: self.generation,
            problem_code: self.problem_code.clone(),
        }
    }

    pub fn status(&mut self) -> Result<CameraBrokerSnapshot, String> {
        match self.process.running() {
            Ok(true) => {
                self.state = CameraBrokerState::Active;
                self.problem_code = None;
                Ok(self.snapshot())
            }
            Ok(false) if self.state == CameraBrokerState::Active => self.fail(EXITED),
            Ok(false) => Ok(self.snapshot()),
            Err(_) => self.fail(STATUS_FAILED),
        }
    }

    pub fn ensure_started(&mut self) -> Result<CameraBrokerSnapshot, String> {
        if self
            .process
            .running()
            .map_err(|_| STATUS_FAILED.to_owned())?
        {
            self.state = CameraBrokerState::Active;
            self.problem_code = None;
            return Ok(self.snapshot());
        }
        if self.process.start().is_err() {
            return self.fail_start(START_FAILED);
        }
        match self.process.running() {
            Ok(true) => {
                self.generation = self
                    .generation
                    .checked_add(1)
                    .ok_or_else(|| "camera broker generation exhausted".to_owned())?;
                self.state = CameraBrokerState::Active;
                self.problem_code = None;
                Ok(self.snapshot())
            }
            Ok(false) => self.fail_start(NOT_READY),
            Err(_) => self.fail_start(STATUS_FAILED),
        }
    }

    pub fn ensure_stopped(&mut self) -> Result<CameraBrokerSnapshot, String> {
        if self.process.running().is_ok_and(|running| !running) {
            self.state = CameraBrokerState::Stopped;
            self.problem_code = None;
            return Ok(self.snapshot());
        }
        if self.process.stop().is_err() {
            return self.fail(STOP_FAILED);
        }
        match self.process.running() {
            Ok(false) => {
                self.state = CameraBrokerState::Stopped;
                self.problem_code = None;
                Ok(self.snapshot())
            }
            Ok(true) => self.fail(STILL_RUNNING),
            Err(_) => self.fail(STATUS_FAILED),
        }
    }

    fn fail(&mut self, problem_code: &'static str) -> Result<CameraBrokerSnapshot, String> {
        self.state = CameraBrokerState::Failed;
        self.problem_code = Some(problem_code.to_owned());
        Err(problem_code.to_owned())
    }

    fn fail_start(&mut self, problem_code: &'static str) -> Result<CameraBrokerSnapshot, String> {
        if self.process.stop().is_err() {
            return self.fail(ROLLBACK_FAILED);
        }
        self.fail(problem_code)
    }
}

pub struct CameraBrokerClient {
    next_request_id: AtomicU64,
}

impl Default for CameraBrokerClient {
    fn default() -> Self {
        Self {
            next_request_id: AtomicU64::new(1),
        }
    }
}

impl CameraBrokerClient {
    pub fn try_status(&self) -> Result<CameraBrokerSnapshot, String> {
        self.invoke_with(ControlOperation::Status, true)
    }

    pub fn status(&self) -> Result<CameraBrokerSnapshot, String> {
        self.invoke(ControlOperation::Status)
    }

    pub fn start(&self) -> Result<CameraBrokerSnapshot, String> {
        self.invoke(ControlOperation::Start)
    }

    pub fn stop(&self) -> Result<CameraBrokerSnapshot, String> {
        self.invoke(ControlOperation::Stop)
    }

    fn invoke(&self, operation: ControlOperation) -> Result<CameraBrokerSnapshot, String> {
        self.invoke_with(operation, false)
    }

    fn invoke_with(
        &self,
        operation: ControlOperation,
        bounded_probe: bool,
    ) -> Result<CameraBrokerSnapshot, String> {
        let request_id = self
            .next_request_id
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                value.checked_add(1)
            })
            .map_err(|_| "camera broker request ID exhausted".to_owned())?;
        let request = ControlRequest {
            schema_version: CONTROL_SCHEMA_VERSION,
            request_id,
            operation,
        };
        let response: ControlResponse = if bounded_probe {
            try_invoke(CAMERA_CONTROL_PIPE_NAME, &request)?
        } else {
            invoke(CAMERA_CONTROL_PIPE_NAME, &request)?
        };
        if response.schema_version != CONTROL_SCHEMA_VERSION || response.request_id != request_id {
            return Err("mismatched camera broker response".to_owned());
        }
        if !response.success {
            return Err(response
                .problem_code
                .unwrap_or_else(|| "CAPY.CAMERA_BROKER.CONTROL_FAILED".to_owned()));
        }
        response
            .snapshot
            .ok_or_else(|| "camera broker response omitted state".to_owned())
    }
}

pub fn camera_control_server_loop<P: CameraBrokerProcess>(
    runtime: Arc<Mutex<CameraBrokerRuntime<P>>>,
    stop: Arc<AtomicBool>,
) -> Result<(), String> {
    let user_sid = current_process_user_sid()?;
    camera_control_server_loop_with_policy(runtime, stop, false, &user_sid)
}

/// Runs a projection-scoped Broker lease. A successful Stop response is
/// flushed before the host exits; failed Stop remains available for retry.
pub fn camera_control_server_loop_until_stopped<P: CameraBrokerProcess>(
    runtime: Arc<Mutex<CameraBrokerRuntime<P>>>,
    stop: Arc<AtomicBool>,
) -> Result<(), String> {
    let user_sid = current_process_user_sid()?;
    camera_control_server_loop_with_policy(runtime, stop, true, &user_sid)
}

/// Runs the projection-scoped Broker for the exact ordinary user that
/// initiated cross-account UAC. The SID is syntax-validated by Windows before
/// it is admitted to the DACL.
pub fn camera_control_server_loop_until_stopped_for_user<P: CameraBrokerProcess>(
    runtime: Arc<Mutex<CameraBrokerRuntime<P>>>,
    stop: Arc<AtomicBool>,
    requesting_user_sid: &str,
) -> Result<(), String> {
    camera_control_server_loop_with_policy(runtime, stop, true, requesting_user_sid)
}

fn camera_control_server_loop_with_policy<P: CameraBrokerProcess>(
    runtime: Arc<Mutex<CameraBrokerRuntime<P>>>,
    stop: Arc<AtomicBool>,
    exit_after_successful_stop: bool,
    requesting_user_sid: &str,
) -> Result<(), String> {
    let pipe_sddl = camera_pipe_sddl(requesting_user_sid)?;
    let pipe = NamedPipe::create(CAMERA_CONTROL_PIPE_NAME, &pipe_sddl)?;
    while !stop.load(Ordering::Acquire) {
        if !pipe.wait_for_client(&stop)? || stop.load(Ordering::Acquire) {
            break;
        }
        let request: ControlRequest = match pipe.read_json() {
            Ok(request) => request,
            Err(_) => {
                pipe.disconnect();
                continue;
            }
        };
        let stop_host = exit_after_successful_stop && request.operation == ControlOperation::Stop;
        let response = dispatch_request(&runtime, request);
        let stop_host = stop_host && response.success;
        let wrote_response = pipe.write_json(&response).is_ok();
        pipe.disconnect();
        if stop_host && wrote_response {
            break;
        }
    }
    Ok(())
}

pub fn current_process_user_sid() -> Result<String, String> {
    let mut token = std::ptr::null_mut();
    // SAFETY: token is an out parameter and is closed below after success.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err("open camera Broker process token".to_owned());
    }
    let result = (|| {
        let mut byte_length = 0_u32;
        unsafe {
            GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut byte_length);
        }
        if byte_length == 0 || byte_length > 64 * 1024 {
            return Err("invalid camera Broker token user size".to_owned());
        }
        let words = (byte_length as usize).div_ceil(std::mem::size_of::<usize>());
        let mut buffer = vec![0_usize; words];
        // SAFETY: the aligned buffer is at least byte_length bytes and token is live.
        if unsafe {
            GetTokenInformation(
                token,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                byte_length,
                &mut byte_length,
            )
        } == 0
        {
            return Err("read camera Broker token user".to_owned());
        }
        // SAFETY: successful TokenUser output starts with a valid TOKEN_USER.
        let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
        let mut sid_text = std::ptr::null_mut();
        if unsafe { ConvertSidToStringSidW(user.User.Sid, &mut sid_text) } == 0 {
            return Err("format camera Broker token user SID".to_owned());
        }
        let sid = read_local_wide_string(sid_text);
        unsafe { LocalFree(sid_text.cast()) };
        sid
    })();
    unsafe { CloseHandle(token) };
    result
}

fn camera_pipe_sddl(requesting_user_sid: &str) -> Result<String, String> {
    if requesting_user_sid.len() > 184 || !requesting_user_sid.starts_with("S-1-") {
        return Err("camera Broker requesting user SID is outside bounds".to_owned());
    }
    let sid_wide = requesting_user_sid
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let mut parsed_sid = std::ptr::null_mut();
    // SAFETY: sid_wide is immutable terminated UTF-16; LocalFree releases the
    // validation allocation returned by Windows.
    if unsafe { ConvertStringSidToSidW(sid_wide.as_ptr(), &mut parsed_sid) } == 0 {
        return Err("camera Broker requesting user SID is invalid".to_owned());
    }
    unsafe { LocalFree(parsed_sid) };
    Ok(format!("{PIPE_SDDL_PREFIX}(A;;GA;;;{requesting_user_sid})"))
}

fn read_local_wide_string(value: *const u16) -> Result<String, String> {
    let mut length = 0_usize;
    // SAFETY: callers provide a terminated LocalAlloc string.
    while unsafe { *value.add(length) } != 0 {
        length += 1;
        if length > 184 {
            return Err("camera Broker token user SID is outside bounds".to_owned());
        }
    }
    String::from_utf16(unsafe { std::slice::from_raw_parts(value, length) })
        .map_err(|_| "camera Broker token user SID is invalid".to_owned())
}

pub fn wake_camera_control_server() {
    wake(CAMERA_CONTROL_PIPE_NAME);
}

fn dispatch_request<P: CameraBrokerProcess>(
    runtime: &Arc<Mutex<CameraBrokerRuntime<P>>>,
    request: ControlRequest,
) -> ControlResponse {
    if request.schema_version != CONTROL_SCHEMA_VERSION {
        return failed_response(
            request.request_id,
            "CAPY.CAMERA_BROKER.UNSUPPORTED_CONTROL_VERSION",
        );
    }
    let Ok(mut runtime) = runtime.lock() else {
        return failed_response(request.request_id, "CAPY.CAMERA_BROKER.STATE_UNAVAILABLE");
    };
    let result = match request.operation {
        ControlOperation::Status => runtime.status(),
        ControlOperation::Start => runtime.ensure_started(),
        ControlOperation::Stop => runtime.ensure_stopped(),
    };
    match result {
        Ok(snapshot) => ControlResponse {
            schema_version: CONTROL_SCHEMA_VERSION,
            request_id: request.request_id,
            success: true,
            snapshot: Some(snapshot),
            problem_code: None,
        },
        Err(problem_code) => ControlResponse {
            schema_version: CONTROL_SCHEMA_VERSION,
            request_id: request.request_id,
            success: false,
            snapshot: None,
            problem_code: Some(problem_code),
        },
    }
}

fn failed_response(request_id: u64, problem_code: &'static str) -> ControlResponse {
    ControlResponse {
        schema_version: CONTROL_SCHEMA_VERSION,
        request_id,
        success: false,
        snapshot: None,
        problem_code: Some(problem_code.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_pipe::MAX_CONTROL_BYTES;
    use std::{thread, time::Duration};

    static PIPE_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[derive(Default)]
    struct FakeProcess {
        running: bool,
        starts: usize,
        stops: usize,
        fail_start_after_launch: bool,
        fail_stop: bool,
    }

    impl CameraBrokerProcess for FakeProcess {
        fn start(&mut self) -> Result<(), String> {
            self.running = true;
            self.starts += 1;
            if self.fail_start_after_launch {
                return Err("injected start failure".to_owned());
            }
            Ok(())
        }

        fn running(&mut self) -> Result<bool, String> {
            Ok(self.running)
        }

        fn stop(&mut self) -> Result<(), String> {
            self.stops += 1;
            if self.fail_stop {
                return Err("injected stop failure".to_owned());
            }
            self.running = false;
            Ok(())
        }
    }

    fn runtime() -> Arc<Mutex<CameraBrokerRuntime<FakeProcess>>> {
        Arc::new(Mutex::new(CameraBrokerRuntime::new(FakeProcess::default())))
    }

    #[test]
    fn request_schema_is_closed_and_versioned() {
        assert!(
            serde_json::from_str::<ControlRequest>(
                r#"{"schemaVersion":1,"requestId":1,"operation":"start","path":"x"}"#
            )
            .is_err()
        );
        let response = dispatch_request(
            &runtime(),
            ControlRequest {
                schema_version: 2,
                request_id: 9,
                operation: ControlOperation::Status,
            },
        );
        assert!(!response.success);
        assert_eq!(response.request_id, 9);
    }

    #[test]
    fn pipe_acl_names_only_the_validated_requesting_user() {
        let sid = current_process_user_sid().expect("current SID");
        let sddl = camera_pipe_sddl(&sid).expect("validated SDDL");
        assert_eq!(sddl, format!("{PIPE_SDDL_PREFIX}(A;;GA;;;{sid})"));
        assert!(camera_pipe_sddl("S-1-5-4)(A;;GA;;;WD").is_err());
        assert!(camera_pipe_sddl("not-a-sid").is_err());
    }

    #[test]
    fn lifecycle_is_idempotent_bounded_and_discloses_no_launch_authority() {
        let runtime = runtime();
        for (operation, expected) in [
            (ControlOperation::Start, CameraBrokerState::Active),
            (ControlOperation::Start, CameraBrokerState::Active),
            (ControlOperation::Status, CameraBrokerState::Active),
            (ControlOperation::Stop, CameraBrokerState::Stopped),
            (ControlOperation::Stop, CameraBrokerState::Stopped),
        ] {
            let response = dispatch_request(
                &runtime,
                ControlRequest {
                    schema_version: 1,
                    request_id: 1,
                    operation,
                },
            );
            assert!(response.success);
            assert_eq!(response.snapshot.as_ref().unwrap().state, expected);
            let json = serde_json::to_vec(&response).unwrap();
            assert!(json.len() < MAX_CONTROL_BYTES);
            for forbidden in ["endpoint", "executable", "path", "arguments"] {
                assert!(!String::from_utf8_lossy(&json).contains(forbidden));
            }
        }
        let runtime = runtime.lock().unwrap();
        assert_eq!(runtime.process.starts, 1);
        assert_eq!(runtime.process.stops, 1);
        assert_eq!(runtime.generation, 1);
    }

    #[test]
    fn unexpected_exit_is_a_stable_failed_state() {
        let runtime = runtime();
        assert!(
            dispatch_request(
                &runtime,
                ControlRequest {
                    schema_version: 1,
                    request_id: 1,
                    operation: ControlOperation::Start,
                },
            )
            .success
        );
        runtime.lock().unwrap().process.running = false;
        let response = dispatch_request(
            &runtime,
            ControlRequest {
                schema_version: 1,
                request_id: 2,
                operation: ControlOperation::Status,
            },
        );
        assert!(!response.success);
        assert_eq!(response.problem_code.as_deref(), Some(EXITED));
        assert_eq!(
            runtime.lock().unwrap().snapshot().state,
            CameraBrokerState::Failed
        );
    }

    #[test]
    fn partial_start_is_rolled_back_and_rollback_failure_is_distinct() {
        let mut runtime = CameraBrokerRuntime::new(FakeProcess {
            fail_start_after_launch: true,
            ..FakeProcess::default()
        });
        assert_eq!(runtime.ensure_started().unwrap_err(), START_FAILED);
        assert!(!runtime.process.running);
        assert_eq!(runtime.process.stops, 1);

        let mut runtime = CameraBrokerRuntime::new(FakeProcess {
            fail_start_after_launch: true,
            fail_stop: true,
            ..FakeProcess::default()
        });
        assert_eq!(runtime.ensure_started().unwrap_err(), ROLLBACK_FAILED);
        assert!(runtime.process.running);
    }

    #[test]
    fn requester_scoped_named_pipe_round_trip_controls_fake_runtime() {
        let _guard = PIPE_TEST_LOCK.lock().unwrap();
        let runtime = runtime();
        let stop = Arc::new(AtomicBool::new(false));
        let server_runtime = Arc::clone(&runtime);
        let server_stop = Arc::clone(&stop);
        let server = thread::spawn(move || {
            camera_control_server_loop_until_stopped(server_runtime, server_stop)
        });
        let client = CameraBrokerClient::default();
        let mut ready = None;
        for _ in 0..20 {
            if let Ok(snapshot) = client.try_status() {
                ready = Some(snapshot);
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        if ready.is_none() {
            stop.store(true, Ordering::Release);
            wake_camera_control_server();
            panic!("pipe ready: server={:?}", server.join());
        }
        assert_eq!(ready.expect("pipe ready").state, CameraBrokerState::Stopped);
        assert_eq!(
            client.start().expect("start").state,
            CameraBrokerState::Active
        );
        assert_eq!(
            client.stop().expect("stop").state,
            CameraBrokerState::Stopped
        );
        server.join().expect("server thread").expect("server loop");
    }
}
