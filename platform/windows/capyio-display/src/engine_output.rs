use thiserror::Error;
use windows::Win32::{
    Devices::Display::{
        DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE, DISPLAYCONFIG_PATH_INFO,
        GetDisplayConfigBufferSizes, QDC_ALL_PATHS, QueryDisplayConfig, SDC_ALLOW_CHANGES,
        SDC_APPLY, SDC_SAVE_TO_DATABASE, SDC_USE_SUPPLIED_DISPLAY_CONFIG, SDC_VALIDATE,
        SetDisplayConfig,
    },
    Foundation::{LPARAM, RECT},
    Graphics::Gdi::{
        DISPLAYCONFIG_PATH_ACTIVE, DISPLAYCONFIG_PATH_MODE_IDX_INVALID, EnumDisplayMonitors,
        GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW,
    },
    UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext},
};
use windows::core::BOOL;

use crate::{DisplayTopologyError, IddConnectedMonitor, IddMonitorIdentity, IddMonitorMode};

const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const SNAPSHOT_ATTEMPTS: usize = 4;

/// Fixes display coordinates before this process creates or validates an IDD
/// monitor. Mature capture engines opt into per-monitor DPI awareness; doing
/// that only after topology activation can invalidate a monitor that was
/// positioned using DPI-virtualized GDI coordinates.
pub fn configure_process_display_dpi_awareness() -> Result<(), DisplayOutputBindingError> {
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) }.map_err(
        |error| DisplayOutputBindingError::Win32 {
            operation: "set per-monitor-v2 display DPI awareness",
            status: error.code().0,
        },
    )
}

/// An ephemeral Windows capture selector proven to belong to one CapyIO IDD
/// monitor. Its fields are private so callers cannot construct it from UI or
/// network input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedWindowsDisplayOutput {
    gdi_device_name: String,
    identity: IddMonitorIdentity,
    mode: IddMonitorMode,
}

impl VerifiedWindowsDisplayOutput {
    #[must_use]
    pub fn gdi_device_name(&self) -> &str {
        &self.gdi_device_name
    }

    #[must_use]
    pub const fn identity(&self) -> IddMonitorIdentity {
        self.identity
    }

