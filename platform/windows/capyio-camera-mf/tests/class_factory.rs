#![cfg(windows)]

use capyio_windows_camera_mf::{
    CAPYIO_CAMERA_SOURCE_GUID, MediaFoundationRuntime, create_media_source_class_factory,
    server_can_unload,
};
use windows::{
    Win32::{
        Foundation::{CLASS_E_NOAGGREGATION, E_NOINTERFACE},
        Media::MediaFoundation::{
            IMFActivate, IMFAttributes, IMFMediaSource, IMFMediaSourceEx,
            MF_VIRTUALCAMERA_PROVIDE_ASSOCIATED_CAMERA_SOURCES,
        },
    },
    core::{IUnknown, Interface, Result},
};

#[test]
fn class_factory_creates_source_rejects_aggregation_and_tracks_unload() -> Result<()> {
    let _media_foundation = MediaFoundationRuntime::startup()?;
    assert!(server_can_unload());
    assert_eq!(
        CAPYIO_CAMERA_SOURCE_GUID.to_u128(),
        0x35754be3_54b6_4133_a1c7_1716395c6f1c
    );

    let factory = create_media_source_class_factory()?;
    assert!(!server_can_unload());
    unsafe { factory.LockServer(true)? };

    let activate: IMFActivate = unsafe { factory.CreateInstance(None::<&IUnknown>)? };
    assert!(!server_can_unload());
    let activate_attributes: IMFAttributes = activate.cast()?;
    assert_eq!(
        unsafe {
            activate_attributes.GetUINT32(&MF_VIRTUALCAMERA_PROVIDE_ASSOCIATED_CAMERA_SOURCES)?
        },
        1
    );
    let source: IMFMediaSourceEx = unsafe { activate.ActivateObject()? };
    let source_base: IMFMediaSource = source.cast()?;
    let source_attributes = unsafe { source.GetSourceAttributes()? };
    assert_eq!(
        unsafe {
            source_attributes.GetUINT32(&MF_VIRTUALCAMERA_PROVIDE_ASSOCIATED_CAMERA_SOURCES)?
        },
        1
    );

    let aggregation_error = unsafe { factory.CreateInstance::<_, IMFActivate>(&source_base) }
        .expect_err("COM aggregation is not supported");
    assert_eq!(aggregation_error.code(), CLASS_E_NOAGGREGATION);

    let direct_source_error =
        unsafe { factory.CreateInstance::<_, IMFMediaSourceEx>(None::<&IUnknown>) }
            .expect_err("the class factory exposes IMFActivate, not a source directly");
    assert_eq!(direct_source_error.code(), E_NOINTERFACE);

    let unsupported_error = unsafe { factory.CreateInstance::<_, IUnknown>(None::<&IUnknown>) }
        .and_then(|unknown| unknown.cast::<windows::Win32::System::Com::IClassFactory>())
        .expect_err("the media source does not implement IClassFactory");
    assert_eq!(unsupported_error.code(), E_NOINTERFACE);

    unsafe { activate.ShutdownObject()? };
    drop(source_attributes);
    drop(source_base);
    drop(source);
    drop(activate_attributes);
    drop(activate);
    assert!(!server_can_unload());
    unsafe { factory.LockServer(false)? };
    drop(factory);
    assert!(server_can_unload());
    Ok(())
}
