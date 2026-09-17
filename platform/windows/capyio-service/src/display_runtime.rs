//! Service-owned display execution. Host readiness is not media readiness.
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

use crate::{SingleDisplayServiceHost, load_single_display_service_config};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DisplayExecutionState {
    Stopped,
    Starting,
    Running,
    Stopping,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayExecutionSnapshot {
    pub schema_version: u8,
    pub generation: u64,
    pub state: DisplayExecutionState,
    pub problem_code: Option<&'static str>,
}

/// Owns exactly one cooperative host thread and an independent stop signal.
/// Shutdown joins the host, including its existing worker/topology cleanup.
/// It must not run on a UI or audio callback thread.
pub struct DisplayServiceRuntime {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<(), &'static str>>>,
    ready: Option<mpsc::Receiver<()>>,
    startup_deadline: Option<Instant>,
    snapshot: DisplayExecutionSnapshot,
}

impl Default for DisplayServiceRuntime {
    fn default() -> Self {
        Self {
            stop: Arc::new(AtomicBool::new(false)),
            worker: None,
            ready: None,
            startup_deadline: None,
            snapshot: DisplayExecutionSnapshot {
                schema_version: 1,
                generation: 0,
                state: DisplayExecutionState::Stopped,
                problem_code: None,
            },
        }
    }
}

impl DisplayServiceRuntime {
    pub fn start(&mut self, path: PathBuf) -> Result<(), &'static str> {
        self.start_with(move || {
            let config = load_single_display_service_config(&path)
                .map_err(|_| "CAPY.DISPLAY.CONFIGURATION_INVALID")?;
            let host = SingleDisplayServiceHost::open(config)
                .map_err(|_| "CAPY.DISPLAY.HOST_OPEN_FAILED")?;
            Ok(move |stop| host.run(stop).map_err(|_| "CAPY.DISPLAY.HOST_FAILED"))
        })
    }

    pub(super) fn start_with<F, R>(&mut self, open: F) -> Result<(), &'static str>
    where
        F: FnOnce() -> Result<R, &'static str> + Send + 'static,
        R: FnOnce(Arc<AtomicBool>) -> Result<(), &'static str> + 'static,
    {
        self.poll();
        if self.worker.is_some() {
            return Err("CAPY.DISPLAY.HOST_BUSY");
        }
        let generation = self
            .snapshot
            .generation
            .checked_add(1)
            .ok_or("CAPY.DISPLAY.GENERATION_EXHAUSTED")?;
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let (sender, receiver) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("capyio-service-display".into())
            .spawn(move || {
                if worker_stop.load(Ordering::Acquire) {
                    return Ok(());
                }
                let run = open()?;
                let _ = sender.send(());
                // Even cancellation during open enters run so the host cleans up.
                let result = run(Arc::clone(&worker_stop));
                if result.is_ok() && !worker_stop.load(Ordering::Acquire) {
                    Err("CAPY.DISPLAY.HOST_EXITED")
                } else {
                    result
                }
            })
            .map_err(|_| {
                self.snapshot.state = DisplayExecutionState::Failed;
                self.snapshot.problem_code = Some("CAPY.DISPLAY.HOST_SPAWN_FAILED");
                "CAPY.DISPLAY.HOST_SPAWN_FAILED"
            })?;
        self.stop = stop;
        self.worker = Some(worker);
        self.ready = Some(receiver);
        self.startup_deadline = Some(Instant::now() + Duration::from_secs(10));
        self.snapshot = DisplayExecutionSnapshot {
            schema_version: 1,
            generation,
            state: DisplayExecutionState::Starting,
            problem_code: None,
        };
        Ok(())
    }

    pub fn snapshot(&self) -> DisplayExecutionSnapshot {
        self.snapshot.clone()
    }

    /// Nonblocking; a display exit never requests service/audio shutdown.
    pub fn poll(&mut self) {
        if self
            .ready
            .as_ref()
            .is_some_and(|ready| ready.try_recv().is_ok())
        {
            self.ready = None;
            self.startup_deadline = None;
            if self.snapshot.state == DisplayExecutionState::Starting {
                self.snapshot.state = DisplayExecutionState::Running;
            }
        }
        if self.worker.as_ref().is_some_and(JoinHandle::is_finished) {
            self.join_finished();
        }
        if self.snapshot.state == DisplayExecutionState::Starting
            && self
                .startup_deadline
                .is_some_and(|deadline| Instant::now() >= deadline)
        {
            self.stop.store(true, Ordering::Release);
            self.snapshot.state = DisplayExecutionState::Stopping;
            self.snapshot.problem_code = Some("CAPY.DISPLAY.HOST_START_TIMEOUT");
            self.startup_deadline = None;
        }
    }

    pub fn request_stop(&mut self) {
        self.poll();
        if self.worker.is_some() {
            self.stop.store(true, Ordering::Release);
            self.snapshot.state = DisplayExecutionState::Stopping;
        }
    }

    pub fn shutdown(&mut self) -> DisplayExecutionSnapshot {
        self.request_stop();
        self.join_finished();
        self.snapshot()
    }

    fn join_finished(&mut self) {
        let Some(worker) = self.worker.take() else {
            return;
        };
        self.ready = None;
        self.startup_deadline = None;
        let problem = match worker.join() {
            Ok(Ok(())) if self.stop.load(Ordering::Acquire) => None,
            Ok(Ok(())) => Some("CAPY.DISPLAY.HOST_EXITED"),
            Ok(Err(problem)) => Some(problem),
            Err(_) => Some("CAPY.DISPLAY.HOST_PANICKED"),
        };
        self.snapshot.problem_code = problem.or(self.snapshot.problem_code);
        self.snapshot.state = if self.snapshot.problem_code.is_some() {
            DisplayExecutionState::Failed
        } else {
            DisplayExecutionState::Stopped
        };
    }
}

