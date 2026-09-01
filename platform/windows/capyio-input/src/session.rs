use std::fmt;

use capyio_input::{InputStreamDescriptor, TouchpadDescriptor, TouchpadFrame};

use crate::{
    BatchInjectionOutcome, SyntheticTouchpadDevice, SyntheticTouchpadInjectionError,
    WindowsTouchpadBatch, WindowsTouchpadProjection, WindowsTouchpadProjectionDisposition,
    WindowsTouchpadProjectionError, WindowsTouchpadProjector,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyntheticTouchpadSessionState {
    Active,
    Failed,
    Closed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SyntheticTouchpadSubmission {
    pub disposition: WindowsTouchpadProjectionDisposition,
    pub batches_submitted: u8,
    pub contact_records_submitted: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SyntheticTouchpadSessionError {
    Projection(WindowsTouchpadProjectionError),
    Injection {
        primary: SyntheticTouchpadInjectionError,
        cleanup: Option<SyntheticTouchpadInjectionError>,
    },
    Inactive(SyntheticTouchpadSessionState),
}

impl fmt::Display for SyntheticTouchpadSessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Projection(error) => write!(formatter, "touchpad projection failed: {error}"),
            Self::Injection {
                primary,
                cleanup: None,
            } => write!(formatter, "touchpad submission failed: {primary}"),
            Self::Injection {
                primary,
                cleanup: Some(cleanup),
            } => write!(
                formatter,
                "touchpad submission failed: {primary}; cancellation also failed: {cleanup}"
            ),
            Self::Inactive(state) => {
                write!(formatter, "touchpad session is not active: {state:?}")
            }
        }
    }
}

impl std::error::Error for SyntheticTouchpadSessionError {}

impl From<WindowsTouchpadProjectionError> for SyntheticTouchpadSessionError {
    fn from(error: WindowsTouchpadProjectionError) -> Self {
        Self::Projection(error)
    }
}

impl From<SyntheticTouchpadInjectionError> for SyntheticTouchpadSessionError {
    fn from(error: SyntheticTouchpadInjectionError) -> Self {
        Self::Injection {
            primary: error,
            cleanup: None,
        }
    }
}

trait BatchDevice {
    fn inject_batch(
        &mut self,
        batch: &WindowsTouchpadBatch,
    ) -> Result<BatchInjectionOutcome, SyntheticTouchpadInjectionError>;
}

impl BatchDevice for SyntheticTouchpadDevice {
    fn inject_batch(
        &mut self,
        batch: &WindowsTouchpadBatch,
    ) -> Result<BatchInjectionOutcome, SyntheticTouchpadInjectionError> {
        Self::inject_batch(self, batch)
    }
}

struct TouchpadSessionCore<D: BatchDevice> {
    projector: WindowsTouchpadProjector,
    device: D,
    state: SyntheticTouchpadSessionState,
}

impl<D: BatchDevice> TouchpadSessionCore<D> {
    fn new(projector: WindowsTouchpadProjector, device: D) -> Self {
        Self {
            projector,
            device,
            state: SyntheticTouchpadSessionState::Active,
        }
    }

    fn state(&self) -> SyntheticTouchpadSessionState {
        self.state
    }

    fn submit_frame(
        &mut self,
        frame: &TouchpadFrame,
    ) -> Result<SyntheticTouchpadSubmission, SyntheticTouchpadSessionError> {
        self.require_active()?;
        let projection = self.projector.project(frame)?;
        self.submit_projection(&projection)
    }

    fn advance_epoch(
        &mut self,
        new_epoch: u64,
        first_sequence: u64,
    ) -> Result<SyntheticTouchpadSubmission, SyntheticTouchpadSessionError> {
        self.require_active()?;
        let projection = self.projector.advance_epoch(new_epoch, first_sequence)?;
        self.submit_projection(&projection)
    }

    fn close(&mut self) -> Result<SyntheticTouchpadSubmission, SyntheticTouchpadSessionError> {
        self.require_active()?;
        let projection = self.projector.cancel_active();
        match self.submit_projection(&projection) {
            Ok(submission) => {
                self.state = SyntheticTouchpadSessionState::Closed;
                Ok(submission)
            }
            Err(error) => Err(error),
        }
    }

