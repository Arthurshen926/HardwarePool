use capyio_input::{
    InputFrameHeader, InputStreamDescriptor, NormalizedMagnitude, SequenceGap, TouchpadButtonState,
    TouchpadButtonType, TouchpadContact, TouchpadDescriptor, TouchpadFrame, TouchpadFrameKind,
    TouchpadPhysicalSize, TouchpadPosition,
};
#[cfg(windows)]
use capyio_windows_input::NativeTouchpadBatch;
use capyio_windows_input::{
    WindowsTouchpadContactPhase, WindowsTouchpadProjectionDisposition,
    WindowsTouchpadProjectionError, WindowsTouchpadProjector,
};

fn stream(epoch: u64) -> InputStreamDescriptor {
    InputStreamDescriptor {
        stream_id: "00000000-0000-4000-8000-00000000c501"
            .parse()
            .expect("stream ID"),
        stream_epoch: epoch,
        clock_domain_id: "fixture.touchpad.monotonic".to_owned(),
    }
}

fn descriptor() -> TouchpadDescriptor {
    TouchpadDescriptor {
        physical_size: TouchpadPhysicalSize {
            width_himetric: 10_000,
            height_himetric: 6_000,
        },
        max_contacts: 5,
        button_type: TouchpadButtonType::NonClickable,
        reports_contact_size: false,
        reports_pressure: true,
    }
}

fn contact(contact_id: u32, x: u32, y: u32) -> TouchpadContact {
    TouchpadContact {
        contact_id,
        position: TouchpadPosition {
            x_himetric: x,
            y_himetric: y,
        },
        confidence: true,
        size: None,
        pressure: Some(NormalizedMagnitude::new(40_000)),
    }
}

fn frame(
    epoch: u64,
    sequence: u64,
    kind: TouchpadFrameKind,
    contacts: Vec<TouchpadContact>,
) -> TouchpadFrame {
    TouchpadFrame {
        header: InputFrameHeader {
            stream_id: stream(epoch).stream_id,
            stream_epoch: epoch,
            sequence,
            source_timestamp_nanos: 1_000 + sequence,
        },
        kind,
        button: TouchpadButtonState::Released,
        contacts,
    }
}

#[test]
fn complete_snapshots_emit_active_release_and_cancel_records_once() {
    let mut projector =
        WindowsTouchpadProjector::new(&stream(1), descriptor(), 0).expect("projector");
    assert_eq!(projector.device_parameters().max_contacts, 5);
    assert_eq!(projector.device_parameters().width_himetric, 10_000);
    assert_eq!(projector.device_parameters().height_himetric, 6_000);
    let initial = projector
        .project(&frame(1, 0, TouchpadFrameKind::CancelAll, Vec::new()))
        .expect("initial cancel");
    assert_eq!(
        initial.disposition,
        WindowsTouchpadProjectionDisposition::Cancelled
    );
    assert!(initial.batches().is_empty());

    let active = projector
        .project(&frame(
            1,
            1,
            TouchpadFrameKind::Update,
            vec![contact(20, 8_000, 4_000), contact(10, 2_000, 1_000)],
        ))
        .expect("active");
    assert_eq!(active.batch_count(), 1);
    assert_eq!(
        active.batches()[0]
            .contacts()
            .iter()
            .map(|item| (item.contact_id, item.phase))
            .collect::<Vec<_>>(),
        vec![
            (10, WindowsTouchpadContactPhase::Pressed),
            (20, WindowsTouchpadContactPhase::Pressed),
        ]
    );

    let release_one = projector
        .project(&frame(
            1,
            2,
            TouchpadFrameKind::Update,
            vec![contact(20, 8_100, 4_100)],
        ))
        .expect("release one");
    assert_eq!(
        release_one.batches()[0]
            .contacts()
            .iter()
            .map(|item| (item.contact_id, item.phase))
            .collect::<Vec<_>>(),
        vec![
            (20, WindowsTouchpadContactPhase::Updated),
            (10, WindowsTouchpadContactPhase::Released),
        ]
    );

    let cancel = projector
        .project(&frame(1, 3, TouchpadFrameKind::CancelAll, Vec::new()))
        .expect("cancel");
    assert_eq!(cancel.batches()[0].contacts().len(), 1);
    assert_eq!(
        cancel.batches()[0].contacts()[0].phase,
        WindowsTouchpadContactPhase::Cancelled
    );
    let repeated = projector
        .project(&frame(1, 4, TouchpadFrameKind::CancelAll, Vec::new()))
        .expect("idempotent cancel");
    assert!(repeated.batches().is_empty());
}

