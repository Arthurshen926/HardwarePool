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

    let base = view.Value.cast::<u8>();
    let magic = unsafe { ptr::read_unaligned(base.cast::<u32>()) };
    let read_i64 = |offset: usize| unsafe { ptr::read_unaligned(base.add(offset).cast::<i64>()) };
    let read_u32 = |offset: usize| unsafe { ptr::read_unaligned(base.add(offset).cast::<u32>()) };
    let produced = read_i64(64).max(0) as u64;
    let dropped = read_i64(56).max(0) as u64;
    let attach_attempts = read_i64(72).max(0) as u64;
    let attach_successes = read_i64(80).max(0) as u64;
    let attach_sample_rate = read_u32(88);
    let attach_channels = read_u32(92);
    let attach_stage = read_u32(96);
    let attach_error = read_u32(100);
    unsafe {
        UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS { Value: view.Value });
        CloseHandle(mapping);
    }
    let valid = magic == 0x524f_4950;
    let _ = fs::write(
        result_path,
        format!(
            "open=true map=true magic=0x{magic:08x} valid={valid} produced={produced} dropped={dropped} attach_attempts={attach_attempts} attach_successes={attach_successes} attach_sample_rate={attach_sample_rate} attach_channels={attach_channels} attach_stage={attach_stage} attach_error={attach_error}\n"
        ),
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
