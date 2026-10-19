//! Composes the authenticated private data stream with the existing native
//! Sink factory boundary. Runtime state/lease authority remain host-owned.

use std::{
    fmt::Display,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use capyio_control_tls::touchpad::{TouchpadTlsBinding, TouchpadTlsChannel, TouchpadTlsStopHandle};
use capyio_core::{Route, RouteState};
use capyio_input::{
    InputStreamDescriptor, TouchpadButtonType, TouchpadDescriptor, TouchpadPhysicalSize,
};

use crate::{
    PrivateTouchpadReceiverLimits, PrivateTouchpadRouteBinding, PrivateTouchpadSink,
    PrivateTouchpadSinkFactory, PrivateTouchpadTransportReceiver,
    PrivateTouchpadTransportReceiverState,
};

/// A host checks its own Sink lease, exact Ready Session and current Route.
/// Values are local trusted state, never a peer's declaration of authority.
pub struct TouchpadSinkAdmission {
    pub route: Route,
    pub local_lease_expires_at_ms: u64,
    pub session_ready: bool,
}

pub fn touchpad_tls_descriptor() -> TouchpadDescriptor {
    TouchpadDescriptor {
        physical_size: TouchpadPhysicalSize {
            width_himetric: 12_000,
            height_himetric: 7_000,
        },
        max_contacts: 5,
        button_type: TouchpadButtonType::NonClickable,
        reports_contact_size: false,
        reports_pressure: true,
    }
}

pub fn touchpad_tls_stream(binding: TouchpadTlsBinding) -> InputStreamDescriptor {
    InputStreamDescriptor {
        stream_id: binding.stream_id,
        stream_epoch: binding.epoch,
        clock_domain_id: "android.elapsed-realtime-nanos".to_owned(),
    }
}

pub fn touchpad_record_binding(binding: TouchpadTlsBinding) -> PrivateTouchpadRouteBinding {
    PrivateTouchpadRouteBinding {
        route_id: binding.route_id,
        session_id: binding.session.session_id,
        source: binding.source,
        sink: binding.sink,
        route_epoch: binding.epoch,
        authorization_expires_at_ms: Some(binding.expires_at_ms),
    }
}

fn time_ms() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_millis() as u64)
        .map_err(|_| "touchpad authorization clock unavailable".to_owned())
}

fn validate_admission(
    binding: TouchpadTlsBinding,
    admission: TouchpadSinkAdmission,
    now: u64,
    starting: bool,
) -> Result<(), String> {
    if !admission.session_ready
        || admission.local_lease_expires_at_ms <= now
        || admission.local_lease_expires_at_ms > binding.expires_at_ms
        || (!starting && admission.route.state != RouteState::Active)
        || TouchpadTlsBinding::from_route(binding.session, &admission.route, binding.stream_id, now)
            .map_err(|error| error.to_string())?
            != binding
    {
        return Err("touchpad Route, Session or local Sink lease is no longer admitted".to_owned());
    }
    Ok(())
}

/// One worker owns this value. Stop handles contain shutdown authority only;
/// hosts must signal one before joining a worker. No UI or native callback I/O.
pub struct SecureTouchpadReceiver<F: PrivateTouchpadSinkFactory> {
    channel: TouchpadTlsChannel,
    receiver: PrivateTouchpadTransportReceiver<F>,
    clock: Instant,
    last_now_ms: u64,
    frames: u64,
}

impl<F: PrivateTouchpadSinkFactory> SecureTouchpadReceiver<F>
where
    F::Error: Display,
    <F::Sink as PrivateTouchpadSink>::Error: Display,
{
    pub fn accept(
        mut channel: TouchpadTlsChannel,
        factory: F,
        mut admission: impl FnMut() -> Result<TouchpadSinkAdmission, String>,
    ) -> Result<Self, String> {
        let binding = channel.binding();
        let now = time_ms()?;
        validate_admission(binding, admission()?, now, true)?;
        let hello = channel
            .receive_record()
            .map_err(|error| error.to_string())?;
        validate_admission(binding, admission()?, time_ms()?, true)?;
        let mut receiver = PrivateTouchpadTransportReceiver::new(
            touchpad_record_binding(binding),
            touchpad_tls_stream(binding),
            touchpad_tls_descriptor(),
            0,
            PrivateTouchpadReceiverLimits {
                max_packets_per_second: 240,
                active_idle_timeout_nanos: 2_000_000_000,
            },
            factory,
        )
        .map_err(|error| error.to_string())?;
        // This is the first point that can create/open a real native Sink.
        receiver
            .accept_hello(&hello)
            .map_err(|error| error.to_string())?;
        Ok(Self {
            channel,
            receiver,
            clock: Instant::now(),
            last_now_ms: now,
            frames: 0,
        })
    }

    pub fn stop_handle(&self) -> Result<TouchpadTlsStopHandle, String> {
        self.channel
            .stop_handle()
            .map_err(|error| error.to_string())
    }

    pub const fn frames(&self) -> u64 {
        self.frames
    }

    /// true: processed and acknowledged a frame; false: orderly Close.
    /// All failures cancel native state and terminate this data epoch.
    pub fn receive_next(
        &mut self,
        mut admission: impl FnMut() -> Result<TouchpadSinkAdmission, String>,
    ) -> Result<bool, String> {
        let result = (|| {
            self.check_admission(admission()?)?;
            let record = self
                .channel
                .receive_record()
                .map_err(|error| error.to_string())?;
            self.check_admission(admission()?)?;
            match record.get(5) {
                Some(2) => {
                    let arrival = (self.clock.elapsed().as_nanos() as u64).max(1);
                    let accepted = self
                        .receiver
                        .receive_data(&record, arrival)
                        .map_err(|error| error.to_string())?;
                    self.channel
                        .send_record(accepted.ack.as_bytes())
                        .map_err(|error| error.to_string())?;
                    self.frames = self.frames.saturating_add(1);
                    Ok(true)
                }
                Some(4) => {
                    self.receiver
                        .accept_close(&record)
                        .map_err(|error| error.to_string())?;
                    Ok(false)
                }
                _ => Err("unexpected touchpad data record kind".to_owned()),
            }
        })();
        if !matches!(result, Ok(true)) {
            let cleanup = self.close();
            if let Err(cleanup) = cleanup {
                return Err(format!(
                    "{}; touchpad cleanup: {cleanup}",
                    result.err().unwrap_or_else(|| "close requested".to_owned())
                ));
            }
        }
        result
    }

    fn check_admission(&mut self, admission: TouchpadSinkAdmission) -> Result<(), String> {
        let now = time_ms()?;
        if now < self.last_now_ms {
            return Err("touchpad authorization clock moved backwards".to_owned());
        }
        self.last_now_ms = now;
        validate_admission(self.channel.binding(), admission, now, self.frames == 0)
    }

    pub fn close(&mut self) -> Result<(), String> {
        self.channel.close();
        if self.receiver.state() == PrivateTouchpadTransportReceiverState::Active {
            self.receiver
                .disconnect()
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

impl<F: PrivateTouchpadSinkFactory> Drop for SecureTouchpadReceiver<F> {
    fn drop(&mut self) {
        self.channel.close();
        if self.receiver.state() == PrivateTouchpadTransportReceiverState::Active {
            let _ = self.receiver.disconnect();
        }
    }
}