#[test]
fn gap_and_epoch_clear_native_state_before_suppressing_updates() {
    let mut projector =
        WindowsTouchpadProjector::new(&stream(1), descriptor(), 0).expect("projector");
    projector
        .project(&frame(1, 0, TouchpadFrameKind::CancelAll, Vec::new()))
        .expect("cancel");
    projector
        .project(&frame(
            1,
            1,
            TouchpadFrameKind::Update,
            vec![contact(1, 1_000, 1_000)],
        ))
        .expect("active");

    let gap = projector
        .project(&frame(
            1,
            3,
            TouchpadFrameKind::Update,
            vec![contact(1, 2_000, 2_000)],
        ))
        .expect("gap");
    assert_eq!(
        gap.disposition,
        WindowsTouchpadProjectionDisposition::GapRequiresCancelAll(SequenceGap {
            first_missing: 2,
            last_missing: 2,
        })
    );
    assert_eq!(
        gap.batches()[0].contacts()[0].phase,
        WindowsTouchpadContactPhase::Cancelled
    );

    let suppressed = projector
        .project(&frame(
            1,
            4,
            TouchpadFrameKind::Update,
            vec![contact(1, 3_000, 3_000)],
        ))
        .expect("suppressed");
    assert_eq!(
        suppressed.disposition,
        WindowsTouchpadProjectionDisposition::SuppressedUntilCancelAll
    );
    assert!(suppressed.batches().is_empty());

    projector
        .project(&frame(1, 5, TouchpadFrameKind::CancelAll, Vec::new()))
        .expect("recovery cancel");
    projector
        .project(&frame(
            1,
            6,
            TouchpadFrameKind::Update,
            vec![contact(2, 4_000, 4_000)],
        ))
        .expect("active after cancel");
    let epoch = projector.advance_epoch(2, 100).expect("epoch");
    assert_eq!(
        epoch.disposition,
        WindowsTouchpadProjectionDisposition::EpochCancelled
    );
    assert_eq!(
        epoch.batches()[0].contacts()[0].phase,
        WindowsTouchpadContactPhase::Cancelled
    );
    let new_epoch = projector
        .project(&frame(
            2,
            100,
            TouchpadFrameKind::Update,
            vec![contact(2, 5_000, 5_000)],
        ))
        .expect("new epoch suppressed");
    assert_eq!(
        new_epoch.disposition,
        WindowsTouchpadProjectionDisposition::SuppressedUntilCancelAll
    );
}

#[test]
fn five_for_five_replacement_splits_release_then_active_within_bounds() {
    let mut projector =
        WindowsTouchpadProjector::new(&stream(1), descriptor(), 0).expect("projector");
    projector
        .project(&frame(1, 0, TouchpadFrameKind::CancelAll, Vec::new()))
        .expect("cancel");
    projector
        .project(&frame(
            1,
            1,
            TouchpadFrameKind::Update,
            (0..5).map(|id| contact(id, 100 + id, 100)).collect(),
        ))
        .expect("first five");
    let replaced = projector
        .project(&frame(
            1,
            2,
            TouchpadFrameKind::Update,
            (10..15).map(|id| contact(id, 200 + id, 200)).collect(),
        ))
        .expect("replacement");
    assert_eq!(replaced.batch_count(), 2);
    assert!(replaced.batches().iter().all(|batch| batch.len() <= 5));
    assert!(
        replaced.batches()[0]
            .contacts()
            .iter()
            .all(|item| item.phase == WindowsTouchpadContactPhase::Released)
    );
    assert!(
        replaced.batches()[1]
            .contacts()
            .iter()
            .all(|item| item.phase == WindowsTouchpadContactPhase::Pressed)
    );
}