    fn require_active(&self) -> Result<(), SyntheticTouchpadSessionError> {
        if self.state == SyntheticTouchpadSessionState::Active {
            Ok(())
        } else {
            Err(SyntheticTouchpadSessionError::Inactive(self.state))
        }
    }

    fn submit_projection(
        &mut self,
        projection: &WindowsTouchpadProjection,
    ) -> Result<SyntheticTouchpadSubmission, SyntheticTouchpadSessionError> {
        let mut submission = SyntheticTouchpadSubmission {
            disposition: projection.disposition,
            batches_submitted: 0,
            contact_records_submitted: 0,
        };
        for batch in projection.batches() {
            match self.device.inject_batch(batch) {
                Ok(BatchInjectionOutcome::SkippedEmpty) => {}
                Ok(BatchInjectionOutcome::Submitted { contacts }) => {
                    submission.batches_submitted += 1;
                    submission.contact_records_submitted += contacts;
                }
                Err(primary) => {
                    let cleanup = self.cancel_after_failure();
                    return Err(SyntheticTouchpadSessionError::Injection { primary, cleanup });
                }
            }
        }
        Ok(submission)
    }

    fn cancel_after_failure(&mut self) -> Option<SyntheticTouchpadInjectionError> {
        let cleanup = self.projector.cancel_active();
        let mut cleanup_error = None;
        for batch in cleanup.batches() {
            if let Err(error) = self.device.inject_batch(batch) {
                cleanup_error = Some(error);
                break;
            }
        }
        self.state = SyntheticTouchpadSessionState::Failed;
        cleanup_error
    }

    fn best_effort_drop_cleanup(&mut self) {
        if self.state != SyntheticTouchpadSessionState::Active {
            return;
        }
        let cleanup = self.projector.cancel_active();
        for batch in cleanup.batches() {
            if self.device.inject_batch(batch).is_err() {
                break;
            }
        }
        self.state = SyntheticTouchpadSessionState::Closed;
    }
}

impl<D: BatchDevice> Drop for TouchpadSessionCore<D> {
    fn drop(&mut self) {
        self.best_effort_drop_cleanup();
    }
}

pub struct SyntheticTouchpadSession {
    core: TouchpadSessionCore<SyntheticTouchpadDevice>,
}

impl SyntheticTouchpadSession {
    pub fn open(
        stream: &InputStreamDescriptor,
        descriptor: TouchpadDescriptor,
        first_sequence: u64,
    ) -> Result<Self, SyntheticTouchpadSessionError> {
        let projector = WindowsTouchpadProjector::new(stream, descriptor, first_sequence)?;
        let device = SyntheticTouchpadDevice::create(projector.device_parameters())?;
        Ok(Self {
            core: TouchpadSessionCore::new(projector, device),
        })
    }

    #[must_use]
    pub fn state(&self) -> SyntheticTouchpadSessionState {
        self.core.state()
    }

    pub fn submit_frame(
        &mut self,
        frame: &TouchpadFrame,
    ) -> Result<SyntheticTouchpadSubmission, SyntheticTouchpadSessionError> {
        self.core.submit_frame(frame)
    }

    pub fn advance_epoch(
        &mut self,
        new_epoch: u64,
        first_sequence: u64,
    ) -> Result<SyntheticTouchpadSubmission, SyntheticTouchpadSessionError> {
        self.core.advance_epoch(new_epoch, first_sequence)
    }

