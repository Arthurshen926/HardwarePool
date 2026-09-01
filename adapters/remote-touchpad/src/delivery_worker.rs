use std::{error::Error, fmt};

use capyio_core::{PortRef, RouteId};
use capyio_input::{InputStreamDescriptor, TouchpadDescriptor, TouchpadFrame};

use crate::{
    PrivateTouchpadAdmittedChannel, PrivateTouchpadBindingMismatch, PrivateTouchpadClockSample,
    PrivateTouchpadDeliveryBuildError, PrivateTouchpadDeliveryError,
    PrivateTouchpadDeliverySession, PrivateTouchpadMonotonicClock, PrivateTouchpadPacketSource,
    PrivateTouchpadPacketSourceError, PrivateTouchpadRouteBinding,
    PrivateTouchpadRouteBindingError, PrivateTouchpadRouteProvider, delivery::binding_mismatch,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PrivateTouchpadRuntimeDeliveryMetrics {
    pub clock_samples: u64,
    pub route_snapshots: u64,
    pub frames_delivered: u64,
    pub closes: u64,
}

#[derive(Debug)]
pub enum PrivateTouchpadRuntimeDeliveryBuildError<ProviderError, ClockError> {
    Clock(ClockError),
    RouteProvider(ProviderError),
    Binding(PrivateTouchpadRouteBindingError),
    Source(PrivateTouchpadPacketSourceError),
    Delivery(PrivateTouchpadDeliveryBuildError),
}

impl<ProviderError: fmt::Display, ClockError: fmt::Display> fmt::Display
    for PrivateTouchpadRuntimeDeliveryBuildError<ProviderError, ClockError>
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Clock(error) => {
                write!(formatter, "private touchpad delivery clock failed: {error}")
            }
            Self::RouteProvider(error) => {
                write!(
                    formatter,
                    "private touchpad delivery Route provider failed: {error}"
                )
            }
            Self::Binding(error) => write!(
                formatter,
                "private touchpad delivery binding failed: {error}"
            ),
            Self::Source(error) => error.fmt(formatter),
            Self::Delivery(error) => error.fmt(formatter),
        }
    }
}

impl<ProviderError: Error + 'static, ClockError: Error + 'static> Error
    for PrivateTouchpadRuntimeDeliveryBuildError<ProviderError, ClockError>
{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Clock(error) => Some(error),
            Self::RouteProvider(error) => Some(error),
            Self::Binding(error) => Some(error),
            Self::Source(error) => Some(error),
            Self::Delivery(error) => Some(error),
        }
    }
}

#[derive(Debug)]
pub enum PrivateTouchpadRuntimeDeliveryError<ProviderError, ClockError> {
    Clock(ClockError),
    RouteProvider(ProviderError),
    ClockRegression {
        previous: PrivateTouchpadClockSample,
        actual: PrivateTouchpadClockSample,
    },
    Binding(PrivateTouchpadRouteBindingError),
    BindingChanged(PrivateTouchpadBindingMismatch),
    Delivery(PrivateTouchpadDeliveryError),
}

impl<ProviderError: fmt::Display, ClockError: fmt::Display> fmt::Display
    for PrivateTouchpadRuntimeDeliveryError<ProviderError, ClockError>
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Clock(error) => {
                write!(formatter, "private touchpad delivery clock failed: {error}")
            }
            Self::RouteProvider(error) => {
                write!(
                    formatter,
                    "private touchpad delivery Route provider failed: {error}"
                )
            }
            Self::ClockRegression { previous, actual } => write!(
                formatter,
                "private touchpad delivery clock regressed from {previous:?} to {actual:?}"
            ),
            Self::Binding(error) => write!(
                formatter,
                "private touchpad delivery binding failed: {error}"
            ),
            Self::BindingChanged(error) => {
                write!(
                    formatter,
                    "private touchpad Runtime binding changed: {error:?}"
                )
            }
            Self::Delivery(error) => error.fmt(formatter),
        }
    }
}

impl<ProviderError: Error + 'static, ClockError: Error + 'static> Error
    for PrivateTouchpadRuntimeDeliveryError<ProviderError, ClockError>
{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Clock(error) => Some(error),
            Self::RouteProvider(error) => Some(error),
            Self::Binding(error) => Some(error),
            Self::Delivery(error) => Some(error),
            Self::ClockRegression { .. } | Self::BindingChanged(_) => None,
        }
    }
}

type DeliveryResult<T, Provider, Clock> = Result<
    T,
    PrivateTouchpadRuntimeDeliveryError<
        <Provider as PrivateTouchpadRouteProvider>::Error,
        <Clock as PrivateTouchpadMonotonicClock>::Error,
    >,
>;

pub struct PrivateTouchpadRuntimeDeliveryWorker<Provider, Clock, Channel>
where
    Provider: PrivateTouchpadRouteProvider,
    Clock: PrivateTouchpadMonotonicClock,
    Channel: PrivateTouchpadAdmittedChannel,
{
    route_id: RouteId,
    expected_source: PortRef,
    stream_epoch: u64,
    expected_binding: PrivateTouchpadRouteBinding,
    provider: Provider,
    clock: Clock,
    delivery: PrivateTouchpadDeliverySession<Channel>,
    last_clock: PrivateTouchpadClockSample,
    metrics: PrivateTouchpadRuntimeDeliveryMetrics,
}

