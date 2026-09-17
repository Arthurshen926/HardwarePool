use capyio_core::StreamId;
use capyio_display::{
    DisplayFrameDescriptor, DisplayFrameFlags, DisplayStreamSpec, LatestDisplayFrameSlot,
    LatestFramePushOutcome, OwnedDisplayFrame,
};
use capyio_video::{VideoColorimetry, VideoPixelFormat};

use crate::WindowsDisplayError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CaptureSize {
    pub width: u32,
    pub height: u32,
}

impl CaptureSize {
    pub fn validate(self) -> Result<(), WindowsDisplayError> {
        if self.width == 0 || self.height == 0 {
            return Err(WindowsDisplayError::InvalidFrame(
                "capture dimensions must be positive".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct CapturedBgraFrame {
    pub size: CaptureSize,
    pub source_timestamp_nanos: u64,
    pub payload: Vec<u8>,
}

impl CapturedBgraFrame {
    fn validate(&self, selected: &DisplayStreamSpec) -> Result<(), WindowsDisplayError> {
        self.size.validate()?;
        if self.size.width != selected.width || self.size.height != selected.height {
            return Err(WindowsDisplayError::ReconfigureRequired {
                width: self.size.width,
                height: self.size.height,
            });
        }
        let expected = selected.packed_frame_bytes().ok_or_else(|| {
            WindowsDisplayError::InvalidFrame("selected frame size overflowed".to_owned())
        })?;
        let actual = u64::try_from(self.payload.len()).map_err(|_| {
            WindowsDisplayError::InvalidFrame("payload length cannot fit in u64".to_owned())
        })?;
        if actual != expected {
            return Err(WindowsDisplayError::InvalidFrame(
                "packed BGRA payload length does not match the exact mode".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum CaptureBackendPoll {
    Idle,
    Frame(CapturedBgraFrame),
    SizeChanged(CaptureSize),
    TargetClosed,
}

/// Platform projection consumed by the deterministic mirror Source.
///
/// Implementations must keep callback work bounded. The Windows WGC backend
/// performs all D3D copy/map and payload allocation from `poll`, not from its
/// frame-arrival callback.
pub trait MirrorCaptureBackend {
    fn start(&mut self) -> Result<(), WindowsDisplayError>;
    fn poll(&mut self) -> Result<CaptureBackendPoll, WindowsDisplayError>;
    fn reconfigure(&mut self, size: CaptureSize) -> Result<(), WindowsDisplayError>;
    fn stop(&mut self) -> Result<(), WindowsDisplayError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MirrorCaptureState {
    Ready,
    Running,
    ReconfigureRequired(CaptureSize),
    TargetClosed,
    Stopped,
    Failed,
}

impl MirrorCaptureState {
    const fn label(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Running => "running",
            Self::ReconfigureRequired(_) => "reconfigure-required",
            Self::TargetClosed => "target-closed",
            Self::Stopped => "stopped",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MirrorCaptureMetrics {
    pub frames_stored: u64,
    pub idle_polls: u64,
    pub size_changes: u64,
    pub clears_stored: u64,
    pub backend_failures: u64,
    pub pending_replacements: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapturePumpOutcome {
    Idle,
    FrameStored(LatestFramePushOutcome),
    ReconfigureRequired {
        size: CaptureSize,
        clear: LatestFramePushOutcome,
    },
    TargetClosed {
        clear: LatestFramePushOutcome,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CaptureStopOutcome {
    pub already_stopped: bool,
    pub clear: Option<LatestFramePushOutcome>,
}

#[derive(Debug)]
pub struct MirrorCaptureSource<B> {
    backend: B,
    selected: DisplayStreamSpec,
    stream_id: StreamId,
    stream_epoch: u64,
    next_sequence: u64,
    last_timestamp_nanos: Option<u64>,
    slot: LatestDisplayFrameSlot,
    state: MirrorCaptureState,
    metrics: MirrorCaptureMetrics,
}

impl<B: MirrorCaptureBackend> MirrorCaptureSource<B> {
    pub fn new(
        backend: B,
        selected: DisplayStreamSpec,
        stream_id: StreamId,
        stream_epoch: u64,
    ) -> Result<Self, WindowsDisplayError> {
        validate_mirror_stream(&selected)?;
        let slot = LatestDisplayFrameSlot::new(selected.clone(), stream_id, stream_epoch)?;
        Ok(Self {
            backend,
            selected,
            stream_id,
            stream_epoch,
            next_sequence: 0,
            last_timestamp_nanos: None,
            slot,
            state: MirrorCaptureState::Ready,
            metrics: MirrorCaptureMetrics::default(),
        })
    }

    pub fn start(&mut self) -> Result<(), WindowsDisplayError> {
        if self.state != MirrorCaptureState::Ready {
            return Err(self.invalid_state("start"));
        }
        if let Err(error) = self.backend.start() {
            self.metrics.backend_failures = self.metrics.backend_failures.saturating_add(1);
            self.state = MirrorCaptureState::Failed;
            return Err(error);
        }
        self.state = MirrorCaptureState::Running;
        Ok(())
    }

    pub fn poll(&mut self) -> Result<CapturePumpOutcome, WindowsDisplayError> {
        if self.state != MirrorCaptureState::Running {
            return Err(self.invalid_state("poll"));
        }
        let polled = match self.backend.poll() {
            Ok(polled) => polled,
            Err(error) => {
                self.fail_closed();
                self.metrics.backend_failures = self.metrics.backend_failures.saturating_add(1);
                self.state = MirrorCaptureState::Failed;
                return Err(error);
            }
        };
        match polled {
            CaptureBackendPoll::Idle => {
                self.metrics.idle_polls = self.metrics.idle_polls.saturating_add(1);
                Ok(CapturePumpOutcome::Idle)
            }
            CaptureBackendPoll::Frame(frame) => self.store_frame(frame),
            CaptureBackendPoll::SizeChanged(size) => self.require_reconfigure(size),
            CaptureBackendPoll::TargetClosed => {
                let clear = self.store_clear()?;
                self.state = MirrorCaptureState::TargetClosed;
                Ok(CapturePumpOutcome::TargetClosed { clear })
            }
        }
    }

    pub fn reconfigure(
        &mut self,
        selected: DisplayStreamSpec,
        new_epoch: u64,
    ) -> Result<bool, WindowsDisplayError> {
        let expected = match self.state {
            MirrorCaptureState::ReconfigureRequired(size) => size,
            _ => return Err(self.invalid_state("reconfigure")),
        };
        validate_mirror_stream(&selected)?;
        if selected.width != expected.width || selected.height != expected.height {
            return Err(WindowsDisplayError::ReconfigureSizeMismatch);
        }
        if new_epoch <= self.stream_epoch {
            return Err(capyio_display::DisplayContractError::NonAdvancingEpoch {
                current_epoch: self.stream_epoch,
                new_epoch,
            }
            .into());
        }
        if let Err(error) = self.backend.reconfigure(expected) {
            self.metrics.backend_failures = self.metrics.backend_failures.saturating_add(1);
            self.state = MirrorCaptureState::Failed;
            return Err(error);
        }
        let discarded = self.slot.reconfigure(selected.clone(), new_epoch)?;
        self.selected = selected;
        self.stream_epoch = new_epoch;
        self.next_sequence = 0;
        self.last_timestamp_nanos = None;
        self.state = MirrorCaptureState::Running;
        Ok(discarded)
    }

    pub fn stop(&mut self) -> Result<CaptureStopOutcome, WindowsDisplayError> {
        if self.state == MirrorCaptureState::Stopped {
            return Ok(CaptureStopOutcome {
                already_stopped: true,
                clear: None,
            });
        }
        let clear_result = if matches!(
            self.state,
            MirrorCaptureState::Running | MirrorCaptureState::ReconfigureRequired(_)
        ) {
            self.store_clear().map(Some)
        } else {
            Ok(None)
        };
        let stop_result = self.backend.stop();
        if let Err(error) = stop_result {
            self.metrics.backend_failures = self.metrics.backend_failures.saturating_add(1);
            self.state = MirrorCaptureState::Failed;
            return Err(error);
        }
        self.state = MirrorCaptureState::Stopped;
        let clear = clear_result?;
        Ok(CaptureStopOutcome {
            already_stopped: false,
            clear,
        })
    }

    pub fn take_latest(&mut self) -> Option<OwnedDisplayFrame> {
        self.slot.take()
    }

    #[must_use]
    pub const fn state(&self) -> MirrorCaptureState {
        self.state
    }

    #[must_use]
    pub fn metrics(&self) -> MirrorCaptureMetrics {
        MirrorCaptureMetrics {
            pending_replacements: self.slot.replacements(),
            ..self.metrics
        }
    }

    fn store_frame(
        &mut self,
        frame: CapturedBgraFrame,
    ) -> Result<CapturePumpOutcome, WindowsDisplayError> {
        if let Err(error) = frame.validate(&self.selected) {
            if let WindowsDisplayError::ReconfigureRequired { width, height } = error {
                return self.require_reconfigure(CaptureSize { width, height });
            }
            self.fail_closed();
            self.state = MirrorCaptureState::Failed;
            return Err(error);
        }
        if self
            .last_timestamp_nanos
            .is_some_and(|previous| frame.source_timestamp_nanos < previous)
        {
            let error =
                WindowsDisplayError::InvalidFrame("source timestamp moved backwards".to_owned());
            self.fail_closed();
            self.state = MirrorCaptureState::Failed;
            return Err(error);
        }
        let sequence = self.take_sequence()?;
        let payload_bytes = u64::try_from(frame.payload.len())
            .map_err(|_| WindowsDisplayError::ArithmeticExhausted)?;
        let owned = OwnedDisplayFrame {
            descriptor: DisplayFrameDescriptor {
                stream_id: self.stream_id,
                stream_epoch: self.stream_epoch,
                sequence,
                source_timestamp_nanos: frame.source_timestamp_nanos,
                duration_nanos: frame_duration_nanos(&self.selected)?,
                payload_bytes,
                flags: DisplayFrameFlags::default(),
            },
            payload: frame.payload,
        };
        let outcome = self.slot.push(owned)?;
        self.last_timestamp_nanos = Some(frame.source_timestamp_nanos);
        self.metrics.frames_stored = self.metrics.frames_stored.saturating_add(1);
        Ok(CapturePumpOutcome::FrameStored(outcome))
    }

    fn require_reconfigure(
        &mut self,
        size: CaptureSize,
    ) -> Result<CapturePumpOutcome, WindowsDisplayError> {
        size.validate()?;
        let clear = self.store_clear()?;
        self.metrics.size_changes = self.metrics.size_changes.saturating_add(1);
        self.state = MirrorCaptureState::ReconfigureRequired(size);
        Ok(CapturePumpOutcome::ReconfigureRequired { size, clear })
    }

    fn store_clear(&mut self) -> Result<LatestFramePushOutcome, WindowsDisplayError> {
        let sequence = self.take_sequence()?;
        let clear = OwnedDisplayFrame {
            descriptor: DisplayFrameDescriptor {
                stream_id: self.stream_id,
                stream_epoch: self.stream_epoch,
                sequence,
                source_timestamp_nanos: self.last_timestamp_nanos.unwrap_or(0),
                duration_nanos: frame_duration_nanos(&self.selected)?,
                payload_bytes: 0,
                flags: DisplayFrameFlags {
                    discontinuity: true,
                    end_of_stream: false,
                    clear_surface: true,
                },
            },
            payload: Vec::new(),
        };
        let outcome = self.slot.push(clear)?;
        self.metrics.clears_stored = self.metrics.clears_stored.saturating_add(1);
        Ok(outcome)
    }

    fn take_sequence(&mut self) -> Result<u64, WindowsDisplayError> {
        let sequence = self.next_sequence;
        self.next_sequence = sequence
            .checked_add(1)
            .ok_or(WindowsDisplayError::ArithmeticExhausted)?;
        Ok(sequence)
    }

    fn fail_closed(&mut self) {
        if self.store_clear().is_err() {
            let _ = self.slot.take();
        }
    }

    fn invalid_state(&self, operation: &'static str) -> WindowsDisplayError {
        WindowsDisplayError::InvalidState {
            operation,
            state: self.state.label(),
        }
    }
}

fn validate_mirror_stream(selected: &DisplayStreamSpec) -> Result<(), WindowsDisplayError> {
    selected.validate()?;
    if selected.pixel_format != VideoPixelFormat::Bgra8
        || selected.colorimetry != VideoColorimetry::SrgbFull
    {
        return Err(WindowsDisplayError::InvalidStream(
            "WGC mirror v1 requires exact BGRA8/sRGB output".to_owned(),
        ));
    }
    Ok(())
}

fn frame_duration_nanos(selected: &DisplayStreamSpec) -> Result<u64, WindowsDisplayError> {
    1_000_000_000_u128
        .checked_mul(u128::from(selected.frame_rate.denominator()))
        .and_then(|value| value.checked_div(u128::from(selected.frame_rate.numerator())))
        .and_then(|value| u64::try_from(value).ok())
        .filter(|value| *value > 0)
        .ok_or(WindowsDisplayError::ArithmeticExhausted)
}