impl Drop for DisplayServiceRuntime {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn optional_display_config_rejects_relative_missing_and_duplicate_paths() {
        let base = vec![
            "service",
            "--broker",
            "broker.exe",
            "--bind-ip",
            "127.0.0.1",
            "--port",
            "40101",
        ];
        let absolute = std::env::temp_dir().join("capyio-display-config-fixture.json");
        let path = absolute.to_str().unwrap();
        let mut args = base.clone();
        args.extend(["--single-display-config", path]);
        let config = crate::ServiceConfig::parse(args.clone()).unwrap();
        assert_eq!(config.single_display_config, Some(absolute.clone()));
        args.extend(["--single-display-config", path]);
        assert_eq!(
            crate::ServiceConfig::parse(args).unwrap_err(),
            crate::ConfigError::InvalidDisplayConfig
        );
        for invalid in ["", "relative.json"] {
            let mut args = base.clone();
            args.extend(["--single-display-config", invalid]);
            assert_eq!(
                crate::ServiceConfig::parse(args).unwrap_err(),
                crate::ConfigError::InvalidDisplayConfig
            );
        }
        let mut args = base;
        args.push("--single-display-config");
        assert_eq!(
            crate::ServiceConfig::parse(args).unwrap_err(),
            crate::ConfigError::MissingValue("--single-display-config")
        );
    }

