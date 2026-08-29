use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};

use crate::{
    MicrophoneHostProcess, MicrophoneHostRuntime, MicrophoneHostSnapshot,
    local_pipe::{NamedPipe, invoke, try_invoke, wake},
};

#[cfg(not(test))]
pub const MICROPHONE_CONTROL_PIPE_NAME: &str = r"\\.\pipe\CapyIO.Microphone.Control.v1";
#[cfg(test)]
pub const MICROPHONE_CONTROL_PIPE_NAME: &str = r"\\.\pipe\CapyIO.Microphone.Control.v1.test";
const CONTROL_SCHEMA_VERSION: u8 = 1;
// The host runs in the interactive user's logon session. Object-owner rights
// keep control per-user; LocalSystem and Administrators retain recovery access.
const PIPE_SDDL: &str = "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;OW)";

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
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
    snapshot: Option<MicrophoneHostSnapshot>,
    problem_code: Option<String>,
}

pub struct MicrophoneHostClient {
    next_request_id: AtomicU64,
}

impl Default for MicrophoneHostClient {
    fn default() -> Self {
        Self {
            next_request_id: AtomicU64::new(1),
        }
    }
}

impl MicrophoneHostClient {
    pub fn try_status(&self) -> Result<MicrophoneHostSnapshot, String> {
        self.invoke_with(ControlOperation::Status, true)
    }

    pub fn status(&self) -> Result<MicrophoneHostSnapshot, String> {
        self.invoke(ControlOperation::Status)
    }

    pub fn start(&self) -> Result<MicrophoneHostSnapshot, String> {
        self.invoke(ControlOperation::Start)
    }

    pub fn stop(&self) -> Result<MicrophoneHostSnapshot, String> {
        self.invoke(ControlOperation::Stop)
    }

    fn invoke(&self, operation: ControlOperation) -> Result<MicrophoneHostSnapshot, String> {
        self.invoke_with(operation, false)
    }

    fn invoke_with(
        &self,
        operation: ControlOperation,
        bounded_probe: bool,
    ) -> Result<MicrophoneHostSnapshot, String> {
        let request_id = self
            .next_request_id
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                value.checked_add(1)
            })
            .map_err(|_| "microphone host request ID exhausted".to_owned())?;
        let request = ControlRequest {
            schema_version: CONTROL_SCHEMA_VERSION,
            request_id,
            operation,
        };
        let response: ControlResponse = if bounded_probe {
            try_invoke(MICROPHONE_CONTROL_PIPE_NAME, &request)?
        } else {
            invoke(MICROPHONE_CONTROL_PIPE_NAME, &request)?
        };
        if response.schema_version != CONTROL_SCHEMA_VERSION || response.request_id != request_id {
            return Err("mismatched microphone host response".to_owned());
        }
        if !response.success {
            return Err(response
                .problem_code
                .unwrap_or_else(|| "CAPY.MICROPHONE_HOST.CONTROL_FAILED".to_owned()));
        }
        response
            .snapshot
            .ok_or_else(|| "microphone host response omitted state".to_owned())
    }
}

pub fn microphone_control_server_loop<P: MicrophoneHostProcess>(
    runtime: Arc<Mutex<MicrophoneHostRuntime<P>>>,
    stop: Arc<AtomicBool>,
) -> Result<(), String> {
    let pipe = NamedPipe::create(MICROPHONE_CONTROL_PIPE_NAME, PIPE_SDDL)?;
    while !stop.load(Ordering::Acquire) {
        if !pipe.wait_for_client(&stop)? || stop.load(Ordering::Acquire) {
            break;
        }
        let request = match pipe.read_json() {
            Ok(request) => request,
            Err(_) => {
                pipe.disconnect();
                continue;
            }
        };
        let response = dispatch_request(&runtime, request);
        let _ = pipe.write_json(&response);
        pipe.disconnect();
    }
    Ok(())
}

pub fn wake_microphone_control_server() {
    wake(MICROPHONE_CONTROL_PIPE_NAME);
}

