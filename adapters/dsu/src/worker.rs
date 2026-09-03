use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::io;
use std::net::SocketAddrV4;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use capyio_data_plane::{
    BoundedEnvelopeQueue, DataEnvelope, DataPlaneError, ImuSampleV1, PushOutcome,
};
use capyio_input::{
    GamepadControls, GamepadState, InputContractError, InputSequenceOutcome, InputSequenceTracker,
};

use crate::protocol::validate_dsu_controls;
use crate::{
    DsuControlsMapping, DsuLoopbackConfig, DsuLoopbackServer, DsuMotionMapping, DsuMotionSample,
    DsuPacketError, DsuPollStats, DsuPublishStats, DsuTransportError, MotionProjectionError,
    project_imu_envelope,
};

pub const MAX_DSU_WORKER_QUEUE_CAPACITY: usize = 1_024;
pub const DEFAULT_DSU_WORKER_QUEUE_CAPACITY: usize = 64;
pub const DEFAULT_DSU_CONTROLS_QUEUE_CAPACITY: usize = 64;
const MAX_DSU_INPUTS_PER_STREAM_PER_CYCLE: usize = 16;
pub const MAX_DSU_WORKER_POLL_INTERVAL: Duration = Duration::from_millis(100);
pub const DEFAULT_DSU_WORKER_POLL_INTERVAL: Duration = Duration::from_millis(2);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DsuImuWorkerConfig {
    pub server: DsuLoopbackConfig,
    pub queue_capacity: usize,
    pub controls_queue_capacity: usize,
    pub poll_interval: Duration,
    pub motion_mapping: DsuMotionMapping,
    pub controls_mapping: DsuControlsMapping,
}

impl DsuImuWorkerConfig {
    #[must_use]
    pub const fn new(server: DsuLoopbackConfig) -> Self {
        Self {
            server,
            queue_capacity: DEFAULT_DSU_WORKER_QUEUE_CAPACITY,
            controls_queue_capacity: DEFAULT_DSU_CONTROLS_QUEUE_CAPACITY,
            poll_interval: DEFAULT_DSU_WORKER_POLL_INTERVAL,
            motion_mapping: DsuMotionMapping::identity(),
            controls_mapping: DsuControlsMapping::identity(),
        }
    }

    fn validate(self) -> Result<Self, DsuWorkerError> {
        if self.queue_capacity == 0 || self.queue_capacity > MAX_DSU_WORKER_QUEUE_CAPACITY {
            return Err(DsuWorkerError::InvalidQueueCapacity {
                actual: self.queue_capacity,
                maximum: MAX_DSU_WORKER_QUEUE_CAPACITY,
            });
        }
        if self.poll_interval.is_zero() || self.poll_interval > MAX_DSU_WORKER_POLL_INTERVAL {
            return Err(DsuWorkerError::InvalidPollInterval {
                actual: self.poll_interval,
                maximum: MAX_DSU_WORKER_POLL_INTERVAL,
            });
        }
        Ok(self)
    }

    fn validate_controls_queue(self) -> Result<Self, DsuWorkerError> {
        if self.controls_queue_capacity == 0
            || self.controls_queue_capacity > MAX_DSU_WORKER_QUEUE_CAPACITY
        {
            return Err(DsuWorkerError::InvalidControlsQueueCapacity {
                actual: self.controls_queue_capacity,
                maximum: MAX_DSU_WORKER_QUEUE_CAPACITY,
            });
        }
        Ok(self)
    }
}

#[derive(Debug)]
pub enum DsuWorkerError {
    InvalidQueueCapacity { actual: usize, maximum: usize },
    InvalidControlsQueueCapacity { actual: usize, maximum: usize },
    InvalidPollInterval { actual: Duration, maximum: Duration },
    InputQueue(DataPlaneError),
    InputContract(InputContractError),
    ControlsProjection(DsuPacketError),
    Transport(DsuTransportError),
    Spawn(io::Error),
    WorkerPanicked,
}

impl Display for DsuWorkerError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidQueueCapacity { actual, maximum } => write!(
                formatter,
                "DSU worker queue capacity {actual} is outside 1..={maximum}"
            ),
            Self::InvalidControlsQueueCapacity { actual, maximum } => write!(
                formatter,
                "DSU worker controls queue capacity {actual} is outside 1..={maximum}"
            ),
            Self::InvalidPollInterval { actual, maximum } => write!(
                formatter,
                "DSU worker poll interval {actual:?} is outside 1ns..={maximum:?}"
            ),
            Self::InputQueue(error) => {
                write!(formatter, "DSU worker input queue is invalid: {error}")
            }
            Self::InputContract(error) => {
                write!(formatter, "DSU worker controls anchor is invalid: {error}")
            }
            Self::ControlsProjection(error) => {
                write!(formatter, "DSU worker controls mapping is invalid: {error}")
            }
            Self::Transport(error) => write!(formatter, "DSU worker transport failed: {error}"),
            Self::Spawn(error) => write!(formatter, "DSU worker thread spawn failed: {error}"),
            Self::WorkerPanicked => formatter.write_str("DSU worker thread panicked"),
        }
    }
}

impl Error for DsuWorkerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InputQueue(error) => Some(error),
            Self::InputContract(error) => Some(error),
            Self::ControlsProjection(error) => Some(error),
            Self::Transport(error) => Some(error),
            Self::Spawn(error) => Some(error),
            Self::InvalidQueueCapacity { .. }
            | Self::InvalidControlsQueueCapacity { .. }
            | Self::InvalidPollInterval { .. }
            | Self::WorkerPanicked => None,
        }
    }
}

impl From<DataPlaneError> for DsuWorkerError {
    fn from(error: DataPlaneError) -> Self {
        Self::InputQueue(error)
    }
}

impl From<DsuTransportError> for DsuWorkerError {
    fn from(error: DsuTransportError) -> Self {
        Self::Transport(error)
    }
}

impl From<InputContractError> for DsuWorkerError {
    fn from(error: InputContractError) -> Self {
        Self::InputContract(error)
    }
}

