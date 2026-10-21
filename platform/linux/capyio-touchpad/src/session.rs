//! The transport owns its device; a rejected or uncertain frame is terminal.
use crate::{EventBatch, TouchpadProjector};
use capyio_input::{InputStreamDescriptor, TouchpadDescriptor, TouchpadFrame};
use capyio_remote_touchpad_adapter::{PrivateTouchpadSink, PrivateTouchpadSinkFactory};
use std::{
    io,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// Success means the entire batch, including SYN_REPORT, was committed. A queue
/// admission or partial write MUST NOT return success. Error makes the transport
/// unusable; destroy its device rather than retrying an uncertain frame.
pub trait TouchpadTransportStop: Send + Sync {
    fn stop(&self);
}

pub trait TouchpadEventTransport {
    fn stop_handle(&self) -> io::Result<Option<std::sync::Arc<dyn TouchpadTransportStop>>> {
        Ok(None)
    }
    fn submit(&mut self, events: &EventBatch) -> io::Result<()>;
    /// Release native contacts, synchronize, destroy, and close, idempotently.
    fn close(&mut self) -> io::Result<()>;
}

fn error(message: impl ToString) -> io::Error {
    io::Error::other(message.to_string())
}
pub(crate) fn wall_ms() -> io::Result<u64> {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(error)?
            .as_millis(),
    )
    .map_err(error)
}

