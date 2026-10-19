//! Windows user-scoped protection for bounded control-credential bundles.
//!
//! This crate encrypts and decrypts bytes only. It does not choose a file path,
//! persist data, enroll a peer or expose unprotected bytes to logs.

use thiserror::Error;

pub const MAX_CREDENTIAL_BUNDLE_BYTES: usize = 65_536;
pub const MAX_PROTECTED_CREDENTIAL_BYTES: usize = MAX_CREDENTIAL_BUNDLE_BYTES + 4_096;

const ENTROPY_CONTEXT: &[u8] = b"CapyIO paired control credential v1";

pub fn protect(plaintext: &[u8]) -> Result<Vec<u8>, CredentialProtectionError> {
    validate_plaintext(plaintext)?;
    platform::protect(plaintext)
}

pub fn unprotect(ciphertext: &[u8]) -> Result<Vec<u8>, CredentialProtectionError> {
    if ciphertext.is_empty() {
        return Err(CredentialProtectionError::EmptyProtectedCredential);
    }
    if ciphertext.len() > MAX_PROTECTED_CREDENTIAL_BYTES {
        return Err(CredentialProtectionError::ProtectedCredentialTooLarge);
    }
    let plaintext = platform::unprotect(ciphertext)?;
    validate_plaintext(&plaintext)?;
    Ok(plaintext)
}

