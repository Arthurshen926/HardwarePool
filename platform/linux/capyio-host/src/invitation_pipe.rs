//! Explicit private invitation delivery, never the ordinary management channel.
//!
//! The caller owns an existing FIFO and its isolated reader. Same-user processes
//! are trusted: metadata checks do not authenticate the reader or protect against
//! a malicious process running as that user. No paths or payloads enter errors.
//! Scratch clearing is best effort, not protection against compiler elision,
//! process inspection, swap, or copies held by the caller.

use rustix::fs::{self, AtFlags, FileType, Mode, OFlags, Stat};
use std::{
    io,
    os::fd::OwnedFd,
    path::{Component, Path},
    time::{Duration, Instant},
};

const MAX_PAYLOAD: usize = 4096;
const RETRY_INTERVAL: Duration = Duration::from_millis(2);

pub(crate) struct InvitationPipe {
    fd: OwnedFd,
    poisoned: bool,
}

impl InvitationPipe {
    /// Requires the private reader to be open already; never waits for a reader.
    pub(crate) fn open(path: &Path) -> io::Result<Self> {
        validate_path(path)?;
        let name = path.file_name().ok_or_else(invalid_target)?;
        let parent = path.parent().ok_or_else(invalid_target)?;
        let directory_flags =
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
        let mut directory =
            fs::open("/", directory_flags, Mode::empty()).map_err(|_| invalid_target())?;
        for component in parent.components() {
            match component {
                Component::RootDir => (),
                Component::Normal(name) => {
                    directory = fs::openat(&directory, name, directory_flags, Mode::empty())
                        .map_err(|_| invalid_target())?;
                }
                _ => return Err(invalid_target()),
            }
        }
        let uid = rustix::process::geteuid().as_raw();
        let parent_stat = fs::fstat(&directory).map_err(|_| invalid_target())?;
        if !private_metadata(&parent_stat, FileType::Directory, 0o700, uid) {
            return Err(invalid_target());
        }
        // Reject devices and other non-FIFO objects before opening them. The
        // descriptor is checked again so a replacement cannot redirect output.
        let before = fs::statat(&directory, name, AtFlags::SYMLINK_NOFOLLOW)
            .map_err(|_| invalid_target())?;
        if !private_fifo(&before, uid) {
            return Err(invalid_target());
        }
        let fd = fs::openat(
            &directory,
            name,
            OFlags::WRONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
            Mode::empty(),
        )
        .map_err(|_| invalid_target())?;
        let actual = fs::fstat(&fd).map_err(|_| invalid_target())?;
        if !private_fifo(&actual, uid) || !same_object(&before, &actual) {
            return Err(invalid_target());
        }
        // Fail closed when a standard descriptor cannot be inspected, too.
        for standard in [
            fs::fstat(io::stdin()),
            fs::fstat(io::stdout()),
            fs::fstat(io::stderr()),
        ] {
            if same_object(&actual, &standard.map_err(|_| invalid_target())?) {
                return Err(invalid_target());
            }
        }
        Ok(Self {
            fd,
            poisoned: false,
        })
    }

    /// Sends a u32 big-endian length followed by at most 4096 encoded bytes.
    /// Any error poisons this instance, including a partial frame or timeout.
    pub(crate) fn send(&mut self, bytes: &[u8], deadline: Instant) -> io::Result<()> {
        send_frame(&mut self.poisoned, bytes, deadline, |chunk| {
            rustix::io::write(&self.fd, chunk).map_err(io::Error::from)
        })
    }
}

fn send_frame(
    poisoned: &mut bool,
    bytes: &[u8],
    deadline: Instant,
    write: impl FnMut(&[u8]) -> io::Result<usize>,
) -> io::Result<()> {
    if *poisoned {
        return Err(io::Error::new(
            io::ErrorKind::BrokenPipe,
            "invitation pipe is closed",
        ));
    }
    *poisoned = true;
    let frame = Frame::encode(bytes)?;
    write_frame(frame.as_slice(), deadline, write)?;
    *poisoned = false;
    Ok(())
}

fn invalid_target() -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        "private invitation FIFO unavailable",
    )
}

fn validate_path(path: &Path) -> io::Result<()> {
    if !path.is_absolute()
        || path.file_name().is_none()
        || path
            .components()
            .any(|part| !matches!(part, Component::RootDir | Component::Normal(_)))
    {
        return Err(invalid_target());
    }
    Ok(())
}

fn private_metadata(stat: &Stat, kind: FileType, permissions: u32, uid: u32) -> bool {
    private_attributes(stat.st_mode, stat.st_uid, kind, permissions, uid)
}

fn private_attributes(mode: u32, owner: u32, kind: FileType, permissions: u32, uid: u32) -> bool {
    FileType::from_raw_mode(mode) == kind && owner == uid && mode & 0o7777 == permissions
}

fn private_fifo(stat: &Stat, uid: u32) -> bool {
    private_metadata(stat, FileType::Fifo, 0o600, uid) && stat.st_nlink == 1
}

fn same_object(a: &Stat, b: &Stat) -> bool {
    a.st_dev == b.st_dev && a.st_ino == b.st_ino
}

struct Frame {
    bytes: [u8; MAX_PAYLOAD + 4],
    used: usize,
}

