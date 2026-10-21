//! One private, owned invitation FIFO. No caller-selected path is chmodded,
//! recursively removed or reused, and no invitation bytes reach standard I/O.

use std::ffi::{CStr, CString, OsString};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::terminal::{SecretLine, wipe};

const MAX_FRAME: usize = 4096;
const PARTIAL_TIMEOUT: Duration = Duration::from_secs(1);
const FIFO_NAME: &CStr = c"invitation.fifo";

pub(crate) struct InvitationFifo {
    reader: OwnedFd,
    // Keep the FIFO readable before its one genuine writer has opened it.
    // This descriptor is never used to write and is never inherited by a child.
    keepalive: Option<OwnedFd>,
    directory: PrivateDirectory,
    fifo_identity: Identity,
    path: PathBuf,
    frame: FrameReader,
}

impl InvitationFifo {
    pub(crate) fn create() -> io::Result<Self> {
        let directory = PrivateDirectory::create()?;
        // SAFETY: The directory descriptor is owned; the fixed name has no slash.
        if unsafe { libc::mkfifoat(directory.fd.as_raw_fd(), FIFO_NAME.as_ptr(), 0o600) } != 0 {
            return Err(failure(
                io::ErrorKind::Other,
                "private invitation FIFO creation failed",
            ));
        }
        // Capture identity before opening. A construction error removes only
        // this newly created exact directory entry, never unrelated replacements.
        let fifo_identity = identity_at(directory.fd.as_raw_fd(), FIFO_NAME)?;
        let result = (|| {
            let reader = open_at(
                directory.fd.as_raw_fd(),
                FIFO_NAME,
                libc::O_RDONLY | libc::O_NONBLOCK,
            )?;
            validate_fifo(&reader, fifo_identity)?;
            let keepalive = open_at(
                directory.fd.as_raw_fd(),
                FIFO_NAME,
                libc::O_WRONLY | libc::O_NONBLOCK,
            )?;
            validate_fifo(&keepalive, fifo_identity)?;
            Ok((reader, keepalive))
        })();
        let (reader, keepalive) = match result {
            Ok(descriptors) => descriptors,
            Err(error) => {
                remove_owned_fifo(directory.fd.as_raw_fd(), fifo_identity);
                return Err(error);
            }
        };
        let path = directory.path.join("invitation.fifo");
        Ok(Self {
            reader,
            keepalive: Some(keepalive),
            directory,
            fifo_identity,
            path,
            frame: FrameReader::default(),
        })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// At most two nonblocking reads; one four-byte big-endian bounded frame.
    /// After delivery this is inert. The owner must still enforce total lifetime.
    pub(crate) fn poll(&mut self) -> io::Result<Option<SecretLine>> {
        let mut received_any = false;
        let raw = self.reader.as_raw_fd();
        let result = self.frame.poll(Instant::now(), |bytes| {
            // SAFETY: bytes is a live writable slice and raw is owned/nonblocking.
            let count = unsafe { libc::read(raw, bytes.as_mut_ptr().cast(), bytes.len()) };
            if count < 0 {
                return Err(io::Error::last_os_error());
            }
            received_any |= count > 0;
            Ok(count as usize)
        });
        // Once genuine input arrives, EOF is meaningful, including truncation.
        if received_any || result.is_err() {
            self.keepalive = None;
        }
        result
    }
}

impl Drop for InvitationFifo {
    fn drop(&mut self) {
        remove_owned_fifo(self.directory.fd.as_raw_fd(), self.fifo_identity);
        // PrivateDirectory subsequently removes only the original empty dir.
    }
}

struct PrivateDirectory {
    fd: OwnedFd,
    path: PathBuf,
    identity: Identity,
}

impl PrivateDirectory {
    fn create() -> io::Result<Self> {
        // A fixed /tmp root avoids environment-selected/symlinked TMPDIR paths.
        // mkdtemp creates a fresh 0700 directory atomically, respecting umask.
        let mut template = b"/tmp/capyio-pairing-XXXXXX\0".to_vec();
        // SAFETY: template is mutable, NUL-terminated and has six final X bytes.
        let result = unsafe { libc::mkdtemp(template.as_mut_ptr().cast()) };
        if result.is_null() {
            return Err(failure(
                io::ErrorKind::Other,
                "private invitation directory creation failed",
            ));
        }
        template.pop();
        let path = PathBuf::from(OsString::from_vec(template));
        let metadata = std::fs::symlink_metadata(&path).map_err(|_| {
            failure(
                io::ErrorKind::Other,
                "private invitation directory unavailable",
            )
        })?;
        let identity = Identity::from_metadata(&metadata);
        let path_c = CString::new(path.as_os_str().as_bytes())
            .map_err(|_| failure(io::ErrorKind::InvalidInput, "invalid invitation directory"))?;
        // SAFETY: path_c is a NUL-terminated path. NOFOLLOW excludes substitution.
        let raw = unsafe {
            libc::open(
                path_c.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            )
        };
        if raw < 0 {
            remove_owned_directory(&path, identity);
            return Err(failure(
                io::ErrorKind::Other,
                "private invitation directory open failed",
            ));
        }
        // SAFETY: open returned a fresh owned descriptor.
        let fd = unsafe { OwnedFd::from_raw_fd(raw) };
        let directory = Self { fd, path, identity };
        let stat = stat_fd(&directory.fd)?;
        // SAFETY: geteuid inspects the calling process only.
        if Identity::from_stat(&stat) != identity
            || stat.st_mode & libc::S_IFMT != libc::S_IFDIR
            || stat.st_mode & 0o7777 != 0o700
            || stat.st_uid != unsafe { libc::geteuid() }
        {
            return Err(failure(
                io::ErrorKind::PermissionDenied,
                "invitation directory is not private",
            ));
        }
        Ok(directory)
    }
}

impl Drop for PrivateDirectory {
    fn drop(&mut self) {
        remove_owned_directory(&self.path, self.identity);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Identity {
    device: u64,
    inode: u64,
}

impl Identity {
    fn from_stat(stat: &libc::stat) -> Self {
        Self {
            device: stat.st_dev,
            inode: stat.st_ino,
        }
    }

    fn from_metadata(metadata: &std::fs::Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
        }
    }
}

fn open_at(directory: libc::c_int, name: &CStr, flags: libc::c_int) -> io::Result<OwnedFd> {
    // SAFETY: name is NUL-terminated; flags do not request creation.
    let raw = unsafe {
        libc::openat(
            directory,
            name.as_ptr(),
            flags | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        )
    };
    if raw < 0 {
        return Err(failure(
            io::ErrorKind::Other,
            "private invitation FIFO open failed",
        ));
    }
    // SAFETY: openat returned a fresh owned descriptor.
    Ok(unsafe { OwnedFd::from_raw_fd(raw) })
}

fn stat_fd(fd: &OwnedFd) -> io::Result<libc::stat> {
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    // SAFETY: fstat initializes stat on success, checked before assume_init.
    if unsafe { libc::fstat(fd.as_raw_fd(), stat.as_mut_ptr()) } != 0 {
        return Err(failure(
            io::ErrorKind::Other,
            "private invitation descriptor inspection failed",
        ));
    }
    // SAFETY: fstat succeeded.
    Ok(unsafe { stat.assume_init() })
}

fn validate_fifo(fd: &OwnedFd, identity: Identity) -> io::Result<()> {
    let stat = stat_fd(fd)?;
    // SAFETY: geteuid inspects the calling process only.
    if Identity::from_stat(&stat) != identity
        || stat.st_mode & libc::S_IFMT != libc::S_IFIFO
        || stat.st_mode & 0o7777 != 0o600
        || stat.st_uid != unsafe { libc::geteuid() }
        || stat.st_nlink != 1
    {
        return Err(failure(
            io::ErrorKind::PermissionDenied,
            "invitation FIFO is not private",
        ));
    }
    Ok(())
}

fn identity_at(directory: libc::c_int, name: &CStr) -> io::Result<Identity> {
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    // SAFETY: fixed leaf name and valid directory; stat initialized on success.
    if unsafe {
        libc::fstatat(
            directory,
            name.as_ptr(),
            stat.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } != 0
    {
        return Err(failure(
            io::ErrorKind::Other,
            "private invitation path inspection failed",
        ));
    }
    // SAFETY: fstatat succeeded.
    Ok(Identity::from_stat(&unsafe { stat.assume_init() }))
}

fn remove_owned_fifo(directory: libc::c_int, identity: Identity) {
    if identity_at(directory, FIFO_NAME).is_ok_and(|current| current == identity) {
        // SAFETY: Fixed leaf is verified as the entry this owner created.
        // Same-UID adversarial races are outside this private-directory boundary.
        unsafe { libc::unlinkat(directory, FIFO_NAME.as_ptr(), 0) };
    }
}

fn remove_owned_directory(path: &Path, identity: Identity) {
    if std::fs::symlink_metadata(path)
        .is_ok_and(|metadata| metadata.is_dir() && Identity::from_metadata(&metadata) == identity)
    {
        // Never recurse: unrelated inserted files prevent directory removal.
        let _ = std::fs::remove_dir(path);
    }
}

#[derive(Default)]
struct FrameReader {
    prefix: [u8; 4],
    prefix_used: usize,
    body: Vec<u8>,
    body_used: usize,
    started: Option<Instant>,
    done: bool,
    failed: bool,
}

impl FrameReader {
    fn poll(
        &mut self,
        now: Instant,
        mut read: impl FnMut(&mut [u8]) -> io::Result<usize>,
    ) -> io::Result<Option<SecretLine>> {
        let started_before = self.started;
        let mut result = self.poll_inner(now, &mut read);
        // The supplied clock makes pure tests deterministic, while a fresh
        // elapsed check covers descheduling across the two real nonblocking reads.
        let start = started_before.or(self.started);
        if result.is_ok() && start.is_some_and(|start| start.elapsed() >= PARTIAL_TIMEOUT) {
            result = Err(failure(
                io::ErrorKind::TimedOut,
                "partial invitation timed out",
            ));
        }
        if result.is_err() {
            self.clear();
            self.failed = true;
        } else if result.as_ref().is_ok_and(Option::is_some) {
            // The returned frame now owns the body. Disarm its partial deadline
            // so an idle completed reader cannot fail during human consent.
            self.clear();
        }
        result
    }

    fn poll_inner(
        &mut self,
        now: Instant,
        read: &mut impl FnMut(&mut [u8]) -> io::Result<usize>,
    ) -> io::Result<Option<SecretLine>> {
        if self.failed {
            return Err(failure(
                io::ErrorKind::BrokenPipe,
                "invitation channel is terminal",
            ));
        }
        if self.done {
            return Ok(None);
        }
        if self
            .started
            .is_some_and(|start| now.saturating_duration_since(start) >= PARTIAL_TIMEOUT)
        {
            return Err(failure(
                io::ErrorKind::TimedOut,
                "partial invitation timed out",
            ));
        }
        if self.prefix_used < self.prefix.len() {
            let count = read_some(read, &mut self.prefix[self.prefix_used..])?;
            if count > 0 && self.started.is_none() {
                self.started = Some(now);
            }
            self.prefix_used += count;
            if self.prefix_used != self.prefix.len() {
                return Ok(None);
            }
        }
        if self.body.is_empty() {
            let size = u32::from_be_bytes(self.prefix) as usize;
            if size == 0 || size > MAX_FRAME {
                return Err(failure(
                    io::ErrorKind::InvalidData,
                    "invalid invitation frame length",
                ));
            }
            self.body = vec![0; size];
        }
        self.body_used += read_some(read, &mut self.body[self.body_used..])?;
        if self.body_used != self.body.len() {
            return Ok(None);
        }
        let result = SecretLine::new(std::mem::take(&mut self.body));
        self.done = true;
        Ok(Some(result))
    }

    fn clear(&mut self) {
        wipe(&mut self.prefix);
        wipe(&mut self.body);
        self.body.clear();
        self.prefix_used = 0;
        self.body_used = 0;
        self.started = None;
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
        Ok(0) => Err(failure(
            io::ErrorKind::UnexpectedEof,
            "invitation channel closed",
        )),
        Ok(count) if count <= bytes.len() => Ok(count),
        Ok(_) => Err(failure(
            io::ErrorKind::InvalidData,
            "invalid invitation read length",
        )),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
            ) =>
        {
            Ok(0)
        }
        Err(_) => Err(failure(
            io::ErrorKind::Other,
            "invitation channel read failed",
        )),
    }
}

fn failure(kind: io::ErrorKind, message: &'static str) -> io::Error {
    io::Error::new(kind, message)
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;

    fn source(bytes: &mut VecDeque<u8>, max: usize, output: &mut [u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let size = max.min(bytes.len()).min(output.len());
        for byte in &mut output[..size] {
            *byte = bytes.pop_front().unwrap();
        }
        Ok(size)
    }

    #[test]
    fn pure_invitation_fragmented_frame_is_bounded_and_delivered_once() {
        let mut reader = FrameReader::default();
        let mut bytes: VecDeque<_> = [0, 0, 0, 3, b'a', b'b', b'c', b'x'].into();
        let start = Instant::now();
        let frame = loop {
            if let Some(frame) = reader
                .poll(start, |output| source(&mut bytes, 1, output))
                .unwrap()
            {
                break frame;
            }
        };
        assert_eq!(frame.as_bytes(), b"abc");
        assert_eq!(bytes, VecDeque::from(*b"x"));
        assert!(
            reader
                .poll(start, |_| panic!("completed reader is inert"))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn pure_invitation_completed_reader_has_no_partial_deadline() {
        let mut reader = FrameReader::default();
        let start = Instant::now();
        let mut bytes: VecDeque<_> = [0, 0, 0, 1, b'a'].into();
        assert!(
            reader
                .poll(start, |output| source(&mut bytes, 4, output))
                .unwrap()
                .is_some()
        );
        assert!(reader.done);
        assert!(reader.started.is_none());
        assert!(
            reader
                .poll(start + PARTIAL_TIMEOUT * 2, |_| panic!("completed read"))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn pure_invitation_zero_and_oversized_frames_fail_before_allocation() {
        for size in [0_u32, MAX_FRAME as u32 + 1, u32::MAX] {
            let mut reader = FrameReader::default();
            let mut bytes: VecDeque<_> = size.to_be_bytes().into();
            assert!(
                reader
                    .poll(Instant::now(), |output| source(&mut bytes, 4, output))
                    .is_err()
            );
            assert!(reader.body.is_empty());
            assert!(reader.failed);
        }
    }

    #[test]
    fn pure_invitation_partial_timeout_clears_secrets_and_poisons_channel() {
        let mut reader = FrameReader::default();
        let start = Instant::now();
        let mut bytes: VecDeque<_> = [0, 0, 0, 8, b'1', b'2'].into();
        assert!(
            reader
                .poll(start, |output| source(&mut bytes, 4, output))
                .unwrap()
                .is_none()
        );
        assert!(reader.body.contains(&b'1'));
        assert_eq!(
            reader
                .poll(start + PARTIAL_TIMEOUT, |_| panic!("expired read"))
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::TimedOut
        );
        assert!(reader.body.is_empty());
        assert!(reader.prefix.iter().all(|byte| *byte == 0));
        assert!(reader.poll(start, |_| panic!("poisoned read")).is_err());
    }

    #[test]
    fn pure_invitation_eof_and_read_errors_are_redacted_and_terminal() {
        for eof in [true, false] {
            let mut reader = FrameReader::default();
            let error = reader
                .poll(Instant::now(), |_| {
                    if eof {
                        Ok(0)
                    } else {
                        Err(io::Error::other("do not echo this secret"))
                    }
                })
                .err()
                .unwrap();
            assert!(!error.to_string().contains("secret"));
            assert!(reader.failed);
        }
    }
}
