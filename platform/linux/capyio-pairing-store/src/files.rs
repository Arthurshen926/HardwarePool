use super::StoreError;
use rustix::fs::{self, AtFlags, FileType, Mode, OFlags};
use std::{
    fs::File,
    io::{Read, Write},
    os::fd::AsFd,
    path::{Component, Path},
    sync::atomic::{AtomicU64, Ordering},
};

static SEQUENCE: AtomicU64 = AtomicU64::new(1);
pub(super) struct Files {
    owner_pid: u32,
    root: File,
    _lock: File,
}
impl Files {
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        if !path.is_absolute() {
            return Err(StoreError::Metadata);
        }
        let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
        let mut root = File::from(fs::open("/", flags, Mode::empty())?);
        for component in path.components() {
            match component {
                Component::RootDir => (),
                Component::Normal(name) => {
                    root = File::from(fs::openat(&root, name, flags, Mode::empty())?)
                }
                _ => return Err(StoreError::Metadata),
            }
        }
        let stat = fs::fstat(&root)?;
        if stat.st_uid != rustix::process::geteuid().as_raw() || stat.st_mode & 0o777 != 0o700 {
            return Err(StoreError::Metadata);
        }
        let lock = File::from(fs::openat(
            &root,
            ".owner.lock",
            OFlags::RDWR | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
            Mode::RUSR | Mode::WUSR,
        )?);
        validate(&lock)?;
        fs::flock(&lock, fs::FlockOperation::NonBlockingLockExclusive)
            .map_err(|_| StoreError::Busy)?;
        root.sync_all()?;
        Ok(Self {
            root,
            _lock: lock,
            owner_pid: std::process::id(),
        })
    }
    pub fn check_owner(&self) -> Result<(), StoreError> {
        if self.owner_pid != std::process::id() {
            return Err(StoreError::Busy);
        }
        Ok(())
    }
    pub fn read(&self, name: &str, maximum: usize) -> Result<Option<Vec<u8>>, StoreError> {
        self.check_owner()?;
        let fd = match fs::openat(
            &self.root,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
            Mode::empty(),
        ) {
            Ok(fd) => fd,
            Err(rustix::io::Errno::NOENT) => return Ok(None),
            Err(e) => return Err(std::io::Error::from(e).into()),
        };
        let file = File::from(fd);
        validate(&file)?;
        let mut bytes = Vec::new();
        file.take(maximum as u64 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > maximum {
            return Err(StoreError::Metadata);
        }
        Ok(Some(bytes))
    }
    pub fn write(&self, name: &str, bytes: &[u8]) -> Result<(), StoreError> {
        self.check_owner()?;
        // Existing objects must be safe regular files; never overwrite symlinks.
        self.read(name, super::MAX_DOCUMENT)?;
        let mut temporary = None;
        for _ in 0..128 {
            let name = format!(
                ".pending-{}-{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            );
            match fs::openat(
                &self.root,
                &name,
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::RUSR | Mode::WUSR,
            ) {
                Ok(fd) => {
                    temporary = Some((name, File::from(fd)));
                    break;
                }
                Err(rustix::io::Errno::EXIST) => (),
                Err(e) => return Err(std::io::Error::from(e).into()),
            }
        }
        let (temporary, mut file) = temporary.ok_or(StoreError::Metadata)?;
        let result = (|| {
            file.write_all(bytes)?;
            file.sync_all()?;
            fs::renameat(&self.root, &temporary, &self.root, name)?;
            self.root.sync_all()?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::unlinkat(&self.root, &temporary, AtFlags::empty());
        }
        result
    }
    pub fn remove(&self, name: &str) -> Result<(), StoreError> {
        self.check_owner()?;
        fs::unlinkat(&self.root, name, AtFlags::empty())?;
        self.root.sync_all()?;
        Ok(())
    }
}
fn validate(file: impl AsFd) -> Result<(), StoreError> {
    let stat = fs::fstat(file)?;
    if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile
        || stat.st_nlink != 1
        || stat.st_uid != rustix::process::geteuid().as_raw()
        || stat.st_mode & 0o777 != 0o600
    {
        return Err(StoreError::Metadata);
    }
    Ok(())
}
impl From<rustix::io::Errno> for StoreError {
    fn from(error: rustix::io::Errno) -> Self {
        Self::Io(error.into())
    }
}

impl Drop for Files {
    fn drop(&mut self) {
        // Explicitly unlock before close: another thread may be between fork and
        // exec with a transient inherited CLOEXEC descriptor. Closing only our
        // descriptor would keep that shared open-file-description locked briefly.
        if self.owner_pid == std::process::id() {
            let _ = fs::flock(&self._lock, fs::FlockOperation::Unlock);
        }
    }
}
