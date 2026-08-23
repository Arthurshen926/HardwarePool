#![forbid(unsafe_code)]

//! Synchronous foundation supervisor for one NDJSON Sidecar process.
//! The interface is intentionally sequential, with bounded queues and per-response deadlines.

use std::collections::{BTreeSet, VecDeque};
use std::ffi::OsStr;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, TrySendError, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use capyio_adapter_sdk::{
    ADAPTER_CONTROL_PROTOCOL_MAJOR, ADAPTER_CONTROL_PROTOCOL_MINOR, AdapterCatalog,
    AdapterManifest, ControlProtocolVersion, InitializeParams, InitializeResult, ProbeResult,
    ResponseCorrelator, RouteParams, RouteStatusResult, RpcError, RpcRequest, SmokeSample,
    decode_response_line, encode_request_line,
};
use capyio_core::{AdapterInstanceId, NodeId, RouteId};
use capyio_runtime::{NodeRuntime, RuntimeError};
use serde::{Serialize, de::DeserializeOwned};
use thiserror::Error;

const MAX_RETAINED_STDERR_LINES: usize = 128;
const MAX_STDERR_LINE_CHARS: usize = 2_048;
const MAX_BUFFERED_STDOUT_LINES: usize = 8;
const RESPONSE_DEADLINE: Duration = Duration::from_secs(5);

pub struct SidecarHost {
    manifest: AdapterManifest,
    child: Child,
    stdin: Option<ChildStdin>,
    stdout_lines: Receiver<Result<Vec<u8>, String>>,
    stdout_thread: Option<JoinHandle<()>>,
    stderr_lines: Arc<Mutex<VecDeque<String>>>,
    stderr_thread: Option<JoinHandle<()>>,
    correlator: ResponseCorrelator,
    next_request_id: u64,
    prepared_routes: BTreeSet<RouteId>,
    stopped: bool,
}

