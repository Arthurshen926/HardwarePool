//! Private opt-in launcher framing, not the default management protocol.
//!
//! The launcher must exclusively own trusted inherited stdin/stdout FIFO pipes.
//! Stdin is NOT inherently secure: inheritance and local launcher isolation are
//! prerequisites, not authentication supplied by this module. Duplicated file
//! descriptors share status flags with the originals; O_NONBLOCK therefore also
//! affects the original descriptors. Drop attempts to restore their original
//! flags. Other readers/writers and concurrent flag changes are unsupported.
//! Buffers are overwritten on error/drop, but this is best-effort Rust fill,
//! not a guarantee against compiler elision, copies, swap, or process inspection.

use std::io;
use std::os::fd::{AsFd, OwnedFd};
use std::time::{Duration, Instant};

use rustix::fs::{FileType, OFlags, fcntl_getfl, fcntl_setfl, fstat};

const MAX_FRAME: usize = 65_536;
const RETRY_INTERVAL: Duration = Duration::from_millis(2);
const PARTIAL_FRAME_TIMEOUT: Duration = Duration::from_secs(1);

pub(crate) struct SecretBytes(Vec<u8>);

impl SecretBytes {
    pub(crate) fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

impl Drop for SecretBytes {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

struct NonblockingPipe {
    fd: OwnedFd,
    original_flags: OFlags,
}

impl NonblockingPipe {
    fn duplicate(fd: impl AsFd) -> io::Result<Self> {
        let fd = rustix::io::fcntl_dupfd_cloexec(fd, 3)
            .map_err(|_| fail(io::ErrorKind::Other, "launcher pipe duplication failed"))?;
        let stat = fstat(&fd)
            .map_err(|_| fail(io::ErrorKind::Other, "launcher pipe inspection failed"))?;
        if FileType::from_raw_mode(stat.st_mode) != FileType::Fifo {
            return Err(fail(
                io::ErrorKind::InvalidInput,
                "launcher requires inherited FIFO pipes",
            ));
        }
        let original_flags = fcntl_getfl(&fd)
            .map_err(|_| fail(io::ErrorKind::Other, "launcher pipe flags unavailable"))?;
        Ok(Self { fd, original_flags })
    }

    fn enable_nonblocking(&self) -> io::Result<()> {
        fcntl_setfl(&self.fd, self.original_flags | OFlags::NONBLOCK)
            .map_err(|_| fail(io::ErrorKind::Other, "launcher nonblocking setup failed"))
    }
}

impl Drop for NonblockingPipe {
    fn drop(&mut self) {
        let _ = fcntl_setfl(&self.fd, self.original_flags);
    }
}

#[derive(Default)]
struct FrameReader {
    prefix: [u8; 4],
    prefix_used: usize,
    body: Option<SecretBytes>,
    body_used: usize,
    limit: Option<usize>,
    started_at: Option<Instant>,
    terminal: bool,
}

impl FrameReader {
    fn clear(&mut self) {
        self.prefix.fill(0);
        self.prefix_used = 0;
        self.body = None;
        self.body_used = 0;
        self.limit = None;
        self.started_at = None;
    }

    fn abort(&mut self) {
        self.clear();
        self.terminal = true;
    }

    // At most two read calls per poll, neither consuming bytes of the next frame.
    fn poll(
        &mut self,
        max: usize,
        read: impl FnMut(&mut [u8]) -> io::Result<usize>,
    ) -> io::Result<Option<SecretBytes>> {
        let now = Instant::now();
        let previous_start = self.started_at;
        let result = self.poll_at(max, now, read);
        // Check again after the bounded read step, including scheduler delays.
        // Completion resets reader state, so retain the previous start (or the
        // current poll's start for a frame completed in this single step).
        let started = previous_start
            .or(self.started_at)
            .or_else(|| result.as_ref().is_ok_and(Option::is_some).then_some(now));
        if result.is_ok()
            && started.is_some_and(|started| started.elapsed() >= PARTIAL_FRAME_TIMEOUT)
        {
            self.abort();
            return Err(fail(
                io::ErrorKind::TimedOut,
                "launcher partial frame deadline expired",
            ));
        }
        result
    }

