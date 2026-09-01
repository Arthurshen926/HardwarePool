use std::{error::Error as StdError, fmt};

use capyio_windows_camera::{
    CAPYIO_CAMERA_SOURCE_CLSID, MfVirtualCameraAccess, MfVirtualCameraLifetime,
    MfVirtualCameraPlan, MfVirtualCameraRegistrationBackend,
};
use windows::{
    Win32::Media::{
        KernelStreaming::KSCATEGORY_VIDEO_CAMERA,
        MediaFoundation::{
            IMFAsyncCallback, IMFAttributes, IMFMediaSource, IMFVirtualCamera,
            MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK, MF_E_ATTRIBUTENOTFOUND,
            MFCreateVirtualCamera, MFVirtualCameraAccess_CurrentUser,
            MFVirtualCameraLifetime_Session, MFVirtualCameraType_SoftwareCameraSource,
        },
    },
    core::{Error, Interface, PCWSTR},
};

pub const MAX_VIRTUAL_CAMERA_SYMBOLIC_LINK_UTF16: usize = 4096;

#[derive(Default)]
pub struct WindowsMfVirtualCameraBackend {
    camera: Option<IMFVirtualCamera>,
    symbolic_link: Option<String>,
    started: bool,
}

impl WindowsMfVirtualCameraBackend {
    #[must_use]
    pub fn symbolic_link(&self) -> Option<&str> {
        self.symbolic_link.as_deref()
    }

    #[must_use]
    pub const fn is_prepared(&self) -> bool {
        self.camera.is_some()
    }

    #[must_use]
    pub const fn is_started(&self) -> bool {
        self.started
    }

    pub fn get_media_source(&self) -> Result<IMFMediaSource, MfVirtualCameraBackendError> {
        if !self.started {
            return Err(MfVirtualCameraBackendError::NotStarted);
        }
        let camera = self
            .camera
            .as_ref()
            .ok_or(MfVirtualCameraBackendError::NotPrepared)?;
        unsafe { camera.GetMediaSource() }.map_err(MfVirtualCameraBackendError::Windows)
    }

    pub fn remove(&mut self) -> Result<(), MfVirtualCameraBackendError> {
        let camera = self.camera()?.clone();
        unsafe { camera.Remove() }.map_err(MfVirtualCameraBackendError::Windows)?;
        self.started = false;
        self.symbolic_link = None;
        Ok(())
    }

    fn camera(&self) -> Result<&IMFVirtualCamera, MfVirtualCameraBackendError> {
        self.camera
            .as_ref()
            .ok_or(MfVirtualCameraBackendError::NotPrepared)
    }
}

impl MfVirtualCameraRegistrationBackend for WindowsMfVirtualCameraBackend {
    type Error = MfVirtualCameraBackendError;

    fn prepare(&mut self, plan: &MfVirtualCameraPlan) -> Result<(), Self::Error> {
        if self.camera.is_some() {
            return Err(MfVirtualCameraBackendError::AlreadyPrepared);
        }
        if plan.source_clsid() != CAPYIO_CAMERA_SOURCE_CLSID
            || plan.lifetime() != MfVirtualCameraLifetime::Session
            || plan.access() != MfVirtualCameraAccess::CurrentUser
        {
            return Err(MfVirtualCameraBackendError::UnsupportedPlan);
        }

        let friendly_name = wide_null(plan.friendly_name());
        let source_id = wide_null(plan.source_clsid());
        let categories = [KSCATEGORY_VIDEO_CAMERA];
        let camera = unsafe {
            MFCreateVirtualCamera(
                MFVirtualCameraType_SoftwareCameraSource,
                MFVirtualCameraLifetime_Session,
                MFVirtualCameraAccess_CurrentUser,
                PCWSTR::from_raw(friendly_name.as_ptr()),
                PCWSTR::from_raw(source_id.as_ptr()),
                Some(&categories),
            )
        }
        .map_err(MfVirtualCameraBackendError::Windows)?;
        let attributes: IMFAttributes = camera
            .cast()
            .map_err(MfVirtualCameraBackendError::Windows)?;
        let existing_symbolic_link = match unsafe {
            attributes.GetStringLength(&MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK)
        } {
            Ok(_) => Some(read_bounded_string(
                &attributes,
                &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
            )?),
            Err(error) if error.code() == MF_E_ATTRIBUTENOTFOUND => None,
            Err(error) => return Err(MfVirtualCameraBackendError::Windows(error)),
        };
        self.camera = Some(camera);
        self.symbolic_link = existing_symbolic_link;
        self.started = false;
        Ok(())
    }

