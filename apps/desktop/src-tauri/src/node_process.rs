//! Windows UI is a client. Only --node-host creates Runtime and platform workers.
use super::*;
use capyio_windows_service::local_pipe::{NamedPipe, invoke_node};
use serde_json::{Value, json};
use std::os::windows::process::CommandExt;

const API_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    version: u32,
    command: String,
    args: Value,
}

fn endpoint() -> Result<(String, String), String> {
    let sid = capyio_windows_service::current_process_user_sid()?;
    let mut session = 0;
    if unsafe {
        windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId(
            std::process::id(),
            &mut session,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok((
        format!(r"\\.\pipe\CapyIO.Node.v1.{sid}.{session}"),
        format!("D:P(A;;GA;;;{sid})"),
    ))
}

pub(super) fn call(command: &str, args: Value) -> Result<Value, String> {
    let (name, _) = endpoint()?;
    let response: Result<Value, String> = invoke_node(
        &name,
        &Request {
            version: API_VERSION,
            command: command.to_owned(),
            args,
        },
    )?;
    response
}

fn ensure_host() -> Result<(), String> {
    if call("host_status", json!({})).is_ok() {
        return Ok(());
    }
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut child = std::process::Command::new(executable)
        .arg("--node-host")
        .creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("start node host: {e}"))?;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if call("host_status", json!({})).is_ok() {
            return Ok(());
        }
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            return Err(format!("node host exited during startup: {status}"));
        }
        if Instant::now() >= deadline {
            return Err("node host startup timed out".into());
        }
        thread::sleep(Duration::from_millis(100));
    }
}

pub(super) fn run_host() -> Result<(), String> {
    let (name, sddl) = endpoint()?;
    // First-instance creation reserves the endpoint before any Runtime or worker exists.
    let first = NamedPipe::create_node_api(&name, &sddl, true)?;
    let state = Arc::new(HostState(Arc::new(desktop_host::DesktopHost::start())));
    let stop = Arc::new(AtomicBool::new(false));
    let mut workers = Vec::new();
    for _ in 0..3 {
        let (name, sddl, state, stop) = (
            name.clone(),
            sddl.clone(),
            Arc::clone(&state),
            Arc::clone(&stop),
        );
        workers.push(thread::spawn(move || {
            let result = NamedPipe::create_node_api(&name, &sddl, false)
                .and_then(|pipe| serve(&pipe, &state, &stop));
            if let Err(error) = result {
                eprintln!("node API worker: {error}");
            }
        }));
    }
    let result = serve(&first, &state, &stop);
    stop.store(true, Ordering::Release);
    for worker in workers {
        let _ = worker.join();
    }
    // All API calls have finished before the host tears down its platform workers.
    drop(state);
    result
}

fn serve(pipe: &NamedPipe, state: &HostState, stop: &AtomicBool) -> Result<(), String> {
    while pipe.wait_for_client(stop)? {
        let response = pipe.read_json::<Request>().and_then(|request| {
            if request.version != API_VERSION {
                return Err("unsupported node API version".into());
            }
            match request.command.as_str() {
                "host_status" => Ok(json!({"version": API_VERSION, "pid": std::process::id()})),
                "host_shutdown" => {
                    stop.store(true, Ordering::Release);
                    Ok(Value::Null)
                }
                command => {
                    if stop.load(Ordering::Acquire) {
                        return Err("node host is stopping".into());
                    }
                    host_commands::dispatch(state, command, &request.args)
                }
            }
        });
        let _ = pipe.write_node_response(&response);
        pipe.disconnect();
    }
    Ok(())
}

pub(super) fn run_shell() {
    let instance = match background_host::Instance::acquire() {
        Ok(Some(instance)) => instance,
        Ok(None) => return,
        Err(error) => {
            background_host::show_error(
                "无法打开 CapyIO",
                "请稍后重新打开；如果已有窗口，请先切换到该窗口。",
                &error,
            );
            std::process::exit(2);
        }
    };
    if let Err(error) = ensure_host() {
        background_host::show_error(
            "无法连接后台节点",
            "请稍后重新打开 CapyIO。如果问题持续，请保留以下详情以便排查。",
            &error,
        );
        std::process::exit(2);
    }
    let app = match background_host::configure(tauri::Builder::default())
        .invoke_handler(|invoke| {
            let command = invoke.message.command().to_owned();
            let tauri::ipc::InvokeBody::Json(args) = invoke.message.payload() else {
                invoke
                    .resolver
                    .reject("node commands require JSON arguments");
                return true;
            };
            let args = args.clone();
            invoke.resolver.respond_async(async move {
                tauri::async_runtime::spawn_blocking(move || call(&command, args))
                    .await
                    .map_err(|_| tauri::ipc::InvokeError::from("node API task failed"))?
                    .map_err(tauri::ipc::InvokeError::from)
            });
            true
        })
        .build(tauri::generate_context!())
    {
        Ok(app) => app,
        Err(error) => {
            background_host::show_error(
                "无法创建 CapyIO 窗口",
                "请重新打开 CapyIO。后台节点可能仍在运行；重开窗口会重新连接。",
                &error.to_string(),
            );
            std::process::exit(2);
        }
    };
    let wake_loop = instance.listen(app.handle().clone());
    let code = app.run_return(|_, _| {});
    drop(wake_loop);
    if code != 0 {
        std::process::exit(code);
    }
}