    #[must_use]
    pub const fn mode(&self) -> IddMonitorMode {
        self.mode
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum DisplayOutputBindingError {
    #[error("the requested IDD display mode is invalid")]
    InvalidMode,
    #[error("expected exactly one CapyIO static display adapter output, found {count}")]
    StaticAdapterOutputCount { count: usize },
    #[error("Windows display inventory operation {operation} failed with status {status}")]
    Win32 {
        operation: &'static str,
        status: i32,
    },
    #[error("no Windows display path belongs to the requested CapyIO IDD monitor")]
    MissingOwnedPath,
    #[error("more than one Windows display path claims the requested CapyIO IDD monitor")]
    AmbiguousOwnedPath,
    #[error("the Windows display path owned by CapyIO is not active")]
    InactiveOwnedPath,
    #[error("the Windows display target owned by CapyIO is unavailable")]
    UnavailableOwnedTarget,
    #[error("the owned Windows display source name is not a canonical \\\\.\\DISPLAYn selector")]
    InvalidOutputName,
    #[error("the active owned display mode does not equal the requested IDD mode")]
    ModeMismatch,
    #[error("the active Windows display path has an invalid source mode record")]
    InvalidSourceMode,
    #[error("the activated owned display path did not become a desktop monitor")]
    DesktopMonitorUnavailable,
    #[error("the owned Windows display binding changed during verification")]
    UnstableBinding,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DisplayPathSnapshot {
    target_adapter_luid_low: u32,
    target_adapter_luid_high: i32,
    target_id: u32,
    gdi_device_name: String,
    active: bool,
    target_available: bool,
    mode: Option<IddMonitorMode>,
}

/// Resolves the exact active Windows output owned by `monitor` and verifies it
/// twice. This must run immediately before engine start because DISPLAY names
/// are topology-scoped and can change after arrival/departure.
pub fn verify_owned_windows_display_output(
    monitor: IddConnectedMonitor,
    expected_mode: IddMonitorMode,
) -> Result<VerifiedWindowsDisplayOutput, DisplayOutputBindingError> {
    expected_mode
        .validate()
        .map_err(|_| DisplayOutputBindingError::InvalidMode)?;
    let first = resolve_snapshot(monitor, expected_mode, &query_path_snapshots(monitor)?)?;
    if !desktop_monitor_is_independent(first.gdi_device_name()) {
        return Err(DisplayOutputBindingError::DesktopMonitorUnavailable);
    }
    let second = resolve_snapshot(monitor, expected_mode, &query_path_snapshots(monitor)?)?;
    if !desktop_monitor_is_independent(second.gdi_device_name()) {
        return Err(DisplayOutputBindingError::DesktopMonitorUnavailable);
    }
    if first != second {
        return Err(DisplayOutputBindingError::UnstableBinding);
    }
    Ok(first)
}

/// Adds only the exact available monitor returned by the SudoVDA add request
/// to the current extended desktop. Windows persists topology choices by
/// monitor identity, so reconnecting a previously disabled Capability GUID can
/// otherwise leave a successfully-created driver target inactive.
pub fn activate_owned_windows_display_output(
    monitor: IddConnectedMonitor,
) -> Result<(), DisplayOutputBindingError> {
    let paths = super::display_config::query_all_paths().map_err(map_topology_error)?;
    let mut owned = paths
        .iter()
        .enumerate()
        .filter(|(_, path)| {
            path.targetInfo.adapterId.LowPart == monitor.adapter_luid_low
                && path.targetInfo.adapterId.HighPart == monitor.adapter_luid_high
                && path.targetInfo.id == monitor.target_id
        })
        .collect::<Vec<_>>();
    if owned.is_empty() {
        return Err(DisplayOutputBindingError::MissingOwnedPath);
    }
    owned.retain(|(_, path)| path.targetInfo.targetAvailable.as_bool());
    if owned.is_empty() {
        return Err(DisplayOutputBindingError::UnavailableOwnedTarget);
    }
    let owned_desktop_monitor_present = owned.iter().any(|(_, path)| {
        path.flags & DISPLAYCONFIG_PATH_ACTIVE != 0
            && super::display_config::source_gdi_name(path)
                .is_ok_and(|name| desktop_monitor_is_independent(&name))
    });
    if owned_desktop_monitor_present {
        return Ok(());
    }

    // QDC_ALL_PATHS may expose several inactive source-path alternatives for
    // one exact target. They do not represent multiple monitors. Ask CCD to
    // validate each exact-target alternative and apply only the one Windows
    // accepts alongside the currently active topology.
    let active = paths
        .iter()
        .filter(|path| {
            path.flags & DISPLAYCONFIG_PATH_ACTIVE != 0
                && !(path.targetInfo.adapterId.LowPart == monitor.adapter_luid_low
                    && path.targetInfo.adapterId.HighPart == monitor.adapter_luid_high
                    && path.targetInfo.id == monitor.target_id)
        })
        .copied()
        .collect::<Vec<_>>();
    let mut last_validation_status = 0;
    for (_, candidate) in owned {
        let mut selected = active.clone();
        selected.push(*candidate);
        for path in &mut selected {
            path.flags |= DISPLAYCONFIG_PATH_ACTIVE;
            path.sourceInfo.Anonymous.modeInfoIdx = DISPLAYCONFIG_PATH_MODE_IDX_INVALID;
            path.targetInfo.Anonymous.modeInfoIdx = DISPLAYCONFIG_PATH_MODE_IDX_INVALID;
        }
        let validate = unsafe {
            SetDisplayConfig(
                Some(&selected),
                None,
                SDC_VALIDATE | SDC_USE_SUPPLIED_DISPLAY_CONFIG | SDC_ALLOW_CHANGES,
            )
        };
        if validate != 0 {
            last_validation_status = validate;
            continue;
        }
        let apply = unsafe {
            SetDisplayConfig(
                Some(&selected),
                None,
                SDC_APPLY
                    | SDC_USE_SUPPLIED_DISPLAY_CONFIG
                    | SDC_ALLOW_CHANGES
                    | SDC_SAVE_TO_DATABASE,
            )
        };
        if apply != 0 {
            return Err(DisplayOutputBindingError::Win32 {
                operation: "activate exact SudoVDA extended topology",
                status: apply,
            });
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1500);
        let mut position_staged = false;
        loop {
            let active_name = query_path_snapshots(monitor)?
                .into_iter()
                .find(|path| path.active && path.target_available)
                .map(|path| path.gdi_device_name);
            if let Some(active_name) = active_name.as_deref() {
                if !position_staged {
                    ensure_independent_desktop_position(active_name, monitor)?;
                    position_staged = true;
                }
                if desktop_monitor_is_independent(active_name) {
                    return Ok(());
                }
            }
            if std::time::Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
    if last_validation_status == 0 {
        Err(DisplayOutputBindingError::DesktopMonitorUnavailable)
    } else {
        Err(DisplayOutputBindingError::Win32 {
            operation: "validate exact SudoVDA extended topology alternatives",
            status: last_validation_status,
        })
    }
}

#[derive(Clone, Copy)]
struct DesktopRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl DesktopRect {
    fn overlaps(self, other: Self) -> bool {
        self.left < other.right
            && other.left < self.right
            && self.top < other.bottom
            && other.top < self.bottom
    }
}

struct DesktopMonitorInventory {
    expected: Option<DesktopRect>,
    others: Vec<DesktopRect>,
}

fn desktop_monitor_inventory(expected_name: &str) -> DesktopMonitorInventory {
    struct State<'a> {
        expected_name: &'a str,
        expected: Option<DesktopRect>,
        others: Vec<DesktopRect>,
    }

    unsafe extern "system" fn enumerate(
        monitor: HMONITOR,
        _device_context: HDC,
        _rect: *mut RECT,
        data: LPARAM,
    ) -> BOOL {
        // SAFETY: `data` points to the stack-owned State for the synchronous
        // EnumDisplayMonitors call below.
        let state = unsafe { &mut *(data.0 as *mut State<'_>) };
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        // SAFETY: `info` has the documented size and remains live for the call.
        let obtained = unsafe {
            GetMonitorInfoW(
                monitor,
                &mut info as *mut MONITORINFOEXW as *mut MONITORINFO,
            )
        };
        if obtained.as_bool() {
            let end = info
                .szDevice
                .iter()
                .position(|value| *value == 0)
                .unwrap_or(info.szDevice.len());
            let name = String::from_utf16_lossy(&info.szDevice[..end]);
            let monitor = info.monitorInfo.rcMonitor;
            let rect = DesktopRect {
                left: monitor.left,
                top: monitor.top,
                right: monitor.right,
                bottom: monitor.bottom,
            };
            if name.eq_ignore_ascii_case(state.expected_name) {
                state.expected = Some(rect);
            } else {
                state.others.push(rect);
            }
        }
        true.into()
    }

    let mut state = State {
        expected_name,
        expected: None,
        others: Vec::new(),
    };
    // SAFETY: callback state is valid for this synchronous enumeration.
    let _ = unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(enumerate),
            LPARAM((&raw mut state).cast::<()>() as isize),
        )
    };
    DesktopMonitorInventory {
        expected: state.expected,
        others: state.others,
    }
}

fn desktop_monitor_is_independent(expected_name: &str) -> bool {
    let inventory = desktop_monitor_inventory(expected_name);
    inventory.expected.is_some_and(|expected| {
        inventory
            .others
            .iter()
            .all(|other| !expected.overlaps(*other))
    })
}

fn ensure_independent_desktop_position(
    expected_name: &str,
    monitor: IddConnectedMonitor,
) -> Result<(), DisplayOutputBindingError> {
    let inventory = desktop_monitor_inventory(expected_name);
    if inventory.expected.is_some_and(|expected| {
        inventory
            .others
            .iter()
            .all(|other| !expected.overlaps(*other))
    }) {
        return Ok(());
    }
    // EnumDisplayMonitors coordinates are DPI-context dependent, while
    // ChangeDisplaySettingsExW always consumes physical desktop pixels. Using
    // a GDI rectangle from a DPI-unaware worker can therefore place a new
    // 2640-pixel output halfway across a 2880-pixel high-DPI desktop. The
    // caller then sees a process-local non-overlapping HMONITOR even though a
    // fresh process never receives the monitor. CCD source modes are already
    // expressed in the physical desktop coordinate space used by the setter.
    let (right, top) = ccd_position_after_other_outputs(monitor)?;
    super::display_config::position_display_output(expected_name, right, top)
        .map_err(map_topology_error)
}

fn ccd_position_after_other_outputs(
    monitor: IddConnectedMonitor,
) -> Result<(i32, i32), DisplayOutputBindingError> {
    let (paths, modes) = query_display_inventory()?;
    let mut rectangles = Vec::new();
    for path in paths.iter().filter(|path| {
        path.flags & DISPLAYCONFIG_PATH_ACTIVE != 0
            && !(path.targetInfo.adapterId.LowPart == monitor.adapter_luid_low
                && path.targetInfo.adapterId.HighPart == monitor.adapter_luid_high
                && path.targetInfo.id == monitor.target_id)
    }) {
        rectangles.push(source_desktop_rect(path, &modes)?);
    }
    Ok(position_after_rectangles(&rectangles))
}

fn position_after_rectangles(rectangles: &[DesktopRect]) -> (i32, i32) {
    (
        rectangles.iter().map(|rect| rect.right).max().unwrap_or(0),
        rectangles.iter().map(|rect| rect.top).min().unwrap_or(0),
    )
}

fn source_desktop_rect(
    path: &DISPLAYCONFIG_PATH_INFO,
    modes: &[DISPLAYCONFIG_MODE_INFO],
) -> Result<DesktopRect, DisplayOutputBindingError> {
    // SAFETY: QDC_ALL_PATHS without virtual-mode awareness defines this union
    // member as modeInfoIdx.
    let mode_index = unsafe { path.sourceInfo.Anonymous.modeInfoIdx };
    if mode_index == DISPLAYCONFIG_PATH_MODE_IDX_INVALID {
        return Err(DisplayOutputBindingError::InvalidSourceMode);
    }
    let mode = modes
        .get(mode_index as usize)
        .ok_or(DisplayOutputBindingError::InvalidSourceMode)?;
    if mode.infoType != DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE
        || mode.id != path.sourceInfo.id
        || mode.adapterId.LowPart != path.sourceInfo.adapterId.LowPart
        || mode.adapterId.HighPart != path.sourceInfo.adapterId.HighPart
    {
        return Err(DisplayOutputBindingError::InvalidSourceMode);
    }
    // SAFETY: infoType above proves the sourceMode union member is active.
    let source = unsafe { mode.Anonymous.sourceMode };
    let width =
        i32::try_from(source.width).map_err(|_| DisplayOutputBindingError::InvalidSourceMode)?;
    let height =
        i32::try_from(source.height).map_err(|_| DisplayOutputBindingError::InvalidSourceMode)?;
    Ok(DesktopRect {
        left: source.position.x,
        top: source.position.y,
        right: source
            .position
            .x
            .checked_add(width)
            .ok_or(DisplayOutputBindingError::InvalidSourceMode)?,
        bottom: source
            .position
            .y
            .checked_add(height)
            .ok_or(DisplayOutputBindingError::InvalidSourceMode)?,
    })
}

/// Resolves the one installed static CapyIO 0.1.1 output and binds the
/// service-authorized single Sink identity to it. Unlike the dynamic path,
/// this driver has no per-monitor control inventory; exclusivity comes from
/// the service's one-route/one-mapping lease plus the exact driver identity.
pub fn verify_static_capyio_display_output(
    identity: IddMonitorIdentity,
    expected_mode: IddMonitorMode,
) -> Result<VerifiedWindowsDisplayOutput, DisplayOutputBindingError> {
    expected_mode
        .validate()
        .map_err(|_| DisplayOutputBindingError::InvalidMode)?;
    let first = resolve_static_output(identity, expected_mode)?;
    let second = resolve_static_output(identity, expected_mode)?;
    if first != second {
        return Err(DisplayOutputBindingError::UnstableBinding);
    }
    Ok(first)
}

fn resolve_static_output(
    identity: IddMonitorIdentity,
    expected_mode: IddMonitorMode,
) -> Result<VerifiedWindowsDisplayOutput, DisplayOutputBindingError> {
    let expected_name =
        super::display_config::find_capyio_gdi_device_name().map_err(map_topology_error)?;
    let (paths, modes) = query_display_inventory()?;
    let snapshots = paths
        .iter()
        .filter_map(|path| {
            let name = match super::display_config::source_gdi_name(path) {
                Ok(name) if name.eq_ignore_ascii_case(&expected_name) => name,
                Ok(_) => return None,
                Err(error) => return Some(Err(map_topology_error(error))),
            };
            let active = path.flags & DISPLAYCONFIG_PATH_ACTIVE != 0;
            let target_available = path.targetInfo.targetAvailable.as_bool();
            let mode = if active && target_available {
                match path_mode(path, &modes) {
                    Ok(mode) => Some(mode),
                    Err(error) => return Some(Err(error)),
                }
            } else {
                None
            };
            Some(Ok(DisplayPathSnapshot {
                target_adapter_luid_low: path.targetInfo.adapterId.LowPart,
                target_adapter_luid_high: path.targetInfo.adapterId.HighPart,
                target_id: path.targetInfo.id,
                gdi_device_name: name,
                active,
                target_available,
                mode,
            }))
        })
        .collect::<Result<Vec<_>, _>>()?;
    resolve_static_snapshot(identity, expected_mode, &expected_name, &snapshots)
}

fn resolve_static_snapshot(
    identity: IddMonitorIdentity,
    expected_mode: IddMonitorMode,
    expected_name: &str,
    paths: &[DisplayPathSnapshot],
) -> Result<VerifiedWindowsDisplayOutput, DisplayOutputBindingError> {
    let mut matches = paths
        .iter()
        .filter(|path| path.gdi_device_name.eq_ignore_ascii_case(expected_name));
    let path = matches
        .next()
        .ok_or(DisplayOutputBindingError::MissingOwnedPath)?;
    if matches.next().is_some() {
        return Err(DisplayOutputBindingError::AmbiguousOwnedPath);
    }
    if !path.target_available {
        return Err(DisplayOutputBindingError::UnavailableOwnedTarget);
    }
    if !path.active {
        return Err(DisplayOutputBindingError::InactiveOwnedPath);
    }
    if !is_canonical_output_name(&path.gdi_device_name) {
        return Err(DisplayOutputBindingError::InvalidOutputName);
    }
    if path.mode != Some(expected_mode) {
        return Err(DisplayOutputBindingError::ModeMismatch);
    }
    Ok(VerifiedWindowsDisplayOutput {
        gdi_device_name: path.gdi_device_name.clone(),
        identity,
        mode: expected_mode,
    })
}

fn resolve_snapshot(
    monitor: IddConnectedMonitor,
    expected_mode: IddMonitorMode,
    paths: &[DisplayPathSnapshot],
) -> Result<VerifiedWindowsDisplayOutput, DisplayOutputBindingError> {
    let matches = paths
        .iter()
        .filter(|path| {
            path.target_adapter_luid_low == monitor.adapter_luid_low
                && path.target_adapter_luid_high == monitor.adapter_luid_high
                && path.target_id == monitor.target_id
        })
        .collect::<Vec<_>>();
    if matches.is_empty() {
        return Err(DisplayOutputBindingError::MissingOwnedPath);
    }
    let available = matches
        .iter()
        .copied()
        .filter(|path| path.target_available)
        .collect::<Vec<_>>();
    if available.is_empty() {
        return Err(DisplayOutputBindingError::UnavailableOwnedTarget);
    }
    let active = available
        .iter()
        .copied()
        .filter(|path| path.active)
        .collect::<Vec<_>>();
    if active.is_empty() {
        return Err(DisplayOutputBindingError::InactiveOwnedPath);
    }
    if active.len() != 1 {
        return Err(DisplayOutputBindingError::AmbiguousOwnedPath);
    }
    let path = active[0];
    if !is_canonical_output_name(&path.gdi_device_name) {
        return Err(DisplayOutputBindingError::InvalidOutputName);
    }
    if path.mode != Some(expected_mode) {
        return Err(DisplayOutputBindingError::ModeMismatch);
    }
    Ok(VerifiedWindowsDisplayOutput {
        gdi_device_name: path.gdi_device_name.clone(),
        identity: monitor.identity,
        mode: expected_mode,
    })
}

fn query_path_snapshots(
    monitor: IddConnectedMonitor,
) -> Result<Vec<DisplayPathSnapshot>, DisplayOutputBindingError> {
    let (paths, modes) = query_display_inventory()?;
    paths
        .iter()
        .filter(|path| {
            path.targetInfo.adapterId.LowPart == monitor.adapter_luid_low
                && path.targetInfo.adapterId.HighPart == monitor.adapter_luid_high
                && path.targetInfo.id == monitor.target_id
        })
        .map(|path| {
            let active = path.flags & DISPLAYCONFIG_PATH_ACTIVE != 0;
            let target_available = path.targetInfo.targetAvailable.as_bool();
            let (gdi_device_name, mode) = if active && target_available {
                let name =
                    super::display_config::source_gdi_name(path).map_err(map_topology_error)?;
                (name, Some(path_mode(path, &modes)?))
            } else {
                (String::new(), None)
            };
            Ok(DisplayPathSnapshot {
                target_adapter_luid_low: path.targetInfo.adapterId.LowPart,
                target_adapter_luid_high: path.targetInfo.adapterId.HighPart,
                target_id: path.targetInfo.id,
                gdi_device_name,
                active,
                target_available,
                mode,
            })
        })
        .collect()
}

fn query_display_inventory()
-> Result<(Vec<DISPLAYCONFIG_PATH_INFO>, Vec<DISPLAYCONFIG_MODE_INFO>), DisplayOutputBindingError> {
    for _ in 0..SNAPSHOT_ATTEMPTS {
        let mut path_count = 0;
        let mut mode_count = 0;
        let status =
            unsafe { GetDisplayConfigBufferSizes(QDC_ALL_PATHS, &mut path_count, &mut mode_count) };
        if status.0 != 0 {
            return Err(DisplayOutputBindingError::Win32 {
                operation: "size display binding inventory",
                status: status.0 as i32,
            });
        }
        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
        let mut modes = vec![Default::default(); mode_count as usize];
        let status = unsafe {
            QueryDisplayConfig(
                QDC_ALL_PATHS,
                &mut path_count,
                paths.as_mut_ptr(),
                &mut mode_count,
                modes.as_mut_ptr(),
                None,
            )
        };
        if status.0 == ERROR_INSUFFICIENT_BUFFER {
            continue;
        }
        if status.0 != 0 {
            return Err(DisplayOutputBindingError::Win32 {
                operation: "query display binding inventory",
                status: status.0 as i32,
            });
        }
        paths.truncate(path_count as usize);
        modes.truncate(mode_count as usize);
        return Ok((paths, modes));
    }
    Err(DisplayOutputBindingError::UnstableBinding)
}

fn map_topology_error(error: DisplayTopologyError) -> DisplayOutputBindingError {
    match error {
        DisplayTopologyError::AdapterOutputCount { count } => {
            DisplayOutputBindingError::StaticAdapterOutputCount { count }
        }
        DisplayTopologyError::Win32 { operation, status } => {
            DisplayOutputBindingError::Win32 { operation, status }
        }
        _ => DisplayOutputBindingError::UnstableBinding,
    }
}

fn path_mode(
    path: &DISPLAYCONFIG_PATH_INFO,
    modes: &[DISPLAYCONFIG_MODE_INFO],
) -> Result<IddMonitorMode, DisplayOutputBindingError> {
    // SAFETY: QDC_ALL_PATHS without virtual-mode awareness defines this union
    // member as modeInfoIdx.
    let mode_index = unsafe { path.sourceInfo.Anonymous.modeInfoIdx };
    if mode_index == DISPLAYCONFIG_PATH_MODE_IDX_INVALID {
        return Err(DisplayOutputBindingError::InvalidSourceMode);
    }
    let mode = modes
        .get(mode_index as usize)
        .ok_or(DisplayOutputBindingError::InvalidSourceMode)?;
    if mode.infoType != DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE
        || mode.id != path.sourceInfo.id
        || mode.adapterId.LowPart != path.sourceInfo.adapterId.LowPart
        || mode.adapterId.HighPart != path.sourceInfo.adapterId.HighPart
    {
        return Err(DisplayOutputBindingError::InvalidSourceMode);
    }
    let rate = path.targetInfo.refreshRate;
    let refresh_millihz = rational_refresh_millihz(rate.Numerator, rate.Denominator)
        .ok_or(DisplayOutputBindingError::InvalidSourceMode)?;
    // SAFETY: infoType above proves the sourceMode union member is active.
    let source = unsafe { mode.Anonymous.sourceMode };
    Ok(IddMonitorMode {
        width: source.width,
        height: source.height,
        refresh_millihz,
    })
}

fn rational_refresh_millihz(numerator: u32, denominator: u32) -> Option<u32> {
    let denominator = u64::from(denominator);
    if denominator == 0 {
        return None;
    }
    let scaled = u64::from(numerator).checked_mul(1000)?;
    let rounded = scaled.checked_add(denominator / 2)? / denominator;
    u32::try_from(rounded).ok()
}

fn is_canonical_output_name(value: &str) -> bool {
    let Some(number) = value.strip_prefix(r"\\.\DISPLAY") else {
        return false;
    };
    !number.is_empty()
        && number.bytes().all(|byte| byte.is_ascii_digit())
        && !number.starts_with('0')
}

#[cfg(test)]
mod tests {
    use capyio_core::ids::CapabilityId;

    use super::*;

    fn identity() -> IddMonitorIdentity {
        IddMonitorIdentity::new(
            "10112233-4455-6677-8899-aabbccddeeff"
                .parse::<CapabilityId>()
                .unwrap(),
        )
        .unwrap()
    }

    fn monitor() -> IddConnectedMonitor {
        IddConnectedMonitor {
            identity: identity(),
            connector_index: 1,
            target_id: 9,
            adapter_luid_low: 42,
            adapter_luid_high: -7,
        }
    }

    fn mode() -> IddMonitorMode {
        IddMonitorMode {
            width: 2640,
            height: 1216,
            refresh_millihz: 120_000,
        }
    }

    fn path() -> DisplayPathSnapshot {
        DisplayPathSnapshot {
            target_adapter_luid_low: 42,
            target_adapter_luid_high: -7,
            target_id: 9,
            gdi_device_name: r"\\.\DISPLAY2".to_owned(),
            active: true,
            target_available: true,
            mode: Some(mode()),
        }
    }

    #[test]
    fn exact_luid_target_mode_and_output_produce_owned_binding() {
        let mut unrelated_broken_path = path();
        unrelated_broken_path.target_adapter_luid_low = 99;
        unrelated_broken_path.gdi_device_name = "foreign-invalid-name".to_owned();
        unrelated_broken_path.mode = None;
        let binding =
            resolve_snapshot(monitor(), mode(), &[unrelated_broken_path, path()]).unwrap();
        assert_eq!(binding.gdi_device_name(), r"\\.\DISPLAY2");
        assert_eq!(binding.identity(), identity());
        assert_eq!(binding.mode(), mode());
    }

    #[test]
    fn static_output_binds_only_the_exact_driver_name_and_authorized_identity() {
        let mut foreign = path();
        foreign.gdi_device_name = r"\\.\DISPLAY7".to_owned();
        let binding =
            resolve_static_snapshot(identity(), mode(), r"\\.\DISPLAY2", &[foreign, path()])
                .unwrap();
        assert_eq!(binding.gdi_device_name(), r"\\.\DISPLAY2");
        assert_eq!(binding.identity(), identity());
        assert_eq!(binding.mode(), mode());
    }

    #[test]
    fn static_output_rejects_missing_duplicate_inactive_and_changed_mode() {
        assert_eq!(
            resolve_static_snapshot(identity(), mode(), r"\\.\DISPLAY3", &[path()]),
            Err(DisplayOutputBindingError::MissingOwnedPath)
        );
        assert_eq!(
            resolve_static_snapshot(identity(), mode(), r"\\.\DISPLAY2", &[path(), path()]),
            Err(DisplayOutputBindingError::AmbiguousOwnedPath)
        );
        let mut inactive = path();
        inactive.active = false;
        inactive.mode = None;
        assert_eq!(
            resolve_static_snapshot(identity(), mode(), r"\\.\DISPLAY2", &[inactive]),
            Err(DisplayOutputBindingError::InactiveOwnedPath)
        );
        let mut changed = path();
        changed.mode.as_mut().unwrap().refresh_millihz = 60_000;
        assert_eq!(
            resolve_static_snapshot(identity(), mode(), r"\\.\DISPLAY2", &[changed]),
            Err(DisplayOutputBindingError::ModeMismatch)
        );
    }

    #[test]
    fn static_adapter_inventory_failure_keeps_its_exact_count() {
        assert_eq!(
            map_topology_error(DisplayTopologyError::AdapterOutputCount { count: 0 }),
            DisplayOutputBindingError::StaticAdapterOutputCount { count: 0 }
        );
        assert_eq!(
            map_topology_error(DisplayTopologyError::AdapterOutputCount { count: 2 }),
            DisplayOutputBindingError::StaticAdapterOutputCount { count: 2 }
        );
    }

    #[test]
    fn same_target_on_a_different_adapter_is_not_owned() {
        let mut foreign = path();
        foreign.target_adapter_luid_low = 41;
        assert_eq!(
            resolve_snapshot(monitor(), mode(), &[foreign]),
            Err(DisplayOutputBindingError::MissingOwnedPath)
        );
    }

    #[test]
    fn duplicate_inactive_unavailable_and_mode_mismatch_fail_closed() {
        assert_eq!(
            resolve_snapshot(monitor(), mode(), &[path(), path()]),
            Err(DisplayOutputBindingError::AmbiguousOwnedPath)
        );
        let mut historical = path();
        historical.active = false;
        historical.mode = None;
        let binding = resolve_snapshot(monitor(), mode(), &[historical, path()]).unwrap();
        assert_eq!(binding.gdi_device_name(), r"\\.\DISPLAY2");
        let mut inactive = path();
        inactive.active = false;
        inactive.mode = None;
        assert_eq!(
            resolve_snapshot(monitor(), mode(), &[inactive]),
            Err(DisplayOutputBindingError::InactiveOwnedPath)
        );
        let mut unavailable = path();
        unavailable.target_available = false;
        assert_eq!(
            resolve_snapshot(monitor(), mode(), &[unavailable]),
            Err(DisplayOutputBindingError::UnavailableOwnedTarget)
        );
        let mut wrong_mode = path();
        wrong_mode.mode.as_mut().unwrap().width = 1920;
        assert_eq!(
            resolve_snapshot(monitor(), mode(), &[wrong_mode]),
            Err(DisplayOutputBindingError::ModeMismatch)
        );
    }

    #[test]
    fn noncanonical_output_names_are_rejected() {
        for invalid in [
            r"\\.\display2",
            r"\\.\DISPLAY02",
            r"\\.\DISPLAY0",
            "DISPLAY2",
        ] {
            let mut candidate = path();
            candidate.gdi_device_name = invalid.to_owned();
            assert_eq!(
                resolve_snapshot(monitor(), mode(), &[candidate]),
                Err(DisplayOutputBindingError::InvalidOutputName)
            );
        }
    }

    #[test]
    fn displayconfig_fractional_rates_round_to_the_idd_millihertz_contract() {
        assert_eq!(rational_refresh_millihz(60_000, 1001), Some(59_940));
        assert_eq!(rational_refresh_millihz(120_000, 1001), Some(119_880));
        assert_eq!(rational_refresh_millihz(60, 1), Some(60_000));
        assert_eq!(rational_refresh_millihz(60, 0), None);
    }

    #[test]
    fn desktop_rectangles_may_touch_but_must_not_overlap() {
        let first = DesktopRect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        let right = DesktopRect {
            left: 1920,
            top: 0,
            right: 4560,
            bottom: 1216,
        };
        let overlapping = DesktopRect {
            left: 0,
            top: 0,
            right: 2640,
            bottom: 1216,
        };
        assert!(!first.overlaps(right));
        assert!(first.overlaps(overlapping));
    }

    #[test]
    fn new_output_position_uses_physical_ccd_extent() {
        let high_dpi_source = DesktopRect {
            left: 0,
            top: 0,
            right: 2880,
            bottom: 1800,
        };
        let left_source = DesktopRect {
            left: -1920,
            top: 240,
            right: 0,
            bottom: 1320,
        };
        assert_eq!(
            position_after_rectangles(&[high_dpi_source, left_source]),
            (2880, 0)
        );
        assert_eq!(position_after_rectangles(&[]), (0, 0));
    }
}
