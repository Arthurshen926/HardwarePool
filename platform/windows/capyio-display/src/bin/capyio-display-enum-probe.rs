#[cfg(not(windows))]
fn main() {
    eprintln!("capyio-display-enum-probe is only available on Windows");
    std::process::exit(2);
}

#[cfg(windows)]
fn main() {
    use std::mem::size_of;
    use windows::Win32::{
        Foundation::{LPARAM, RECT},
        Graphics::Gdi::{
            DISPLAY_DEVICEW, EnumDisplayDevicesW, EnumDisplayMonitors, GetMonitorInfoW, HDC,
            HMONITOR, MONITORINFO, MONITORINFOEXW,
        },
    };
    use windows::core::PCWSTR;

    unsafe extern "system" fn enumerate_monitor(
        monitor: HMONITOR,
        _device_context: HDC,
        rectangle: *mut RECT,
        _context: LPARAM,
    ) -> windows::core::BOOL {
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
        let obtained = unsafe {
            GetMonitorInfoW(
                monitor,
                &mut info as *mut MONITORINFOEXW as *mut MONITORINFO,
            )
        };
        let rect = if rectangle.is_null() {
            RECT::default()
        } else {
            unsafe { *rectangle }
        };
        println!(
            "HMONITOR handle={:?} name={:?} rect={},{},{},{} info_ok={}",
            monitor,
            wide(&info.szDevice),
            rect.left,
            rect.top,
            rect.right,
            rect.bottom,
            obtained.as_bool(),
        );
        true.into()
    }

    let enumerated =
        unsafe { EnumDisplayMonitors(None, None, Some(enumerate_monitor), LPARAM::default()) };
    println!("EnumDisplayMonitors ok={}", enumerated.as_bool());

    match capyio_windows_display::inspect_windows_display_topology() {
        Ok(paths) => {
            for path in paths.into_iter().filter(|path| path.active) {
                println!(
                    "DISPLAYCONFIG name={:?} adapter={:?} adapter_id={:?} monitor={:?} monitor_path={:?} technology={} category={:?} active={} available={} source={} target={}",
                    path.gdi_device_name,
                    path.adapter_description,
                    path.adapter_device_id,
                    path.monitor_friendly_name,
                    path.monitor_device_path,
                    path.output_technology,
                    path.category,
                    path.active,
                    path.target_available,
                    path.source_id,
                    path.target_id,
                );
            }
        }
        Err(error) => eprintln!("DISPLAYCONFIG inventory error: {error}"),
    }

    match capyio_windows_display::preview_physical_display_suppression() {
        Ok(summary) => {
            println!(
                "PHYSICAL_SUPPRESSION_PREVIEW suppressed={} retained={}",
                summary.suppressed.len(),
                summary.retained.len()
            );
            for path in summary.suppressed {
                println!(
                    "PHYSICAL_SUPPRESSION_CANDIDATE name={:?} adapter={:?} monitor={:?}",
                    path.gdi_device_name, path.adapter_description, path.monitor_friendly_name
                );
            }
        }
        Err(error) => eprintln!("PHYSICAL_SUPPRESSION_PREVIEW unavailable: {error}"),
    }

    for index in 0..256 {
        let mut device = DISPLAY_DEVICEW {
            cb: size_of::<DISPLAY_DEVICEW>() as u32,
            ..Default::default()
        };
        if !unsafe { EnumDisplayDevicesW(PCWSTR::null(), index, &mut device, 0) }.as_bool() {
            break;
        }
        println!(
            "DISPLAY_DEVICE index={} name={:?} string={:?} id={:?} key={:?}",
            index,
            wide(&device.DeviceName),
            wide(&device.DeviceString),
            wide(&device.DeviceID),
            wide(&device.DeviceKey)
        );
    }
}

#[cfg(windows)]
fn wide(value: &[u16]) -> String {
    let length = value
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(value.len());
    String::from_utf16_lossy(&value[..length])
}
