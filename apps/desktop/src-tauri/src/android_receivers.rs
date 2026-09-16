use std::{
    env,
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::Duration,
};

use capyio_windows_input::{VhfBrokerInterfaceProbe, probe_vhf_broker_interface};

use crate::quick_actions::{QUICK_ACTION_SCHEMA_VERSION, QuickActionOperation, UiQuickAction};

pub const TOUCHPAD_RECEIVER_ACTION_ID: &str = "capyio.quick-action.android-touchpad-receiver";
pub const CAMERA_RECEIVER_ACTION_ID: &str = "capyio.quick-action.android-camera-receiver";
pub const SYSTEM_CAMERA_ACTION_ID: &str = "capyio.quick-action.android-system-camera";

const TOUCHPAD_PORT: u16 = 61_000;
const CAMERA_PORT: u16 = 38_173;
const STDERR_TAIL_LIMIT: usize = 4_096;
const READY_LINE_LIMIT: usize = 512;
const READY_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReceiverKind {
    Touchpad,
    Camera,
    SystemCamera,
}

impl ReceiverKind {
    const fn action_id(self) -> &'static str {
        match self {
            Self::Touchpad => TOUCHPAD_RECEIVER_ACTION_ID,
            Self::Camera => CAMERA_RECEIVER_ACTION_ID,
            Self::SystemCamera => SYSTEM_CAMERA_ACTION_ID,
        }
    }

    const fn title(self) -> &'static str {
        match self {
            Self::Touchpad => "接收手机触控板",
            Self::Camera => "接收手机摄像头",
            Self::SystemCamera => "手机作为系统摄像头",
        }
    }

    const fn summary(self) -> &'static str {
        match self {
            Self::Touchpad => "Android Touchpad → Windows 触控板 · 桌面端自动托管",
            Self::Camera => "Android Camera2 → Windows AVC 解码接收端 · 桌面端自动托管",
            Self::SystemCamera => {
                "供 Windows 相机应用使用 · 需预先部署相机组件及共享内存权限 · 无手机画面时显示占位图"
            }
        }
    }

    const fn executable(self) -> &'static str {
        match self {
            Self::Touchpad => "capyio-ptp-adb-lab.exe",
            Self::Camera | Self::SystemCamera => "capyio-avc-lab-receiver.exe",
        }
    }

    const fn port(self) -> u16 {
        match self {
            Self::Touchpad => TOUCHPAD_PORT,
            Self::Camera | Self::SystemCamera => CAMERA_PORT,
        }
    }

    fn arguments(self) -> Result<Vec<&'static str>, String> {
        match self {
            Self::Touchpad => select_touchpad_arguments(probe_vhf_broker_interface()),
            Self::Camera | Self::SystemCamera => Ok(vec![
                "--max-access-units",
                "7200",
                if self == Self::SystemCamera {
                    "--publish-shared"
                } else {
                    "--decode-nv12"
                },
                "--reconnect-grace-millis",
                "60000",
            ]),
        }
    }
}

struct ReceiverProcess {
    child: Child,
    serial: String,
    projection: &'static str,
    stderr_tail: Arc<Mutex<Vec<u8>>>,
}

struct Receiver {
    kind: ReceiverKind,
    process: Option<ReceiverProcess>,
    problem: Option<String>,
}

impl Receiver {
    const fn new(kind: ReceiverKind) -> Self {
        Self {
            kind,
            process: None,
            problem: None,
        }
    }

