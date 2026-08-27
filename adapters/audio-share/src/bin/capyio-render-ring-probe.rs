#[cfg(windows)]
fn main() -> std::process::ExitCode {
    use std::{env, fs, ptr};

    use windows_sys::Win32::{
        Foundation::{CloseHandle, GetLastError},
        System::Memory::{
            FILE_MAP_READ, FILE_MAP_WRITE, MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile,
            OpenFileMappingW, UnmapViewOfFile,
        },
    };

    let Some(result_path) = env::args_os().nth(1) else {
        eprintln!("usage: capyio-render-ring-probe <result-path>");
        return std::process::ExitCode::FAILURE;
    };
    let name = "Global\\CapyIO.RenderRing.v1"
        .encode_utf16()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let mapping = unsafe { OpenFileMappingW(FILE_MAP_READ | FILE_MAP_WRITE, 0, name.as_ptr()) };
    if mapping.is_null() {
        let code = unsafe { GetLastError() };
        let _ = fs::write(result_path, format!("open=false error={code}\n"));
        return std::process::ExitCode::FAILURE;
    }

    let view = unsafe { MapViewOfFile(mapping, FILE_MAP_READ | FILE_MAP_WRITE, 0, 0, 128) };
    if view.Value.is_null() {
        let code = unsafe { GetLastError() };
        unsafe { CloseHandle(mapping) };
        let _ = fs::write(result_path, format!("open=true map=false error={code}\n"));
        return std::process::ExitCode::FAILURE;
    }

    let magic = unsafe { ptr::read_unaligned(view.Value.cast::<u32>()) };
    unsafe {
        UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS { Value: view.Value });
        CloseHandle(mapping);
    }
    let valid = magic == 0x524f_4950;
    let _ = fs::write(
        result_path,
        format!("open=true map=true magic=0x{magic:08x} valid={valid}\n"),
    );
    if valid {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}

#[cfg(not(windows))]
fn main() -> std::process::ExitCode {
    eprintln!("capyio-render-ring-probe is supported only on Windows");
    std::process::ExitCode::FAILURE
}
