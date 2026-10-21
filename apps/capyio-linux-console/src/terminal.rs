//! Explicit controlling-terminal input, independent of redirected standard I/O.
//!
//! No input is echoed, including ordinary confirmation text. Control-C/Control-D
//! are bytes rather than signals while this guard is alive. Normal cancellation
//! and unwinding restore termios; SIGKILL, abort and externally terminating
//! signals cannot be repaired by a destructor. The caller owns signal policy.
//! Wiping is best effort, not protection against copies, swap or same-UID tracing.

use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::atomic::{Ordering, compiler_fence};
use std::time::Instant;

const MAX_LINE: usize = 256;
const MAX_WRITE: usize = 16_384;

/// Deliberately has no Debug or Clone implementation.
pub(crate) struct SecretLine(Vec<u8>);

impl SecretLine {
    pub(crate) fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub(crate) fn as_str(&self) -> io::Result<&str> {
        std::str::from_utf8(&self.0)
            .map_err(|_| failure(io::ErrorKind::InvalidData, "terminal input is not UTF-8"))
    }
}

impl Drop for SecretLine {
    fn drop(&mut self) {
        wipe(&mut self.0);
    }
}

pub(crate) fn wipe(bytes: &mut [u8]) {
    for byte in bytes {
        // SAFETY: Each pointer denotes one exclusively borrowed live byte.
        unsafe { std::ptr::write_volatile(byte, 0) };
    }
    compiler_fence(Ordering::SeqCst);
}

pub(crate) struct Terminal {
    fd: OwnedFd,
    saved: libc::termios,
    input: LineInput,
}

impl Terminal {
    pub(crate) fn open() -> io::Result<Self> {
        // SAFETY: A fixed NUL-terminated path; no create operation or mode arg.
        let raw = unsafe {
            libc::open(
                c"/dev/tty".as_ptr(),
                libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC | libc::O_NONBLOCK,
            )
        };
        if raw < 0 {
            return Err(failure(
                io::ErrorKind::NotConnected,
                "an interactive controlling terminal is required",
            ));
        }
        // SAFETY: open returned a fresh owned descriptor.
        let fd = unsafe { OwnedFd::from_raw_fd(raw) };
        verify_foreground(fd.as_raw_fd())?;
        // SAFETY: tcgetattr initializes the output on success, checked below.
        let mut saved = std::mem::MaybeUninit::<libc::termios>::uninit();
        if unsafe { libc::tcgetattr(fd.as_raw_fd(), saved.as_mut_ptr()) } != 0 {
            return Err(failure(
                io::ErrorKind::Other,
                "terminal settings unavailable",
            ));
        }
        // SAFETY: tcgetattr succeeded.
        let saved = unsafe { saved.assume_init() };
        let terminal = Self {
            fd,
            saved,
            input: LineInput::default(),
        };
        let mut raw = saved;
        // SAFETY: raw is an initialized termios object.
        unsafe { libc::cfmakeraw(&mut raw) };
        // Preserve readable newline output without restoring input processing,
        // echo, canonical buffering, flow control or terminal-generated signals.
        raw.c_oflag |= libc::OPOST | libc::ONLCR;
        raw.c_cc[libc::VMIN] = 1;
        raw.c_cc[libc::VTIME] = 0;
        // The guard already exists, so even a partial setup failure attempts
        // restoration. Never wait for draining terminal output.
        if unsafe { libc::tcsetattr(terminal.fd.as_raw_fd(), libc::TCSANOW, &raw) } != 0 {
            return Err(failure(io::ErrorKind::Other, "terminal setup failed"));
        }
        terminal.flush_input()?;
        Ok(terminal)
    }