    fn poll_at(
        &mut self,
        max: usize,
        now: Instant,
        mut read: impl FnMut(&mut [u8]) -> io::Result<usize>,
    ) -> io::Result<Option<SecretBytes>> {
        let result = self.poll_inner(max, now, &mut read);
        if result.is_err() {
            self.abort();
        }
        result
    }

    fn poll_inner(
        &mut self,
        max: usize,
        now: Instant,
        read: &mut impl FnMut(&mut [u8]) -> io::Result<usize>,
    ) -> io::Result<Option<SecretBytes>> {
        if self.terminal {
            return Err(fail(
                io::ErrorKind::BrokenPipe,
                "launcher input is terminal",
            ));
        }
        if max == 0 || max > MAX_FRAME || self.limit.is_some_and(|limit| limit != max) {
            return Err(fail(
                io::ErrorKind::InvalidInput,
                "invalid launcher frame bound",
            ));
        }
        if self
            .started_at
            .is_some_and(|started| now.saturating_duration_since(started) >= PARTIAL_FRAME_TIMEOUT)
        {
            return Err(fail(
                io::ErrorKind::TimedOut,
                "launcher partial frame deadline expired",
            ));
        }
        self.limit = Some(max);
        if self.prefix_used < 4 {
            let received = read_some(read, &mut self.prefix[self.prefix_used..])?;
            if received != 0 && self.started_at.is_none() {
                self.started_at = Some(now);
            }
            self.prefix_used += received;
            if self.prefix_used < 4 {
                return Ok(None);
            }
        }
        if self.body.is_none() {
            let length = u32::from_be_bytes(self.prefix) as usize;
            if length == 0 || length > max {
                return Err(fail(
                    io::ErrorKind::InvalidData,
                    "invalid launcher frame length",
                ));
            }
            self.body = Some(SecretBytes(vec![0; length]));
        }
        let body = self.body.as_mut().expect("body initialized");
        self.body_used += read_some(read, &mut body.0[self.body_used..])?;
        if self.body_used != body.0.len() {
            return Ok(None);
        }
        let body = self.body.take();
        self.clear();
        Ok(body)
    }
}

impl Drop for FrameReader {
    fn drop(&mut self) {
        self.clear();
    }
}

fn read_some(
    read: &mut impl FnMut(&mut [u8]) -> io::Result<usize>,
    bytes: &mut [u8],
) -> io::Result<usize> {
    match read(bytes) {
        Ok(0) => Err(fail(io::ErrorKind::UnexpectedEof, "launcher input closed")),
        Ok(n) if n <= bytes.len() => Ok(n),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
            ) =>
        {
            Ok(0)
        }
        _ => Err(fail(io::ErrorKind::Other, "launcher input failed")),
    }
}

pub(crate) struct LauncherIo {
    input: NonblockingPipe,
    output: NonblockingPipe,
    reader: FrameReader,
    terminal: bool,
}

impl LauncherIo {
    pub(crate) fn stdio() -> io::Result<Self> {
        // Capture both originals before modifying either, including when the
        // inherited descriptors unexpectedly share an open file description.
        let input = NonblockingPipe::duplicate(io::stdin())?;
        let output = NonblockingPipe::duplicate(io::stdout())?;
        input.enable_nonblocking()?;
        output.enable_nonblocking()?;
        Ok(Self {
            input,
            output,
            reader: FrameReader::default(),
            terminal: false,
        })
    }

    /// The deadline bounds the entire frame, not each fragment. Failure poisons
    /// the channel and erases partial input; retrying a partial frame is forbidden.
    pub(crate) fn read_frame(&mut self, max: usize, deadline: Instant) -> io::Result<SecretBytes> {
        loop {
            if Instant::now() >= deadline {
                self.abort();
                return Err(fail(
                    io::ErrorKind::TimedOut,
                    "launcher input deadline expired",
                ));
            }
            if let Some(frame) = self.poll_frame(max)? {
                if Instant::now() >= deadline {
                    self.abort();
                    return Err(fail(
                        io::ErrorKind::TimedOut,
                        "launcher input deadline expired",
                    ));
                }
                return Ok(frame);
            }
            pause(deadline);
        }
    }

