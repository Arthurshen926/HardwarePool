//! Closed, non-secret worker exit diagnostics. Not a wire authorization signal.
#[derive(Clone, Copy)]
#[repr(i32)]
pub enum DisplayWorkerStage {
    Handshake = 40,
    Session = 41,
    EngineLoad = 42,
    MonitorCreate = 43,
    OutputBind = 44,
    EngineStart = 45,
    Streaming = 46,
    Cleanup = 47,
    DisplayModeChanged = 48,
}

pub fn display_worker_problem(exit_code: u32) -> &'static str {
    match exit_code {
        40 => "CAPY.DISPLAY.WORKER_HANDSHAKE_FAILED",
        41 => "CAPY.DISPLAY.WORKER_SESSION_FAILED",
        42 => "CAPY.DISPLAY.ENGINE_LOAD_FAILED",
        43 => "CAPY.DISPLAY.MONITOR_CREATE_FAILED",
        44 => "CAPY.DISPLAY.OUTPUT_BIND_FAILED",
        45 => "CAPY.DISPLAY.ENGINE_START_FAILED",
        46 => "CAPY.DISPLAY.STREAMING_FAILED",
        47 => "CAPY.DISPLAY.WORKER_CLEANUP_FAILED",
        48 => "CAPY.DISPLAY.DISPLAY_MODE_CHANGED",
        _ => "CAPY.DISPLAY.ENGINE_WORKER_FAILED",
    }
}

pub(crate) fn retained_display_worker_problem(code: &str) -> Option<&'static str> {
    (40..=48)
        .map(display_worker_problem)
        .find(|candidate| *candidate == code)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_closed_stage_codes_are_retained() {
        for code in 40..=48 {
            let problem = display_worker_problem(code);
            assert_eq!(retained_display_worker_problem(problem), Some(problem));
        }
        assert_eq!(
            display_worker_problem(0xc0000005),
            "CAPY.DISPLAY.ENGINE_WORKER_FAILED"
        );
        assert_eq!(retained_display_worker_problem("arbitrary detail"), None);
    }
}
