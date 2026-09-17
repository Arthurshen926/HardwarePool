use std::{
    cell::UnsafeCell,
    mem::MaybeUninit,
    sync::atomic::{AtomicU8, AtomicU64, AtomicUsize, Ordering},
};

use capyio_display_stream_adapter::engine_abi::{EngineEventKind, EngineEventV2};

const EVENT_CAPACITY: usize = 64;
const EMPTY: u8 = 0;
const WRITING: u8 = 1;
const READY: u8 = 2;
const READING: u8 = 3;

struct EventSlot {
    state: AtomicU8,
    value: UnsafeCell<MaybeUninit<EngineEventV2>>,
}

impl EventSlot {
    const fn new() -> Self {
        Self {
            state: AtomicU8::new(EMPTY),
            value: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }
}

// SAFETY: producers obtain exclusive slot write access through EMPTY->WRITING,
// and the consumer obtains exclusive read access through READY->READING. State
// publication uses acquire/release ordering and EngineEventV2 is Copy.
unsafe impl Sync for EventSlot {}

pub(crate) struct EngineEventQueue {
    expected_adapter: [u8; 16],
    expected_display: [u8; 16],
    cursor: AtomicUsize,
    dropped: AtomicU64,
    invalid: AtomicU64,
    fatal: EventSlot,
    slots: [EventSlot; EVENT_CAPACITY],
}

impl EngineEventQueue {
    pub(crate) fn new(expected_adapter: [u8; 16], expected_display: [u8; 16]) -> Self {
        Self {
            expected_adapter,
            expected_display,
            cursor: AtomicUsize::new(0),
            dropped: AtomicU64::new(0),
            invalid: AtomicU64::new(0),
            fatal: EventSlot::new(),
            slots: [const { EventSlot::new() }; EVENT_CAPACITY],
        }
    }

    pub(crate) fn accept_callback(&self, event: *const EngineEventV2) {
        if event.is_null() {
            self.invalid.fetch_add(1, Ordering::Relaxed);
            return;
        }
        // SAFETY: the ABI contract requires a readable event pointer for the
        // callback duration; the record is copied before callback return.
        let event = unsafe { *event };
        if event.validate().is_err()
            || event.adapter_instance_id != self.expected_adapter
            || event.display_capability_id != self.expected_display
        {
            self.invalid.fetch_add(1, Ordering::Relaxed);
            return;
        }
        self.push(event);
    }

    fn push(&self, event: EngineEventV2) {
        if event.kind == EngineEventKind::Fatal as u32 {
            if self.write_slot(&self.fatal, event) {
                return;
            }
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return;
        }
        let start = self.cursor.fetch_add(1, Ordering::Relaxed);
        for offset in 0..EVENT_CAPACITY {
            let slot = &self.slots[start.wrapping_add(offset) % EVENT_CAPACITY];
            if self.write_slot(slot, event) {
                return;
            }
        }
        self.dropped.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn drain(&self) -> EngineEventBatch {
        let mut events = Vec::with_capacity(EVENT_CAPACITY + 1);
        if let Some(event) = Self::read_slot(&self.fatal) {
            events.push(event);
        }
        for slot in &self.slots {
            if let Some(event) = Self::read_slot(slot) {
                events.push(event);
            }
        }
        events.sort_unstable_by_key(|event| event.sequence);
        EngineEventBatch {
            events,
            dropped: self.dropped.swap(0, Ordering::Relaxed),
            invalid: self.invalid.swap(0, Ordering::Relaxed),
        }
    }

    fn write_slot(&self, slot: &EventSlot, event: EngineEventV2) -> bool {
        if slot
            .state
            .compare_exchange(EMPTY, WRITING, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return false;
        }
        // SAFETY: this producer exclusively owns a WRITING slot.
        unsafe { (*slot.value.get()).write(event) };
        slot.state.store(READY, Ordering::Release);
        true
    }

    fn read_slot(slot: &EventSlot) -> Option<EngineEventV2> {
        if slot
            .state
            .compare_exchange(READY, READING, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return None;
        }
        // SAFETY: this consumer exclusively owns a published READING slot and
        // EngineEventV2 is initialized before READY release.
        let event = unsafe { (*slot.value.get()).assume_init_read() };
        slot.state.store(EMPTY, Ordering::Release);
        Some(event)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct EngineEventBatch {
    pub events: Vec<EngineEventV2>,
    pub dropped: u64,
    pub invalid: u64,
}

#[cfg(test)]
mod tests {
    use std::{mem::size_of, sync::Arc, thread};

    use capyio_display_stream_adapter::engine_abi::{
        ENGINE_ABI_V2, EngineCallStatus, EngineEventKind,
    };

    use super::*;

    fn event(sequence: u64) -> EngineEventV2 {
        EngineEventV2 {
            struct_size: size_of::<EngineEventV2>() as u32,
            abi_version: ENGINE_ABI_V2,
            kind: EngineEventKind::CongestionChanged as u32,
            status: EngineCallStatus::Ok as i32,
            adapter_instance_id: [1; 16],
            display_capability_id: [2; 16],
            sequence,
            value: sequence,
        }
    }

    #[test]
    fn concurrent_producers_are_bounded_and_consumer_orders_retained_events() {
        let queue = Arc::new(EngineEventQueue::new([1; 16], [2; 16]));
        let mut workers = Vec::new();
        for worker in 0..4 {
            let queue = Arc::clone(&queue);
            workers.push(thread::spawn(move || {
                for index in 0..64 {
                    queue.push(event(worker * 64 + index + 1));
                }
            }));
        }
        for worker in workers {
            worker.join().unwrap();
        }
        let batch = queue.drain();
        assert_eq!(batch.events.len(), EVENT_CAPACITY);
        assert_eq!(batch.dropped, 192);
        assert!(
            batch
                .events
                .windows(2)
                .all(|pair| pair[0].sequence < pair[1].sequence)
        );
    }

    #[test]
    fn callback_rejects_null_invalid_and_cross_identity_records() {
        let queue = EngineEventQueue::new([1; 16], [2; 16]);
        queue.accept_callback(std::ptr::null());
        let mut invalid = event(1);
        invalid.display_capability_id = [3; 16];
        queue.accept_callback(&invalid);
        let batch = queue.drain();
        assert!(batch.events.is_empty());
        assert_eq!(batch.invalid, 2);
    }

    #[test]
    fn fatal_event_survives_full_ordinary_queue() {
        let queue = EngineEventQueue::new([1; 16], [2; 16]);
        for sequence in 1..=EVENT_CAPACITY as u64 {
            queue.push(event(sequence));
        }
        let mut fatal = event(EVENT_CAPACITY as u64 + 1);
        fatal.kind = EngineEventKind::Fatal as u32;
        fatal.status = EngineCallStatus::InternalFailure as i32;
        queue.push(fatal);

        let batch = queue.drain();
        assert_eq!(batch.events.len(), EVENT_CAPACITY + 1);
        assert_eq!(batch.dropped, 0);
        assert_eq!(batch.events.last(), Some(&fatal));
    }
}
