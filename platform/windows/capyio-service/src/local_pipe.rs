use std::{
    ffi::c_void,
    fs::OpenOptions,
    mem,
    os::windows::io::AsRawHandle,
    ptr,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

use serde::{Serialize, de::DeserializeOwned};
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
    Storage::FileSystem::{
        FILE_FLAG_FIRST_PIPE_INSTANCE, FlushFileBuffers, PIPE_ACCESS_DUPLEX, ReadFile, WriteFile,
    },
    System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_NOWAIT, PIPE_READMODE_BYTE,
        PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
    },
};

pub(crate) const MAX_CONTROL_BYTES: usize = 4 * 1024;
const IO_DEADLINE: Duration = Duration::from_secs(2);
const CLIENT_RESPONSE_DEADLINE: Duration = Duration::from_secs(10);
const RETRY_INTERVAL: Duration = Duration::from_millis(10);

pub(crate) fn invoke<Request: Serialize, Response: DeserializeOwned>(
    pipe_name: &str,
    request: &Request,
) -> Result<Response, String> {
    invoke_with_open_timeout(pipe_name, request, IO_DEADLINE)
}

pub(crate) fn try_invoke<Request: Serialize, Response: DeserializeOwned>(
    pipe_name: &str,
    request: &Request,
) -> Result<Response, String> {
    invoke_with_open_timeout(pipe_name, request, Duration::from_millis(100))
}

fn invoke_with_open_timeout<Request: Serialize, Response: DeserializeOwned>(
    pipe_name: &str,
    request: &Request,
    open_timeout: Duration,
) -> Result<Response, String> {
    let payload = serde_json::to_vec(request).map_err(|_| "encode local control request")?;
    let mut pipe = open_client_pipe(pipe_name, open_timeout)?;
    write_frame(&mut pipe, &payload, MAX_CONTROL_BYTES)?;
    let response_payload = read_frame(&mut pipe, MAX_CONTROL_BYTES, CLIENT_RESPONSE_DEADLINE)?;
    serde_json::from_slice(&response_payload)
        .map_err(|_| "invalid local control response".to_owned())
}

pub(crate) fn wake(pipe_name: &str) {
    let _ = OpenOptions::new().read(true).write(true).open(pipe_name);
}

fn open_client_pipe(pipe_name: &str, open_timeout: Duration) -> Result<std::fs::File, String> {
    let deadline = Instant::now() + open_timeout;
    loop {
        match OpenOptions::new().read(true).write(true).open(pipe_name) {
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
                return Err("local control host is unavailable".to_owned());
            }
            Err(_) => return Err("local control host is unavailable".to_owned()),
        }
    }
}

fn write_frame(
    stream: &mut impl std::io::Write,
    payload: &[u8],
    max_bytes: usize,
) -> Result<(), String> {
    if payload.is_empty() || payload.len() > max_bytes {
        return Err("local control message is outside bounds".to_owned());
    }
    let length = u32::try_from(payload.len()).map_err(|_| "control message length overflow")?;
    stream
        .write_all(&length.to_le_bytes())
        .and_then(|()| stream.write_all(payload))
        .map_err(|_| "write local control message".to_owned())
}

fn read_frame(
    stream: &mut std::fs::File,
    max_bytes: usize,
    timeout: Duration,
) -> Result<Vec<u8>, String> {
    let mut length = [0_u8; 4];
    read_client_exact(stream, &mut length, timeout)?;
    let length = usize::try_from(u32::from_le_bytes(length))
        .map_err(|_| "local control response length overflow".to_owned())?;
    if length == 0 || length > max_bytes {
        return Err("local control response is outside bounds".to_owned());
    }
    let mut payload = vec![0_u8; length];
    read_client_exact(stream, &mut payload, timeout)?;
    Ok(payload)
}

fn read_client_exact(
    stream: &std::fs::File,
    buffer: &mut [u8],
    timeout: Duration,
) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    let handle = stream.as_raw_handle() as HANDLE;
    let mut offset = 0;
    while offset < buffer.len() && Instant::now() < deadline {
        let mut read = 0_u32;
        // SAFETY: the File owns a connected pipe handle and buffer is valid for
        // this bounded read. The server pipe is deliberately non-blocking.
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
        return Err("read local control response".to_owned());
    }
    (offset == buffer.len())
        .then_some(())
        .ok_or_else(|| "local control response timed out".to_owned())
}

pub struct NamedPipe {
    handle: HANDLE,
    max_bytes: usize,
}

