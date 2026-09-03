use capyio_input::{InputStreamDescriptor, TouchpadFrameKind};
use capyio_windows_input::{
    FIXED_DOUBLE_TAP_DRAG_FRAMES, FIXED_INJECTION_INTERVAL_MILLIS, FIXED_INJECTION_UPDATE_FRAMES,
    SyntheticTouchpadGesture, TouchpadInjectionDryRun, build_touchpad_injection_fixture,
};

fn stream() -> InputStreamDescriptor {
    InputStreamDescriptor {
        stream_id: "00000000-0000-4000-8000-00000000c601"
            .parse()
            .expect("stream ID"),
        stream_epoch: 1,
        clock_domain_id: "windows.fixture.monotonic".to_owned(),
    }
}

#[test]
fn every_fixed_gesture_is_bounded_cancelled_and_deterministic() {
    for gesture in [
        SyntheticTouchpadGesture::OneFingerTap,
        SyntheticTouchpadGesture::OneFingerMotion,
        SyntheticTouchpadGesture::TwoFingerPan,
        SyntheticTouchpadGesture::ThreeFingerSwipe,
        SyntheticTouchpadGesture::FourFingerSwipe,
    ] {
        let fixture = build_touchpad_injection_fixture(gesture, stream());
        assert_eq!(fixture.interval_millis, FIXED_INJECTION_INTERVAL_MILLIS);
        assert_eq!(
            fixture.frames.len(),
            FIXED_INJECTION_UPDATE_FRAMES as usize + 3
        );
        assert_eq!(
            fixture.frames.first().expect("first").kind,
            TouchpadFrameKind::CancelAll
        );
        assert_eq!(
            fixture.frames.last().expect("last").kind,
            TouchpadFrameKind::CancelAll
        );
        assert!(fixture.frames.last().expect("last").contacts.is_empty());
        assert!(fixture.frames.iter().all(|frame| frame.contacts.len() <= 5));

        let metrics = fixture.dry_run().expect("dry run");
        let contacts = u64::from(gesture.contact_count());
        assert_eq!(
            metrics,
            TouchpadInjectionDryRun {
                frames_projected: FIXED_INJECTION_UPDATE_FRAMES + 3,
                batches_encoded: FIXED_INJECTION_UPDATE_FRAMES + 1,
                contact_records_encoded: (FIXED_INJECTION_UPDATE_FRAMES + 1) * contacts,
                active_records: FIXED_INJECTION_UPDATE_FRAMES * contacts,
                released_records: contacts,
                cancelled_records: 0,
                peak_batch_contacts: gesture.contact_count(),
                peak_batches_per_frame: 1,
            }
        );
    }
}

#[test]
fn double_tap_drag_has_two_contacts_separated_by_a_released_gap() {
    let fixture = build_touchpad_injection_fixture(
        SyntheticTouchpadGesture::OneFingerDoubleTapDrag,
        stream(),
    );
    assert_eq!(fixture.frames.len(), FIXED_DOUBLE_TAP_DRAG_FRAMES as usize);
    assert_eq!(
        fixture.frames.first().expect("first").kind,
        TouchpadFrameKind::CancelAll
    );
    assert_eq!(
        fixture.frames.last().expect("last").kind,
        TouchpadFrameKind::CancelAll
    );

    let first_tap = &fixture.frames[1..=2];
    assert!(
        first_tap
            .iter()
            .all(|frame| { frame.contacts.len() == 1 && frame.contacts[0].contact_id == 1 })
    );
    assert!(
        fixture.frames[3..=5]
            .iter()
            .all(|frame| frame.contacts.is_empty())
    );

    let second_contact = &fixture.frames[6..=19];
    assert!(
        second_contact
            .iter()
            .all(|frame| { frame.contacts.len() == 1 && frame.contacts[0].contact_id == 2 })
    );
    assert_eq!(
        second_contact[0].contacts[0].position,
        second_contact[1].contacts[0].position
    );
    assert!(
        second_contact.first().expect("second down").contacts[0]
            .position
            .x_himetric
            < second_contact.last().expect("second moved").contacts[0]
                .position
                .x_himetric
    );
    assert!(fixture.frames[20].contacts.is_empty());

    let metrics = fixture.dry_run().expect("dry run");
    assert_eq!(metrics.frames_projected, FIXED_DOUBLE_TAP_DRAG_FRAMES);
    assert_eq!(metrics.peak_batch_contacts, 1);
    assert_eq!(metrics.released_records, 2);
    assert_eq!(metrics.cancelled_records, 0);
}

#[test]
fn gesture_names_are_closed_and_round_trip() {
    for gesture in [
        SyntheticTouchpadGesture::OneFingerTap,
        SyntheticTouchpadGesture::OneFingerDoubleTapDrag,
        SyntheticTouchpadGesture::OneFingerMotion,
        SyntheticTouchpadGesture::TwoFingerPan,
        SyntheticTouchpadGesture::ThreeFingerSwipe,
        SyntheticTouchpadGesture::FourFingerSwipe,
    ] {
        assert_eq!(
            gesture.name().parse::<SyntheticTouchpadGesture>(),
            Ok(gesture)
        );
    }
    assert!("tap".parse::<SyntheticTouchpadGesture>().is_err());
}

#[test]
fn contact_ids_remain_stable_while_fixed_motion_changes_positions() {
    let fixture =
        build_touchpad_injection_fixture(SyntheticTouchpadGesture::FourFingerSwipe, stream());
    let updates = &fixture.frames[1..=FIXED_INJECTION_UPDATE_FRAMES as usize];
    let first = updates.first().expect("first update");
    let last = updates.last().expect("last update");
    assert_eq!(
        first
            .contacts
            .iter()
            .map(|contact| contact.contact_id)
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4]
    );
    assert_eq!(
        last.contacts
            .iter()
            .map(|contact| contact.contact_id)
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4]
    );
    assert!(
        first
            .contacts
            .iter()
            .zip(&last.contacts)
            .all(|(start, end)| start.position.x_himetric < end.position.x_himetric)
    );
}