impl SidecarHost {
    pub fn spawn<I, S>(
        executable: &Path,
        args: I,
        manifest: AdapterManifest,
    ) -> Result<Self, HostError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        manifest.validate()?;
        let mut child = Command::new(executable)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdin = child.stdin.take().ok_or(HostError::MissingPipe("stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or(HostError::MissingPipe("stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or(HostError::MissingPipe("stderr"))?;
        let stderr_lines = Arc::new(Mutex::new(VecDeque::new()));
        let retained = Arc::clone(&stderr_lines);
        let stderr_thread = thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                let Ok(line) = line else { break };
                let bounded = line.chars().take(MAX_STDERR_LINE_CHARS).collect();
                let Ok(mut lines) = retained.lock() else {
                    break;
                };
                lines.push_back(bounded);
                while lines.len() > MAX_RETAINED_STDERR_LINES {
                    lines.pop_front();
                }
            }
        });
        let (stdout_sender, stdout_lines) = sync_channel(MAX_BUFFERED_STDOUT_LINES);
        let stdout_thread = thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = Vec::new();
                let item = match reader.read_until(b'\n', &mut line) {
                    Ok(0) => break,
                    Ok(_) => Ok(line),
                    Err(error) => Err(error.to_string()),
                };
                let terminal = item.is_err();
                match stdout_sender.try_send(item) {
                    Ok(()) => {}
                    Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => break,
                }
                if terminal {
                    break;
                }
            }
        });

        Ok(Self {
            manifest,
            child,
            stdin: Some(stdin),
            stdout_lines,
            stdout_thread: Some(stdout_thread),
            stderr_lines,
            stderr_thread: Some(stderr_thread),
            correlator: ResponseCorrelator::default(),
            next_request_id: 1,
            prepared_routes: BTreeSet::new(),
            stopped: false,
        })
    }

    #[must_use]
    pub fn manifest(&self) -> &AdapterManifest {
        &self.manifest
    }

    pub fn initialize(
        &mut self,
        adapter_instance_id: AdapterInstanceId,
    ) -> Result<InitializeResult, HostError> {
        self.request(
            "adapter.initialize",
            &InitializeParams {
                adapter_instance_id: adapter_instance_id.to_string(),
                control_protocol: ControlProtocolVersion {
                    major: ADAPTER_CONTROL_PROTOCOL_MAJOR,
                    minor: ADAPTER_CONTROL_PROTOCOL_MINOR,
                },
            },
        )
    }

    pub fn probe(&mut self) -> Result<ProbeResult, HostError> {
        self.request("adapter.probe", &())
    }

    pub fn catalog(&mut self) -> Result<AdapterCatalog, HostError> {
        self.request("adapter.catalog", &())
    }

    pub fn health(&mut self) -> Result<ProbeResult, HostError> {
        self.request("adapter.health", &())
    }

    pub fn prepare_route(&mut self, route_id: RouteId) -> Result<(), HostError> {
        let accepted: bool = self.request("route.prepare", &RouteParams { route_id })?;
        if !accepted {
            return Err(HostError::Rejected("route.prepare"));
        }
        self.prepared_routes.insert(route_id);
        Ok(())
    }

    pub fn start_route(&mut self, route_id: RouteId) -> Result<SmokeSample, HostError> {
        if !self.prepared_routes.contains(&route_id) {
            return Err(HostError::RouteNotPrepared(route_id));
        }
        self.request("route.start", &RouteParams { route_id })
    }

    pub fn stop_route(&mut self, route_id: RouteId) -> Result<(), HostError> {
        let accepted: bool = self.request("route.stop", &RouteParams { route_id })?;
        if !accepted {
            return Err(HostError::Rejected("route.stop"));
        }
        self.prepared_routes.remove(&route_id);
        Ok(())
    }

    pub fn route_status(&mut self, route_id: RouteId) -> Result<RouteStatusResult, HostError> {
        self.request("route.status", &RouteParams { route_id })
    }

    pub fn crash_for_smoke_test(&mut self) -> Result<(), HostError> {
        let _: bool = self.request("test.crash", &())?;
        Ok(())
    }

    pub fn shutdown(&mut self) -> Result<(), HostError> {
        if self.stopped {
            return Ok(());
        }
        let accepted: bool = self.request("adapter.shutdown", &())?;
        if !accepted {
            return Err(HostError::Rejected("adapter.shutdown"));
        }
        self.stdin.take();
        let status = self.child.wait()?;
        self.stopped = true;
        self.join_io_threads();
        if status.success() {
            Ok(())
        } else {
            Err(HostError::UnexpectedExit(status))
        }
    }

    #[must_use]
    pub fn stderr_lines(&self) -> Vec<String> {
        self.stderr_lines
            .lock()
            .map_or_else(|_| Vec::new(), |lines| lines.iter().cloned().collect())
    }

    fn request<P: Serialize, R: DeserializeOwned>(
        &mut self,
        method: &'static str,
        params: &P,
    ) -> Result<R, HostError> {
        if self.stopped {
            return Err(HostError::AlreadyStopped);
        }
        let id = self.next_request_id;
        self.next_request_id = self
            .next_request_id
            .checked_add(1)
            .ok_or(HostError::RequestIdExhausted)?;
        self.correlator.register(id)?;
        let request = RpcRequest::new(id, method, params)?;
        let bytes = encode_request_line(&request)?;
        let write_result = self
            .stdin
            .as_mut()
            .ok_or(HostError::MissingPipe("stdin"))?
            .write_all(&bytes)
            .and_then(|()| self.stdin.as_mut().expect("stdin checked").flush());
        if let Err(error) = write_result {
            self.correlator.abandon(id);
            return Err(error.into());
        }

        let line = match self.stdout_lines.recv_timeout(RESPONSE_DEADLINE) {
            Ok(Ok(line)) => line,
            Ok(Err(error)) => {
                self.correlator.abandon(id);
                return Err(HostError::StdoutRead(error));
            }
            Err(RecvTimeoutError::Timeout) => {
                self.correlator.abandon(id);
                return Err(HostError::ResponseDeadline {
                    method,
                    timeout: RESPONSE_DEADLINE,
                });
            }
            Err(RecvTimeoutError::Disconnected) => {
                self.correlator.abandon(id);
                if let Some(status) = wait_for_exit(&mut self.child, RESPONSE_DEADLINE)? {
                    self.stopped = true;
                    self.stdin.take();
                    self.join_io_threads();
                    return Err(HostError::UnexpectedExit(status));
                }
                return Err(HostError::ControlChannelClosed);
            }
        };
        let response = match decode_response_line(&line) {
            Ok(response) => response,
            Err(error) => {
                self.correlator.abandon(id);
                return Err(error.into());
            }
        };
        self.correlator.resolve(&response)?;
        Ok(response.decode_result()?)
    }

    fn join_io_threads(&mut self) {
        if let Some(thread) = self.stdout_thread.take() {
            let _ = thread.join();
        }
        if let Some(thread) = self.stderr_thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for SidecarHost {
    fn drop(&mut self) {
        if !self.stopped {
            self.stdin.take();
            if self.child.try_wait().ok().flatten().is_none() {
                let _ = self.child.kill();
            }
            let _ = self.child.wait();
            self.stopped = true;
        }
        self.join_io_threads();
    }
}

pub fn apply_unexpected_exit(
    runtime: &mut NodeRuntime,
    node_id: NodeId,
    adapter_id: AdapterInstanceId,
    status: &ExitStatus,
) -> Result<(), RuntimeError> {
    let code = status.code().map_or_else(
        || "adapter_process_terminated".to_owned(),
        |code| format!("adapter_process_exit_{code}"),
    );
    runtime.fail_adapter(node_id, adapter_id, code)
}

#[derive(Debug, Error)]
pub enum HostError {
    #[error("Sidecar I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Manifest(#[from] capyio_adapter_sdk::ManifestError),
    #[error(transparent)]
    Rpc(#[from] RpcError),
    #[error("Sidecar did not expose its {0} pipe")]
    MissingPipe(&'static str),
    #[error("Sidecar process exited unexpectedly: {0}")]
    UnexpectedExit(ExitStatus),
    #[error("Sidecar stdout reader failed: {0}")]
    StdoutRead(String),
    #[error("Sidecar control response to {method} exceeded {timeout:?}")]
    ResponseDeadline {
        method: &'static str,
        timeout: Duration,
    },
    #[error("Sidecar stdout control channel closed before process exit")]
    ControlChannelClosed,
    #[error("Sidecar is already stopped")]
    AlreadyStopped,
    #[error("Sidecar request ID space is exhausted")]
    RequestIdExhausted,
    #[error("Route {0} was not prepared by this Host")]
    RouteNotPrepared(RouteId),
    #[error("Sidecar rejected {0}")]
    Rejected(&'static str),
}

fn wait_for_exit(
    child: &mut Child,
    timeout: Duration,
) -> Result<Option<ExitStatus>, std::io::Error> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if Instant::now() >= deadline {
            return Ok(None);
        }
        thread::sleep(Duration::from_millis(10));
    }
}