impl NamedPipe {
    pub(crate) fn create(pipe_name: &str, pipe_sddl: &str) -> Result<Self, String> {
        Self::create_with_flags(pipe_name, pipe_sddl, 0, MAX_CONTROL_BYTES, 1)
    }

    pub(crate) fn create_exclusive(pipe_name: &str, pipe_sddl: &str) -> Result<Self, String> {
        Self::create_with_flags(
            pipe_name,
            pipe_sddl,
            FILE_FLAG_FIRST_PIPE_INSTANCE,
            MAX_CONTROL_BYTES,
            1,
        )
    }

    /// User-host control only; existing service endpoints retain their 4 KiB limit.
    pub fn create_node_api(pipe_name: &str, pipe_sddl: &str, first: bool) -> Result<Self, String> {
        Self::create_with_flags(
            pipe_name,
            pipe_sddl,
            if first {
                FILE_FLAG_FIRST_PIPE_INSTANCE
            } else {
                0
            },
            NODE_API_BYTES,
            4,
        )
    }

    fn create_with_flags(
        pipe_name: &str,
        pipe_sddl: &str,
        flags: u32,
        max_bytes: usize,
        instances: u32,
    ) -> Result<Self, String> {
        let mut descriptor = SecurityDescriptor::new(pipe_sddl)?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: u32::try_from(mem::size_of::<SECURITY_ATTRIBUTES>())
                .expect("SECURITY_ATTRIBUTES size fits u32"),
            lpSecurityDescriptor: descriptor.as_mut_ptr(),
            bInheritHandle: 0,
        };
        let name = wide(pipe_name);
        let deadline = Instant::now() + IO_DEADLINE;
        loop {
            // SAFETY: the name and security descriptor remain alive for this
            // call. A successful handle is exclusively owned by Self.
            let handle = unsafe {
                CreateNamedPipeW(
                    name.as_ptr(),
                    PIPE_ACCESS_DUPLEX | flags,
                    PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_NOWAIT | PIPE_REJECT_REMOTE_CLIENTS,
                    instances,
                    max_bytes as u32 + 4,
                    max_bytes as u32 + 4,
                    0,
                    &attributes,
                )
            };
            if handle != INVALID_HANDLE_VALUE {
                return Ok(Self { handle, max_bytes });
            }
            // SAFETY: GetLastError immediately follows CreateNamedPipeW.
            if unsafe { GetLastError() } == ERROR_PIPE_BUSY && Instant::now() < deadline {
                thread::sleep(RETRY_INTERVAL);
                continue;
            }
            return Err("create local control pipe".to_owned());
        }
    }

    pub fn wait_for_client(&self, stop: &AtomicBool) -> Result<bool, String> {
        while !stop.load(Ordering::Acquire) {
            // SAFETY: handle is a live named-pipe handle and no OVERLAPPED
            // structure is used in non-blocking mode.
            if unsafe { ConnectNamedPipe(self.handle, ptr::null_mut()) } != 0 {
                return Ok(true);
            }
            // SAFETY: GetLastError immediately follows ConnectNamedPipe.
            match unsafe { GetLastError() } {
                ERROR_PIPE_CONNECTED => return Ok(true),
                ERROR_PIPE_LISTENING => thread::sleep(RETRY_INTERVAL),
                ERROR_NO_DATA => {
                    self.disconnect();
                    thread::sleep(RETRY_INTERVAL);
                }
                _ => return Err("connect local control pipe".to_owned()),
            }
        }
        Ok(false)
    }

    pub fn disconnect(&self) {
        // SAFETY: handle is a live pipe instance. A raced disconnect is benign.
        unsafe {
            DisconnectNamedPipe(self.handle);
        }
    }

    pub fn read_json<T: DeserializeOwned>(&self) -> Result<T, String> {
        let mut length = [0_u8; 4];
        self.read_exact(&mut length)?;
        let length = usize::try_from(u32::from_le_bytes(length))
            .map_err(|_| "local control request length overflow".to_owned())?;
        if length == 0 || length > self.max_bytes {
            return Err("local control request outside bounds".to_owned());
        }
        let mut payload = vec![0_u8; length];
        self.read_exact(&mut payload)?;
        serde_json::from_slice(&payload).map_err(|_| "invalid local control request".to_owned())
    }

    pub(crate) fn write_json<T: Serialize>(&self, response: &T) -> Result<(), String> {
        self.write_response(response)?;
        // Existing service transport retains its delivery semantics.
        if unsafe { FlushFileBuffers(self.handle) } == 0 {
            return Err("flush local control response".to_owned());
        }
        Ok(())
    }

    /// Bounded acknowledgement avoids an unbounded FlushFileBuffers wait on UI exit.
    pub fn write_node_response<T: Serialize>(&self, response: &T) -> Result<(), String> {
        self.write_response(response)?;
        let mut ack = [0u8];
        self.read_exact(&mut ack)?;
        if ack != [1] {
            return Err("invalid node response acknowledgement".into());
        }
        Ok(())
    }

    fn write_response<T: Serialize>(&self, response: &T) -> Result<(), String> {
        let payload = serde_json::to_vec(response).map_err(|_| "encode local control response")?;
        if payload.is_empty() || payload.len() > self.max_bytes {
            return Err("local control response outside bounds".to_owned());
        }
        let length = u32::try_from(payload.len()).map_err(|_| "control response overflow")?;
        self.write_all(&length.to_le_bytes())?;
        self.write_all(&payload)?;
        Ok(())
    }

    fn read_exact(&self, buffer: &mut [u8]) -> Result<(), String> {
        let deadline = Instant::now() + IO_DEADLINE;
        let mut offset = 0;
        while offset < buffer.len() && Instant::now() < deadline {
            let mut read = 0_u32;
            // SAFETY: buffer is valid for the bounded requested write and the
            // handle is live synchronous non-blocking pipe handle.
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
            // SAFETY: GetLastError immediately follows ReadFile.
            let error = unsafe { GetLastError() };
            if result == 0 && error == ERROR_BROKEN_PIPE {
                break;
            }
            if result != 0 || error == ERROR_NO_DATA {
                thread::sleep(RETRY_INTERVAL);
                continue;
            }
            return Err("read local control pipe".to_owned());
        }
        (offset == buffer.len())
            .then_some(())
            .ok_or_else(|| "local control read timed out".to_owned())
    }

    fn write_all(&self, buffer: &[u8]) -> Result<(), String> {
        let deadline = Instant::now() + IO_DEADLINE;
        let mut offset = 0;
        while offset < buffer.len() && Instant::now() < deadline {
            let mut written = 0_u32;
            // SAFETY: buffer is valid for the bounded requested read and the
            // handle is a live synchronous non-blocking pipe handle.
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
            // SAFETY: GetLastError immediately follows WriteFile.
            let error = unsafe { GetLastError() };
            if result != 0 || error == ERROR_NO_DATA {
                thread::sleep(RETRY_INTERVAL);
                continue;
            }
            return Err("write local control pipe".to_owned());
        }
        (offset == buffer.len())
            .then_some(())
            .ok_or_else(|| "local control write timed out".to_owned())
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
    fn new(pipe_sddl: &str) -> Result<Self, String> {
        let sddl = wide(pipe_sddl);
        let mut descriptor = ptr::null_mut();
        // SAFETY: SDDL is terminated immutable UTF-16 and LocalFree releases
        // the returned descriptor in Drop.
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                ptr::null_mut(),
            )
        } == 0
        {
            return Err("create local control pipe ACL".to_owned());
        }
        Ok(Self(descriptor))
    }

    fn as_mut_ptr(&mut self) -> *mut c_void {
        self.0.cast()
    }
}

