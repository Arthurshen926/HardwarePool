use crate::{Frame, Projection, SubmitOutcome, error};
use std::{
    fs,
    io::{self, Write},
    os::unix::fs::{MetadataExt, PermissionsExt},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub(crate) struct NativeProjection {
    child: Child,
    node_name: String,
    sender: Option<SyncSender<Frame>>,
    writer: Option<JoinHandle<()>>,
    failed: Arc<AtomicBool>,
    stopped: bool,
    reaped: bool,
}
impl NativeProjection {
    pub fn spawn(lifetime: Duration, deadline_ms: u64) -> io::Result<Self> {
        let executable = std::env::current_exe()?;
        let parent = executable
            .parent()
            .ok_or_else(|| error("executable directory unavailable"))?;
        let helper = parent.join("capyio-linux-camera-helper");
        let metadata = fs::symlink_metadata(&helper)?;
        let owner = fs::metadata(&executable)?;
        if !metadata.file_type().is_file()
            || metadata.permissions().mode() & 0o022 != 0
            || metadata.uid() != owner.uid()
        {
            return Err(error(
                "camera helper must be an owned non-writable regular sibling binary",
            ));
        }
        // A fresh typed UUID, not user input, uniquely names only this owned node.
        let token = capyio_core::StreamId::new().to_string();
        let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR")
            .ok_or_else(|| error("PipeWire user runtime directory missing"))?;
        let remote = std::env::var("PIPEWIRE_REMOTE").unwrap_or_else(|_| "pipewire-0".into());
        if remote.is_empty()
            || remote.len() > 96
            || !remote
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        {
            return Err(error(
                "PipeWire remote must be a local socket name, not a path",
            ));
        }
        let mut child = Command::new(helper)
            .env_clear()
            .env("XDG_RUNTIME_DIR", runtime_dir)
            .env("PIPEWIRE_REMOTE", remote)
            .args([
                "--token",
                &token,
                "--lifetime-ms",
                &lifetime.as_millis().to_string(),
                "--deadline-unix-ms",
                &deadline_ms.to_string(),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| error("private helper pipe unavailable"))?;
        let (sender, receiver) = mpsc::sync_channel::<Frame>(1);
        let failed = Arc::new(AtomicBool::new(false));
        let writer_failed = Arc::clone(&failed);
        let writer = match thread::Builder::new()
            .name("capyio-camera-ipc".into())
            .spawn(move || {
                while let Ok(frame) = receiver.recv() {
                    let d = frame.descriptor;
                    let mut header = [0u8; 32];
                    header[..4].copy_from_slice(b"CPNV");
                    header[4..6].copy_from_slice(&1u16.to_le_bytes());
                    header[6..8].copy_from_slice(&32u16.to_le_bytes());
                    header[8..16].copy_from_slice(&d.sequence.to_le_bytes());
                    header[16..24].copy_from_slice(&d.source_timestamp_nanos.to_le_bytes());
                    header[24..28].copy_from_slice(&(crate::FRAME_BYTES as u32).to_le_bytes());
                    header[28..32].copy_from_slice(&u32::from(d.flags.discontinuity).to_le_bytes());
                    // Only this non-real-time writer can block. Kill/reap closes the
                    // pipe before owner joins; helper also has independent deadlines.
                    if stdin
                        .write_all(&header)
                        .and_then(|()| stdin.write_all(&frame.payload))
                        .is_err()
                    {
                        writer_failed.store(true, Ordering::Release);
                        break;
                    }
                }
            }) {
            Ok(writer) => writer,
            Err(reason) => {
                let _ = reap_until(&mut child, Duration::from_secs(1));
                return Err(reason);
            }
        };
        Ok(Self {
            child,
            node_name: format!("capyio.camera.{token}"),
            sender: Some(sender),
            writer: Some(writer),
            failed,
            stopped: false,
            reaped: false,
        })
    }
}
impl Projection for NativeProjection {
    fn submit(&mut self, frame: Frame) -> io::Result<SubmitOutcome> {
        self.check()?;
        match self
            .sender
            .as_ref()
            .ok_or_else(|| error("camera stopped"))?
            .try_send(frame)
        {
            Ok(()) => Ok(SubmitOutcome::Queued),
            Err(TrySendError::Full(_)) => Ok(SubmitOutcome::DroppedBackpressure),
            Err(TrySendError::Disconnected(_)) => Err(error("camera writer disconnected")),
        }
    }
    fn check(&mut self) -> io::Result<()> {
        if self.stopped || self.failed.load(Ordering::Acquire) || self.child.try_wait()?.is_some() {
            return Err(error("camera helper exited or private pipe failed"));
        }
        Ok(())
    }
    fn node_name(&self) -> Option<&str> {
        Some(&self.node_name)
    }
    fn pid(&self) -> Option<u32> {
        (!self.stopped).then_some(self.child.id())
    }
    fn stop(&mut self) -> io::Result<()> {
        self.stopped = true;
        self.sender.take();
        if !self.reaped {
            // Finite retryable cleanup; never use a blocking wait on a child.
            reap_until(&mut self.child, Duration::from_secs(1))?;
            self.reaped = true;
        }
        if let Some(writer) = self.writer.as_ref() {
            let deadline = Instant::now() + Duration::from_secs(1);
            while !writer.is_finished() {
                if Instant::now() >= deadline {
                    return Err(error("camera writer cleanup deadline"));
                }
                thread::sleep(Duration::from_millis(1));
            }
        }
        if let Some(writer) = self.writer.take() {
            // Joining an already-finished thread cannot await pipe progress.
            writer.join().map_err(|_| error("camera writer panicked"))?;
        }
        Ok(())
    }
}
impl Drop for NativeProjection {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn reap_until(child: &mut Child, budget: Duration) -> io::Result<()> {
    if child.try_wait()?.is_some() {
        return Ok(());
    }
    child.kill()?;
    let deadline = Instant::now() + budget;
    loop {
        if child.try_wait()?.is_some() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(error("camera child reap deadline"));
        }
        thread::sleep(Duration::from_millis(1));
    }
}
