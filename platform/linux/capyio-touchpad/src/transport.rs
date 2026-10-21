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
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
const IO_BUDGET: Duration = Duration::from_millis(250);
fn error(value: impl ToString) -> io::Error {
    io::Error::other(value.to_string())
}

fn verify_helper(executable: &Path, helper: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(helper)?;
    let owner = fs::metadata(executable)?;
    if !metadata.file_type().is_file()
        || metadata.permissions().mode() & 0o022 != 0
        || metadata.uid() != owner.uid()
    {
        return Err(error(
            "touchpad helper must be an owned non-writable regular sibling binary",
        ));
    }
    Ok(())
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
/// Deterministic software-only fault modes. Faults target batch three, after
/// the initial CancelAll and one complete contact snapshot have committed.
#[cfg(feature = "test-fixture")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TouchpadFixtureMode {
    Normal,
    FailCreate,
    PartialWrite,
    DelayedCommit,
    Backpressure,
}
#[cfg(feature = "test-fixture")]
impl TouchpadFixtureMode {
    fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::FailCreate => "fail-create",
            Self::PartialWrite => "partial-write",
            Self::DelayedCommit => "delayed-commit",
            Self::Backpressure => "backpressure",
        }
    }
}
/// A caller-owned trace descriptor, never a helper path or native device path.
/// The fixture makes trace writes nonblocking and fails closed if a pipe cannot
/// keep up. The caller must drain a pipe concurrently to retain complete evidence.
/// The trace contains only synthetic fixture events, never credentials.
#[cfg(feature = "test-fixture")]
pub struct TouchpadFixtureConfig {
    pub trace: fs::File,
    pub mode: TouchpadFixtureMode,
}
enum HelperTarget {
    #[cfg(feature = "native-uinput")]
    Native,
    #[cfg(feature = "test-fixture")]
    Fixture(TouchpadFixtureConfig),
}
impl HelperTarget {
    fn sibling_name(&self) -> &'static str {
        match self {
            #[cfg(feature = "native-uinput")]
            Self::Native => "capyio-linux-touchpad-helper",
            #[cfg(feature = "test-fixture")]
            Self::Fixture(_) => "capyio-linux-touchpad-fixture-helper",
        }
    }
    fn configure(self, command: &mut Command) {
        match self {
            #[cfg(feature = "native-uinput")]
            Self::Native => {
                command.stderr(Stdio::null());
            }
            #[cfg(feature = "test-fixture")]
            Self::Fixture(fixture) => {
                command.env("CAPYIO_FAKE_MODE", fixture.mode.name());
                command.stderr(Stdio::from(fixture.trace));
            }
        }
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
    #[cfg(feature = "native-uinput")]
    pub(crate) fn spawn(deadline_ms: u64) -> io::Result<Self> {
        Self::spawn_target(deadline_ms, HelperTarget::Native)
    }
    #[cfg(feature = "test-fixture")]
    pub(crate) fn spawn_fixture(
        deadline_ms: u64,
        fixture: TouchpadFixtureConfig,
    ) -> io::Result<Self> {
        Self::spawn_target(deadline_ms, HelperTarget::Fixture(fixture))
    }
    fn spawn_target(deadline_ms: u64, target: HelperTarget) -> io::Result<Self> {
        let executable = std::env::current_exe()?;
        let helper = executable
            .parent()
            .ok_or_else(|| error("executable directory absent"))?
            .join(target.sibling_name());
        verify_helper(&executable, &helper)?;
        let remaining = deadline_ms
            .checked_sub(wall_ms()?)
            .filter(|v| *v > 0 && *v <= 300_000)
            .ok_or_else(|| error("touchpad helper deadline invalid"))?;
        let (input, child_input) = UnixStream::pair()?;
        let (output, child_output) = UnixStream::pair()?;
        input.set_write_timeout(Some(IO_BUDGET))?;
        output.set_read_timeout(Some(IO_BUDGET))?;
        let mut command = Command::new(helper);
        command
            .env_clear()
            .stdin(Stdio::from(OwnedFd::from(child_input)))
            .stdout(Stdio::from(OwnedFd::from(child_output)));
        target.configure(&mut command);
        let child = command.spawn()?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Directory(std::path::PathBuf);
    impl Directory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "capyio-touchpad-sibling-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn file(&self, name: &str, mode: u32) -> std::path::PathBuf {
            let path = self.0.join(name);
            fs::write(&path, b"never executed").unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
            path
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn sibling_checks_reject_missing_symlink_directory_and_writable_binary() {
        let directory = Directory::new();
        let owner = directory.file("owner", 0o700);
        assert!(verify_helper(&owner, &directory.0.join("missing")).is_err());
        assert!(verify_helper(&owner, &directory.0).is_err());
        let helper = directory.file("owned", 0o700);
        assert!(verify_helper(&owner, &helper).is_ok());
        let link = directory.0.join("symlink");
        std::os::unix::fs::symlink(&helper, &link).unwrap();
        assert!(verify_helper(&owner, &link).is_err());
        for mode in [0o720, 0o702, 0o777] {
            fs::set_permissions(&helper, fs::Permissions::from_mode(mode)).unwrap();
            assert!(verify_helper(&owner, &helper).is_err());
        }
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(verify_helper(&owner, &helper).is_ok());
    }
    #[test]
    fn targets_keep_native_and_fixture_names_distinct() {
        #[cfg(feature = "native-uinput")]
        assert_eq!(
            HelperTarget::Native.sibling_name(),
            "capyio-linux-touchpad-helper"
        );
        #[cfg(feature = "test-fixture")]
        {
            let directory = Directory::new();
            let path = directory.file("trace", 0o600);
            let target = HelperTarget::Fixture(TouchpadFixtureConfig {
                trace: fs::File::create(path).unwrap(),
                mode: TouchpadFixtureMode::Normal,
            });
            assert_eq!(
                target.sibling_name(),
                "capyio-linux-touchpad-fixture-helper"
            );
            assert_ne!(target.sibling_name(), "capyio-linux-touchpad-helper");
        }
    }
}
