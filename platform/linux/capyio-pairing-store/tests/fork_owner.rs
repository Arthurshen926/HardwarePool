//! A separate single-threaded executable, deliberately NOT the multithreaded Rust
//! test harness. Fork happens before starting threads or an enrollment manager.
#[cfg(target_os = "linux")]
fn main() {
    use capyio_core::NodeId;
    use capyio_linux_pairing_store::{
        BackendError, CredentialBackend, LinuxPairingStore, SecretBytes, StoreError,
    };
    use capyio_pairing::CredentialHandle;
    use std::{fs, os::unix::fs::PermissionsExt};
    struct Unused;
    impl CredentialBackend for Unused {
        fn load(&self, _: NodeId, _: &CredentialHandle) -> Result<SecretBytes, BackendError> {
            panic!("backend must not be touched")
        }
        fn put_exact(&self, _: NodeId, _: &CredentialHandle, _: &[u8]) -> Result<(), BackendError> {
            panic!("backend must not be touched")
        }
    }
    let root =
        std::env::temp_dir().join(format!("capyio-pairing-fork-owner-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let owner = NodeId::new();
    let store = LinuxPairingStore::open(&root, owner, Unused).unwrap();
    // SAFETY: this harness-free process is single-threaded. There are no Rust
    // executor/manager threads or concurrently held application locks at fork.
    let pid = unsafe { libc::fork() };
    assert!(pid >= 0, "fork failed");
    if pid == 0 {
        let rejected = matches!(store.load_directory(), Err(StoreError::Busy));
        drop(store); // Must close only, not LOCK_UN the live parent's description.
        std::process::exit(if rejected { 0 } else { 1 });
    }
    let mut status = 0;
    // SAFETY: pid is our exact child and status points to live writable storage.
    assert_eq!(unsafe { libc::waitpid(pid, &mut status, 0) }, pid);
    assert!(libc::WIFEXITED(status));
    assert_eq!(libc::WEXITSTATUS(status), 0);
    assert!(
        matches!(
            LinuxPairingStore::open(&root, owner, Unused),
            Err(StoreError::Busy)
        ),
        "child Drop released live parent lock"
    );
    drop(store);
    let reopened = LinuxPairingStore::open(&root, owner, Unused).unwrap();
    drop(reopened);
    fs::remove_dir_all(root).unwrap();
    println!(
        "fork_owner: inherited operations rejected; child Drop preserves parent lock; parent Drop releases lock"
    );
}
#[cfg(not(target_os = "linux"))]
fn main() {}