    fn start(&mut self) -> Result<(), Self::Error> {
        if self.started {
            return Err(MfVirtualCameraBackendError::AlreadyStarted);
        }
        let camera = self.camera()?.clone();
        unsafe { camera.Start(None::<&IMFAsyncCallback>) }
            .map_err(MfVirtualCameraBackendError::Windows)?;
        self.started = true;
        let attributes: IMFAttributes = camera
            .cast()
            .map_err(MfVirtualCameraBackendError::Windows)?;
        self.symbolic_link = Some(read_bounded_string(
            &attributes,
            &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
        )?);
        Ok(())
    }

    fn stop(&mut self) -> Result<(), Self::Error> {
        if !self.started {
            return Err(MfVirtualCameraBackendError::NotStarted);
        }
        unsafe { self.camera()?.Stop() }.map_err(MfVirtualCameraBackendError::Windows)?;
        self.started = false;
        Ok(())
    }

    fn shutdown(&mut self) -> Result<(), Self::Error> {
        let Some(camera) = self.camera.as_ref() else {
            self.started = false;
            return Ok(());
        };
        unsafe { camera.Shutdown() }.map_err(MfVirtualCameraBackendError::Windows)?;
        self.camera = None;
        self.started = false;
        Ok(())
    }
}

impl Drop for WindowsMfVirtualCameraBackend {
    fn drop(&mut self) {
        if let Some(camera) = self.camera.as_ref() {
            if self.started {
                let _ = unsafe { camera.Stop() };
            }
            let _ = unsafe { camera.Shutdown() };
        }
    }
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}

fn read_bounded_string(
    attributes: &IMFAttributes,
    key: *const windows::core::GUID,
) -> Result<String, MfVirtualCameraBackendError> {
    let length = unsafe { attributes.GetStringLength(key) }
        .map_err(MfVirtualCameraBackendError::Windows)? as usize;
    if length == 0 || length > MAX_VIRTUAL_CAMERA_SYMBOLIC_LINK_UTF16 {
        return Err(MfVirtualCameraBackendError::InvalidSymbolicLinkLength {
            utf16_units: length,
        });
    }
    let mut buffer = vec![0_u16; length + 1];
    let mut written = 0_u32;
    unsafe { attributes.GetString(key, &mut buffer, Some(&mut written)) }
        .map_err(MfVirtualCameraBackendError::Windows)?;
    let written = written as usize;
    if written != length || buffer[length] != 0 {
        return Err(MfVirtualCameraBackendError::InvalidSymbolicLinkLength {
            utf16_units: written,
        });
    }
    String::from_utf16(&buffer[..length])
        .map_err(|_| MfVirtualCameraBackendError::InvalidSymbolicLinkUtf16)
}

#[derive(Debug)]
pub enum MfVirtualCameraBackendError {
    Windows(Error),
    AlreadyPrepared,
    AlreadyStarted,
    NotPrepared,
    NotStarted,
    UnsupportedPlan,
    InvalidSymbolicLinkLength { utf16_units: usize },
    InvalidSymbolicLinkUtf16,
}

impl fmt::Display for MfVirtualCameraBackendError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Windows(error) => write!(formatter, "Media Foundation call failed: {error}"),
            Self::AlreadyPrepared => formatter.write_str("virtual camera is already prepared"),
            Self::AlreadyStarted => formatter.write_str("virtual camera is already started"),
            Self::NotPrepared => formatter.write_str("virtual camera is not prepared"),
            Self::NotStarted => formatter.write_str("virtual camera is not started"),
            Self::UnsupportedPlan => formatter.write_str(
                "only the CapyIO session/current-user SoftwareCameraSource plan is supported",
            ),
            Self::InvalidSymbolicLinkLength { utf16_units } => write!(
                formatter,
                "virtual camera symbolic link has invalid UTF-16 length {utf16_units}"
            ),
            Self::InvalidSymbolicLinkUtf16 => {
                formatter.write_str("virtual camera symbolic link is not valid UTF-16")
            }
        }
    }
}

impl StdError for MfVirtualCameraBackendError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Windows(error) => Some(error),
            _ => None,
        }
    }
}
