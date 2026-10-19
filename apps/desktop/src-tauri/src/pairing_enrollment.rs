use std::{
    env,
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use capyio_control_credentials::ControlCredentialBundle;
use capyio_core::NodeId;
use capyio_pairing::{
    CertificateFingerprint, CredentialHandle, PairedPeerDirectory, PairedPeerRecord,
    PairingInvitation, PairingState,
};
use rcgen::{
    BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose,
};
use sha2::{Digest, Sha256};

use super::DesktopControlPlaneState;

pub(crate) fn create_invitation(
    control: &mut DesktopControlPlaneState,
    local_node_id: NodeId,
    local_display_name: &str,
    target_node_id: NodeId,
    target_display_name: &str,
    renew_existing: bool,
) -> Result<PathBuf, String> {
    if target_node_id == local_node_id || target_node_id.as_uuid().is_nil() {
        return Err("target Node ID must identify another non-nil Node".to_owned());
    }
    if target_display_name.trim().is_empty() || target_display_name.len() > 128 {
        return Err("target display name must contain 1..=128 bytes".to_owned());
    }
    let store = control
        .pairing_store
        .clone()
        .ok_or_else(|| "pairing credential store is unavailable".to_owned())?;

    let issued = issue_identities(local_node_id, target_node_id)?;
    let mut windows_bundle_bytes = issued.windows_bundle.encode().map_err(|e| e.to_string())?;
    let windows_handle = handle("windows-dpapi:", &windows_bundle_bytes)?;
    let mut android_bundle_bytes = issued.android_bundle.encode().map_err(|e| e.to_string())?;
    let android_handle = handle("android-keystore:", &android_bundle_bytes)?;
    let paired_at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "system clock predates Unix epoch".to_owned())?
        .as_millis()
        .try_into()
        .map_err(|_| "system clock exceeds pairing timestamp range".to_owned())?;

    let result = (|| {
        let mut windows_directory = store.load_directory().map_err(|e| e.to_string())?;
        let new_record = PairedPeerRecord::new(
            target_node_id,
            target_display_name.trim(),
            issued.android_name.clone(),
            windows_handle.clone(),
            issued.android_leaf_fingerprint,
            paired_at_ms,
        )
        .map_err(|e| e.to_string())?;
        if renew_existing {
            let existing = windows_directory
                .peers()
                .find(|peer| peer.node_id == target_node_id)
                .ok_or("selected peer does not have an existing pairing to renew")?;
            if !matches!(existing.state, PairingState::Active) {
                return Err("revoked peer cannot be reactivated by pairing renewal".to_owned());
            }
            // A different Windows account may see the public directory but
            // cannot decrypt the original DPAPI bundle. Never let it replace
            // that user's live trust record.
            store
                .load_control_credential(&existing.credential_handle)
                .map_err(|_| "当前账号无法解密旧配对；请用原配对账号更新证书。".to_owned())?;
            windows_directory = replace_peer_record(local_node_id, &windows_directory, new_record)?;
        } else {
            windows_directory
                .add(new_record)
                .map_err(|e| e.to_string())?;
        }

        let mut android_directory =
            PairedPeerDirectory::new(target_node_id).map_err(|e| e.to_string())?;
        android_directory
            .add(
                PairedPeerRecord::new(
                    local_node_id,
                    local_display_name,
                    issued.windows_name,
                    android_handle,
                    issued.windows_leaf_fingerprint,
                    paired_at_ms,
                )
                .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
        let invitation = PairingInvitation::new(
            target_node_id,
            android_directory.encode_json().map_err(|e| e.to_string())?,
            android_bundle_bytes.clone(),
        )
        .map_err(|e| e.to_string())?;
        let mut invitation_bytes = invitation.encode().map_err(|e| e.to_string())?;

        let written = match write_invitation(target_node_id, &invitation_bytes) {
            Ok(path) => path,
            Err(error) => {
                invitation_bytes.fill(0);
                return Err(error);
            }
        };
        if let Err(error) = store.put_credential(&windows_handle, &windows_bundle_bytes) {
            let _ = store.remove_credential(&windows_handle);
            let _ = fs::remove_file(&written);
            invitation_bytes.fill(0);
            return Err(error.to_string());
        }
        if let Err(error) = store.commit_directory(&windows_directory) {
            let _ = store.remove_credential(&windows_handle);
            let _ = fs::remove_file(&written);
            invitation_bytes.fill(0);
            return Err(error.to_string());
        }
        control.attach_pairing_store(store.clone())?;
        invitation_bytes.fill(0);
        Ok(written)
    })();
    windows_bundle_bytes.fill(0);
    android_bundle_bytes.fill(0);
    result
}

fn replace_peer_record(
    local_node_id: NodeId,
    existing: &PairedPeerDirectory,
    record: PairedPeerRecord,
) -> Result<PairedPeerDirectory, String> {
    let Some(previous) = existing.peers().find(|peer| peer.node_id == record.node_id) else {
        return Err("selected peer does not have an existing pairing to renew".to_owned());
    };
    if !matches!(previous.state, PairingState::Active) {
        return Err("revoked peer cannot be reactivated by pairing renewal".to_owned());
    }
    let mut replacement = PairedPeerDirectory::new(local_node_id).map_err(|e| e.to_string())?;
    for peer in existing.peers() {
        if peer.node_id != record.node_id {
            replacement.add(peer.clone()).map_err(|e| e.to_string())?;
        }
    }
    replacement.add(record).map_err(|e| e.to_string())?;
    Ok(replacement)
}

struct IssuedIdentities {
    windows_bundle: ControlCredentialBundle,
    android_bundle: ControlCredentialBundle,
    windows_leaf_fingerprint: CertificateFingerprint,
    android_leaf_fingerprint: CertificateFingerprint,
    windows_name: String,
    android_name: String,
}

fn issue_identities(local: NodeId, target: NodeId) -> Result<IssuedIdentities, String> {
    let mut ca_params = CertificateParams::new(Vec::<String>::new()).map_err(|e| e.to_string())?;
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
    ];
    let ca_key = KeyPair::generate().map_err(|e| e.to_string())?;
    let ca_cert = ca_params.self_signed(&ca_key).map_err(|e| e.to_string())?;
    let ca_der = ca_cert.der().to_vec();
    let issuer = Issuer::new(ca_params, ca_key);
    let windows_name = format!("windows-{local}.capyio.local");
    let android_name = format!("android-{target}.capyio.local");
    // A Node may accept or dial the same paired peer. Bind both TLS roles to
    // the same pinned leaf instead of assigning a global role to a platform.
    let (windows_cert, windows_key) = leaf(&issuer, &windows_name)?;
    let (android_cert, android_key) = leaf(&issuer, &android_name)?;
    let windows_fingerprint = CertificateFingerprint::from_certificate_der(&windows_cert)
        .ok_or_else(|| "issued Windows certificate is invalid".to_owned())?;
    let android_fingerprint = CertificateFingerprint::from_certificate_der(&android_cert)
        .ok_or_else(|| "issued Android certificate is invalid".to_owned())?;
    Ok(IssuedIdentities {
        windows_bundle: ControlCredentialBundle::new(
            vec![windows_cert, ca_der.clone()],
            windows_key,
            ca_der.clone(),
        )
        .map_err(|e| e.to_string())?,
        android_bundle: ControlCredentialBundle::new(
            vec![android_cert, ca_der.clone()],
            android_key,
            ca_der,
        )
        .map_err(|e| e.to_string())?,
        windows_leaf_fingerprint: windows_fingerprint,
        android_leaf_fingerprint: android_fingerprint,
        windows_name,
        android_name,
    })
}