    #[derive(Default)]
    struct FakeAudio {
        running: bool,
    }
    impl crate::BrokerProcess for FakeAudio {
        fn start(&mut self) -> Result<(), &'static str> {
            self.running = true;
            Ok(())
        }
        fn running(&mut self) -> Result<bool, String> {
            Ok(self.running)
        }
        fn receiver_present(&mut self) -> Result<bool, String> {
            Ok(self.running)
        }
        fn stop(&mut self) -> Result<(), String> {
            self.running = false;
            Ok(())
        }
    }

    #[test]
    fn display_failure_preserves_active_audio_runtime_snapshot() {
        let mut audio = crate::BrokerServiceRuntime::new(FakeAudio::default(), 1).unwrap();
        audio.start().unwrap();
        audio.poll();
        assert_eq!(audio.snapshot().state, crate::BrokerServiceState::Active);
        let before = audio.snapshot();
        let mut display = DisplayServiceRuntime::default();
        display
            .start_with(|| Ok(|_| Err("CAPY.DISPLAY.HOST_FAILED")))
            .unwrap();
        wait(&mut display, DisplayExecutionState::Failed);
        display.shutdown();
        audio.poll();
        assert_eq!(audio.snapshot(), before);
        audio.stop().unwrap();
    }

    fn wait(runtime: &mut DisplayServiceRuntime, state: DisplayExecutionState) {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            runtime.poll();
            if runtime.snapshot().state == state {
                break;
            }
            assert!(Instant::now() < deadline, "{:?}", runtime.snapshot());
            thread::yield_now();
        }
    }

    fn cooperative(stop: Arc<AtomicBool>) -> Result<(), &'static str> {
        while !stop.load(Ordering::Acquire) {
            thread::yield_now();
        }
        Ok(())
    }

    #[test]
    fn independent_host_failure_does_not_stop_other_execution() {
        let mut healthy = DisplayServiceRuntime::default();
        healthy.start_with(|| Ok(cooperative)).unwrap();
        wait(&mut healthy, DisplayExecutionState::Running);
        let mut failing = DisplayServiceRuntime::default();
        failing
            .start_with(|| Ok(|_| Err("CAPY.DISPLAY.HOST_FAILED")))
            .unwrap();
        wait(&mut failing, DisplayExecutionState::Failed);
        healthy.poll();
        assert_eq!(healthy.snapshot().state, DisplayExecutionState::Running);
        assert!(!healthy.stop.load(Ordering::Acquire));
        assert_eq!(healthy.shutdown().state, DisplayExecutionState::Stopped);
    }

    #[test]
    fn duplicate_start_rejected_and_restart_has_new_generation() {
        let mut runtime = DisplayServiceRuntime::default();
        runtime.start_with(|| Ok(cooperative)).unwrap();
        assert_eq!(
            runtime.start_with(|| Ok(cooperative)),
            Err("CAPY.DISPLAY.HOST_BUSY")
        );
        runtime.shutdown();
        runtime.start_with(|| Ok(cooperative)).unwrap();
        assert_eq!(runtime.snapshot().generation, 2);
        runtime.shutdown();
        assert_eq!(runtime.shutdown().state, DisplayExecutionState::Stopped);
    }

    #[test]
    fn unexpected_clean_exit_is_not_successful_streaming() {
        let mut runtime = DisplayServiceRuntime::default();
        runtime.start_with(|| Ok(|_| Ok(()))).unwrap();
        wait(&mut runtime, DisplayExecutionState::Failed);
        assert_eq!(
            runtime.shutdown().problem_code,
            Some("CAPY.DISPLAY.HOST_EXITED")
        );
    }

    #[test]
    fn open_failure_is_retained_after_stop() {
        let mut runtime = DisplayServiceRuntime::default();
        runtime
            .start_with(|| {
                Err::<fn(Arc<AtomicBool>) -> Result<(), &'static str>, _>(
                    "CAPY.DISPLAY.CONFIGURATION_INVALID",
                )
            })
            .unwrap();
        wait(&mut runtime, DisplayExecutionState::Failed);
        assert_eq!(
            runtime.shutdown().problem_code,
            Some("CAPY.DISPLAY.CONFIGURATION_INVALID")
        );
    }

    #[test]
    fn cancellation_during_open_still_runs_cleanup() {
        let mut runtime = DisplayServiceRuntime::default();
        let (opened, observed) = mpsc::sync_channel(1);
        let (release, blocked) = mpsc::sync_channel(1);
        let cleaned = Arc::new(AtomicBool::new(false));
        let worker_cleaned = Arc::clone(&cleaned);
        runtime
            .start_with(move || {
                opened.send(()).unwrap();
                blocked.recv_timeout(Duration::from_secs(3)).unwrap();
                Ok(move |stop: Arc<AtomicBool>| {
                    assert!(stop.load(Ordering::Acquire));
                    worker_cleaned.store(true, Ordering::Release);
                    Ok(())
                })
            })
            .unwrap();
        observed.recv_timeout(Duration::from_secs(3)).unwrap();
        runtime.request_stop();
        release.send(()).unwrap();
        assert_eq!(runtime.shutdown().state, DisplayExecutionState::Stopped);
        assert!(cleaned.load(Ordering::Acquire));
    }

    #[test]
    fn startup_timeout_cancels_only_owned_task_and_blocks_restart_until_reaped() {
        let mut runtime = DisplayServiceRuntime::default();
        let (entered, observed) = mpsc::sync_channel(1);
        let (release, blocked) = mpsc::sync_channel(1);
        runtime
            .start_with(move || {
                entered.send(()).unwrap();
                blocked.recv_timeout(Duration::from_secs(3)).unwrap();
                Ok(cooperative)
            })
            .unwrap();
        observed.recv_timeout(Duration::from_secs(3)).unwrap();
        runtime.startup_deadline = Some(Instant::now());
        runtime.poll();
        assert_eq!(runtime.snapshot().state, DisplayExecutionState::Stopping);
        assert_eq!(
            runtime.snapshot().problem_code,
            Some("CAPY.DISPLAY.HOST_START_TIMEOUT")
        );
        assert_eq!(
            runtime.start_with(|| Ok(cooperative)),
            Err("CAPY.DISPLAY.HOST_BUSY")
        );
        release.send(()).unwrap();
        let final_state = runtime.shutdown();
        assert_eq!(final_state.state, DisplayExecutionState::Failed);
        assert_eq!(
            final_state.problem_code,
            Some("CAPY.DISPLAY.HOST_START_TIMEOUT")
        );
    }

    #[test]
    fn host_resources_can_remain_thread_affine() {
        let mut runtime = DisplayServiceRuntime::default();
        runtime
            .start_with(|| {
                let resource = std::rc::Rc::new(());
                Ok(move |stop| {
                    cooperative(stop)?;
                    drop(resource);
                    Ok(())
                })
            })
            .unwrap();
        wait(&mut runtime, DisplayExecutionState::Running);
        assert_eq!(runtime.shutdown().state, DisplayExecutionState::Stopped);
    }

    #[test]
    fn drop_joins_owned_execution() {
        let cleaned = Arc::new(AtomicBool::new(false));
        let worker_cleaned = Arc::clone(&cleaned);
        let mut runtime = DisplayServiceRuntime::default();
        runtime
            .start_with(move || {
                Ok(move |stop| {
                    cooperative(stop)?;
                    worker_cleaned.store(true, Ordering::Release);
                    Ok(())
                })
            })
            .unwrap();
        wait(&mut runtime, DisplayExecutionState::Running);
        drop(runtime);
        assert!(cleaned.load(Ordering::Acquire));
    }

    #[test]
    fn panic_is_a_retained_failure() {
        let mut runtime = DisplayServiceRuntime::default();
        runtime
            .start_with(|| Ok(|_| panic!("synthetic host panic")))
            .unwrap();
        wait(&mut runtime, DisplayExecutionState::Failed);
        assert_eq!(
            runtime.shutdown().problem_code,
            Some("CAPY.DISPLAY.HOST_PANICKED")
        );
    }
}
