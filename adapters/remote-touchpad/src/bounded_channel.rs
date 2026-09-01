use std::{cell::RefCell, collections::VecDeque, error::Error, fmt, rc::Rc};

use crate::{
    PrivateTouchpadAdmittedChannel, PrivateTouchpadChannelAdmissionError,
    PrivateTouchpadChannelSendOutcome, PrivateTouchpadPacketV1, PrivateTouchpadRouteBinding,
};

pub const MAX_PRIVATE_TOUCHPAD_HOST_CHANNEL_PACKETS: u8 = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrivateTouchpadHostChannelBuildError {
    InvalidCapacity {
        actual: u8,
        minimum: u8,
        maximum: u8,
    },
}

impl fmt::Display for PrivateTouchpadHostChannelBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCapacity {
                actual,
                minimum,
                maximum,
            } => write!(
                formatter,
                "private touchpad host channel capacity {actual} is outside {minimum}..={maximum}"
            ),
        }
    }
}

impl Error for PrivateTouchpadHostChannelBuildError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PrivateTouchpadHostChannelReceiveOutcome {
    Packet(PrivateTouchpadPacketV1),
    Empty,
    Closed,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PrivateTouchpadHostChannelMetrics {
    pub packets_enqueued: u64,
    pub packets_received: u64,
    pub rejected_before_write: u64,
    pub packets_discarded: u64,
    pub sender_closes: u64,
    pub receiver_closes: u64,
}

struct PrivateTouchpadHostChannelState {
    binding: PrivateTouchpadRouteBinding,
    admission_error: Option<PrivateTouchpadChannelAdmissionError>,
    capacity: usize,
    queue: VecDeque<PrivateTouchpadPacketV1>,
    sender_closed: bool,
    receiver_closed: bool,
    metrics: PrivateTouchpadHostChannelMetrics,
}

impl PrivateTouchpadHostChannelState {
    fn discard_queue(&mut self) {
        self.metrics.packets_discarded = self
            .metrics
            .packets_discarded
            .saturating_add(self.queue.len() as u64);
        self.queue.clear();
    }
}

type SharedHostChannelState = Rc<RefCell<PrivateTouchpadHostChannelState>>;

pub struct PrivateTouchpadHostChannelAdmission {
    shared: SharedHostChannelState,
}

impl PrivateTouchpadHostChannelAdmission {
    pub fn replace_binding(&mut self, binding: PrivateTouchpadRouteBinding) {
        let mut state = self.shared.borrow_mut();
        state.discard_queue();
        state.binding = binding;
        state.admission_error = None;
    }

    pub fn deny(&mut self, error: PrivateTouchpadChannelAdmissionError) {
        let mut state = self.shared.borrow_mut();
        state.discard_queue();
        state.admission_error = Some(error);
    }

    #[must_use]
    pub fn metrics(&self) -> PrivateTouchpadHostChannelMetrics {
        self.shared.borrow().metrics
    }
}

pub struct PrivateTouchpadHostChannel {
    shared: SharedHostChannelState,
}

impl PrivateTouchpadAdmittedChannel for PrivateTouchpadHostChannel {
    fn current_binding(
        &self,
    ) -> Result<PrivateTouchpadRouteBinding, PrivateTouchpadChannelAdmissionError> {
        let state = self.shared.borrow();
        if state.sender_closed || state.receiver_closed {
            return Err(PrivateTouchpadChannelAdmissionError::Unavailable);
        }
        if let Some(error) = state.admission_error {
            return Err(error);
        }
        Ok(state.binding.clone())
    }

    fn send(&mut self, packet: &PrivateTouchpadPacketV1) -> PrivateTouchpadChannelSendOutcome {
        let mut state = self.shared.borrow_mut();
        if state.sender_closed
            || state.receiver_closed
            || state.admission_error.is_some()
            || state.queue.len() == state.capacity
        {
            state.metrics.rejected_before_write =
                state.metrics.rejected_before_write.saturating_add(1);
            return PrivateTouchpadChannelSendOutcome::RejectedBeforeWrite;
        }
        state.queue.push_back(packet.clone());
        state.metrics.packets_enqueued = state.metrics.packets_enqueued.saturating_add(1);
        PrivateTouchpadChannelSendOutcome::Delivered
    }

    fn close(&mut self) {
        let mut state = self.shared.borrow_mut();
        if !state.sender_closed {
            state.sender_closed = true;
            state.metrics.sender_closes = state.metrics.sender_closes.saturating_add(1);
        }
    }
}

impl Drop for PrivateTouchpadHostChannel {
    fn drop(&mut self) {
        self.close();
    }
}

pub struct PrivateTouchpadHostChannelReceiver {
    shared: SharedHostChannelState,
}

impl PrivateTouchpadHostChannelReceiver {
    pub fn receive(&mut self) -> PrivateTouchpadHostChannelReceiveOutcome {
        let mut state = self.shared.borrow_mut();
        if state.receiver_closed {
            return PrivateTouchpadHostChannelReceiveOutcome::Closed;
        }
        if let Some(packet) = state.queue.pop_front() {
            state.metrics.packets_received = state.metrics.packets_received.saturating_add(1);
            return PrivateTouchpadHostChannelReceiveOutcome::Packet(packet);
        }
        if state.sender_closed {
            PrivateTouchpadHostChannelReceiveOutcome::Closed
        } else {
            PrivateTouchpadHostChannelReceiveOutcome::Empty
        }
    }

    pub fn close(&mut self) {
        let mut state = self.shared.borrow_mut();
        if !state.receiver_closed {
            state.receiver_closed = true;
            state.discard_queue();
            state.metrics.receiver_closes = state.metrics.receiver_closes.saturating_add(1);
        }
    }

    #[must_use]
    pub fn metrics(&self) -> PrivateTouchpadHostChannelMetrics {
        self.shared.borrow().metrics
    }
}

impl Drop for PrivateTouchpadHostChannelReceiver {
    fn drop(&mut self) {
        self.close();
    }
}

pub fn private_touchpad_host_channel(
    binding: PrivateTouchpadRouteBinding,
    capacity: u8,
) -> Result<
    (
        PrivateTouchpadHostChannelAdmission,
        PrivateTouchpadHostChannel,
        PrivateTouchpadHostChannelReceiver,
    ),
    PrivateTouchpadHostChannelBuildError,
> {
    if !(1..=MAX_PRIVATE_TOUCHPAD_HOST_CHANNEL_PACKETS).contains(&capacity) {
        return Err(PrivateTouchpadHostChannelBuildError::InvalidCapacity {
            actual: capacity,
            minimum: 1,
            maximum: MAX_PRIVATE_TOUCHPAD_HOST_CHANNEL_PACKETS,
        });
    }
    let shared = Rc::new(RefCell::new(PrivateTouchpadHostChannelState {
        binding,
        admission_error: None,
        capacity: usize::from(capacity),
        queue: VecDeque::with_capacity(usize::from(capacity)),
        sender_closed: false,
        receiver_closed: false,
        metrics: PrivateTouchpadHostChannelMetrics::default(),
    }));
    Ok((
        PrivateTouchpadHostChannelAdmission {
            shared: Rc::clone(&shared),
        },
        PrivateTouchpadHostChannel {
            shared: Rc::clone(&shared),
        },
        PrivateTouchpadHostChannelReceiver { shared },
    ))
}