    fn start(&mut self) -> Result<(), String> {
        self.poll();
        if self.process.is_some() {
            return Ok(());
        }
        self.problem = None;
        let adb = adb_path()?;
        let serial = single_authorized_device(&adb)?;
        let executable = sibling_executable(self.kind.executable())?;
        let arguments = self.kind.arguments()?;
        let projection = if self.kind == ReceiverKind::SystemCamera {
            "NV12 shared frames → system camera"
        } else if self.kind == ReceiverKind::Camera {
            "AVC → NV12 decode"
        } else if arguments.contains(&"--vhf") {
            "installed VHF"
        } else {
            "user-mode synthetic"
        };
        configure_reverse(&adb, &serial, self.kind.port())?;
        let mut child = Command::new(&executable)
            .args(arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                let _ = remove_reverse(&adb, &serial, self.kind.port());
                format!("无法启动 {}: {error}", executable.display())
            })?;
        let stderr_tail = Arc::new(Mutex::new(Vec::with_capacity(STDERR_TAIL_LIMIT)));
        if let Some(stderr) = child.stderr.take() {
            capture_stderr_tail(stderr, Arc::clone(&stderr_tail));
        }
        let Some(stdout) = child.stdout.take() else {
            let _ = child.kill();
            let _ = child.wait();
            let _ = remove_reverse(&adb, &serial, self.kind.port());
            return Err("接收端就绪通道不可用".to_owned());
        };
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let kind = self.kind;
        thread::spawn(move || {
            let mut stdout = stdout;
            let result = read_receiver_ready(kind, &mut stdout);
            let _ = ready_sender.send(result);
            let _ = io::copy(&mut stdout, &mut io::sink());
        });
        let ready = match ready_receiver.recv_timeout(READY_TIMEOUT) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => Err("接收端就绪等待超时".to_owned()),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err("接收端就绪通道提前关闭".to_owned()),
        };
        if let Err(error) = ready {
            let _ = child.kill();
            let _ = child.wait();
            let _ = remove_reverse(&adb, &serial, self.kind.port());
            let detail = stderr_tail
                .lock()
                .map(|bytes| String::from_utf8_lossy(&bytes).trim().to_owned())
                .unwrap_or_default();
            return if detail.is_empty() {
                Err(error)
            } else {
                Err(format!("{error}: {detail}"))
            };
        }
        self.process = Some(ReceiverProcess {
            child,
            serial,
            projection,
            stderr_tail,
        });
        Ok(())
    }

    fn stop(&mut self) -> Result<(), String> {
        let adb = adb_path();
        if let Some(mut process) = self.process.take() {
            if process
                .child
                .try_wait()
                .map_err(|error| error.to_string())?
                .is_none()
            {
                process.child.kill().map_err(|error| error.to_string())?;
                process.child.wait().map_err(|error| error.to_string())?;
            }
            if let Ok(adb) = adb {
                remove_reverse(&adb, &process.serial, self.kind.port())?;
            }
        }
        self.problem = None;
        Ok(())
    }

    fn poll(&mut self) {
        let Some(process) = self.process.as_mut() else {
            return;
        };
        let outcome = process.child.try_wait();
        match outcome {
            Ok(None) => return,
            Ok(Some(status)) => {
                let detail = process
                    .stderr_tail
                    .lock()
                    .map(|bytes| String::from_utf8_lossy(&bytes).trim().to_owned())
                    .unwrap_or_default();
                self.problem = Some(receiver_exit_problem(self.kind, status.code(), &detail));
            }
            Err(error) => {
                self.problem = Some(format!("无法读取接收端状态: {error}"));
            }
        }
        if let Some(process) = self.process.take()
            && let Ok(adb) = adb_path()
        {
            let _ = remove_reverse(&adb, &process.serial, self.kind.port());
        }
    }

    fn dto(&mut self) -> UiQuickAction {
        self.poll();
        let running = self.process.is_some();
        UiQuickAction {
            schema_version: QUICK_ACTION_SCHEMA_VERSION,
            id: self.kind.action_id(),
            kind: "capability_control",
            title: self.kind.title(),
            summary: self.kind.summary(),
            status: if running {
                "active"
            } else if self.problem.is_some() {
                "failed"
            } else {
                "idle"
            },
            simulated: false,
            route_id: None,
            route_state: None,
            route_epoch: None,
            available_operations: if running { vec!["stop"] } else { vec!["start"] },
            evidence_level: if running {
                "process_and_route_state"
            } else {
                "not_started"
            },
            connection_hint: self.process.as_ref().map(|process| {
                format!(
                    "ADB {} · 127.0.0.1:{} · {}",
                    process.serial,
                    self.kind.port(),
                    process.projection
                )
            }),
            problem_code: self
                .problem
                .as_ref()
                .map(|problem| receiver_problem_code(self.kind, problem).to_owned()),
            problem: self.problem.clone(),
        }
    }
}

