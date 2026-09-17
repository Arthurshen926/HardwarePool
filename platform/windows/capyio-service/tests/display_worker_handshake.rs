#![cfg(windows)]

use std::{
    fs::File,
    io::{Read, Write},
    os::windows::io::{AsRawHandle, FromRawHandle},
    process::{Child, Command, ExitStatus, Stdio},
    ptr,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use capyio_display_engine_host::{
    AuthenticatedEngineLaunch, DISPLAY_WORKER_CONFIG_ACK_BYTES, DISPLAY_WORKER_CONTROL_ACK_BYTES,
    DISPLAY_WORKER_LEASE_ACK_BYTES, DISPLAY_WORKER_START_ACK_BYTES, DisplayWorkerControlCommand,
    DisplayWorkerControlRecord, DisplayWorkerEngineConfigRecord, DisplayWorkerLease,
    DisplayWorkerStartAcknowledgement, verify_display_worker_config_ack,
    verify_display_worker_control_ack, verify_display_worker_lease_ack,
    verify_display_worker_start_ack,
};
use capyio_display_stream_adapter::{
    DisplayMode, DisplayOnlyPolicy, DisplayStreamSessionRequest, DynamicRange,
    ValidatedDisplayStreamSession, VideoCodec, launch_record::GameStreamLaunchBinding,
};
use capyio_windows_display::{DisplaySessionContext, InteractiveDisplaySessionId};
use capyio_windows_service::{BoundDisplayWorkerPeer, ExpectedDisplayWorkerProcessId};
use windows_sys::Win32::{
    Foundation::{ERROR_PIPE_CONNECTED, GetLastError, HANDLE, INVALID_HANDLE_VALUE},
    Storage::FileSystem::PIPE_ACCESS_DUPLEX,
    System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
        PIPE_TYPE_BYTE, PIPE_WAIT,
    },
};

#[test]
fn service_and_real_worker_complete_start_key_frame_and_graceful_stop() {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pipe_name = format!(
        r"\\.\pipe\CapyIO.DisplayWorker.{}.{suffix}",
        std::process::id()
    );
    let mut wide_name = pipe_name.encode_utf16().collect::<Vec<_>>();
    wide_name.push(0);
    // SAFETY: the unique name is NUL-terminated, all modes and buffer sizes are
    // fixed, and a successful handle is transferred to File below.
    let raw_pipe = unsafe {
        CreateNamedPipeW(
            wide_name.as_ptr(),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            256,
            256,
            0,
            ptr::null(),
        )
    };
    assert_ne!(raw_pipe, INVALID_HANDLE_VALUE);
    // SAFETY: raw_pipe is the unique successful CreateNamedPipeW result.
    let mut server = unsafe { File::from_raw_handle(raw_pipe) };

    let mut child = Command::new(env!(
        "CARGO_BIN_EXE_capyio-display-worker-connection-fixture"
    ))
    .args(["--lease-pipe", &pipe_name])
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::piped())
    .spawn()
    .expect("spawn display worker fixture");
    let expected_process = ExpectedDisplayWorkerProcessId::from_spawned_child(&child).unwrap();

    // SAFETY: server owns a listening synchronous pipe and no OVERLAPPED value
    // is required. A fast client may already have connected.
    if unsafe { ConnectNamedPipe(server.as_raw_handle() as HANDLE, ptr::null_mut()) } == 0 {
        // SAFETY: GetLastError immediately follows ConnectNamedPipe.
        assert_eq!(unsafe { GetLastError() }, ERROR_PIPE_CONNECTED);
    }

    let context = DisplaySessionContext::query().unwrap();
    let authorized_session =
        InteractiveDisplaySessionId::new(context.process_session_id()).unwrap();
    let peer =
        BoundDisplayWorkerPeer::bind_connected_pipe(&server, expected_process, authorized_session)
            .unwrap();
    assert!(peer.is_running().unwrap());

    let session = ValidatedDisplayStreamSession::new(DisplayStreamSessionRequest {
        adapter_instance_id: [1; 16],
        display_capability_id: [2; 16],
        mode: DisplayMode {
            width: 1920,
            height: 1080,
            refresh_millihertz: 60_000,
        },
        codec: VideoCodec::Hevc,
        dynamic_range: DynamicRange::Sdr8,
        bitrate_kbps: 30_000,
        port_base: 50_000,
        policy: DisplayOnlyPolicy::CAPYIO,
    })
    .unwrap();
    let lease = DisplayWorkerLease::new(
        7,
        authorized_session,
        &session,
        GameStreamLaunchBinding {
            session_id: [3; 16],
            route_id: [4; 16],
            stream_id: [5; 16],
            stream_epoch: 6,
            host_ipv4: std::net::Ipv4Addr::LOCALHOST,
        },
    )
    .unwrap();
    let record = lease.encode();
    server.write_all(&record).unwrap();
    let mut acknowledgement = [0_u8; DISPLAY_WORKER_LEASE_ACK_BYTES];
    server.read_exact(&mut acknowledgement).unwrap();
    verify_display_worker_lease_ack(&record, &acknowledgement).unwrap();
    exchange_engine_configuration(&mut server, &lease, &session);

    let start = AuthenticatedEngineLaunch::new(
        session.request().adapter_instance_id,
        session.request().display_capability_id,
        17,
        [0x42; 16],
        18,
        [0x24; 8],
        19,
    )
    .unwrap()
    .into_worker_start_record(&lease)
    .unwrap();
    server.write_all(start.as_bytes()).unwrap();
    let mut start_acknowledgement = [0_u8; DISPLAY_WORKER_START_ACK_BYTES];
    server.read_exact(&mut start_acknowledgement).unwrap();
    let start_acknowledgement =
        DisplayWorkerStartAcknowledgement::take_from(&mut start_acknowledgement);
    verify_display_worker_start_ack(&start, &lease, &start_acknowledgement).unwrap();

    std::thread::sleep(Duration::from_millis(100));
    assert!(child.try_wait().unwrap().is_none());
    assert!(peer.is_running().unwrap());
    exchange_control(
        &mut server,
        &lease,
        DisplayWorkerControlCommand::RequestKeyFrame,
        1,
    );
    exchange_control(
        &mut server,
        &lease,
        DisplayWorkerControlCommand::GracefulStop,
        2,
    );

    let deadline = Instant::now() + Duration::from_secs(2);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < deadline, "display worker did not exit");
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(status.success());
    assert!(!peer.is_running().unwrap());
}