fn leaf(issuer: &Issuer<'static, KeyPair>, name: &str) -> Result<(Vec<u8>, Vec<u8>), String> {
    let mut params = CertificateParams::new(vec![name.to_owned()]).map_err(|e| e.to_string())?;
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    params.extended_key_usages = vec![
        ExtendedKeyUsagePurpose::ClientAuth,
        ExtendedKeyUsagePurpose::ServerAuth,
    ];
    let key = KeyPair::generate().map_err(|e| e.to_string())?;
    let cert = params.signed_by(&key, issuer).map_err(|e| e.to_string())?;
    Ok((cert.der().to_vec(), key.serialize_der()))
}

fn handle(prefix: &str, bytes: &[u8]) -> Result<CredentialHandle, String> {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut hex, "{byte:02x}").map_err(|_| "credential handle encoding failed")?;
    }
    CredentialHandle::new(format!("{prefix}{hex}")).map_err(|e| e.to_string())
}

fn write_invitation(target: NodeId, bytes: &[u8]) -> Result<PathBuf, String> {
    let profile = env::var_os("USERPROFILE")
        .ok_or_else(|| "Windows user profile is unavailable".to_owned())?;
    let downloads = PathBuf::from(profile).join("Downloads");
    for suffix in 0..100_u8 {
        let short = target.to_string()[..8].to_owned();
        let name = if suffix == 0 {
            format!("CapyIO-Pair-{short}.cpi")
        } else {
            format!("CapyIO-Pair-{short}-{suffix}.cpi")
        };
        let path = downloads.join(name);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                let written = file.write_all(bytes).and_then(|_| file.sync_all());
                drop(file);
                if let Err(error) = written {
                    let _ = fs::remove_file(&path);
                    return Err(format!("cannot write invitation: {error}"));
                }
                return Ok(path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("cannot create invitation in Downloads: {error}")),
        }
    }
    Err("pairing invitation filename namespace is exhausted".to_owned())
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeSet,
        io::Write,
        net::{Ipv4Addr, TcpListener},
        thread,
        time::Duration,
    };

    use capyio_control_link::CatalogControlLink;
    use capyio_control_tls::{
        AuthenticatedControlClientChannel, AuthenticatedControlServer, ControlTlsClientIdentity,
        ControlTlsEndpoint, ControlTlsNodeBinding, ControlTlsServerIdentity,
    };

    use super::*;

    #[test]
    fn explicit_pair_renewal_replaces_only_the_selected_peer() {
        let local = NodeId::new();
        let target = NodeId::new();
        let other = NodeId::new();
        let record = |node, name: &str, value: u8| {
            PairedPeerRecord::new(
                node,
                name,
                "peer.capyio.test",
                CredentialHandle::new(format!(
                    "windows-dpapi:{}",
                    format!("{value:02x}").repeat(32)
                ))
                .unwrap(),
                CertificateFingerprint::new([value; 32]).unwrap(),
                100,
            )
            .unwrap()
        };
        let mut original = PairedPeerDirectory::new(local).unwrap();
        original.add(record(target, "old", 1)).unwrap();
        original.add(record(other, "other", 2)).unwrap();
        let updated = replace_peer_record(local, &original, record(target, "new", 3)).unwrap();
        assert_eq!(updated.peers().count(), 2);
        assert_eq!(
            updated
                .peers()
                .find(|peer| peer.node_id == target)
                .unwrap()
                .display_name,
            "new"
        );
        assert_eq!(
            updated
                .peers()
                .find(|peer| peer.node_id == other)
                .unwrap()
                .display_name,
            "other"
        );
        assert_eq!(
            original
                .peers()
                .find(|peer| peer.node_id == target)
                .unwrap()
                .display_name,
            "old"
        );
        assert!(
            replace_peer_record(local, &original, record(NodeId::new(), "unknown", 4)).is_err()
        );
        original.revoke(target, 101).unwrap();
        assert!(replace_peer_record(local, &original, record(target, "resurrect", 5)).is_err());
    }

    #[test]
    fn newly_issued_pair_authenticates_in_both_dial_directions() {
        let windows = NodeId::new();
        let android = NodeId::new();
        let issued = issue_identities(windows, android).expect("issued pair");
        let windows_bundle = issued.windows_bundle.encode().expect("Windows bundle");
        let android_bundle = issued.android_bundle.encode().expect("Android bundle");
        for windows_accepts in [false, true] {
            let (
                server_node,
                client_node,
                server_name,
                server_bundle,
                client_bundle,
                server_fingerprint,
                client_fingerprint,
            ) = if windows_accepts {
                (
                    windows,
                    android,
                    issued.windows_name.as_str(),
                    &windows_bundle,
                    &android_bundle,
                    issued.windows_leaf_fingerprint,
                    issued.android_leaf_fingerprint,
                )
            } else {
                (
                    android,
                    windows,
                    issued.android_name.as_str(),
                    &android_bundle,
                    &windows_bundle,
                    issued.android_leaf_fingerprint,
                    issued.windows_leaf_fingerprint,
                )
            };
            let nodes = ControlTlsNodeBinding::new(server_node, client_node).expect("Node binding");
            let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("loopback");
            let address = listener.local_addr().expect("address");
            let server = AuthenticatedControlServer::from_listener_fresh_session(
                listener,
                ControlTlsServerIdentity::from_bundle(
                    client_fingerprint,
                    ControlCredentialBundle::decode(server_bundle).expect("server identity"),
                ),
                nodes,
                Duration::from_secs(5),
            )
            .expect("server");
            let worker = thread::spawn(move || server.accept().expect("accepted peer").binding());
            let client = AuthenticatedControlClientChannel::connect_fresh_session(
                ControlTlsEndpoint {
                    address,
                    io_timeout: Duration::from_secs(5),
                },
                server_name,
                ControlTlsClientIdentity::from_bundle(
                    server_fingerprint,
                    ControlCredentialBundle::decode(client_bundle).expect("client identity"),
                ),
                nodes,
            )
            .expect("dialed peer");
            assert_eq!(client.binding(), worker.join().expect("accept worker"));
            assert_eq!(client.binding().server_node_id, server_node);
            assert_eq!(client.binding().client_node_id, client_node);
        }
    }

    #[test]
    fn windows_inbound_gate_ignores_untrusted_handshake_then_exchanges_catalog() {
        let windows = capyio_testkit::windows_node();
        let android = capyio_testkit::android_node();
        let windows_id = windows.id;
        let android_id = android.id;
        let issued = issue_identities(windows_id, android_id).expect("issued pair");
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("loopback");
        let address = listener.local_addr().expect("address");
        listener.set_nonblocking(true).expect("bounded accept");
        let server = AuthenticatedControlServer::from_listener_fresh_session(
            listener,
            ControlTlsServerIdentity::from_bundle(
                issued.android_leaf_fingerprint,
                ControlCredentialBundle::decode(
                    &issued.windows_bundle.encode().expect("Windows bundle"),
                )
                .expect("Windows identity"),
            ),
            ControlTlsNodeBinding::new(windows_id, android_id).expect("Node binding"),
            Duration::from_secs(2),
        )
        .expect("server");
        let windows_worker = thread::spawn(move || {
            let mut runtime = capyio_runtime::NodeRuntime::new(windows).expect("Windows runtime");
            crate::paired_control_listener::accept_and_exchange(
                &server,
                &mut runtime,
                android_id,
                "Android",
                Duration::from_secs(3),
            )
            .expect("inbound catalog")
        });
        // A random TCP caller cannot consume the one-pair accept gate.
        let mut untrusted = std::net::TcpStream::connect(address).expect("untrusted socket");
        untrusted.write_all(b"not TLS").expect("junk handshake");
        drop(untrusted);
        let channel = AuthenticatedControlClientChannel::connect_fresh_session(
            ControlTlsEndpoint {
                address,
                io_timeout: Duration::from_secs(2),
            },
            &issued.windows_name,
            ControlTlsClientIdentity::from_bundle(
                issued.windows_leaf_fingerprint,
                ControlCredentialBundle::decode(
                    &issued.android_bundle.encode().expect("Android bundle"),
                )
                .expect("Android identity"),
            ),
            ControlTlsNodeBinding::new(windows_id, android_id).expect("Node binding"),
        )
        .expect("Android dials Windows");
        let mut android_runtime =
            capyio_runtime::NodeRuntime::new(android).expect("Android runtime");
        let mut link = CatalogControlLink::admit_dialed_peer(
            &mut android_runtime,
            channel,
            "Windows".to_owned(),
        )
        .expect("dialed Session");
        let outgoing = link
            .prepare_catalog(&android_runtime, &BTreeSet::new())
            .expect("Android catalog");
        link.send_catalog(outgoing).expect("send catalog");
        let incoming = link.receive_catalog().expect("Windows catalog");
        link.apply_catalog(&mut android_runtime, incoming)
            .expect("apply catalog");
        link.send_heartbeat(1, false).expect("heartbeat request");
        let ack = link.receive_heartbeat().expect("heartbeat acknowledgement");
        assert!(ack.acknowledgement);
        assert_eq!(ack.sequence, 1);
        assert_eq!(windows_worker.join().expect("listener worker"), 2);
        link.close_and_reconcile(&mut android_runtime)
            .expect("cleanup");
    }

    #[test]
    fn paired_inbound_link_keeps_bidirectional_heartbeats_after_catalog() {
        use capyio_control_link::ReceivedControlMessage;

        let windows = capyio_testkit::windows_node();
        let android = capyio_testkit::android_node();
        let windows_id = windows.id;
        let android_id = android.id;
        let issued = issue_identities(windows.id, android.id).expect("issued pair");
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("loopback");
        let address = listener.local_addr().expect("address");
        let server = AuthenticatedControlServer::from_listener_fresh_session(
            listener,
            ControlTlsServerIdentity::from_bundle(
                issued.android_leaf_fingerprint,
                issued.windows_bundle,
            ),
            ControlTlsNodeBinding::new(windows_id, android_id).expect("Node binding"),
            Duration::from_secs(3),
        )
        .expect("server");
        let windows_worker = thread::spawn(move || {
            let mut runtime = capyio_runtime::NodeRuntime::new(windows).expect("runtime");
            let channel = server.accept().expect("accepted channel");
            assert!(
                channel
                    .peer_addr()
                    .expect("peer endpoint")
                    .ip()
                    .is_loopback()
            );
            let mut link = CatalogControlLink::admit_accepted_peer(
                &mut runtime,
                channel,
                "Android".to_owned(),
            )
            .expect("admitted");
            let incoming = link.receive_catalog().expect("Android catalog");
            link.apply_catalog(&mut runtime, incoming)
                .expect("apply catalog");
            let outgoing = link
                .prepare_catalog(&runtime, &BTreeSet::new())
                .expect("catalog");
            link.send_catalog(outgoing).expect("send catalog");
            for sequence in 1..=3 {
                link.send_heartbeat(sequence, false)
                    .expect("Windows heartbeat");
                let request = link.receive_heartbeat().expect("Android request");
                assert!(!request.acknowledgement && request.sequence == sequence);
                link.send_heartbeat(sequence, true)
                    .expect("Android acknowledgement");
                let response = link.receive_heartbeat().expect("Windows response");
                assert!(response.acknowledgement && response.sequence == sequence);
            }
            link.close_and_reconcile(&mut runtime).expect("cleanup");
        });
        let channel = AuthenticatedControlClientChannel::connect_fresh_session(
            ControlTlsEndpoint {
                address,
                io_timeout: Duration::from_secs(3),
            },
            &issued.windows_name,
            ControlTlsClientIdentity::from_bundle(
                issued.windows_leaf_fingerprint,
                issued.android_bundle,
            ),
            ControlTlsNodeBinding::new(windows_id, android_id).expect("Node binding"),
        )
        .expect("Android dials");
        let mut runtime = capyio_runtime::NodeRuntime::new(android).expect("runtime");
        let mut link =
            CatalogControlLink::admit_dialed_peer(&mut runtime, channel, "Windows".to_owned())
                .expect("admitted");
        let outgoing = link
            .prepare_catalog(&runtime, &BTreeSet::new())
            .expect("catalog");
        link.send_catalog(outgoing).expect("send catalog");
        let incoming = link.receive_catalog().expect("Windows catalog");
        link.apply_catalog(&mut runtime, incoming)
            .expect("apply catalog");
        for sequence in 1..=3 {
            link.send_heartbeat(sequence, false)
                .expect("Android heartbeat");
            let request = link.receive_control_message().expect("Windows request");
            let ReceivedControlMessage::Heartbeat(request) = request else {
                panic!("expected Windows heartbeat");
            };
            assert!(!request.acknowledgement && request.sequence == sequence);
            link.send_heartbeat(sequence, true)
                .expect("Windows acknowledgement");
            let response = link.receive_heartbeat().expect("Android response");
            assert!(response.acknowledgement && response.sequence == sequence);
        }
        link.close_and_reconcile(&mut runtime).expect("cleanup");
        windows_worker.join().expect("Windows worker");
    }
}