fn validate_plaintext(value: &[u8]) -> Result<(), CredentialProtectionError> {
    if value.is_empty() {
        return Err(CredentialProtectionError::EmptyCredential);
    }
    if value.len() > MAX_CREDENTIAL_BUNDLE_BYTES {
        return Err(CredentialProtectionError::CredentialTooLarge);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum CredentialProtectionError {
    #[error("credential bundle must not be empty")]
    EmptyCredential,
    #[error("credential bundle exceeds the size limit")]
    CredentialTooLarge,
    #[error("protected credential must not be empty")]
    EmptyProtectedCredential,
    #[error("protected credential exceeds the size limit")]
    ProtectedCredentialTooLarge,
    #[error("Windows DPAPI protection failed")]
    ProtectionFailed,
    #[error("Windows DPAPI unprotection failed")]
    UnprotectionFailed,
    #[error("Windows credential protection is unavailable on this platform")]
    UnsupportedPlatform,
}

#[cfg(windows)]
mod platform {
    use std::{ffi::c_void, ptr};

    use windows::{
        Win32::{
            Foundation::{HLOCAL, LocalFree},
            Security::Cryptography::{
                CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
            },
        },
        core::PCWSTR,
    };

    use super::{
        CredentialProtectionError, ENTROPY_CONTEXT, MAX_CREDENTIAL_BUNDLE_BYTES,
        MAX_PROTECTED_CREDENTIAL_BYTES,
    };

    pub(super) fn protect(plaintext: &[u8]) -> Result<Vec<u8>, CredentialProtectionError> {
        let input = blob(plaintext)?;
        let entropy = blob(ENTROPY_CONTEXT)?;
        let mut output = CRYPT_INTEGER_BLOB::default();
        // SAFETY: input blobs reference immutable slices for the call; output
        // is initialized and later released with LocalFree.
        unsafe {
            CryptProtectData(
                &input,
                PCWSTR::null(),
                Some(&entropy),
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
        .map_err(|_| CredentialProtectionError::ProtectionFailed)?;
        OwnedLocalBlob::new(output, false).copy(MAX_PROTECTED_CREDENTIAL_BYTES)
    }

    pub(super) fn unprotect(ciphertext: &[u8]) -> Result<Vec<u8>, CredentialProtectionError> {
        let input = blob(ciphertext)?;
        let entropy = blob(ENTROPY_CONTEXT)?;
        let mut output = CRYPT_INTEGER_BLOB::default();
        // SAFETY: input blobs reference immutable slices for the call; output
        // is initialized and later zeroed and released with LocalFree.
        unsafe {
            CryptUnprotectData(
                &input,
                None,
                Some(&entropy),
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
        .map_err(|_| CredentialProtectionError::UnprotectionFailed)?;
        OwnedLocalBlob::new(output, true).copy(MAX_CREDENTIAL_BUNDLE_BYTES)
    }

    fn blob(value: &[u8]) -> Result<CRYPT_INTEGER_BLOB, CredentialProtectionError> {
        let length = u32::try_from(value.len())
            .map_err(|_| CredentialProtectionError::CredentialTooLarge)?;
        Ok(CRYPT_INTEGER_BLOB {
            cbData: length,
            pbData: value.as_ptr().cast_mut(),
        })
    }

    struct OwnedLocalBlob {
        blob: CRYPT_INTEGER_BLOB,
        clear_before_free: bool,
    }

    impl OwnedLocalBlob {
        fn new(blob: CRYPT_INTEGER_BLOB, clear_before_free: bool) -> Self {
            Self {
                blob,
                clear_before_free,
            }
        }

        fn copy(&self, maximum: usize) -> Result<Vec<u8>, CredentialProtectionError> {
            let length = usize::try_from(self.blob.cbData)
                .map_err(|_| CredentialProtectionError::ProtectedCredentialTooLarge)?;
            if length == 0 || length > maximum || self.blob.pbData.is_null() {
                return Err(CredentialProtectionError::ProtectedCredentialTooLarge);
            }
            // SAFETY: DPAPI returned cbData readable bytes owned by self.
            Ok(unsafe { std::slice::from_raw_parts(self.blob.pbData, length) }.to_vec())
        }
    }

    impl Drop for OwnedLocalBlob {
        fn drop(&mut self) {
            if self.blob.pbData.is_null() {
                return;
            }
            if self.clear_before_free {
                // SAFETY: the DPAPI allocation contains cbData writable bytes.
                unsafe { ptr::write_bytes(self.blob.pbData, 0, self.blob.cbData as usize) };
            }
            // SAFETY: DPAPI output is released by LocalFree exactly once.
            unsafe {
                LocalFree(Some(HLOCAL(self.blob.pbData.cast::<c_void>())));
            }
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::CredentialProtectionError;

    pub(super) fn protect(_plaintext: &[u8]) -> Result<Vec<u8>, CredentialProtectionError> {
        Err(CredentialProtectionError::UnsupportedPlatform)
    }

    pub(super) fn unprotect(_ciphertext: &[u8]) -> Result<Vec<u8>, CredentialProtectionError> {
        Err(CredentialProtectionError::UnsupportedPlatform)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_fail_before_platform_access() {
        assert_eq!(
            protect(&[]),
            Err(CredentialProtectionError::EmptyCredential)
        );
        assert_eq!(
            protect(&vec![0; MAX_CREDENTIAL_BUNDLE_BYTES + 1]),
            Err(CredentialProtectionError::CredentialTooLarge)
        );
        assert_eq!(
            unprotect(&[]),
            Err(CredentialProtectionError::EmptyProtectedCredential)
        );
        assert_eq!(
            unprotect(&vec![0; MAX_PROTECTED_CREDENTIAL_BYTES + 1]),
            Err(CredentialProtectionError::ProtectedCredentialTooLarge)
        );
    }

    #[cfg(windows)]
    #[test]
    fn current_user_dpapi_round_trip_and_tamper_rejection() {
        let plaintext = b"bounded test credential; not a production key";
        let protected = protect(plaintext).expect("protect");
        assert_ne!(protected, plaintext);
        assert_eq!(unprotect(&protected).expect("unprotect"), plaintext);

        let mut tampered = protected;
        let index = tampered.len() / 2;
        tampered[index] ^= 0x80;
        assert_eq!(
            unprotect(&tampered),
            Err(CredentialProtectionError::UnprotectionFailed)
        );
    }
}
