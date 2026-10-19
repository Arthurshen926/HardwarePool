use std::{error::Error, fmt};

use capyio_core::{PortRef, Route, RouteId};
use capyio_input::{InputStreamDescriptor, TouchpadDescriptor};
#[cfg(windows)]
use capyio_input::{TouchpadButtonState, TouchpadButtonType, TouchpadFrame, TouchpadFrameKind};

use crate::{
    PrivateTouchpadEnqueueOutcome, PrivateTouchpadEpochAdvanceOutcome,
    PrivateTouchpadIngressLimits, PrivateTouchpadPumpOutcome, PrivateTouchpadReceiverError,
    PrivateTouchpadRouteBinding, PrivateTouchpadRouteSession,
    PrivateTouchpadRouteSessionBuildError, PrivateTouchpadRouteSessionError,
    PrivateTouchpadRouteSessionState, PrivateTouchpadSink,
    ingress::validate_private_touchpad_route_session_construction,
};

/// One coherent sample of the clocks used by Route authorization and ingress.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PrivateTouchpadClockSample {
    pub now_ms: u64,
    pub now_nanos: u64,
}

/// Read-only Route boundary supplied by the Node composition layer.
///
/// Returning an owned snapshot prevents one worker action from observing a
/// Route transition halfway through validation.
pub trait PrivateTouchpadRouteProvider {
    type Error;

    fn current_route(&mut self, route_id: RouteId) -> Result<Route, Self::Error>;
}

/// Monotonic clock boundary supplied by the Node composition layer.
pub trait PrivateTouchpadMonotonicClock {
    type Error;

    fn sample(&mut self) -> Result<PrivateTouchpadClockSample, Self::Error>;
}

/// Opens one platform Sink only after Route and semantic preflight succeeds.
pub trait PrivateTouchpadSinkFactory {
    type Sink: PrivateTouchpadSink;
    type Error;

    fn open(
        &mut self,
        stream: &InputStreamDescriptor,
        descriptor: TouchpadDescriptor,
        first_sequence: u64,
    ) -> Result<Self::Sink, Self::Error>;
}

/// Production Windows factory. Calling `open` creates a synthetic touchpad;
/// constructing this zero-sized factory has no platform side effect.
#[cfg(windows)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WindowsSyntheticTouchpadSinkFactory;

#[cfg(windows)]
impl PrivateTouchpadSinkFactory for WindowsSyntheticTouchpadSinkFactory {
    type Sink = capyio_windows_input::SyntheticTouchpadSession;
    type Error = capyio_windows_input::SyntheticTouchpadSessionError;

    fn open(
        &mut self,
        stream: &InputStreamDescriptor,
        descriptor: TouchpadDescriptor,
        first_sequence: u64,
    ) -> Result<Self::Sink, Self::Error> {
        capyio_windows_input::SyntheticTouchpadSession::open(stream, descriptor, first_sequence)
    }
}

/// Production VHF fallback factory. The protected driver interface is opened
/// only after `new_with_sink_factory` completes Route and contract preflight.
#[cfg(windows)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WindowsVhfTouchpadSinkFactory;

#[cfg(windows)]
impl PrivateTouchpadSinkFactory for WindowsVhfTouchpadSinkFactory {
    type Sink = capyio_windows_input::VhfTouchpadSession<capyio_windows_input::VhfWin32Transport>;
    type Error = capyio_windows_input::VhfTouchpadSessionError;

    fn open(
        &mut self,
        stream: &InputStreamDescriptor,
        descriptor: TouchpadDescriptor,
        _first_sequence: u64,
    ) -> Result<Self::Sink, Self::Error> {
        capyio_windows_input::VhfTouchpadSession::open_win32(descriptor, stream.stream_epoch)
    }
}