impl<Provider, Clock, Channel> PrivateTouchpadRuntimeDeliveryWorker<Provider, Clock, Channel>
where
    Provider: PrivateTouchpadRouteProvider,
    Clock: PrivateTouchpadMonotonicClock,
    Channel: PrivateTouchpadAdmittedChannel,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        route_id: RouteId,
        expected_source: PortRef,
        stream: InputStreamDescriptor,
        descriptor: TouchpadDescriptor,
        first_sequence: u64,
        mut provider: Provider,
        mut clock: Clock,
        mut channel: Channel,
    ) -> Result<Self, PrivateTouchpadRuntimeDeliveryBuildError<Provider::Error, Clock::Error>> {
        let sample = match clock.sample() {
            Ok(sample) => sample,
            Err(error) => {
                channel.close();
                return Err(PrivateTouchpadRuntimeDeliveryBuildError::Clock(error));
            }
        };
        let route = match provider.current_route(route_id) {
            Ok(route) => route,
            Err(error) => {
                channel.close();
                return Err(PrivateTouchpadRuntimeDeliveryBuildError::RouteProvider(
                    error,
                ));
            }
        };
        let binding = match PrivateTouchpadRouteBinding::bind_active_source(
            &route,
            route_id,
            expected_source,
            stream.stream_epoch,
            sample.now_ms,
        ) {
            Ok(binding) => binding,
            Err(error) => {
                channel.close();
                return Err(PrivateTouchpadRuntimeDeliveryBuildError::Binding(error));
            }
        };
        let source =
            match PrivateTouchpadPacketSource::new(stream.clone(), descriptor, first_sequence) {
                Ok(source) => source,
                Err(error) => {
                    channel.close();
                    return Err(PrivateTouchpadRuntimeDeliveryBuildError::Source(error));
                }
            };
        let delivery = PrivateTouchpadDeliverySession::new(binding.clone(), source, channel)
            .map_err(PrivateTouchpadRuntimeDeliveryBuildError::Delivery)?;
        Ok(Self {
            route_id,
            expected_source,
            stream_epoch: stream.stream_epoch,
            expected_binding: binding,
            provider,
            clock,
            delivery,
            last_clock: sample,
            metrics: PrivateTouchpadRuntimeDeliveryMetrics {
                clock_samples: 1,
                route_snapshots: 1,
                ..PrivateTouchpadRuntimeDeliveryMetrics::default()
            },
        })
    }

    #[must_use]
    pub const fn metrics(&self) -> PrivateTouchpadRuntimeDeliveryMetrics {
        self.metrics
    }

    pub fn deliver(&mut self, frame: &TouchpadFrame) -> DeliveryResult<(), Provider, Clock> {
        self.context()?;
        self.delivery
            .deliver(frame)
            .map_err(PrivateTouchpadRuntimeDeliveryError::Delivery)?;
        self.metrics.frames_delivered = self.metrics.frames_delivered.saturating_add(1);
        Ok(())
    }

    pub fn close(&mut self) -> DeliveryResult<(), Provider, Clock> {
        self.context()?;
        self.delivery
            .close()
            .map_err(PrivateTouchpadRuntimeDeliveryError::Delivery)?;
        self.metrics.closes = self.metrics.closes.saturating_add(1);
        Ok(())
    }

    fn context(&mut self) -> DeliveryResult<(), Provider, Clock> {
        let sample = match self.clock.sample() {
            Ok(sample) => sample,
            Err(error) => {
                self.delivery.fail_closed();
                return Err(PrivateTouchpadRuntimeDeliveryError::Clock(error));
            }
        };
        self.metrics.clock_samples = self.metrics.clock_samples.saturating_add(1);
        if sample.now_ms < self.last_clock.now_ms || sample.now_nanos < self.last_clock.now_nanos {
            let previous = self.last_clock;
            self.delivery.fail_closed();
            return Err(PrivateTouchpadRuntimeDeliveryError::ClockRegression {
                previous,
                actual: sample,
            });
        }
        self.last_clock = sample;
        let route = match self.provider.current_route(self.route_id) {
            Ok(route) => route,
            Err(error) => {
                self.delivery.fail_closed();
                return Err(PrivateTouchpadRuntimeDeliveryError::RouteProvider(error));
            }
        };
        self.metrics.route_snapshots = self.metrics.route_snapshots.saturating_add(1);
        let binding = match PrivateTouchpadRouteBinding::bind_active_source(
            &route,
            self.route_id,
            self.expected_source,
            self.stream_epoch,
            sample.now_ms,
        ) {
            Ok(binding) => binding,
            Err(error) => {
                self.delivery.fail_closed();
                return Err(PrivateTouchpadRuntimeDeliveryError::Binding(error));
            }
        };
        if let Some(mismatch) = binding_mismatch(&self.expected_binding, &binding) {
            self.delivery.fail_closed();
            return Err(PrivateTouchpadRuntimeDeliveryError::BindingChanged(
                mismatch,
            ));
        }
        Ok(())
    }
}
