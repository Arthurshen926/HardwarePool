use super::*;
use capyio_pairing::CertificateFingerprint;
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
static SEQ: AtomicU64 = AtomicU64::new(1);
#[derive(Clone, Default)]
struct Fake {
    reads: Arc<AtomicU64>,
    puts: Arc<AtomicU64>,
    entries: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
    error: Arc<Mutex<Option<BackendError>>>,
}
impl CredentialBackend for Fake {
    fn load(&self, owner: NodeId, handle: &CredentialHandle) -> Result<SecretBytes, BackendError> {
        self.reads.fetch_add(1, Ordering::Relaxed);
        if let Some(e) = *self.error.lock().unwrap() {
            return Err(e);
        }
        SecretBytes::new(
            self.entries
                .lock()
                .unwrap()
                .get(&format!("{owner}:{}", handle.as_str()))
                .ok_or(BackendError::Missing)?
                .clone(),
        )
    }
    fn put_exact(
        &self,
        owner: NodeId,
        handle: &CredentialHandle,
        bytes: &[u8],
    ) -> Result<(), BackendError> {
        self.puts.fetch_add(1, Ordering::Relaxed);
        if let Some(e) = *self.error.lock().unwrap() {
            return Err(e);
        }
        let mut entries = self.entries.lock().unwrap();
        let key = format!("{owner}:{}", handle.as_str());
        if let Some(existing) = entries.get(&key)
            && existing != bytes
        {
            return Err(BackendError::Conflict);
        }
        entries.insert(key, bytes.to_vec());
        Ok(())
    }
}
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "capyio-linux-pairing-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn fixture(owner: NodeId, peer: NodeId, marker: u8) -> Vec<u8> {
    let credential = ControlCredentialBundle::new(
        vec![vec![marker]],
        b"SYNTHETIC_PRIVATE_KEY_MARKER".to_vec(),
        b"synthetic ca".to_vec(),
    )
    .unwrap()
    .encode()
    .unwrap();
    let mut directory = PairedPeerDirectory::new(owner).unwrap();
    directory
        .add(
            PairedPeerRecord::new(
                peer,
                "synthetic peer",
                "peer.test",
                CredentialHandle::new(handle_for(Sha256::digest(&credential).into())).unwrap(),
                CertificateFingerprint::from_certificate_der(&[marker]).unwrap(),
                1,
            )
            .unwrap(),
        )
        .unwrap();
    PairingInvitation::new(owner, directory.encode_json().unwrap(), credential)
        .unwrap()
        .encode()
        .unwrap()
}
fn store(t: &Temp, owner: NodeId, backend: Fake) -> LinuxPairingStore<Fake> {
    LinuxPairingStore::open(&t.0, owner, backend).unwrap()
}
#[test]
fn stage_restart_recovery_reuses_exact_key_and_bootstrap_selects_one_peer() {
    let t = Temp::new();
    let owner = NodeId::new();
    let peer = NodeId::new();
    let b = Fake::default();
    let bytes = fixture(owner, peer, 1);
    let mut s = store(&t, owner, b.clone());
    assert!(!s.has_pending_enrollment().unwrap());
    s.stage_enrollment(&bytes).unwrap();
    assert!(s.has_pending_enrollment().unwrap());
    assert!(s.bootstrap(peer).is_err());
    assert_eq!(s.load_directory().unwrap().peers().len(), 0);
    let original = b.entries.lock().unwrap().clone();
    drop(s);
    let mut s = store(&t, owner, b.clone());
    let manager = EnrollmentManager::default();
    assert!(s.recover(&manager).unwrap());
    assert_eq!(manager.status().phase, "recovered");
    assert!(!s.recover(&manager).unwrap());
    assert!(!s.has_pending_enrollment().unwrap());
    let material = s.bootstrap(peer).unwrap();
    assert_eq!(
        PairedPeerDirectory::decode_json(owner, &material.directory_json().unwrap())
            .unwrap()
            .peers()
            .len(),
        1
    );
    assert_eq!(b.entries.lock().unwrap().clone(), original);
    let other = NodeId::new();
    s.stage_enrollment(&fixture(owner, other, 2)).unwrap();
    s.commit_pending_enrollment().unwrap();
    assert_eq!(s.load_directory().unwrap().peers().len(), 2);
    assert_eq!(
        PairedPeerDirectory::decode_json(
            owner,
            &s.bootstrap(peer).unwrap().directory_json().unwrap()
        )
        .unwrap()
        .peers()
        .len(),
        1
    );
    for entry in fs::read_dir(&t.0).unwrap() {
        let bytes = fs::read(entry.unwrap().path()).unwrap();
        assert!(
            !bytes
                .windows(b"SYNTHETIC_PRIVATE_KEY_MARKER".len())
                .any(|w| w == b"SYNTHETIC_PRIVATE_KEY_MARKER")
        );
    }
}
#[test]
fn unavailable_locked_missing_duplicate_and_replaced_credentials_fail_closed() {
    let t = Temp::new();
    let owner = NodeId::new();
    let peer = NodeId::new();
    let b = Fake::default();
    let mut s = store(&t, owner, b.clone());
    for error in [
        BackendError::Unavailable,
        BackendError::Locked,
        BackendError::Conflict,
    ] {
        *b.error.lock().unwrap() = Some(error);
        assert!(s.stage_enrollment(&fixture(owner, peer, 1)).is_err());
    }
    *b.error.lock().unwrap() = None;
    s.stage_enrollment(&fixture(owner, peer, 1)).unwrap();
    let original = b.entries.lock().unwrap().clone();
    b.entries.lock().unwrap().clear();
    assert!(s.commit_pending_enrollment().is_err());
    *b.entries.lock().unwrap() = original;
    s.commit_pending_enrollment().unwrap();
    for value in b.entries.lock().unwrap().values_mut() {
        *value = ControlCredentialBundle::new(vec![vec![1]], vec![2], vec![3])
            .unwrap()
            .encode()
            .unwrap();
    }
    assert!(s.bootstrap(peer).is_err());
}
#[test]
fn wrong_owner_different_pin_and_pending_peer_cannot_replace_approval() {
    let t = Temp::new();
    let owner = NodeId::new();
    let peer = NodeId::new();
    let b = Fake::default();
    let mut s = store(&t, owner, b.clone());
    assert!(
        s.stage_enrollment(&fixture(NodeId::new(), peer, 1))
            .is_err()
    );
    s.stage_enrollment(&fixture(owner, peer, 1)).unwrap();
    assert!(s.stage_enrollment(&fixture(owner, peer, 2)).is_err());
    assert!(
        s.stage_enrollment(&fixture(owner, NodeId::new(), 3))
            .is_err()
    );
    s.commit_pending_enrollment().unwrap();
    assert!(s.stage_enrollment(&fixture(owner, peer, 2)).is_err());
    drop(s);
    assert!(store(&t, NodeId::new(), b).load_directory().is_err());
}
#[test]
fn revoked_peer_and_duplicate_handle_are_rejected_before_secret_read() {
    let t = Temp::new();
    let owner = NodeId::new();
    let peer = NodeId::new();
    let b = Fake::default();
    let mut s = store(&t, owner, b.clone());
    s.stage_enrollment(&fixture(owner, peer, 1)).unwrap();
    s.commit_pending_enrollment().unwrap();
    s.revoke(peer, 2).unwrap();
    let reads = b.reads.load(Ordering::Relaxed);
    *b.error.lock().unwrap() = Some(BackendError::Locked);
    assert!(matches!(s.bootstrap(peer), Err(StoreError::Inactive)));
    assert!(s.stage_enrollment(&fixture(owner, peer, 1)).is_err());
    assert!(
        s.stage_enrollment(&fixture(owner, NodeId::new(), 1))
            .is_err()
    );
    assert_eq!(b.reads.load(Ordering::Relaxed), reads);
}
#[test]
fn corrupt_oversize_journal_and_symlinks_fail_closed() {
    let t = Temp::new();
    let owner = NodeId::new();
    let mut s = store(&t, owner, Fake::default());
    for bytes in [b"{}".to_vec(), vec![b' '; MAX_DOCUMENT + 1]] {
        fs::write(t.0.join(JOURNAL), bytes).unwrap();
        fs::set_permissions(t.0.join(JOURNAL), fs::Permissions::from_mode(0o600)).unwrap();
        assert!(s.has_pending_enrollment().is_err());
        assert!(s.commit_pending_enrollment().is_err());
        assert!(s.bootstrap(NodeId::new()).is_err());
    }
    fs::remove_file(t.0.join(JOURNAL)).unwrap();
    symlink("/dev/null", t.0.join(JOURNAL)).unwrap();
    assert!(s.commit_pending_enrollment().is_err());
    let alias = t.0.with_extension("link");
    symlink(&t.0, &alias).unwrap();
    assert!(LinuxPairingStore::open(&alias, owner, Fake::default()).is_err());
    fs::remove_file(alias).unwrap();
}
#[test]
fn exclusive_owner_rejects_concurrent_open_and_releases_on_drop() {
    let t = Temp::new();
    let owner = NodeId::new();
    let s = store(&t, owner, Fake::default());
    let root = t.0.clone();
    assert!(
        std::thread::spawn(move || matches!(
            LinuxPairingStore::open(&root, owner, Fake::default()),
            Err(StoreError::Busy)
        ))
        .join()
        .unwrap()
    );
    drop(s);
    store(&t, owner, Fake::default());
}
#[test]
fn crash_after_directory_commit_before_journal_removal_is_idempotent() {
    let t = Temp::new();
    let owner = NodeId::new();
    let peer = NodeId::new();
    let b = Fake::default();
    let mut s = store(&t, owner, b.clone());
    s.stage_enrollment(&fixture(owner, peer, 1)).unwrap();
    let (pending, digest) = s.read(JOURNAL, true).unwrap().unwrap();
    s.write(DIRECTORY, &pending, digest).unwrap();
    drop(s);
    let mut s = store(&t, owner, b);
    assert_eq!(
        s.commit_pending_enrollment().unwrap().unwrap().node_id,
        peer
    );
    assert!(s.commit_pending_enrollment().unwrap().is_none());
}
#[test]
fn metadata_failure_never_deletes_protected_credential() {
    let t = Temp::new();
    let owner = NodeId::new();
    let peer = NodeId::new();
    let b = Fake::default();
    let mut s = store(&t, owner, b.clone());
    // Invalid metadata is rejected before protected writes. A later commit
    // failure keeps the prepared credential and journal for exact retry.
    fs::create_dir(t.0.join(JOURNAL)).unwrap();
    assert!(s.stage_enrollment(&fixture(owner, peer, 1)).is_err());
    assert!(b.entries.lock().unwrap().is_empty());
    fs::remove_dir(t.0.join(JOURNAL)).unwrap();
    s.stage_enrollment(&fixture(owner, peer, 1)).unwrap();
    let original = b.entries.lock().unwrap().clone();
    fs::create_dir(t.0.join(DIRECTORY)).unwrap();
    assert!(s.commit_pending_enrollment().is_err());
    assert_eq!(*b.entries.lock().unwrap(), original);
    assert!(t.0.join(JOURNAL).exists());
    fs::remove_dir(t.0.join(DIRECTORY)).unwrap();
    s.commit_pending_enrollment().unwrap();
}
#[test]
fn idle_manager_poll_does_not_create_credentials_or_files() {
    let t = Temp::new();
    let b = Fake::default();
    let mut s = store(&t, NodeId::new(), b.clone());
    assert!(!s.poll_enrollment(&EnrollmentManager::default()).unwrap());
    assert!(b.entries.lock().unwrap().is_empty());
    assert_eq!(fs::read_dir(&t.0).unwrap().count(), 1);
}
#[test]
fn full_directory_rejects_stage_without_pending_journal_or_new_secret() {
    let t = Temp::new();
    let owner = NodeId::new();
    let b = Fake::default();
    let mut s = store(&t, owner, b.clone());
    let mut directory = PairedPeerDirectory::new(owner).unwrap();
    let mut bindings = Bindings::new();
    for marker in 1..=capyio_pairing::MAX_PAIRED_PEERS {
        let inv =
            PairingInvitation::decode_for(owner, &fixture(owner, NodeId::new(), marker as u8))
                .unwrap();
        let offered = PairedPeerDirectory::decode_json(owner, inv.directory_json()).unwrap();
        let peer = offered.peers().next().unwrap().clone();
        bindings.insert(
            peer.credential_handle.as_str().to_owned(),
            Sha256::digest(inv.credential_bundle()).into(),
        );
        directory.add(peer).unwrap();
    }
    s.write(DIRECTORY, &directory, bindings).unwrap();
    assert!(
        s.stage_enrollment(&fixture(owner, NodeId::new(), 100))
            .is_err()
    );
    assert!(!t.0.join(JOURNAL).exists());
    assert!(b.entries.lock().unwrap().is_empty());
}
#[test]
fn manager_bilateral_preparation_commits_and_bootstraps_both_sides() {
    use std::{
        collections::BTreeSet,
        thread,
        time::{Duration, Instant},
    };
    let a_root = Temp::new();
    let b_root = Temp::new();
    let a_id = NodeId::new();
    let b_id = NodeId::new();
    let mut a_store = store(&a_root, a_id, Fake::default());
    let mut b_store = store(&b_root, b_id, Fake::default());
    let mut a = EnrollmentManager::default();
    let mut b = EnrollmentManager::default();
    let hosted = b
        .welcome(
            b_id,
            "synthetic B".into(),
            "127.0.0.1".parse().unwrap(),
            0,
            BTreeSet::new(),
            HANDLE_PREFIX,
        )
        .unwrap();
    a.join(
        a_id,
        "synthetic A".into(),
        hosted.endpoint.unwrap().parse().unwrap(),
        hosted.pin.unwrap(),
        BTreeSet::new(),
        HANDLE_PREFIX,
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while a.status().phase != "confirm" || b.status().phase != "confirm" {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    a.approve().unwrap();
    b.approve().unwrap();
    while a.status().phase != "complete" || b.status().phase != "complete" {
        assert!(
            Instant::now() < deadline,
            "a={} b={}",
            a.status().phase,
            b.status().phase
        );
        a_store.poll_enrollment(&a).unwrap();
        b_store.poll_enrollment(&b).unwrap();
        thread::sleep(Duration::from_millis(10));
    }
    assert!(a_store.bootstrap(b_id).is_ok());
    assert!(b_store.bootstrap(a_id).is_ok());
}
#[test]
fn process_lock_child() {
    let Some(root) = std::env::var_os("CAPYIO_TEST_LOCK_ROOT") else {
        return;
    };
    let owner = std::env::var("CAPYIO_TEST_LOCK_OWNER")
        .unwrap()
        .parse()
        .unwrap();
    let _store = LinuxPairingStore::open(Path::new(&root), owner, Fake::default()).unwrap();
    fs::write(Path::new(&root).join("child-ready"), b"ready").unwrap();
    std::thread::sleep(std::time::Duration::from_secs(30));
}
#[test]
fn process_death_releases_os_lock_without_deleting_credentials_or_metadata() {
    let t = Temp::new();
    let owner = NodeId::new();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "tests::process_lock_child", "--nocapture"])
        .env("CAPYIO_TEST_LOCK_ROOT", &t.0)
        .env("CAPYIO_TEST_LOCK_OWNER", owner.to_string())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !t.0.join("child-ready").exists() {
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("child lock readiness timeout");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let blocked = matches!(
        LinuxPairingStore::open(&t.0, owner, Fake::default()),
        Err(StoreError::Busy)
    );
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(blocked);
    store(&t, owner, Fake::default());
}
#[test]
fn missing_committed_credential_is_never_recreated_from_replayed_invitation() {
    let t = Temp::new();
    let owner = NodeId::new();
    let peer = NodeId::new();
    let b = Fake::default();
    let mut s = store(&t, owner, b.clone());
    let invitation = fixture(owner, peer, 1);
    s.stage_enrollment(&invitation).unwrap();
    s.commit_pending_enrollment().unwrap();
    b.entries.lock().unwrap().clear();
    assert!(s.stage_enrollment(&invitation).is_err());
    assert!(b.entries.lock().unwrap().is_empty());
    assert!(!t.0.join(JOURNAL).exists());
}
#[test]
fn stale_journal_cannot_resurrect_revoked_peer() {
    let t = Temp::new();
    let owner = NodeId::new();
    let peer = NodeId::new();
    let b = Fake::default();
    let mut s = store(&t, owner, b.clone());
    s.stage_enrollment(&fixture(owner, peer, 1)).unwrap();
    let saved = fs::read(t.0.join(JOURNAL)).unwrap();
    s.commit_pending_enrollment().unwrap();
    s.revoke(peer, 2).unwrap();
    s.files.write(JOURNAL, &saved).unwrap();
    assert!(s.commit_pending_enrollment().is_err());
    assert!(s.bootstrap(peer).is_err());
    assert_eq!(s.load_directory().unwrap().active_peers().count(), 0);
    assert_eq!(b.entries.lock().unwrap().len(), 1);
}
#[test]
fn injected_post_put_journal_failure_preserves_orphan_and_exact_retry() {
    struct Fault {
        fake: Fake,
        root: PathBuf,
    }
    impl CredentialBackend for Fault {
        fn load(&self, o: NodeId, h: &CredentialHandle) -> Result<SecretBytes, BackendError> {
            self.fake.load(o, h)
        }
        fn put_exact(&self, o: NodeId, h: &CredentialHandle, b: &[u8]) -> Result<(), BackendError> {
            self.fake.put_exact(o, h, b)?;
            fs::create_dir(self.root.join(JOURNAL)).unwrap();
            Ok(())
        }
    }
    let t = Temp::new();
    let owner = NodeId::new();
    let peer = NodeId::new();
    let b = Fake::default();
    let mut s = LinuxPairingStore::open(
        &t.0,
        owner,
        Fault {
            fake: b.clone(),
            root: t.0.clone(),
        },
    )
    .unwrap();
    let invitation = fixture(owner, peer, 1);
    assert!(s.stage_enrollment(&invitation).is_err());
    assert_eq!(b.entries.lock().unwrap().len(), 1);
    assert_eq!(s.load_directory().unwrap().peers().len(), 0);
    drop(s);
    fs::remove_dir(t.0.join(JOURNAL)).unwrap();
    let original = b.entries.lock().unwrap().clone();
    let mut s = store(&t, owner, b.clone());
    s.stage_enrollment(&invitation).unwrap();
    s.commit_pending_enrollment().unwrap();
    assert_eq!(*b.entries.lock().unwrap(), original);
}
#[test]
fn duplicate_binding_keys_and_mismatched_handle_digest_fail_closed() {
    let t = Temp::new();
    let owner = NodeId::new();
    let peer = NodeId::new();
    let mut s = store(&t, owner, Fake::default());
    s.stage_enrollment(&fixture(owner, peer, 1)).unwrap();
    let (directory, bindings) = s.read(JOURNAL, true).unwrap().unwrap();
    let (handle, digest) = bindings.first_key_value().unwrap();
    let duplicate = format!(
        "{{\"version\":1,\"owner\":{},\"directory\":{},\"credential_sha256\":{{{}:{},{}:{}}}}}",
        serde_json::to_string(&owner).unwrap(),
        serde_json::to_string(&String::from_utf8(directory.encode_json().unwrap()).unwrap())
            .unwrap(),
        serde_json::to_string(handle).unwrap(),
        serde_json::to_string(digest).unwrap(),
        serde_json::to_string(handle).unwrap(),
        serde_json::to_string(digest).unwrap()
    );
    s.files.write(JOURNAL, duplicate.as_bytes()).unwrap();
    assert!(s.pending_enrollment().is_err());
    assert!(s.commit_pending_enrollment().is_err());
    s.write(
        JOURNAL,
        &directory,
        Bindings::from([(handle.clone(), [42; 32])]),
    )
    .unwrap();
    assert!(s.commit_pending_enrollment().is_err());
}

#[test]
fn pending_summary_and_confirmed_recovery_are_exact_and_read_only_until_confirmed() {
    let t = Temp::new();
    let owner = NodeId::new();
    let peer = NodeId::new();
    let b = Fake::default();
    let mut s = store(&t, owner, b.clone());
    let manager = EnrollmentManager::default();
    assert!(s.pending_enrollment().unwrap().is_none());
    assert!(!s.recover_confirmed(&manager, peer, [0; 32]).unwrap());
    s.stage_enrollment(&fixture(owner, peer, 1)).unwrap();
    let reads = b.reads.load(Ordering::Relaxed);
    let puts = b.puts.load(Ordering::Relaxed);
    let entries = b.entries.lock().unwrap().clone();
    let summary = s.pending_enrollment().unwrap().unwrap();
    assert_eq!(summary.peer_node_id, peer);
    assert_eq!(summary.display_name, "synthetic peer");
    assert_eq!(b.reads.load(Ordering::Relaxed), reads);
    assert!(matches!(
        s.recover_confirmed(&manager, NodeId::new(), summary.transaction_digest),
        Err(StoreError::Metadata)
    ));
    assert!(matches!(
        s.recover_confirmed(&manager, peer, [0; 32]),
        Err(StoreError::Metadata)
    ));
    assert_eq!(b.reads.load(Ordering::Relaxed), reads);
    assert_eq!(manager.status().phase, "idle");
    assert!(
        s.recover_confirmed(&manager, peer, summary.transaction_digest)
            .unwrap()
    );
    assert_eq!(manager.status().phase, "recovered");
    assert!(
        !s.recover_confirmed(&manager, peer, summary.transaction_digest)
            .unwrap()
    );
    assert_eq!(b.puts.load(Ordering::Relaxed), puts);
    assert_eq!(*b.entries.lock().unwrap(), entries);
}

#[test]
fn changed_exact_journal_bytes_invalidate_confirmation_without_secret_access() {
    let t = Temp::new();
    let owner = NodeId::new();
    let peer = NodeId::new();
    let b = Fake::default();
    let mut s = store(&t, owner, b.clone());
    s.stage_enrollment(&fixture(owner, peer, 1)).unwrap();
    let summary = s.pending_enrollment().unwrap().unwrap();
    let reads = b.reads.load(Ordering::Relaxed);
    let mut bytes = fs::read(t.0.join(JOURNAL)).unwrap();
    bytes.push(b' '); // Semantically identical, but not the confirmed transaction bytes.
    s.files.write(JOURNAL, &bytes).unwrap();
    let changed = s.pending_enrollment().unwrap().unwrap();
    assert_ne!(summary.transaction_digest, changed.transaction_digest);
    assert!(
        s.recover_confirmed(
            &EnrollmentManager::default(),
            peer,
            summary.transaction_digest
        )
        .is_err()
    );
    assert_eq!(b.reads.load(Ordering::Relaxed), reads);
    assert!(s.load_directory().unwrap().peers().next().is_none());
    assert_eq!(fs::read(t.0.join(JOURNAL)).unwrap(), bytes);
}

#[test]
fn confirmed_recovery_backend_failures_preserve_journal_and_never_put() {
    let t = Temp::new();
    let owner = NodeId::new();
    let peer = NodeId::new();
    let b = Fake::default();
    let mut s = store(&t, owner, b.clone());
    s.stage_enrollment(&fixture(owner, peer, 1)).unwrap();
    let summary = s.pending_enrollment().unwrap().unwrap();
    let bytes = fs::read(t.0.join(JOURNAL)).unwrap();
    let entries = b.entries.lock().unwrap().clone();
    let puts = b.puts.load(Ordering::Relaxed);
    for error in [
        BackendError::Locked,
        BackendError::Missing,
        BackendError::Unavailable,
        BackendError::Conflict,
    ] {
        *b.error.lock().unwrap() = Some(error);
        let reads = b.reads.load(Ordering::Relaxed);
        assert_eq!(s.pending_enrollment().unwrap().unwrap(), summary);
        assert_eq!(b.reads.load(Ordering::Relaxed), reads);
        assert!(
            matches!(s.recover_confirmed(&EnrollmentManager::default(), peer, summary.transaction_digest), Err(StoreError::Backend(e)) if e == error)
        );
        assert_eq!(fs::read(t.0.join(JOURNAL)).unwrap(), bytes);
        assert!(s.load_directory().unwrap().peers().next().is_none());
        assert_eq!(b.puts.load(Ordering::Relaxed), puts);
        assert_eq!(*b.entries.lock().unwrap(), entries);
    }
}

#[test]
fn pending_summary_rejects_revoked_conflicting_and_corrupt_metadata_before_reads() {
    let t = Temp::new();
    let owner = NodeId::new();
    let peer = NodeId::new();
    let b = Fake::default();
    let mut s = store(&t, owner, b.clone());
    s.stage_enrollment(&fixture(owner, peer, 1)).unwrap();
    let summary = s.pending_enrollment().unwrap().unwrap();
    let journal = fs::read(t.0.join(JOURNAL)).unwrap();
    s.commit_pending_enrollment().unwrap();
    s.files.write(JOURNAL, &journal).unwrap();
    // The crash window after directory write is a valid idempotent transaction.
    assert_eq!(s.pending_enrollment().unwrap().unwrap(), summary);
    assert!(
        s.recover_confirmed(
            &EnrollmentManager::default(),
            peer,
            summary.transaction_digest
        )
        .unwrap()
    );
    s.files.write(JOURNAL, &journal).unwrap();
    s.revoke(peer, 2).unwrap();
    let reads = b.reads.load(Ordering::Relaxed);
    assert!(s.pending_enrollment().is_err());
    assert!(
        s.recover_confirmed(
            &EnrollmentManager::default(),
            peer,
            summary.transaction_digest
        )
        .is_err()
    );
    assert_eq!(b.reads.load(Ordering::Relaxed), reads);
    // A different pin for the same peer is not the staged approval.
    let other = PairingInvitation::decode_for(owner, &fixture(owner, peer, 2)).unwrap();
    let directory = PairedPeerDirectory::decode_json(owner, other.directory_json()).unwrap();
    let record = directory.peers().next().unwrap();
    s.write(
        DIRECTORY,
        &directory,
        Bindings::from([(
            record.credential_handle.as_str().to_owned(),
            Sha256::digest(other.credential_bundle()).into(),
        )]),
    )
    .unwrap();
    assert!(s.pending_enrollment().is_err());
    assert!(
        s.recover_confirmed(
            &EnrollmentManager::default(),
            peer,
            summary.transaction_digest
        )
        .is_err()
    );
    s.files.write(DIRECTORY, b"{}").unwrap();
    assert!(s.pending_enrollment().is_err());
    assert!(
        s.recover_confirmed(
            &EnrollmentManager::default(),
            peer,
            summary.transaction_digest
        )
        .is_err()
    );
    assert_eq!(b.reads.load(Ordering::Relaxed), reads);
}

#[test]
fn pending_summary_rejects_wrong_owner_duplicate_and_corrupt_journal_before_reads() {
    let t = Temp::new();
    let owner = NodeId::new();
    let peer = NodeId::new();
    let b = Fake::default();
    let mut s = store(&t, owner, b.clone());
    s.stage_enrollment(&fixture(owner, peer, 1)).unwrap();
    let summary = s.pending_enrollment().unwrap().unwrap();
    let bytes = fs::read(t.0.join(JOURNAL)).unwrap();
    let reads = b.reads.load(Ordering::Relaxed);
    let mut wrong_owner: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    wrong_owner["owner"] = serde_json::to_value(NodeId::new()).unwrap();
    let duplicate = String::from_utf8(bytes.clone())
        .unwrap()
        .replacen("{", "{\"version\":1,", 1);
    for invalid in [
        serde_json::to_vec(&wrong_owner).unwrap(),
        duplicate.into_bytes(),
        b"{}".to_vec(),
        vec![b' '; MAX_DOCUMENT],
    ] {
        s.files.write(JOURNAL, &invalid).unwrap();
        assert!(s.pending_enrollment().is_err());
        assert!(
            s.recover_confirmed(
                &EnrollmentManager::default(),
                peer,
                summary.transaction_digest
            )
            .is_err()
        );
        assert_eq!(b.reads.load(Ordering::Relaxed), reads);
    }
    s.files.write(JOURNAL, &bytes).unwrap();
    drop(s);
    let wrong = store(&t, NodeId::new(), b.clone());
    assert!(wrong.pending_enrollment().is_err());
    assert_eq!(b.reads.load(Ordering::Relaxed), reads);
}

#[test]
fn replacement_during_protected_read_is_not_committed_or_removed() {
    struct ReplacesJournal {
        fake: Fake,
        root: PathBuf,
        replacement: Vec<u8>,
    }
    impl CredentialBackend for ReplacesJournal {
        fn load(
            &self,
            owner: NodeId,
            handle: &CredentialHandle,
        ) -> Result<SecretBytes, BackendError> {
            let secret = self.fake.load(owner, handle)?;
            fs::write(self.root.join(JOURNAL), &self.replacement).unwrap();
            Ok(secret)
        }
        fn put_exact(&self, _: NodeId, _: &CredentialHandle, _: &[u8]) -> Result<(), BackendError> {
            panic!("confirmed recovery must never write protected storage")
        }
    }
    let t = Temp::new();
    let owner = NodeId::new();
    let peer = NodeId::new();
    let b = Fake::default();
    let mut s = store(&t, owner, b.clone());
    s.stage_enrollment(&fixture(owner, peer, 1)).unwrap();
    let summary = s.pending_enrollment().unwrap().unwrap();
    let mut replacement = fs::read(t.0.join(JOURNAL)).unwrap();
    replacement.push(b' ');
    drop(s);
    let mut s = LinuxPairingStore::open(
        &t.0,
        owner,
        ReplacesJournal {
            fake: b,
            root: t.0.clone(),
            replacement: replacement.clone(),
        },
    )
    .unwrap();
    assert!(
        s.recover_confirmed(
            &EnrollmentManager::default(),
            peer,
            summary.transaction_digest
        )
        .is_err()
    );
    assert_eq!(fs::read(t.0.join(JOURNAL)).unwrap(), replacement);
    assert!(!t.0.join(DIRECTORY).exists());
}
