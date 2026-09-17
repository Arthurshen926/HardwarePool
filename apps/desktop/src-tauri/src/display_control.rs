//! UI observes/controls the service, never owns the display worker.
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiDisplaySenderState {
    schema_version: u8,
    available: bool,
    state: String,
    generation: u64,
    problem_code: Option<String>,
}

#[derive(Clone, Copy)]
enum Operation {
    Status,
    Start,
    Stop,
}

async fn execute(operation: Operation) -> Result<Option<UiDisplaySenderState>, String> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(move || {
            use capyio_windows_service::{DisplayControlOperation, DisplayServiceClient};
            let command = match operation {
                Operation::Status => DisplayControlOperation::Status,
                Operation::Start => DisplayControlOperation::Start,
                Operation::Stop => DisplayControlOperation::Stop,
            };
            match DisplayServiceClient::default().invoke(command) {
                Ok(snapshot) => Ok(Some(UiDisplaySenderState {
                    schema_version: 1,
                    available: true,
                    generation: snapshot.generation,
                    state: match snapshot.state {
                        capyio_windows_service::DisplayExecutionState::Stopped => "stopped",
                        capyio_windows_service::DisplayExecutionState::Starting => "starting",
                        capyio_windows_service::DisplayExecutionState::Running => "running",
                        capyio_windows_service::DisplayExecutionState::Stopping => "stopping",
                        capyio_windows_service::DisplayExecutionState::Failed => "failed",
                    }
                    .into(),
                    problem_code: snapshot.problem_code,
                })),
                Err(_) if matches!(operation, Operation::Status) => {
                    Ok(Some(UiDisplaySenderState {
                        schema_version: 1,
                        available: false,
                        state: "unavailable".into(),
                        generation: 0,
                        problem_code: Some("CAPY.DISPLAY.CONTROL_UNAVAILABLE".into()),
                    }))
                }
                Err(error) => Err(error),
            }
        })
        .await
        .map_err(|_| "CAPY.DISPLAY.CONTROL_TASK_FAILED".to_owned())?
    }
    #[cfg(not(windows))]
    {
        match operation {
            Operation::Status => Ok(None),
            _ => Err("CAPY.DISPLAY.PLATFORM_UNSUPPORTED".into()),
        }
    }
}

#[tauri::command]
pub async fn get_display_sender_state() -> Result<Option<UiDisplaySenderState>, String> {
    execute(Operation::Status).await
}
#[tauri::command]
pub async fn start_display_sender() -> Result<Option<UiDisplaySenderState>, String> {
    execute(Operation::Start).await
}
#[tauri::command]
pub async fn stop_display_sender() -> Result<Option<UiDisplaySenderState>, String> {
    execute(Operation::Stop).await
}
