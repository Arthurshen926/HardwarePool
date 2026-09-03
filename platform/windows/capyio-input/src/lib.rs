#![deny(unsafe_op_in_unsafe_fn)]

//! Runtime probe for the Windows synthetic Precision Touchpad API.
//!
//! The current Windows SDK may predate these exports even when the operating
//! system provides them, so this crate deliberately resolves the API from
//! `user32.dll` at runtime. The default probe performs no input injection. The
//! optional device smoke creates a five-contact, 100 x 60 mm synthetic
//! touchpad and immediately destroys it without submitting contact frames.

use std::fmt;

mod injection_fixture;
mod projection;
mod session;
mod vhf_broker;
mod vhf_win32;

pub use injection_fixture::{
    FIXED_DOUBLE_TAP_DRAG_FRAMES, FIXED_INJECTION_INTERVAL_MILLIS, FIXED_INJECTION_UPDATE_FRAMES,
    SyntheticTouchpadGesture, TouchpadInjectionDryRun, TouchpadInjectionFixture,
    build_touchpad_injection_fixture,
};
pub use projection::{
    MAX_WINDOWS_TOUCHPAD_BATCHES, WindowsTouchpadBatch, WindowsTouchpadContact,
    WindowsTouchpadContactPhase, WindowsTouchpadProjection, WindowsTouchpadProjectionDisposition,
    WindowsTouchpadProjectionError, WindowsTouchpadProjector,
};
#[cfg(windows)]
pub use projection::{NativeTouchpadBatch, NativeTouchpadContactView};
pub use session::{
    SyntheticTouchpadSession, SyntheticTouchpadSessionError, SyntheticTouchpadSessionState,
    SyntheticTouchpadSubmission,
};
pub use vhf_broker::{
    VHF_BROKER_MAX_CONTACTS, VHF_BROKER_RECORD_SIZE, VhfBrokerClient, VhfBrokerClientError,
    VhfBrokerCodecError, VhfBrokerContact, VhfBrokerProjectionError, VhfBrokerRecordEncoder,
    VhfBrokerRecordTransport, VhfBrokerSnapshot, VhfBrokerSnapshotProjector,
    VhfBrokerTransportError, VhfTouchpadSession, VhfTouchpadSessionError, VhfTouchpadSessionState,
};
pub use vhf_win32::{VhfBrokerInterfaceProbe, VhfWin32Transport, probe_vhf_broker_interface};

pub const IMPLEMENTATION_STATUS: &str = "synthetic-touchpad-sink-session";
pub const PROBE_SCHEMA_VERSION: u32 = 1;
pub const MAX_SYNTHETIC_TOUCHPAD_CONTACTS: u32 = 5;

const CREATE_DEVICE_SYMBOL: &str = "CreateSyntheticPointerDevice2";
const INJECT_POINTER_SYMBOL: &str = "InjectSyntheticPointerInput";
const INJECT_ACTION_SYMBOL: &str = "InjectTouchpadAction";
const DESTROY_DEVICE_SYMBOL: &str = "DestroySyntheticPointerDevice";

