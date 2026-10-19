use super::*;
use capyio_pairing::{PairedPeerRecord, PairingInvitation};

impl WindowsPairingStore {
    fn pending_path(&self) -> PathBuf {
        self.root
            .join(format!("pending-enrollment-{}.json", self.local_node_id))
    }

    pub fn pending_enrollment(&self) -> Result<Option<PairedPeerDirectory>, String> {
        let Some(bytes) =
            read_bounded_optional(&self.pending_path(), MAX_PAIRED_DIRECTORY_JSON_BYTES)
                .map_err(|e| e.to_string())?
        else {
            return Ok(None);
        };
        let directory = PairedPeerDirectory::decode_json(self.local_node_id, &bytes)
            .map_err(|e| e.to_string())?;
        if directory.peers().count() != 1 {
            return Err("invalid pending enrollment peer count".into());
        }
        Ok(Some(directory))
    }

    /// Called only after both users approved the authenticated enrollment transcript.
    pub fn stage_enrollment(&self, bytes: &[u8]) -> Result<(), String> {
        let invitation =
            PairingInvitation::decode_for(self.local_node_id, bytes).map_err(|e| e.to_string())?;
        let offered =
            PairedPeerDirectory::decode_json(self.local_node_id, invitation.directory_json())
                .map_err(|e| e.to_string())?;
        if offered.peers().count() != 1 {
            return Err("invalid enrollment peer count".into());
        }
        let peer = offered.peers().next().ok_or("missing enrollment peer")?;
        if let Some(pending) = self.pending_enrollment()?
            && pending.peers().ne(offered.peers())
        {
            return Err("请先恢复尚未完成的配对，不会覆盖恢复记录".into());
        }
        let directory = self.load_directory().map_err(|e| e.to_string())?;
        if directory
            .peers()
            .any(|p| p.node_id == peer.node_id && p != peer)
        {
            return Err("现有配对与恢复记录不同，未覆盖证书".into());
        }
        let credential = ControlCredentialBundle::decode(invitation.credential_bundle())
            .map_err(|e| e.to_string())?;
        match self.load_credential(&peer.credential_handle) {
            Ok(mut stored) => {
                let matches = stored == invitation.credential_bundle();
                stored.fill(0);
                if !matches {
                    return Err("stored enrollment credential differs".into());
                }
            }
            Err(PairingStoreError::CredentialNotFound) => self
                .put_control_credential(&peer.credential_handle, &credential)
                .map_err(|e| e.to_string())?,
            Err(e) => return Err(e.to_string()),
        }
        atomic_write(
            &self.pending_path(),
            &offered.encode_json().map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())
    }

    /// Idempotent replay uses the original protected key; never replaces a different pin.
    pub fn commit_pending_enrollment(&self) -> Result<Option<PairedPeerRecord>, String> {
        let Some(pending) = self.pending_enrollment()? else {
            return Ok(None);
        };
        let peer = pending
            .peers()
            .next()
            .ok_or("missing pending peer")?
            .clone();
        self.load_control_credential(&peer.credential_handle)
            .map_err(|e| e.to_string())?;
        let mut directory = self.load_directory().map_err(|e| e.to_string())?;
        let existing = directory
            .peers()
            .find(|p| p.node_id == peer.node_id)
            .cloned();
        match existing {
            Some(record) if record == peer => (),
            Some(_) => return Err("现有配对与恢复记录不同，未覆盖证书".into()),
            None => directory.add(peer.clone()).map_err(|e| e.to_string())?,
        }
        self.commit_directory(&directory)
            .map_err(|e| e.to_string())?;
        fs::remove_file(self.pending_path()).map_err(|e| e.to_string())?;
        Ok(Some(peer))
    }
}