#[cfg(windows)]
const TAP_DRAG_FIRST_TAP_MIN_DURATION_NANOS: u64 = 20_000_000;
#[cfg(windows)]
const TAP_DRAG_FIRST_TAP_MAX_DURATION_NANOS: u64 = 300_000_000;
#[cfg(windows)]
const TAP_DRAG_FIRST_TAP_MAX_MOTION_HIMETRIC: i64 = 150;
#[cfg(windows)]
const TAP_DRAG_MAX_INTER_CONTACT_GAP_NANOS: u64 = 500_000_000;
#[cfg(windows)]
const TAP_DRAG_MAX_START_POSITION_DELTA_HIMETRIC: i64 = 500;

#[cfg(windows)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TapCandidate {
    released_at_nanos: u64,
    x_himetric: u32,
    y_himetric: u32,
}

#[cfg(windows)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ActiveTap {
    started_at_nanos: u64,
    x_himetric: u32,
    y_himetric: u32,
    max_delta_x_himetric: i64,
    max_delta_y_himetric: i64,
}

#[cfg(windows)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct TapDragButtonLatch {
    active_tap: Option<ActiveTap>,
    candidate: Option<TapCandidate>,
    button_held: bool,
    previous_contact_count: usize,
}

#[cfg(windows)]
impl TapDragButtonLatch {
    fn project(&mut self, frame: &TouchpadFrame) -> TouchpadFrame {
        let mut output = frame.clone();
        output.button = TouchpadButtonState::Released;
        if frame.kind == TouchpadFrameKind::CancelAll {
            *self = Self::default();
            return output;
        }
        let count = frame.contacts.len();
        match frame.contacts.as_slice() {
            [_] if self.button_held => output.button = TouchpadButtonState::Pressed,
            [contact] if self.previous_contact_count == 0 => {
                let qualifies = self.candidate.is_some_and(|candidate| {
                    frame
                        .header
                        .source_timestamp_nanos
                        .checked_sub(candidate.released_at_nanos)
                        .is_some_and(|gap| gap <= TAP_DRAG_MAX_INTER_CONTACT_GAP_NANOS)
                        && (i64::from(contact.position.x_himetric)
                            - i64::from(candidate.x_himetric))
                        .abs()
                            <= TAP_DRAG_MAX_START_POSITION_DELTA_HIMETRIC
                        && (i64::from(contact.position.y_himetric)
                            - i64::from(candidate.y_himetric))
                        .abs()
                            <= TAP_DRAG_MAX_START_POSITION_DELTA_HIMETRIC
                });
                self.candidate = None;
                if qualifies {
                    self.button_held = true;
                    output.button = TouchpadButtonState::Pressed;
                    self.active_tap = None;
                } else {
                    self.active_tap = Some(ActiveTap {
                        started_at_nanos: frame.header.source_timestamp_nanos,
                        x_himetric: contact.position.x_himetric,
                        y_himetric: contact.position.y_himetric,
                        max_delta_x_himetric: 0,
                        max_delta_y_himetric: 0,
                    });
                }
            }
            [contact] => {
                if let Some(active) = self.active_tap.as_mut() {
                    active.max_delta_x_himetric = active.max_delta_x_himetric.max(
                        (i64::from(contact.position.x_himetric) - i64::from(active.x_himetric))
                            .abs(),
                    );
                    active.max_delta_y_himetric = active.max_delta_y_himetric.max(
                        (i64::from(contact.position.y_himetric) - i64::from(active.y_himetric))
                            .abs(),
                    );
                }
            }
            [] => {
                if self.button_held {
                    self.button_held = false;
                    self.active_tap = None;
                    self.candidate = None;
                } else if self.previous_contact_count == 1
                    && let Some(active) = self.active_tap.take()
                {
                    let duration = frame
                        .header
                        .source_timestamp_nanos
                        .saturating_sub(active.started_at_nanos);
                    if (TAP_DRAG_FIRST_TAP_MIN_DURATION_NANOS
                        ..=TAP_DRAG_FIRST_TAP_MAX_DURATION_NANOS)
                        .contains(&duration)
                        && active.max_delta_x_himetric <= TAP_DRAG_FIRST_TAP_MAX_MOTION_HIMETRIC
                        && active.max_delta_y_himetric <= TAP_DRAG_FIRST_TAP_MAX_MOTION_HIMETRIC
                    {
                        self.candidate = Some(TapCandidate {
                            released_at_nanos: frame.header.source_timestamp_nanos,
                            x_himetric: active.x_himetric,
                            y_himetric: active.y_himetric,
                        });
                    } else {
                        self.candidate = None;
                    }
                }
            }
            _ => {
                self.active_tap = None;
                self.candidate = None;
                self.button_held = false;
            }
        }
        self.previous_contact_count = count;
        output
    }
}