fn read_receiver_ready(kind: ReceiverKind, reader: &mut impl Read) -> Result<(), String> {
    let mut line = Vec::with_capacity(128);
    for _ in 0..READY_LINE_LIMIT {
        let mut byte = [0_u8];
        if reader.read(&mut byte).map_err(|error| error.to_string())? == 0 {
            return Err("接收端在就绪前关闭输出".to_owned());
        }
        if byte[0] == b'\n' {
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            let line = std::str::from_utf8(&line).map_err(|_| "接收端就绪记录不是 UTF-8")?;
            let matches = match kind {
                ReceiverKind::Touchpad => line == "lab_status=listening",
                ReceiverKind::Camera | ReceiverKind::SystemCamera => {
                    line.starts_with("CAPYIO_AVC_LAB_READY ")
                }
            };
            return matches
                .then_some(())
                .ok_or_else(|| "接收端就绪记录不匹配".to_owned());
        }
        line.push(byte[0]);
    }
    Err("接收端就绪记录超出长度限制".to_owned())
}

fn select_touchpad_arguments(probe: VhfBrokerInterfaceProbe) -> Result<Vec<&'static str>, String> {
    match probe {
        VhfBrokerInterfaceProbe::Single => Ok(touchpad_arguments(true)),
        VhfBrokerInterfaceProbe::Absent => Ok(touchpad_arguments(false)),
        VhfBrokerInterfaceProbe::Multiple => {
            Err("检测到多个 VHF 接口，无法确定触控板目标".to_owned())
        }
        VhfBrokerInterfaceProbe::Failed(code) => {
            Err(format!("VHF 接口检测失败（Windows 错误 {code}）"))
        }
        VhfBrokerInterfaceProbe::UnsupportedPlatform => {
            Err("当前平台不支持 Windows 触控板接收".to_owned())
        }
    }
}

fn touchpad_arguments(vhf_available: bool) -> Vec<&'static str> {
    let mut arguments = vec![
        "--inject",
        "--acknowledge-desktop-input",
        "--manual-session",
    ];
    if vhf_available {
        arguments.push("--vhf");
    }
    arguments
}

pub(crate) fn capture_stderr_tail(
    mut stderr: impl Read + Send + 'static,
    tail: Arc<Mutex<Vec<u8>>>,
) {
    thread::spawn(move || {
        let mut chunk = [0_u8; 512];
        while let Ok(count) = stderr.read(&mut chunk) {
            if count == 0 {
                break;
            }
            let Ok(mut bytes) = tail.lock() else {
                break;
            };
            append_stderr_tail(&mut bytes, &chunk[..count]);
        }
    });
}

fn append_stderr_tail(tail: &mut Vec<u8>, chunk: &[u8]) {
    tail.extend_from_slice(chunk);
    if tail.len() > STDERR_TAIL_LIMIT {
        let overflow = tail.len() - STDERR_TAIL_LIMIT;
        tail.drain(..overflow);
    }
}

fn receiver_exit_problem(kind: ReceiverKind, code: Option<i32>, stderr: &str) -> String {
    let exit = code.map_or_else(|| "无退出码".to_owned(), |value| value.to_string());
    if kind == ReceiverKind::SystemCamera
        && stderr.contains("CreateFileMappingW failed with error 5")
    {
        return "Windows 拒绝创建系统摄像头共享通道（错误 5）；当前原型需以管理员身份运行 CapyIO Windows 客户端".to_owned();
    }
    if kind == ReceiverKind::Touchpad
        && stderr.contains("VHF touchpad session failed")
        && stderr.contains("Win32(5)")
    {
        return "已安装的 VHF 触控板接口需要管理员权限；请以管理员身份运行 CapyIO Windows 客户端后重试".to_owned();
    }
    if kind == ReceiverKind::Touchpad
        && (stderr.contains("submission failed: 5")
            || stderr.contains("batch submission failed: 5")
            || stderr.contains("Access is denied"))
    {
        return "Windows 拒绝合成触控板输入（错误 5）；请使用同等权限运行桌面端，或在明确授权后改用现有 VHF 实验路径".to_owned();
    }
    let bounded_detail = stderr.lines().last().unwrap_or_default().trim();
    if bounded_detail.is_empty() {
        format!("接收端已退出（{exit}）；请确认手机端功能已启动并重试")
    } else {
        format!("接收端已退出（{exit}）：{bounded_detail}")
    }
}

