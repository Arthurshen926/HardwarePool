use std::{
    ffi::c_void,
    ptr,
    sync::atomic::{AtomicU32, Ordering},
};

use windows::{
    Win32::{
        Foundation::{
            CLASS_E_CLASSNOTAVAILABLE, CLASS_E_NOAGGREGATION, E_POINTER, E_UNEXPECTED, S_FALSE,
            S_OK,
        },
        System::Com::{IClassFactory, IClassFactory_Impl},
    },
    core::{BOOL, Error, GUID, HRESULT, Interface, Ref, Result},
};

use crate::activation::create_media_source_activate;

pub const CAPYIO_CAMERA_SOURCE_GUID: GUID = GUID::from_u128(0x35754be3_54b6_4133_a1c7_1716395c6f1c);

static ACTIVE_COM_OBJECTS: AtomicU32 = AtomicU32::new(0);
static SERVER_LOCKS: AtomicU32 = AtomicU32::new(0);

pub(crate) struct ComServerLease;

impl ComServerLease {
    pub(crate) fn new() -> Result<Self> {
        ACTIVE_COM_OBJECTS
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                current.checked_add(1)
            })
            .map(|_| Self)
            .map_err(|_| Error::from_hresult(E_UNEXPECTED))
    }
}

impl Drop for ComServerLease {
    fn drop(&mut self) {
        let previous = ACTIVE_COM_OBJECTS.fetch_sub(1, Ordering::AcqRel);
        debug_assert!(previous > 0);
    }
}

#[windows::core::implement(IClassFactory)]
struct MediaSourceClassFactory {
    _lease: ComServerLease,
}

impl MediaSourceClassFactory {
    fn new() -> Result<Self> {
        Ok(Self {
            _lease: ComServerLease::new()?,
        })
    }
}

impl IClassFactory_Impl for MediaSourceClassFactory_Impl {
    fn CreateInstance(
        &self,
        outer: Ref<'_, windows::core::IUnknown>,
        interface_id: *const GUID,
        object: *mut *mut c_void,
    ) -> Result<()> {
        if object.is_null() || interface_id.is_null() {
            return Err(Error::from_hresult(E_POINTER));
        }
        unsafe { object.write(ptr::null_mut()) };
        if !outer.is_null() {
            return Err(Error::from_hresult(CLASS_E_NOAGGREGATION));
        }

        let activate = create_media_source_activate()?;
        let status = unsafe { activate.query(interface_id, object) };
        status.ok()
    }

    fn LockServer(&self, lock: BOOL) -> Result<()> {
        if lock.as_bool() {
            SERVER_LOCKS
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                    current.checked_add(1)
                })
                .map(|_| ())
                .map_err(|_| Error::from_hresult(E_UNEXPECTED))
        } else {
            SERVER_LOCKS
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                    current.checked_sub(1)
                })
                .map(|_| ())
                .map_err(|_| Error::from_hresult(E_UNEXPECTED))
        }
    }
}

pub fn create_media_source_class_factory() -> Result<IClassFactory> {
    Ok(MediaSourceClassFactory::new()?.into())
}

#[must_use]
pub fn server_can_unload() -> bool {
    ACTIVE_COM_OBJECTS.load(Ordering::Acquire) == 0 && SERVER_LOCKS.load(Ordering::Acquire) == 0
}

fn get_class_object(
    class_id: *const GUID,
    interface_id: *const GUID,
    object: *mut *mut c_void,
) -> Result<()> {
    if object.is_null() || class_id.is_null() || interface_id.is_null() {
        return Err(Error::from_hresult(E_POINTER));
    }
    unsafe { object.write(ptr::null_mut()) };
    if unsafe { *class_id } != CAPYIO_CAMERA_SOURCE_GUID {
        return Err(Error::from_hresult(CLASS_E_CLASSNOTAVAILABLE));
    }

    let factory = create_media_source_class_factory()?;
    unsafe { factory.query(interface_id, object) }.ok()
}

#[allow(non_snake_case)]
#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllGetClassObject(
    class_id: *const GUID,
    interface_id: *const GUID,
    object: *mut *mut c_void,
) -> HRESULT {
    match get_class_object(class_id, interface_id, object) {
        Ok(()) => S_OK,
        Err(error) => error.code(),
    }
}

#[allow(non_snake_case)]
#[unsafe(no_mangle)]
pub extern "system" fn DllCanUnloadNow() -> HRESULT {
    if server_can_unload() { S_OK } else { S_FALSE }
}
