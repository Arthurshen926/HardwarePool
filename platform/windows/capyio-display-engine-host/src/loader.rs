use std::{
    ffi::{OsStr, c_void},
    fmt,
    fs::File,
    io::{self, Read},
    mem::{self, size_of},
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    ptr,
    sync::atomic::{Ordering, compiler_fence},
};

use capyio_display_stream_adapter::{
    ValidatedDisplayStreamSession,
    engine_abi::{
        ENGINE_ABI_V2, EngineAbiError, EngineCallStatus, EngineEventV2, EngineLaunchMaterialV1,
        EngineProbeV2, EngineStartV2, ValidatedEngineProbe, WindowsCaptureOutput,
    },
    lifecycle::{EngineLifecycle, EngineLifecycleError, EngineLifecycleState},
};
use capyio_windows_display::{
    DisplayOutputBindingError, IddConnectedMonitor, IddMonitorMode,
    VerifiedInteractiveDisplaySession, VerifiedWindowsDisplayOutput,
    verify_owned_windows_display_output, verify_static_capyio_display_output,
};
use sha2::{Digest, Sha256};
use windows_sys::Win32::{
    Foundation::{FreeLibrary, HMODULE},
    System::LibraryLoader::{
        GetProcAddress, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, LOAD_LIBRARY_SEARCH_SYSTEM32,
        LoadLibraryExW,
    },
};

use crate::{
    DisplayWorkerLease, DisplayWorkerProtocolError, DisplayWorkerStartRecord, EngineEventBatch,
    event_queue::EngineEventQueue,
};

type EventCallback = unsafe extern "C" fn(*mut c_void, *const EngineEventV2);
type ProbeFn = unsafe extern "C" fn(*mut EngineProbeV2) -> i32;
type CreateFn = unsafe extern "C" fn(*const EngineCallbacksV2, *mut *mut c_void) -> i32;
type StartFn = unsafe extern "C" fn(*mut c_void, *const EngineStartV2) -> i32;
type AuthenticatedStartFn =
    unsafe extern "C" fn(*mut c_void, *const EngineStartV2, *const EngineLaunchMaterialV1) -> i32;
type RequestKeyFrameFn = unsafe extern "C" fn(*mut c_void) -> i32;
type StopFn = unsafe extern "C" fn(*mut c_void, u32) -> i32;
type DestroyFn = unsafe extern "C" fn(*mut c_void, u32) -> i32;
type GetApiFn = unsafe extern "C" fn(*mut EngineApiV2) -> i32;

#[derive(Clone, Copy)]
#[repr(C)]
struct EngineCallbacksV2 {
    struct_size: u32,
    abi_version: u32,
    on_event: Option<EventCallback>,
    context: *mut c_void,
}

#[derive(Clone, Copy)]
#[repr(C)]
struct EngineApiV2 {
    struct_size: u32,
    abi_version: u32,
    probe: Option<ProbeFn>,
    create: Option<CreateFn>,
    start: Option<StartFn>,
    request_key_frame: Option<RequestKeyFrameFn>,
    stop: Option<StopFn>,
    destroy: Option<DestroyFn>,
    reserved: [*mut c_void; 4],
}

impl EngineApiV2 {
    const fn empty() -> Self {
        Self {
            struct_size: size_of::<Self>() as u32,
            abi_version: ENGINE_ABI_V2,
            probe: None,
            create: None,
            start: None,
            request_key_frame: None,
            stop: None,
            destroy: None,
            reserved: [ptr::null_mut(); 4],
        }
    }

    fn validate(&self) -> Result<(), DisplayEngineLoadError> {
        if self.struct_size != size_of::<Self>() as u32 || self.abi_version != ENGINE_ABI_V2 {
            return Err(DisplayEngineLoadError::ApiLayoutMismatch);
        }
        if self.probe.is_none()
            || self.create.is_none()
            || self.start.is_none()
            || self.request_key_frame.is_none()
            || self.stop.is_none()
            || self.destroy.is_none()
            || self.reserved.iter().any(|value| !value.is_null())
        {
            return Err(DisplayEngineLoadError::IncompleteApiTable);
        }
        Ok(())
    }
}

struct ConstructionModule(HMODULE);

impl Drop for ConstructionModule {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: the guard owns the successful LoadLibraryExW reference and
            // no engine handle can exist while construction is incomplete.
            unsafe { FreeLibrary(self.0) };
        }
    }
}

