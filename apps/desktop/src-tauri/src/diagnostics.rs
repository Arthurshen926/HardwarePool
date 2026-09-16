use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Diagnostics {
    backend: &'static str,
    version: &'static str,
    revision: &'static str,
    built_at: &'static str,
    checks: Vec<Check>,
}

#[derive(Serialize)]
pub(crate) struct Check {
    label: &'static str,
    detail: String,
}

#[tauri::command]
pub(crate) fn get_diagnostics() -> Diagnostics {
    let mut checks = Vec::new();
    for (label, result) in [
        ("ADB 程序", crate::android_receivers::adb_path()),
        (
            "系统相机会话宿主",
            crate::android_receivers::sibling_executable(crate::camera_session::EXECUTABLE),
        ),
        (
            "触控板接收器",
            crate::android_receivers::sibling_executable("capyio-ptp-adb-lab.exe"),
        ),
        (
            "摄像头接收器",
            crate::android_receivers::sibling_executable("capyio-avc-lab-receiver.exe"),
        ),
    ] {
        checks.push(Check {
            label,
            detail: result
                .map(|_| "文件存在；尚未验证设备连接或运行权限".to_owned())
                .unwrap_or_else(|error| error),
        });
    }
    checks.push(Check {
        label: "验收范围",
        detail: "本页仅检查程序文件。音频服务、驱动权限、手机连接和系统摄像头投影尚需单独验证。"
            .to_owned(),
    });
    Diagnostics {
        backend: "Windows Tauri",
        version: env!("CARGO_PKG_VERSION"),
        revision: env!("CAPYIO_BUILD_REVISION"),
        built_at: env!("CAPYIO_BUILD_TIME"),
        checks,
    }
}