impl Drop for SecurityDescriptor {
    fn drop(&mut self) {
        // SAFETY: ConvertStringSecurityDescriptor allocated this descriptor.
        unsafe {
            LocalFree(self.0.cast());
        }
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

const NODE_API_BYTES: usize = 1024 * 1024;

/// Current-user node command transport. No media payloads use this endpoint.
pub fn invoke_node<Request: Serialize, Response: DeserializeOwned>(
    pipe_name: &str,
    request: &Request,
) -> Result<Response, String> {
    use std::io::Write;
    let payload = serde_json::to_vec(request).map_err(|_| "encode node request")?;
    let mut pipe = open_client_pipe(pipe_name, IO_DEADLINE)?;
    verify_node_server(&pipe)?;
    let mode = PIPE_READMODE_BYTE | PIPE_NOWAIT;
    if unsafe {
        windows_sys::Win32::System::Pipes::SetNamedPipeHandleState(
            pipe.as_raw_handle() as HANDLE,
            &mode,
            ptr::null(),
            ptr::null(),
        )
    } == 0
    {
        return Err("configure bounded node API I/O".into());
    }
    write_frame(&mut pipe, &payload, NODE_API_BYTES)?;
    let response = read_frame(&mut pipe, NODE_API_BYTES, Duration::from_secs(60))?;
    pipe.write_all(&[1])
        .map_err(|_| "acknowledge node response")?;
    serde_json::from_slice(&response).map_err(|_| "invalid node response".into())
}

// The client only sends commands to this installed executable, under the pipe's
// current-user ACL. Do not send enrollment PINs to an arbitrary endpoint owner.
fn verify_node_server(pipe: &std::fs::File) -> Result<(), String> {
    use windows_sys::Win32::System::{
        Pipes::GetNamedPipeServerProcessId,
        Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW},
    };
    let mut pid = 0;
    if unsafe { GetNamedPipeServerProcessId(pipe.as_raw_handle() as HANDLE, &mut pid) } == 0 {
        return Err("identify node API host".into());
    }
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return Err("inspect node API host".into());
    }
    let mut image = vec![0u16; 32768];
    let mut length = image.len() as u32;
    let result = unsafe { QueryFullProcessImageNameW(process, 0, image.as_mut_ptr(), &mut length) };
    unsafe {
        CloseHandle(process);
    }
    if result == 0 {
        return Err("read node API host identity".into());
    }
    let actual = std::path::PathBuf::from(String::from_utf16_lossy(&image[..length as usize]));
    let expected = std::env::current_exe().map_err(|_| "locate node executable")?;
    let actual = actual.canonicalize().map_err(|_| "resolve node API host")?;
    let expected = expected
        .canonicalize()
        .map_err(|_| "resolve node executable")?;
    if !actual
        .to_string_lossy()
        .eq_ignore_ascii_case(&expected.to_string_lossy())
    {
        return Err("node API host executable mismatch; exit the old node before upgrading".into());
    }
    Ok(())
}

