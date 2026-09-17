use std::{collections::BTreeMap, mem::size_of, thread, time::Duration};

use thiserror::Error;
use windows::{
    Win32::{
        Devices::Display::{
            DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
            DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_COMPONENT_VIDEO,
            DISPLAYCONFIG_OUTPUT_TECHNOLOGY_COMPOSITE_VIDEO, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_D_JPN,
            DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EMBEDDED,
            DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EXTERNAL,
            DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_USB_TUNNEL,
            DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DVI, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_HD15,
            DISPLAYCONFIG_OUTPUT_TECHNOLOGY_HDMI, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INDIRECT_VIRTUAL,
            DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INDIRECT_WIRED,
            DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_LVDS,
            DISPLAYCONFIG_OUTPUT_TECHNOLOGY_MIRACAST, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_SDI,
            DISPLAYCONFIG_OUTPUT_TECHNOLOGY_SDTVDONGLE, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_SVIDEO,
            DISPLAYCONFIG_OUTPUT_TECHNOLOGY_UDI_EMBEDDED,
            DISPLAYCONFIG_OUTPUT_TECHNOLOGY_UDI_EXTERNAL, DISPLAYCONFIG_PATH_INFO,
            DISPLAYCONFIG_SOURCE_DEVICE_NAME, DISPLAYCONFIG_TARGET_DEVICE_NAME,
            DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QDC_ALL_PATHS,
            QueryDisplayConfig, SDC_ALLOW_CHANGES, SDC_APPLY, SDC_SAVE_TO_DATABASE,
            SDC_USE_SUPPLIED_DISPLAY_CONFIG, SDC_VALIDATE, SetDisplayConfig,
        },
        Graphics::Gdi::{
            CDS_GLOBAL, CDS_NORESET, CDS_RESET, CDS_TEST, CDS_UPDATEREGISTRY,
            ChangeDisplaySettingsExW, DEVMODEW, DISP_CHANGE_SUCCESSFUL, DISPLAY_DEVICEW,
            DISPLAYCONFIG_PATH_ACTIVE, DISPLAYCONFIG_PATH_MODE_IDX_INVALID, DM_BITSPERPEL,
            DM_DISPLAYFREQUENCY, DM_PELSHEIGHT, DM_PELSWIDTH, DM_POSITION, ENUM_CURRENT_SETTINGS,
            ENUM_DISPLAY_SETTINGS_FLAGS, ENUM_DISPLAY_SETTINGS_MODE, EnumDisplayDevicesW,
            EnumDisplaySettingsExW,
        },
    },
    core::PCWSTR,
};