fn receiver_problem_code(kind: ReceiverKind, problem: &str) -> &'static str {
    if kind == ReceiverKind::SystemCamera {
        if problem.contains("管理员身份") {
            "CAPY.WINDOWS.SYSTEM_CAMERA_ELEVATION_REQUIRED"
        } else {
            "CAPY.WINDOWS.SYSTEM_CAMERA_FAILED"
        }
    } else if kind == ReceiverKind::Touchpad && problem.contains("管理员权限") {
        "CAPY.WINDOWS.TOUCHPAD_ELEVATION_REQUIRED"
    } else if kind == ReceiverKind::Touchpad && problem.contains("错误 5") {
        "CAPY.WINDOWS.TOUCHPAD_INJECTION_DENIED"
    } else {
        "CAPY.WINDOWS.ANDROID_RECEIVER_EXITED"
    }
}

pub struct AndroidReceiverActions {
    touchpad: Receiver,
    camera: Receiver,
    system_camera: Receiver,
    camera_session: Option<crate::camera_session::CameraSession>,
}

impl AndroidReceiverActions {
    pub const fn new() -> Self {
        Self {
            touchpad: Receiver::new(ReceiverKind::Touchpad),
            camera: Receiver::new(ReceiverKind::Camera),
            system_camera: Receiver::new(ReceiverKind::SystemCamera),
            camera_session: None,
        }
    }

    pub fn actions(&mut self) -> Vec<UiQuickAction> {
        self.poll_system_camera();
        let mut system = self.system_camera.dto();
        if self.camera_session.is_some() && self.system_camera.process.is_some() {
            system.connection_hint =
                Some("系统相机会话已就绪；接收器运行中，手机画面到达需单独确认".to_owned());
        }
        apply_system_camera_permission(&mut system, crate::camera_session::process_is_elevated());
        vec![self.touchpad.dto(), self.camera.dto(), system]
    }

    pub fn invoke(
        &mut self,
        action_id: &str,
        operation: QuickActionOperation,
    ) -> Result<UiQuickAction, String> {
        self.poll_system_camera();
        if action_id == SYSTEM_CAMERA_ACTION_ID {
            let result = match operation {
                QuickActionOperation::Start | QuickActionOperation::Retry => {
                    self.start_system_camera()
                }
                QuickActionOperation::Stop => self.stop_system_camera(),
            };
            if let Err(error) = result {
                self.system_camera.problem = Some(error);
            }
            return Ok(self.system_camera.dto());
        }
        if action_id == CAMERA_RECEIVER_ACTION_ID
            && !matches!(operation, QuickActionOperation::Stop)
            && self.camera_session.is_some()
        {
            return Err("请先停止系统摄像头，再启动仅解码接收".to_owned());
        }
        let receiver = match action_id {
            TOUCHPAD_RECEIVER_ACTION_ID => &mut self.touchpad,
            CAMERA_RECEIVER_ACTION_ID => &mut self.camera,
            _ => return Err("unknown Android receiver Quick Action".to_owned()),
        };
        match operation {
            QuickActionOperation::Start | QuickActionOperation::Retry => receiver.start()?,
            QuickActionOperation::Stop => receiver.stop()?,
        }
        Ok(receiver.dto())
    }

    pub fn owns(action_id: &str) -> bool {
        matches!(
            action_id,
            TOUCHPAD_RECEIVER_ACTION_ID | CAMERA_RECEIVER_ACTION_ID | SYSTEM_CAMERA_ACTION_ID
        )
    }

    pub fn shutdown(&mut self) {
        let _ = self.stop_system_camera();
        let _ = self.touchpad.stop();
        let _ = self.camera.stop();
    }

    fn start_system_camera(&mut self) -> Result<(), String> {
        if self.camera_session.is_some() {
            return Ok(());
        }
        match crate::camera_session::process_is_elevated() {
            Ok(true) => {}
            Ok(false) => {
                return Err(
                    "当前原型需以管理员身份运行 CapyIO Windows 客户端，才能创建系统摄像头共享通道"
                        .to_owned(),
                );
            }
            Err(error) => return Err(error),
        }
        self.camera.poll();
        if self.camera.process.is_some() {
            return Err("请先停止仅解码接收，再启动系统摄像头".to_owned());
        }
        // Resolve prerequisites before creating a camera session.
        sibling_executable(self.system_camera.kind.executable())?;
        let adb = adb_path()?;
        single_authorized_device(&adb)?;
        let mut session = crate::camera_session::CameraSession::start()?;
        if let Err(error) = self.system_camera.start() {
            let cleanup = session.stop();
            return Err(format!("{error}; 相机回滚: {cleanup:?}"));
        }
        self.camera_session = Some(session);
        Ok(())
    }