const REQUIRED_SYMBOLS: [&str; 4] = [
    CREATE_DEVICE_SYMBOL,
    INJECT_POINTER_SYMBOL,
    INJECT_ACTION_SYMBOL,
    DESTROY_DEVICE_SYMBOL,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ApiSymbolStatus {
    pub name: &'static str,
    pub exported: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyntheticTouchpadApiProbe {
    pub platform: &'static str,
    pub user32_loaded: bool,
    pub load_error_code: Option<u32>,
    pub symbols: [ApiSymbolStatus; 4],
}

impl SyntheticTouchpadApiProbe {
    #[must_use]
    pub fn is_available(&self) -> bool {
        self.user32_loaded && self.symbols.iter().all(|symbol| symbol.exported)
    }

    #[must_use]
    pub fn missing_symbols(&self) -> Vec<&'static str> {
        self.symbols
            .iter()
            .filter_map(|symbol| (!symbol.exported).then_some(symbol.name))
            .collect()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SyntheticTouchpadParameters {
    /// Maximum simultaneously active contacts. Windows permits `1..=5`.
    pub max_contacts: u32,
    /// Physical width in himetric units (1/100 mm).
    pub width_himetric: u32,
    /// Physical height in himetric units (1/100 mm).
    pub height_himetric: u32,
}

impl Default for SyntheticTouchpadParameters {
    fn default() -> Self {
        Self {
            max_contacts: MAX_SYNTHETIC_TOUCHPAD_CONTACTS,
            width_himetric: 10_000,
            height_himetric: 6_000,
        }
    }
}

impl SyntheticTouchpadParameters {
    pub fn validate(self) -> Result<(), SyntheticTouchpadParameterError> {
        if !(1..=MAX_SYNTHETIC_TOUCHPAD_CONTACTS).contains(&self.max_contacts) {
            return Err(SyntheticTouchpadParameterError::InvalidContactCount(
                self.max_contacts,
            ));
        }
        if self.width_himetric == 0 || self.height_himetric == 0 {
            return Err(SyntheticTouchpadParameterError::MissingPhysicalSize);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyntheticTouchpadParameterError {
    InvalidContactCount(u32),
    MissingPhysicalSize,
}

impl fmt::Display for SyntheticTouchpadParameterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidContactCount(count) => write!(
                formatter,
                "synthetic touchpads require 1..={MAX_SYNTHETIC_TOUCHPAD_CONTACTS} contacts, received {count}"
            ),
            Self::MissingPhysicalSize => {
                formatter.write_str("synthetic touchpad physical dimensions must be non-zero")
            }
        }
    }
}

impl std::error::Error for SyntheticTouchpadParameterError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SyntheticTouchpadInjectionError {
    UnsupportedPlatform,
    InvalidParameters(SyntheticTouchpadParameterError),
    User32LoadFailed { error_code: u32 },
    MissingSymbols { symbols: Vec<&'static str> },
    CreationFailed { error_code: u32 },
    SubmissionFailed { error_code: u32 },
}

impl fmt::Display for SyntheticTouchpadInjectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedPlatform => {
                formatter.write_str("synthetic touchpad injection requires Windows")
            }
            Self::InvalidParameters(error) => {
                write!(formatter, "invalid device parameters: {error}")
            }
            Self::User32LoadFailed { error_code } => {
                write!(
                    formatter,
                    "failed to load System32 user32.dll: {error_code}"
                )
            }
            Self::MissingSymbols { symbols } => {
                write!(
                    formatter,
                    "required user32 exports are missing: {}",
                    symbols.join(",")
                )
            }
            Self::CreationFailed { error_code } => {
                write!(
                    formatter,
                    "synthetic touchpad creation failed: {error_code}"
                )
            }
            Self::SubmissionFailed { error_code } => {
                write!(
                    formatter,
                    "synthetic touchpad batch submission failed: {error_code}"
                )
            }
        }
    }
}

impl std::error::Error for SyntheticTouchpadInjectionError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BatchInjectionOutcome {
    SkippedEmpty,
    Submitted { contacts: u8 },
}

pub struct SyntheticTouchpadDevice {
    inner: platform::SyntheticTouchpadDevice,
}

impl SyntheticTouchpadDevice {
    pub fn create(
        parameters: SyntheticTouchpadParameters,
    ) -> Result<Self, SyntheticTouchpadInjectionError> {
        parameters
            .validate()
            .map_err(SyntheticTouchpadInjectionError::InvalidParameters)?;
        Ok(Self {
            inner: platform::create_injection_device(parameters)?,
        })
    }

    pub fn inject_batch(
        &mut self,
        batch: &WindowsTouchpadBatch,
    ) -> Result<BatchInjectionOutcome, SyntheticTouchpadInjectionError> {
        platform::inject_batch(&mut self.inner, batch)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeviceCreationProbe {
    UnsupportedPlatform,
    InvalidParameters(SyntheticTouchpadParameterError),
    User32LoadFailed {
        error_code: u32,
    },
    MissingSymbols {
        symbols: Vec<&'static str>,
    },
    CreationFailed {
        error_code: u32,
    },
    CreatedAndDestroyed {
        parameters: SyntheticTouchpadParameters,
    },
}

/// Resolve every API required by the synthetic Precision Touchpad path.
///
/// This is a read-only symbol probe. It does not create a device or inject
/// pointer input.
#[must_use]
pub fn probe_synthetic_touchpad_api() -> SyntheticTouchpadApiProbe {
    platform::probe_api()
}

/// Create and immediately destroy a synthetic Precision Touchpad.
///
/// No contact frame or gesture action is injected. Callers must opt into this
/// operation explicitly; normal API probing remains read-only.
#[must_use]
pub fn probe_synthetic_touchpad_device_creation(
    parameters: SyntheticTouchpadParameters,
) -> DeviceCreationProbe {
    if let Err(error) = parameters.validate() {
        return DeviceCreationProbe::InvalidParameters(error);
    }
    platform::probe_device_creation(parameters)
}

#[cfg(not(windows))]
mod platform {
    use super::{
        ApiSymbolStatus, BatchInjectionOutcome, DeviceCreationProbe, REQUIRED_SYMBOLS,
        SyntheticTouchpadApiProbe, SyntheticTouchpadInjectionError, SyntheticTouchpadParameters,
        WindowsTouchpadBatch,
    };

    pub(super) struct SyntheticTouchpadDevice;

    pub(super) fn probe_api() -> SyntheticTouchpadApiProbe {
        SyntheticTouchpadApiProbe {
            platform: std::env::consts::OS,
            user32_loaded: false,
            load_error_code: None,
            symbols: REQUIRED_SYMBOLS.map(|name| ApiSymbolStatus {
                name,
                exported: false,
            }),
        }
    }

    pub(super) fn probe_device_creation(
        _parameters: SyntheticTouchpadParameters,
    ) -> DeviceCreationProbe {
        DeviceCreationProbe::UnsupportedPlatform
    }

    pub(super) fn create_injection_device(
        _parameters: SyntheticTouchpadParameters,
    ) -> Result<SyntheticTouchpadDevice, SyntheticTouchpadInjectionError> {
        Err(SyntheticTouchpadInjectionError::UnsupportedPlatform)
    }

    pub(super) fn inject_batch(
        _device: &mut SyntheticTouchpadDevice,
        _batch: &WindowsTouchpadBatch,
    ) -> Result<BatchInjectionOutcome, SyntheticTouchpadInjectionError> {
        Err(SyntheticTouchpadInjectionError::UnsupportedPlatform)
    }
}

#[cfg(windows)]
mod platform {
    use std::{ffi::c_void, mem};

    use windows_sys::{
        Win32::{
            Foundation::{FARPROC, FreeLibrary, GetLastError, HMODULE},
            System::LibraryLoader::{GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW},
            UI::Controls::POINTER_TYPE_INFO,
        },
        core::{PCSTR, PCWSTR},
    };

    use super::{
        ApiSymbolStatus, BatchInjectionOutcome, CREATE_DEVICE_SYMBOL, DESTROY_DEVICE_SYMBOL,
        DeviceCreationProbe, INJECT_POINTER_SYMBOL, NativeTouchpadBatch, REQUIRED_SYMBOLS,
        SyntheticTouchpadApiProbe, SyntheticTouchpadInjectionError, SyntheticTouchpadParameters,
        WindowsTouchpadBatch,
    };

    const USER32_DLL: [u16; 11] = [
        b'u' as u16,
        b's' as u16,
        b'e' as u16,
        b'r' as u16,
        b'3' as u16,
        b'2' as u16,
        b'.' as u16,
        b'd' as u16,
        b'l' as u16,
        b'l' as u16,
        0,
    ];

    const PT_TOUCHPAD: u32 = 5;
    const POINTER_FEEDBACK_NONE: u32 = 3;
    const SDCO_PHYSICAL_SIZE: u32 = 0x1;

    #[repr(C)]
    struct SyntheticDeviceCreationParams {
        pointer_type: u32,
        max_count: u32,
        feedback_mode: u32,
        monitor: *mut c_void,
        device_width: u32,
        device_height: u32,
        options: u32,
    }

    type CreateSyntheticPointerDevice2 =
        unsafe extern "system" fn(parameters: *const SyntheticDeviceCreationParams) -> *mut c_void;
    type DestroySyntheticPointerDevice = unsafe extern "system" fn(device: *mut c_void);
    type InjectSyntheticPointerInput = unsafe extern "system" fn(
        device: *mut c_void,
        pointer_info: *const POINTER_TYPE_INFO,
        count: u32,
    ) -> i32;

    pub(super) struct SyntheticTouchpadDevice {
        _user32: User32,
        handle: *mut c_void,
        inject: InjectSyntheticPointerInput,
        destroy: DestroySyntheticPointerDevice,
    }

    impl Drop for SyntheticTouchpadDevice {
        fn drop(&mut self) {
            if !self.handle.is_null() {
                // SAFETY: `handle` was returned by CreateSyntheticPointerDevice2,
                // remains owned by this object, and is destroyed exactly once
                // before the retained user32 module is unloaded.
                unsafe { (self.destroy)(self.handle) };
                self.handle = std::ptr::null_mut();
            }
        }
    }

    struct User32(HMODULE);

    impl User32 {
        fn load() -> Result<Self, u32> {
            // SAFETY: `USER32_DLL` is a valid, NUL-terminated UTF-16 string and
            // LOAD_LIBRARY_SEARCH_SYSTEM32 prevents current-directory lookup.
            let module = unsafe {
                LoadLibraryExW(
                    USER32_DLL.as_ptr() as PCWSTR,
                    std::ptr::null_mut(),
                    LOAD_LIBRARY_SEARCH_SYSTEM32,
                )
            };
            if module.is_null() {
                // SAFETY: immediately captures the calling thread's last-error
                // value after the failed Windows API call.
                Err(unsafe { GetLastError() })
            } else {
                Ok(Self(module))
            }
        }

        fn symbol(&self, symbol: &'static str) -> FARPROC {
            debug_assert!(symbol.is_ascii() && !symbol.as_bytes().contains(&0));
            let mut name = Vec::with_capacity(symbol.len() + 1);
            name.extend_from_slice(symbol.as_bytes());
            name.push(0);
            // SAFETY: `self.0` is a live module handle and `name` is a
            // NUL-terminated ASCII export name for the duration of the call.
            unsafe { GetProcAddress(self.0, name.as_ptr() as PCSTR) }
        }
    }

    impl Drop for User32 {
        fn drop(&mut self) {
            // SAFETY: the handle was returned by LoadLibraryExW and is released
            // exactly once by this owner after all resolved calls have ended.
            let _ = unsafe { FreeLibrary(self.0) };
        }
    }

    pub(super) fn probe_api() -> SyntheticTouchpadApiProbe {
        match User32::load() {
            Ok(user32) => SyntheticTouchpadApiProbe {
                platform: "windows",
                user32_loaded: true,
                load_error_code: None,
                symbols: REQUIRED_SYMBOLS.map(|name| ApiSymbolStatus {
                    name,
                    exported: user32.symbol(name).is_some(),
                }),
            },
            Err(error_code) => SyntheticTouchpadApiProbe {
                platform: "windows",
                user32_loaded: false,
                load_error_code: Some(error_code),
                symbols: REQUIRED_SYMBOLS.map(|name| ApiSymbolStatus {
                    name,
                    exported: false,
                }),
            },
        }
    }

    pub(super) fn probe_device_creation(
        parameters: SyntheticTouchpadParameters,
    ) -> DeviceCreationProbe {
        let user32 = match User32::load() {
            Ok(user32) => user32,
            Err(error_code) => return DeviceCreationProbe::User32LoadFailed { error_code },
        };

        let probe = SyntheticTouchpadApiProbe {
            platform: "windows",
            user32_loaded: true,
            load_error_code: None,
            symbols: REQUIRED_SYMBOLS.map(|name| ApiSymbolStatus {
                name,
                exported: user32.symbol(name).is_some(),
            }),
        };
        let missing = probe.missing_symbols();
        if !missing.is_empty() {
            return DeviceCreationProbe::MissingSymbols { symbols: missing };
        }

        let create = user32
            .symbol(CREATE_DEVICE_SYMBOL)
            .expect("availability checked above");
        let destroy = user32
            .symbol(DESTROY_DEVICE_SYMBOL)
            .expect("availability checked above");

        // SAFETY: GetProcAddress returned these exact documented user32 export
        // names. Windows function pointers share one representation; the
        // signatures match the Microsoft declarations used by this probe.
        let create: CreateSyntheticPointerDevice2 = unsafe { mem::transmute(create) };
        // SAFETY: same reasoning as for the create function above.
        let destroy: DestroySyntheticPointerDevice = unsafe { mem::transmute(destroy) };

        let native_parameters = SyntheticDeviceCreationParams {
            pointer_type: PT_TOUCHPAD,
            max_count: parameters.max_contacts,
            feedback_mode: POINTER_FEEDBACK_NONE,
            monitor: std::ptr::null_mut(),
            device_width: parameters.width_himetric,
            device_height: parameters.height_himetric,
            options: SDCO_PHYSICAL_SIZE,
        };
        // SAFETY: the structure is `repr(C)`, fully initialized, and lives for
        // the duration of this synchronous call. All values were validated
        // against the documented PT_TOUCHPAD constraints.
        let device = unsafe { create(&raw const native_parameters) };
        if device.is_null() {
            // SAFETY: immediately captures the error from CreateSyntheticPointerDevice2.
            return DeviceCreationProbe::CreationFailed {
                error_code: unsafe { GetLastError() },
            };
        }

        // SAFETY: `device` is the non-null handle returned above. No injection
        // occurs and the handle is destroyed exactly once before user32 unload.
        unsafe { destroy(device) };
        DeviceCreationProbe::CreatedAndDestroyed { parameters }
    }

    pub(super) fn create_injection_device(
        parameters: SyntheticTouchpadParameters,
    ) -> Result<SyntheticTouchpadDevice, SyntheticTouchpadInjectionError> {
        let user32 = User32::load().map_err(|error_code| {
            SyntheticTouchpadInjectionError::User32LoadFailed { error_code }
        })?;
        let required = [
            CREATE_DEVICE_SYMBOL,
            INJECT_POINTER_SYMBOL,
            DESTROY_DEVICE_SYMBOL,
        ];
        let missing = required
            .into_iter()
            .filter(|symbol| user32.symbol(symbol).is_none())
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(SyntheticTouchpadInjectionError::MissingSymbols { symbols: missing });
        }

        let create = user32
            .symbol(CREATE_DEVICE_SYMBOL)
            .expect("availability checked above");
        let inject = user32
            .symbol(INJECT_POINTER_SYMBOL)
            .expect("availability checked above");
        let destroy = user32
            .symbol(DESTROY_DEVICE_SYMBOL)
            .expect("availability checked above");
        // SAFETY: GetProcAddress returned the exact documented user32 export
        // names and the signatures match the Microsoft declarations.
        let create: CreateSyntheticPointerDevice2 = unsafe { mem::transmute(create) };
        // SAFETY: same signature reasoning as for `create`.
        let inject: InjectSyntheticPointerInput = unsafe { mem::transmute(inject) };
        // SAFETY: same signature reasoning as for `create`.
        let destroy: DestroySyntheticPointerDevice = unsafe { mem::transmute(destroy) };

        let native_parameters = SyntheticDeviceCreationParams {
            pointer_type: PT_TOUCHPAD,
            max_count: parameters.max_contacts,
            feedback_mode: POINTER_FEEDBACK_NONE,
            monitor: std::ptr::null_mut(),
            device_width: parameters.width_himetric,
            device_height: parameters.height_himetric,
            options: SDCO_PHYSICAL_SIZE,
        };
        // SAFETY: the repr(C) structure is fully initialized, its pointer is
        // valid for the synchronous call, and parameters were validated by the
        // public constructor before entering this platform function.
        let handle = unsafe { create(&raw const native_parameters) };
        if handle.is_null() {
            // SAFETY: immediately captures the creating thread's last error.
            return Err(SyntheticTouchpadInjectionError::CreationFailed {
                error_code: unsafe { GetLastError() },
            });
        }
        Ok(SyntheticTouchpadDevice {
            _user32: user32,
            handle,
            inject,
            destroy,
        })
    }

    pub(super) fn inject_batch(
        device: &mut SyntheticTouchpadDevice,
        batch: &WindowsTouchpadBatch,
    ) -> Result<BatchInjectionOutcome, SyntheticTouchpadInjectionError> {
        if batch.is_empty() {
            return Ok(BatchInjectionOutcome::SkippedEmpty);
        }
        let native = NativeTouchpadBatch::encode(batch);
        // SAFETY: the device handle is live, `native` contains `len` initialized
        // PT_TOUCHPAD structures, and both remain valid for this synchronous
        // call. The count is bounded to five by WindowsTouchpadBatch.
        let submitted =
            unsafe { (device.inject)(device.handle, native.as_ptr(), u32::from(native.len())) };
        if submitted == 0 {
            // SAFETY: immediately captures the failed submission's last error.
            return Err(SyntheticTouchpadInjectionError::SubmissionFailed {
                error_code: unsafe { GetLastError() },
            });
        }
        Ok(BatchInjectionOutcome::Submitted {
            contacts: native.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_parameters_are_a_five_contact_himetric_device() {
        let parameters = SyntheticTouchpadParameters::default();
        assert_eq!(parameters.max_contacts, 5);
        assert_eq!(
            (parameters.width_himetric, parameters.height_himetric),
            (10_000, 6_000)
        );
        assert_eq!(parameters.validate(), Ok(()));
    }

    #[test]
    fn parameter_validation_rejects_invalid_contact_counts_and_size() {
        for max_contacts in [0, 6, u32::MAX] {
            assert_eq!(
                SyntheticTouchpadParameters {
                    max_contacts,
                    ..SyntheticTouchpadParameters::default()
                }
                .validate(),
                Err(SyntheticTouchpadParameterError::InvalidContactCount(
                    max_contacts
                ))
            );
        }
        assert_eq!(
            SyntheticTouchpadParameters {
                width_himetric: 0,
                ..SyntheticTouchpadParameters::default()
            }
            .validate(),
            Err(SyntheticTouchpadParameterError::MissingPhysicalSize)
        );
    }

    #[test]
    fn availability_is_exactly_the_required_symbol_conjunction() {
        let probe = probe_synthetic_touchpad_api();
        assert_eq!(probe.symbols.map(|symbol| symbol.name), REQUIRED_SYMBOLS);
        assert_eq!(
            probe.is_available(),
            probe.user32_loaded && probe.symbols.iter().all(|symbol| symbol.exported)
        );
        assert_eq!(
            probe.missing_symbols(),
            probe
                .symbols
                .iter()
                .filter_map(|symbol| (!symbol.exported).then_some(symbol.name))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn invalid_parameters_never_reach_platform_device_creation() {
        assert!(matches!(
            probe_synthetic_touchpad_device_creation(SyntheticTouchpadParameters {
                max_contacts: 0,
                ..SyntheticTouchpadParameters::default()
            }),
            DeviceCreationProbe::InvalidParameters(
                SyntheticTouchpadParameterError::InvalidContactCount(0)
            )
        ));
        assert!(matches!(
            SyntheticTouchpadDevice::create(SyntheticTouchpadParameters {
                max_contacts: 0,
                ..SyntheticTouchpadParameters::default()
            }),
            Err(SyntheticTouchpadInjectionError::InvalidParameters(
                SyntheticTouchpadParameterError::InvalidContactCount(0)
            ))
        ));
    }
}
