use std::{
    ffi::c_void,
    fs::OpenOptions,
    io::Write,
    mem,
    os::windows::io::AsRawHandle,
    ptr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, ERROR_ACCESS_DENIED, ERROR_BROKEN_PIPE, ERROR_FILE_NOT_FOUND, ERROR_NO_DATA,
        ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, ERROR_PIPE_LISTENING, GetLastError, HANDLE,
        INVALID_HANDLE_VALUE, LocalFree,
    },
    Security::{
        Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1},
        PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES,
    },
    Storage::FileSystem::{FlushFileBuffers, PIPE_ACCESS_DUPLEX, ReadFile, WriteFile},
    System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_NOWAIT, PIPE_READMODE_BYTE,
        PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
    },
};

use crate::{AudioShareSupervisor, BrokerProcess, BrokerServiceRuntime, BrokerServiceSnapshot};

pub const CONTROL_PIPE_NAME: &str = r"\\.\pipe\CapyIO.Broker.Control.v1";
const CONTROL_SCHEMA_VERSION: u8 = 1;
const MAX_CONTROL_BYTES: usize = 4 * 1024;
const IO_DEADLINE: Duration = Duration::from_secs(2);
const CLIENT_RESPONSE_DEADLINE: Duration = Duration::from_secs(10);
const RETRY_INTERVAL: Duration = Duration::from_millis(10);
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
        let request = ControlRequest {
            schema_version: CONTROL_SCHEMA_VERSION,
            request_id,
            operation,
        };
        let payload = serde_json::to_vec(&request).map_err(|_| "encode service request")?;
        let mut pipe = open_client_pipe()?;
        write_frame(&mut pipe, &payload)?;
        let response_payload = read_frame(&mut pipe)?;
        let response: ControlResponse = serde_json::from_slice(&response_payload)
            .map_err(|_| "invalid CapyIO service response".to_owned())?;
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

fn open_client_pipe() -> Result<std::fs::File, String> {
    let deadline = Instant::now() + IO_DEADLINE;
    loop {
        match OpenOptions::new()
            .read(true)
            .write(true)
            .open(CONTROL_PIPE_NAME)
        {
            Ok(pipe) => return Ok(pipe),
            Err(error) if Instant::now() < deadline => {
                let code = error.raw_os_error().unwrap_or_default() as u32;
                if matches!(
                    code,
                    ERROR_PIPE_BUSY | ERROR_ACCESS_DENIED | ERROR_FILE_NOT_FOUND
                ) {
                    thread::sleep(RETRY_INTERVAL);
                    continue;
                }
                return Err("CapyIO Broker service is unavailable".to_owned());
            }
            Err(_) => return Err("CapyIO Broker service is unavailable".to_owned()),
        }
    }
}

fn write_frame(stream: &mut impl Write, payload: &[u8]) -> Result<(), String> {
    if payload.is_empty() || payload.len() > MAX_CONTROL_BYTES {
        return Err("CapyIO service control message is outside bounds".to_owned());
    }
    let length = u32::try_from(payload.len()).map_err(|_| "control message length overflow")?;
    stream
        .write_all(&length.to_le_bytes())
        .and_then(|()| stream.write_all(payload))
        .map_err(|_| "write CapyIO service control message".to_owned())
}

fn read_frame(stream: &mut std::fs::File) -> Result<Vec<u8>, String> {
    let mut length = [0_u8; 4];
    read_client_exact(stream, &mut length)?;
    let length = usize::try_from(u32::from_le_bytes(length))
        .map_err(|_| "CapyIO service response length overflow".to_owned())?;
    if length == 0 || length > MAX_CONTROL_BYTES {
        return Err("CapyIO service response is outside bounds".to_owned());
    }
    let mut payload = vec![0_u8; length];
    read_client_exact(stream, &mut payload)?;
    Ok(payload)
}

fn read_client_exact(stream: &std::fs::File, buffer: &mut [u8]) -> Result<(), String> {
    let deadline = Instant::now() + CLIENT_RESPONSE_DEADLINE;
    let handle = stream.as_raw_handle() as HANDLE;
    let mut offset = 0;
    while offset < buffer.len() && Instant::now() < deadline {
        let mut read = 0_u32;
        // SAFETY: the File owns a connected pipe handle and buffer is valid for
        // the fixed bounded read. The client deliberately polls a non-blocking
        // server pipe while Broker startup completes.
        let result = unsafe {
            ReadFile(
                handle,
                buffer[offset..].as_mut_ptr(),
                u32::try_from(buffer.len() - offset).expect("control buffer fits u32"),
                &mut read,
                ptr::null_mut(),
            )
        };
        if result != 0 && read > 0 {
            offset += read as usize;
            continue;
        }
        // SAFETY: GetLastError immediately follows ReadFile.
        let error = unsafe { GetLastError() };
        if result == 0 && error == ERROR_BROKEN_PIPE {
            break;
        }
        if result != 0 || error == ERROR_NO_DATA {
            thread::sleep(RETRY_INTERVAL);
            continue;
        }
        return Err("read CapyIO service response".to_owned());
    }
    (offset == buffer.len())
        .then_some(())
        .ok_or_else(|| "CapyIO service response timed out".to_owned())
}

