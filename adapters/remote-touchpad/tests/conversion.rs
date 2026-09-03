use capyio_input::{
    InputFrameHeader, InputStreamDescriptor, NormalizedPosition, PointerButton, PointerButtonPhase,
    PointerEvent, TouchContact, TouchFrame,
};
use capyio_remote_touchpad_adapter::{
    DRAG_HOLD_DURATION_NANOS, PointerConversion, TouchpadConversionError, TouchpadConverter,
};

const INPUT_STREAM: &str = "00000000-0000-4000-8000-00000000c101";
const OUTPUT_STREAM: &str = "00000000-0000-4000-8000-00000000c102";
const OTHER_STREAM: &str = "00000000-0000-4000-8000-00000000c103";
const CLOCK_DOMAIN: &str = "fixture.touch.monotonic";

fn stream(id: &str, epoch: u64, clock_domain: &str) -> InputStreamDescriptor {
    InputStreamDescriptor {
        stream_id: id.parse().expect("fixture StreamId"),
        stream_epoch: epoch,
        clock_domain_id: clock_domain.to_owned(),
    }
}

fn converter(first_output_sequence: u64) -> TouchpadConverter {
    TouchpadConverter::new(
        stream(INPUT_STREAM, 1, CLOCK_DOMAIN),
        0,
        stream(OUTPUT_STREAM, 4, CLOCK_DOMAIN),
        first_output_sequence,
    )
    .expect("converter")
}

fn contact(id: u16, x: u16, y: u16) -> TouchContact {
    TouchContact {
        contact_id: id,
        position: NormalizedPosition::new(x, y),
        pressure: None,
    }
}

fn touch(sequence: u64, timestamp: u64, contacts: Vec<TouchContact>) -> TouchFrame {
    touch_in_epoch(1, sequence, timestamp, contacts)
}

fn touch_in_epoch(
    epoch: u64,
    sequence: u64,
    timestamp: u64,
    contacts: Vec<TouchContact>,
) -> TouchFrame {
    TouchFrame {
        header: InputFrameHeader {
            stream_id: INPUT_STREAM.parse().expect("fixture StreamId"),
            stream_epoch: epoch,
            sequence,
            source_timestamp_nanos: timestamp,
        },
        contacts,
    }
}

fn one(output: PointerConversion) -> capyio_input::PointerFrame {
    match output {
        PointerConversion::One(frame) => frame,
        other => panic!("expected one Pointer frame, received {other:?}"),
    }
}

fn two(output: PointerConversion) -> (capyio_input::PointerFrame, capyio_input::PointerFrame) {
    match output {
        PointerConversion::Two(first, second) => (first, second),
        other => panic!("expected two Pointer frames, received {other:?}"),
    }
}

#[test]
fn one_finger_motion_is_relative_and_empty_snapshot_resets() {
    let mut converter = converter(40);
    assert!(
        converter
            .convert(&touch(0, 1_000_000_000, vec![contact(7, 10_000, 20_000)]))
            .expect("anchor")
            .is_empty()
    );

    let motion = one(converter
        .convert(&touch(1, 1_010_000_000, vec![contact(7, 10_900, 19_300)]))
        .expect("motion"));
    assert_eq!(motion.header.sequence, 40);
    assert_eq!(motion.header.stream_epoch, 4);
    assert_eq!(
        motion.events,
        vec![PointerEvent::RelativeMotion {
            delta_x: 900,
            delta_y: -700,
        }]
    );

    let reset = one(converter
        .convert(&touch(2, 1_020_000_000, Vec::new()))
        .expect("empty reset"));
    assert_eq!(reset.header.sequence, 41);
    assert_eq!(reset.events, vec![PointerEvent::Reset]);
}

#[test]
fn short_stationary_contact_emits_click_then_reset() {
    let mut converter = converter(0);
    converter
        .convert(&touch(0, 2_000_000_000, vec![contact(3, 30_000, 40_000)]))
        .expect("anchor");

    let (click, reset) = two(converter
        .convert(&touch(1, 2_100_000_000, Vec::new()))
        .expect("tap"));
    assert_eq!(
        click.events,
        vec![
            PointerEvent::Button {
                button: PointerButton::Left,
                phase: PointerButtonPhase::Pressed,
            },
            PointerEvent::Button {
                button: PointerButton::Left,
                phase: PointerButtonPhase::Released,
            },
        ]
    );
    assert_eq!(reset.events, vec![PointerEvent::Reset]);
    assert_eq!((click.header.sequence, reset.header.sequence), (0, 1));
}

#[test]
fn stationary_hold_enters_drag_and_release_precedes_reset() {
    let mut converter = converter(10);
    let started_at = 3_000_000_000;
    converter
        .convert(&touch(0, started_at, vec![contact(5, 20_000, 20_000)]))
        .expect("anchor");

    let press = one(converter
        .convert(&touch(
            1,
            started_at + DRAG_HOLD_DURATION_NANOS,
            vec![contact(5, 20_000, 20_000)],
        ))
        .expect("drag press"));
    assert_eq!(
        press.events,
        vec![PointerEvent::Button {
            button: PointerButton::Left,
            phase: PointerButtonPhase::Pressed,
        }]
    );

    let drag = one(converter
        .convert(&touch(
            2,
            started_at + DRAG_HOLD_DURATION_NANOS + 10_000_000,
            vec![contact(5, 20_300, 19_800)],
        ))
        .expect("drag motion"));
    assert_eq!(
        drag.events,
        vec![PointerEvent::RelativeMotion {
            delta_x: 300,
            delta_y: -200,
        }]
    );

    let (release, reset) = two(converter
        .convert(&touch(
            3,
            started_at + DRAG_HOLD_DURATION_NANOS + 20_000_000,
            Vec::new(),
        ))
        .expect("drag release"));
    assert_eq!(
        release.events,
        vec![PointerEvent::Button {
            button: PointerButton::Left,
            phase: PointerButtonPhase::Released,
        }]
    );
    assert_eq!(reset.events, vec![PointerEvent::Reset]);
}

