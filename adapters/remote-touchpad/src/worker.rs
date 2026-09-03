use std::{error::Error, fmt};

use capyio_core::{PortRef, Route, RouteId};
use capyio_input::{InputStreamDescriptor, TouchpadDescriptor};

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