#[cfg(all(test, windows))]
mod windows_precision_tests {
    use super::TapDragButtonLatch;
    use capyio_input::{
        InputFrameHeader, TouchpadButtonState, TouchpadContact, TouchpadFrame, TouchpadFrameKind,
        TouchpadPosition,
    };

    fn frame(sequence: u64, timestamp: u64, contacts: usize) -> TouchpadFrame {
        TouchpadFrame {
            header: InputFrameHeader {
                stream_id: "00000000-0000-4000-8000-00000000e021"
                    .parse()
                    .expect("stream id"),
                stream_epoch: 1,
                sequence,
                source_timestamp_nanos: timestamp,
            },
            kind: TouchpadFrameKind::Update,
            button: TouchpadButtonState::Released,
            contacts: (0..contacts)
                .map(|contact_id| TouchpadContact {
                    contact_id: contact_id as u32,
                    position: TouchpadPosition {
                        x_himetric: 2_000 + contact_id as u32 * 500,
                        y_himetric: 3_000,
                    },
                    confidence: true,
                    size: None,
                    pressure: None,
                })
                .collect(),
        }
    }

    #[test]
    fn clickpad_latch_holds_second_tap_until_release() {
        let mut latch = TapDragButtonLatch::default();
        assert_eq!(
            latch.project(&frame(0, 10_000_000, 1)).button,
            TouchpadButtonState::Released
        );
        assert_eq!(
            latch.project(&frame(1, 110_000_000, 0)).button,
            TouchpadButtonState::Released
        );
        assert_eq!(
            latch.project(&frame(2, 210_000_000, 1)).button,
            TouchpadButtonState::Pressed
        );
        assert_eq!(
            latch.project(&frame(3, 230_000_000, 1)).button,
            TouchpadButtonState::Pressed
        );
        assert_eq!(
            latch.project(&frame(4, 250_000_000, 0)).button,
            TouchpadButtonState::Released
        );
    }

    #[test]
    fn multi_contact_frame_cancels_clickpad_latch() {
        let mut latch = TapDragButtonLatch::default();
        let _ = latch.project(&frame(0, 10_000_000, 1));
        let _ = latch.project(&frame(1, 110_000_000, 0));
        assert_eq!(
            latch.project(&frame(2, 210_000_000, 2)).button,
            TouchpadButtonState::Released
        );
        assert_eq!(
            latch.project(&frame(3, 230_000_000, 1)).button,
            TouchpadButtonState::Released
        );
    }
}

/// VHF Precision Touchpad sink used by the unified product path. It exposes a
/// ClickPad descriptor and preserves the standalone lab's double-tap-drag
/// compatibility latch while leaving multi-contact gestures to Windows.
#[cfg(windows)]
pub struct WindowsVhfPrecisionTouchpadSink {
    session: capyio_windows_input::VhfTouchpadSession<capyio_windows_input::VhfWin32Transport>,
    tap_drag: TapDragButtonLatch,
}

#[cfg(windows)]
impl PrivateTouchpadSink for WindowsVhfPrecisionTouchpadSink {
    type Error = capyio_windows_input::VhfTouchpadSessionError;

    fn submit_frame(&mut self, frame: &TouchpadFrame) -> Result<(), Self::Error> {
        self.session.submit_frame(&self.tap_drag.project(frame))
    }

    fn advance_epoch(&mut self, new_epoch: u64, _first_sequence: u64) -> Result<(), Self::Error> {
        self.tap_drag = TapDragButtonLatch::default();
        self.session.advance_epoch(new_epoch)
    }