#[test]
fn gap_resets_and_suppresses_contacts_until_empty_snapshot() {
    let mut converter = converter(0);
    let started_at = 4_000_000_000;
    converter
        .convert(&touch(0, started_at, vec![contact(1, 1_000, 1_000)]))
        .expect("anchor");
    let press = one(converter
        .convert(&touch(
            1,
            started_at + DRAG_HOLD_DURATION_NANOS,
            vec![contact(1, 1_000, 1_000)],
        ))
        .expect("drag press before gap"));
    assert!(matches!(
        press.events.as_slice(),
        [PointerEvent::Button {
            phase: PointerButtonPhase::Pressed,
            ..
        }]
    ));

    let reset = one(converter
        .convert(&touch(3, 4_520_000_000, vec![contact(1, 2_000, 2_000)]))
        .expect("gap"));
    assert_eq!(reset.events, vec![PointerEvent::Reset]);
    assert!(
        converter
            .convert(&touch(4, 4_530_000_000, vec![contact(1, 3_000, 3_000)]))
            .expect("suppressed")
            .is_empty()
    );
    assert_eq!(
        one(converter
            .convert(&touch(5, 4_540_000_000, Vec::new()))
            .expect("clear"))
        .events,
        vec![PointerEvent::Reset]
    );

    assert!(
        converter
            .convert(&touch(6, 4_550_000_000, vec![contact(9, 10_000, 10_000)]))
            .expect("new anchor")
            .is_empty()
    );
    assert_eq!(
        one(converter
            .convert(&touch(7, 4_560_000_000, vec![contact(9, 11_000, 10_000)]))
            .expect("recovered motion"))
        .events,
        vec![PointerEvent::RelativeMotion {
            delta_x: 1_000,
            delta_y: 0,
        }]
    );
}

#[test]
fn epoch_advance_uses_fresh_output_epoch_and_resets_before_new_input() {
    let mut converter = converter(90);
    converter
        .convert(&touch(0, 5_000_000_000, vec![contact(1, 5_000, 5_000)]))
        .expect("old epoch anchor");

    let reset = converter
        .advance_epoch(2, 100, 5, 200, 5_100_000_000)
        .expect("advance");
    assert_eq!(reset.header.stream_epoch, 5);
    assert_eq!(reset.header.sequence, 200);
    assert_eq!(reset.events, vec![PointerEvent::Reset]);

    assert!(
        converter
            .convert(&touch_in_epoch(
                2,
                100,
                5_110_000_000,
                vec![contact(2, 6_000, 6_000)],
            ))
            .expect("new anchor")
            .is_empty()
    );
    let motion = one(converter
        .convert(&touch_in_epoch(
            2,
            101,
            5_120_000_000,
            vec![contact(2, 7_000, 6_000)],
        ))
        .expect("new epoch motion"));
    assert_eq!(motion.header.stream_epoch, 5);
    assert_eq!(motion.header.sequence, 201);
}

#[test]
fn multi_contact_input_resets_without_gesture_expansion() {
    let mut converter = converter(0);
    let reset = one(converter
        .convert(&touch(
            0,
            6_000_000_000,
            vec![contact(1, 1_000, 1_000), contact(2, 2_000, 2_000)],
        ))
        .expect("unsupported multi-contact"));
    assert_eq!(reset.events, vec![PointerEvent::Reset]);
    assert!(
        converter
            .convert(&touch(1, 6_010_000_000, vec![contact(1, 3_000, 3_000)]))
            .expect("suppressed")
            .is_empty()
    );
}

#[test]
fn constructor_rejects_shared_identity_and_clock_relabeling() {
    assert!(matches!(
        TouchpadConverter::new(
            stream(INPUT_STREAM, 1, CLOCK_DOMAIN),
            0,
            stream(INPUT_STREAM, 1, CLOCK_DOMAIN),
            0,
        ),
        Err(TouchpadConversionError::SharedStreamId)
    ));
    assert!(matches!(
        TouchpadConverter::new(
            stream(INPUT_STREAM, 1, CLOCK_DOMAIN),
            0,
            stream(OTHER_STREAM, 1, "another.clock"),
            0,
        ),
        Err(TouchpadConversionError::ClockDomainMismatch)
    ));
}

#[test]
fn output_exhaustion_is_transactional_for_two_frame_tap() {
    let mut converter = converter(u64::MAX);
    converter
        .convert(&touch(0, 7_000_000_000, vec![contact(4, 1_000, 1_000)]))
        .expect("anchor");
    let release = touch(1, 7_100_000_000, Vec::new());
    assert!(matches!(
        converter.convert(&release),
        Err(TouchpadConversionError::OutputSequenceExhausted)
    ));
    assert!(matches!(
        converter.convert(&release),
        Err(TouchpadConversionError::OutputSequenceExhausted)
    ));
}