    fn stop_system_camera(&mut self) -> Result<(), String> {
        let camera = self
            .camera_session
            .take()
            .map(|mut session| session.stop())
            .transpose();
        let receiver = self.system_camera.stop();
        camera?;
        receiver
    }

    fn poll_system_camera(&mut self) {
        self.system_camera.poll();
        let problem = self
            .camera_session
            .as_mut()
            .and_then(|session| session.poll().err())
            .or_else(|| {
                if self.camera_session.is_some() && self.system_camera.process.is_none() {
                    Some(
                        self.system_camera
                            .problem
                            .clone()
                            .unwrap_or_else(|| "画面接收器已停止".to_owned()),
                    )
                } else {
                    None
                }
            });
        if let Some(problem) = problem {
            let cleanup = self.stop_system_camera();
            self.system_camera.problem = Some(format!("{problem}; 清理: {cleanup:?}"));
        }
    }
}

fn apply_system_camera_permission(action: &mut UiQuickAction, elevated: Result<bool, String>) {
    if action.status == "active" {
        return;
    }
    match elevated {
        Ok(true) => {}
        Ok(false) => {
            action.status = "blocked";
            action.available_operations.clear();
            action.problem_code = Some("CAPY.WINDOWS.SYSTEM_CAMERA_ELEVATION_REQUIRED".to_owned());
            action.problem = Some(
                "当前原型需以管理员身份运行 CapyIO Windows 客户端，才能创建系统摄像头共享通道"
                    .to_owned(),
            );
        }
        Err(error) => {
            action.status = "blocked";
            action.available_operations.clear();
            action.problem_code = Some("CAPY.WINDOWS.SYSTEM_CAMERA_PREFLIGHT_FAILED".to_owned());
            action.problem = Some(error);
        }
    }
}

