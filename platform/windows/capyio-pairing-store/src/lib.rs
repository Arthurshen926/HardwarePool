#![deny(unsafe_op_in_unsafe_fn)]

//! Current-user pairing persistence. The directory contains no secrets;
//! credential files contain only DPAPI-protected opaque bytes.

use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use capyio_control_credentials::ControlCredentialBundle;
use capyio_core::NodeId;
use capyio_pairing::{CredentialHandle, MAX_PAIRED_DIRECTORY_JSON_BYTES, PairedPeerDirectory};
use capyio_windows_credential_protection::{
    self as protection, MAX_CREDENTIAL_BUNDLE_BYTES, MAX_PROTECTED_CREDENTIAL_BYTES,
};
use thiserror::Error;

const ROOT_COMPONENTS: [&str; 3] = ["CapyIO", "Control", "Pairing"];
const DIRECTORY_FILE: &str = "peers-v1.json";
const CREDENTIAL_DIRECTORY: &str = "credentials-v1";
const HANDLE_PREFIX: &str = "windows-dpapi:";
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

mod enrollment;

#[derive(Clone, Debug)]
pub struct WindowsPairingStore {
    root: PathBuf,
    local_node_id: NodeId,
}

impl WindowsPairingStore {
    /// The current process's selected store location, for non-secret UI diagnostics.
    pub fn directory_path(&self) -> &Path {
        &self.root
    }

    pub fn default_for_user(local_node_id: NodeId) -> Result<Self, PairingStoreError> {
        Self::from_local_app_data(&current_user_local_app_data()?, local_node_id)
    }

    pub fn from_local_app_data(
        base: &Path,
        local_node_id: NodeId,
    ) -> Result<Self, PairingStoreError> {
        if base.as_os_str().is_empty() || local_node_id.as_uuid().is_nil() {
            return Err(PairingStoreError::InvalidStoreIdentity);
        }
        let mut root = base.to_path_buf();
        for component in ROOT_COMPONENTS {
            root.push(component);
        }
        Ok(Self {
            root,
            local_node_id,
        })
    }

    pub fn load_directory(&self) -> Result<PairedPeerDirectory, PairingStoreError> {
        let path = self.root.join(DIRECTORY_FILE);
        let Some(bytes) = read_bounded_optional(&path, MAX_PAIRED_DIRECTORY_JSON_BYTES)? else {
            return PairedPeerDirectory::new(self.local_node_id).map_err(Into::into);
        };
        PairedPeerDirectory::decode_json(self.local_node_id, &bytes).map_err(Into::into)
    }

    /// Commits only non-secret metadata. Callers must successfully put every
    /// newly referenced credential before this final directory commit.
    pub fn commit_directory(
        &self,
        directory: &PairedPeerDirectory,
    ) -> Result<(), PairingStoreError> {
        let bytes = directory.encode_json()?;
        // Re-decode against this owner before any filesystem mutation.
        PairedPeerDirectory::decode_json(self.local_node_id, &bytes)?;
        atomic_write(&self.root.join(DIRECTORY_FILE), &bytes)
    }

    pub fn put_credential(
        &self,
        handle: &CredentialHandle,
        plaintext: &[u8],
    ) -> Result<(), PairingStoreError> {
        if plaintext.is_empty() || plaintext.len() > MAX_CREDENTIAL_BUNDLE_BYTES {
            return Err(PairingStoreError::CredentialSize);
        }
        let protected = protection::protect(plaintext)?;
        atomic_write(&self.credential_path(handle)?, &protected)
    }

    pub fn load_credential(&self, handle: &CredentialHandle) -> Result<Vec<u8>, PairingStoreError> {
        let path = self.credential_path(handle)?;
        let bytes = read_bounded_optional(&path, MAX_PROTECTED_CREDENTIAL_BYTES)?
            .ok_or(PairingStoreError::CredentialNotFound)?;
        protection::unprotect(&bytes).map_err(Into::into)
    }

    pub fn put_control_credential(
        &self,
        handle: &CredentialHandle,
        bundle: &ControlCredentialBundle,
    ) -> Result<(), PairingStoreError> {
        let mut encoded = bundle.encode()?;
        let result = self.put_credential(handle, &encoded);
        encoded.fill(0);
        result
    }

    pub fn load_control_credential(
        &self,
        handle: &CredentialHandle,
    ) -> Result<ControlCredentialBundle, PairingStoreError> {
        let mut plaintext = self.load_credential(handle)?;
        let result = ControlCredentialBundle::decode(&plaintext).map_err(Into::into);
        plaintext.fill(0);
        result
    }

    /// Removes one exact DPAPI envelope. Used to roll back a failed metadata
    /// commit during enrollment; it never scans or removes the store root.
    pub fn remove_credential(&self, handle: &CredentialHandle) -> Result<(), PairingStoreError> {
        let path = self.credential_path(handle)?;
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(PairingStoreError::Write(error)),
        }
    }

    fn credential_path(&self, handle: &CredentialHandle) -> Result<PathBuf, PairingStoreError> {
        let name = handle
            .as_str()
            .strip_prefix(HANDLE_PREFIX)
            .ok_or(PairingStoreError::UnsupportedCredentialHandle)?;
        if name.len() != 64
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(PairingStoreError::UnsupportedCredentialHandle);
        }
        Ok(self
            .root
            .join(CREDENTIAL_DIRECTORY)
            .join(format!("{name}.dpapi")))
    }
}

