//! Explicit accepted-channel supervision. No listener, credentials or admission
//! synthesis. A trusted embedding host must supply an already admitted owner.
use capyio_linux_audio::{AuthenticatedPlaybackOwner, PlaybackMetrics, PlaybackOwnerEvent};
use capyio_runtime::RuntimeSnapshot;
use std::{
    io,
    sync::mpsc::{self, Receiver, SyncSender, TrySendError},
    time::Duration,
};

/// Fixed pacing prevents an idle control connection from busy-spinning. Media
/// receive is separately bounded to 10ms; control replies to at most one second.
const TICK: Duration = Duration::from_millis(2);

#[derive(Clone)]
pub struct PlaybackStopHandle(SyncSender<()>);
impl PlaybackStopHandle {
    /// Idempotent, nonblocking local cancellation. Dropping all handles also
    /// requests Stop, so a vanished lifecycle owner cannot leave playback alive.
    pub fn request_stop(&self) {
        match self.0.try_send(()) {
            Ok(()) | Err(TrySendError::Full(())) | Err(TrySendError::Disconnected(())) => {}
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaybackExit {
    LocalStop,
    LifecycleOwnerGone,
    PeerStop,
}

/// Runs on the calling lifecycle thread, never a detached background worker.
/// Only an actual authenticated/admitted owner can enter this API. No config or
/// peer message can manufacture its grant, lease, paired identity or Session.
/// Existing stdin/stdout LinuxNodeHost and the default CLI are unchanged.
pub struct LinuxPlaybackHost {
    owner: AuthenticatedPlaybackOwner,
    stop: Receiver<()>,
}
impl LinuxPlaybackHost {
    pub fn from_admitted_trusted_lab(
        owner: AuthenticatedPlaybackOwner,
    ) -> (Self, PlaybackStopHandle) {
        let (sender, stop) = mpsc::sync_channel(1);
        (Self { owner, stop }, PlaybackStopHandle(sender))
    }
    /// Sends the local offer then fairly polls authenticated control and media.
    /// No authorization renewal or reconnect. Every exit closes the dedicated
    /// Session and attempts bounded media cleanup before returning.
    pub fn run(&mut self) -> io::Result<PlaybackExit> {
        run_owned(&mut self.owner, &self.stop)
    }
    pub fn snapshot(&self) -> RuntimeSnapshot {
        self.owner.snapshot()
    }
    pub fn metrics(&self) -> Option<PlaybackMetrics> {
        self.owner.metrics()
    }
}

trait PlaybackDriver {
    fn offer(&mut self) -> io::Result<()>;
    fn poll(&mut self) -> io::Result<Option<PlaybackOwnerEvent>>;
    fn stop(&mut self) -> io::Result<()>;
}
impl PlaybackDriver for AuthenticatedPlaybackOwner {
    fn offer(&mut self) -> io::Result<()> {
        self.send_local_offer()
    }
    fn poll(&mut self) -> io::Result<Option<PlaybackOwnerEvent>> {
        self.poll()
    }
    fn stop(&mut self) -> io::Result<()> {
        self.stop()
    }
}
fn local_stop(stop: &Receiver<()>) -> Option<PlaybackExit> {
    match stop.try_recv() {
        Ok(()) => Some(PlaybackExit::LocalStop),
        Err(mpsc::TryRecvError::Disconnected) => Some(PlaybackExit::LifecycleOwnerGone),
        Err(mpsc::TryRecvError::Empty) => None,
    }
}
fn run_owned(driver: &mut impl PlaybackDriver, stop: &Receiver<()>) -> io::Result<PlaybackExit> {
    let result = (|| {
        if let Some(reason) = local_stop(stop) {
            return Ok(reason);
        }
        driver.offer()?;
        loop {
            if let Some(reason) = local_stop(stop) {
                return Ok(reason);
            }
            if driver.poll()? == Some(PlaybackOwnerEvent::Stopped) {
                return Ok(PlaybackExit::PeerStop);
            }
            match stop.recv_timeout(TICK) {
                Ok(()) => return Ok(PlaybackExit::LocalStop),
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Ok(PlaybackExit::LifecycleOwnerGone);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    })();
    // Cleanup failure is never reported as successful Stop. The real owner
    // revokes Session authority even when process cleanup fails.
    driver.stop()?;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct Driver {
        offered: usize,
        polls: usize,
        stops: usize,
        fail_offer: bool,
        fail_poll: bool,
        fail_stop: bool,
        cancel: Option<PlaybackStopHandle>,
    }
    impl PlaybackDriver for Driver {
        fn offer(&mut self) -> io::Result<()> {
            self.offered += 1;
            if self.fail_offer {
                Err(io::Error::other("offer failed"))
            } else {
                Ok(())
            }
        }
        fn poll(&mut self) -> io::Result<Option<PlaybackOwnerEvent>> {
            self.polls += 1;
            if let Some(stop) = &self.cancel {
                stop.request_stop();
                return Ok(None);
            }
            if self.fail_poll {
                return Err(io::Error::other("poll failed"));
            }
            Ok(if self.polls == 3 {
                Some(PlaybackOwnerEvent::Stopped)
            } else {
                None
            })
        }
        fn stop(&mut self) -> io::Result<()> {
            self.stops += 1;
            if self.fail_stop {
                Err(io::Error::other("cleanup failed"))
            } else {
                Ok(())
            }
        }
    }
    #[test]
    fn supervision_idle_steps_continue_until_peer_stop() {
        let (_sender, receiver) = mpsc::sync_channel(1);
        let mut driver = Driver::default();
        assert_eq!(
            run_owned(&mut driver, &receiver).unwrap(),
            PlaybackExit::PeerStop
        );
        assert_eq!((driver.offered, driver.polls, driver.stops), (1, 3, 1));
    }
    #[test]
    fn supervision_local_stop_and_owner_loss_prevent_offer() {
        for gone in [false, true] {
            let (sender, receiver) = mpsc::sync_channel(1);
            if !gone {
                sender.try_send(()).unwrap();
            }
            drop(sender);
            let mut driver = Driver::default();
            assert_eq!(
                run_owned(&mut driver, &receiver).unwrap(),
                if gone {
                    PlaybackExit::LifecycleOwnerGone
                } else {
                    PlaybackExit::LocalStop
                }
            );
            assert_eq!((driver.offered, driver.polls, driver.stops), (0, 0, 1));
        }
    }
    #[test]
    fn supervision_stop_after_poll_is_observed_without_another_step() {
        let (sender, receiver) = mpsc::sync_channel(1);
        let mut driver = Driver {
            cancel: Some(PlaybackStopHandle(sender)),
            ..Driver::default()
        };
        assert_eq!(
            run_owned(&mut driver, &receiver).unwrap(),
            PlaybackExit::LocalStop
        );
        assert_eq!((driver.offered, driver.polls, driver.stops), (1, 1, 1));
    }
    #[test]
    fn supervision_failures_always_cleanup_and_never_report_success() {
        for which in 0..3 {
            let (_sender, receiver) = mpsc::sync_channel(1);
            let mut driver = Driver {
                fail_offer: which == 0,
                fail_poll: which == 1,
                fail_stop: which == 2,
                ..Driver::default()
            };
            assert!(run_owned(&mut driver, &receiver).is_err());
            assert_eq!(driver.stops, 1);
        }
    }
    #[test]
    fn supervision_stop_handle_is_bounded_and_idempotent() {
        let (sender, receiver) = mpsc::sync_channel(1);
        let stop = PlaybackStopHandle(sender);
        for _ in 0..100 {
            stop.request_stop();
        }
        assert_eq!(receiver.try_recv(), Ok(()));
        assert_eq!(receiver.try_recv(), Err(mpsc::TryRecvError::Empty));
        drop(receiver);
        stop.request_stop();
    }
}