pub struct DisplayEngineLibrary {
    module: HMODULE,
    canonical_path: PathBuf,
    probe: ValidatedEngineProbe,
    api: EngineApiV2,
    authenticated_start: AuthenticatedStartFn,
    safe_to_unload: bool,
}

impl DisplayEngineLibrary {
    pub fn load_verified(
        path: &Path,
        expected_sha256: [u8; 32],
    ) -> Result<Self, DisplayEngineLoadError> {
        validate_requested_path(path)?;
        let canonical_path = path.canonicalize().map_err(DisplayEngineLoadError::File)?;
        validate_requested_path(&canonical_path)?;
        if hash_file(&canonical_path)? != expected_sha256 {
            return Err(DisplayEngineLoadError::HashMismatch);
        }

        let wide_path = wide_null(&canonical_path)?;
        // SAFETY: the path is absolute, canonical, NUL-free and points to the
        // hash-verified file. Search is limited to the DLL directory and System32.
        let module = unsafe {
            LoadLibraryExW(
                wide_path.as_ptr(),
                ptr::null_mut(),
                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
            )
        };
        if module.is_null() {
            return Err(DisplayEngineLoadError::Load(io::Error::last_os_error()));
        }
        let guard = ConstructionModule(module);

        // SAFETY: module is a live library reference owned by guard and the
        // exported ASCII symbol is NUL-terminated.
        let raw_symbol =
            unsafe { GetProcAddress(module, c"capyio_display_engine_get_api_v2".as_ptr().cast()) }
                .ok_or(DisplayEngineLoadError::MissingEntrypoint)?;
        // SAFETY: the named ABI entrypoint has the fixed signature declared by
        // capyio_display_engine_v1.h; its returned v2 table is validated before use.
        let get_api: GetApiFn = unsafe { mem::transmute(raw_symbol) };
        let mut api = EngineApiV2::empty();
        // SAFETY: api points to writable ABI-v2 storage for the duration of the call.
        validate_call_status(unsafe { get_api(&mut api) })?;
        api.validate()?;

        // This separate versioned extension keeps ABI v2 frozen. Falling back
        // to its unauthenticated start function is forbidden.
        let authenticated_start_symbol = unsafe {
            GetProcAddress(
                module,
                c"capyio_display_engine_start_authenticated_v1"
                    .as_ptr()
                    .cast(),
            )
        }
        .ok_or(DisplayEngineLoadError::MissingAuthenticatedStartEntrypoint)?;
        // SAFETY: the fixed symbol signature is declared by the shared header.
        let authenticated_start: AuthenticatedStartFn =
            unsafe { mem::transmute(authenticated_start_symbol) };

        let mut raw_probe = EngineProbeV2::display_only_baseline(1);
        // SAFETY: validation above proved probe is non-null and raw_probe is valid
        // writable ABI-v2 storage.
        validate_call_status(unsafe { api.probe.unwrap()(&mut raw_probe) })?;
        let probe = raw_probe.validate().map_err(DisplayEngineLoadError::Abi)?;

        let module = guard.0;
        mem::forget(guard);
        Ok(Self {
            module,
            canonical_path,
            probe,
            api,
            authenticated_start,
            safe_to_unload: true,
        })
    }

    #[must_use]
    pub fn canonical_path(&self) -> &Path {
        &self.canonical_path
    }

    #[must_use]
    pub const fn probe(&self) -> ValidatedEngineProbe {
        self.probe
    }