    fn close(&mut self) -> Result<(), Self::Error> {
        self.tap_drag = TapDragButtonLatch::default();
        self.session.close()
    }
}

#[cfg(windows)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WindowsVhfPrecisionTouchpadSinkFactory;

#[cfg(windows)]
impl PrivateTouchpadSinkFactory for WindowsVhfPrecisionTouchpadSinkFactory {
    type Sink = WindowsVhfPrecisionTouchpadSink;
    type Error = capyio_windows_input::VhfTouchpadSessionError;

    fn open(
        &mut self,
        stream: &InputStreamDescriptor,
        mut descriptor: TouchpadDescriptor,
        _first_sequence: u64,
    ) -> Result<Self::Sink, Self::Error> {
        descriptor.button_type = TouchpadButtonType::ClickPad;
        Ok(WindowsVhfPrecisionTouchpadSink {
            session: capyio_windows_input::VhfTouchpadSession::open_win32(
                descriptor,
                stream.stream_epoch,
            )?,
            tap_drag: TapDragButtonLatch::default(),
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PrivateTouchpadRuntimeWorkerMetrics {
    pub clock_samples: u64,
    pub route_snapshots: u64,
    pub activations: u64,
    pub packets_enqueued: u64,
    pub pump_ticks: u64,
    pub packets_processed: u64,
    pub epoch_advances: u64,
    pub discarded_packets: u64,
    pub stops: u64,
}

#[derive(Debug)]
pub enum PrivateTouchpadRuntimeWorkerBuildError<ProviderError, ClockError> {
    Clock(ClockError),
    RouteProvider(ProviderError),
    Session(PrivateTouchpadRouteSessionBuildError),
}

#[derive(Debug)]
pub enum PrivateTouchpadRuntimeWorkerFactoryBuildError<ProviderError, ClockError, FactoryError> {
    Clock(ClockError),
    RouteProvider(ProviderError),
    Preflight(PrivateTouchpadRouteSessionBuildError),
    SinkFactory(FactoryError),
    Session(PrivateTouchpadRouteSessionBuildError),
}

impl<ProviderError: fmt::Display, ClockError: fmt::Display, FactoryError: fmt::Display> fmt::Display
    for PrivateTouchpadRuntimeWorkerFactoryBuildError<ProviderError, ClockError, FactoryError>
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Clock(error) => write!(formatter, "private touchpad clock failed: {error}"),
            Self::RouteProvider(error) => {
                write!(formatter, "private touchpad Route provider failed: {error}")
            }
            Self::Preflight(error) => {
                write!(formatter, "private touchpad Sink preflight failed: {error}")
            }
            Self::SinkFactory(error) => {
                write!(formatter, "private touchpad Sink factory failed: {error}")
            }
            Self::Session(error) => error.fmt(formatter),
        }
    }
}

impl<ProviderError: Error + 'static, ClockError: Error + 'static, FactoryError: Error + 'static>
    Error
    for PrivateTouchpadRuntimeWorkerFactoryBuildError<ProviderError, ClockError, FactoryError>
{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Clock(error) => Some(error),
            Self::RouteProvider(error) => Some(error),
            Self::Preflight(error) | Self::Session(error) => Some(error),
            Self::SinkFactory(error) => Some(error),
        }
    }
}

impl<ProviderError: fmt::Display, ClockError: fmt::Display> fmt::Display
    for PrivateTouchpadRuntimeWorkerBuildError<ProviderError, ClockError>
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Clock(error) => write!(formatter, "private touchpad clock failed: {error}"),
            Self::RouteProvider(error) => {
                write!(formatter, "private touchpad Route provider failed: {error}")
            }
            Self::Session(error) => error.fmt(formatter),
        }
    }
}

impl<ProviderError: Error + 'static, ClockError: Error + 'static> Error
    for PrivateTouchpadRuntimeWorkerBuildError<ProviderError, ClockError>
{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Clock(error) => Some(error),
            Self::RouteProvider(error) => Some(error),
            Self::Session(error) => Some(error),
        }
    }
}

