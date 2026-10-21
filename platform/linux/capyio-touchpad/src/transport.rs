//! Private bounded helper IPC. This synchronous transport belongs exclusively
//! to the receive worker, never the Runtime owner's poll loop.
use crate::session::wall_ms;
use crate::{EventBatch, TouchpadEventTransport};
use std::{
    fs,
    io::{self, Read, Write},
    net::Shutdown,
    os::{
        fd::OwnedFd,
        unix::{
            fs::{MetadataExt, PermissionsExt},
            net::UnixStream,
        },
    },
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
const IO_BUDGET: Duration = Duration::from_millis(250);
fn error(value: impl ToString) -> io::Error {
    io::Error::other(value.to_string())
}

fn write_bounded(stream: &mut UnixStream, mut bytes: &[u8]) -> io::Result<()> {
    let deadline = Instant::now() + IO_BUDGET;
    while !bytes.is_empty() {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .filter(|v| !v.is_zero())
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::TimedOut, "touchpad IPC write deadline")
            })?;
        stream.set_write_timeout(Some(remaining))?;
        match stream.write(bytes) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "touchpad IPC closed",
                ));
            }
            Ok(count) => bytes = &bytes[count..],
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// Shutdown only, safe to invoke before joining a blocked receive worker.
/// EOF makes the helper release and destroy. It does not promise graceful
/// events after SIGKILL; descriptor closure is the final backstop.
pub struct TouchpadHelperStopHandle {
    input: UnixStream,
    output: UnixStream,
}
impl TouchpadHelperStopHandle {
    pub fn stop(&self) {
        let _ = self.input.shutdown(Shutdown::Both);
        let _ = self.output.shutdown(Shutdown::Both);
    }
}
pub struct OwnedTouchpadHelper {
    child: Child,
    input: UnixStream,
    output: UnixStream,
    stopped: bool,
    reaped: bool,
    failed: bool,
}
impl OwnedTouchpadHelper {
    pub(crate) fn spawn(deadline_ms: u64) -> io::Result<Self> {
        let executable = std::env::current_exe()?;
        let helper = executable
            .parent()
            .ok_or_else(|| error("executable directory absent"))?
            .join("capyio-linux-touchpad-helper");
        let metadata = fs::symlink_metadata(&helper)?;
        let owner = fs::metadata(&executable)?;
        if !metadata.file_type().is_file()
            || metadata.permissions().mode() & 0o022 != 0
            || metadata.uid() != owner.uid()
        {
            return Err(error(
                "touchpad helper must be an owned non-writable regular sibling binary",
            ));
        }
        let remaining = deadline_ms
            .checked_sub(wall_ms()?)
            .filter(|v| *v > 0 && *v <= 300_000)
            .ok_or_else(|| error("touchpad helper deadline invalid"))?;
        let (input, child_input) = UnixStream::pair()?;
        let (output, child_output) = UnixStream::pair()?;
        input.set_write_timeout(Some(IO_BUDGET))?;
        output.set_read_timeout(Some(IO_BUDGET))?;
        let child = Command::new(helper)
            .env_clear()
            .stdin(Stdio::from(OwnedFd::from(child_input)))
            .stdout(Stdio::from(OwnedFd::from(child_output)))
            .stderr(Stdio::null())
            .spawn()?;
        let mut owned = Self {
            child,
            input,
            output,
            stopped: false,
            reaped: false,
            failed: false,
        };
        let mut header = [0; 24];
        header[..4].copy_from_slice(b"HPT1");
        header[4..8].copy_from_slice(&(remaining as u32).to_le_bytes());
        header[8..12].copy_from_slice(&2_000u32.to_le_bytes());
        header[16..24].copy_from_slice(&deadline_ms.to_le_bytes());
        if let Err(e) = write_bounded(&mut owned.input, &header).and_then(|()| owned.ack()) {
            owned.failed = true;
            let _ = owned.close();
            return Err(e);
        }
        Ok(owned)
    }
    pub fn stop_handle(&self) -> io::Result<TouchpadHelperStopHandle> {
        Ok(TouchpadHelperStopHandle {
            input: self.input.try_clone()?,
            output: self.output.try_clone()?,
        })
    }
    fn ack(&mut self) -> io::Result<()> {
        let mut ack = [0; 1];
        self.output.read_exact(&mut ack)?;
        if ack != [1] {
            return Err(error("touchpad helper rejected record"));
        }
        Ok(())
    }
    fn reap(&mut self) -> io::Result<()> {
        let deadline = Instant::now() + Duration::from_millis(500);
        loop {
            if let Some(status) = self.child.try_wait()? {
                self.reaped = true;
                return if status.success() {
                    Ok(())
                } else {
                    Err(error("touchpad helper exited unsuccessfully"))
                };
            }
            if Instant::now() >= deadline {
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
        // Only our child. Hard-kill closes the uinput fd but cannot guarantee an
        // explicit release event. Surface that loss of graceful cleanup.
        self.child.kill()?;
        let deadline = Instant::now() + Duration::from_millis(500);
        while self.child.try_wait()?.is_none() {
            if Instant::now() >= deadline {
                return Err(error("touchpad helper reap deadline"));
            }
            thread::sleep(Duration::from_millis(1));
        }
        self.reaped = true;
        Err(error(
            "touchpad helper required hard kill; explicit release events unconfirmed",
        ))
    }
}
impl crate::session::TouchpadTransportStop for TouchpadHelperStopHandle {
    fn stop(&self) {
        Self::stop(self);
    }
}
impl TouchpadEventTransport for OwnedTouchpadHelper {
    fn stop_handle(
        &self,
    ) -> io::Result<Option<std::sync::Arc<dyn crate::session::TouchpadTransportStop>>> {
        Ok(Some(std::sync::Arc::new(Self::stop_handle(self)?)))
    }
    fn submit(&mut self, events: &EventBatch) -> io::Result<()> {
        if self.stopped || self.failed {
            return Err(error("touchpad helper inactive"));
        }
        let events = events.as_slice();
        if events.is_empty() || events.len() > 64 {
            return Err(error("invalid touchpad batch length"));
        }
        let mut record = [0u8; 8 + 64 * 8];
        record[..4].copy_from_slice(b"HPF1");
        record[4..6].copy_from_slice(&(events.len() as u16).to_le_bytes());
        for (event, bytes) in events.iter().zip(record[8..].chunks_exact_mut(8)) {
            bytes[..2].copy_from_slice(&event.event_type.to_le_bytes());
            bytes[2..4].copy_from_slice(&event.code.to_le_bytes());
            bytes[4..8].copy_from_slice(&event.value.to_le_bytes());
        }
        let result = write_bounded(&mut self.input, &record[..8 + events.len() * 8])
            .and_then(|()| self.ack());
        if result.is_err() {
            self.failed = true;
            let _ = self.input.shutdown(Shutdown::Both);
            let _ = self.output.shutdown(Shutdown::Both);
        }
        result
    }
    fn close(&mut self) -> io::Result<()> {
        if self.stopped {
            return if self.reaped { Ok(()) } else { self.reap() };
        }
        self.stopped = true;
        let graceful = if self.failed {
            Err(error("touchpad helper channel failed"))
        } else {
            write_bounded(&mut self.input, b"HPF1\0\0\0\0").and_then(|()| self.ack())
        };
        let _ = self.input.shutdown(Shutdown::Both);
        let _ = self.output.shutdown(Shutdown::Both);
        let reaped = self.reap();
        graceful.and(reaped)
    }
}
impl Drop for OwnedTouchpadHelper {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