    /// Bounded output to /dev/tty only, with one absolute operation deadline.
    pub(crate) fn write(&mut self, text: &str, deadline: Instant) -> io::Result<()> {
        if text.len() > MAX_WRITE {
            return Err(failure(
                io::ErrorKind::InvalidInput,
                "terminal output too long",
            ));
        }
        let mut remaining = text.as_bytes();
        while !remaining.is_empty() {
            crate::controller::check(deadline)?;
            if Instant::now() >= deadline {
                return Err(failure(
                    io::ErrorKind::TimedOut,
                    "terminal output timed out",
                ));
            }
            verify_foreground(self.fd.as_raw_fd())?;
            // SAFETY: remaining is a live readable slice; fd is owned/nonblocking.
            let count = unsafe {
                libc::write(
                    self.fd.as_raw_fd(),
                    remaining.as_ptr().cast(),
                    remaining.len(),
                )
            };
            if count > 0 {
                remaining = &remaining[count as usize..];
            } else if count == 0 {
                return Err(failure(io::ErrorKind::WriteZero, "terminal output closed"));
            } else {
                match io::Error::last_os_error().kind() {
                    io::ErrorKind::Interrupted => continue,
                    io::ErrorKind::WouldBlock => wait_writable(self.fd.as_raw_fd(), deadline)?,
                    _ => return Err(failure(io::ErrorKind::Other, "terminal output failed")),
                }
            }
        }
        crate::controller::check(deadline)?;
        if Instant::now() >= deadline {
            return Err(failure(
                io::ErrorKind::TimedOut,
                "terminal output timed out",
            ));
        }
        Ok(())
    }

    /// Discard both kernel-queued and partial application input for every prompt.
    /// A second flush excludes text typed while the new prompt was being written.
    pub(crate) fn begin_prompt(&mut self, text: &str, deadline: Instant) -> io::Result<()> {
        self.input.clear();
        self.flush_input()?;
        self.write(text, deadline)?;
        self.flush_input()?;
        self.input.armed = true;
        Ok(())
    }

    /// One nonblocking read of at most 256 bytes. Idle text is never remembered.
    /// Poll during idle waits too, so Control-C/Control-D can cancel promptly.
    pub(crate) fn poll_line(&mut self) -> io::Result<Option<SecretLine>> {
        verify_foreground(self.fd.as_raw_fd())?;
        self.input.reject_extra()?;
        let mut bytes = [0_u8; MAX_LINE];
        // SAFETY: bytes is a live writable slice and the fd is nonblocking.
        let count =
            unsafe { libc::read(self.fd.as_raw_fd(), bytes.as_mut_ptr().cast(), bytes.len()) };
        let result = if count > 0 {
            self.input.consume(&bytes[..count as usize])
        } else if count == 0 {
            Err(failure(
                io::ErrorKind::UnexpectedEof,
                "terminal input closed",
            ))
        } else {
            match io::Error::last_os_error().kind() {
                io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock => Ok(None),
                _ => Err(failure(io::ErrorKind::Other, "terminal input failed")),
            }
        };
        wipe(&mut bytes);
        if result.is_err() {
            self.input.clear();
        }
        result
    }

    fn flush_input(&self) -> io::Result<()> {
        verify_foreground(self.fd.as_raw_fd())?;
        // SAFETY: This is the explicitly owned controlling-terminal descriptor.
        if unsafe { libc::tcflush(self.fd.as_raw_fd(), libc::TCIFLUSH) } != 0 {
            return Err(failure(io::ErrorKind::Other, "terminal input flush failed"));
        }
        Ok(())
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        self.input.clear();
        // Do not leave raw input queued for the shell after normal cancellation.
        // If foreground ownership was lost, restoration must not stop this
        // process with SIGTTOU. Block only that signal on this thread for the
        // bounded restoration, then restore the original mask exactly.
        // SAFETY: fd/saved remain valid, sigemptyset initializes blocked, and a
        // successful pthread_sigmask initializes previous before it is read.
        unsafe {
            let mut blocked = std::mem::MaybeUninit::<libc::sigset_t>::uninit();
            let mut previous = std::mem::MaybeUninit::<libc::sigset_t>::uninit();
            if libc::sigemptyset(blocked.as_mut_ptr()) != 0 {
                return;
            }
            let mut blocked = blocked.assume_init();
            if libc::sigaddset(&mut blocked, libc::SIGTTOU) != 0 {
                return;
            }
            let masked =
                libc::pthread_sigmask(libc::SIG_BLOCK, &blocked, previous.as_mut_ptr()) == 0;
            if masked || verify_foreground(self.fd.as_raw_fd()).is_ok() {
                libc::tcflush(self.fd.as_raw_fd(), libc::TCIFLUSH);
                libc::tcsetattr(self.fd.as_raw_fd(), libc::TCSANOW, &self.saved);
            }
            if masked {
                libc::pthread_sigmask(libc::SIG_SETMASK, previous.as_ptr(), std::ptr::null_mut());
            }
        }
    }
}

