use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};

use crate::{
    AudioShareSupervisor, BrokerProcess, BrokerServiceRuntime, BrokerServiceSnapshot,
    local_pipe::{NamedPipe, invoke, wake},
};

pub const CONTROL_PIPE_NAME: &str = r"\\.\pipe\CapyIO.Broker.Control.v1";
const CONTROL_SCHEMA_VERSION: u8 = 1;
const PIPE_SDDL: &str = "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;0x12019f;;;IU)";

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
    snapshot: Option<BrokerServiceSnapshot>,
    problem_code: Option<String>,
}

pub struct BrokerServiceClient {
    next_request_id: AtomicU64,
}

impl Default for BrokerServiceClient {
    fn default() -> Self {
        Self {
            next_request_id: AtomicU64::new(1),
        }
    }
}

impl BrokerServiceClient {
    pub fn status(&self) -> Result<BrokerServiceSnapshot, String> {
        self.invoke(ControlOperation::Status)
    }

    pub fn start(&self) -> Result<BrokerServiceSnapshot, String> {
        self.invoke(ControlOperation::Start)
    }

    pub fn stop(&self) -> Result<BrokerServiceSnapshot, String> {
        self.invoke(ControlOperation::Stop)
    }

    fn invoke(&self, operation: ControlOperation) -> Result<BrokerServiceSnapshot, String> {
        let request_id = self
            .next_request_id
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                value.checked_add(1)
            })
            .map_err(|_| "CapyIO service request ID exhausted".to_owned())?;
        let response: ControlResponse = invoke(
            CONTROL_PIPE_NAME,
            &ControlRequest {
                schema_version: CONTROL_SCHEMA_VERSION,
                request_id,
                operation,
            },
        )?;
        if response.schema_version != CONTROL_SCHEMA_VERSION || response.request_id != request_id {
            return Err("mismatched CapyIO service response".to_owned());
        }
        if !response.success {
            return Err(response
                .problem_code
                .unwrap_or_else(|| "CAPY.WINDOWS_SERVICE.CONTROL_FAILED".to_owned()));
        }
        response
            .snapshot
            .ok_or_else(|| "CapyIO service response omitted state".to_owned())
    }
}

pub fn control_server_loop(
    runtime: Arc<Mutex<BrokerServiceRuntime<AudioShareSupervisor>>>,
    stop: Arc<AtomicBool>,
) -> Result<(), String> {
    let pipe = NamedPipe::create(CONTROL_PIPE_NAME, PIPE_SDDL)?;
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

pub fn wake_control_server() {
    wake(CONTROL_PIPE_NAME);
}

fn dispatch_request<P: BrokerProcess>(
    runtime: &Arc<Mutex<BrokerServiceRuntime<P>>>,
    request: ControlRequest,
) -> ControlResponse {
    if request.schema_version != CONTROL_SCHEMA_VERSION {
        return failed_response(
            request.request_id,
            "CAPY.WINDOWS_SERVICE.UNSUPPORTED_CONTROL_VERSION",
        );
    }
    let Ok(mut runtime) = runtime.lock() else {
        return failed_response(request.request_id, "CAPY.WINDOWS_SERVICE.STATE_UNAVAILABLE");
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
        Err(_) => failed_response(request.request_id, "CAPY.WINDOWS_SERVICE.CONTROL_FAILED"),
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

    #[derive(Default)]
    struct FakeBroker {
        running: bool,
        receiver: bool,
    }

    impl BrokerProcess for FakeBroker {
        fn start(&mut self) -> Result<(), String> {
            self.running = true;
            Ok(())
        }

        fn running(&mut self) -> Result<bool, String> {
            Ok(self.running)
        }

        fn receiver_present(&mut self) -> Result<bool, String> {
            Ok(self.receiver)
        }

        fn stop(&mut self) -> Result<(), String> {
            self.running = false;
            Ok(())
        }
    }

    #[test]
    fn closed_request_rejects_unknown_fields_and_versions() {
        assert!(
            serde_json::from_str::<ControlRequest>(
                r#"{"schemaVersion":1,"requestId":1,"operation":"status","path":"x"}"#
            )
            .is_err()
        );
        let runtime = Arc::new(Mutex::new(
            BrokerServiceRuntime::new(FakeBroker::default(), 1).expect("runtime"),
        ));
        let response = dispatch_request(
            &runtime,
            ControlRequest {
                schema_version: 2,
                request_id: 7,
                operation: ControlOperation::Status,
            },
        );
        assert!(!response.success);
        assert_eq!(response.request_id, 7);
    }

    #[test]
    fn commands_are_idempotent_and_return_bounded_state() {
        let runtime = Arc::new(Mutex::new(
            BrokerServiceRuntime::new(FakeBroker::default(), 1).expect("runtime"),
        ));
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
            assert!(serde_json::to_vec(&response).unwrap().len() < MAX_CONTROL_BYTES);
        }
    }
}
