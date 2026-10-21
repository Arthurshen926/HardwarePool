#![forbid(unsafe_code)]
//! A finite, owned projection boundary for already-decoded canonical NV12.
//! No decoder, network listener, portal, V4L2 or hardware capture is implemented.
//! The caller admits the authenticated Session and obtains local consent/lease.
//! A PipeWire node or queued frame is not proof of downstream frame delivery.
mod guard;
#[cfg(all(target_os = "linux", feature = "native-pipewire"))]
mod native;
#[cfg(test)]
mod tests;

use capyio_core::{PortRef, RouteId, RouteState, SessionId, StreamId};
use capyio_runtime::NodeRuntime;
use capyio_video::{VideoFrameDescriptor, VideoStreamSpec};
use std::{
    io,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const FRAME_BYTES: usize = 1_382_400;
pub const FRAME_DURATION_NS: u64 = 33_333_333;
pub const MAX_LIFETIME: Duration = Duration::from_secs(300);
pub(crate) fn error(value: impl ToString) -> io::Error {
    io::Error::other(value.to_string())
}
fn wall_ms() -> io::Result<u64> {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(error)?
            .as_millis(),
    )
    .map_err(error)
}

/// Trusted control-to-decoded-media handoff, never authority by itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CameraBinding {
    pub session_id: SessionId,
    pub route_id: RouteId,
    pub stream_id: StreamId,
    pub stream_epoch: u64,
    pub local_sink: PortRef,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmitOutcome {
    Queued,
    DroppedBackpressure,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CameraMetrics {
    pub queued: u64,
    pub dropped: u64,
}

#[cfg_attr(
    not(any(test, all(target_os = "linux", feature = "native-pipewire"))),
    allow(dead_code)
)]
struct Frame {
    descriptor: VideoFrameDescriptor,
    payload: Vec<u8>,
}
trait Projection: Send {
    fn submit(&mut self, frame: Frame) -> io::Result<SubmitOutcome>;
    fn check(&mut self) -> io::Result<()>;
    fn stop(&mut self) -> io::Result<()>;
    fn pid(&self) -> Option<u32> {
        None
    }
    fn node_name(&self) -> Option<&str> {
        None
    }
}