#[test]
fn integrated_button_descriptors_are_rejected_until_native_mapping_is_defined() {
    for button_type in [
        TouchpadButtonType::ClickPad,
        TouchpadButtonType::PressurePad,
    ] {
        let descriptor = TouchpadDescriptor {
            button_type,
            ..descriptor()
        };
        assert_eq!(
            WindowsTouchpadProjector::new(&stream(1), descriptor, 0)
                .expect_err("button mapping is not implemented"),
            WindowsTouchpadProjectionError::UnsupportedButtonType(button_type)
        );
    }
}

#[cfg(windows)]
#[test]
fn native_encoder_uses_touchpad_himetric_fields_and_documented_flags() {
    const PT_TOUCHPAD: i32 = 5;
    const IN_RANGE: u32 = 0x0000_0002;
    const IN_CONTACT: u32 = 0x0000_0004;
    const CONFIDENCE: u32 = 0x0000_4000;
    const CANCELLED: u32 = 0x0000_8000;
    const DOWN: u32 = 0x0001_0000;
    const UPDATE: u32 = 0x0002_0000;
    const UP: u32 = 0x0004_0000;

    let mut projector =
        WindowsTouchpadProjector::new(&stream(1), descriptor(), 0).expect("projector");
    projector
        .project(&frame(1, 0, TouchpadFrameKind::CancelAll, Vec::new()))
        .expect("cancel");
    let active = projector
        .project(&frame(
            1,
            1,
            TouchpadFrameKind::Update,
            vec![contact(7, 3_000, 4_000)],
        ))
        .expect("active");
    let native = NativeTouchpadBatch::encode(&active.batches()[0]);
    assert_eq!(native.len(), 1);
    assert!(!native.as_ptr().is_null());
    let view = native.inspect(0).expect("native contact");
    assert_eq!(view.pointer_type, PT_TOUCHPAD);
    assert_eq!(view.pointer_id, 7);
    assert_eq!(
        view.pointer_flags,
        IN_RANGE | IN_CONTACT | DOWN | CONFIDENCE
    );
    assert_eq!((view.x_himetric, view.y_himetric), (3_000, 4_000));
    assert_eq!((view.x_himetric_raw, view.y_himetric_raw), (3_000, 4_000));
    assert_eq!((view.dw_time, view.performance_count), (0, 0));
    assert_eq!(view.touch_mask, 0);

    let update = projector
        .project(&frame(
            1,
            2,
            TouchpadFrameKind::Update,
            vec![contact(7, 3_100, 4_100)],
        ))
        .expect("update");
    let native_update = NativeTouchpadBatch::encode(&update.batches()[0]);
    assert_eq!(
        native_update
            .inspect(0)
            .expect("updated contact")
            .pointer_flags,
        IN_RANGE | IN_CONTACT | UPDATE | CONFIDENCE
    );

    let release = projector
        .project(&frame(1, 3, TouchpadFrameKind::Update, Vec::new()))
        .expect("release");
    let native_release = NativeTouchpadBatch::encode(&release.batches()[0]);
    assert_eq!(
        native_release
            .inspect(0)
            .expect("released contact")
            .pointer_flags,
        UP | CONFIDENCE
    );

    projector
        .project(&frame(
            1,
            4,
            TouchpadFrameKind::Update,
            vec![contact(8, 2_000, 2_000)],
        ))
        .expect("second press");
    let cancel = projector
        .project(&frame(1, 5, TouchpadFrameKind::CancelAll, Vec::new()))
        .expect("cancel");
    let native_cancel = NativeTouchpadBatch::encode(&cancel.batches()[0]);
    assert_eq!(
        native_cancel
            .inspect(0)
            .expect("cancel contact")
            .pointer_flags,
        CANCELLED | UP | CONFIDENCE
    );
}