fn verify_foreground(fd: libc::c_int) -> io::Result<()> {
    // SAFETY: These calls inspect the descriptor and the current process only.
    let (is_tty, terminal_group, terminal_session, group, session) = unsafe {
        (
            libc::isatty(fd),
            libc::tcgetpgrp(fd),
            libc::tcgetsid(fd),
            libc::getpgrp(),
            libc::getsid(0),
        )
    };
    if is_tty != 1
        || terminal_group <= 0
        || terminal_group != group
        || terminal_session <= 0
        || terminal_session != session
    {
        return Err(failure(
            io::ErrorKind::PermissionDenied,
            "the console must own the foreground controlling terminal",
        ));
    }
    Ok(())
}

fn wait_writable(fd: libc::c_int, deadline: Instant) -> io::Result<()> {
    crate::controller::check(deadline)?;
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(failure(
            io::ErrorKind::TimedOut,
            "terminal output timed out",
        ));
    }
    // Poll for at most 10 ms so foreground ownership and the deadline are
    // rechecked even if the terminal cannot drain. No unbounded syscall retry.
    let timeout = remaining.as_millis().clamp(1, 10) as libc::c_int;
    let mut descriptor = libc::pollfd {
        fd,
        events: libc::POLLOUT,
        revents: 0,
    };
    // SAFETY: descriptor is a valid single pollfd object.
    let result = unsafe { libc::poll(&mut descriptor, 1, timeout) };
    crate::controller::check(deadline)?;
    if result < 0 && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
        return Err(failure(io::ErrorKind::Other, "terminal output poll failed"));
    }
    if descriptor.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
        return Err(failure(io::ErrorKind::BrokenPipe, "terminal output closed"));
    }
    Ok(())
}

struct LineInput {
    bytes: [u8; MAX_LINE],
    used: usize,
    armed: bool,
    completed: bool,
    extra_input: bool,
}

impl Default for LineInput {
    fn default() -> Self {
        Self {
            bytes: [0; MAX_LINE],
            used: 0,
            armed: false,
            completed: false,
            extra_input: false,
        }
    }
}

impl LineInput {
    fn clear(&mut self) {
        wipe(&mut self.bytes);
        self.used = 0;
        self.armed = false;
        self.completed = false;
        self.extra_input = false;
    }

    fn reject_extra(&mut self) -> io::Result<()> {
        if self.extra_input {
            self.clear();
            return Err(failure(
                io::ErrorKind::InvalidData,
                "unexpected extra terminal input",
            ));
        }
        Ok(())
    }

    fn consume(&mut self, bytes: &[u8]) -> io::Result<Option<SecretLine>> {
        let result = self.consume_inner(bytes);
        if result.is_err() {
            self.clear();
        }
        result
    }

    fn consume_inner(&mut self, bytes: &[u8]) -> io::Result<Option<SecretLine>> {
        self.reject_extra()?;
        // Cancellation wins over a newline in the same bounded read.
        if bytes.contains(&3) || bytes.contains(&4) {
            return Err(failure(
                io::ErrorKind::Interrupted,
                "terminal input cancelled",
            ));
        }
        if self.completed && !bytes.is_empty() {
            return Err(failure(
                io::ErrorKind::InvalidData,
                "unexpected extra terminal input",
            ));
        }
        if !self.armed {
            return Ok(None);
        }
        for (index, &byte) in bytes.iter().enumerate() {
            match byte {
                b'\n' | b'\r' => {
                    let line = SecretLine::new(self.bytes[..self.used].to_vec());
                    self.clear();
                    self.completed = true;
                    self.extra_input = index + 1 != bytes.len();
                    return Ok(Some(line));
                }
                8 | 127 => {
                    if self.used > 0 {
                        let old = self.used;
                        self.used -= 1;
                        while self.used > 0 && self.bytes[self.used] & 0xc0 == 0x80 {
                            self.used -= 1;
                        }
                        wipe(&mut self.bytes[self.used..old]);
                    }
                }
                21 => {
                    wipe(&mut self.bytes);
                    self.used = 0;
                }
                0..=31 => {
                    return Err(failure(
                        io::ErrorKind::InvalidData,
                        "unsupported terminal control input",
                    ));
                }
                _ => {
                    if self.used == MAX_LINE {
                        return Err(failure(
                            io::ErrorKind::InvalidData,
                            "terminal input too long",
                        ));
                    }
                    self.bytes[self.used] = byte;
                    self.used += 1;
                }
            }
        }
        Ok(None)
    }
}

