//! Service-owned display execution. Host readiness is not media readiness.
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

use crate::SingleDisplayServiceHost;
use crate::display_host::{
    DisplaySessionSettings, load_single_display_service_config_with_settings,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DisplayPeerLaunchRequest {
    pub session_id: String,
    pub route_id: String,
    pub stream_id: String,
    pub stream_epoch: u64,
    pub sink_capability_id: String,
    pub host_ipv4: String,
    pub settings: DisplaySessionSettings,
    pub expires_at_ms: u64,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DisplayPeerLaunchPrepared {
    pub generation: u64,
    pub launch_record: Vec<u8>,
}
pub(crate) fn peer_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn peer_host_problem(
    status: &crate::SingleDisplayStatusHandle,
    fallback: &'static str,
) -> &'static str {
    match status.snapshot().problem_code.as_deref() {
        Some("CAPY.DISPLAY.ENGINE_READY_FAILED") => "CAPY.DISPLAY.ENGINE_READY_FAILED",
        Some("CAPY.DISPLAY.ENGINE_READY_TIMEOUT") => "CAPY.DISPLAY.ENGINE_READY_TIMEOUT",
        Some("CAPY.DISPLAY.ENGINE_READY_PIPE_FAILED") => "CAPY.DISPLAY.ENGINE_READY_PIPE_FAILED",
        Some("CAPY.DISPLAY.ENGINE_READY_ACK_INVALID") => "CAPY.DISPLAY.ENGINE_READY_ACK_INVALID",
        Some("CAPY.DISPLAY.ENGINE_WORKER_FAILED") => "CAPY.DISPLAY.ENGINE_WORKER_FAILED",
        Some(code) => {
            crate::display_worker_failure::retained_display_worker_problem(code).unwrap_or(fallback)
        }
        None => fallback,
    }
}

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

type PeerCommit = (Vec<u8>, mpsc::SyncSender<Result<(), &'static str>>);

/// Owns exactly one cooperative host thread and an independent stop signal.
/// Shutdown joins the host, including its existing worker/topology cleanup.
/// It must not run on a UI or audio callback thread.
pub struct DisplayServiceRuntime {
    peer_commit: Option<mpsc::SyncSender<PeerCommit>>,
    peer_result: Option<mpsc::Receiver<Result<(), &'static str>>>,
    peer_deadline: Option<Arc<AtomicU64>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<(), &'static str>>>,
    ready: Option<mpsc::Receiver<()>>,
    startup_deadline: Option<Instant>,
    snapshot: DisplayExecutionSnapshot,
}

impl Default for DisplayServiceRuntime {
    fn default() -> Self {
        Self {
            peer_commit: None,
            peer_result: None,
            peer_deadline: None,
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
    pub fn prepare_peer(
        &mut self,
        path: PathBuf,
        request: DisplayPeerLaunchRequest,
    ) -> Result<DisplayPeerLaunchPrepared, &'static str> {
        request.settings.validate()?;
        let remaining = request.expires_at_ms.saturating_sub(peer_now_ms());
        if remaining == 0 || remaining > 600_000 || request.stream_epoch == 0 {
            return Err("CAPY.DISPLAY.PEER_LEASE_INVALID");
        }
        let (prepared_tx, prepared_rx) = mpsc::sync_channel(1);
        let deadline = Arc::new(AtomicU64::new(request.expires_at_ms));
        let host_deadline = Arc::clone(&deadline);
        let (commit_tx, commit_rx) =
            mpsc::sync_channel::<(Vec<u8>, mpsc::SyncSender<Result<(), &'static str>>)>(1);
        self.start_with(move || {
            let config =
                crate::display_host::load_peer_display_service_config(&path, request.settings)
                    .map_err(|_| "CAPY.DISPLAY.CONFIGURATION_INVALID")?;
            let mut host = SingleDisplayServiceHost::open_peer(config)
                .map_err(|_| "CAPY.DISPLAY.HOST_OPEN_FAILED")?;
            let pending = host
                .prepare_peer(&request, host_deadline)
                .map_err(|_| "CAPY.DISPLAY.PEER_PREPARE_FAILED")?;
            let status = host.status_handle();
            let _ = prepared_tx.send(pending.record().to_vec());
            Ok(move |stop: Arc<AtomicBool>| {
                let deadline = Instant::now() + Duration::from_secs(30);
                let mut pending = Some(pending);
                loop {
                    if stop.load(Ordering::Acquire)
                        || Instant::now() >= deadline
                        || peer_now_ms() >= request.expires_at_ms
                    {
                        let cancelled = stop.load(Ordering::Acquire);
                        stop.store(true, Ordering::Release);
                        host.run(stop).map_err(|_| "CAPY.DISPLAY.HOST_FAILED")?;
                        return if cancelled {
                            Ok(())
                        } else {
                            Err("CAPY.DISPLAY.PEER_ACK_TIMEOUT")
                        };
                    }
                    match commit_rx.recv_timeout(Duration::from_millis(25)) {
                        Ok((mut ack, reply)) => {
                            let result =
                                host.commit_peer(pending.take().unwrap(), &ack)
                                    .map_err(|_| {
                                        peer_host_problem(
                                            &status,
                                            "CAPY.DISPLAY.PEER_COMMIT_FAILED",
                                        )
                                    });
                            ack.fill(0);
                            let _ = reply.send(result);
                            if result.is_err() {
                                stop.store(true, Ordering::Release);
                            }
                            let run = host.run(stop).map_err(|e| {
                                if e == "CAPY.DISPLAY.LEASE_EXPIRED" {
                                    "CAPY.DISPLAY.LEASE_EXPIRED"
                                } else {
                                    peer_host_problem(&status, "CAPY.DISPLAY.HOST_FAILED")
                                }
                            });
                            return result.and(run);
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(_) => {
                            stop.store(true, Ordering::Release);
                            return host.run(stop).map_err(|_| "CAPY.DISPLAY.HOST_FAILED");
                        }
                    }
                }
            })
        })?;
        self.peer_commit = Some(commit_tx);
        self.peer_deadline = Some(deadline);
        self.startup_deadline = Some(Instant::now() + Duration::from_secs(35));
        match prepared_rx.recv_timeout(Duration::from_secs(8)) {
            Ok(launch_record) => Ok(DisplayPeerLaunchPrepared {
                generation: self.snapshot.generation,
                launch_record,
            }),
            Err(_) => {
                self.request_stop();
                Err("CAPY.DISPLAY.PEER_PREPARE_FAILED")
            }
        }
    }
    pub fn commit_peer(
        &mut self,
        generation: u64,
        acknowledgement: Vec<u8>,
    ) -> Result<(), &'static str> {
        if generation != self.snapshot.generation || acknowledgement.len() != 64 {
            return Err("CAPY.DISPLAY.PEER_ACK_INVALID");
        }
        let sender = self
            .peer_commit
            .take()
            .ok_or("CAPY.DISPLAY.PEER_ACK_INVALID")?;
        let (tx, rx) = mpsc::sync_channel(1);
        sender
            .send((acknowledgement, tx))
            .map_err(|_| "CAPY.DISPLAY.PEER_COMMIT_FAILED")?;
        self.peer_result = Some(rx);
        self.snapshot.state = DisplayExecutionState::Starting;
        self.startup_deadline = Some(Instant::now() + Duration::from_secs(40));
        Ok(())
    }
    pub fn renew_peer(&mut self, generation: u64, expires_at_ms: u64) -> Result<(), &'static str> {
        self.poll();
        let now = peer_now_ms();
        if generation != self.snapshot.generation
            || !matches!(
                self.snapshot.state,
                DisplayExecutionState::Starting | DisplayExecutionState::Running
            )
            || expires_at_ms <= now
            || expires_at_ms.saturating_sub(now) > 600_000
        {
            return Err("CAPY.DISPLAY.PEER_LEASE_INVALID");
        }
        let deadline = self
            .peer_deadline
            .as_ref()
            .ok_or("CAPY.DISPLAY.PEER_LEASE_INVALID")?;
        if deadline.load(Ordering::Acquire) <= now {
            return Err("CAPY.DISPLAY.PEER_LEASE_INVALID");
        }
        deadline.store(expires_at_ms, Ordering::Release);
        Ok(())
    }
    pub fn start(&mut self, path: PathBuf) -> Result<(), &'static str> {
        self.start_with_settings(path, None)
    }

    pub fn start_with_settings(
        &mut self,
        path: PathBuf,
        settings: Option<DisplaySessionSettings>,
    ) -> Result<(), &'static str> {
        if let Some(settings) = settings {
            settings.validate()?;
        }
        self.start_with(move || {
            let config = load_single_display_service_config_with_settings(&path, settings)
                .map_err(|_| "CAPY.DISPLAY.CONFIGURATION_INVALID")?;
            let host = SingleDisplayServiceHost::open(config)
                .map_err(|_| "CAPY.DISPLAY.HOST_OPEN_FAILED")?;
            let status = host.status_handle();
            Ok(move |stop| {
                host.run(stop)
                    .map_err(|_| match status.snapshot().problem_code.as_deref() {
                        Some("CAPY.DISPLAY.ENGINE_WORKER_FAILED") => {
                            "CAPY.DISPLAY.ENGINE_WORKER_FAILED"
                        }
                        Some(code) => {
                            crate::display_worker_failure::retained_display_worker_problem(code)
                                .unwrap_or("CAPY.DISPLAY.HOST_FAILED")
                        }
                        _ => "CAPY.DISPLAY.HOST_FAILED",
                    })
            })
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
            if self.peer_commit.is_none()
                && self.peer_result.is_none()
                && self.snapshot.state == DisplayExecutionState::Starting
            {
                self.startup_deadline = None;
                self.snapshot.state = DisplayExecutionState::Running;
            }
        }
        if let Some(result) = self.peer_result.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.peer_result = None;
            self.startup_deadline = None;
            match result {
                Ok(()) if self.snapshot.state == DisplayExecutionState::Starting => {
                    self.snapshot.state = DisplayExecutionState::Running
                }
                Err(problem) => {
                    self.snapshot.problem_code = Some(problem);
                    self.snapshot.state = DisplayExecutionState::Stopping;
                    self.stop.store(true, Ordering::Release);
                }
                _ => {}
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
        self.peer_commit = None;
        self.poll();
        if self.worker.is_some() {
            self.stop.store(true, Ordering::Release);
            self.snapshot.state = DisplayExecutionState::Stopping;
        }
    }

    pub fn stop_peer(&mut self, generation: u64) -> Result<(), &'static str> {
        self.poll();
        if generation == 0 || generation != self.snapshot.generation {
            return Err("CAPY.DISPLAY.PEER_GENERATION_MISMATCH");
        }
        // A completed stop for this generation is idempotent. An active
        // legacy/manual host is not owned by this peer API.
        if matches!(
            self.snapshot.state,
            DisplayExecutionState::Stopped | DisplayExecutionState::Failed
        ) {
            return Ok(());
        }
        if self.peer_deadline.is_none() {
            return Err("CAPY.DISPLAY.PEER_GENERATION_MISMATCH");
        }
        self.request_stop();
        Ok(())
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
        self.peer_commit = None;
        self.peer_result = None;
        self.peer_deadline = None;
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
    fn stale_peer_stop_does_not_stop_new_generation() {
        let mut runtime = DisplayServiceRuntime::default();
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
        runtime.peer_deadline = Some(Arc::new(AtomicU64::new(peer_now_ms() + 60_000)));
        assert!(runtime.stop_peer(2).is_err());
        assert!(!runtime.stop.load(Ordering::Acquire));
        runtime.stop_peer(1).unwrap();
        assert!(runtime.stop.load(Ordering::Acquire));
        runtime.shutdown();
        runtime.stop_peer(1).unwrap();
    }

    #[test]
    fn peer_commit_is_nonblocking_and_reports_worker_result() {
        let mut runtime = DisplayServiceRuntime::default();
        let (tx, rx) = mpsc::sync_channel(1);
        runtime.peer_commit = Some(tx);
        runtime.snapshot.generation = 8;
        runtime.commit_peer(8, vec![3; 64]).unwrap();
        assert_eq!(runtime.snapshot.state, DisplayExecutionState::Starting);
        assert!(runtime.commit_peer(8, vec![3; 64]).is_err());
        let (ack, reply) = rx.try_recv().unwrap();
        assert_eq!(ack, vec![3; 64]);
        reply.send(Ok(())).unwrap();
        runtime.poll();
        assert_eq!(runtime.snapshot.state, DisplayExecutionState::Running);
        let (tx, rx) = mpsc::sync_channel(1);
        runtime.peer_commit = Some(tx);
        runtime.commit_peer(8, vec![4; 64]).unwrap();
        rx.try_recv()
            .unwrap()
            .1
            .send(Err("CAPY.DISPLAY.PEER_COMMIT_FAILED"))
            .unwrap();
        runtime.poll();
        assert_eq!(
            runtime.snapshot.problem_code,
            Some("CAPY.DISPLAY.PEER_COMMIT_FAILED")
        );
    }

    #[test]
    fn peer_prepare_rejects_expired_or_unbounded_grant_before_open() {
        let mut runtime = DisplayServiceRuntime::default();
        let mut request = DisplayPeerLaunchRequest {
            session_id: String::new(),
            route_id: String::new(),
            stream_id: String::new(),
            stream_epoch: 1,
            sink_capability_id: String::new(),
            host_ipv4: "127.0.0.1".into(),
            settings: DisplaySessionSettings {
                width: 1280,
                height: 720,
                refresh_millihertz: 60_000,
                bitrate_kbps: 8000,
            },
            expires_at_ms: 0,
        };
        assert_eq!(
            runtime
                .prepare_peer(PathBuf::from("never-read"), request.clone())
                .err(),
            Some("CAPY.DISPLAY.PEER_LEASE_INVALID")
        );
        request.expires_at_ms = peer_now_ms() + 700_000;
        assert_eq!(
            runtime
                .prepare_peer(PathBuf::from("never-read"), request)
                .err(),
            Some("CAPY.DISPLAY.PEER_LEASE_INVALID")
        );
        assert_eq!(runtime.snapshot.generation, 0);
    }

    #[test]
    fn peer_renew_is_generation_scoped_and_cannot_revive_expiry() {
        let mut runtime = DisplayServiceRuntime::default();
        let now = peer_now_ms();
        let deadline = Arc::new(AtomicU64::new(now + 60_000));
        runtime.peer_deadline = Some(Arc::clone(&deadline));
        runtime.snapshot.generation = 3;
        runtime.snapshot.state = DisplayExecutionState::Running;
        assert!(runtime.renew_peer(2, now + 120_000).is_err());
        runtime.renew_peer(3, now + 120_000).unwrap();
        assert_eq!(deadline.load(Ordering::Acquire), now + 120_000);
        deadline.store(now - 1, Ordering::Release);
        assert!(runtime.renew_peer(3, now + 120_000).is_err());
    }

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