    pub fn start_session(
        &mut self,
        session: &ValidatedDisplayStreamSession,
        monitor: IddConnectedMonitor,
        execution_session: VerifiedInteractiveDisplaySession,
        launch: AuthenticatedEngineLaunch,
    ) -> Result<DisplayEngineSession<'_>, DisplayEngineLoadError> {
        let requested = session.request().mode;
        let mode = IddMonitorMode {
            width: requested.width,
            height: requested.height,
            refresh_millihz: requested.refresh_millihertz,
        };
        let binding = verify_owned_windows_display_output(monitor, mode)
            .map_err(DisplayEngineLoadError::OutputBinding)?;
        self.start_verified_output_session(session, &binding, execution_session, launch)
    }

    pub fn start_verified_output_session(
        &mut self,
        session: &ValidatedDisplayStreamSession,
        binding: &VerifiedWindowsDisplayOutput,
        execution_session: VerifiedInteractiveDisplaySession,
        launch: AuthenticatedEngineLaunch,
    ) -> Result<DisplayEngineSession<'_>, DisplayEngineLoadError> {
        if session.request().display_capability_id
            != *binding.identity().capability_id().as_uuid().as_bytes()
        {
            return Err(DisplayEngineLoadError::BindingIdentityMismatch);
        }
        let requested_mode = session.request().mode;
        let bound_mode = binding.mode();
        if requested_mode.width != bound_mode.width
            || requested_mode.height != bound_mode.height
            || requested_mode.refresh_millihertz != bound_mode.refresh_millihz
        {
            return Err(DisplayEngineLoadError::BindingModeMismatch);
        }
        let capture_output = WindowsCaptureOutput::parse(binding.gdi_device_name())
            .map_err(DisplayEngineLoadError::Abi)?;
        self.start_session_with_output(session, capture_output, Some(execution_session), launch)
    }

    /// Resolves the installed static 0.1.1 CapyIO output inside the already
    /// authorized interactive worker immediately before native creation.
    pub fn start_static_session(
        &mut self,
        session: &ValidatedDisplayStreamSession,
        execution_session: VerifiedInteractiveDisplaySession,
        launch: AuthenticatedEngineLaunch,
    ) -> Result<DisplayEngineSession<'_>, DisplayEngineLoadError> {
        let request = session.request();
        let identity = capyio_windows_display::IddMonitorIdentity::from_capability_bytes(
            request.display_capability_id,
        )
        .map_err(|_| DisplayEngineLoadError::InvalidStaticIdentity)?;
        let mode = IddMonitorMode {
            width: request.mode.width,
            height: request.mode.height,
            refresh_millihz: request.mode.refresh_millihertz,
        };
        let binding = verify_static_capyio_display_output(identity, mode)
            .map_err(DisplayEngineLoadError::OutputBinding)?;
        self.start_verified_output_session(session, &binding, execution_session, launch)
    }

    /// Exercises the native lifecycle without a real IDD monitor. This escape
    /// hatch exists only in explicitly feature-gated fixture binaries.
    ///
    /// # Safety
    ///
    /// The caller must use this only for a non-production ABI fixture and must
    /// not treat `capture_output` as a capability ownership authorization.
    #[cfg(feature = "fixture-probe")]
    pub unsafe fn start_fixture_session(
        &mut self,
        session: &ValidatedDisplayStreamSession,
        capture_output: WindowsCaptureOutput,
        launch: AuthenticatedEngineLaunch,
    ) -> Result<DisplayEngineSession<'_>, DisplayEngineLoadError> {
        self.start_session_with_output(session, capture_output, None, launch)
    }

    fn start_session_with_output(
        &mut self,
        session: &ValidatedDisplayStreamSession,
        capture_output: WindowsCaptureOutput,
        execution_session: Option<VerifiedInteractiveDisplaySession>,
        launch: AuthenticatedEngineLaunch,
    ) -> Result<DisplayEngineSession<'_>, DisplayEngineLoadError> {
        #[cfg(not(feature = "fixture-probe"))]
        debug_assert!(execution_session.is_some());
        let _execution_session = execution_session;
        if !self.safe_to_unload {
            return Err(DisplayEngineLoadError::EngineMayStillExecute);
        }
        launch.validate_for(session)?;
        let start = EngineStartV2::new(self.probe, session, capture_output)
            .map_err(DisplayEngineLoadError::Abi)?;
        let mut lifecycle = EngineLifecycle::from_validated_probe(self.probe);
        lifecycle.begin_create()?;
        let mut events = Box::new(EngineEventQueue::new(
            session.request().adapter_instance_id,
            session.request().display_capability_id,
        ));
        let callbacks = EngineCallbacksV2 {
            struct_size: size_of::<EngineCallbacksV2>() as u32,
            abi_version: ENGINE_ABI_V2,
            on_event: Some(enqueue_event),
            context: (&mut *events as *mut EngineEventQueue).cast(),
        };
        let mut handle = ptr::null_mut();
        // SAFETY: the API table was validated, callback storage is pinned by
        // Box, and output points to writable handle storage.
        let create_status =
            match parse_call_status(unsafe { self.api.create.unwrap()(&callbacks, &mut handle) }) {
                Ok(status) => status,
                Err(error) => {
                    // An unknown return value is an ABI violation, so neither
                    // a null handle nor the documented failure contract proves
                    // that native code discarded the callback context.
                    self.safe_to_unload = false;
                    Box::leak(events);
                    return Err(error);
                }
            };
        if create_status != EngineCallStatus::Ok {
            let lifecycle_error = lifecycle.complete_create(create_status).unwrap_err();
            if !handle.is_null() {
                self.safe_to_unload = false;
                Box::leak(events);
                return Err(DisplayEngineLoadError::HandleOnFailedCreate);
            }
            return Err(DisplayEngineLoadError::Lifecycle(lifecycle_error));
        }
        if handle.is_null() {
            // Success without an ownership handle cannot be cleaned up. Keep
            // both callback storage and code mapped in case the foreign engine
            // retained the callback despite violating the ABI contract.
            self.safe_to_unload = false;
            Box::leak(events);
            return Err(DisplayEngineLoadError::MissingCreatedHandle);
        }
        lifecycle.complete_create(create_status)?;
        self.safe_to_unload = false;
        let prepared = lifecycle.prepare_start(session, capture_output)?;
        debug_assert_eq!(prepared, start);
        // SAFETY: handle came from the validated create function, start is a
        // validated ABI-v2 record, launch is valid and Route-bound, and both
        // inputs remain alive for this atomic extension call.
        let start_status = match parse_call_status(unsafe {
            (self.authenticated_start)(handle, &start, &launch.material)
        }) {
            Ok(status) => status,
            Err(error) => {
                let failed = DisplayEngineSession {
                    library: self,
                    handle,
                    lifecycle,
                    events: Some(events),
                };
                drop(failed);
                return Err(error);
            }
        };
        if let Err(error) = lifecycle.complete_start(start_status) {
            let mut failed = DisplayEngineSession {
                library: self,
                handle,
                lifecycle,
                events: Some(events),
            };
            let _ = failed.shutdown(MAX_OPERATION_TIMEOUT_MILLIS);
            return Err(DisplayEngineLoadError::Lifecycle(error));
        }

        Ok(DisplayEngineSession {
            library: self,
            handle,
            lifecycle,
            events: Some(events),
        })
    }

    pub fn try_unload(&mut self) -> Result<(), DisplayEngineLoadError> {
        if !self.safe_to_unload {
            return Err(DisplayEngineLoadError::EngineMayStillExecute);
        }
        if self.module.is_null() {
            return Ok(());
        }
        // SAFETY: lifecycle proves no native handle or callback can remain and
        // this object owns the LoadLibraryExW reference.
        if unsafe { FreeLibrary(self.module) } == 0 {
            return Err(DisplayEngineLoadError::Unload(io::Error::last_os_error()));
        }
        self.module = ptr::null_mut();
        Ok(())
    }
}