/// Keeps a service reservation alive until the client has opened the shared object.
pub(crate) fn invoke_service_handoff<T>(
    pipe_name: &str,
    request: &impl Serialize,
    acquire: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    use std::io::Write;
    use windows_sys::Win32::System::{
        Pipes::{GetNamedPipeServerProcessId, SetNamedPipeHandleState},
        Services::{
            CloseServiceHandle, OpenSCManagerW, OpenServiceW, QueryServiceStatusEx,
            SC_MANAGER_CONNECT, SC_STATUS_PROCESS_INFO, SERVICE_QUERY_STATUS,
            SERVICE_STATUS_PROCESS,
        },
    };
    let mut pipe = open_client_pipe(pipe_name, IO_DEADLINE)?;
    let mut pid = 0;
    if unsafe { GetNamedPipeServerProcessId(pipe.as_raw_handle() as HANDLE, &mut pid) } == 0 {
        return Err("identify camera allocation service".into());
    }
    let manager = unsafe { OpenSCManagerW(ptr::null(), ptr::null(), SC_MANAGER_CONNECT) };
    if manager.is_null() {
        return Err("inspect service manager".into());
    }
    let name = wide(crate::SERVICE_NAME);
    let service = unsafe { OpenServiceW(manager, name.as_ptr(), SERVICE_QUERY_STATUS) };
    unsafe { CloseServiceHandle(manager) };
    if service.is_null() {
        return Err("CapyIO Broker is not installed".into());
    }
    let mut status: SERVICE_STATUS_PROCESS = unsafe { mem::zeroed() };
    let mut needed = 0;
    let queried = unsafe {
        QueryServiceStatusEx(
            service,
            SC_STATUS_PROCESS_INFO,
            (&raw mut status).cast(),
            mem::size_of_val(&status) as u32,
            &mut needed,
        )
    };
    unsafe { CloseServiceHandle(service) };
    if queried == 0 || pid == 0 || pid != status.dwProcessId {
        return Err("camera allocation endpoint does not belong to CapyIO Broker".into());
    }
    let mode = PIPE_READMODE_BYTE | PIPE_NOWAIT;
    if unsafe {
        SetNamedPipeHandleState(
            pipe.as_raw_handle() as HANDLE,
            &mode,
            ptr::null(),
            ptr::null(),
        )
    } == 0
    {
        return Err("configure camera allocation deadline".into());
    }
    let payload = serde_json::to_vec(request).map_err(|_| "encode camera allocation request")?;
    write_frame(&mut pipe, &payload, MAX_CONTROL_BYTES)?;
    let bytes = read_frame(&mut pipe, MAX_CONTROL_BYTES, IO_DEADLINE)?;
    let reply: Result<(), String> =
        serde_json::from_slice(&bytes).map_err(|_| "invalid camera allocation response")?;
    let result = reply.and_then(|()| acquire());
    pipe.write_all(&[1])
        .map_err(|_| "acknowledge camera allocation")?;
    result
}