    pub fn close(&mut self) -> Result<SyntheticTouchpadSubmission, SyntheticTouchpadSessionError> {
        self.core.close()
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use capyio_input::{
        InputFrameHeader, TouchpadButtonState, TouchpadButtonType, TouchpadContact,
        TouchpadFrameKind, TouchpadPhysicalSize, TouchpadPosition,
    };

    use super::*;
    use crate::WindowsTouchpadContactPhase;

    #[derive(Default)]
    struct FakeDevice {
        calls: usize,
        fail_calls: Vec<usize>,
        batches: Vec<WindowsTouchpadBatch>,
    }

    #[derive(Clone, Default)]
    struct ObservedDevice {
        batches: Rc<RefCell<Vec<WindowsTouchpadBatch>>>,
    }

    impl BatchDevice for ObservedDevice {
        fn inject_batch(
            &mut self,
            batch: &WindowsTouchpadBatch,
        ) -> Result<BatchInjectionOutcome, SyntheticTouchpadInjectionError> {
            self.batches.borrow_mut().push(*batch);
            Ok(BatchInjectionOutcome::Submitted {
                contacts: batch.len(),
            })
        }
    }

    impl BatchDevice for FakeDevice {
        fn inject_batch(
            &mut self,
            batch: &WindowsTouchpadBatch,
        ) -> Result<BatchInjectionOutcome, SyntheticTouchpadInjectionError> {
            self.calls += 1;
            self.batches.push(*batch);
            if self.fail_calls.contains(&self.calls) {
                Err(SyntheticTouchpadInjectionError::SubmissionFailed { error_code: 5 })
            } else if batch.is_empty() {
                Ok(BatchInjectionOutcome::SkippedEmpty)
            } else {
                Ok(BatchInjectionOutcome::Submitted {
                    contacts: batch.len(),
                })
            }
        }
    }

    fn stream(epoch: u64) -> InputStreamDescriptor {
        InputStreamDescriptor {
            stream_id: "00000000-0000-4000-8000-00000000c603"
                .parse()
                .expect("stream ID"),
            stream_epoch: epoch,
            clock_domain_id: "windows.session.test".to_owned(),
        }
    }

    fn descriptor() -> TouchpadDescriptor {
        TouchpadDescriptor {
            physical_size: TouchpadPhysicalSize {
                width_himetric: 10_000,
                height_himetric: 6_000,
            },
            max_contacts: 5,
            button_type: TouchpadButtonType::NonClickable,
            reports_contact_size: false,
            reports_pressure: false,
        }
    }

    fn frame(epoch: u64, sequence: u64, contacts: Vec<TouchpadContact>) -> TouchpadFrame {
        TouchpadFrame {
            header: InputFrameHeader {
                stream_id: stream(epoch).stream_id,
                stream_epoch: epoch,
                sequence,
                source_timestamp_nanos: sequence + 1,
            },
            kind: TouchpadFrameKind::Update,
            button: TouchpadButtonState::Released,
            contacts,
        }
    }

    fn cancel_frame(epoch: u64, sequence: u64) -> TouchpadFrame {
        TouchpadFrame {
            kind: TouchpadFrameKind::CancelAll,
            ..frame(epoch, sequence, Vec::new())
        }
    }

    fn contact(id: u32) -> TouchpadContact {
        TouchpadContact {
            contact_id: id,
            position: TouchpadPosition {
                x_himetric: 5_000,
                y_himetric: 3_000,
            },
            confidence: true,
            size: None,
            pressure: None,
        }
    }

    fn core(device: FakeDevice) -> TouchpadSessionCore<FakeDevice> {
        let projector =
            WindowsTouchpadProjector::new(&stream(1), descriptor(), 0).expect("projector");
        TouchpadSessionCore::new(projector, device)
    }

    #[test]
    fn explicit_close_cancels_contacts_and_rejects_more_frames() {
        let mut session = core(FakeDevice::default());
        session
            .submit_frame(&cancel_frame(1, 0))
            .expect("initial cancel");
        let submitted = session
            .submit_frame(&frame(1, 1, vec![contact(1)]))
            .expect("active frame");
        assert_eq!(submitted.batches_submitted, 1);
        let closed = session.close().expect("close");
        assert_eq!(closed.batches_submitted, 1);
        assert_eq!(session.state(), SyntheticTouchpadSessionState::Closed);
        assert_eq!(session.device.batches.len(), 2);
        assert_eq!(
            session.device.batches[1].contacts()[0].phase,
            WindowsTouchpadContactPhase::Cancelled
        );
        assert_eq!(
            session
                .submit_frame(&frame(1, 2, Vec::new()))
                .expect_err("closed session"),
            SyntheticTouchpadSessionError::Inactive(SyntheticTouchpadSessionState::Closed)
        );
    }

    #[test]
    fn submission_failure_attempts_cancel_and_poisons_session() {
        let mut session = core(FakeDevice {
            fail_calls: vec![1],
            ..FakeDevice::default()
        });
        session
            .submit_frame(&cancel_frame(1, 0))
            .expect("initial cancel");
        let error = session
            .submit_frame(&frame(1, 1, vec![contact(2)]))
            .expect_err("submission must fail");
        assert_eq!(
            error,
            SyntheticTouchpadSessionError::Injection {
                primary: SyntheticTouchpadInjectionError::SubmissionFailed { error_code: 5 },
                cleanup: None,
            }
        );
        assert_eq!(session.state(), SyntheticTouchpadSessionState::Failed);
        assert_eq!(session.device.calls, 2);
        assert_eq!(
            session.device.batches[1].contacts()[0].phase,
            WindowsTouchpadContactPhase::Cancelled
        );
        assert_eq!(
            session.close().expect_err("failed session"),
            SyntheticTouchpadSessionError::Inactive(SyntheticTouchpadSessionState::Failed)
        );
    }

    #[test]
    fn cleanup_failure_is_retained_with_primary_failure() {
        let mut session = core(FakeDevice {
            fail_calls: vec![1, 2],
            ..FakeDevice::default()
        });
        session
            .submit_frame(&cancel_frame(1, 0))
            .expect("initial cancel");
        assert_eq!(
            session
                .submit_frame(&frame(1, 1, vec![contact(3)]))
                .expect_err("submission and cleanup must fail"),
            SyntheticTouchpadSessionError::Injection {
                primary: SyntheticTouchpadInjectionError::SubmissionFailed { error_code: 5 },
                cleanup: Some(SyntheticTouchpadInjectionError::SubmissionFailed { error_code: 5 }),
            }
        );
        assert_eq!(session.state(), SyntheticTouchpadSessionState::Failed);
    }

    #[test]
    fn epoch_advance_cancels_active_contacts_before_new_epoch() {
        let mut session = core(FakeDevice::default());
        session
            .submit_frame(&cancel_frame(1, 0))
            .expect("initial cancel");
        session
            .submit_frame(&frame(1, 1, vec![contact(4)]))
            .expect("active frame");
        let advanced = session.advance_epoch(2, 50).expect("epoch advance");
        assert_eq!(
            advanced.disposition,
            WindowsTouchpadProjectionDisposition::EpochCancelled
        );
        assert_eq!(advanced.batches_submitted, 1);
        assert_eq!(
            session.device.batches[1].contacts()[0].phase,
            WindowsTouchpadContactPhase::Cancelled
        );
    }

    #[test]
    fn abandoned_active_session_attempts_bounded_drop_cancellation() {
        let observed = ObservedDevice::default();
        let batches = Rc::clone(&observed.batches);
        {
            let projector =
                WindowsTouchpadProjector::new(&stream(1), descriptor(), 0).expect("projector");
            let mut session = TouchpadSessionCore::new(projector, observed);
            session
                .submit_frame(&cancel_frame(1, 0))
                .expect("initial cancel");
            session
                .submit_frame(&frame(1, 1, vec![contact(5)]))
                .expect("active frame");
        }
        let batches = batches.borrow();
        assert_eq!(batches.len(), 2);
        assert_eq!(
            batches[1].contacts()[0].phase,
            WindowsTouchpadContactPhase::Cancelled
        );
    }

    #[test]
    fn projection_error_keeps_session_active_and_transactional() {
        let mut session = core(FakeDevice::default());
        session
            .submit_frame(&cancel_frame(1, 0))
            .expect("initial cancel");
        let mut wrong_stream = frame(1, 1, vec![contact(6)]);
        wrong_stream.header.stream_id = "00000000-0000-4000-8000-00000000ffff"
            .parse()
            .expect("different stream ID");
        assert!(matches!(
            session.submit_frame(&wrong_stream),
            Err(SyntheticTouchpadSessionError::Projection(_))
        ));
        assert_eq!(session.state(), SyntheticTouchpadSessionState::Active);
        let recovered = session
            .submit_frame(&frame(1, 1, vec![contact(6)]))
            .expect("same sequence remains valid after transactional rejection");
        assert_eq!(recovered.batches_submitted, 1);
    }
}