/// One-shot, Route-bound secret material received only after peer and Route
/// authentication. It deliberately implements neither `Clone` nor `Debug`.
pub struct AuthenticatedEngineLaunch {
    adapter_instance_id: [u8; 16],
    display_capability_id: [u8; 16],
    material: EngineLaunchMaterialV1,
}

impl AuthenticatedEngineLaunch {
    pub fn new(
        adapter_instance_id: [u8; 16],
        display_capability_id: [u8; 16],
        launch_session_id: u32,
        key: [u8; 16],
        key_id: u32,
        ping_payload: [u8; 8],
        control_connect_data: u32,
    ) -> Result<Self, DisplayEngineLoadError> {
        if adapter_instance_id == [0; 16] || display_capability_id == [0; 16] {
            return Err(DisplayEngineLoadError::InvalidLaunchIdentity);
        }
        let material = EngineLaunchMaterialV1::new(
            launch_session_id,
            key,
            key_id,
            ping_payload,
            control_connect_data,
        );
        material.validate().map_err(DisplayEngineLoadError::Abi)?;
        Ok(Self {
            adapter_instance_id,
            display_capability_id,
            material,
        })
    }

    pub(crate) fn validate_for(
        &self,
        session: &ValidatedDisplayStreamSession,
    ) -> Result<(), DisplayEngineLoadError> {
        self.material
            .validate()
            .map_err(DisplayEngineLoadError::Abi)?;
        let request = session.request();
        if self.adapter_instance_id != request.adapter_instance_id
            || self.display_capability_id != request.display_capability_id
        {
            return Err(DisplayEngineLoadError::LaunchIdentityMismatch);
        }
        Ok(())
    }