    /// Bounded nonblocking progress with a nonrenewable one-second frame timer
    /// starting at the first prefix byte. True idle has no frame timer. Callers
    /// remain responsible for any shorter deadline and overall phase lifetime.
    pub(crate) fn poll_frame(&mut self, max: usize) -> io::Result<Option<SecretBytes>> {
        if self.terminal {
            return Err(fail(
                io::ErrorKind::BrokenPipe,
                "launcher channel is terminal",
            ));
        }
        let input = &self.input.fd;
        let result = self.reader.poll(max, |bytes| {
            rustix::io::read(input, bytes).map_err(io::Error::from)
        });
        if result.is_err() {
            self.abort();
        }
        result
    }

    pub(crate) fn send_json(
        &mut self,
        value: &serde_json::Value,
        deadline: Instant,
    ) -> io::Result<()> {
        if self.terminal {
            return Err(fail(
                io::ErrorKind::BrokenPipe,
                "launcher channel is terminal",
            ));
        }
        let result = (|| {
            let frame = encode_json(value)?;
            write_frame(frame.as_slice(), deadline, |bytes| {
                rustix::io::write(&self.output.fd, bytes).map_err(io::Error::from)
            })
        })();
        if result.is_err() {
            self.abort();
        }
        result
    }

    fn abort(&mut self) {
        self.terminal = true;
        self.reader.abort();
    }
}

fn encode_json(value: &serde_json::Value) -> io::Result<SecretBytes> {
    // Fixed allocation prevents reallocation from leaving old serialized copies.
    let mut bytes = SecretBytes(vec![0; MAX_FRAME + 4]);
    let mut writer = io::Cursor::new(&mut bytes.0[4..]);
    serde_json::to_writer(&mut writer, value).map_err(|_| {
        fail(
            io::ErrorKind::InvalidData,
            "launcher output encoding failed",
        )
    })?;
    let length = writer.position() as usize;
    bytes.0[..4].copy_from_slice(&(length as u32).to_be_bytes());
    // Truncated tail is already zero; capacity never contained other data.
    bytes.0.truncate(length + 4);
    Ok(bytes)
}

fn write_frame(
    bytes: &[u8],
    deadline: Instant,
    mut write: impl FnMut(&[u8]) -> io::Result<usize>,
) -> io::Result<()> {
    let mut offset = 0;
    while offset < bytes.len() {
        if Instant::now() >= deadline {
            return Err(fail(
                io::ErrorKind::TimedOut,
                "launcher output deadline expired",
            ));
        }
        match write(&bytes[offset..]) {
            Ok(0) => return Err(fail(io::ErrorKind::WriteZero, "launcher output closed")),
            Ok(n) if n <= bytes.len() - offset => offset += n,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                pause(deadline)
            }
            _ => return Err(fail(io::ErrorKind::Other, "launcher output failed")),
        }
    }
    if Instant::now() >= deadline {
        return Err(fail(
            io::ErrorKind::TimedOut,
            "launcher output deadline expired",
        ));
    }
    Ok(())
}

fn pause(deadline: Instant) {
    std::thread::sleep(RETRY_INTERVAL.min(deadline.saturating_duration_since(Instant::now())));
}