impl From<DsuPacketError> for DsuWorkerError {
    fn from(error: DsuPacketError) -> Self {
        Self::ControlsProjection(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DsuSubmitOutcome {
    Submitted,
    QueueFull,
    Stopped,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DsuNeutralOutcome {
    Requested,
    Stopped,
}

#[derive(Clone)]
pub struct DsuImuWorkerSender {
    sender: SyncSender<DataEnvelope<ImuSampleV1>>,
    stopped: Arc<AtomicBool>,
    counters: Arc<WorkerCounters>,
}

impl DsuImuWorkerSender {
    pub fn try_submit(&self, envelope: DataEnvelope<ImuSampleV1>) -> DsuSubmitOutcome {
        if self.stopped.load(Ordering::Acquire) {
            return DsuSubmitOutcome::Stopped;
        }
        match self.sender.try_send(envelope) {
            Ok(()) => {
                increment(&self.counters.samples_submitted, 1);
                DsuSubmitOutcome::Submitted
            }
            Err(TrySendError::Full(_)) => {
                increment(&self.counters.queue_full, 1);
                DsuSubmitOutcome::QueueFull
            }
            Err(TrySendError::Disconnected(_)) => {
                self.stopped.store(true, Ordering::Release);
                DsuSubmitOutcome::Stopped
            }
        }
    }
}

/// Non-blocking sender for the independent complete-gamepad-state stream.
///
/// Queue overflow requests a fail-safe neutral transition in the worker before
/// the next accepted state is projected.
#[derive(Clone)]
pub struct DsuGamepadWorkerSender {
    sender: SyncSender<QueuedControls>,
    stopped: Arc<AtomicBool>,
    controls_generation: Arc<AtomicU64>,
    counters: Arc<WorkerCounters>,
}

impl DsuGamepadWorkerSender {
    pub fn try_submit(&self, state: GamepadState) -> DsuSubmitOutcome {
        if self.stopped.load(Ordering::Acquire) {
            return DsuSubmitOutcome::Stopped;
        }
        let generation = self.controls_generation.load(Ordering::Acquire);
        match self.sender.try_send(QueuedControls { state, generation }) {
            Ok(()) => {
                increment(&self.counters.controls_submitted, 1);
                DsuSubmitOutcome::Submitted
            }
            Err(TrySendError::Full(_)) => {
                increment(&self.counters.controls_queue_full, 1);
                if advance_controls_generation(&self.controls_generation) {
                    DsuSubmitOutcome::QueueFull
                } else {
                    increment(&self.counters.controls_generation_exhausted, 1);
                    self.stopped.store(true, Ordering::Release);
                    DsuSubmitOutcome::Stopped
                }
            }
            Err(TrySendError::Disconnected(_)) => {
                self.stopped.store(true, Ordering::Release);
                DsuSubmitOutcome::Stopped
            }
        }
    }

    /// Requests a non-blocking fail-safe neutral transition for upstream Route
    /// offline/failed/stopped or peer-loss lifecycle signals.
    pub fn request_neutral(&self) -> DsuNeutralOutcome {
        if self.stopped.load(Ordering::Acquire) {
            return DsuNeutralOutcome::Stopped;
        }
        increment(&self.counters.controls_neutral_requests, 1);
        if advance_controls_generation(&self.controls_generation) {
            DsuNeutralOutcome::Requested
        } else {
            increment(&self.counters.controls_generation_exhausted, 1);
            self.stopped.store(true, Ordering::Release);
            DsuNeutralOutcome::Stopped
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct QueuedControls {
    state: GamepadState,
    generation: u64,
}

struct WorkerReceivers {
    motion: Receiver<DataEnvelope<ImuSampleV1>>,
    controls: Option<Receiver<QueuedControls>>,
}

struct WorkerRunConfig {
    motion_mapping: DsuMotionMapping,
    controls_mapping: DsuControlsMapping,
    controls_tracker: Option<InputSequenceTracker>,
    poll_interval: Duration,
}

struct WorkerShared {
    stopped: Arc<AtomicBool>,
    controls_generation: Arc<AtomicU64>,
    counters: Arc<WorkerCounters>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DsuImuWorkerStats {
    pub samples_submitted: u64,
    pub queue_full: u64,
    pub internal_motion_queue_full: u64,
    pub samples_accepted: u64,
    pub input_gaps: u64,
    pub missing_sequences: u64,
    pub late_samples: u64,
    pub wrong_stream_samples: u64,
    pub stale_epoch_samples: u64,
    pub future_epoch_samples: u64,
    pub invalid_envelopes: u64,
    pub projection_errors: u64,
    pub controls_submitted: u64,
    pub controls_queue_full: u64,
    pub controls_accepted: u64,
    pub controls_gaps: u64,
    pub controls_missing_sequences: u64,
    pub late_controls: u64,
    pub wrong_stream_controls: u64,
    pub stale_epoch_controls: u64,
    pub future_epoch_controls: u64,
    pub invalid_controls: u64,
    pub unsupported_controls: u64,
    pub exhausted_controls: u64,
    pub controls_cached_without_motion: u64,
    pub controls_neutral_resets: u64,
    pub controls_neutral_packets_sent: u64,
    pub controls_stop_neutral_packets: u64,
    pub controls_failure_neutral_packets: u64,
    pub controls_stale_generation_drops: u64,
    pub controls_neutral_requests: u64,
    pub controls_generation_exhausted: u64,
    pub dsu_datagrams_received: u64,
    pub malformed_dsu_datagrams: u64,
    pub dsu_responses_sent: u64,
    pub subscriptions_added: u64,
    pub subscriptions_renewed: u64,
    pub subscriptions_replaced: u64,
    pub subscriptions_rejected_full: u64,
    pub subscriptions_expired: u64,
    pub active_subscribers: u64,
    pub motion_packets_sent: u64,
    pub motion_packets_would_block: u64,
    pub motion_packet_send_errors: u64,
    pub dsu_pad_packets_sent: u64,
    pub dsu_pad_packets_would_block: u64,
    pub dsu_pad_packet_send_errors: u64,
    pub transport_failures: u64,
    pub stopped: bool,
}

#[derive(Default)]
struct WorkerCounters {
    samples_submitted: AtomicU64,
    queue_full: AtomicU64,
    internal_motion_queue_full: AtomicU64,
    samples_accepted: AtomicU64,
    input_gaps: AtomicU64,
    missing_sequences: AtomicU64,
    late_samples: AtomicU64,
    wrong_stream_samples: AtomicU64,
    stale_epoch_samples: AtomicU64,
    future_epoch_samples: AtomicU64,
    invalid_envelopes: AtomicU64,
    projection_errors: AtomicU64,
    controls_submitted: AtomicU64,
    controls_queue_full: AtomicU64,
    controls_accepted: AtomicU64,
    controls_gaps: AtomicU64,
    controls_missing_sequences: AtomicU64,
    late_controls: AtomicU64,
    wrong_stream_controls: AtomicU64,
    stale_epoch_controls: AtomicU64,
    future_epoch_controls: AtomicU64,
    invalid_controls: AtomicU64,
    unsupported_controls: AtomicU64,
    exhausted_controls: AtomicU64,
    controls_cached_without_motion: AtomicU64,
    controls_neutral_resets: AtomicU64,
    controls_neutral_packets_sent: AtomicU64,
    controls_stop_neutral_packets: AtomicU64,
    controls_failure_neutral_packets: AtomicU64,
    controls_stale_generation_drops: AtomicU64,
    controls_neutral_requests: AtomicU64,
    controls_generation_exhausted: AtomicU64,
    dsu_datagrams_received: AtomicU64,
    malformed_dsu_datagrams: AtomicU64,
    dsu_responses_sent: AtomicU64,
    subscriptions_added: AtomicU64,
    subscriptions_renewed: AtomicU64,
    subscriptions_replaced: AtomicU64,
    subscriptions_rejected_full: AtomicU64,
    subscriptions_expired: AtomicU64,
    active_subscribers: AtomicU64,
    motion_packets_sent: AtomicU64,
    motion_packets_would_block: AtomicU64,
    motion_packet_send_errors: AtomicU64,
    dsu_pad_packets_sent: AtomicU64,
    dsu_pad_packets_would_block: AtomicU64,
    dsu_pad_packet_send_errors: AtomicU64,
    transport_failures: AtomicU64,
}

impl WorkerCounters {
    fn snapshot(&self, stopped: bool) -> DsuImuWorkerStats {
        DsuImuWorkerStats {
            samples_submitted: load(&self.samples_submitted),
            queue_full: load(&self.queue_full),
            internal_motion_queue_full: load(&self.internal_motion_queue_full),
            samples_accepted: load(&self.samples_accepted),
            input_gaps: load(&self.input_gaps),
            missing_sequences: load(&self.missing_sequences),
            late_samples: load(&self.late_samples),
            wrong_stream_samples: load(&self.wrong_stream_samples),
            stale_epoch_samples: load(&self.stale_epoch_samples),
            future_epoch_samples: load(&self.future_epoch_samples),
            invalid_envelopes: load(&self.invalid_envelopes),
            projection_errors: load(&self.projection_errors),
            controls_submitted: load(&self.controls_submitted),
            controls_queue_full: load(&self.controls_queue_full),
            controls_accepted: load(&self.controls_accepted),
            controls_gaps: load(&self.controls_gaps),
            controls_missing_sequences: load(&self.controls_missing_sequences),
            late_controls: load(&self.late_controls),
            wrong_stream_controls: load(&self.wrong_stream_controls),
            stale_epoch_controls: load(&self.stale_epoch_controls),
            future_epoch_controls: load(&self.future_epoch_controls),
            invalid_controls: load(&self.invalid_controls),
            unsupported_controls: load(&self.unsupported_controls),
            exhausted_controls: load(&self.exhausted_controls),
            controls_cached_without_motion: load(&self.controls_cached_without_motion),
            controls_neutral_resets: load(&self.controls_neutral_resets),
            controls_neutral_packets_sent: load(&self.controls_neutral_packets_sent),
            controls_stop_neutral_packets: load(&self.controls_stop_neutral_packets),
            controls_failure_neutral_packets: load(&self.controls_failure_neutral_packets),
            controls_stale_generation_drops: load(&self.controls_stale_generation_drops),
            controls_neutral_requests: load(&self.controls_neutral_requests),
            controls_generation_exhausted: load(&self.controls_generation_exhausted),
            dsu_datagrams_received: load(&self.dsu_datagrams_received),
            malformed_dsu_datagrams: load(&self.malformed_dsu_datagrams),
            dsu_responses_sent: load(&self.dsu_responses_sent),
            subscriptions_added: load(&self.subscriptions_added),
            subscriptions_renewed: load(&self.subscriptions_renewed),
            subscriptions_replaced: load(&self.subscriptions_replaced),
            subscriptions_rejected_full: load(&self.subscriptions_rejected_full),
            subscriptions_expired: load(&self.subscriptions_expired),
            active_subscribers: load(&self.active_subscribers),
            motion_packets_sent: load(&self.motion_packets_sent),
            motion_packets_would_block: load(&self.motion_packets_would_block),
            motion_packet_send_errors: load(&self.motion_packet_send_errors),
            dsu_pad_packets_sent: load(&self.dsu_pad_packets_sent),
            dsu_pad_packets_would_block: load(&self.dsu_pad_packets_would_block),
            dsu_pad_packet_send_errors: load(&self.dsu_pad_packet_send_errors),
            transport_failures: load(&self.transport_failures),
            stopped,
        }
    }
}

/// Bounded, caller-owned worker that maps one fixed IMU stream epoch to DSU.
///
/// The validated `stream_anchor` supplied to [`Self::start`] contributes only
/// its stream identity and epoch; it is not submitted as a sample. A new Route
/// epoch starts a new worker. `stop` is idempotent and joins the thread;
/// dropping the worker performs the same bounded shutdown request.
pub struct DsuImuWorker {
    local_address: SocketAddrV4,
    sender: DsuImuWorkerSender,
    controls_sender: Option<DsuGamepadWorkerSender>,
    stopped: Arc<AtomicBool>,
    counters: Arc<WorkerCounters>,
    thread: Option<JoinHandle<Result<(), DsuTransportError>>>,
}

impl DsuImuWorker {
    pub fn start(
        config: DsuImuWorkerConfig,
        stream_anchor: &DataEnvelope<ImuSampleV1>,
    ) -> Result<Self, DsuWorkerError> {
        Self::start_internal(config, stream_anchor, None)
    }

    /// Starts a worker that combines independent IMU and gamepad-state streams.
    ///
    /// Both anchors identify a fixed stream epoch and neither is submitted
    /// implicitly. The controls anchor selects the first accepted controls
    /// sequence; the IMU queue derives its first sequence from the first
    /// submitted envelope. Controls received before the first valid motion
    /// sample are cached without emitting a DSU packet.
    pub fn start_with_controls(
        config: DsuImuWorkerConfig,
        motion_anchor: &DataEnvelope<ImuSampleV1>,
        controls_anchor: &GamepadState,
    ) -> Result<Self, DsuWorkerError> {
        controls_anchor.validate()?;
        validate_dsu_controls(controls_anchor.controls)?;
        let tracker = InputSequenceTracker::new(
            controls_anchor.header.stream_id,
            controls_anchor.header.stream_epoch,
            controls_anchor.header.sequence,
        )?;
        Self::start_internal(config, motion_anchor, Some(tracker))
    }

    fn start_internal(
        config: DsuImuWorkerConfig,
        stream_anchor: &DataEnvelope<ImuSampleV1>,
        controls_tracker: Option<InputSequenceTracker>,
    ) -> Result<Self, DsuWorkerError> {
        let config = config.validate()?;
        let config = if controls_tracker.is_some() {
            config.validate_controls_queue()?
        } else {
            config
        };
        stream_anchor.validate_for_profile(&ImuSampleV1::profile())?;
        let server = DsuLoopbackServer::bind(config.server)?;
        let local_address = server.local_address();
        let input_queue = BoundedEnvelopeQueue::new(
            ImuSampleV1::profile(),
            stream_anchor.stream_id,
            stream_anchor.stream_epoch,
            1,
        )?;
        let (motion_sender, motion_receiver) = mpsc::sync_channel(config.queue_capacity);
        let (controls_sender, controls_receiver) = if controls_tracker.is_some() {
            let (sender, receiver) = mpsc::sync_channel(config.controls_queue_capacity);
            (Some(sender), Some(receiver))
        } else {
            (None, None)
        };
        let stopped = Arc::new(AtomicBool::new(false));
        let controls_generation = Arc::new(AtomicU64::new(0));
        let counters = Arc::new(WorkerCounters::default());
        let thread_stopped = Arc::clone(&stopped);
        let thread_controls_generation = Arc::clone(&controls_generation);
        let thread_counters = Arc::clone(&counters);
        let thread = thread::Builder::new()
            .name("capyio-dsu-imu".to_owned())
            .spawn(move || {
                run_worker(
                    server,
                    input_queue,
                    WorkerReceivers {
                        motion: motion_receiver,
                        controls: controls_receiver,
                    },
                    WorkerRunConfig {
                        motion_mapping: config.motion_mapping,
                        controls_mapping: config.controls_mapping,
                        controls_tracker,
                        poll_interval: config.poll_interval,
                    },
                    WorkerShared {
                        stopped: thread_stopped,
                        controls_generation: thread_controls_generation,
                        counters: thread_counters,
                    },
                )
            })
            .map_err(DsuWorkerError::Spawn)?;
        let worker_sender = DsuImuWorkerSender {
            sender: motion_sender,
            stopped: Arc::clone(&stopped),
            counters: Arc::clone(&counters),
        };
        let controls_sender = controls_sender.map(|sender| DsuGamepadWorkerSender {
            sender,
            stopped: Arc::clone(&stopped),
            controls_generation,
            counters: Arc::clone(&counters),
        });
        Ok(Self {
            local_address,
            sender: worker_sender,
            controls_sender,
            stopped,
            counters,
            thread: Some(thread),
        })
    }

    #[must_use]
    pub const fn local_address(&self) -> SocketAddrV4 {
        self.local_address
    }

    #[must_use]
    pub fn sender(&self) -> DsuImuWorkerSender {
        self.sender.clone()
    }

    /// Returns the controls sender when the worker was started with a controls
    /// anchor. The IMU-only compatibility mode returns `None`.
    #[must_use]
    pub fn controls_sender(&self) -> Option<DsuGamepadWorkerSender> {
        self.controls_sender.clone()
    }

    #[must_use]
    /// Returns a non-coherent live snapshot of independent atomic counters.
    ///
    /// Counters are monotonic, while `active_subscribers` is a live gauge and
    /// `stopped` is a lifecycle flag. Related fields can briefly appear out of
    /// order while the worker is running. After [`Self::stop`] joins, the final
    /// snapshot is stable.
    pub fn stats(&self) -> DsuImuWorkerStats {
        self.counters.snapshot(self.stopped.load(Ordering::Acquire))
    }

    pub fn stop(&mut self) -> Result<(), DsuWorkerError> {
        self.stopped.store(true, Ordering::Release);
        let Some(thread) = self.thread.take() else {
            return Ok(());
        };
        thread
            .join()
            .map_err(|_| DsuWorkerError::WorkerPanicked)?
            .map_err(DsuWorkerError::Transport)
    }
}

impl Drop for DsuImuWorker {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn run_worker(
    mut server: DsuLoopbackServer,
    mut input_queue: BoundedEnvelopeQueue<ImuSampleV1>,
    receivers: WorkerReceivers,
    config: WorkerRunConfig,
    shared: WorkerShared,
) -> Result<(), DsuTransportError> {
    let started = Instant::now();
    let mut state = WorkerState {
        latest_motion: None,
        latest_controls: GamepadControls::neutral(),
        controls_tracker: config.controls_tracker,
        motion_mapping: config.motion_mapping,
        controls_mapping: config.controls_mapping,
        controls_generation: 0,
    };
    let mut result = loop {
        if shared.stopped.load(Ordering::Acquire) {
            break Ok(());
        }
        if let Err(error) = synchronize_controls_generation(
            &mut server,
            &mut state,
            &shared.controls_generation,
            elapsed_millis(started),
            &shared.counters,
        ) {
            increment(&shared.counters.transport_failures, 1);
            break Err(error);
        }
        let now_millis = elapsed_millis(started);
        match server.poll(now_millis) {
            Ok(stats) => record_poll(&shared.counters, stats, server.subscriber_count()),
            Err(error) => {
                increment(&shared.counters.transport_failures, 1);
                break Err(error);
            }
        }
        match process_input_cycle(
            &mut server,
            &mut input_queue,
            &receivers,
            &mut state,
            started,
            &shared,
        ) {
            Ok(0) => thread::sleep(config.poll_interval),
            Ok(_) => {}
            Err(error) => {
                increment(&shared.counters.transport_failures, 1);
                break Err(error);
            }
        }
    };
    let cleanup_reason = if result.is_ok() {
        NeutralReason::Stop
    } else {
        NeutralReason::Failure
    };
    if let Err(error) = reset_controls(
        &mut server,
        &mut state,
        elapsed_millis(started),
        &shared.counters,
        cleanup_reason,
    ) {
        increment(&shared.counters.transport_failures, 1);
        if result.is_ok() {
            result = Err(error);
        }
    }
    shared
        .counters
        .active_subscribers
        .store(0, Ordering::Release);
    shared.stopped.store(true, Ordering::Release);
    result
}

fn process_input_cycle(
    server: &mut DsuLoopbackServer,
    input_queue: &mut BoundedEnvelopeQueue<ImuSampleV1>,
    receivers: &WorkerReceivers,
    state: &mut WorkerState,
    started: Instant,
    shared: &WorkerShared,
) -> Result<usize, DsuTransportError> {
    let mut processed = 0;
    for _ in 0..MAX_DSU_INPUTS_PER_STREAM_PER_CYCLE {
        if shared.stopped.load(Ordering::Acquire) {
            break;
        }
        let mut progressed = false;
        synchronize_controls_generation(
            server,
            state,
            &shared.controls_generation,
            elapsed_millis(started),
            &shared.counters,
        )?;
        if let Some(receiver) = receivers.controls.as_ref() {
            match receiver.try_recv() {
                Ok(queued) => {
                    process_controls(
                        server,
                        queued.state,
                        queued.generation,
                        &shared.controls_generation,
                        state,
                        elapsed_millis(started),
                        &shared.counters,
                    )?;
                    processed += 1;
                    progressed = true;
                }
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => {}
            }
        }
        if shared.stopped.load(Ordering::Acquire) {
            break;
        }
        synchronize_controls_generation(
            server,
            state,
            &shared.controls_generation,
            elapsed_millis(started),
            &shared.counters,
        )?;
        match receivers.motion.try_recv() {
            Ok(envelope) => {
                process_motion(
                    server,
                    input_queue,
                    envelope,
                    state,
                    elapsed_millis(started),
                    &shared.counters,
                )?;
                processed += 1;
                progressed = true;
            }
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => {}
        }
        if !progressed {
            break;
        }
    }
    Ok(processed)
}

struct WorkerState {
    latest_motion: Option<DsuMotionSample>,
    latest_controls: GamepadControls,
    controls_tracker: Option<InputSequenceTracker>,
    motion_mapping: DsuMotionMapping,
    controls_mapping: DsuControlsMapping,
    controls_generation: u64,
}

#[derive(Clone, Copy)]
enum NeutralReason {
    Rejected,
    Gap,
    QueueOverflow,
    Stop,
    Failure,
}

fn process_motion(
    server: &mut DsuLoopbackServer,
    input_queue: &mut BoundedEnvelopeQueue<ImuSampleV1>,
    envelope: DataEnvelope<ImuSampleV1>,
    state: &mut WorkerState,
    now_millis: u64,
    counters: &WorkerCounters,
) -> Result<(), DsuTransportError> {
    let outcome = match input_queue.push(envelope) {
        Ok(outcome) => outcome,
        Err(_) => {
            increment(&counters.invalid_envelopes, 1);
            return Ok(());
        }
    };
    match outcome {
        PushOutcome::Enqueued { gap } => {
            increment(&counters.samples_accepted, 1);
            if let Some(gap) = gap {
                increment(&counters.input_gaps, 1);
                increment(
                    &counters.missing_sequences,
                    gap.last_missing
                        .saturating_sub(gap.first_missing)
                        .saturating_add(1),
                );
            }
            let delivery = input_queue
                .pop()
                .expect("an enqueued DSU IMU envelope is immediately available");
            match project_imu_envelope(&delivery.envelope, state.motion_mapping) {
                Ok(motion) => {
                    state.latest_motion = Some(motion);
                    let stats = server.publish_state(
                        now_millis,
                        motion,
                        state.latest_controls,
                        state.controls_mapping,
                    )?;
                    record_publish(counters, stats, server.subscriber_count());
                }
                Err(
                    MotionProjectionError::InvalidEnvelope(_)
                    | MotionProjectionError::DsuFloatOutOfRange
                    | MotionProjectionError::DuplicateSourceAxis(_),
                ) => {
                    increment(&counters.projection_errors, 1);
                }
            }
        }
        // Every accepted envelope is popped before the next receive, so a
        // repeated delivered sequence is normally classified as `Late`.
        // Keep these arms defensive if the internal buffering later changes.
        PushOutcome::Duplicate { .. } => increment(&counters.late_samples, 1),
        PushOutcome::Late { .. } => increment(&counters.late_samples, 1),
        PushOutcome::WrongStream => increment(&counters.wrong_stream_samples, 1),
        PushOutcome::StaleEpoch { .. } => increment(&counters.stale_epoch_samples, 1),
        PushOutcome::FutureEpoch { .. } => increment(&counters.future_epoch_samples, 1),
        PushOutcome::Full { .. } => increment(&counters.internal_motion_queue_full, 1),
    }
    Ok(())
}

fn process_controls(
    server: &mut DsuLoopbackServer,
    controls: GamepadState,
    generation: u64,
    controls_generation: &AtomicU64,
    state: &mut WorkerState,
    now_millis: u64,
    counters: &WorkerCounters,
) -> Result<(), DsuTransportError> {
    let current_generation = controls_generation.load(Ordering::Acquire);
    if generation != current_generation {
        increment(&counters.controls_stale_generation_drops, 1);
        return synchronize_controls_generation(
            server,
            state,
            controls_generation,
            now_millis,
            counters,
        );
    }
    let Some(tracker) = state.controls_tracker.as_mut() else {
        increment(&counters.invalid_controls, 1);
        return Ok(());
    };
    let mut candidate_tracker = *tracker;
    let outcome = match candidate_tracker.observe(controls.header) {
        Ok(outcome) => outcome,
        Err(error) => {
            match error {
                InputContractError::WrongStream { .. } => {
                    increment(&counters.wrong_stream_controls, 1);
                    return Ok(());
                }
                InputContractError::StaleEpoch { .. } => {
                    increment(&counters.stale_epoch_controls, 1);
                    return Ok(());
                }
                InputContractError::FutureEpoch { .. } => {
                    increment(&counters.future_epoch_controls, 1);
                }
                InputContractError::DuplicateOrLate { .. } => {
                    increment(&counters.late_controls, 1);
                    return Ok(());
                }
                InputContractError::SequenceExhausted => {
                    increment(&counters.exhausted_controls, 1);
                }
                _ => increment(&counters.invalid_controls, 1),
            }
            return reset_controls(server, state, now_millis, counters, NeutralReason::Rejected);
        }
    };
    if controls.validate().is_err() {
        increment(&counters.invalid_controls, 1);
        return reset_controls(server, state, now_millis, counters, NeutralReason::Rejected);
    }
    if validate_dsu_controls(controls.controls).is_err() {
        increment(&counters.unsupported_controls, 1);
        return reset_controls(server, state, now_millis, counters, NeutralReason::Rejected);
    }
    *tracker = candidate_tracker;
    if let InputSequenceOutcome::Gap(gap) = outcome {
        increment(&counters.controls_gaps, 1);
        increment(
            &counters.controls_missing_sequences,
            gap.last_missing
                .saturating_sub(gap.first_missing)
                .saturating_add(1),
        );
        reset_controls(server, state, now_millis, counters, NeutralReason::Gap)?;
    }
    increment(&counters.controls_accepted, 1);
    state.latest_controls = controls.controls;
    let Some(motion) = state.latest_motion else {
        increment(&counters.controls_cached_without_motion, 1);
        return Ok(());
    };
    let stats = server.publish_state(
        now_millis,
        motion,
        state.latest_controls,
        state.controls_mapping,
    )?;
    record_publish(counters, stats, server.subscriber_count());
    Ok(())
}

fn synchronize_controls_generation(
    server: &mut DsuLoopbackServer,
    state: &mut WorkerState,
    controls_generation: &AtomicU64,
    now_millis: u64,
    counters: &WorkerCounters,
) -> Result<(), DsuTransportError> {
    let requested_generation = controls_generation.load(Ordering::Acquire);
    if requested_generation == state.controls_generation {
        return Ok(());
    }
    state.controls_generation = requested_generation;
    reset_controls(
        server,
        state,
        now_millis,
        counters,
        NeutralReason::QueueOverflow,
    )
}

fn reset_controls(
    server: &mut DsuLoopbackServer,
    state: &mut WorkerState,
    now_millis: u64,
    counters: &WorkerCounters,
    reason: NeutralReason,
) -> Result<(), DsuTransportError> {
    if state.latest_controls == GamepadControls::neutral() {
        return Ok(());
    }
    state.latest_controls = GamepadControls::neutral();
    increment(&counters.controls_neutral_resets, 1);
    let Some(motion) = state.latest_motion else {
        return Ok(());
    };
    let stats = server.publish_state(
        now_millis,
        motion,
        state.latest_controls,
        state.controls_mapping,
    )?;
    increment(
        &counters.controls_neutral_packets_sent,
        usize_to_u64(stats.packets_sent),
    );
    if matches!(reason, NeutralReason::Stop) {
        increment(
            &counters.controls_stop_neutral_packets,
            usize_to_u64(stats.packets_sent),
        );
    }
    if matches!(reason, NeutralReason::Failure) {
        increment(
            &counters.controls_failure_neutral_packets,
            usize_to_u64(stats.packets_sent),
        );
    }
    record_publish(counters, stats, server.subscriber_count());
    Ok(())
}

fn record_poll(counters: &WorkerCounters, stats: DsuPollStats, active_subscribers: usize) {
    increment(
        &counters.dsu_datagrams_received,
        usize_to_u64(stats.datagrams_received),
    );
    increment(
        &counters.malformed_dsu_datagrams,
        usize_to_u64(stats.malformed_datagrams),
    );
    increment(
        &counters.dsu_responses_sent,
        usize_to_u64(stats.responses_sent),
    );
    increment(
        &counters.subscriptions_added,
        usize_to_u64(stats.subscriptions_added),
    );
    increment(
        &counters.subscriptions_renewed,
        usize_to_u64(stats.subscriptions_renewed),
    );
    increment(
        &counters.subscriptions_replaced,
        usize_to_u64(stats.subscriptions_replaced),
    );
    increment(
        &counters.subscriptions_rejected_full,
        usize_to_u64(stats.subscriptions_rejected_full),
    );
    increment(
        &counters.subscriptions_expired,
        usize_to_u64(stats.subscriptions_expired),
    );
    counters
        .active_subscribers
        .store(usize_to_u64(active_subscribers), Ordering::Release);
}

fn record_publish(counters: &WorkerCounters, stats: DsuPublishStats, active_subscribers: usize) {
    increment(
        &counters.subscriptions_expired,
        usize_to_u64(stats.subscriptions_expired),
    );
    increment(
        &counters.motion_packets_sent,
        usize_to_u64(stats.packets_sent),
    );
    increment(
        &counters.motion_packets_would_block,
        usize_to_u64(stats.packets_would_block),
    );
    increment(
        &counters.motion_packet_send_errors,
        usize_to_u64(stats.packet_send_errors),
    );
    increment(
        &counters.dsu_pad_packets_sent,
        usize_to_u64(stats.packets_sent),
    );
    increment(
        &counters.dsu_pad_packets_would_block,
        usize_to_u64(stats.packets_would_block),
    );
    increment(
        &counters.dsu_pad_packet_send_errors,
        usize_to_u64(stats.packet_send_errors),
    );
    counters
        .active_subscribers
        .store(usize_to_u64(active_subscribers), Ordering::Release);
}

fn elapsed_millis(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn usize_to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn advance_controls_generation(generation: &AtomicU64) -> bool {
    generation
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, checked_add_one)
        .is_ok()
}

fn checked_add_one(value: u64) -> Option<u64> {
    value.checked_add(1)
}

fn increment(counter: &AtomicU64, value: u64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
        Some(current.saturating_add(value))
    });
}

fn load(counter: &AtomicU64) -> u64 {
    counter.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use capyio_data_plane::parse_imu_fixture_jsonl;
    use capyio_input::{GamepadButton, GamepadButtons, InputFrameHeader};

    const FIXTURE: &str = include_str!("../../../fixtures/imu/imu_samples_v1.jsonl");

    fn gamepad_state(sequence: u64, controls: GamepadControls) -> GamepadState {
        GamepadState {
            header: InputFrameHeader {
                stream_id: "00000000-0000-4000-8000-00000000c002".parse().unwrap(),
                stream_epoch: 1,
                sequence,
                source_timestamp_nanos: sequence,
            },
            controls,
        }
    }

    #[test]
    fn sender_reports_bounded_channel_pressure_without_blocking() {
        let (sender, _receiver) = mpsc::sync_channel(1);
        let stopped = Arc::new(AtomicBool::new(false));
        let counters = Arc::new(WorkerCounters::default());
        let sender = DsuImuWorkerSender {
            sender,
            stopped,
            counters: Arc::clone(&counters),
        };
        let envelope = parse_imu_fixture_jsonl(FIXTURE, 6).unwrap().remove(0);

        assert_eq!(
            sender.try_submit(envelope.clone()),
            DsuSubmitOutcome::Submitted
        );
        assert_eq!(sender.try_submit(envelope), DsuSubmitOutcome::QueueFull);
        let stats = counters.snapshot(false);
        assert_eq!(stats.samples_submitted, 1);
        assert_eq!(stats.queue_full, 1);
    }

    #[test]
    fn controls_overflow_generation_prevents_queued_state_from_reactivating() {
        let (sender, receiver) = mpsc::sync_channel(1);
        let stopped = Arc::new(AtomicBool::new(false));
        let controls_generation = Arc::new(AtomicU64::new(0));
        let counters = Arc::new(WorkerCounters::default());
        let sender = DsuGamepadWorkerSender {
            sender,
            stopped,
            controls_generation: Arc::clone(&controls_generation),
            counters: Arc::clone(&counters),
        };
        let stream_id = "00000000-0000-4000-8000-00000000c002".parse().unwrap();
        let controls = GamepadControls {
            buttons: GamepadButtons::empty().with(GamepadButton::South),
            ..GamepadControls::neutral()
        };
        let state = GamepadState {
            header: InputFrameHeader {
                stream_id,
                stream_epoch: 1,
                sequence: 0,
                source_timestamp_nanos: 1,
            },
            controls,
        };

        assert_eq!(sender.try_submit(state), DsuSubmitOutcome::Submitted);
        assert_eq!(
            sender.try_submit(GamepadState {
                header: InputFrameHeader {
                    sequence: 1,
                    ..state.header
                },
                controls: GamepadControls::neutral(),
            }),
            DsuSubmitOutcome::QueueFull
        );
        assert_eq!(controls_generation.load(Ordering::Acquire), 1);

        let QueuedControls {
            state: queued,
            generation,
        } = receiver.try_recv().unwrap();
        assert_eq!(generation, 0);
        let mut server = DsuLoopbackServer::bind(DsuLoopbackConfig::local_lab(0, 7)).unwrap();
        let mut worker_state = WorkerState {
            latest_motion: None,
            latest_controls: controls,
            controls_tracker: Some(InputSequenceTracker::new(stream_id, 1, 0).unwrap()),
            motion_mapping: DsuMotionMapping::identity(),
            controls_mapping: DsuControlsMapping::identity(),
            controls_generation: 0,
        };
        process_controls(
            &mut server,
            queued,
            generation,
            &controls_generation,
            &mut worker_state,
            0,
            &counters,
        )
        .unwrap();

        assert_eq!(worker_state.latest_controls, GamepadControls::neutral());
        let stats = counters.snapshot(false);
        assert_eq!(stats.controls_stale_generation_drops, 1);
        assert_eq!(stats.controls_neutral_resets, 1);
        assert_eq!(stats.controls_accepted, 0);

        let recovered_controls = GamepadControls {
            buttons: GamepadButtons::empty().with(GamepadButton::East),
            ..GamepadControls::neutral()
        };
        process_controls(
            &mut server,
            gamepad_state(2, recovered_controls),
            1,
            &controls_generation,
            &mut worker_state,
            1,
            &counters,
        )
        .unwrap();
        assert_eq!(worker_state.latest_controls, recovered_controls);
        let recovered_stats = counters.snapshot(false);
        assert_eq!(recovered_stats.controls_accepted, 1);
        assert_eq!(recovered_stats.controls_gaps, 1);
        assert_eq!(recovered_stats.controls_missing_sequences, 2);
    }

    #[test]
    fn motion_and_controls_channel_pressure_is_isolated() {
        let stopped = Arc::new(AtomicBool::new(false));
        let generation = Arc::new(AtomicU64::new(0));
        let counters = Arc::new(WorkerCounters::default());
        let envelope = parse_imu_fixture_jsonl(FIXTURE, 6).unwrap().remove(0);
        let controls = gamepad_state(0, GamepadControls::neutral());

        let (motion_channel, _motion_receiver) = mpsc::sync_channel(1);
        let (controls_channel, _controls_receiver) = mpsc::sync_channel(1);
        let motion_sender = DsuImuWorkerSender {
            sender: motion_channel,
            stopped: Arc::clone(&stopped),
            counters: Arc::clone(&counters),
        };
        let controls_sender = DsuGamepadWorkerSender {
            sender: controls_channel,
            stopped: Arc::clone(&stopped),
            controls_generation: Arc::clone(&generation),
            counters: Arc::clone(&counters),
        };
        assert_eq!(
            motion_sender.try_submit(envelope.clone()),
            DsuSubmitOutcome::Submitted
        );
        assert_eq!(
            motion_sender.try_submit(envelope.clone()),
            DsuSubmitOutcome::QueueFull
        );
        assert_eq!(
            controls_sender.try_submit(controls),
            DsuSubmitOutcome::Submitted
        );

        let (motion_channel, _motion_receiver) = mpsc::sync_channel(1);
        let (controls_channel, _controls_receiver) = mpsc::sync_channel(1);
        let motion_sender = DsuImuWorkerSender {
            sender: motion_channel,
            stopped: Arc::clone(&stopped),
            counters: Arc::clone(&counters),
        };
        let controls_sender = DsuGamepadWorkerSender {
            sender: controls_channel,
            stopped,
            controls_generation: generation,
            counters: Arc::clone(&counters),
        };
        assert_eq!(
            controls_sender.try_submit(controls),
            DsuSubmitOutcome::Submitted
        );
        assert_eq!(
            controls_sender.try_submit(gamepad_state(1, GamepadControls::neutral())),
            DsuSubmitOutcome::QueueFull
        );
        assert_eq!(
            motion_sender.try_submit(envelope),
            DsuSubmitOutcome::Submitted
        );

        let stats = counters.snapshot(false);
        assert_eq!(stats.samples_submitted, 2);
        assert_eq!(stats.queue_full, 1);
        assert_eq!(stats.controls_submitted, 2);
        assert_eq!(stats.controls_queue_full, 1);
    }

    #[test]
    fn controls_generation_exhaustion_stops_without_wrapping() {
        let (sender, _receiver) = mpsc::sync_channel(1);
        let stopped = Arc::new(AtomicBool::new(false));
        let controls_generation = Arc::new(AtomicU64::new(u64::MAX));
        let counters = Arc::new(WorkerCounters::default());
        let sender = DsuGamepadWorkerSender {
            sender,
            stopped: Arc::clone(&stopped),
            controls_generation: Arc::clone(&controls_generation),
            counters: Arc::clone(&counters),
        };

        assert_eq!(sender.request_neutral(), DsuNeutralOutcome::Stopped);
        assert!(stopped.load(Ordering::Acquire));
        assert_eq!(controls_generation.load(Ordering::Acquire), u64::MAX);
        assert_eq!(counters.snapshot(true).controls_generation_exhausted, 1);
    }

    #[test]
    fn input_cycle_has_equal_fixed_per_stream_budgets() {
        let fixture = parse_imu_fixture_jsonl(FIXTURE, 6).unwrap().remove(0);
        let stream_id = fixture.stream_id;
        let stream_epoch = fixture.stream_epoch;
        let (motion_sender, motion_receiver) = mpsc::sync_channel(32);
        let (controls_sender, controls_receiver) = mpsc::sync_channel(32);
        for sequence in 0..=MAX_DSU_INPUTS_PER_STREAM_PER_CYCLE {
            let mut envelope = fixture.clone();
            envelope.sequence = u64::try_from(sequence).unwrap();
            motion_sender.try_send(envelope).unwrap();
            controls_sender
                .try_send(QueuedControls {
                    state: gamepad_state(
                        u64::try_from(sequence).unwrap(),
                        GamepadControls::neutral(),
                    ),
                    generation: 0,
                })
                .unwrap();
        }
        let receivers = WorkerReceivers {
            motion: motion_receiver,
            controls: Some(controls_receiver),
        };
        let mut input_queue =
            BoundedEnvelopeQueue::new(ImuSampleV1::profile(), stream_id, stream_epoch, 1).unwrap();
        let controls_stream = gamepad_state(0, GamepadControls::neutral()).header;
        let mut state = WorkerState {
            latest_motion: None,
            latest_controls: GamepadControls::neutral(),
            controls_tracker: Some(
                InputSequenceTracker::new(
                    controls_stream.stream_id,
                    controls_stream.stream_epoch,
                    0,
                )
                .unwrap(),
            ),
            motion_mapping: DsuMotionMapping::identity(),
            controls_mapping: DsuControlsMapping::identity(),
            controls_generation: 0,
        };
        let shared = WorkerShared {
            stopped: Arc::new(AtomicBool::new(false)),
            controls_generation: Arc::new(AtomicU64::new(0)),
            counters: Arc::new(WorkerCounters::default()),
        };
        let mut server = DsuLoopbackServer::bind(DsuLoopbackConfig::local_lab(0, 7)).unwrap();
        let started = Instant::now();

        assert_eq!(
            process_input_cycle(
                &mut server,
                &mut input_queue,
                &receivers,
                &mut state,
                started,
                &shared,
            )
            .unwrap(),
            MAX_DSU_INPUTS_PER_STREAM_PER_CYCLE * 2
        );
        let first = shared.counters.snapshot(false);
        assert_eq!(
            first.samples_accepted,
            u64::try_from(MAX_DSU_INPUTS_PER_STREAM_PER_CYCLE).unwrap()
        );
        assert_eq!(first.controls_accepted, first.samples_accepted);

        assert_eq!(
            process_input_cycle(
                &mut server,
                &mut input_queue,
                &receivers,
                &mut state,
                started,
                &shared,
            )
            .unwrap(),
            2
        );
        let final_stats = shared.counters.snapshot(false);
        assert_eq!(
            final_stats.samples_accepted,
            u64::try_from(MAX_DSU_INPUTS_PER_STREAM_PER_CYCLE + 1).unwrap()
        );
        assert_eq!(final_stats.controls_accepted, final_stats.samples_accepted);

        let mut stopped_motion = fixture;
        stopped_motion.sequence = 17;
        motion_sender.try_send(stopped_motion).unwrap();
        controls_sender
            .try_send(QueuedControls {
                state: gamepad_state(17, GamepadControls::neutral()),
                generation: 0,
            })
            .unwrap();
        shared.stopped.store(true, Ordering::Release);
        assert_eq!(
            process_input_cycle(
                &mut server,
                &mut input_queue,
                &receivers,
                &mut state,
                started,
                &shared,
            )
            .unwrap(),
            0
        );
        let stopped_stats = shared.counters.snapshot(true);
        assert_eq!(stopped_stats.samples_accepted, 17);
        assert_eq!(stopped_stats.controls_accepted, 17);
    }
}