fn dispatch_request<P: MicrophoneHostProcess>(
    runtime: &Arc<Mutex<MicrophoneHostRuntime<P>>>,
    request: ControlRequest,
) -> ControlResponse {
    if request.schema_version != CONTROL_SCHEMA_VERSION {
        return failed_response(
            request.request_id,
            "CAPY.MICROPHONE_HOST.UNSUPPORTED_CONTROL_VERSION",
        );
    }
    let Ok(mut runtime) = runtime.lock() else {
        return failed_response(request.request_id, "CAPY.MICROPHONE_HOST.STATE_UNAVAILABLE");
    };
    let result = match request.operation {
        ControlOperation::Status => Ok(runtime.snapshot()),
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
        Err(_) => {
            let snapshot = runtime.snapshot();
            ControlResponse {
                schema_version: CONTROL_SCHEMA_VERSION,
                request_id: request.request_id,
                success: false,
                problem_code: snapshot
                    .problem_code
                    .or_else(|| Some("CAPY.MICROPHONE_HOST.CONTROL_FAILED".to_owned())),
                snapshot: None,
            }
        }
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
    use crate::{MicrophoneHostStartError, MicrophoneHostState, local_pipe::MAX_CONTROL_BYTES};
    use std::{thread, time::Duration};

    static PIPE_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[derive(Default)]
    struct FakeProcess {
        running: bool,
    }

    impl MicrophoneHostProcess for FakeProcess {
        fn start(&mut self) -> Result<(), MicrophoneHostStartError> {
            self.running = true;
            Ok(())
        }

        fn running(&mut self) -> Result<bool, String> {
            Ok(self.running)
        }

        fn phone_present(&mut self) -> Result<bool, String> {
            Ok(false)
        }

        fn stop(&mut self) -> Result<(), String> {
            self.running = false;
            Ok(())
        }
    }

    fn runtime() -> Arc<Mutex<MicrophoneHostRuntime<FakeProcess>>> {
        Arc::new(Mutex::new(
            MicrophoneHostRuntime::new(
                FakeProcess::default(),
                "100.64.0.10:8554".parse().unwrap(),
                3,
                120,
            )
            .unwrap(),
        ))
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
    fn commands_are_idempotent_bounded_and_disclose_no_launch_authority() {
        let runtime = runtime();
        for operation in [
            ControlOperation::Start,
            ControlOperation::Start,
            ControlOperation::Status,
            ControlOperation::Stop,
            ControlOperation::Stop,
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
            let json = serde_json::to_vec(&response).unwrap();
            assert!(json.len() < MAX_CONTROL_BYTES);
            assert!(
                !json
                    .windows("endpoint".len())
                    .any(|value| value == b"endpoint")
            );
            assert!(
                !json
                    .windows("executable".len())
                    .any(|value| value == b"executable")
            );
        }
    }

    #[test]
    fn owner_scoped_named_pipe_round_trip_controls_fake_runtime() {
        let _guard = PIPE_TEST_LOCK.lock().unwrap();
        let runtime = runtime();
        let stop = Arc::new(AtomicBool::new(false));
        let server_runtime = Arc::clone(&runtime);
        let server_stop = Arc::clone(&stop);
        let server =
            thread::spawn(move || microphone_control_server_loop(server_runtime, server_stop));
        let client = MicrophoneHostClient::default();
        let mut ready = None;
        for _ in 0..20 {
            if let Ok(snapshot) = client.status() {
                ready = Some(snapshot);
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            ready.expect("pipe ready").state,
            MicrophoneHostState::Stopped
        );
        assert_eq!(
            client.start().expect("start").state,
            MicrophoneHostState::WaitingForPhone
        );
        assert_eq!(
            client.stop().expect("stop").state,
            MicrophoneHostState::Stopped
        );
        stop.store(true, Ordering::Release);
        wake_microphone_control_server();
        server.join().expect("server thread").expect("server loop");
    }
}