#[derive(Debug)]
pub enum PrivateTouchpadRuntimeWorkerError<ProviderError, ClockError, SinkError> {
    Clock {
        error: ClockError,
        cleanup: Option<Box<PrivateTouchpadReceiverError<SinkError>>>,
    },
    RouteProvider {
        error: ProviderError,
        cleanup: Option<Box<PrivateTouchpadReceiverError<SinkError>>>,
    },
    ClockRegression {
        previous: PrivateTouchpadClockSample,
        actual: PrivateTouchpadClockSample,
        cleanup: Option<Box<PrivateTouchpadReceiverError<SinkError>>>,
    },
    Session(PrivateTouchpadRouteSessionError<SinkError>),
}

type WorkerResult<T, Provider, Clock, Sink> = Result<
    T,
    PrivateTouchpadRuntimeWorkerError<
        <Provider as PrivateTouchpadRouteProvider>::Error,
        <Clock as PrivateTouchpadMonotonicClock>::Error,
        <Sink as PrivateTouchpadSink>::Error,
    >,
>;

type WorkerFactoryBuildResult<T, Provider, Clock, Factory> = Result<
    T,
    PrivateTouchpadRuntimeWorkerFactoryBuildError<
        <Provider as PrivateTouchpadRouteProvider>::Error,
        <Clock as PrivateTouchpadMonotonicClock>::Error,
        <Factory as PrivateTouchpadSinkFactory>::Error,
    >,
>;

impl<ProviderError: fmt::Display, ClockError: fmt::Display, SinkError: fmt::Display> fmt::Display
    for PrivateTouchpadRuntimeWorkerError<ProviderError, ClockError, SinkError>
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Clock {
                error,
                cleanup: None,
            } => write!(formatter, "private touchpad clock failed: {error}"),
            Self::Clock {
                error,
                cleanup: Some(cleanup),
            } => write!(
                formatter,
                "private touchpad clock failed: {error}; receiver cleanup also failed: {cleanup}"
            ),
            Self::RouteProvider {
                error,
                cleanup: None,
            } => write!(formatter, "private touchpad Route provider failed: {error}"),
            Self::RouteProvider {
                error,
                cleanup: Some(cleanup),
            } => write!(
                formatter,
                "private touchpad Route provider failed: {error}; receiver cleanup also failed: {cleanup}"
            ),
            Self::ClockRegression {
                previous,
                actual,
                cleanup: None,
            } => write!(
                formatter,
                "private touchpad worker clock regressed from {previous:?} to {actual:?}"
            ),
            Self::ClockRegression {
                previous,
                actual,
                cleanup: Some(cleanup),
            } => write!(
                formatter,
                "private touchpad worker clock regressed from {previous:?} to {actual:?}; receiver cleanup also failed: {cleanup}"
            ),
            Self::Session(error) => error.fmt(formatter),
        }
    }
}

impl<ProviderError: Error + 'static, ClockError: Error + 'static, SinkError: Error + 'static> Error
    for PrivateTouchpadRuntimeWorkerError<ProviderError, ClockError, SinkError>
{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Clock { error, .. } => Some(error),
            Self::RouteProvider { error, .. } => Some(error),
            Self::ClockRegression { .. } => None,
            Self::Session(error) => Some(error),
        }
    }
}

/// Deterministic driver for one private touchpad Route session.
///
/// This object does not create a thread, open a socket or touch an OS device.
/// A Node composition loop calls these methods for start, packet, tick, epoch
/// and stop commands. Every live action obtains exactly one clock sample and
/// one immutable Route snapshot before delegating to the bounded session.
pub struct PrivateTouchpadRuntimeWorker<Provider, Clock, Sink>
where
    Provider: PrivateTouchpadRouteProvider,
    Clock: PrivateTouchpadMonotonicClock,
    Sink: PrivateTouchpadSink,
{
    route_id: RouteId,
    provider: Provider,
    clock: Clock,
    session: PrivateTouchpadRouteSession<Sink>,
    last_clock: PrivateTouchpadClockSample,
    metrics: PrivateTouchpadRuntimeWorkerMetrics,
}