pub struct LinuxTouchpadSession<T: TouchpadEventTransport> {
    projector: TouchpadProjector,
    transport: T,
    stopped: bool,
    initialized: bool,
    deadline_ms: u64,
    monotonic_deadline: Instant,
    last_wall_ms: u64,
}
impl<T: TouchpadEventTransport> LinuxTouchpadSession<T> {
    pub fn new(
        projector: TouchpadProjector,
        mut transport: T,
        deadline_ms: u64,
    ) -> io::Result<Self> {
        let validation = (|| {
            let now = wall_ms()?;
            let remaining = deadline_ms
                .checked_sub(now)
                .filter(|v| *v > 0 && *v <= 300_000)
                .ok_or_else(|| {
                    error("touchpad requires an unexpired finite deadline of at most 300 seconds")
                })?;
            Ok::<_, io::Error>((now, remaining))
        })();
        let (now, remaining) = match validation {
            Ok(value) => value,
            Err(e) => {
                let _ = transport.close();
                return Err(e);
            }
        };
        Ok(Self {
            projector,
            transport,
            stopped: false,
            initialized: false,
            deadline_ms,
            monotonic_deadline: Instant::now() + Duration::from_millis(remaining),
            last_wall_ms: now,
        })
    }
    fn check(&mut self) -> io::Result<()> {
        let now = wall_ms()?;
        if self.stopped
            || now < self.last_wall_ms
            || now >= self.deadline_ms
            || Instant::now() >= self.monotonic_deadline
        {
            return Err(error(
                "touchpad stopped, expired or authorization clock regressed",
            ));
        }
        self.last_wall_ms = now;
        Ok(())
    }
    fn fail(&mut self, primary: io::Error) -> io::Error {
        match self.close() {
            Ok(()) => primary,
            Err(cleanup) => error(format!("{primary}; touchpad cleanup: {cleanup}")),
        }
    }
}
impl<T: TouchpadEventTransport> PrivateTouchpadSink for LinuxTouchpadSession<T> {
    type Error = io::Error;
    fn submit_frame(&mut self, frame: &TouchpadFrame) -> io::Result<()> {
        if let Err(e) = self.check() {
            return Err(self.fail(e));
        }
        if !self.initialized && frame.kind != capyio_input::TouchpadFrameKind::CancelAll {
            return Err(self.fail(error("touchpad initial frame must be CancelAll")));
        }
        let events = match self.projector.project(frame) {
            Ok(events) => events,
            Err(e) => return Err(self.fail(error(e))),
        };
        if let Err(e) = self.transport.submit(&events) {
            return Err(self.fail(e));
        }
        self.initialized = true;
        // A complete native write can race expiry; do not acknowledge expired input.
        if let Err(e) = self.check() {
            return Err(self.fail(e));
        }
        Ok(())
    }
    fn advance_epoch(&mut self, epoch: u64, first: u64) -> io::Result<()> {
        if let Err(e) = self.check() {
            return Err(self.fail(e));
        }
        let events = match self.projector.advance_epoch(epoch, first) {
            Ok(events) => events,
            Err(e) => return Err(self.fail(error(e))),
        };
        if let Err(e) = self.transport.submit(&events) {
            return Err(self.fail(e));
        }
        self.initialized = false;
        Ok(())
    }
    fn close(&mut self) -> io::Result<()> {
        if self.stopped {
            return Ok(());
        }
        self.stopped = true;
        // The helper's own close repeats release defensively. Never retry a data
        // frame after uncertain I/O; close is a terminal, idempotent operation.
        let release = self.projector.release_all();
        let sent = self.transport.submit(&release);
        let closed = self.transport.close();
        sent.and(closed)
    }
}
impl<T: TouchpadEventTransport> Drop for LinuxTouchpadSession<T> {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

/// A single-use factory pinned to the exact admitted stream. Construction has
/// no device side effects. SecureTouchpadReceiver invokes open after its exact
/// Hello and local Runtime admission checks; the opener must recheck admission
/// immediately before spawning and give the helper this same finite deadline.
pub struct LinuxTouchpadSinkFactory<O> {
    stream: InputStreamDescriptor,
    descriptor: TouchpadDescriptor,
    first_sequence: u64,
    deadline_ms: u64,
    opener: Option<O>,
}
impl<O> LinuxTouchpadSinkFactory<O> {
    pub fn new(
        stream: InputStreamDescriptor,
        descriptor: TouchpadDescriptor,
        first_sequence: u64,
        deadline_ms: u64,
        opener: O,
    ) -> io::Result<Self> {
        TouchpadProjector::new(&stream, descriptor, first_sequence).map_err(error)?;
        let now = wall_ms()?;
        if deadline_ms <= now || deadline_ms - now > 300_000 {
            return Err(error("invalid touchpad factory deadline"));
        }
        Ok(Self {
            stream,
            descriptor,
            first_sequence,
            deadline_ms,
            opener: Some(opener),
        })
    }
}
impl<O, T> PrivateTouchpadSinkFactory for LinuxTouchpadSinkFactory<O>
where
    O: FnOnce(u64) -> io::Result<T>,
    T: TouchpadEventTransport,
{
    type Sink = LinuxTouchpadSession<T>;
    type Error = io::Error;
    fn open(
        &mut self,
        stream: &InputStreamDescriptor,
        descriptor: TouchpadDescriptor,
        first_sequence: u64,
    ) -> io::Result<Self::Sink> {
        if stream != &self.stream
            || descriptor != self.descriptor
            || first_sequence != self.first_sequence
        {
            self.opener.take();
            return Err(error("touchpad factory binding mismatch"));
        }
        let projector =
            TouchpadProjector::new(stream, descriptor, first_sequence).map_err(error)?;
        let opener = self
            .opener
            .take()
            .ok_or_else(|| error("touchpad factory already consumed"))?;
        if wall_ms()? >= self.deadline_ms {
            return Err(error("touchpad factory deadline expired"));
        }
        let transport = opener(self.deadline_ms)?;
        // If admission expires during opening, close the newly created native
        // owner before returning, rather than losing a live transport on error.
        LinuxTouchpadSession::new(projector, transport, self.deadline_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use capyio_core::StreamId;
    use capyio_input::{InputFrameHeader, TouchpadButtonState, TouchpadFrameKind};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    struct Fake {
        closes: Arc<AtomicUsize>,
        fail: bool,
    }
    impl TouchpadEventTransport for Fake {
        fn submit(&mut self, _: &EventBatch) -> io::Result<()> {
            if self.fail {
                Err(error("uncertain write"))
            } else {
                Ok(())
            }
        }
        fn close(&mut self) -> io::Result<()> {
            self.closes.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }
    fn stream() -> InputStreamDescriptor {
        InputStreamDescriptor {
            stream_id: StreamId::new(),
            stream_epoch: 1,
            clock_domain_id: "test".into(),
        }
    }
    fn frame(
        stream: &InputStreamDescriptor,
        sequence: u64,
        kind: TouchpadFrameKind,
    ) -> TouchpadFrame {
        TouchpadFrame {
            header: InputFrameHeader {
                stream_id: stream.stream_id,
                stream_epoch: stream.stream_epoch,
                sequence,
                source_timestamp_nanos: sequence + 1,
            },
            kind,
            button: TouchpadButtonState::Released,
            contacts: vec![],
        }
    }
    #[test]
    fn expired_constructor_closes_even_transport_without_drop() {
        let closes = Arc::new(AtomicUsize::new(0));
        let projector =
            TouchpadProjector::new(&stream(), crate::linux_touchpad_descriptor(), 0).unwrap();
        assert!(
            LinuxTouchpadSession::new(
                projector,
                Fake {
                    closes: closes.clone(),
                    fail: false
                },
                1
            )
            .is_err()
        );
        assert_eq!(closes.load(Ordering::SeqCst), 1);
    }
    #[test]
    fn startup_update_and_uncertain_write_close_once() {
        for fail in [false, true] {
            let stream = stream();
            let closes = Arc::new(AtomicUsize::new(0));
            let projector =
                TouchpadProjector::new(&stream, crate::linux_touchpad_descriptor(), 0).unwrap();
            let mut session = LinuxTouchpadSession::new(
                projector,
                Fake {
                    closes: closes.clone(),
                    fail,
                },
                wall_ms().unwrap() + 5000,
            )
            .unwrap();
            let kind = if fail {
                TouchpadFrameKind::CancelAll
            } else {
                TouchpadFrameKind::Update
            };
            assert!(session.submit_frame(&frame(&stream, 0, kind)).is_err());
            drop(session);
            assert_eq!(closes.load(Ordering::SeqCst), 1);
        }
    }
    #[test]
    fn advancing_epoch_requires_new_initial_cancel_all() {
        let mut stream = stream();
        let closes = Arc::new(AtomicUsize::new(0));
        let projector =
            TouchpadProjector::new(&stream, crate::linux_touchpad_descriptor(), 0).unwrap();
        let mut session = LinuxTouchpadSession::new(
            projector,
            Fake {
                closes: closes.clone(),
                fail: false,
            },
            wall_ms().unwrap() + 5000,
        )
        .unwrap();
        session
            .submit_frame(&frame(&stream, 0, TouchpadFrameKind::CancelAll))
            .unwrap();
        session.advance_epoch(2, 0).unwrap();
        stream.stream_epoch = 2;
        assert!(
            session
                .submit_frame(&frame(&stream, 0, TouchpadFrameKind::Update))
                .is_err()
        );
        assert_eq!(closes.load(Ordering::SeqCst), 1);
    }
}
