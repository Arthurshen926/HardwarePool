//! Software event traces only: no native helper or input device is opened.
use capyio_input::{
    InputFrameHeader, InputStreamDescriptor, NormalizedMagnitude, TouchpadButtonState,
    TouchpadButtonType, TouchpadContact, TouchpadContactSize, TouchpadFrame, TouchpadFrameKind,
    TouchpadPosition,
};
use capyio_linux_touchpad::{
    EventBatch, InputEvent, MAX_EVENTS_PER_FRAME, TouchpadProjector, linux_touchpad_descriptor,
};

const ABS: u16 = 3;
const SLOT: u16 = 0x2f;
const TRACKING: u16 = 0x39;
const X: u16 = 0x35;
const Y: u16 = 0x36;
const PRESSURE: u16 = 0x3a;
const TOOLS: [u16; 5] = [0x145, 0x14d, 0x14e, 0x14f, 0x148];

fn stream(epoch: u64) -> InputStreamDescriptor {
    InputStreamDescriptor {
        stream_id: "00000000-0000-4000-8000-00000000c703".parse().unwrap(),
        stream_epoch: epoch,
        clock_domain_id: "android.elapsed-realtime-nanos".into(),
    }
}
fn contact(id: u32) -> TouchpadContact {
    TouchpadContact {
        contact_id: id,
        position: TouchpadPosition {
            x_himetric: 2_000,
            y_himetric: 3_000,
        },
        confidence: true,
        size: None,
        pressure: Some(NormalizedMagnitude::new(32_768)),
    }
}
fn frame(epoch: u64, sequence: u64, contacts: Vec<TouchpadContact>) -> TouchpadFrame {
    TouchpadFrame {
        header: InputFrameHeader {
            stream_id: stream(epoch).stream_id,
            stream_epoch: epoch,
            sequence,
            source_timestamp_nanos: sequence * 1_000,
        },
        kind: TouchpadFrameKind::Update,
        button: TouchpadButtonState::Released,
        contacts,
    }
}
fn cancel(epoch: u64, sequence: u64) -> TouchpadFrame {
    TouchpadFrame {
        kind: TouchpadFrameKind::CancelAll,
        ..frame(epoch, sequence, vec![])
    }
}
fn projector() -> TouchpadProjector {
    TouchpadProjector::new(&stream(1), linux_touchpad_descriptor(), 0).unwrap()
}
fn ready() -> TouchpadProjector {
    let mut p = projector();
    assert_released(&p.project(&cancel(1, 0)).unwrap());
    p
}
fn value(batch: &EventBatch, event_type: u16, code: u16) -> Option<i32> {
    batch
        .as_slice()
        .iter()
        .rev()
        .find(|e| e.event_type == event_type && e.code == code)
        .map(|e| e.value)
}
fn assert_batch(batch: &EventBatch, contacts: usize) {
    let events = batch.as_slice();
    assert!(!events.is_empty() && events.len() <= MAX_EVENTS_PER_FRAME);
    assert_eq!(
        events.last(),
        Some(&InputEvent {
            event_type: 0,
            code: 0,
            value: 0
        })
    );
    assert_eq!(events.iter().filter(|e| e.event_type == 0).count(), 1);
    assert_eq!(
        events[0],
        InputEvent {
            event_type: 1,
            code: 0x14a,
            value: i32::from(contacts != 0)
        }
    );
    for (n, code) in TOOLS.iter().enumerate() {
        assert_eq!(value(batch, 1, *code), Some(i32::from(contacts == n + 1)));
    }
    assert!(
        events.iter().all(|e| e.event_type != 2),
        "never relative cursor events"
    );
    assert!(
        events
            .iter()
            .all(|e| e.event_type != 1 || e.code == 0x14a || TOOLS.contains(&e.code)),
        "never physical button events"
    );
}
fn assert_released(batch: &EventBatch) {
    assert_batch(batch, 0);
    assert_eq!(value(batch, ABS, 0x18), Some(0));
    assert!(
        batch
            .as_slice()
            .iter()
            .all(|e| e.code != TRACKING || e.value == -1)
    );
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct SlotState {
    tracking: Option<i32>,
    x: i32,
    y: i32,
    pressure: i32,
}
#[derive(Default)]
struct Trace {
    slots: [SlotState; 5],
    selected: usize,
}
impl Trace {
    fn apply(&mut self, batch: &EventBatch, contacts: usize) {
        assert_batch(batch, contacts);
        for event in batch.as_slice() {
            if event.event_type != ABS {
                continue;
            }
            match event.code {
                SLOT => {
                    assert!((0..5).contains(&event.value));
                    self.selected = event.value as usize;
                }
                TRACKING => {
                    assert!(event.value >= -1);
                    self.slots[self.selected].tracking = (event.value >= 0).then_some(event.value);
                }
                X => {
                    assert!((0..=12_000).contains(&event.value));
                    self.slots[self.selected].x = event.value;
                }
                Y => {
                    assert!((0..=7_000).contains(&event.value));
                    self.slots[self.selected].y = event.value;
                }
                PRESSURE => {
                    assert!((0..=65_535).contains(&event.value));
                    self.slots[self.selected].pressure = event.value;
                }
                0 | 1 | 0x18 => {}
                _ => panic!("unexpected ABS event: {event:?}"),
            }
        }
        let ids: Vec<_> = self.slots.iter().filter_map(|s| s.tracking).collect();
        assert_eq!(ids.len(), contacts);
        for (index, id) in ids.iter().enumerate() {
            assert!(!ids[..index].contains(id));
        }
    }
}

#[test]
fn fixed_descriptor_rejects_all_unsupported_shapes() {
    let base = linux_touchpad_descriptor();
    assert_eq!(base.physical_size.width_himetric, 12_000);
    assert_eq!(base.physical_size.height_himetric, 7_000);
    assert_eq!(base.max_contacts, 5);
    assert_eq!(base.button_type, TouchpadButtonType::NonClickable);
    assert!(base.reports_pressure && !base.reports_contact_size);
    for mutation in 0..6 {
        let mut d = base;
        match mutation {
            0 => d.physical_size.width_himetric += 1,
            1 => d.physical_size.height_himetric -= 1,
            2 => d.max_contacts = 4,
            3 => d.button_type = TouchpadButtonType::ClickPad,
            4 => d.reports_pressure = false,
            _ => d.reports_contact_size = true,
        }
        assert!(TouchpadProjector::new(&stream(1), d, 0).is_err());
    }
}

#[test]
fn initial_update_and_empty_update_do_not_replace_cancel_all() {
    let mut p = projector();
    assert_released(&p.project(&frame(1, 0, vec![contact(7)])).unwrap());
    assert_released(&p.project(&frame(1, 1, vec![])).unwrap());
    assert_released(&p.project(&frame(1, 2, vec![contact(7)])).unwrap());
    assert_released(&p.project(&cancel(1, 3)).unwrap());
    assert_batch(&p.project(&frame(1, 4, vec![contact(7)])).unwrap(), 1);
}

#[test]
fn full_five_contact_snapshot_reorder_preserves_slots_and_primary_axes() {
    let mut p = ready();
    let contacts: Vec<_> = (0..5)
        .map(|id| TouchpadContact {
            position: TouchpadPosition {
                x_himetric: 100 + id,
                y_himetric: 200 + id,
            },
            ..contact(id)
        })
        .collect();
    let first = p.project(&frame(1, 1, contacts.clone())).unwrap();
    let mut trace = Trace::default();
    trace.apply(&first, 5);
    let before = trace.slots;
    let mut reversed = contacts;
    reversed.reverse();
    let second = p.project(&frame(1, 2, reversed)).unwrap();
    trace.apply(&second, 5);
    assert_eq!(trace.slots, before);
    assert!(!second.as_slice().iter().any(|e| e.code == TRACKING));
    assert_eq!(value(&second, ABS, 0), Some(100));
    assert_eq!(value(&second, ABS, 1), Some(200));
}

#[test]
fn omitted_contacts_release_before_slot_reuse_with_fresh_local_tracking_ids() {
    let mut p = ready();
    let mut trace = Trace::default();
    trace.apply(
        &p.project(&frame(1, 1, vec![contact(u32::MAX), contact(1)]))
            .unwrap(),
        2,
    );
    let original = trace.slots[0].tracking.unwrap();
    assert!(original >= 0);
    trace.apply(&p.project(&frame(1, 2, vec![contact(1)])).unwrap(), 1);
    assert_eq!(trace.slots[0].tracking, None);
    trace.apply(
        &p.project(&frame(1, 3, vec![contact(1), contact(u32::MAX)]))
            .unwrap(),
        2,
    );
    assert!(trace.slots[0].tracking.unwrap() > original);
    let old = trace.slots;
    let replacement = p
        .project(&frame(1, 4, (10..15).map(contact).collect()))
        .unwrap();
    trace.apply(&replacement, 5);
    let first_new = replacement
        .as_slice()
        .iter()
        .position(|e| e.code == TRACKING && e.value >= 0)
        .unwrap();
    let last_release = replacement
        .as_slice()
        .iter()
        .rposition(|e| e.code == TRACKING && e.value == -1)
        .unwrap();
    assert!(last_release < first_new);
    for state in trace.slots {
        assert!(state.tracking.unwrap() > old.iter().filter_map(|s| s.tracking).max().unwrap());
    }
}

#[test]
fn pressure_is_exact_normalized_value_and_missing_measurement_is_zero() {
    let mut p = ready();
    let mut trace = Trace::default();
    for (n, pressure) in [Some(0), Some(1), Some(32_768), Some(u16::MAX), None]
        .into_iter()
        .enumerate()
    {
        let mut c = contact(3);
        c.position = TouchpadPosition {
            x_himetric: if n % 2 == 0 { 12_000 } else { 0 },
            y_himetric: if n % 2 == 0 { 7_000 } else { 0 },
        };
        c.pressure = pressure.map(NormalizedMagnitude::new);
        let batch = p.project(&frame(1, n as u64 + 1, vec![c])).unwrap();
        trace.apply(&batch, 1);
        assert_eq!(trace.slots[0].pressure, i32::from(pressure.unwrap_or(0)));
        assert_eq!(
            value(&batch, ABS, 0x18),
            Some(i32::from(pressure.unwrap_or(0)))
        );
        assert_eq!(trace.slots[0].x, c.position.x_himetric as i32);
        assert_eq!(trace.slots[0].y, c.position.y_himetric as i32);
    }
}

#[test]
fn gap_releases_all_and_requires_explicit_cancel_all_before_resume() {
    let mut p = ready();
    let mut trace = Trace::default();
    trace.apply(
        &p.project(&frame(1, 1, vec![contact(1), contact(2)]))
            .unwrap(),
        2,
    );
    trace.apply(&p.project(&frame(1, 3, vec![contact(1)])).unwrap(), 0);
    assert_released(&p.project(&frame(1, 4, vec![])).unwrap());
    assert_released(&p.project(&frame(1, 5, vec![contact(1)])).unwrap());
    assert_released(&p.project(&cancel(1, 8)).unwrap());
    trace.apply(&p.project(&frame(1, 9, vec![contact(1)])).unwrap(), 1);
}

#[test]
fn confidence_loss_cancels_entire_snapshot_until_cancel_all() {
    let mut p = ready();
    let mut trace = Trace::default();
    trace.apply(
        &p.project(&frame(1, 1, vec![contact(1), contact(2)]))
            .unwrap(),
        2,
    );
    let uncertain = TouchpadContact {
        confidence: false,
        ..contact(2)
    };
    trace.apply(
        &p.project(&frame(1, 2, vec![contact(1), uncertain]))
            .unwrap(),
        0,
    );
    assert_released(
        &p.project(&frame(1, 3, vec![contact(1), contact(2)]))
            .unwrap(),
    );
    assert_released(&p.project(&frame(1, 4, vec![])).unwrap());
    assert_released(&p.project(&frame(1, 5, vec![contact(3)])).unwrap());
    assert_released(&p.project(&cancel(1, 6)).unwrap());
    trace.apply(&p.project(&frame(1, 7, vec![contact(3)])).unwrap(), 1);
}

#[test]
fn epoch_transition_releases_and_requires_new_epoch_cancel_all() {
    let mut p = ready();
    let mut trace = Trace::default();
    trace.apply(&p.project(&frame(1, 1, vec![contact(1)])).unwrap(), 1);
    trace.apply(&p.advance_epoch(2, 0).unwrap(), 0);
    assert!(p.advance_epoch(2, 0).is_err());
    assert!(p.project(&cancel(1, 2)).is_err());
    assert_released(&p.project(&frame(2, 0, vec![contact(1)])).unwrap());
    assert_released(&p.project(&cancel(2, 1)).unwrap());
    trace.apply(&p.project(&frame(2, 2, vec![contact(1)])).unwrap(), 1);
    assert!(trace.slots[0].tracking.unwrap() > 0);
}

#[test]
fn malformed_snapshot_rejection_is_transactional() {
    let baseline = ready();
    for mutation in 0..12 {
        let mut p = baseline.clone();
        let mut bad = frame(1, 1, vec![contact(1)]);
        match mutation {
            0 => bad.contacts.push(contact(1)),
            1 => bad.contacts = (0..6).map(contact).collect(),
            2 => bad.contacts[0].position.x_himetric = 12_001,
            3 => bad.contacts[0].position.y_himetric = u32::MAX,
            4 => {
                bad.contacts[0].size = Some(TouchpadContactSize {
                    width_himetric: 1,
                    height_himetric: 1,
                })
            }
            5 => bad.button = TouchpadButtonState::Pressed,
            6 => bad.kind = TouchpadFrameKind::CancelAll,
            7 => bad.header.stream_epoch = 2,
            8 => bad.header.stream_id = "00000000-0000-4000-8000-00000000c704".parse().unwrap(),
            9 => bad.header.stream_epoch = 0,
            10 => bad.header.sequence = 0,
            _ => {
                bad.kind = TouchpadFrameKind::CancelAll;
                bad.contacts.clear();
                bad.button = TouchpadButtonState::Pressed;
            }
        }
        assert!(p.project(&bad).is_err(), "mutation {mutation}");
        let valid = frame(1, 1, vec![contact(1)]);
        assert_eq!(
            p.project(&valid).unwrap(),
            baseline.clone().project(&valid).unwrap(),
            "mutation {mutation} advanced state"
        );
    }
}

#[test]
fn timestamp_regression_and_replay_do_not_advance_state() {
    let mut p = ready();
    p.project(&frame(1, 1, vec![contact(1)])).unwrap();
    let baseline = p.clone();
    let mut bad = frame(1, 2, vec![contact(2)]);
    bad.header.source_timestamp_nanos = 999;
    assert!(p.project(&bad).is_err());
    assert!(p.project(&frame(1, 1, vec![contact(2)])).is_err());
    let valid = frame(1, 2, vec![contact(2)]);
    assert_eq!(
        p.project(&valid).unwrap(),
        baseline.clone().project(&valid).unwrap()
    );
}

#[test]
fn explicit_release_is_idempotent_and_resets_all_keys_and_slots() {
    let mut p = ready();
    let mut trace = Trace::default();
    trace.apply(
        &p.project(&frame(1, 1, (0..5).map(contact).collect()))
            .unwrap(),
        5,
    );
    let released = p.release_all();
    assert_eq!(
        released
            .as_slice()
            .iter()
            .filter(|e| e.code == TRACKING && e.value == -1)
            .count(),
        5
    );
    trace.apply(&released, 0);
    trace.apply(&p.release_all(), 0);
}

#[test]
fn deterministic_randomized_snapshots_preserve_bounded_trace_and_slot_lifetimes() {
    let mut p = ready();
    let mut trace = Trace::default();
    let mut seed = 0x72e3_98bc_u64;
    let mut last_ids = Vec::<u32>::new();
    let mut previous = std::collections::BTreeMap::<u32, (usize, i32)>::new();
    for sequence in 1..=4_096_u64 {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        let count = (seed % 6) as usize;
        let shift = ((seed >> 8) % 12) as u32;
        let mut contacts: Vec<_> = (0..count)
            .map(|index| {
                let id = (shift + index as u32) % 12;
                TouchpadContact {
                    position: TouchpadPosition {
                        x_himetric: ((seed >> (index + 1)) % 12_001) as u32,
                        y_himetric: ((seed >> (index + 5)) % 7_001) as u32,
                    },
                    pressure: Some(NormalizedMagnitude::new((seed >> (index + 9)) as u16)),
                    ..contact(id)
                }
            })
            .collect();
        if seed & 1 != 0 {
            contacts.reverse();
        }
        let valid = frame(1, sequence, contacts.clone());
        if sequence % 7 == 0 {
            let mut bad = valid.clone();
            bad.contacts = vec![contact(3), contact(3)];
            assert!(p.project(&bad).is_err());
        }
        let batch = p.project(&valid).unwrap();
        trace.apply(&batch, count);
        let mut selected = 0;
        let mut next = std::collections::BTreeMap::new();
        let mut index = 0;
        for e in batch.as_slice() {
            if e.event_type == ABS && e.code == SLOT {
                selected = e.value as usize;
            }
            if e.event_type == ABS && e.code == X {
                let c = contacts[index];
                let tracking = trace.slots[selected].tracking.unwrap();
                if let Some(old) = previous.get(&c.contact_id) {
                    assert_eq!(*old, (selected, tracking));
                }
                next.insert(c.contact_id, (selected, tracking));
                index += 1;
            }
        }
        for old in &last_ids {
            if !next.contains_key(old) {
                let old_tracking = previous[old].1;
                assert!(!trace.slots.iter().any(|s| s.tracking == Some(old_tracking)));
            }
        }
        last_ids = contacts.iter().map(|c| c.contact_id).collect();
        previous = next;
    }
}