impl<Provider, Clock, Sink> PrivateTouchpadRuntimeWorker<Provider, Clock, Sink>
where
    Provider: PrivateTouchpadRouteProvider,
    Clock: PrivateTouchpadMonotonicClock,
    Sink: PrivateTouchpadSink,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        route_id: RouteId,
        expected_sink: PortRef,
        stream: InputStreamDescriptor,
        descriptor: TouchpadDescriptor,
        first_sequence: u64,
        limits: PrivateTouchpadIngressLimits,
        mut provider: Provider,
        mut clock: Clock,
        sink: Sink,
    ) -> Result<Self, PrivateTouchpadRuntimeWorkerBuildError<Provider::Error, Clock::Error>> {
        let sample = clock
            .sample()
            .map_err(PrivateTouchpadRuntimeWorkerBuildError::Clock)?;
        let route = provider
            .current_route(route_id)
            .map_err(PrivateTouchpadRuntimeWorkerBuildError::RouteProvider)?;
        let session = PrivateTouchpadRouteSession::new(
            &route,
            expected_sink,
            stream,
            descriptor,
            first_sequence,
            sample.now_ms,
            limits,
            sink,
        )
        .map_err(PrivateTouchpadRuntimeWorkerBuildError::Session)?;
        Ok(Self {
            route_id,
            provider,
            clock,
            session,
            last_clock: sample,
            metrics: PrivateTouchpadRuntimeWorkerMetrics {
                clock_samples: 1,
                route_snapshots: 1,
                ..PrivateTouchpadRuntimeWorkerMetrics::default()
            },
        })
    }

    #[must_use]
    pub const fn state(&self) -> PrivateTouchpadRouteSessionState {
        self.session.state()
    }

    #[must_use]
    pub const fn binding(&self) -> &PrivateTouchpadRouteBinding {
        self.session.binding()
    }

    #[must_use]
    pub fn queued_packets(&self) -> u8 {
        self.session.queued_packets()
    }

    #[must_use]
    pub const fn metrics(&self) -> PrivateTouchpadRuntimeWorkerMetrics {
        self.metrics
    }

    pub fn activate(&mut self) -> WorkerResult<(), Provider, Clock, Sink> {
        let (route, sample) = self.context()?;
        self.session
            .activate(&route, sample.now_ms)
            .map_err(PrivateTouchpadRuntimeWorkerError::Session)?;
        self.metrics.activations = self.metrics.activations.saturating_add(1);
        Ok(())
    }

    pub fn enqueue(
        &mut self,
        packet: &[u8],
    ) -> WorkerResult<PrivateTouchpadEnqueueOutcome, Provider, Clock, Sink> {
        let (route, sample) = self.context()?;
        let outcome = self
            .session
            .enqueue(&route, sample.now_ms, packet, sample.now_nanos)
            .map_err(PrivateTouchpadRuntimeWorkerError::Session)?;
        self.metrics.packets_enqueued = self.metrics.packets_enqueued.saturating_add(1);
        Ok(outcome)
    }

    pub fn tick(&mut self) -> WorkerResult<PrivateTouchpadPumpOutcome, Provider, Clock, Sink> {
        let (route, sample) = self.context()?;
        let outcome = self
            .session
            .pump(&route, sample.now_ms, sample.now_nanos)
            .map_err(PrivateTouchpadRuntimeWorkerError::Session)?;
        self.metrics.pump_ticks = self.metrics.pump_ticks.saturating_add(1);
        self.metrics.packets_processed = self
            .metrics
            .packets_processed
            .saturating_add(u64::from(outcome.packets_processed));
        Ok(outcome)
    }

    pub fn advance_epoch(
        &mut self,
        first_sequence: u64,
    ) -> WorkerResult<PrivateTouchpadEpochAdvanceOutcome, Provider, Clock, Sink> {
        let (route, sample) = self.context()?;
        let outcome = self
            .session
            .advance_epoch(&route, first_sequence, sample.now_ms, sample.now_nanos)
            .map_err(PrivateTouchpadRuntimeWorkerError::Session)?;
        self.metrics.epoch_advances = self.metrics.epoch_advances.saturating_add(1);
        self.metrics.discarded_packets = self
            .metrics
            .discarded_packets
            .saturating_add(u64::from(outcome.discarded_packets));
        Ok(outcome)
    }

    pub fn stop(&mut self) -> WorkerResult<(), Provider, Clock, Sink> {
        self.session
            .disconnect()
            .map_err(PrivateTouchpadRuntimeWorkerError::Session)?;
        self.metrics.stops = self.metrics.stops.saturating_add(1);
        Ok(())
    }

    fn context(
        &mut self,
    ) -> WorkerResult<(Route, PrivateTouchpadClockSample), Provider, Clock, Sink> {
        let sample = match self.clock.sample() {
            Ok(sample) => sample,
            Err(error) => {
                let cleanup = self.session.fail_closed();
                return Err(PrivateTouchpadRuntimeWorkerError::Clock { error, cleanup });
            }
        };
        self.metrics.clock_samples = self.metrics.clock_samples.saturating_add(1);
        if sample.now_ms < self.last_clock.now_ms || sample.now_nanos < self.last_clock.now_nanos {
            let previous = self.last_clock;
            let cleanup = self.session.fail_closed();
            return Err(PrivateTouchpadRuntimeWorkerError::ClockRegression {
                previous,
                actual: sample,
                cleanup,
            });
        }
        self.last_clock = sample;
        let route = match self.provider.current_route(self.route_id) {
            Ok(route) => route,
            Err(error) => {
                let cleanup = self.session.fail_closed();
                return Err(PrivateTouchpadRuntimeWorkerError::RouteProvider { error, cleanup });
            }
        };
        self.metrics.route_snapshots = self.metrics.route_snapshots.saturating_add(1);
        Ok((route, sample))
    }
}

