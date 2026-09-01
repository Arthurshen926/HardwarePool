use std::{
    ffi::c_void,
    ptr,
    sync::{Mutex, MutexGuard},
};

use windows::{
    Win32::{
        Foundation::{E_POINTER, E_UNEXPECTED},
        Media::MediaFoundation::{
            IMFActivate, IMFActivate_Impl, IMFAttributes, IMFAttributes_Impl, IMFAttributes_Vtbl,
            MF_ATTRIBUTE_TYPE, MF_ATTRIBUTES_MATCH_TYPE,
            MF_VIRTUALCAMERA_PROVIDE_ASSOCIATED_CAMERA_SOURCES, MFCreateAttributes,
        },
        System::Com::StructuredStorage::PROPVARIANT,
    },
    core::{BOOL, Error, GUID, IUnknown, Interface, PCWSTR, PWSTR, Ref, Result},
};

use crate::{
    com_server::ComServerLease,
    windows_impl::{CapyIoMediaSourceHandle, create_registered_media_source_with_attributes},
};

#[windows::core::implement(IMFActivate)]
struct MediaSourceActivate {
    _lease: ComServerLease,
    attributes: IMFAttributes,
    source: Mutex<Option<CapyIoMediaSourceHandle>>,
}

impl MediaSourceActivate {
    fn new() -> Result<Self> {
        let mut attributes = None;
        unsafe { MFCreateAttributes(&mut attributes, 4)? };
        let attributes = attributes.ok_or_else(|| Error::from_hresult(E_UNEXPECTED))?;
        unsafe {
            attributes.SetUINT32(&MF_VIRTUALCAMERA_PROVIDE_ASSOCIATED_CAMERA_SOURCES, 1)?;
        }
        Ok(Self {
            _lease: ComServerLease::new()?,
            attributes,
            source: Mutex::new(None),
        })
    }

    fn lock_source(&self) -> Result<MutexGuard<'_, Option<CapyIoMediaSourceHandle>>> {
        self.source
            .lock()
            .map_err(|_| Error::from_hresult(E_UNEXPECTED))
    }

    fn attribute_vtable(&self) -> &IMFAttributes_Vtbl {
        Interface::vtable(&self.attributes)
    }

    fn attribute_raw(&self) -> *mut c_void {
        Interface::as_raw(&self.attributes)
    }
}

pub(crate) fn create_media_source_activate() -> Result<IMFActivate> {
    Ok(MediaSourceActivate::new()?.into())
}

impl IMFActivate_Impl for MediaSourceActivate_Impl {
    fn ActivateObject(&self, interface_id: *const GUID, object: *mut *mut c_void) -> Result<()> {
        if interface_id.is_null() || object.is_null() {
            return Err(Error::from_hresult(E_POINTER));
        }
        unsafe { object.write(ptr::null_mut()) };

        let mut source = self.lock_source()?;
        if source.is_none() {
            *source = Some(create_registered_media_source_with_attributes(
                &self.attributes,
            )?);
        }
        let source = source
            .as_ref()
            .ok_or_else(|| Error::from_hresult(E_UNEXPECTED))?;
        unsafe { source.source().query(interface_id, object) }.ok()
    }

    fn ShutdownObject(&self) -> Result<()> {
        let source = self.lock_source()?.take();
        if let Some(source) = source {
            unsafe { source.source().Shutdown()? };
        }
        Ok(())
    }

    fn DetachObject(&self) -> Result<()> {
        self.lock_source()?.take();
        Ok(())
    }
}

impl IMFAttributes_Impl for MediaSourceActivate_Impl {
    fn GetItem(&self, key: *const GUID, value: *mut PROPVARIANT) -> Result<()> {
        unsafe { self.attributes.GetItem(key, Some(value)) }
    }

    fn GetItemType(&self, key: *const GUID) -> Result<MF_ATTRIBUTE_TYPE> {
        unsafe { self.attributes.GetItemType(key) }
    }

    fn CompareItem(&self, key: *const GUID, value: *const PROPVARIANT) -> Result<BOOL> {
        unsafe { self.attributes.CompareItem(key, value) }
    }

    fn Compare(
        &self,
        theirs: Ref<'_, IMFAttributes>,
        match_type: MF_ATTRIBUTES_MATCH_TYPE,
    ) -> Result<BOOL> {
        unsafe { self.attributes.Compare(theirs.ok()?, match_type) }
    }

    fn GetUINT32(&self, key: *const GUID) -> Result<u32> {
        unsafe { self.attributes.GetUINT32(key) }
    }

    fn GetUINT64(&self, key: *const GUID) -> Result<u64> {
        unsafe { self.attributes.GetUINT64(key) }
    }