    pub fn into_worker_start_record(
        self,
        lease: &DisplayWorkerLease,
    ) -> Result<DisplayWorkerStartRecord, DisplayWorkerProtocolError> {
        DisplayWorkerStartRecord::new(
            lease,
            self.adapter_instance_id,
            self.display_capability_id,
            &self.material,
        )
    }
}

impl Drop for AuthenticatedEngineLaunch {
    fn drop(&mut self) {
        // SAFETY: the object is exclusively borrowed during Drop. Volatile
        // writes plus a compiler fence prevent removal of this best-effort
        // in-process secret wipe.
        unsafe {
            let bytes = ptr::from_mut(&mut self.material).cast::<u8>();
            for index in 0..size_of::<EngineLaunchMaterialV1>() {
                ptr::write_volatile(bytes.add(index), 0);
            }
        }
        compiler_fence(Ordering::SeqCst);
    }
}

const MAX_OPERATION_TIMEOUT_MILLIS: u32 = 10_000;

pub struct DisplayEngineSession<'library> {
    library: &'library mut DisplayEngineLibrary,
    handle: *mut c_void,
    lifecycle: EngineLifecycle,
    events: Option<Box<EngineEventQueue>>,
}

impl DisplayEngineSession<'_> {
    #[must_use]
    pub fn lifecycle_state(&self) -> EngineLifecycleState {
        self.lifecycle.state()
    }

    #[must_use]
    pub fn drain_events(&self) -> EngineEventBatch {
        self.events.as_ref().unwrap().drain()
    }

    pub fn request_key_frame(&mut self) -> Result<(), DisplayEngineLoadError> {
        if self.lifecycle.state() != EngineLifecycleState::Running {
            return Err(DisplayEngineLoadError::SessionNotRunning);
        }
        // SAFETY: lifecycle and handle prove a running native engine instance.
        let status =
            parse_call_status(unsafe { self.library.api.request_key_frame.unwrap()(self.handle) })?;
        require_ok(status)
    }

    pub fn stop(&mut self, timeout_millis: u32) -> Result<(), DisplayEngineLoadError> {
        self.lifecycle.begin_stop(timeout_millis)?;
        // SAFETY: lifecycle proves the handle is running and timeout was bounded.
        let status = parse_call_status(unsafe {
            self.library.api.stop.unwrap()(self.handle, timeout_millis)
        })?;
        self.lifecycle.complete_stop(status)?;
        Ok(())
    }

    pub fn destroy(&mut self, timeout_millis: u32) -> Result<(), DisplayEngineLoadError> {
        self.lifecycle.begin_destroy(timeout_millis)?;
        // SAFETY: lifecycle proves the handle exists and no running session is
        // accepted in this state.
        let status = parse_call_status(unsafe {
            self.library.api.destroy.unwrap()(self.handle, timeout_millis)
        })?;
        self.lifecycle.complete_destroy(status)?;
        self.handle = ptr::null_mut();
        self.library.safe_to_unload = true;
        Ok(())
    }

    pub fn shutdown(&mut self, timeout_millis: u32) -> Result<(), DisplayEngineLoadError> {
        if self.lifecycle.state() == EngineLifecycleState::Running {
            self.stop(timeout_millis)?;
        }
        if matches!(
            self.lifecycle.state(),
            EngineLifecycleState::Ready | EngineLifecycleState::Failed
        ) {
            self.destroy(timeout_millis)?;
        }
        if self.lifecycle.state() == EngineLifecycleState::Destroyed {
            Ok(())
        } else {
            Err(DisplayEngineLoadError::SessionNotQuiescent)
        }
    }
}

impl Drop for DisplayEngineSession<'_> {
    fn drop(&mut self) {
        if self.shutdown(MAX_OPERATION_TIMEOUT_MILLIS).is_err() {
            if let Some(events) = self.events.take() {
                Box::leak(events);
            }
            self.library.safe_to_unload = false;
        }
    }
}

unsafe extern "C" fn enqueue_event(context: *mut c_void, event: *const EngineEventV2) {
    if context.is_null() {
        return;
    }
    // SAFETY: create receives the stable Box allocation as callback context and
    // it remains alive or is deliberately leaked until callbacks quiesce.
    let queue = unsafe { &*context.cast::<EngineEventQueue>() };
    queue.accept_callback(event);
}