impl<Provider, Clock, Sink> PrivateTouchpadRuntimeWorker<Provider, Clock, Sink>
where
    Provider: PrivateTouchpadRouteProvider,
    Clock: PrivateTouchpadMonotonicClock,
    Sink: PrivateTouchpadSink,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_sink_factory<Factory>(
        route_id: RouteId,
        expected_sink: PortRef,
        stream: InputStreamDescriptor,
        descriptor: TouchpadDescriptor,
        first_sequence: u64,
        limits: PrivateTouchpadIngressLimits,
        mut provider: Provider,
        mut clock: Clock,
        mut factory: Factory,
    ) -> WorkerFactoryBuildResult<Self, Provider, Clock, Factory>
    where
        Factory: PrivateTouchpadSinkFactory<Sink = Sink>,
    {
        let sample = clock
            .sample()
            .map_err(PrivateTouchpadRuntimeWorkerFactoryBuildError::Clock)?;
        let route = provider
            .current_route(route_id)
            .map_err(PrivateTouchpadRuntimeWorkerFactoryBuildError::RouteProvider)?;
        validate_private_touchpad_route_session_construction(
            &route,
            expected_sink,
            &stream,
            descriptor,
            first_sequence,
            sample.now_ms,
            limits,
        )
        .map_err(PrivateTouchpadRuntimeWorkerFactoryBuildError::Preflight)?;
        let sink = factory
            .open(&stream, descriptor, first_sequence)
            .map_err(PrivateTouchpadRuntimeWorkerFactoryBuildError::SinkFactory)?;
        let session = PrivateTouchpadRouteSession::new(
            &route,
            expected_sink,
            stream,
            descriptor,
            first_sequence,
            sample.now_ms,
            limits,
            sink,
        )
        .map_err(PrivateTouchpadRuntimeWorkerFactoryBuildError::Session)?;
        Ok(Self {
            route_id,
            provider,
            clock,
            session,
            last_clock: sample,
            metrics: PrivateTouchpadRuntimeWorkerMetrics {
                clock_samples: 1,
                route_snapshots: 1,
                ..PrivateTouchpadRuntimeWorkerMetrics::default()
            },
        })
    }
}