    fn GetDouble(&self, key: *const GUID) -> Result<f64> {
        unsafe { self.attributes.GetDouble(key) }
    }

    fn GetGUID(&self, key: *const GUID) -> Result<GUID> {
        unsafe { self.attributes.GetGUID(key) }
    }

    fn GetStringLength(&self, key: *const GUID) -> Result<u32> {
        unsafe { self.attributes.GetStringLength(key) }
    }

    fn GetString(
        &self,
        key: *const GUID,
        value: PWSTR,
        buffer_size: u32,
        length: *mut u32,
    ) -> Result<()> {
        unsafe {
            (self.attribute_vtable().GetString)(
                self.attribute_raw(),
                key,
                value,
                buffer_size,
                length,
            )
            .ok()
        }
    }

    fn GetAllocatedString(
        &self,
        key: *const GUID,
        value: *mut PWSTR,
        length: *mut u32,
    ) -> Result<()> {
        unsafe { self.attributes.GetAllocatedString(key, value, length) }
    }

    fn GetBlobSize(&self, key: *const GUID) -> Result<u32> {
        unsafe { self.attributes.GetBlobSize(key) }
    }

    fn GetBlob(
        &self,
        key: *const GUID,
        buffer: *mut u8,
        buffer_size: u32,
        blob_size: *mut u32,
    ) -> Result<()> {
        unsafe {
            (self.attribute_vtable().GetBlob)(
                self.attribute_raw(),
                key,
                buffer,
                buffer_size,
                blob_size,
            )
            .ok()
        }
    }

    fn GetAllocatedBlob(
        &self,
        key: *const GUID,
        buffer: *mut *mut u8,
        size: *mut u32,
    ) -> Result<()> {
        unsafe { self.attributes.GetAllocatedBlob(key, buffer, size) }
    }

    fn GetUnknown(
        &self,
        key: *const GUID,
        interface_id: *const GUID,
        object: *mut *mut c_void,
    ) -> Result<()> {
        unsafe {
            (self.attribute_vtable().GetUnknown)(self.attribute_raw(), key, interface_id, object)
                .ok()
        }
    }

    fn SetItem(&self, key: *const GUID, value: *const PROPVARIANT) -> Result<()> {
        unsafe { self.attributes.SetItem(key, value) }
    }

    fn DeleteItem(&self, key: *const GUID) -> Result<()> {
        unsafe { self.attributes.DeleteItem(key) }
    }

    fn DeleteAllItems(&self) -> Result<()> {
        unsafe { self.attributes.DeleteAllItems() }
    }

    fn SetUINT32(&self, key: *const GUID, value: u32) -> Result<()> {
        unsafe { self.attributes.SetUINT32(key, value) }
    }

    fn SetUINT64(&self, key: *const GUID, value: u64) -> Result<()> {
        unsafe { self.attributes.SetUINT64(key, value) }
    }

    fn SetDouble(&self, key: *const GUID, value: f64) -> Result<()> {
        unsafe { self.attributes.SetDouble(key, value) }
    }

    fn SetGUID(&self, key: *const GUID, value: *const GUID) -> Result<()> {
        unsafe { self.attributes.SetGUID(key, value) }
    }

    fn SetString(&self, key: *const GUID, value: &PCWSTR) -> Result<()> {
        unsafe { self.attributes.SetString(key, *value) }
    }

    fn SetBlob(&self, key: *const GUID, buffer: *const u8, buffer_size: u32) -> Result<()> {
        unsafe {
            (self.attribute_vtable().SetBlob)(self.attribute_raw(), key, buffer, buffer_size).ok()
        }
    }

    fn SetUnknown(&self, key: *const GUID, unknown: Ref<'_, IUnknown>) -> Result<()> {
        let raw = unknown.as_ref().map_or(ptr::null_mut(), Interface::as_raw);
        unsafe { (self.attribute_vtable().SetUnknown)(self.attribute_raw(), key, raw).ok() }
    }

    fn LockStore(&self) -> Result<()> {
        unsafe { self.attributes.LockStore() }
    }

    fn UnlockStore(&self) -> Result<()> {
        unsafe { self.attributes.UnlockStore() }
    }

    fn GetCount(&self) -> Result<u32> {
        unsafe { self.attributes.GetCount() }
    }

    fn GetItemByIndex(&self, index: u32, key: *mut GUID, value: *mut PROPVARIANT) -> Result<()> {
        unsafe { self.attributes.GetItemByIndex(index, key, Some(value)) }
    }

    fn CopyAllItems(&self, destination: Ref<'_, IMFAttributes>) -> Result<()> {
        unsafe { self.attributes.CopyAllItems(destination.ok()?) }
    }
}
