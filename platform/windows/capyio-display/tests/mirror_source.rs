use std::collections::VecDeque;

use capyio_core::StreamId;
use capyio_display::{DisplayStreamSpec, LatestFramePushOutcome};
use capyio_video::{VideoColorimetry, VideoPixelFormat};
use capyio_windows_display::{
    CaptureBackendPoll, CapturePumpOutcome, CaptureSize, CapturedBgraFrame, MirrorCaptureBackend,
    MirrorCaptureSource, MirrorCaptureState, WindowsDisplayError,
};

#[derive(Debug, Default)]
struct FakeBackend {
    events: VecDeque<Result<CaptureBackendPoll, WindowsDisplayError>>,
    started: bool,
    stopped: bool,
    reconfigured: Vec<CaptureSize>,
}

impl FakeBackend {
    fn with_events(
        events: impl IntoIterator<Item = Result<CaptureBackendPoll, WindowsDisplayError>>,
    ) -> Self {
        Self {
            events: events.into_iter().collect(),
            ..Self::default()
        }
    }
}

impl MirrorCaptureBackend for FakeBackend {
    fn start(&mut self) -> Result<(), WindowsDisplayError> {
        self.started = true;
        Ok(())
    }

    fn poll(&mut self) -> Result<CaptureBackendPoll, WindowsDisplayError> {
        self.events
            .pop_front()
            .unwrap_or(Ok(CaptureBackendPoll::Idle))
    }

    fn reconfigure(&mut self, size: CaptureSize) -> Result<(), WindowsDisplayError> {
        self.reconfigured.push(size);
        Ok(())
    }

    fn stop(&mut self) -> Result<(), WindowsDisplayError> {
        self.stopped = true;
        Ok(())
    }
}

fn spec(width: u32, height: u32) -> DisplayStreamSpec {
    DisplayStreamSpec {
        width,
        height,
        ..DisplayStreamSpec::fixture_720p30_bgra()
    }
}

fn frame(width: u32, height: u32, timestamp: u64, fill: u8) -> CapturedBgraFrame {
    CapturedBgraFrame {
        size: CaptureSize { width, height },
        source_timestamp_nanos: timestamp,
        payload: vec![fill; usize::try_from(width * height * 4).unwrap()],
    }
}

#[test]
fn source_requires_exact_bgra_stream_and_lifecycle() {
    let mut nv12 = spec(64, 36);
    nv12.pixel_format = VideoPixelFormat::Nv12;
    nv12.colorimetry = VideoColorimetry::Bt709Limited;
    assert!(matches!(
        MirrorCaptureSource::new(FakeBackend::default(), nv12, StreamId::new(), 1),
        Err(WindowsDisplayError::InvalidStream(_))
    ));

    let mut source =
        MirrorCaptureSource::new(FakeBackend::default(), spec(64, 36), StreamId::new(), 1).unwrap();
    assert!(matches!(
        source.poll(),
        Err(WindowsDisplayError::InvalidState {
            operation: "poll",
            state: "ready"
        })
    ));
    source.start().unwrap();
    assert_eq!(source.state(), MirrorCaptureState::Running);
    assert_eq!(source.poll().unwrap(), CapturePumpOutcome::Idle);
    let stopped = source.stop().unwrap();
    assert!(!stopped.already_stopped);
    assert!(stopped.clear.is_some());
    assert!(source.take_latest().unwrap().descriptor.flags.clear_surface);
}

#[test]
fn source_replaces_pending_frames_without_building_latency() {
    let backend = FakeBackend::with_events([
        Ok(CaptureBackendPoll::Frame(frame(64, 36, 10, 1))),
        Ok(CaptureBackendPoll::Frame(frame(64, 36, 20, 2))),
    ]);
    let mut source = MirrorCaptureSource::new(backend, spec(64, 36), StreamId::new(), 1).unwrap();
    source.start().unwrap();

    assert_eq!(
        source.poll().unwrap(),
        CapturePumpOutcome::FrameStored(LatestFramePushOutcome::Stored {
            gap: None,
            replaced_pending: false
        })
    );
    assert_eq!(
        source.poll().unwrap(),
        CapturePumpOutcome::FrameStored(LatestFramePushOutcome::Stored {
            gap: None,
            replaced_pending: true
        })
    );
    let latest = source.take_latest().unwrap();
    assert_eq!(latest.descriptor.sequence, 1);
    assert_eq!(latest.payload[0], 2);
    assert_eq!(source.metrics().frames_stored, 2);
    assert_eq!(source.metrics().pending_replacements, 1);
}

#[test]
fn size_change_clears_and_requires_exact_new_epoch() {
    let resized = CaptureSize {
        width: 80,
        height: 40,
    };
    let backend = FakeBackend::with_events([
        Ok(CaptureBackendPoll::Frame(frame(64, 36, 10, 1))),
        Ok(CaptureBackendPoll::SizeChanged(resized)),
        Ok(CaptureBackendPoll::Frame(frame(80, 40, 20, 3))),
    ]);
    let mut source = MirrorCaptureSource::new(backend, spec(64, 36), StreamId::new(), 1).unwrap();
    source.start().unwrap();
    source.poll().unwrap();
    assert!(matches!(
        source.poll().unwrap(),
        CapturePumpOutcome::ReconfigureRequired { size, .. } if size == resized
    ));
    assert_eq!(
        source.state(),
        MirrorCaptureState::ReconfigureRequired(resized)
    );
    let clear = source.take_latest().unwrap();
    assert!(clear.descriptor.flags.clear_surface);
    assert!(clear.payload.is_empty());
    assert_eq!(
        source.reconfigure(spec(81, 40), 2),
        Err(WindowsDisplayError::ReconfigureSizeMismatch)
    );
    assert!(matches!(
        source.reconfigure(spec(80, 40), 1),
        Err(WindowsDisplayError::DisplayContract(_))
    ));
    source.reconfigure(spec(80, 40), 2).unwrap();
    assert_eq!(source.state(), MirrorCaptureState::Running);
    source.poll().unwrap();
    let first_new_epoch = source.take_latest().unwrap();
    assert_eq!(first_new_epoch.descriptor.stream_epoch, 2);
    assert_eq!(first_new_epoch.descriptor.sequence, 0);
}

#[test]
fn target_close_stop_and_backend_failure_fail_closed() {
    let backend = FakeBackend::with_events([
        Ok(CaptureBackendPoll::Frame(frame(64, 36, 10, 1))),
        Ok(CaptureBackendPoll::TargetClosed),
    ]);
    let mut source = MirrorCaptureSource::new(backend, spec(64, 36), StreamId::new(), 1).unwrap();
    source.start().unwrap();
    source.poll().unwrap();
    assert!(matches!(
        source.poll().unwrap(),
        CapturePumpOutcome::TargetClosed { .. }
    ));
    assert_eq!(source.state(), MirrorCaptureState::TargetClosed);
    assert!(source.take_latest().unwrap().descriptor.flags.clear_surface);
    assert!(!source.stop().unwrap().already_stopped);
    assert!(source.stop().unwrap().already_stopped);

    let backend =
        FakeBackend::with_events([Err(WindowsDisplayError::Backend("device lost".to_owned()))]);
    let mut failed = MirrorCaptureSource::new(backend, spec(64, 36), StreamId::new(), 1).unwrap();
    failed.start().unwrap();
    assert_eq!(
        failed.poll(),
        Err(WindowsDisplayError::Backend("device lost".to_owned()))
    );
    assert_eq!(failed.state(), MirrorCaptureState::Failed);
    assert!(failed.take_latest().unwrap().descriptor.flags.clear_surface);
}