const EXPECTED_DEVICE_ID: &str = "Root\\CapyIOVirtualDisplay";
const EXPECTED_DEVICE_STRING: &str = "CapyIO Phone Display";
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapyIoDisplayPath {
    pub gdi_device_name: String,
    pub active: bool,
    pub target_available: bool,
    pub source_id: u32,
    pub target_id: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisplayTopologyApplyResult {
    pub validation_status: i32,
    pub apply_status: i32,
    pub mode_test_status: i32,
    pub mode_stage_status: i32,
    pub mode_apply_status: i32,
    pub before: CapyIoDisplayPath,
    pub after: CapyIoDisplayPath,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum DisplayPathCategory {
    Physical,
    Virtual,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowsDisplayPath {
    pub gdi_device_name: String,
    pub adapter_device_id: String,
    pub adapter_description: String,
    pub monitor_friendly_name: String,
    pub monitor_device_path: String,
    pub output_technology: i32,
    pub category: DisplayPathCategory,
    pub active: bool,
    pub target_available: bool,
    pub source_id: u32,
    pub target_id: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhysicalDisplaySuppressionSummary {
    pub suppressed: Vec<WindowsDisplayPath>,
    pub retained: Vec<WindowsDisplayPath>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct DisplayPathKey {
    source_adapter_low: u32,
    source_adapter_high: i32,
    source_id: u32,
    target_adapter_low: u32,
    target_adapter_high: i32,
    target_id: u32,
}

#[derive(Debug)]
pub struct PhysicalDisplaySuppressionLease {
    suppressed_keys: Vec<DisplayPathKey>,
    summary: PhysicalDisplaySuppressionSummary,
    restored: bool,
}

#[derive(Debug)]
pub struct WindowsDisplayTopologyLease {
    active_keys: Vec<DisplayPathKey>,
    restored: bool,
}

impl WindowsDisplayTopologyLease {
    pub fn capture() -> Result<Self, DisplayTopologyError> {
        Ok(Self {
            active_keys: active_path_keys(&query_all_paths()?),
            restored: false,
        })
    }

    #[must_use]
    pub const fn is_restored(&self) -> bool {
        self.restored
    }

    pub fn restore(&mut self) -> Result<(), DisplayTopologyError> {
        if self.restored {
            return Err(DisplayTopologyError::TopologyLeaseAlreadyRestored);
        }
        let paths = query_all_paths()?;
        let mut selected = paths
            .iter()
            .filter(|path| {
                path.flags & DISPLAYCONFIG_PATH_ACTIVE != 0
                    && path.targetInfo.targetAvailable.as_bool()
            })
            .copied()
            .collect::<Vec<_>>();
        for key in &self.active_keys {
            if selected.iter().any(|path| display_path_key(path) == *key) {
                continue;
            }
            let path = paths
                .iter()
                .find(|path| {
                    display_path_key(path) == *key && path.targetInfo.targetAvailable.as_bool()
                })
                .copied()
                .ok_or(DisplayTopologyError::TopologyPathUnavailable {
                    source_id: key.source_id,
                    target_id: key.target_id,
                })?;
            selected.push(path);
        }
        apply_selected_paths(&mut selected, "restore captured Windows display topology")?;
        let active = active_path_keys(&query_all_paths()?);
        if self.active_keys.iter().any(|key| !active.contains(key)) {
            return Err(DisplayTopologyError::TopologyRestorePostcondition);
        }
        self.restored = true;
        Ok(())
    }

    pub fn restore_after_target_departure(
        &mut self,
        adapter_luid_low: u32,
        adapter_luid_high: i32,
        target_id: u32,
    ) -> Result<(), DisplayTopologyError> {
        const DEPARTURE_ATTEMPTS: usize = 50;
        const DEPARTURE_STABLE_SAMPLES: usize = 10;
        const RESTORE_STABLE_SAMPLES: usize = 100;
        const POLL_MILLIS: u64 = 100;
        let mut absent_samples = 0;
        for _ in 0..DEPARTURE_ATTEMPTS {
            let paths = query_all_paths()?;
            let target_active = paths.iter().any(|path| {
                path.flags & DISPLAYCONFIG_PATH_ACTIVE != 0
                    && path.targetInfo.adapterId.LowPart == adapter_luid_low
                    && path.targetInfo.adapterId.HighPart == adapter_luid_high
                    && path.targetInfo.id == target_id
            });
            if !target_active {
                absent_samples += 1;
                if absent_samples == DEPARTURE_STABLE_SAMPLES {
                    break;
                }
            } else {
                absent_samples = 0;
            }
            thread::sleep(Duration::from_millis(POLL_MILLIS));
        }
        if absent_samples != DEPARTURE_STABLE_SAMPLES {
            return Err(DisplayTopologyError::TopologyDepartureTimeout { target_id });
        }

        // Driver removal can publish a second topology commit after the exact
        // target first becomes inactive. Restore only after a stable absence,
        // then require the captured paths to survive another stability window.
        // A coexisting display manager can perform its own bounded recovery
        // retries after a foreign IDD target leaves. Outlast that transient
        // arbitration without retrying forever, then require a long stable
        // window before reporting success.
        for _ in 0..5 {
            self.restore()?;
            let mut stable = true;
            for _ in 0..RESTORE_STABLE_SAMPLES {
                thread::sleep(Duration::from_millis(POLL_MILLIS));
                let active = active_path_keys(&query_all_paths()?);
                if self.active_keys.iter().any(|key| !active.contains(key)) {
                    stable = false;
                    break;
                }
            }
            if stable {
                return Ok(());
            }
            self.restored = false;
        }
        Err(DisplayTopologyError::TopologyRestorePostcondition)
    }
}

impl PhysicalDisplaySuppressionLease {
    #[must_use]
    pub fn summary(&self) -> &PhysicalDisplaySuppressionSummary {
        &self.summary
    }

    #[must_use]
    pub const fn is_restored(&self) -> bool {
        self.restored
    }

    pub fn restore(&mut self) -> Result<(), DisplayTopologyError> {
        if self.restored {
            return Err(DisplayTopologyError::SuppressionAlreadyRestored);
        }
        let paths = query_all_paths()?;
        let mut selected = paths
            .iter()
            .filter(|path| path.flags & DISPLAYCONFIG_PATH_ACTIVE != 0)
            .copied()
            .collect::<Vec<_>>();
        for key in &self.suppressed_keys {
            if selected.iter().any(|path| display_path_key(path) == *key) {
                continue;
            }
            let path = paths
                .iter()
                .find(|path| {
                    display_path_key(path) == *key && path.targetInfo.targetAvailable.as_bool()
                })
                .copied()
                .ok_or(DisplayTopologyError::SuppressedPathUnavailable {
                    source_id: key.source_id,
                    target_id: key.target_id,
                })?;
            selected.push(path);
        }
        apply_selected_paths(
            &mut selected,
            "restore suppressed physical display topology",
        )?;
        let active = active_path_keys(&query_all_paths()?);
        if self.suppressed_keys.iter().any(|key| !active.contains(key)) {
            return Err(DisplayTopologyError::SuppressionPostcondition {
                operation: "restore physical displays",
            });
        }
        self.restored = true;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum DisplayTopologyError {
    #[error("Windows display topology operation {operation} failed with status {status}")]
    Win32 {
        operation: &'static str,
        status: i32,
    },
    #[error("expected exactly one CapyIO display adapter output, found {count}")]
    AdapterOutputCount { count: usize },
    #[error(
        "expected exactly one available DisplayConfig path for {gdi_device_name}, found {count}"
    )]
    PathCount {
        gdi_device_name: String,
        count: usize,
    },
    #[error("Windows display path count changed repeatedly during enumeration")]
    UnstableInventory,
    #[error("CapyIO display path became inactive during the {milliseconds}ms stability window")]
    ActivationDidNotStabilize { milliseconds: u64 },
    #[error("expected exactly one {width}x{height}@{frequency} CapyIO mode, found {count}")]
    ModeCount {
        width: u32,
        height: u32,
        frequency: u32,
        count: usize,
    },
    #[error("physical-display suppression requires at least one active virtual display")]
    NoActiveVirtualDisplay,
    #[error("there is no active confidently classified physical display to suppress")]
    NoActivePhysicalDisplay,
    #[error("the display topology changed while physical-display suppression was prepared")]
    SuppressionInventoryChanged,
    #[error("the suppressed display source {source_id} target {target_id} is no longer available")]
    SuppressedPathUnavailable { source_id: u32, target_id: u32 },
    #[error("physical-display suppression has already been restored")]
    SuppressionAlreadyRestored,
    #[error("physical-display suppression postcondition failed while trying to {operation}")]
    SuppressionPostcondition { operation: &'static str },
    #[error("captured display source {source_id} target {target_id} is no longer available")]
    TopologyPathUnavailable { source_id: u32, target_id: u32 },
    #[error("the Windows display topology lease has already been restored")]
    TopologyLeaseAlreadyRestored,
    #[error("the captured Windows display topology was not restored exactly")]
    TopologyRestorePostcondition,
    #[error("removed display target {target_id} did not leave the active topology in time")]
    TopologyDepartureTimeout { target_id: u32 },
}

pub fn inspect_capyio_display_path() -> Result<CapyIoDisplayPath, DisplayTopologyError> {
    let gdi_device_name = find_capyio_gdi_device_name()?;
    let paths = query_all_paths()?;
    let (_, summary) = find_unique_capyio_path(&paths, &gdi_device_name)?;
    Ok(summary)
}

pub fn inspect_windows_display_topology() -> Result<Vec<WindowsDisplayPath>, DisplayTopologyError> {
    describe_paths(&query_all_paths()?)
}

pub fn preview_physical_display_suppression()
-> Result<PhysicalDisplaySuppressionSummary, DisplayTopologyError> {
    let descriptions = inspect_windows_display_topology()?;
    build_physical_suppression_summary(&descriptions)
}

pub fn suppress_active_physical_displays()
-> Result<PhysicalDisplaySuppressionLease, DisplayTopologyError> {
    let first_paths = query_all_paths()?;
    let first_descriptions = describe_paths(&first_paths)?;
    let second_paths = query_all_paths()?;
    let second_descriptions = describe_paths(&second_paths)?;
    if active_inventory_signature(&first_paths, &first_descriptions)
        != active_inventory_signature(&second_paths, &second_descriptions)
    {
        return Err(DisplayTopologyError::SuppressionInventoryChanged);
    }
    let summary = build_physical_suppression_summary(&second_descriptions)?;
    let suppressed_keys = second_paths
        .iter()
        .zip(&second_descriptions)
        .filter(|(_, description)| {
            description.active && description.category == DisplayPathCategory::Physical
        })
        .map(|(path, _)| display_path_key(path))
        .collect::<Vec<_>>();
    let retained_keys = second_paths
        .iter()
        .zip(&second_descriptions)
        .filter(|(_, description)| {
            description.active && description.category != DisplayPathCategory::Physical
        })
        .map(|(path, _)| display_path_key(path))
        .collect::<Vec<_>>();
    let mut selected = second_paths
        .iter()
        .zip(&second_descriptions)
        .filter(|(_, description)| {
            description.active && description.category != DisplayPathCategory::Physical
        })
        .map(|(path, _)| *path)
        .collect::<Vec<_>>();
    apply_selected_paths(&mut selected, "suppress active physical displays")?;
    let after_paths = query_all_paths()?;
    let after_descriptions = describe_paths(&after_paths)?;
    let after_active_keys = active_path_keys(&after_paths);
    if after_descriptions
        .iter()
        .any(|path| path.active && path.category == DisplayPathCategory::Physical)
        || !retained_keys
            .iter()
            .all(|expected| after_active_keys.contains(expected))
    {
        let mut rollback = second_paths
            .iter()
            .filter(|path| path.flags & DISPLAYCONFIG_PATH_ACTIVE != 0)
            .copied()
            .collect::<Vec<_>>();
        let _ = apply_selected_paths(&mut rollback, "roll back physical display suppression");
        return Err(DisplayTopologyError::SuppressionPostcondition {
            operation: "suppress physical displays",
        });
    }
    Ok(PhysicalDisplaySuppressionLease {
        suppressed_keys,
        summary,
        restored: false,
    })
}

fn build_physical_suppression_summary(
    descriptions: &[WindowsDisplayPath],
) -> Result<PhysicalDisplaySuppressionSummary, DisplayTopologyError> {
    let retained = descriptions
        .iter()
        .filter(|path| path.active && path.category != DisplayPathCategory::Physical)
        .cloned()
        .collect::<Vec<_>>();
    if !retained
        .iter()
        .any(|path| path.category == DisplayPathCategory::Virtual)
    {
        return Err(DisplayTopologyError::NoActiveVirtualDisplay);
    }
    let suppressed = descriptions
        .iter()
        .filter(|path| path.active && path.category == DisplayPathCategory::Physical)
        .cloned()
        .collect::<Vec<_>>();
    if suppressed.is_empty() {
        return Err(DisplayTopologyError::NoActivePhysicalDisplay);
    }
    Ok(PhysicalDisplaySuppressionSummary {
        suppressed,
        retained,
    })
}

fn describe_paths(
    paths: &[DISPLAYCONFIG_PATH_INFO],
) -> Result<Vec<WindowsDisplayPath>, DisplayTopologyError> {
    let adapters = enumerate_display_adapters();
    paths
        .iter()
        .map(|path| {
            let gdi_device_name = source_gdi_name(path)?;
            let (adapter_description, adapter_device_id) = adapters
                .get(&gdi_device_name.to_ascii_lowercase())
                .cloned()
                .unwrap_or_default();
            let target = target_device_name(path)?;
            let output_technology = path.targetInfo.outputTechnology.0;
            Ok(WindowsDisplayPath {
                gdi_device_name,
                adapter_device_id: adapter_device_id.clone(),
                adapter_description,
                monitor_friendly_name: wide_string(&target.monitorFriendlyDeviceName),
                monitor_device_path: wide_string(&target.monitorDevicePath),
                output_technology,
                category: classify_display_path(&adapter_device_id, output_technology),
                active: path.flags & DISPLAYCONFIG_PATH_ACTIVE != 0,
                target_available: path.targetInfo.targetAvailable.as_bool(),
                source_id: path.sourceInfo.id,
                target_id: path.targetInfo.id,
            })
        })
        .collect()
}

fn active_inventory_signature(
    paths: &[DISPLAYCONFIG_PATH_INFO],
    descriptions: &[WindowsDisplayPath],
) -> Vec<(DisplayPathKey, DisplayPathCategory)> {
    let mut signature = paths
        .iter()
        .zip(descriptions)
        .filter(|(_, description)| description.active)
        .map(|(path, description)| (display_path_key(path), description.category))
        .collect::<Vec<_>>();
    signature.sort();
    signature
}

fn active_path_keys(paths: &[DISPLAYCONFIG_PATH_INFO]) -> Vec<DisplayPathKey> {
    paths
        .iter()
        .filter(|path| path.flags & DISPLAYCONFIG_PATH_ACTIVE != 0)
        .map(display_path_key)
        .collect()
}

fn display_path_key(path: &DISPLAYCONFIG_PATH_INFO) -> DisplayPathKey {
    DisplayPathKey {
        source_adapter_low: path.sourceInfo.adapterId.LowPart,
        source_adapter_high: path.sourceInfo.adapterId.HighPart,
        source_id: path.sourceInfo.id,
        target_adapter_low: path.targetInfo.adapterId.LowPart,
        target_adapter_high: path.targetInfo.adapterId.HighPart,
        target_id: path.targetInfo.id,
    }
}

fn apply_selected_paths(
    selected: &mut [DISPLAYCONFIG_PATH_INFO],
    operation: &'static str,
) -> Result<(), DisplayTopologyError> {
    for path in selected.iter_mut() {
        path.flags |= DISPLAYCONFIG_PATH_ACTIVE;
        path.sourceInfo.Anonymous.modeInfoIdx = DISPLAYCONFIG_PATH_MODE_IDX_INVALID;
        path.targetInfo.Anonymous.modeInfoIdx = DISPLAYCONFIG_PATH_MODE_IDX_INVALID;
    }
    let validation_status = unsafe {
        SetDisplayConfig(
            Some(selected),
            None,
            SDC_VALIDATE | SDC_USE_SUPPLIED_DISPLAY_CONFIG | SDC_ALLOW_CHANGES,
        )
    };
    if validation_status != 0 {
        return Err(DisplayTopologyError::Win32 {
            operation,
            status: validation_status,
        });
    }
    let apply_status = unsafe {
        SetDisplayConfig(
            Some(selected),
            None,
            SDC_APPLY | SDC_USE_SUPPLIED_DISPLAY_CONFIG | SDC_ALLOW_CHANGES,
        )
    };
    if apply_status != 0 {
        return Err(DisplayTopologyError::Win32 {
            operation,
            status: apply_status,
        });
    }
    Ok(())
}

fn classify_display_path(adapter_device_id: &str, output_technology: i32) -> DisplayPathCategory {
    if matches!(
        output_technology,
        value if value == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INDIRECT_WIRED.0
            || value == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INDIRECT_VIRTUAL.0
            || value == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_MIRACAST.0
    ) {
        return DisplayPathCategory::Virtual;
    }
    if !is_physical_connector(output_technology) {
        return DisplayPathCategory::Unknown;
    }
    let normalized = adapter_device_id.trim().to_ascii_uppercase();
    if normalized.starts_with("ROOT\\") || normalized.starts_with("SWD\\") {
        return DisplayPathCategory::Virtual;
    }
    if normalized.starts_with("PCI\\")
        || normalized.starts_with("USB\\")
        || normalized.starts_with("ACPI\\")
    {
        DisplayPathCategory::Physical
    } else {
        // Root-enumerated and unknown-bus adapters are commonly IDDs. Keep
        // them active unless a future signed Adapter registration identifies
        // them more precisely.
        DisplayPathCategory::Unknown
    }
}

fn is_physical_connector(output_technology: i32) -> bool {
    matches!(
        output_technology,
        value if value == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_HD15.0
            || value == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_SVIDEO.0
            || value == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_COMPOSITE_VIDEO.0
            || value == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_COMPONENT_VIDEO.0
            || value == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DVI.0
            || value == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_HDMI.0
            || value == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_LVDS.0
            || value == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_D_JPN.0
            || value == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_SDI.0
            || value == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EXTERNAL.0
            || value == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EMBEDDED.0
            || value == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_UDI_EXTERNAL.0
            || value == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_UDI_EMBEDDED.0
            || value == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_SDTVDONGLE.0
            || value == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_USB_TUNNEL.0
            || value == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL.0
    )
}

pub fn validate_capyio_extended_path() -> Result<i32, DisplayTopologyError> {
    let gdi_device_name = find_capyio_gdi_device_name()?;
    let paths = query_all_paths()?;
    let selected = build_extended_paths(&paths, &gdi_device_name)?;
    let status = unsafe {
        SetDisplayConfig(
            Some(&selected),
            None,
            SDC_VALIDATE | SDC_USE_SUPPLIED_DISPLAY_CONFIG | SDC_ALLOW_CHANGES,
        )
    };
    if status != 0 {
        return Err(DisplayTopologyError::Win32 {
            operation: "validate CapyIO extended topology",
            status,
        });
    }
    Ok(status)
}

pub fn activate_capyio_extended_path() -> Result<DisplayTopologyApplyResult, DisplayTopologyError> {
    let gdi_device_name = find_capyio_gdi_device_name()?;
    let paths = query_all_paths()?;
    let (_, before) = find_unique_capyio_path(&paths, &gdi_device_name)?;
    let selected = build_extended_paths(&paths, &gdi_device_name)?;
    let validation_status = unsafe {
        SetDisplayConfig(
            Some(&selected),
            None,
            SDC_VALIDATE | SDC_USE_SUPPLIED_DISPLAY_CONFIG | SDC_ALLOW_CHANGES,
        )
    };
    if validation_status != 0 {
        return Err(DisplayTopologyError::Win32 {
            operation: "validate CapyIO extended topology",
            status: validation_status,
        });
    }
    let apply_status = unsafe {
        SetDisplayConfig(
            Some(&selected),
            None,
            SDC_APPLY | SDC_USE_SUPPLIED_DISPLAY_CONFIG | SDC_ALLOW_CHANGES | SDC_SAVE_TO_DATABASE,
        )
    };
    if apply_status != 0 {
        return Err(DisplayTopologyError::Win32 {
            operation: "apply CapyIO extended topology",
            status: apply_status,
        });
    }
    let (mode_test_status, mode_stage_status, mode_apply_status) =
        apply_exact_mode(&gdi_device_name, 1980, 912, 60)?;
    const STABILITY_SAMPLE_MILLIS: u64 = 100;
    const STABILITY_SAMPLES: u64 = 10;
    let mut after = inspect_capyio_display_path()?;
    for _ in 0..STABILITY_SAMPLES {
        thread::sleep(Duration::from_millis(STABILITY_SAMPLE_MILLIS));
        after = inspect_capyio_display_path()?;
        if !after.active {
            return Err(DisplayTopologyError::ActivationDidNotStabilize {
                milliseconds: STABILITY_SAMPLE_MILLIS * STABILITY_SAMPLES,
            });
        }
    }
    Ok(DisplayTopologyApplyResult {
        validation_status,
        apply_status,
        mode_test_status,
        mode_stage_status,
        mode_apply_status,
        before,
        after,
    })
}

fn apply_exact_mode(
    gdi_device_name: &str,
    width: u32,
    height: u32,
    frequency: u32,
) -> Result<(i32, i32, i32), DisplayTopologyError> {
    let wide_name = nul_terminated(gdi_device_name);
    let name = PCWSTR(wide_name.as_ptr());
    let mut current = new_devmode();
    if !unsafe {
        EnumDisplaySettingsExW(
            name,
            ENUM_CURRENT_SETTINGS,
            &mut current,
            ENUM_DISPLAY_SETTINGS_FLAGS(0),
        )
    }
    .as_bool()
    {
        return Err(DisplayTopologyError::Win32 {
            operation: "read active CapyIO display mode",
            status: -1,
        });
    }
    let current_position = unsafe { current.Anonymous1.Anonymous2.dmPosition };
    let mut matches = Vec::new();
    for index in 0..256 {
        let mut candidate = new_devmode();
        if !unsafe {
            EnumDisplaySettingsExW(
                name,
                ENUM_DISPLAY_SETTINGS_MODE(index),
                &mut candidate,
                ENUM_DISPLAY_SETTINGS_FLAGS(0),
            )
        }
        .as_bool()
        {
            break;
        }
        if candidate.dmPelsWidth == width
            && candidate.dmPelsHeight == height
            && candidate.dmDisplayFrequency == frequency
        {
            matches.push(candidate);
        }
    }
    if matches.len() != 1 {
        return Err(DisplayTopologyError::ModeCount {
            width,
            height,
            frequency,
            count: matches.len(),
        });
    }
    let mut selected = matches.remove(0);
    selected.Anonymous1.Anonymous2.dmPosition = current_position;
    selected.dmFields =
        DM_POSITION | DM_BITSPERPEL | DM_PELSWIDTH | DM_PELSHEIGHT | DM_DISPLAYFREQUENCY;
    let test = unsafe { ChangeDisplaySettingsExW(name, Some(&selected), None, CDS_TEST, None) };
    if test != DISP_CHANGE_SUCCESSFUL {
        return Err(DisplayTopologyError::Win32 {
            operation: "test exact active CapyIO display mode",
            status: test.0,
        });
    }
    let stage = unsafe {
        ChangeDisplaySettingsExW(
            name,
            Some(&selected),
            None,
            CDS_GLOBAL | CDS_UPDATEREGISTRY | CDS_NORESET,
            None,
        )
    };
    if stage != DISP_CHANGE_SUCCESSFUL {
        return Err(DisplayTopologyError::Win32 {
            operation: "stage exact active CapyIO display mode",
            status: stage.0,
        });
    }
    let apply = unsafe { ChangeDisplaySettingsExW(PCWSTR::null(), None, None, CDS_RESET, None) };
    if apply != DISP_CHANGE_SUCCESSFUL {
        return Err(DisplayTopologyError::Win32 {
            operation: "apply exact active CapyIO display mode",
            status: apply.0,
        });
    }
    Ok((test.0, stage.0, apply.0))
}

pub(super) fn position_display_output(
    gdi_device_name: &str,
    x: i32,
    y: i32,
) -> Result<(), DisplayTopologyError> {
    let wide_name = nul_terminated(gdi_device_name);
    let name = PCWSTR(wide_name.as_ptr());
    let mut current = new_devmode();
    if !unsafe {
        EnumDisplaySettingsExW(
            name,
            ENUM_CURRENT_SETTINGS,
            &mut current,
            ENUM_DISPLAY_SETTINGS_FLAGS(0),
        )
    }
    .as_bool()
    {
        return Err(DisplayTopologyError::Win32 {
            operation: "read owned display position",
            status: -1,
        });
    }
    current.Anonymous1.Anonymous2.dmPosition.x = x;
    current.Anonymous1.Anonymous2.dmPosition.y = y;
    // Some IDDs publish a current mode before the target is fully attached to
    // the session desktop. Submitting only DM_POSITION can update the caller's
    // GDI cache without making the HMONITOR visible to a fresh process. Commit
    // the complete enumerated mode together with its physical-pixel position.
    current.dmFields =
        DM_POSITION | DM_BITSPERPEL | DM_PELSWIDTH | DM_PELSHEIGHT | DM_DISPLAYFREQUENCY;
    let test = unsafe { ChangeDisplaySettingsExW(name, Some(&current), None, CDS_TEST, None) };
    if test != DISP_CHANGE_SUCCESSFUL {
        return Err(DisplayTopologyError::Win32 {
            operation: "test owned display position",
            status: test.0,
        });
    }
    // Apply this output immediately. A staged CDS_NORESET transaction can be
    // observed by the caller through its GDI cache while a fresh process still
    // sees the old desktop. This is also the commit pattern used by the mature
    // display-settings implementation integrated by the video-only adapter.
    let stage =
        unsafe { ChangeDisplaySettingsExW(name, Some(&current), None, CDS_UPDATEREGISTRY, None) };
    if stage != DISP_CHANGE_SUCCESSFUL {
        return Err(DisplayTopologyError::Win32 {
            operation: "stage owned display position",
            status: stage.0,
        });
    }
    let apply = unsafe { ChangeDisplaySettingsExW(PCWSTR::null(), None, None, CDS_RESET, None) };
    if apply != DISP_CHANGE_SUCCESSFUL {
        return Err(DisplayTopologyError::Win32 {
            operation: "apply owned display position",
            status: apply.0,
        });
    }
    Ok(())
}

fn new_devmode() -> DEVMODEW {
    DEVMODEW {
        dmSize: size_of::<DEVMODEW>() as u16,
        ..Default::default()
    }
}

fn nul_terminated(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn build_extended_paths(
    paths: &[DISPLAYCONFIG_PATH_INFO],
    gdi_device_name: &str,
) -> Result<Vec<DISPLAYCONFIG_PATH_INFO>, DisplayTopologyError> {
    let (capyio_index, _) = find_unique_capyio_path(paths, gdi_device_name)?;
    let mut selected = Vec::new();
    for (index, path) in paths.iter().copied().enumerate() {
        if path.flags & DISPLAYCONFIG_PATH_ACTIVE == 0 && index != capyio_index {
            continue;
        }
        let mut path = path;
        path.flags |= DISPLAYCONFIG_PATH_ACTIVE;
        path.sourceInfo.Anonymous.modeInfoIdx = DISPLAYCONFIG_PATH_MODE_IDX_INVALID;
        path.targetInfo.Anonymous.modeInfoIdx = DISPLAYCONFIG_PATH_MODE_IDX_INVALID;
        selected.push(path);
    }
    Ok(selected)
}

fn find_unique_capyio_path(
    paths: &[DISPLAYCONFIG_PATH_INFO],
    gdi_device_name: &str,
) -> Result<(usize, CapyIoDisplayPath), DisplayTopologyError> {
    let mut matches = Vec::new();
    for (index, path) in paths.iter().enumerate() {
        if !path.targetInfo.targetAvailable.as_bool() {
            continue;
        }
        let source_name = source_gdi_name(path)?;
        if source_name.eq_ignore_ascii_case(gdi_device_name) {
            matches.push((index, summarize(path, source_name)));
        }
    }
    if matches.len() != 1 {
        return Err(DisplayTopologyError::PathCount {
            gdi_device_name: gdi_device_name.to_owned(),
            count: matches.len(),
        });
    }
    Ok(matches.remove(0))
}

fn summarize(path: &DISPLAYCONFIG_PATH_INFO, gdi_device_name: String) -> CapyIoDisplayPath {
    CapyIoDisplayPath {
        gdi_device_name,
        active: path.flags & DISPLAYCONFIG_PATH_ACTIVE != 0,
        target_available: path.targetInfo.targetAvailable.as_bool(),
        source_id: path.sourceInfo.id,
        target_id: path.targetInfo.id,
    }
}

pub(super) fn source_gdi_name(
    path: &DISPLAYCONFIG_PATH_INFO,
) -> Result<String, DisplayTopologyError> {
    let mut request = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
        header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
            size: size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
            adapterId: path.sourceInfo.adapterId,
            id: path.sourceInfo.id,
        },
        ..Default::default()
    };
    let status = unsafe { DisplayConfigGetDeviceInfo(&mut request.header) };
    if status != 0 {
        return Err(DisplayTopologyError::Win32 {
            operation: "read DisplayConfig source name",
            status,
        });
    }
    Ok(wide_string(&request.viewGdiDeviceName))
}

fn target_device_name(
    path: &DISPLAYCONFIG_PATH_INFO,
) -> Result<DISPLAYCONFIG_TARGET_DEVICE_NAME, DisplayTopologyError> {
    let mut request = DISPLAYCONFIG_TARGET_DEVICE_NAME {
        header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
            size: size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32,
            adapterId: path.targetInfo.adapterId,
            id: path.targetInfo.id,
        },
        ..Default::default()
    };
    let status = unsafe { DisplayConfigGetDeviceInfo(&mut request.header) };
    if status != 0 {
        return Err(DisplayTopologyError::Win32 {
            operation: "read DisplayConfig target name",
            status,
        });
    }
    Ok(request)
}

fn enumerate_display_adapters() -> BTreeMap<String, (String, String)> {
    let mut adapters = BTreeMap::new();
    for index in 0..256 {
        let mut device = DISPLAY_DEVICEW {
            cb: size_of::<DISPLAY_DEVICEW>() as u32,
            ..Default::default()
        };
        if !unsafe { EnumDisplayDevicesW(PCWSTR::null(), index, &mut device, 0) }.as_bool() {
            break;
        }
        adapters.insert(
            wide_string(&device.DeviceName).to_ascii_lowercase(),
            (
                wide_string(&device.DeviceString),
                wide_string(&device.DeviceID),
            ),
        );
    }
    adapters
}

pub(super) fn find_capyio_gdi_device_name() -> Result<String, DisplayTopologyError> {
    let mut matches = Vec::new();
    for index in 0..256 {
        let mut device = DISPLAY_DEVICEW {
            cb: size_of::<DISPLAY_DEVICEW>() as u32,
            ..Default::default()
        };
        if !unsafe { EnumDisplayDevicesW(PCWSTR::null(), index, &mut device, 0) }.as_bool() {
            break;
        }
        let id = wide_string(&device.DeviceID);
        let description = wide_string(&device.DeviceString);
        if id.eq_ignore_ascii_case(EXPECTED_DEVICE_ID) && description == EXPECTED_DEVICE_STRING {
            matches.push(wide_string(&device.DeviceName));
        }
    }
    if matches.len() != 1 {
        return Err(DisplayTopologyError::AdapterOutputCount {
            count: matches.len(),
        });
    }
    Ok(matches.remove(0))
}

pub(super) fn query_all_paths() -> Result<Vec<DISPLAYCONFIG_PATH_INFO>, DisplayTopologyError> {
    for _ in 0..4 {
        let mut path_count = 0;
        let mut mode_count = 0;
        let size_status =
            unsafe { GetDisplayConfigBufferSizes(QDC_ALL_PATHS, &mut path_count, &mut mode_count) };
        if size_status.0 != 0 {
            return Err(DisplayTopologyError::Win32 {
                operation: "size DisplayConfig inventory",
                status: size_status.0 as i32,
            });
        }
        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
        let mut modes = vec![Default::default(); mode_count as usize];
        let query_status = unsafe {
            QueryDisplayConfig(
                QDC_ALL_PATHS,
                &mut path_count,
                paths.as_mut_ptr(),
                &mut mode_count,
                modes.as_mut_ptr(),
                None,
            )
        };
        if query_status.0 == ERROR_INSUFFICIENT_BUFFER {
            continue;
        }
        if query_status.0 != 0 {
            return Err(DisplayTopologyError::Win32 {
                operation: "query DisplayConfig inventory",
                status: query_status.0 as i32,
            });
        }
        paths.truncate(path_count as usize);
        return Ok(paths);
    }
    Err(DisplayTopologyError::UnstableInventory)
}

fn wide_string(value: &[u16]) -> String {
    let length = value
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(value.len());
    String::from_utf16_lossy(&value[..length])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(name: &str, category: DisplayPathCategory, active: bool) -> WindowsDisplayPath {
        WindowsDisplayPath {
            gdi_device_name: name.to_owned(),
            adapter_device_id: String::new(),
            adapter_description: String::new(),
            monitor_friendly_name: String::new(),
            monitor_device_path: String::new(),
            output_technology: 0,
            category,
            active,
            target_available: true,
            source_id: 0,
            target_id: 0,
        }
    }

    #[test]
    fn indirect_outputs_are_virtual_even_on_a_hardware_adapter() {
        assert_eq!(
            classify_display_path(
                "PCI\\VEN_10DE&DEV_2204",
                DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INDIRECT_WIRED.0,
            ),
            DisplayPathCategory::Virtual
        );
    }

    #[test]
    fn hardware_bus_with_physical_connector_is_physical() {
        assert_eq!(
            classify_display_path(
                "PCI\\VEN_10DE&DEV_2204",
                DISPLAYCONFIG_OUTPUT_TECHNOLOGY_HDMI.0,
            ),
            DisplayPathCategory::Physical
        );
        assert_eq!(
            classify_display_path("ACPI\\PNP0C0D", DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL.0,),
            DisplayPathCategory::Physical
        );
    }

    #[test]
    fn root_enumerated_display_is_virtual_even_with_a_physical_looking_connector() {
        assert_eq!(
            classify_display_path(
                "ROOT\\SUNSHINEVIRTUALDISPLAY",
                DISPLAYCONFIG_OUTPUT_TECHNOLOGY_HDMI.0,
            ),
            DisplayPathCategory::Virtual
        );
    }

    #[test]
    fn unknown_output_technology_fails_safe() {
        assert_eq!(
            classify_display_path("PCI\\VEN_10DE&DEV_2204", -1),
            DisplayPathCategory::Unknown
        );
    }

    #[test]
    fn suppression_plan_removes_only_active_physical_paths() {
        let descriptions = [
            path("virtual", DisplayPathCategory::Virtual, true),
            path("physical", DisplayPathCategory::Physical, true),
            path("unknown", DisplayPathCategory::Unknown, true),
            path("inactive-physical", DisplayPathCategory::Physical, false),
        ];
        let summary = build_physical_suppression_summary(&descriptions).unwrap();
        assert_eq!(
            summary
                .suppressed
                .iter()
                .map(|path| path.gdi_device_name.as_str())
                .collect::<Vec<_>>(),
            ["physical"]
        );
        assert_eq!(
            summary
                .retained
                .iter()
                .map(|path| path.gdi_device_name.as_str())
                .collect::<Vec<_>>(),
            ["virtual", "unknown"]
        );
    }

    #[test]
    fn suppression_plan_requires_an_active_virtual_display() {
        let descriptions = [
            path("physical", DisplayPathCategory::Physical, true),
            path("unknown", DisplayPathCategory::Unknown, true),
        ];
        assert_eq!(
            build_physical_suppression_summary(&descriptions).unwrap_err(),
            DisplayTopologyError::NoActiveVirtualDisplay
        );
    }

    #[test]
    fn suppression_plan_requires_an_active_physical_display() {
        let descriptions = [
            path("virtual", DisplayPathCategory::Virtual, true),
            path("unknown", DisplayPathCategory::Unknown, true),
        ];
        assert_eq!(
            build_physical_suppression_summary(&descriptions).unwrap_err(),
            DisplayTopologyError::NoActivePhysicalDisplay
        );
    }
}