impl Drop for DisplayEngineLibrary {
    fn drop(&mut self) {
        // Deliberately retain the module when explicit lifecycle-safe unload was
        // not completed. Unmapping potentially executing foreign code is worse
        // than retaining one module reference until process exit.
    }
}

fn validate_requested_path(path: &Path) -> Result<(), DisplayEngineLoadError> {
    if !path.is_absolute() {
        return Err(DisplayEngineLoadError::PathNotAbsolute);
    }
    let text = path.as_os_str().to_string_lossy();
    let extended_unc = text
        .get(..8)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(r"\\?\UNC\"));
    let ordinary_unc = text.starts_with("\\\\") && !text.starts_with(r"\\?\");
    if extended_unc || ordinary_unc || text.starts_with("//") {
        return Err(DisplayEngineLoadError::NetworkPathRejected);
    }
    if path.extension().and_then(OsStr::to_str) != Some("dll") {
        return Err(DisplayEngineLoadError::NotDll);
    }
    if !path.is_file() {
        return Err(DisplayEngineLoadError::NotFile);
    }
    Ok(())
}

fn hash_file(path: &Path) -> Result<[u8; 32], DisplayEngineLoadError> {
    let mut file = File::open(path).map_err(DisplayEngineLoadError::File)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(DisplayEngineLoadError::File)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(digest.finalize().into())
}

fn wide_null(path: &Path) -> Result<Vec<u16>, DisplayEngineLoadError> {
    let mut value: Vec<u16> = path.as_os_str().encode_wide().collect();
    if value.contains(&0) {
        return Err(DisplayEngineLoadError::InteriorNul);
    }
    value.push(0);
    Ok(value)
}

fn validate_call_status(raw: i32) -> Result<(), DisplayEngineLoadError> {
    require_ok(parse_call_status(raw)?)
}

fn parse_call_status(raw: i32) -> Result<EngineCallStatus, DisplayEngineLoadError> {
    EngineCallStatus::from_raw(raw).map_err(DisplayEngineLoadError::Abi)
}

fn require_ok(status: EngineCallStatus) -> Result<(), DisplayEngineLoadError> {
    if status == EngineCallStatus::Ok {
        Ok(())
    } else {
        Err(DisplayEngineLoadError::Native(status))
    }
}

#[derive(Debug)]
pub enum DisplayEngineLoadError {
    PathNotAbsolute,
    NetworkPathRejected,
    NotDll,
    NotFile,
    InteriorNul,
    File(io::Error),
    HashMismatch,
    Load(io::Error),
    MissingEntrypoint,
    MissingAuthenticatedStartEntrypoint,
    ApiLayoutMismatch,
    IncompleteApiTable,
    HandleOnFailedCreate,
    MissingCreatedHandle,
    SessionNotRunning,
    BindingIdentityMismatch,
    BindingModeMismatch,
    InvalidStaticIdentity,
    InvalidLaunchIdentity,
    LaunchIdentityMismatch,
    LaunchRecord(String),
    LaunchTransport(String),
    OutputBinding(DisplayOutputBindingError),
    SessionNotQuiescent,
    Abi(EngineAbiError),
    Lifecycle(EngineLifecycleError),
    Native(EngineCallStatus),
    EngineMayStillExecute,
    Unload(io::Error),
}

impl fmt::Display for DisplayEngineLoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PathNotAbsolute => formatter.write_str("display engine path must be absolute"),
            Self::NetworkPathRejected => formatter.write_str("network DLL paths are rejected"),
            Self::NotDll => formatter.write_str("display engine path must have a .dll extension"),
            Self::NotFile => formatter.write_str("display engine path must be an existing file"),
            Self::InteriorNul => formatter.write_str("display engine path contains NUL"),
            Self::File(error) => write!(formatter, "display engine file error: {error}"),
            Self::HashMismatch => formatter.write_str("display engine SHA-256 mismatch"),
            Self::Load(error) => write!(formatter, "LoadLibraryExW failed: {error}"),
            Self::MissingEntrypoint => formatter.write_str("display engine entrypoint is missing"),
            Self::MissingAuthenticatedStartEntrypoint => {
                formatter.write_str("display engine authenticated-start extension is missing")
            }
            Self::ApiLayoutMismatch => {
                formatter.write_str("display engine API table layout mismatch")
            }
            Self::IncompleteApiTable => {
                formatter.write_str("display engine API table is incomplete")
            }
            Self::HandleOnFailedCreate => {
                formatter.write_str("display engine returned a handle after failed create")
            }
            Self::MissingCreatedHandle => {
                formatter.write_str("display engine create succeeded without a handle")
            }
            Self::SessionNotRunning => formatter.write_str("display engine session is not running"),
            Self::BindingIdentityMismatch => {
                formatter.write_str("verified display output belongs to another display Capability")
            }
            Self::BindingModeMismatch => formatter
                .write_str("verified display output mode differs from the engine session mode"),
            Self::InvalidStaticIdentity => {
                formatter.write_str("static display Capability identity is invalid")
            }
            Self::InvalidLaunchIdentity => {
                formatter.write_str("authenticated launch identity must be nonzero")
            }
            Self::LaunchIdentityMismatch => {
                formatter.write_str("authenticated launch belongs to another Adapter or Capability")
            }
            Self::LaunchRecord(error) => {
                write!(formatter, "GameStream launch record failed: {error}")
            }
            Self::LaunchTransport(error) => {
                write!(formatter, "GameStream launch exchange failed: {error}")
            }
            Self::OutputBinding(error) => {
                write!(formatter, "display output binding failed: {error}")
            }
            Self::SessionNotQuiescent => {
                formatter.write_str("display engine session is not proven quiescent")
            }
            Self::Abi(error) => write!(formatter, "display engine ABI rejected: {error}"),
            Self::Lifecycle(error) => {
                write!(formatter, "display engine lifecycle rejected: {error}")
            }
            Self::Native(status) => write!(formatter, "display engine returned {status:?}"),
            Self::EngineMayStillExecute => formatter.write_str("display engine may still execute"),
            Self::Unload(error) => write!(formatter, "FreeLibrary failed: {error}"),
        }
    }
}