/// Owns the actual Runtime and one helper. Access to Runtime is explicit so a
/// local control owner can revoke/stop; every subsequent poll/submit rechecks it.
/// The helper has an independent finite deadline even if the caller stops polling.
/// Dropping this owner destroys its helper and stops only its original epoch.
/// Route stays Starting: queue acceptance must not masquerade as delivered video.
pub struct RuntimeCamera {
    runtime: NodeRuntime,
    guard: guard::Guard,
    projection: Box<dyn Projection>,
    started: Instant,
    lifetime: Duration,
    last_sequence: Option<u64>,
    last_timestamp: Option<u64>,
    discontinuity: bool,
    stopped: bool,
    metrics: CameraMetrics,
}
impl RuntimeCamera {
    /// Spawn only the packaged fixed-name sibling helper after validating the
    /// exact live Runtime, finite grant and local Sink lease. No executable,
    /// module, configuration path or peer-supplied command is accepted.
    pub fn start(
        runtime: NodeRuntime,
        binding: CameraBinding,
        lifetime: Duration,
    ) -> io::Result<Self> {
        let now = wall_ms()?;
        Self::open(runtime, binding, lifetime, now, |lifetime, deadline| {
            #[cfg(all(target_os = "linux", feature = "native-pipewire"))]
            {
                Ok(
                    Box::new(native::NativeProjection::spawn(lifetime, deadline)?)
                        as Box<dyn Projection>,
                )
            }
            #[cfg(not(all(target_os = "linux", feature = "native-pipewire")))]
            {
                let _ = (lifetime, deadline);
                Err(error("native-pipewire feature is not enabled on Linux"))
            }
        })
    }
    fn open(
        runtime: NodeRuntime,
        binding: CameraBinding,
        lifetime: Duration,
        now: u64,
        open: impl FnOnce(Duration, u64) -> io::Result<Box<dyn Projection>>,
    ) -> io::Result<Self> {
        if lifetime.is_zero() || lifetime > MAX_LIFETIME {
            return Err(error("camera lifetime outside 1ms..=300s"));
        }
        let guard = guard::Guard::new(&runtime, binding, now)?;
        let lifetime = lifetime.min(Duration::from_millis(guard.deadline_ms - now));
        if lifetime.as_millis() == 0 {
            return Err(error("camera lifetime is less than 1ms"));
        }
        let started = Instant::now();
        let projection = open(
            lifetime,
            guard
                .deadline_ms
                .min(now.saturating_add(lifetime.as_millis() as u64)),
        )?;
        let mut owner = Self {
            runtime,
            guard,
            projection,
            started,
            lifetime,
            last_sequence: None,
            last_timestamp: None,
            discontinuity: true,
            stopped: false,
            metrics: CameraMetrics::default(),
        };
        // Opening the helper consumes part of the admitted lifetime.
        owner.check_at(now.saturating_add(owner.started.elapsed().as_millis() as u64))?;
        Ok(owner)
    }
    pub fn runtime(&self) -> &NodeRuntime {
        &self.runtime
    }
    pub fn runtime_mut(&mut self) -> &mut NodeRuntime {
        &mut self.runtime
    }
    pub fn helper_pid(&self) -> Option<u32> {
        self.projection.pid()
    }
    pub fn node_name(&self) -> Option<&str> {
        self.projection.node_name()
    }
    pub fn metrics(&self) -> CameraMetrics {
        self.metrics
    }
    pub fn poll(&mut self) -> io::Result<()> {
        let now = wall_ms()?;
        self.check_at(now)
    }
    fn check_at(&mut self, now: u64) -> io::Result<()> {
        let result = (|| {
            if self.stopped || self.started.elapsed() >= self.lifetime {
                return Err(error("camera owner stopped or expired"));
            }
            self.guard.check(&self.runtime, now)?;
            self.projection.check()
        })();
        if let Err(reason) = result {
            let _ = self.stop();
            return Err(reason);
        }
        Ok(())
    }
    /// Payload ownership transfers only after its exact descriptor is checked.
    /// Queue is bounded; backpressure drops whole frames, never partial records.
    /// Invalid input is terminal so a corrupted stream cannot silently resume.
    pub fn submit(
        &mut self,
        descriptor: VideoFrameDescriptor,
        payload: Vec<u8>,
    ) -> io::Result<SubmitOutcome> {
        let now = wall_ms()?;
        self.submit_at(descriptor, payload, now)
    }
    fn submit_at(
        &mut self,
        mut descriptor: VideoFrameDescriptor,
        payload: Vec<u8>,
        now: u64,
    ) -> io::Result<SubmitOutcome> {
        let result = (|| {
            self.check_at(now)?;
            descriptor
                .validate(&VideoStreamSpec::camera_720p30_nv12())
                .map_err(error)?;
            if descriptor.stream_id != self.guard.binding.stream_id
                || descriptor.stream_epoch != self.guard.binding.stream_epoch
                || descriptor.source_timestamp_nanos > i64::MAX as u64
                || descriptor.flags.end_of_stream
                || descriptor.duration_nanos != FRAME_DURATION_NS
                || descriptor.payload_bytes != FRAME_BYTES as u64
                || payload.len() != FRAME_BYTES
                || self
                    .last_sequence
                    .is_some_and(|previous| descriptor.sequence <= previous)
                || self
                    .last_timestamp
                    .is_some_and(|previous| descriptor.source_timestamp_nanos <= previous)
            {
                return Err(error(
                    "invalid camera frame identity, timing, ordering or length",
                ));
            }
            if self
                .last_sequence
                .is_some_and(|previous| previous.checked_add(1) != Some(descriptor.sequence))
            {
                self.discontinuity = true;
            }
            self.last_sequence = Some(descriptor.sequence);
            self.last_timestamp = Some(descriptor.source_timestamp_nanos);
            descriptor.flags.discontinuity |= self.discontinuity;
            let outcome = self.projection.submit(Frame {
                descriptor,
                payload,
            })?;
            match outcome {
                SubmitOutcome::Queued => {
                    self.metrics.queued += 1;
                    self.discontinuity = false;
                }
                SubmitOutcome::DroppedBackpressure => {
                    self.metrics.dropped += 1;
                    self.discontinuity = true;
                }
            }
            Ok(outcome)
        })();
        if result.is_err() {
            let _ = self.stop();
        }
        result
    }
    pub fn stop(&mut self) -> io::Result<()> {
        self.stopped = true;
        let owns = self.guard.owns(&self.runtime);
        if owns
            && self
                .runtime
                .route(self.guard.binding.route_id)
                .is_ok_and(|r| {
                    matches!(
                        r.state,
                        RouteState::Starting | RouteState::Active | RouteState::Offline
                    )
                })
        {
            let _ = self.runtime.begin_route_stop(self.guard.binding.route_id);
        }
        self.projection.stop()?;
        if owns
            && self
                .runtime
                .route(self.guard.binding.route_id)
                .is_ok_and(|r| r.state == RouteState::Stopping)
        {
            self.runtime
                .stop_route(self.guard.binding.route_id)
                .map_err(error)?;
        }
        Ok(())
    }
}
impl Drop for RuntimeCamera {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