fn exchange_control(
    server: &mut File,
    lease: &DisplayWorkerLease,
    command: DisplayWorkerControlCommand,
    sequence: u32,
) {
    let record = DisplayWorkerControlRecord::new(lease, command, sequence).unwrap();
    server.write_all(record.as_bytes()).unwrap();
    let mut acknowledgement = [0_u8; DISPLAY_WORKER_CONTROL_ACK_BYTES];
    server.read_exact(&mut acknowledgement).unwrap();
    verify_display_worker_control_ack(&record, lease, &acknowledgement).unwrap();
}

#[test]
fn real_worker_rejects_a_corrupted_secret_start_without_leaking_it() {
    let (mut server, mut child, peer, authorized_session) = spawn_real_worker();
    let (session, lease) = test_session_and_lease(authorized_session);
    exchange_non_secret_lease(&mut server, &lease);
    exchange_engine_configuration(&mut server, &lease, &session);
    let start = test_start(&session, &lease);
    let mut corrupted = *start.as_bytes();
    corrupted[159] = 1;
    server.write_all(&corrupted).unwrap();
    corrupted.fill(0);

    let (status, stderr) = wait_for_child(&mut child, Duration::from_secs(2));
    assert!(!status.success());
    assert!(stderr.contains("display worker start layout is invalid"));
    assert!(!stderr.contains("42424242"));
    assert!(!peer.is_running().unwrap());
}

#[test]
fn real_worker_rejects_a_corrupted_engine_configuration() {
    let (mut server, mut child, peer, authorized_session) = spawn_real_worker();
    let (session, lease) = test_session_and_lease(authorized_session);
    exchange_non_secret_lease(&mut server, &lease);
    let record = DisplayWorkerEngineConfigRecord::new(
        &lease,
        std::path::Path::new(r"C:\CapyIO\capyio-display-engine.dll"),
        [0x31; 32],
        &session,
    )
    .unwrap();
    let mut corrupted = *record.as_bytes();
    corrupted[119] = 1;
    server.write_all(&corrupted).unwrap();

    let (status, stderr) = wait_for_child(&mut child, Duration::from_secs(2));
    assert!(!status.success());
    assert!(stderr.contains("engine configuration layout is invalid"));
    assert!(!peer.is_running().unwrap());
}

#[test]
fn real_worker_fails_closed_when_service_disconnects_before_secret_start() {
    let (mut server, mut child, peer, authorized_session) = spawn_real_worker();
    let (_, lease) = test_session_and_lease(authorized_session);
    exchange_non_secret_lease(&mut server, &lease);
    drop(server);

    let (status, _) = wait_for_child(&mut child, Duration::from_secs(2));
    assert!(!status.success());
    assert!(!peer.is_running().unwrap());
}

#[test]
fn real_worker_times_out_when_service_withholds_secret_start() {
    let (mut server, mut child, peer, authorized_session) = spawn_real_worker();
    let (session, lease) = test_session_and_lease(authorized_session);
    exchange_non_secret_lease(&mut server, &lease);
    exchange_engine_configuration(&mut server, &lease, &session);

    let (status, stderr) = wait_for_child(&mut child, Duration::from_secs(7));
    assert!(!status.success());
    assert!(stderr.contains("timed out"));
    assert!(!peer.is_running().unwrap());
}

