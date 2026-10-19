//! Explicit lab peer: independent identity and DPAPI store, never the desktop store.
use capyio_core::NodeId;
use capyio_enrollment::EnrollmentManager;
use capyio_windows_pairing_store::WindowsPairingStore;
use std::{
    collections::BTreeSet,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 4 {
        return Err("usage: enrollment_lab <endpoint> <PIN|welcome> <expected-lab-node-id> <new-store-directory>".into());
    }
    let root = PathBuf::from(&args[3]);
    let recovering = args[1] == "recover";
    if !recovering && root.exists() {
        return Err("lab requires a new independent store directory".into());
    }
    let id = if recovering {
        args[2].parse().map_err(|_| "invalid lab Node ID")?
    } else {
        NodeId::new()
    };
    let store = WindowsPairingStore::from_local_app_data(&root, id).map_err(|e| e.to_string())?;
    let mut manager = EnrollmentManager::default();
    let endpoint: std::net::SocketAddr = args[0].parse().map_err(|_| "invalid endpoint")?;
    if recovering {
        store.commit_pending_enrollment()?;
        return check_control(&store, id, endpoint);
    }
    if args[1] == "welcome" || args[1] == "welcome-interrupt" {
        let status = manager.welcome(
            id,
            "CapyIO Windows pairing lab".into(),
            endpoint.ip(),
            endpoint.port(),
            BTreeSet::new(),
            "windows-dpapi:",
        )?;
        println!("lab_pin={}", status.pin.unwrap_or_default());
    } else {
        manager.join(
            id,
            "CapyIO Windows pairing lab".into(),
            endpoint,
            args[1].clone(),
            BTreeSet::new(),
            "windows-dpapi:",
        )?;
    }
    println!("lab_node_id={id}");
    let deadline = Instant::now() + Duration::from_secs(180);
    let mut previous = String::new();
    loop {
        let status = manager.status();
        if status.phase != previous {
            println!("phase={} message={}", status.phase, status.message);
            previous = status.phase.clone();
        }
        match status.phase.as_str() {
            "confirm" => {
                if status.peer_node_id.as_deref() != Some(args[2].as_str()) {
                    manager.cancel();
                    return Err("unexpected lab peer; not approved".into());
                }
                // The caller explicitly selected this independently verified test Node ID.
                manager.approve()?;
            }
            "preparing" => {
                manager.stage_with(|bytes| {
                    store.stage_enrollment(bytes)?;
                    if args[1] == "welcome-interrupt" {
                        println!("lab_fault=exit_after_durable_prepare_before_ack");
                        thread::sleep(Duration::from_secs(3));
                        std::process::exit(0);
                    }
                    Ok(())
                })?;
            }
            "saving" => {
                manager.persist_with(|_| store.commit_pending_enrollment().map(|_| ()))?;
            }
            "complete" => {
                if let (Some(host), Some(port)) =
                    (&status.peer_control_host, status.peer_control_port)
                {
                    let endpoint = std::net::SocketAddr::new(
                        host.parse().map_err(|_| "invalid handoff host")?,
                        port,
                    );
                    check_control(&store, id, endpoint)?;
                }
                return Ok(());
            }
            "failed" | "cancelled" => return Err(status.message),
            _ => (),
        }
        if Instant::now() > deadline {
            manager.cancel();
            return Err("lab timeout".into());
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn check_control(
    store: &WindowsPairingStore,
    id: NodeId,
    endpoint: std::net::SocketAddr,
) -> Result<(), String> {
    use capyio_control_tls::{
        AuthenticatedControlClientChannel, ControlTlsClientIdentity, ControlTlsEndpoint,
        ControlTlsNodeBinding,
    };
    let directory = store.load_directory().map_err(|e| e.to_string())?;
    let peer = directory.peers().next().ok_or("missing saved peer")?;
    let bundle = store
        .load_control_credential(&peer.credential_handle)
        .map_err(|e| e.to_string())?;
    let identity = ControlTlsClientIdentity::from_bundle(peer.peer_certificate_sha256, bundle);
    let binding = ControlTlsNodeBinding::new(peer.node_id, id).map_err(|e| e.to_string())?;
    let _channel = AuthenticatedControlClientChannel::connect_fresh_session(
        ControlTlsEndpoint {
            address: endpoint,
            io_timeout: Duration::from_secs(5),
        },
        &peer.server_name,
        identity,
        binding,
    )
    .map_err(|e| e.to_string())?;
    println!("control_tls_handoff=authenticated endpoint={endpoint}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prepared_record_survives_store_reopen_and_rejects_changed_peer() {
        use capyio_enrollment::LocalIdentity;
        use capyio_pairing::{
            CredentialHandle, PairedPeerDirectory, PairedPeerRecord, PairingInvitation,
        };
        let id = NodeId::new();
        let root = std::env::temp_dir().join(format!("capyio-enrollment-recovery-{id}"));
        let a = LocalIdentity::generate(id, "A".into()).unwrap();
        let b = LocalIdentity::generate(NodeId::new(), "B".into()).unwrap();
        let bundle = a.credential_for(&b.hello).unwrap().encode().unwrap();
        let peer = PairedPeerRecord::new(
            b.hello.node_id,
            "B",
            b.hello.server_name(),
            CredentialHandle::new(format!("windows-dpapi:{}", "1".repeat(64))).unwrap(),
            b.hello.fingerprint().unwrap(),
            1,
        )
        .unwrap();
        let mut offered = PairedPeerDirectory::new(id).unwrap();
        offered.add(peer.clone()).unwrap();
        let invitation = PairingInvitation::new(id, offered.encode_json().unwrap(), bundle)
            .unwrap()
            .encode()
            .unwrap();
        let store = WindowsPairingStore::from_local_app_data(&root, id).unwrap();
        store.stage_enrollment(&invitation).unwrap();
        assert_eq!(store.load_directory().unwrap().peers().count(), 0);
        let reopened = WindowsPairingStore::from_local_app_data(&root, id).unwrap();
        assert_eq!(
            reopened.commit_pending_enrollment().unwrap(),
            Some(peer.clone())
        );
        assert!(reopened.commit_pending_enrollment().unwrap().is_none());
        reopened.stage_enrollment(&invitation).unwrap();
        let mut changed = peer.clone();
        changed.display_name = "different record".into();
        let mut directory = PairedPeerDirectory::new(id).unwrap();
        directory.add(changed.clone()).unwrap();
        reopened.commit_directory(&directory).unwrap();
        assert!(reopened.commit_pending_enrollment().is_err());
        assert_eq!(
            reopened.load_directory().unwrap().peers().next(),
            Some(&changed)
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
