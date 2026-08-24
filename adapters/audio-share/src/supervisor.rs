use std::{
    net::TcpStream,
    process::{Child, Command, ExitStatus, Stdio},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crate::{
    AudioShareConfig, AudioShareError, AudioShareProbe, BoundedRead, DEFAULT_PROBE_OUTPUT_LIMIT,
    ProbeLimits, ReceiverTcpPresence, configure_hidden_process, join_reader,
    peer_presence::receiver_tcp_presence, read_bounded,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SupervisorLimits {
    pub startup_deadline: Duration,
    pub process_output_bytes: usize,
}

impl Default for SupervisorLimits {
    fn default() -> Self {
        Self {
            startup_deadline: Duration::from_secs(5),
            process_output_bytes: DEFAULT_PROBE_OUTPUT_LIMIT,
        }
    }
}

impl SupervisorLimits {
    fn validate(self) -> Result<Self, AudioShareError> {
        if self.startup_deadline.is_zero() || self.startup_deadline > Duration::from_secs(30) {
            return Err(AudioShareError::InvalidSupervisorStartupDeadline);
        }
        if self.process_output_bytes == 0 || self.process_output_bytes > DEFAULT_PROBE_OUTPUT_LIMIT
        {
            return Err(AudioShareError::InvalidSupervisorOutputLimit);
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessOutputSummary {
    pub stdout_retained_bytes: usize,
    pub stderr_retained_bytes: usize,
    pub stdout_overflowed: bool,
    pub stderr_overflowed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessExitReport {
    pub exit_code: Option<i32>,
    pub output: ProcessOutputSummary,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SupervisorStartReport {
    pub process_id: u32,
    pub tcp_listener_ready: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SupervisorStopReport {
    pub was_running: bool,
    pub process_id: Option<u32>,
    pub output: Option<ProcessOutputSummary>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SupervisorStatus {
    Stopped,
    Running { process_id: u32 },
    Exited(ProcessExitReport),
}

struct RunningProcess {
    child: Child,
    stdout: JoinHandle<std::io::Result<BoundedRead>>,
    stderr: JoinHandle<std::io::Result<BoundedRead>>,
}

pub struct AudioShareSupervisor {
    config: AudioShareConfig,
    probe_limits: ProbeLimits,
    limits: SupervisorLimits,
    running: Option<RunningProcess>,
    terminal_exit: Option<ProcessExitReport>,
}

impl std::fmt::Debug for AudioShareSupervisor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AudioShareSupervisor")
            .field("config", &self.config)
            .field("probe_limits", &self.probe_limits)
            .field("limits", &self.limits)
            .field(
                "running",
                &self.running.as_ref().map(|process| process.child.id()),
            )
            .field("terminal_exit", &self.terminal_exit)
            .finish()
    }
}

impl AudioShareSupervisor {
    pub fn new(
        config: AudioShareConfig,
        probe_limits: ProbeLimits,
        limits: SupervisorLimits,
    ) -> Result<Self, AudioShareError> {
        Ok(Self {
            config,
            probe_limits: probe_limits.validate()?,
            limits: limits.validate()?,
            running: None,
            terminal_exit: None,
        })
    }

    pub fn config(&self) -> &AudioShareConfig {
        &self.config
    }

    pub fn start(&mut self) -> Result<SupervisorStartReport, AudioShareError> {
        if matches!(self.status()?, SupervisorStatus::Running { .. }) {
            return Err(AudioShareError::SupervisorAlreadyRunning);
        }

        AudioShareProbe::new(self.probe_limits)?.probe_config(&self.config)?;
        self.terminal_exit = None;

        let mut command = Command::new(self.config.executable());
        command
            .args(self.config.server_args())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        configure_hidden_process(&mut command);
        let mut child = command
            .spawn()
            .map_err(|source| AudioShareError::ProcessSpawn {
                operation: "start server".to_owned(),
                source,
            })?;
        let process_id = child.id();
        let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(AudioShareError::MissingProcessPipe);
        };
        let output_limit = self.limits.process_output_bytes;
        self.running = Some(RunningProcess {
            child,
            stdout: thread::spawn(move || read_bounded(stdout, output_limit)),
            stderr: thread::spawn(move || read_bounded(stderr, output_limit)),
        });

        let started = Instant::now();
        loop {
            let status = self
                .running
                .as_mut()
                .expect("running process exists during startup")
                .child
                .try_wait()
                .map_err(|source| AudioShareError::ProcessWait {
                    operation: "start server".to_owned(),
                    source,
                })?;
            if let Some(status) = status {
                let report = self.finalize_running(status)?;
                return Err(AudioShareError::SupervisorExitedBeforeReady {
                    exit_code: report.exit_code,
                });
            }

            if TcpStream::connect_timeout(&self.config.bind_address(), Duration::from_millis(50))
                .is_ok()
            {
                thread::sleep(Duration::from_millis(20));
                if matches!(self.status()?, SupervisorStatus::Running { .. }) {
                    return Ok(SupervisorStartReport {
                        process_id,
                        tcp_listener_ready: true,
                    });
                }
                let exit_code = self.terminal_exit.and_then(|report| report.exit_code);
                return Err(AudioShareError::SupervisorExitedBeforeReady { exit_code });
            }
            if started.elapsed() >= self.limits.startup_deadline {
                let _ = self.stop()?;
                return Err(AudioShareError::SupervisorStartupTimedOut);
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn status(&mut self) -> Result<SupervisorStatus, AudioShareError> {
        let Some(running) = self.running.as_mut() else {
            return Ok(self
                .terminal_exit
                .map_or(SupervisorStatus::Stopped, SupervisorStatus::Exited));
        };
        match running
            .child
            .try_wait()
            .map_err(|source| AudioShareError::ProcessWait {
                operation: "poll server".to_owned(),
                source,
            })? {
            Some(status) => Ok(SupervisorStatus::Exited(self.finalize_running(status)?)),
            None => Ok(SupervisorStatus::Running {
                process_id: running.child.id(),
            }),
        }
    }

    pub fn receiver_tcp_presence(&mut self) -> Result<ReceiverTcpPresence, AudioShareError> {
        let SupervisorStatus::Running { process_id } = self.status()? else {
            return Ok(ReceiverTcpPresence::SupervisorNotRunning);
        };
        receiver_tcp_presence(process_id, self.config.bind_address())
    }

    pub fn stop(&mut self) -> Result<SupervisorStopReport, AudioShareError> {
        let Some(mut running) = self.running.take() else {
            self.terminal_exit = None;
            return Ok(SupervisorStopReport {
                was_running: false,
                process_id: None,
                output: None,
            });
        };
        let process_id = running.child.id();
        let was_running = running
            .child
            .try_wait()
            .map_err(|source| AudioShareError::ProcessWait {
                operation: "stop server".to_owned(),
                source,
            })?
            .is_none();
        if was_running
            && let Err(source) = running.child.kill()
            && running
                .child
                .try_wait()
                .map_err(|wait_source| AudioShareError::ProcessWait {
                    operation: "stop server after kill race".to_owned(),
                    source: wait_source,
                })?
                .is_none()
        {
            return Err(AudioShareError::ProcessWait {
                operation: "kill server".to_owned(),
                source,
            });
        }
        running
            .child
            .wait()
            .map_err(|source| AudioShareError::ProcessWait {
                operation: "reap server".to_owned(),
                source,
            })?;
        let output = collect_output(running)?;
        self.terminal_exit = None;
        Ok(SupervisorStopReport {
            was_running,
            process_id: Some(process_id),
            output: Some(output),
        })
    }

    fn finalize_running(
        &mut self,
        status: ExitStatus,
    ) -> Result<ProcessExitReport, AudioShareError> {
        let running = self
            .running
            .take()
            .expect("finalize requires a running process");
        let report = ProcessExitReport {
            exit_code: status.code(),
            output: collect_output(running)?,
        };
        self.terminal_exit = Some(report);
        Ok(report)
    }
}

impl Drop for AudioShareSupervisor {
    fn drop(&mut self) {
        if let Some(mut running) = self.running.take() {
            let terminated =
                running.child.kill().is_ok() || matches!(running.child.try_wait(), Ok(Some(_)));
            if terminated {
                let _ = running.child.wait();
                let _ = collect_output(running);
            }
        }
    }
}

fn collect_output(running: RunningProcess) -> Result<ProcessOutputSummary, AudioShareError> {
    let stdout = join_reader(running.stdout, "stdout")?;
    let stderr = join_reader(running.stderr, "stderr")?;
    Ok(ProcessOutputSummary {
        stdout_retained_bytes: stdout.bytes.len(),
        stderr_retained_bytes: stderr.bytes.len(),
        stdout_overflowed: stdout.overflowed,
        stderr_overflowed: stderr.overflowed,
    })
}