fn fail(kind: io::ErrorKind, message: &'static str) -> io::Error {
    io::Error::new(kind, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn fragmented_frames_do_not_overread() {
        let mut input = io::Cursor::new(b"\0\0\0\x03abc\0\0\0\x01z");
        let mut reader = FrameReader::default();
        let mut first = None;
        for _ in 0..7 {
            first = reader
                .poll(8, |bytes| input.read(&mut bytes[..1]))
                .unwrap()
                .or(first);
            if first.is_some() {
                break;
            }
        }
        assert_eq!(first.unwrap().as_slice(), b"abc");
        assert_eq!(input.position(), 7);
        assert_eq!(
            reader
                .poll(8, |bytes| input.read(bytes))
                .unwrap()
                .unwrap()
                .as_slice(),
            b"z"
        );
    }

    #[test]
    fn oversized_zero_and_eof_are_terminal_and_clear() {
        for bytes in [&b"\0\0\0\x09"[..], &b"\0\0\0\0"[..], &b"\0\0\0\x03a"[..]] {
            let mut reader = FrameReader::default();
            let mut input = io::Cursor::new(bytes);
            while reader.poll(8, |bytes| input.read(bytes)).is_ok() {}
            assert!(reader.terminal);
            assert_eq!(reader.prefix, [0; 4]);
            assert!(reader.body.is_none());
            assert!(
                reader
                    .poll(8, |_| panic!("terminal reader performed I/O"))
                    .is_err()
            );
        }
    }

    #[test]
    fn nonblocking_and_interrupted_preserve_partial_state() {
        let mut reader = FrameReader::default();
        assert!(
            reader
                .poll(8, |_| Err(io::ErrorKind::WouldBlock.into()))
                .unwrap()
                .is_none()
        );
        assert!(
            reader
                .poll(8, |_| Err(io::ErrorKind::Interrupted.into()))
                .unwrap()
                .is_none()
        );
        assert!(
            reader
                .poll(9, |_| panic!("changed bound performed I/O"))
                .is_err()
        );
        assert!(reader.terminal);
    }

    #[test]
    fn partial_frame_timer_is_nonrenewable_but_idle_has_no_timer() {
        let mut reader = FrameReader::default();
        let start = Instant::now();
        for now in [start, start + Duration::from_secs(20)] {
            assert!(
                reader
                    .poll_at(8, now, |_| Err(io::ErrorKind::WouldBlock.into()))
                    .unwrap()
                    .is_none()
            );
            assert!(reader.started_at.is_none());
        }
        let start = start + Duration::from_secs(21);
        assert!(
            reader
                .poll_at(8, start, |bytes| {
                    bytes[0] = 0;
                    Ok(1)
                })
                .unwrap()
                .is_none()
        );
        assert_eq!(reader.started_at, Some(start));
        assert!(
            reader
                .poll_at(8, start + Duration::from_millis(999), |bytes| {
                    bytes[0] = 0;
                    Ok(1)
                })
                .unwrap()
                .is_none()
        );
        assert_eq!(reader.started_at, Some(start));
        let error = reader
            .poll_at(8, start + PARTIAL_FRAME_TIMEOUT, |_| {
                panic!("expired input performed I/O")
            })
            .err()
            .unwrap();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(reader.terminal);
        assert_eq!(reader.prefix, [0; 4]);
        assert!(reader.started_at.is_none());
    }

    #[test]
    fn partial_body_is_erased_on_read_error() {
        let mut reader = FrameReader::default();
        let mut input = io::Cursor::new(b"\0\0\0\x04key");
        assert!(reader.poll(8, |bytes| input.read(bytes)).unwrap().is_none());
        assert_eq!(reader.body.as_ref().unwrap().as_slice(), b"key\0");
        let error = reader.poll(8, |_| Err(io::Error::other("private bytes")));
        assert_eq!(error.err().unwrap().to_string(), "launcher input failed");
        assert!(reader.body.is_none());
        assert_eq!(reader.body_used, 0);
        assert_eq!(reader.prefix, [0; 4]);
    }

    #[test]
    fn write_backpressure_and_trickle_cannot_extend_deadline() {
        let deadline = Instant::now() + Duration::from_millis(5);
        let error =
            write_frame(b"x", deadline, |_| Err(io::ErrorKind::WouldBlock.into())).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        let deadline = Instant::now() + Duration::from_millis(5);
        let mut calls = 0;
        let error = write_frame(&[0; 100], deadline, |_| {
            calls += 1;
            std::thread::sleep(Duration::from_millis(2));
            Ok(1)
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(calls < 100);
    }

    #[test]
    fn serialization_is_length_prefixed_and_bounded() {
        let frame = encode_json(&serde_json::json!({"ok": true})).unwrap();
        assert_eq!(
            u32::from_be_bytes(frame.as_slice()[..4].try_into().unwrap()) as usize,
            frame.as_slice().len() - 4
        );
        assert!(encode_json(&serde_json::Value::String("x".repeat(MAX_FRAME))).is_err());
    }

    #[test]
    fn writes_are_bounded_and_errors_sanitized() {
        let deadline = Instant::now() + Duration::from_secs(1);
        let mut result = Vec::new();
        write_frame(b"abc", deadline, |bytes| {
            result.push(bytes[0]);
            Ok(1)
        })
        .unwrap();
        assert_eq!(result, b"abc");
        let error = write_frame(b"x", Instant::now(), |_| {
            panic!("expired write performed I/O")
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        let error =
            write_frame(b"x", deadline, |_| Err(io::Error::other("private bytes"))).unwrap_err();
        assert_eq!(error.to_string(), "launcher output failed");
    }
}