pub fn control_server_loop(
    runtime: Arc<Mutex<BrokerServiceRuntime<AudioShareSupervisor>>>,
    stop: Arc<AtomicBool>,
) -> Result<(), String> {
    let pipe = NamedPipe::create()?;
    while !stop.load(Ordering::Acquire) {
        if !pipe.wait_for_client(&stop)? {
            break;
        }
        if stop.load(Ordering::Acquire) {
            break;
        }
        let request = match pipe.read_request() {
            Ok(request) => request,
            Err(_) => {
                pipe.disconnect();
                continue;
            }
        };
        let response = dispatch_request(&runtime, request);
        let _ = pipe.write_response(&response);
        pipe.disconnect();
    }
    Ok(())
}

pub fn wake_control_server() {
    let _ = OpenOptions::new()
        .read(true)
        .write(true)
        .open(CONTROL_PIPE_NAME);
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

struct NamedPipe {
    handle: HANDLE,
}

impl NamedPipe {
    fn create() -> Result<Self, String> {
        let mut descriptor = SecurityDescriptor::new()?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: u32::try_from(mem::size_of::<SECURITY_ATTRIBUTES>())
                .expect("SECURITY_ATTRIBUTES size fits u32"),
            lpSecurityDescriptor: descriptor.as_mut_ptr(),
            bInheritHandle: 0,
        };
        let name = wide(CONTROL_PIPE_NAME);
        let deadline = Instant::now() + IO_DEADLINE;
        loop {
            // SAFETY: name and SECURITY_ATTRIBUTES remain alive for the call;
            // all sizes and modes are fixed, and a successful handle is owned
            // below. The same instance is disconnected and reused for each
            // sequential client.
            let handle = unsafe {
                CreateNamedPipeW(
                    name.as_ptr(),
                    PIPE_ACCESS_DUPLEX,
                    PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_NOWAIT | PIPE_REJECT_REMOTE_CLIENTS,
                    1,
                    MAX_CONTROL_BYTES as u32 + 4,
                    MAX_CONTROL_BYTES as u32 + 4,
                    0,
                    &attributes,
                )
            };
            if handle != INVALID_HANDLE_VALUE {
                return Ok(Self { handle });
            }
            // SAFETY: GetLastError immediately follows CreateNamedPipeW.
            if unsafe { GetLastError() } == ERROR_PIPE_BUSY && Instant::now() < deadline {
                thread::sleep(RETRY_INTERVAL);
                continue;
            }
            return Err("create CapyIO service control pipe".to_owned());
        }
    }

    fn wait_for_client(&self, stop: &AtomicBool) -> Result<bool, String> {
        while !stop.load(Ordering::Acquire) {
            // SAFETY: handle is a live named-pipe handle and no OVERLAPPED
            // structure is used in non-blocking mode.
            if unsafe { ConnectNamedPipe(self.handle, ptr::null_mut()) } != 0 {
                return Ok(true);
            }
            // SAFETY: GetLastError immediately follows the failed Win32 call.
            match unsafe { GetLastError() } {
                ERROR_PIPE_CONNECTED => return Ok(true),
                ERROR_PIPE_LISTENING => thread::sleep(RETRY_INTERVAL),
                ERROR_NO_DATA => {
                    self.disconnect();
                    thread::sleep(RETRY_INTERVAL);
                }
                _ => return Err("connect CapyIO service control pipe".to_owned()),
            }
        }
        Ok(false)
    }

    fn disconnect(&self) {
        // SAFETY: handle is a live pipe instance. ERROR_PIPE_NOT_CONNECTED is
        // harmless when a client raced with a bounded read failure.
        unsafe {
            DisconnectNamedPipe(self.handle);
        }
    }

    fn read_request(&self) -> Result<ControlRequest, String> {
        let mut length = [0_u8; 4];
        self.read_exact(&mut length)?;
        let length = usize::try_from(u32::from_le_bytes(length))
            .map_err(|_| "control request length overflow".to_owned())?;
        if length == 0 || length > MAX_CONTROL_BYTES {
            return Err("control request outside bounds".to_owned());
        }
        let mut payload = vec![0_u8; length];
        self.read_exact(&mut payload)?;
        serde_json::from_slice(&payload).map_err(|_| "invalid control request".to_owned())
    }

    fn write_response(&self, response: &ControlResponse) -> Result<(), String> {
        let payload = serde_json::to_vec(response).map_err(|_| "encode control response")?;
        if payload.is_empty() || payload.len() > MAX_CONTROL_BYTES {
            return Err("control response outside bounds".to_owned());
        }
        let length = u32::try_from(payload.len()).map_err(|_| "control response overflow")?;
        self.write_all(&length.to_le_bytes())?;
        self.write_all(&payload)?;
        // SAFETY: handle is a connected pipe. The local client is already
        // waiting for this bounded response, so flushing prevents
        // DisconnectNamedPipe from discarding unread response bytes.
        if unsafe { FlushFileBuffers(self.handle) } == 0 {
            return Err("flush CapyIO service control response".to_owned());
        }
        Ok(())
    }

    fn read_exact(&self, buffer: &mut [u8]) -> Result<(), String> {
        let deadline = Instant::now() + IO_DEADLINE;
        let mut offset = 0;
        while offset < buffer.len() && Instant::now() < deadline {
            let mut read = 0_u32;
            // SAFETY: buffer slice is valid for the bounded requested write,
            // the handle is live, and synchronous non-blocking I/O uses no
            // OVERLAPPED pointer.
            let result = unsafe {
                ReadFile(
                    self.handle,
                    buffer[offset..].as_mut_ptr(),
                    u32::try_from(buffer.len() - offset).expect("control buffer fits u32"),
                    &mut read,
                    ptr::null_mut(),
                )
            };
            if result != 0 && read > 0 {
                offset += read as usize;
                continue;
            }
            // SAFETY: GetLastError immediately follows the failed Win32 call.
            let error = unsafe { GetLastError() };
            if result == 0 && error == ERROR_BROKEN_PIPE {
                break;
            }
            if result != 0 || matches!(error, ERROR_NO_DATA) {
                thread::sleep(RETRY_INTERVAL);
                continue;
            }
            return Err("read CapyIO service control pipe".to_owned());
        }
        (offset == buffer.len())
            .then_some(())
            .ok_or_else(|| "CapyIO service control read timed out".to_owned())
    }

    fn write_all(&self, buffer: &[u8]) -> Result<(), String> {
        let deadline = Instant::now() + IO_DEADLINE;
        let mut offset = 0;
        while offset < buffer.len() && Instant::now() < deadline {
            let mut written = 0_u32;
            // SAFETY: buffer slice is valid for the bounded requested read and
            // the handle is a live synchronous non-blocking pipe handle.
            let result = unsafe {
                WriteFile(
                    self.handle,
                    buffer[offset..].as_ptr(),
                    u32::try_from(buffer.len() - offset).expect("control buffer fits u32"),
                    &mut written,
                    ptr::null_mut(),
                )
            };
            if result != 0 && written > 0 {
                offset += written as usize;
                continue;
            }
            // SAFETY: GetLastError immediately follows the failed Win32 call.
            let error = unsafe { GetLastError() };
            if result != 0 || error == ERROR_NO_DATA {
                thread::sleep(RETRY_INTERVAL);
                continue;
            }
            return Err("write CapyIO service control pipe".to_owned());
        }
        (offset == buffer.len())
            .then_some(())
            .ok_or_else(|| "CapyIO service control write timed out".to_owned())
    }
}

impl Drop for NamedPipe {
    fn drop(&mut self) {
        // SAFETY: this object exclusively owns the live pipe handle.
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

struct SecurityDescriptor(PSECURITY_DESCRIPTOR);

impl SecurityDescriptor {
    fn new() -> Result<Self, String> {
        let sddl = wide(PIPE_SDDL);
        let mut descriptor = ptr::null_mut();
        // SAFETY: SDDL is a terminated immutable UTF-16 buffer and the output
        // pointer is released with LocalFree in Drop.
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                ptr::null_mut(),
            )
        } == 0
        {
            return Err("create CapyIO service pipe ACL".to_owned());
        }
        Ok(Self(descriptor))
    }

    fn as_mut_ptr(&mut self) -> *mut c_void {
        self.0.cast()
    }
}

impl Drop for SecurityDescriptor {
    fn drop(&mut self) {
        // SAFETY: descriptor was allocated by ConvertStringSecurityDescriptor.
        unsafe {
            LocalFree(self.0.cast());
        }
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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
