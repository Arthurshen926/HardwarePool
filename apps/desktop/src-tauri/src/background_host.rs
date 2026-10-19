//! Single-instance UI shell and explicit node exit. No Runtime lives in this process.
use std::{
    ptr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use tauri::{
    Manager,
    menu::{Menu, MenuItem, Submenu},
};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT,
    },
    System::Threading::{
        CreateEventW, CreateMutexExW, EVENT_MODIFY_STATE, OpenEventW, SYNCHRONIZATION_SYNCHRONIZE,
        SetEvent, WaitForSingleObject,
    },
    UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW},
};

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

pub(super) struct Instance {
    _mutex: Handle,
    wake: Handle,
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

/// Available even when WebView creation fails; no Runtime or UI bridge is required.
pub(super) fn show_error(summary: &str, next_step: &str, error: &str) {
    eprintln!("CapyIO: {summary}: {error}");
    let title = wide("CapyIO");
    let message = wide(&format!("{summary}\n\n{next_step}\n\n技术详情：{error}"));
    unsafe {
        MessageBoxW(
            ptr::null_mut(),
            message.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

impl Instance {
    /// None means the existing host was asked to recreate/focus its window.
    pub fn acquire() -> Result<Option<Self>, String> {
        let sid = capyio_windows_service::current_process_user_sid()?;
        Self::acquire_named(&format!("Local\\CapyIO.DesktopShell.{sid}"))
    }

    fn acquire_named(name: &str) -> Result<Option<Self>, String> {
        let mutex_name = wide(name);
        let wake_name = wide(&format!("{name}.Wake"));
        let raw = unsafe {
            CreateMutexExW(
                ptr::null(),
                mutex_name.as_ptr(),
                0,
                SYNCHRONIZATION_SYNCHRONIZE,
            )
        };
        if raw.is_null() {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let existing = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        let mutex = Handle(raw);
        if existing {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                let event = unsafe { OpenEventW(EVENT_MODIFY_STATE, 0, wake_name.as_ptr()) };
                if !event.is_null() {
                    let event = Handle(event);
                    if unsafe { SetEvent(event.0) } == 0 {
                        return Err(std::io::Error::last_os_error().to_string());
                    }
                    return Ok(None);
                }
                if Instant::now() >= deadline {
                    return Err("已有节点尚未提供窗口唤回入口，请稍后重试".into());
                }
                thread::sleep(Duration::from_millis(100));
            }
        }
        let wake = unsafe { CreateEventW(ptr::null(), 0, 0, wake_name.as_ptr()) };
        if wake.is_null() {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(Some(Self {
            _mutex: mutex,
            wake: Handle(wake),
        }))
    }

    pub fn listen(&self, app: tauri::AppHandle) -> WakeLoop {
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = Arc::clone(&stop);
        // Windows handles are usable across threads. Instance outlives WakeLoop;
        // WakeLoop joins the reader before Instance closes the handle.
        let event = self.wake.0 as usize;
        let worker = thread::spawn(move || {
            while !stopping.load(Ordering::Acquire) {
                match unsafe { WaitForSingleObject(event as HANDLE, 200) } {
                    WAIT_OBJECT_0 => {
                        let target = app.clone();
                        if let Err(error) = app.run_on_main_thread(move || {
                            if let Err(error) = show_window(&target) {
                                show_error(
                                    "无法打开窗口",
                                    "请稍后重新打开 CapyIO。后台节点不会因此停止。",
                                    &error.to_string(),
                                );
                            }
                        }) {
                            eprintln!("CapyIO window dispatch: {error}");
                            break;
                        }
                    }
                    WAIT_TIMEOUT => {}
                    _ => {
                        eprintln!("CapyIO wake wait: {}", std::io::Error::last_os_error());
                        break;
                    }
                }
            }
        });
        WakeLoop {
            stop,
            worker: Some(worker),
        }
    }
}

pub(super) struct WakeLoop {
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Drop for WakeLoop {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn show_window(app: &tauri::AppHandle) -> tauri::Result<()> {
    let window = match app.get_webview_window("main") {
        Some(window) => window,
        None => {
            let window =
                tauri::WebviewWindowBuilder::from_config(app, &app.config().app.windows[0])?
                    .build()?;
            window.set_menu(node_menu(app)?)?;
            window
        }
    };
    window.unminimize()?;
    window.show()?;
    window.set_focus()
}

fn node_menu(app: &tauri::AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let quit = MenuItem::with_id(app, "quit-node", "退出节点（停止共享）", true, None::<&str>)?;
    let node = Submenu::with_items(app, "节点", true, &[&quit])?;
    Menu::with_items(app, &[&node])
}

pub(super) fn configure(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    builder.menu(node_menu).on_menu_event(|app, event| {
        if event.id().as_ref() == "quit-node" {
            let app = app.clone();
            tauri::async_runtime::spawn_blocking(move || {
                match super::node_process::call("host_shutdown", serde_json::json!({})) {
                    Ok(_) => app.exit(0),
                    Err(error) => show_error(
                        "未能确认节点已退出",
                        "窗口将保持打开。请检查共享状态，稍后再次选择“节点 → 退出节点”。",
                        &error,
                    ),
                }
            });
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn second_launch_wakes_owner_and_exit_releases_instance() {
        let name = format!("Local\\CapyIO.Test.{}", capyio_core::RouteId::new());
        let owner = Instance::acquire_named(&name).unwrap().unwrap();
        assert!(Instance::acquire_named(&name).unwrap().is_none());
        assert_eq!(
            unsafe { WaitForSingleObject(owner.wake.0, 1000) },
            WAIT_OBJECT_0
        );
        drop(owner);
        assert!(Instance::acquire_named(&name).unwrap().is_some());
    }
}
