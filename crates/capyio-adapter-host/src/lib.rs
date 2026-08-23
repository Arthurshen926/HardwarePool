#![forbid(unsafe_code)]

//! Synchronous supervisor for one sequential NDJSON Sidecar process.
//! Control lines, diagnostic retention and response time are bounded. Any
//! protocol desynchronization poisons and reaps the process before reuse.

use std::collections::{BTreeMap, VecDeque};
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
    AdapterManifest, ControlProtocolVersion, InitializeParams, InitializeResult,
    MAX_NDJSON_LINE_BYTES, ProbeResult, ResponseCorrelator, RoutePrepareRequest,
    RoutePrepareResult, RouteStartRequest, RouteStartResult, RouteStatusRequest, RouteStatusResult,
    RouteStopRequest, RouteStopResult, RpcError, RpcRequest, decode_response_line,
    encode_request_line,
};
use capyio_core::{AdapterInstanceId, NodeId, RouteId};
use capyio_runtime::{NodeRuntime, RuntimeError};
use serde::{Serialize, de::DeserializeOwned};
use thiserror::Error;

pub const MAX_RETAINED_STDERR_LINES: usize = 128;
pub const MAX_STDERR_LINE_BYTES: usize = 2_048;
const MAX_BUFFERED_STDOUT_LINES: usize = 8;
const DEFAULT_RESPONSE_DEADLINE: Duration = Duration::from_secs(5);
const STDERR_TRUNCATION_MARKER: &str = " [truncated]";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SidecarHostState {
    Running,
    Poisoned,
    Stopped,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SidecarHostOptions {
    pub response_deadline: Duration,
}

impl Default for SidecarHostOptions {
    fn default() -> Self {
        Self {
            response_deadline: DEFAULT_RESPONSE_DEADLINE,
        }
    }
}

pub struct SidecarHost {
    manifest: AdapterManifest,
    child: Child,
    stdin: Option<ChildStdin>,
    stdout_lines: Receiver<Result<Vec<u8>, StdoutFailure>>,
    stdout_thread: Option<JoinHandle<()>>,
    stderr_lines: Arc<Mutex<VecDeque<String>>>,
    stderr_thread: Option<JoinHandle<()>>,
    correlator: ResponseCorrelator,
    next_request_id: u64,
    prepared_routes: BTreeMap<RouteId, u64>,
    state: SidecarHostState,
    response_deadline: Duration,
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
        Self::spawn_with_options(executable, args, manifest, SidecarHostOptions::default())
    }

    pub fn spawn_with_options<I, S>(
        executable: &Path,
        args: I,
        manifest: AdapterManifest,
        options: SidecarHostOptions,
    ) -> Result<Self, HostError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        manifest.validate()?;
        if options.response_deadline.is_zero() {
            return Err(HostError::InvalidResponseDeadline);
        }
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
            let mut reader = BufReader::new(stderr);
            while let Ok(BoundedRead::Line {
                mut bytes,
                truncated,
            }) = read_bounded_line(
                &mut reader,
                MAX_STDERR_LINE_BYTES.saturating_sub(STDERR_TRUNCATION_MARKER.len()),
                OverflowPolicy::TruncateAndDrain,
            ) {
                trim_line_ending(&mut bytes);
                let mut line = String::from_utf8_lossy(&bytes).into_owned();
                let encoding_truncated = truncate_utf8_bytes(
                    &mut line,
                    MAX_STDERR_LINE_BYTES.saturating_sub(STDERR_TRUNCATION_MARKER.len()),
                );
                if truncated || encoding_truncated {
                    line.push_str(STDERR_TRUNCATION_MARKER);
                }
                let Ok(mut lines) = retained.lock() else {
                    break;
                };
                lines.push_back(line);
                while lines.len() > MAX_RETAINED_STDERR_LINES {
                    lines.pop_front();
                }
            }
        });

        let (stdout_sender, stdout_lines) = sync_channel(MAX_BUFFERED_STDOUT_LINES);
        let stdout_thread = thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let item = match read_bounded_line(
                    &mut reader,
                    MAX_NDJSON_LINE_BYTES,
                    OverflowPolicy::RejectImmediately,
                ) {
                    Ok(BoundedRead::Eof) => break,
                    Ok(BoundedRead::Line { bytes, .. }) => Ok(bytes),
                    Ok(BoundedRead::TooLarge { actual_at_least }) => Err(StdoutFailure::TooLarge {
                        actual_at_least,
                        limit: MAX_NDJSON_LINE_BYTES,
                    }),
                    Err(error) => Err(StdoutFailure::Io(error.to_string())),
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
            prepared_routes: BTreeMap::new(),
            state: SidecarHostState::Running,
            response_deadline: options.response_deadline,
        })
    }

    #[must_use]
    pub fn manifest(&self) -> &AdapterManifest {
        &self.manifest
    }

    #[must_use]
    pub const fn state(&self) -> SidecarHostState {
        self.state
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

    pub fn prepare_route(
        &mut self,
        request: RoutePrepareRequest,
    ) -> Result<RoutePrepareResult, HostError> {
        request.validate()?;
        let route_id = request.route_id;
        let epoch = request.epoch;
        let result: RoutePrepareResult = self.request("route.prepare", &request)?;
        if let Err(error) = result.validate() {
            self.poison_and_reap();
            return Err(error.into());
        }
        if !result.accepted {
            return Err(HostError::Rejected("route.prepare"));
        }
        self.prepared_routes.insert(route_id, epoch);
        Ok(result)
    }

    pub fn start_route(
        &mut self,
        request: RouteStartRequest,
    ) -> Result<RouteStartResult, HostError> {
        request.validate()?;
        self.require_prepared_epoch(request.route_id, request.epoch)?;
        let result: RouteStartResult = self.request("route.start", &request)?;
        if let Err(error) = result.validate() {
            self.poison_and_reap();
            return Err(error.into());
        }
        if !result.accepted {
            return Err(HostError::Rejected("route.start"));
        }
        Ok(result)
    }

    pub fn stop_route(&mut self, request: RouteStopRequest) -> Result<(), HostError> {
        request.validate()?;
        self.require_prepared_epoch(request.route_id, request.epoch)?;
        let result: RouteStopResult = self.request("route.stop", &request)?;
        if !result.accepted {
            return Err(HostError::Rejected("route.stop"));
        }
        self.prepared_routes.remove(&request.route_id);
        Ok(())
    }

    pub fn route_status(
        &mut self,
        request: RouteStatusRequest,
    ) -> Result<RouteStatusResult, HostError> {
        let result: RouteStatusResult = self.request("route.status", &request)?;
        if let Err(error) = result.validate() {
            self.poison_and_reap();
            return Err(error.into());
        }
        Ok(result)
    }

    pub fn crash_for_smoke_test(&mut self) -> Result<(), HostError> {
        let _: bool = self.request("test.crash", &())?;
        Ok(())
    }

    pub fn shutdown(&mut self) -> Result<(), HostError> {
        match self.state {
            SidecarHostState::Stopped => return Ok(()),
            SidecarHostState::Poisoned => return Err(HostError::SidecarPoisoned),
            SidecarHostState::Running => {}
        }
        let accepted: bool = self.request("adapter.shutdown", &())?;
        if !accepted {
            return Err(HostError::Rejected("adapter.shutdown"));
        }
        self.stdin.take();
        let status = self.child.wait()?;
        self.state = SidecarHostState::Stopped;
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

    fn require_prepared_epoch(&self, route_id: RouteId, epoch: u64) -> Result<(), HostError> {
        match self.prepared_routes.get(&route_id) {
            Some(prepared_epoch) if *prepared_epoch == epoch => Ok(()),
            Some(prepared_epoch) => Err(HostError::RouteEpochMismatch {
                route_id,
                prepared: *prepared_epoch,
                requested: epoch,
            }),
            None => Err(HostError::RouteNotPrepared(route_id)),
        }
    }

    fn request<P: Serialize, R: DeserializeOwned>(
        &mut self,
        method: &'static str,
        params: &P,
    ) -> Result<R, HostError> {
        match self.state {
            SidecarHostState::Running => {}
            SidecarHostState::Poisoned => return Err(HostError::SidecarPoisoned),
            SidecarHostState::Stopped => return Err(HostError::AlreadyStopped),
        }
        let id = self.next_request_id;
        self.next_request_id = self
            .next_request_id
            .checked_add(1)
            .ok_or(HostError::RequestIdExhausted)?;
        self.correlator.register(id)?;
        let request = match RpcRequest::new(id, method, params) {
            Ok(request) => request,
            Err(error) => {
                self.correlator.abandon(id);
                return Err(error.into());
            }
        };
        let bytes = match encode_request_line(&request) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.correlator.abandon(id);
                return Err(error.into());
            }
        };
        let write_result = match self.stdin.as_mut() {
            Some(stdin) => stdin.write_all(&bytes).and_then(|()| stdin.flush()),
            None => Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "Sidecar stdin is unavailable",
            )),
        };
        if let Err(error) = write_result {
            self.correlator.abandon(id);
            self.poison_and_reap();
            return Err(error.into());
        }

        let line = match self.stdout_lines.recv_timeout(self.response_deadline) {
            Ok(Ok(line)) => line,
            Ok(Err(StdoutFailure::Io(error))) => {
                self.correlator.abandon(id);
                self.poison_and_reap();
                return Err(HostError::StdoutRead(error));
            }
            Ok(Err(StdoutFailure::TooLarge {
                actual_at_least,
                limit,
            })) => {
                self.correlator.abandon(id);
                self.poison_and_reap();
                return Err(RpcError::LineTooLarge {
                    actual: actual_at_least,
                    limit,
                }
                .into());
            }
            Err(RecvTimeoutError::Timeout) => {
                self.correlator.abandon(id);
                self.poison_and_reap();
                return Err(HostError::ResponseDeadline {
                    method,
                    timeout: self.response_deadline,
                });
            }
            Err(RecvTimeoutError::Disconnected) => {
                self.correlator.abandon(id);
                let exit_wait = self.response_deadline.min(Duration::from_millis(250));
                let status = match wait_for_exit(&mut self.child, exit_wait) {
                    Ok(status) => status,
                    Err(error) => {
                        self.poison_and_reap();
                        return Err(error.into());
                    }
                };
                self.poison_and_reap();
                return match status {
                    Some(status) => Err(HostError::UnexpectedExit(status)),
                    None => Err(HostError::ControlChannelClosed),
                };
            }
        };
        let response = match decode_response_line(&line) {
            Ok(response) => response,
            Err(error) => {
                self.correlator.abandon(id);
                self.poison_and_reap();
                return Err(error.into());
            }
        };
        if let Err(error) = self.correlator.resolve(&response) {
            self.poison_and_reap();
            return Err(error.into());
        }
        match response.decode_result() {
            Ok(result) => Ok(result),
            Err(error @ RpcError::Remote { .. }) => Err(error.into()),
            Err(error) => {
                self.poison_and_reap();
                Err(error.into())
            }
        }
    }

    fn poison_and_reap(&mut self) {
        if self.state != SidecarHostState::Running {
            return;
        }
        self.state = SidecarHostState::Poisoned;
        self.stdin.take();
        match self.child.try_wait() {
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => {
                let _ = self.child.kill();
            }
        }
        let _ = self.child.wait();
        self.join_io_threads();
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
        if self.state == SidecarHostState::Running {
            self.stdin.take();
            if self.child.try_wait().ok().flatten().is_none() {
                let _ = self.child.kill();
            }
            let _ = self.child.wait();
            self.state = SidecarHostState::Stopped;
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
    #[error("Sidecar is poisoned after a terminal control-protocol failure")]
    SidecarPoisoned,
    #[error("Sidecar is already stopped")]
    AlreadyStopped,
    #[error("Sidecar response deadline must be greater than zero")]
    InvalidResponseDeadline,
    #[error("Sidecar request ID space is exhausted")]
    RequestIdExhausted,
    #[error("Route {0} was not prepared by this Host")]
    RouteNotPrepared(RouteId),
    #[error("Route {route_id} epoch mismatch: prepared {prepared}, requested {requested}")]
    RouteEpochMismatch {
        route_id: RouteId,
        prepared: u64,
        requested: u64,
    },
    #[error("Sidecar rejected {0}")]
    Rejected(&'static str),
}

#[derive(Debug)]
enum StdoutFailure {
    Io(String),
    TooLarge {
        actual_at_least: usize,
        limit: usize,
    },
}

#[derive(Clone, Copy)]
enum OverflowPolicy {
    RejectImmediately,
    TruncateAndDrain,
}

enum BoundedRead {
    Eof,
    Line { bytes: Vec<u8>, truncated: bool },
    TooLarge { actual_at_least: usize },
}

fn read_bounded_line<R: BufRead>(
    reader: &mut R,
    limit: usize,
    overflow_policy: OverflowPolicy,
) -> Result<BoundedRead, std::io::Error> {
    let mut bytes = Vec::with_capacity(limit.min(8 * 1024));
    let mut truncated = false;
    loop {
        let buffer = reader.fill_buf()?;
        if buffer.is_empty() {
            return if bytes.is_empty() && !truncated {
                Ok(BoundedRead::Eof)
            } else {
                Ok(BoundedRead::Line { bytes, truncated })
            };
        }
        let newline = buffer.iter().position(|byte| *byte == b'\n');
        let available = newline.map_or(buffer.len(), |index| index + 1);

        if truncated {
            reader.consume(available);
            if newline.is_some() {
                return Ok(BoundedRead::Line { bytes, truncated });
            }
            continue;
        }

        let remaining = limit.saturating_sub(bytes.len());
        if available <= remaining {
            bytes.extend_from_slice(&buffer[..available]);
            reader.consume(available);
            if newline.is_some() {
                return Ok(BoundedRead::Line { bytes, truncated });
            }
            continue;
        }

        if remaining > 0 {
            bytes.extend_from_slice(&buffer[..remaining]);
            reader.consume(remaining);
        }
        match overflow_policy {
            OverflowPolicy::RejectImmediately => {
                return Ok(BoundedRead::TooLarge {
                    actual_at_least: limit.saturating_add(1),
                });
            }
            OverflowPolicy::TruncateAndDrain => truncated = true,
        }
    }
}

fn trim_line_ending(bytes: &mut Vec<u8>) {
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
    }
    if bytes.last() == Some(&b'\r') {
        bytes.pop();
    }
}

fn truncate_utf8_bytes(value: &mut String, limit: usize) -> bool {
    if value.len() <= limit {
        return false;
    }
    let mut boundary = limit;
    while boundary > 0 && !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    value.truncate(boundary);
    true
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

#[cfg(test)]
mod tests {
    use std::io::{BufReader, Cursor};

    use super::*;

    #[test]
    fn newline_free_stdout_is_rejected_at_the_limit() {
        let input = vec![b'x'; 65];
        let mut reader = BufReader::new(Cursor::new(input));
        assert!(matches!(
            read_bounded_line(&mut reader, 64, OverflowPolicy::RejectImmediately)
                .expect("bounded read"),
            BoundedRead::TooLarge {
                actual_at_least: 65
            }
        ));
    }

    #[test]
    fn newline_free_stderr_is_truncated_without_growing_the_prefix() {
        let input = vec![b'x'; 4_096];
        let mut reader = BufReader::with_capacity(31, Cursor::new(input));
        match read_bounded_line(&mut reader, 64, OverflowPolicy::TruncateAndDrain)
            .expect("bounded read")
        {
            BoundedRead::Line { bytes, truncated } => {
                assert!(truncated);
                assert_eq!(bytes.len(), 64);
            }
            BoundedRead::Eof | BoundedRead::TooLarge { .. } => panic!("expected truncated line"),
        }
    }
}