#[cfg(windows)]
fn current_user_local_app_data() -> Result<PathBuf, PairingStoreError> {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt, ptr};
    use windows_sys::Win32::{
        System::Com::CoTaskMemFree,
        UI::Shell::{FOLDERID_LocalAppData, SHGetKnownFolderPath},
    };

    let mut raw = ptr::null_mut();
    // SAFETY: null token selects the effective process user's known folder;
    // the returned COM allocation is freed below even if its content is invalid.
    let status =
        unsafe { SHGetKnownFolderPath(&FOLDERID_LocalAppData, 0, ptr::null_mut(), &mut raw) };
    if status < 0 || raw.is_null() {
        if !raw.is_null() {
            // SAFETY: even an unsuccessful shell call may return a COM allocation.
            unsafe { CoTaskMemFree(raw.cast()) };
        }
        return Err(PairingStoreError::LocalAppDataUnavailable);
    }
    let length = unsafe { (0..32768).find(|&index| *raw.add(index) == 0) };
    let path = length.map(|count| {
        // SAFETY: SHGetKnownFolderPath returned a terminated UTF-16 allocation.
        OsString::from_wide(unsafe { std::slice::from_raw_parts(raw, count) })
    });
    // SAFETY: raw is the allocation returned by SHGetKnownFolderPath.
    unsafe { CoTaskMemFree(raw.cast()) };
    path.map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or(PairingStoreError::LocalAppDataUnavailable)
}

#[cfg(not(windows))]
fn current_user_local_app_data() -> Result<PathBuf, PairingStoreError> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or(PairingStoreError::LocalAppDataUnavailable)
}

fn read_bounded_optional(
    path: &Path,
    maximum: usize,
) -> Result<Option<Vec<u8>>, PairingStoreError> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(PairingStoreError::Read(source)),
    };
    let maximum = u64::try_from(maximum).map_err(|_| PairingStoreError::Size)?;
    let mut bytes = Vec::new();
    file.take(maximum + 1)
        .read_to_end(&mut bytes)
        .map_err(PairingStoreError::Read)?;
    if bytes.len() as u64 > maximum {
        return Err(PairingStoreError::Size);
    }
    Ok(Some(bytes))
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), PairingStoreError> {
    let parent = path.parent().ok_or(PairingStoreError::InvalidPath)?;
    fs::create_dir_all(parent).map_err(PairingStoreError::Write)?;
    let (temporary, mut file) = create_temporary(parent)?;
    let result = file.write_all(bytes).and_then(|_| file.sync_all());
    drop(file);
    if let Err(source) = result {
        let _ = fs::remove_file(&temporary);
        return Err(PairingStoreError::Write(source));
    }
    if let Err(source) = platform_replace(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(PairingStoreError::Write(source));
    }
    Ok(())
}

fn create_temporary(parent: &Path) -> Result<(PathBuf, File), PairingStoreError> {
    for _ in 0..128 {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(".capyio-{}-{sequence}.tmp", std::process::id()));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(PairingStoreError::Write(error)),
        }
    }
    Err(PairingStoreError::Write(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "pairing store temporary namespace is exhausted",
    )))
}

#[cfg(windows)]
fn platform_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };
    let from: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let to: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: both NUL-terminated buffers remain valid for the synchronous call.
    if unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn platform_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::rename(from, to)
}

#[derive(Debug, Error)]
pub enum PairingStoreError {
    #[error("LOCALAPPDATA is unavailable")]
    LocalAppDataUnavailable,
    #[error("pairing store identity is invalid")]
    InvalidStoreIdentity,
    #[error("pairing store path is invalid")]
    InvalidPath,
    #[error("credential handle is not a Windows DPAPI handle")]
    UnsupportedCredentialHandle,
    #[error("credential is missing")]
    CredentialNotFound,
    #[error("credential size is invalid")]
    CredentialSize,
    #[error("stored object exceeds its bound")]
    Size,
    #[error("pairing store read failed: {0}")]
    Read(std::io::Error),
    #[error("pairing store write failed: {0}")]
    Write(std::io::Error),
    #[error(transparent)]
    Pairing(#[from] capyio_pairing::PairingError),
    #[error(transparent)]
    Protection(#[from] protection::CredentialProtectionError),
    #[error(transparent)]
    Credential(#[from] capyio_control_credentials::ControlCredentialError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_skips_a_stale_temporary_name() {
        let root = std::env::temp_dir().join(format!(
            "capyio-pairing-store-stale-temp-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).unwrap();
        let sequence = TEMP_SEQUENCE.load(Ordering::Relaxed);
        let stale = root.join(format!(".capyio-{}-{sequence}.tmp", std::process::id()));
        fs::write(&stale, b"stale").unwrap();

        atomic_write(&root.join("target"), b"complete").unwrap();

        assert_eq!(fs::read(root.join("target")).unwrap(), b"complete");
        assert_eq!(fs::read(stale).unwrap(), b"stale");
        fs::remove_dir_all(root).unwrap();
    }
}