impl Drop for LineInput {
    fn drop(&mut self) {
        self.clear();
    }
}

fn failure(kind: io::ErrorKind, message: &'static str) -> io::Error {
    io::Error::new(kind, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pure_terminal_idle_and_second_line_never_become_approval() {
        let mut input = LineInput::default();
        assert!(input.consume(b"yes\n").unwrap().is_none());
        input.armed = true;
        assert!(input.consume(b"no\nyes\n").unwrap().is_some());
        assert!(!input.armed);
        assert!(input.reject_extra().is_err());
        assert!(input.bytes.iter().all(|byte| *byte == 0));
    }

    #[test]
    fn pure_terminal_second_read_rejects_even_partial_extra_input() {
        let mut input = LineInput {
            armed: true,
            ..LineInput::default()
        };
        assert!(input.consume(b"yes\n").unwrap().is_some());
        assert!(input.reject_extra().is_ok());
        assert!(input.consume(b"y").is_err());
        assert!(input.bytes.iter().all(|byte| *byte == 0));
    }

    #[test]
    fn pure_terminal_backspace_and_cancel_line_wipe_removed_input() {
        let mut input = LineInput {
            armed: true,
            ..LineInput::default()
        };
        input.consume("a界".as_bytes()).unwrap();
        input.consume(&[127]).unwrap();
        assert_eq!(input.used, 1);
        assert!(input.bytes[1..].iter().all(|byte| *byte == 0));
        input.consume(&[21]).unwrap();
        assert_eq!(input.used, 0);
        assert!(input.bytes.iter().all(|byte| *byte == 0));
        let result = input.consume(b"12345678\n").unwrap().unwrap();
        assert_eq!(result.as_str().unwrap(), "12345678");
        assert!(input.bytes.iter().all(|byte| *byte == 0));
    }

    #[test]
    fn pure_terminal_cancellation_wins_and_clears_partial_input() {
        for control in [3, 4] {
            let mut input = LineInput {
                armed: true,
                ..LineInput::default()
            };
            input.consume(b"secret").unwrap();
            assert_eq!(
                input.consume(&[b'\n', control]).err().unwrap().kind(),
                io::ErrorKind::Interrupted
            );
            assert!(!input.armed);
            assert!(input.bytes.iter().all(|byte| *byte == 0));
            assert!(input.consume(&[control]).is_err());
        }
    }

    #[test]
    fn pure_terminal_bounds_and_control_characters_fail_closed() {
        let mut input = LineInput {
            armed: true,
            ..LineInput::default()
        };
        assert!(input.consume(&[b'x'; MAX_LINE]).unwrap().is_none());
        assert!(input.consume(b"x").is_err());
        assert!(input.bytes.iter().all(|byte| *byte == 0));
        input.armed = true;
        assert!(input.consume(b"yes\x1b[31m").is_err());
        assert!(input.bytes.iter().all(|byte| *byte == 0));
    }

    #[test]
    fn pure_secret_utf8_error_does_not_include_input() {
        let secret = SecretLine::new(vec![255, b's']);
        assert_eq!(
            secret.as_str().err().unwrap().to_string(),
            "terminal input is not UTF-8"
        );
        assert_eq!(secret.as_bytes(), &[255, b's']);
        let mut bytes = vec![1, 2, 3];
        wipe(&mut bytes);
        assert_eq!(bytes, [0, 0, 0]);
    }
}