impl Frame {
    fn encode(payload: &[u8]) -> io::Result<Self> {
        if payload.is_empty() || payload.len() > MAX_PAYLOAD {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid invitation frame length",
            ));
        }
        let mut result = Self {
            bytes: [0; MAX_PAYLOAD + 4],
            used: payload.len() + 4,
        };
        result.bytes[..4].copy_from_slice(&(payload.len() as u32).to_be_bytes());
        result.bytes[4..result.used].copy_from_slice(payload);
        Ok(result)
    }

    fn as_slice(&self) -> &[u8] {
        &self.bytes[..self.used]
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        self.bytes.fill(0);
    }
}

fn write_frame(
    frame: &[u8],
    deadline: Instant,
    mut write: impl FnMut(&[u8]) -> io::Result<usize>,
) -> io::Result<()> {
    let mut used = 0;
    while used < frame.len() {
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "invitation delivery expired",
            ));
        }
        match write(&frame[used..]) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "invitation delivery stopped",
                ));
            }
            Ok(count) if count <= frame.len() - used => used += count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => (),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(
                    RETRY_INTERVAL.min(deadline.saturating_duration_since(Instant::now())),
                );
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "invitation delivery failed",
                ));
            }
        }
    }
    if Instant::now() >= deadline {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "invitation delivery expired",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_relative_root_and_parent_traversal() {
        for path in ["", "invitation", "/", "/private/../invitation"] {
            assert!(validate_path(Path::new(path)).is_err());
        }
        assert!(validate_path(Path::new("/private/invitation")).is_ok());
    }

    #[test]
    fn private_metadata_requires_exact_type_owner_and_mode() {
        const FIFO: u32 = 0o010000;
        const DIRECTORY: u32 = 0o040000;
        assert!(private_attributes(
            FIFO | 0o600,
            123,
            FileType::Fifo,
            0o600,
            123
        ));
        for mode in [FIFO | 0o660, FIFO | 0o644, FIFO | 0o4600, DIRECTORY | 0o600] {
            assert!(!private_attributes(mode, 123, FileType::Fifo, 0o600, 123));
        }
        assert!(!private_attributes(
            FIFO | 0o600,
            124,
            FileType::Fifo,
            0o600,
            123
        ));
        assert!(private_attributes(
            DIRECTORY | 0o700,
            123,
            FileType::Directory,
            0o700,
            123
        ));
        assert!(!private_attributes(
            DIRECTORY | 0o755,
            123,
            FileType::Directory,
            0o700,
            123
        ));
    }

    #[test]
    fn bounded_big_endian_frame() {
        assert!(Frame::encode(&[]).is_err());
        assert!(Frame::encode(&[0; MAX_PAYLOAD + 1]).is_err());
        let frame = Frame::encode(&[42; MAX_PAYLOAD]).unwrap();
        assert_eq!(&frame.as_slice()[..4], &4096_u32.to_be_bytes());
        assert_eq!(&frame.as_slice()[4..], &[42; MAX_PAYLOAD]);
    }

    #[test]
    fn partial_writes_preserve_one_frame() {
        let frame = Frame::encode(b"private invitation").unwrap();
        let mut output = Vec::new();
        write_frame(
            frame.as_slice(),
            Instant::now() + Duration::from_secs(1),
            |bytes| {
                let count = bytes.len().min(3);
                output.extend_from_slice(&bytes[..count]);
                Ok(count)
            },
        )
        .unwrap();
        assert_eq!(output, frame.as_slice());
    }

    #[test]
    fn expired_frame_never_writes() {
        let error = write_frame(b"secret", Instant::now(), |_| {
            panic!("wrote after deadline")
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    }

    #[test]
    fn partial_failure_has_no_payload_or_underlying_diagnostic() {
        let mut calls = 0;
        let error = write_frame(b"secret", Instant::now() + Duration::from_secs(1), |_| {
            calls += 1;
            if calls == 1 {
                Ok(2)
            } else {
                Err(io::Error::other("secret target path"))
            }
        })
        .unwrap_err();
        assert_eq!(calls, 2);
        assert_eq!(error.to_string(), "invitation delivery failed");
    }

    #[test]
    fn failed_frame_permanently_poisons_writer() {
        let mut poisoned = false;
        let deadline = Instant::now() + Duration::from_secs(1);
        let mut calls = 0;
        assert!(
            send_frame(&mut poisoned, b"secret", deadline, |_| {
                calls += 1;
                if calls == 1 {
                    Ok(2)
                } else {
                    Err(io::ErrorKind::BrokenPipe.into())
                }
            })
            .is_err()
        );
        assert!(poisoned);
        assert!(
            send_frame(&mut poisoned, b"second", deadline, |_| panic!(
                "reused poisoned pipe"
            ))
            .is_err()
        );
        let mut poisoned = false;
        assert!(
            send_frame(&mut poisoned, &[0; MAX_PAYLOAD + 1], deadline, |_| panic!(
                "oversized write"
            ))
            .is_err()
        );
        assert!(poisoned);
    }

    #[test]
    fn backpressure_obeys_absolute_deadline() {
        let error = write_frame(b"secret", Instant::now() + Duration::from_millis(3), |_| {
            Err(io::ErrorKind::WouldBlock.into())
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    }
}