impl Drop for AndroidReceiverActions {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub(crate) fn adb_path() -> Result<PathBuf, String> {
    if let Some(path) = env::var_os("CAPYIO_ADB").map(PathBuf::from)
        && path.is_file()
    {
        return Ok(path);
    }
    for root in [env::var_os("ANDROID_HOME"), env::var_os("ANDROID_SDK_ROOT")]
        .into_iter()
        .flatten()
    {
        let candidate = PathBuf::from(root).join("platform-tools").join("adb.exe");
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err("未找到 Android platform-tools/adb.exe；请设置 ANDROID_HOME 或 CAPYIO_ADB".to_owned())
}

pub(crate) fn sibling_executable(name: &str) -> Result<PathBuf, String> {
    let directory = env::current_exe()
        .map_err(|error| format!("无法定位 CapyIO 桌面程序: {error}"))?
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "CapyIO 桌面程序没有父目录".to_owned())?;
    let path = directory.join(name);
    if path.is_file() {
        return Ok(path);
    }
    #[cfg(test)]
    if let Some(target_profile) = directory.parent() {
        let test_path = target_profile.join(name);
        if test_path.is_file() {
            return Ok(test_path);
        }
    }
    Err(format!("缺少桌面托管组件: {}", path.display()))
}

fn single_authorized_device(adb: &Path) -> Result<String, String> {
    let output = Command::new(adb)
        .arg("devices")
        .output()
        .map_err(|error| format!("无法执行 adb devices: {error}"))?;
    if !output.status.success() {
        return Err("adb devices 执行失败".to_owned());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_single_authorized_device(&stdout)
}

fn parse_single_authorized_device(output: &str) -> Result<String, String> {
    let devices = output
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .filter_map(|(serial, state)| (state.trim() == "device").then_some(serial.trim()))
        .filter(|serial| !serial.is_empty())
        .collect::<Vec<_>>();
    match devices.as_slice() {
        [serial] => Ok((*serial).to_owned()),
        [] => Err("未发现已授权的 Android 调试设备".to_owned()),
        _ => Err("发现多个 Android 调试设备；请只保留一个已授权设备".to_owned()),
    }
}

fn configure_reverse(adb: &Path, serial: &str, port: u16) -> Result<(), String> {
    let mapping = format!("tcp:{port}");
    let status = Command::new(adb)
        .args(["-s", serial, "reverse", &mapping, &mapping])
        .status()
        .map_err(|error| format!("无法创建 ADB reverse 映射: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("ADB reverse 映射失败: {status}"))
    }
}

fn remove_reverse(adb: &Path, serial: &str, port: u16) -> Result<(), String> {
    let mapping = format!("tcp:{port}");
    let status = Command::new(adb)
        .args(["-s", serial, "reverse", "--remove", &mapping])
        .status()
        .map_err(|error| format!("无法移除 ADB reverse 映射: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("移除 ADB reverse 映射失败: {status}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_exactly_one_authorized_device() {
        let output = "List of devices attached\n100.66.157.119:44187\tdevice\n";
        assert_eq!(
            parse_single_authorized_device(output).unwrap(),
            "100.66.157.119:44187"
        );
    }

    #[test]
    fn rejects_zero_or_multiple_authorized_devices() {
        assert!(parse_single_authorized_device("List of devices attached\n").is_err());
        assert!(
            parse_single_authorized_device(
                "List of devices attached\na\tdevice\nb\tdevice\nc\tunauthorized\n"
            )
            .is_err()
        );
    }

    #[test]
    fn receiver_arguments_are_fixed_and_bounded() {
        assert_eq!(ReceiverKind::Touchpad.port(), 61_000);
        assert!(touchpad_arguments(false).contains(&"--manual-session"));
        assert!(!touchpad_arguments(false).contains(&"--vhf"));
        assert!(touchpad_arguments(true).contains(&"--vhf"));
        assert_eq!(ReceiverKind::Camera.port(), 38_173);
        assert!(ReceiverKind::Camera.arguments().unwrap().contains(&"7200"));
        assert!(
            ReceiverKind::SystemCamera
                .arguments()
                .unwrap()
                .contains(&"--publish-shared")
        );
        assert!(
            !ReceiverKind::Camera
                .arguments()
                .unwrap()
                .contains(&"--publish-shared")
        );
        assert_eq!(
            ReceiverKind::SystemCamera.port(),
            ReceiverKind::Camera.port()
        );
    }

    #[test]
    fn receiver_readiness_is_exact_bounded_and_kind_specific() {
        assert!(
            read_receiver_ready(
                ReceiverKind::Touchpad,
                &mut &b"lab_status=listening\r\n"[..]
            )
            .is_ok()
        );
        assert!(
            read_receiver_ready(
                ReceiverKind::Camera,
                &mut &b"CAPYIO_AVC_LAB_READY transport=adb-reverse-lab\n"[..]
            )
            .is_ok()
        );
        assert!(
            read_receiver_ready(
                ReceiverKind::SystemCamera,
                &mut &b"CAPYIO_AVC_LAB_READY transport=adb-reverse-lab\n"[..]
            )
            .is_ok()
        );
        assert!(
            read_receiver_ready(
                ReceiverKind::Touchpad,
                &mut &b"CAPYIO_AVC_LAB_READY x\n"[..]
            )
            .is_err()
        );
        assert!(
            read_receiver_ready(ReceiverKind::Camera, &mut &b"lab_status=listening\n"[..]).is_err()
        );
        assert!(
            read_receiver_ready(ReceiverKind::Camera, &mut &[b'x'; READY_LINE_LIMIT + 1][..])
                .is_err()
        );
    }

    #[test]
    fn ambiguous_or_failed_probe_never_falls_back_to_injection() {
        assert!(select_touchpad_arguments(VhfBrokerInterfaceProbe::Multiple).is_err());
        assert!(select_touchpad_arguments(VhfBrokerInterfaceProbe::Failed(5)).is_err());
        assert!(select_touchpad_arguments(VhfBrokerInterfaceProbe::UnsupportedPlatform).is_err());
        assert!(
            !select_touchpad_arguments(VhfBrokerInterfaceProbe::Absent)
                .unwrap()
                .contains(&"--vhf")
        );
        assert!(
            select_touchpad_arguments(VhfBrokerInterfaceProbe::Single)
                .unwrap()
                .contains(&"--vhf")
        );
    }

    #[test]
    fn touchpad_access_denied_has_a_stable_problem_code() {
        let problem = receiver_exit_problem(
            ReceiverKind::Touchpad,
            Some(1),
            "synthetic touchpad batch submission failed: 5",
        );
        assert_eq!(
            receiver_problem_code(ReceiverKind::Touchpad, &problem),
            "CAPY.WINDOWS.TOUCHPAD_INJECTION_DENIED"
        );
        assert!(problem.contains("错误 5"));
    }

    #[test]
    fn vhf_access_denied_requests_elevated_desktop_without_driver_mutation() {
        let problem = receiver_exit_problem(
            ReceiverKind::Touchpad,
            Some(1),
            "VHF touchpad session failed: Client(Transport(Win32(5)))",
        );
        assert_eq!(
            receiver_problem_code(ReceiverKind::Touchpad, &problem),
            "CAPY.WINDOWS.TOUCHPAD_ELEVATION_REQUIRED"
        );
        assert!(problem.contains("管理员权限"));
    }

    #[test]
    fn receiver_exit_detail_is_bounded_to_the_last_line() {
        assert_eq!(
            receiver_exit_problem(ReceiverKind::Camera, Some(2), "first\nlast"),
            "接收端已退出（2）：last"
        );
    }

    #[test]
    fn stderr_capture_retains_only_the_bounded_tail() {
        let mut tail = vec![b'a'; STDERR_TAIL_LIMIT - 2];
        append_stderr_tail(&mut tail, b"bcdef");
        assert_eq!(tail.len(), STDERR_TAIL_LIMIT);
        assert_eq!(&tail[STDERR_TAIL_LIMIT - 5..], b"bcdef");
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "requires one explicitly authorized physical Android debugging device"]
    fn physical_camera_receiver_requires_readiness_and_cleans_mapping() {
        let mut actions = AndroidReceiverActions::new();
        let camera = actions
            .invoke(CAMERA_RECEIVER_ACTION_ID, QuickActionOperation::Start)
            .expect("start readiness-confirmed camera receiver");
        assert_eq!(camera.status, "active");
        let stopped = actions
            .invoke(CAMERA_RECEIVER_ACTION_ID, QuickActionOperation::Stop)
            .expect("stop readiness-confirmed camera receiver");
        assert_eq!(stopped.status, "idle");
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "requires one explicitly authorized physical Android debugging device"]
    fn physical_device_mapping_and_both_receiver_lifecycles_are_owned() {
        let mut actions = AndroidReceiverActions::new();
        let touchpad = actions
            .invoke(TOUCHPAD_RECEIVER_ACTION_ID, QuickActionOperation::Start)
            .expect("start desktop-owned touchpad receiver");
        assert_eq!(touchpad.status, "active");
        assert!(touchpad.connection_hint.is_some());
        let stopped = actions
            .invoke(TOUCHPAD_RECEIVER_ACTION_ID, QuickActionOperation::Stop)
            .expect("stop desktop-owned touchpad receiver");
        assert_eq!(stopped.status, "idle");

        let camera = actions
            .invoke(CAMERA_RECEIVER_ACTION_ID, QuickActionOperation::Start)
            .expect("start desktop-owned camera receiver");
        assert_eq!(camera.status, "active");
        assert!(camera.connection_hint.is_some());
        let stopped = actions
            .invoke(CAMERA_RECEIVER_ACTION_ID, QuickActionOperation::Stop)
            .expect("stop desktop-owned camera receiver");
        assert_eq!(stopped.status, "idle");
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "requires the authorized Android camera and deployed Windows camera COM source"]
    fn physical_system_camera_action_receives_a_live_phone_frame_and_cleans_up() {
        use capyio_windows_camera_share::CameraSharedIngressConsumer;
        use std::io::Write;
        use std::time::{Duration, Instant};

        let wait_seconds = std::env::var("CAPYIO_LIVE_CAMERA_WAIT_SECONDS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| (5..=60).contains(value))
            .unwrap_or(30);
        let mut actions = AndroidReceiverActions::new();
        let active = actions
            .invoke(SYSTEM_CAMERA_ACTION_ID, QuickActionOperation::Start)
            .expect("start composed system-camera action");
        assert_eq!(active.status, "active");
        println!("CAPYIO_SYSTEM_CAMERA_WAITING_FOR_ANDROID seconds={wait_seconds}");
        std::io::stdout().flush().expect("flush start marker");

        let deadline = Instant::now() + Duration::from_secs(wait_seconds);
        let mut evidence = None;
        let mut consumer = None;
        while Instant::now() < deadline {
            if consumer.is_none() {
                consumer = CameraSharedIngressConsumer::open_current().ok();
            }
            if let Some(active_consumer) = consumer.as_mut()
                && let Ok(Some(frame)) = active_consumer.try_read_latest()
            {
                evidence = Some((
                    frame.descriptor.sequence,
                    frame.descriptor.source_timestamp_nanos,
                    frame.payload.len(),
                    frame
                        .payload
                        .iter()
                        .fold(0_u64, |sum, byte| sum + u64::from(*byte)),
                ));
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }

        let reconnect = std::env::var("CAPYIO_LIVE_CAMERA_RECONNECT").as_deref() == Ok("1");
        if reconnect && let Some(first) = evidence.as_ref() {
            println!("CAPYIO_SYSTEM_CAMERA_FIRST_FRAME sequence={}", first.0);
            std::io::stdout().flush().expect("flush first-frame marker");
            let gap_deadline = Instant::now() + Duration::from_secs(wait_seconds);
            let mut last_frame = Instant::now();
            while Instant::now() < gap_deadline {
                if let Some(active_consumer) = consumer.as_mut()
                    && let Ok(Some(_)) = active_consumer.try_read_latest()
                {
                    last_frame = Instant::now();
                }
                if last_frame.elapsed() >= Duration::from_millis(1_500) {
                    break;
                }
                thread::sleep(Duration::from_millis(50));
            }
            if last_frame.elapsed() >= Duration::from_millis(1_500) {
                println!("CAPYIO_SYSTEM_CAMERA_WAITING_FOR_RESTART");
                std::io::stdout().flush().expect("flush restart marker");
                let reconnect_deadline = Instant::now() + Duration::from_secs(wait_seconds);
                let first_sequence = first.0;
                while Instant::now() < reconnect_deadline {
                    if let Some(active_consumer) = consumer.as_mut()
                        && let Ok(Some(frame)) = active_consumer.try_read_latest()
                        && frame.descriptor.sequence > first_sequence
                    {
                        evidence = Some((
                            frame.descriptor.sequence,
                            frame.descriptor.source_timestamp_nanos,
                            frame.payload.len(),
                            frame
                                .payload
                                .iter()
                                .fold(0_u64, |sum, byte| sum + u64::from(*byte)),
                        ));
                        break;
                    }
                    thread::sleep(Duration::from_millis(50));
                }
            } else {
                evidence = None;
            }
        }

        let external_consumer =
            sibling_executable("capyio-camera-virtual-lab.exe").and_then(|executable| {
                Command::new(executable)
                    .arg("consumer-current-probe")
                    .output()
                    .map_err(|error| error.to_string())
            });
        let stopped = actions.invoke(SYSTEM_CAMERA_ACTION_ID, QuickActionOperation::Stop);
        let evidence = evidence.expect("no decoded phone frame reached the shared ingress mapping");
        let external_consumer = external_consumer.expect("launch independent camera consumer");
        let stopped = stopped.expect("stop composed system-camera action");
        assert_eq!(stopped.status, "idle");
        assert_eq!(evidence.2, 1_382_400);
        assert!(evidence.3 > 0);
        assert!(
            external_consumer.status.success(),
            "independent camera consumer failed: {}",
            String::from_utf8_lossy(&external_consumer.stderr)
        );
        let consumer_output = String::from_utf8_lossy(&external_consumer.stdout);
        assert!(consumer_output.contains("consumer_current_probe=pass"));
        assert!(consumer_output.contains("sample_bytes=1382400"));
        println!(
            "CAPYIO_SYSTEM_CAMERA_LIVE_FRAME sequence={} timestamp_ns={} bytes={} checksum={} external_consumer=pass reconnect={reconnect}",
            evidence.0, evidence.1, evidence.2, evidence.3,
        );
    }

    #[test]
    fn system_camera_global_mapping_denial_has_a_stable_problem_code() {
        let problem = receiver_exit_problem(
            ReceiverKind::SystemCamera,
            Some(1),
            "CAPYIO_AVC_LAB_ERROR Windows CreateFileMappingW failed with error 5",
        );
        assert_eq!(
            receiver_problem_code(ReceiverKind::SystemCamera, &problem),
            "CAPY.WINDOWS.SYSTEM_CAMERA_ELEVATION_REQUIRED"
        );
        assert!(problem.contains("管理员身份"));
    }

    #[test]
    fn system_camera_is_blocked_before_start_without_elevation() {
        let mut action = Receiver::new(ReceiverKind::SystemCamera).dto();
        apply_system_camera_permission(&mut action, Ok(false));
        assert_eq!(action.status, "blocked");
        assert!(action.available_operations.is_empty());
        assert_eq!(
            action.problem_code.as_deref(),
            Some("CAPY.WINDOWS.SYSTEM_CAMERA_ELEVATION_REQUIRED")
        );
    }
}