impl std::error::Error for DisplayEngineLoadError {}

impl From<EngineLifecycleError> for DisplayEngineLoadError {
    fn from(value: EngineLifecycleError) -> Self {
        Self::Lifecycle(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use capyio_display_stream_adapter::{
        DisplayMode, DisplayOnlyPolicy, DisplayStreamSessionRequest, DynamicRange, VideoCodec,
    };

    #[test]
    fn relative_network_and_non_dll_paths_fail_before_loading() {
        assert_eq!(size_of::<EngineCallbacksV2>(), 24);
        assert_eq!(size_of::<EngineApiV2>(), 88);
        assert!(matches!(
            DisplayEngineLibrary::load_verified(Path::new("fixture.dll"), [0; 32]),
            Err(DisplayEngineLoadError::PathNotAbsolute)
        ));
        assert!(matches!(
            DisplayEngineLibrary::load_verified(Path::new(r"\\server\share\fixture.dll"), [0; 32]),
            Err(DisplayEngineLoadError::NetworkPathRejected)
        ));
        assert!(matches!(
            DisplayEngineLibrary::load_verified(Path::new(r"C:\fixture.exe"), [0; 32]),
            Err(DisplayEngineLoadError::NotDll)
        ));
    }

    #[test]
    fn authenticated_launch_rejects_nil_and_cross_route_identity() {
        assert!(matches!(
            AuthenticatedEngineLaunch::new([0; 16], [2; 16], 7, [3; 16], 4, [5; 8], 6),
            Err(DisplayEngineLoadError::InvalidLaunchIdentity)
        ));
        let session = ValidatedDisplayStreamSession::new(DisplayStreamSessionRequest {
            adapter_instance_id: [1; 16],
            display_capability_id: [2; 16],
            mode: DisplayMode {
                width: 2640,
                height: 1216,
                refresh_millihertz: 120_000,
            },
            codec: VideoCodec::Hevc,
            dynamic_range: DynamicRange::Sdr8,
            bitrate_kbps: 45_000,
            port_base: 50_000,
            policy: DisplayOnlyPolicy::CAPYIO,
        })
        .unwrap();
        let launch =
            AuthenticatedEngineLaunch::new([9; 16], [2; 16], 7, [3; 16], 4, [5; 8], 6).unwrap();
        assert!(matches!(
            launch.validate_for(&session),
            Err(DisplayEngineLoadError::LaunchIdentityMismatch)
        ));
    }
}