fn spawn_real_worker() -> (
    File,
    Child,
    BoundDisplayWorkerPeer,
    InteractiveDisplaySessionId,
) {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pipe_name = format!(
        r"\\.\pipe\CapyIO.DisplayWorker.Fault.{}.{suffix}",
        std::process::id()
    );
    let mut wide_name = pipe_name.encode_utf16().collect::<Vec<_>>();
    wide_name.push(0);
    // SAFETY: the unique name is NUL-terminated and a successful handle is
    // immediately transferred to File.
    let raw_pipe = unsafe {
        CreateNamedPipeW(
            wide_name.as_ptr(),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            256,
            256,
            0,
            ptr::null(),
        )
    };
    assert_ne!(raw_pipe, INVALID_HANDLE_VALUE);
    // SAFETY: raw_pipe is uniquely owned after CreateNamedPipeW succeeds.
    let server = unsafe { File::from_raw_handle(raw_pipe) };
    let child = Command::new(env!(
        "CARGO_BIN_EXE_capyio-display-engine-worker-protocol-fixture"
    ))
    .args(["--lease-pipe", &pipe_name])
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::piped())
    .spawn()
    .unwrap();
    let expected_process = ExpectedDisplayWorkerProcessId::from_spawned_child(&child).unwrap();
    // SAFETY: server owns a synchronous pipe; a fast worker may already be
    // connected, which is reported as ERROR_PIPE_CONNECTED.
    if unsafe { ConnectNamedPipe(server.as_raw_handle() as HANDLE, ptr::null_mut()) } == 0 {
        // SAFETY: GetLastError immediately follows ConnectNamedPipe.
        assert_eq!(unsafe { GetLastError() }, ERROR_PIPE_CONNECTED);
    }
    let context = DisplaySessionContext::query().unwrap();
    let authorized_session =
        InteractiveDisplaySessionId::new(context.process_session_id()).unwrap();
    let peer =
        BoundDisplayWorkerPeer::bind_connected_pipe(&server, expected_process, authorized_session)
            .unwrap();
    (server, child, peer, authorized_session)
}

fn test_session_and_lease(
    authorized_session: InteractiveDisplaySessionId,
) -> (ValidatedDisplayStreamSession, DisplayWorkerLease) {
    let session = ValidatedDisplayStreamSession::new(DisplayStreamSessionRequest {
        adapter_instance_id: [1; 16],
        display_capability_id: [2; 16],
        mode: DisplayMode {
            width: 1920,
            height: 1080,
            refresh_millihertz: 60_000,
        },
        codec: VideoCodec::Hevc,
        dynamic_range: DynamicRange::Sdr8,
        bitrate_kbps: 30_000,
        port_base: 50_000,
        policy: DisplayOnlyPolicy::CAPYIO,
    })
    .unwrap();
    let lease = DisplayWorkerLease::new(
        7,
        authorized_session,
        &session,
        GameStreamLaunchBinding {
            session_id: [3; 16],
            route_id: [4; 16],
            stream_id: [5; 16],
            stream_epoch: 6,
            host_ipv4: std::net::Ipv4Addr::LOCALHOST,
        },
    )
    .unwrap();
    (session, lease)
}

fn exchange_non_secret_lease(server: &mut File, lease: &DisplayWorkerLease) {
    let record = lease.encode();
    server.write_all(&record).unwrap();
    let mut acknowledgement = [0_u8; DISPLAY_WORKER_LEASE_ACK_BYTES];
    server.read_exact(&mut acknowledgement).unwrap();
    verify_display_worker_lease_ack(&record, &acknowledgement).unwrap();
}

fn exchange_engine_configuration(
    server: &mut File,
    lease: &DisplayWorkerLease,
    session: &ValidatedDisplayStreamSession,
) {
    let record = DisplayWorkerEngineConfigRecord::new(
        lease,
        std::path::Path::new(r"C:\CapyIO\capyio-display-engine.dll"),
        [0x31; 32],
        session,
    )
    .unwrap();
    server.write_all(record.as_bytes()).unwrap();
    let mut acknowledgement = [0_u8; DISPLAY_WORKER_CONFIG_ACK_BYTES];
    server.read_exact(&mut acknowledgement).unwrap();
    verify_display_worker_config_ack(&record, lease, &acknowledgement).unwrap();
}

fn test_start(
    session: &ValidatedDisplayStreamSession,
    lease: &DisplayWorkerLease,
) -> capyio_display_engine_host::DisplayWorkerStartRecord {
    AuthenticatedEngineLaunch::new(
        session.request().adapter_instance_id,
        session.request().display_capability_id,
        17,
        [0x42; 16],
        18,
        [0x24; 8],
        19,
    )
    .unwrap()
    .into_worker_start_record(lease)
    .unwrap()
}

fn wait_for_child(child: &mut Child, timeout: Duration) -> (ExitStatus, String) {
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < deadline, "display worker did not exit");
        std::thread::sleep(Duration::from_millis(5));
    };
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    (status, stderr)
}
