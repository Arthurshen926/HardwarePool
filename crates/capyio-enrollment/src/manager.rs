use crate::{
    LocalIdentity, authenticate,
    exchange::{receive, send},
};
use capyio_core::NodeId;
use capyio_pairing::{CredentialHandle, PairedPeerDirectory, PairedPeerRecord, PairingInvitation};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    io::{self, Read, Write},
    net::{IpAddr, Shutdown, SocketAddr, TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const WINDOW: Duration = Duration::from_secs(180);

fn announcement_ip(host: IpAddr, announce: bool) -> Option<std::net::Ipv4Addr> {
    match host {
        IpAddr::V4(ip) if announce && !ip.is_loopback() => Some(ip),
        _ => None,
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrollmentStatus {
    pub phase: String,
    pub pin: Option<String>,
    pub endpoint: Option<String>,
    pub peer_node_id: Option<String>,
    pub peer_name: Option<String>,
    pub peer_control_host: Option<String>,
    pub peer_control_port: Option<u16>,
    pub message: String,
    pub discovery_state: String,
    pub discovery_message: String,
    pub candidates: Vec<crate::EnrollmentCandidate>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use capyio_control_credentials::ControlCredentialBundle;

    #[test]
    fn approval_exchange_observes_close_without_local_consent() {
        for peer in [b"".as_slice(), b"\0\0\0\x09approve/1".as_slice()] {
            let mut channel = ApprovalFixture::new(peer);
            assert!(exchange_approvals(&mut channel, || Ok(false)).is_err());
            assert!(channel.sent.is_empty());
        }
    }

    #[test]
    fn approval_exchange_preserves_partial_frame_and_requires_local_consent() {
        let mut channel = ApprovalFixture::new(b"\0\0\0\x09approve/1");
        channel.timeout_between_bytes = true;
        let mut polls = 0;
        exchange_approvals(&mut channel, || {
            polls += 1;
            Ok(polls >= 26)
        })
        .unwrap();
        assert_eq!(channel.sent, b"\0\0\0\x09approve/1");
        assert!(polls >= 26);
    }

    #[test]
    fn approval_exchange_rejects_invalid_frame_and_local_cancellation() {
        let mut channel = ApprovalFixture::new(b"\0\0\0\x09reject!/1");
        assert!(exchange_approvals(&mut channel, || Ok(true)).is_err());
        let mut channel = ApprovalFixture::new(b"\0\0\0\x09approve/1");
        assert!(exchange_approvals(&mut channel, || Err("cancelled".into())).is_err());
        assert!(channel.sent.is_empty());
    }

    struct ApprovalFixture<'a> {
        peer: &'a [u8],
        sent: Vec<u8>,
        timeout_between_bytes: bool,
        timeout_next: bool,
    }
    impl<'a> ApprovalFixture<'a> {
        fn new(peer: &'a [u8]) -> Self {
            Self {
                peer,
                sent: Vec::new(),
                timeout_between_bytes: false,
                timeout_next: false,
            }
        }
    }
    impl Read for ApprovalFixture<'_> {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            if self.timeout_next {
                self.timeout_next = false;
                return Err(io::ErrorKind::TimedOut.into());
            }
            self.timeout_next = self.timeout_between_bytes;
            self.peer.read(bytes)
        }
    }
    impl Write for ApprovalFixture<'_> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.sent.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn welcome_announcement_policy_preserves_default_and_disables_manual() {
        let lan: IpAddr = "192.0.2.1".parse().unwrap();
        assert_eq!(
            announcement_ip(lan, true),
            Some("192.0.2.1".parse().unwrap())
        );
        assert_eq!(announcement_ip(lan, false), None);
        for host in ["127.0.0.1", "::1", "2001:db8::1"] {
            assert_eq!(announcement_ip(host.parse().unwrap(), true), None);
            assert_eq!(announcement_ip(host.parse().unwrap(), false), None);
        }
    }

    #[test]
    fn pin_pairing_requires_both_approvals_and_keeps_keys_local() {
        let a_id = NodeId::new();
        let b_id = NodeId::new();
        let mut a = EnrollmentManager::default();
        let mut b = EnrollmentManager::default();
        let hosted = b
            .welcome(
                b_id,
                "Tablet".into(),
                "127.0.0.1".parse().unwrap(),
                0,
                BTreeSet::new(),
                "android-keystore:",
            )
            .unwrap();
        a.join(
            a_id,
            "Windows".into(),
            hosted.endpoint.unwrap().parse().unwrap(),
            hosted.pin.unwrap(),
            BTreeSet::new(),
            "windows-dpapi:",
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while a.status().phase != "confirm" || b.status().phase != "confirm" {
            assert!(
                Instant::now() < deadline,
                "a={} {}, b={} {}",
                a.status().phase,
                a.status().message,
                b.status().phase,
                b.status().message
            );
            thread::sleep(Duration::from_millis(10));
        }
        a.approve().unwrap();
        b.defer_control_ready();
        assert!(
            !a.persist_with(|_| panic!("saved before peer approval"))
                .unwrap()
        );
        b.approve().unwrap();
        let mut a_saved = None;
        let mut b_saved = None;
        while a.status().phase != "complete" || b.status().phase != "complete" {
            assert!(
                Instant::now() < deadline,
                "a={} {}, b={} {}",
                a.status().phase,
                a.status().message,
                b.status().phase,
                b.status().message
            );
            if a.stage_with(|_| Ok(())).unwrap() {
                assert!(
                    !a.persist_with(|_| panic!("committed before remote preparation"))
                        .unwrap()
                );
            }
            b.stage_with(|_| Ok(())).unwrap();
            a.persist_with(|bytes| {
                a_saved = Some(bytes.to_vec());
                Ok(())
            })
            .unwrap();
            b.persist_with(|bytes| {
                b_saved = Some(bytes.to_vec());
                Ok(())
            })
            .unwrap();
            if b_saved.is_some() {
                b.publish_control_port(std::num::NonZeroU16::new(49876));
            }
            thread::sleep(Duration::from_millis(10));
        }
        let a_inv = PairingInvitation::decode_for(a_id, &a_saved.unwrap()).unwrap();
        let b_inv = PairingInvitation::decode_for(b_id, &b_saved.unwrap()).unwrap();
        assert_eq!(a_inv.target_node_id(), a_id);
        assert_eq!(b_inv.target_node_id(), b_id);
        let a_parts = ControlCredentialBundle::decode(a_inv.credential_bundle())
            .unwrap()
            .into_parts();
        let b_parts = ControlCredentialBundle::decode(b_inv.credential_bundle())
            .unwrap()
            .into_parts();
        assert_ne!(a_parts.1, b_parts.1);
        assert_eq!(a.status().peer_control_host.as_deref(), Some("127.0.0.1"));
        assert_eq!(a.status().peer_control_port, Some(49876));
        assert_eq!(b.status().peer_control_port, None);
        assert_eq!(a_parts.2, b_parts.0[1]);
        assert_eq!(b_parts.2, a_parts.0[1]);
        let a_dir = PairedPeerDirectory::decode_json(a_id, a_inv.directory_json()).unwrap();
        let b_dir = PairedPeerDirectory::decode_json(b_id, b_inv.directory_json()).unwrap();
        assert_eq!(
            a_dir.peers().next().unwrap().paired_at_ms,
            b_dir.peers().next().unwrap().paired_at_ms
        );
    }

    #[test]
    fn wrong_pin_never_reaches_approval_or_persistence() {
        let mut a = EnrollmentManager::default();
        let mut b = EnrollmentManager::default();
        let hosted = b
            .welcome(
                NodeId::new(),
                "B".into(),
                "127.0.0.1".parse().unwrap(),
                0,
                BTreeSet::new(),
                "android-keystore:",
            )
            .unwrap();
        let wrong = if hosted.pin.as_deref() == Some("00000000") {
            "00000001"
        } else {
            "00000000"
        };
        a.join(
            NodeId::new(),
            "A".into(),
            hosted.endpoint.unwrap().parse().unwrap(),
            wrong.into(),
            BTreeSet::new(),
            "windows-dpapi:",
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while a.status().phase != "failed" || b.status().phase != "failed" {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
        assert!(!a.persist_with(|_| panic!("wrong PIN saved")).unwrap());
        assert!(a.approve().is_err());
    }

    #[test]
    fn existing_pair_is_not_replaced_by_a_correct_pin() {
        let a_id = NodeId::new();
        let b_id = NodeId::new();
        let mut a = EnrollmentManager::default();
        let mut b = EnrollmentManager::default();
        let hosted = b
            .welcome(
                b_id,
                "B".into(),
                "127.0.0.1".parse().unwrap(),
                0,
                BTreeSet::new(),
                "android-keystore:",
            )
            .unwrap();
        a.join(
            a_id,
            "A".into(),
            hosted.endpoint.unwrap().parse().unwrap(),
            hosted.pin.unwrap(),
            BTreeSet::from([b_id]),
            "windows-dpapi:",
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while a.status().phase != "failed" {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
        assert!(a.status().message.contains("已有配对"));
        assert!(
            !a.persist_with(|_| panic!("overwrote existing pair"))
                .unwrap()
        );
        b.cancel();
        assert!(!b.persist_with(|_| panic!("saved cancelled pair")).unwrap());
    }

    #[test]
    fn cancelled_window_closes_listener_without_installation() {
        let mut manager = EnrollmentManager::default();
        let hosted = manager
            .welcome(
                NodeId::new(),
                "A".into(),
                "127.0.0.1".parse().unwrap(),
                0,
                BTreeSet::new(),
                "windows-dpapi:",
            )
            .unwrap();
        manager.cancel();
        assert_eq!(manager.status().phase, "cancelled");
        assert!(manager.status().pin.is_none());
        assert!(manager.approve().is_err());
        assert!(
            !manager
                .persist_with(|_| panic!("saved cancelled pair"))
                .unwrap()
        );
        thread::sleep(Duration::from_millis(200));
        assert!(
            TcpStream::connect_timeout(
                &hosted.endpoint.unwrap().parse().unwrap(),
                Duration::from_secs(1)
            )
            .is_err()
        );
    }
}
impl Default for EnrollmentStatus {
    fn default() -> Self {
        Self {
            phase: "idle".into(),
            pin: None,
            endpoint: None,
            peer_node_id: None,
            peer_name: None,
            peer_control_host: None,
            peer_control_port: None,
            message: String::new(),
            discovery_state: "idle".into(),
            discovery_message: String::new(),
            candidates: Vec::new(),
        }
    }
}

struct Inner {
    status: EnrollmentStatus,
    approved: bool,
    cancelled: bool,
    saved: bool,
    prepared: bool,
    installation: Option<Vec<u8>>,
    socket: Option<TcpStream>,
    control_ready: bool,
    control_port: Option<std::num::NonZeroU16>,
}
impl Default for Inner {
    fn default() -> Self {
        Self {
            status: EnrollmentStatus::default(),
            approved: false,
            cancelled: false,
            saved: false,
            prepared: false,
            installation: None,
            socket: None,
            control_ready: true,
            control_port: None,
        }
    }
}
impl Drop for Inner {
    fn drop(&mut self) {
        if let Some(bytes) = &mut self.installation {
            bytes.fill(0);
        }
    }
}

/// Platform owner retains this manager, polls status, and persists only its own
/// candidate using the native secure store after both parties' explicit approval.
pub struct EnrollmentManager {
    inner: Arc<Mutex<Inner>>,
}
impl Default for EnrollmentManager {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner::default())),
        }
    }
}
impl Drop for EnrollmentManager {
    fn drop(&mut self) {
        self.cancel();
    }
}

impl EnrollmentManager {
    pub fn discover(
        &mut self,
        local: NodeId,
        interface: std::net::Ipv4Addr,
    ) -> Result<EnrollmentStatus, String> {
        if self.status().discovery_state == "scanning" {
            return Err("正在搜索配对设备".into());
        }
        let inner = self.begin()?;
        inner.lock().unwrap().status.discovery_state = "scanning".into();
        thread::spawn(move || {
            let result =
                crate::discovery::scan(local, interface, || inner.lock().unwrap().cancelled);
            let mut state = inner.lock().unwrap();
            if state.cancelled {
                return;
            }
            match result {
                Ok(candidates) => {
                    state.status.candidates = candidates;
                    state.status.discovery_state = "complete".into();
                    state.status.discovery_message = if state.status.candidates.is_empty() {
                        "未发现等待配对的设备。请让对方显示 PIN；也可粘贴配对地址或手动填写。"
                    } else {
                        "发现结果尚未认证。选择设备后仍需输入 PIN 并在双方确认。"
                    }
                    .into();
                }
                Err(e) => {
                    state.status.discovery_state = "failed".into();
                    state.status.discovery_message =
                        format!("此网络发现不可用；可粘贴配对地址或手动填写。{e}");
                }
            }
        });
        Ok(self.status())
    }
    /// Local journal replay is not proof that the remote endpoint completed.
    pub fn note_local_recovery(&self, peer_id: NodeId, peer_name: String) {
        let mut state = self.inner.lock().unwrap();
        state.status = EnrollmentStatus::default();
        state.status.phase = "recovered".into();
        state.status.peer_node_id = Some(peer_id.to_string());
        state.status.peer_name = Some(peer_name);
        state.status.message =
            "本机已使用原证书恢复配对；对端如有恢复记录也需恢复，再正常连接。未确认对端已完成。"
                .into();
    }
    /// Android creates its listener only after native credential installation.
    pub fn defer_control_ready(&self) {
        self.inner.lock().unwrap().control_ready = false;
    }
    pub fn publish_control_port(&self, port: Option<std::num::NonZeroU16>) {
        let mut state = self.inner.lock().unwrap();
        state.control_port = port;
        state.control_ready = true;
    }
    pub fn status(&self) -> EnrollmentStatus {
        self.inner.lock().unwrap().status.clone()
    }
    fn begin(&mut self) -> Result<Arc<Mutex<Inner>>, String> {
        if !matches!(
            self.status().phase.as_str(),
            "idle" | "complete" | "failed" | "cancelled" | "recovered"
        ) {
            return Err("已有配对正在进行，请先取消".into());
        }
        self.cancel();
        self.inner = Arc::new(Mutex::new(Inner::default()));
        Ok(self.inner.clone())
    }

    pub fn welcome(
        &mut self,
        local: NodeId,
        name: String,
        host: IpAddr,
        port: u16,
        paired: BTreeSet<NodeId>,
        prefix: &'static str,
    ) -> Result<EnrollmentStatus, String> {
        self.welcome_impl(local, name, host, port, paired, prefix, true)
    }

    /// Open the same explicit PIN window without LAN discovery announcements.
    /// The caller must deliver the endpoint and PIN through its private channel.
    pub fn welcome_manual(
        &mut self,
        local: NodeId,
        name: String,
        host: IpAddr,
        port: u16,
        paired: BTreeSet<NodeId>,
        prefix: &'static str,
    ) -> Result<EnrollmentStatus, String> {
        self.welcome_impl(local, name, host, port, paired, prefix, false)
    }

    #[allow(clippy::too_many_arguments)]
    fn welcome_impl(
        &mut self,
        local: NodeId,
        name: String,
        host: IpAddr,
        port: u16,
        paired: BTreeSet<NodeId>,
        prefix: &'static str,
        announce: bool,
    ) -> Result<EnrollmentStatus, String> {
        validate_address(host)?;
        let listener = TcpListener::bind(SocketAddr::new(host, port)).map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let endpoint = listener
            .local_addr()
            .map_err(|e| e.to_string())?
            .to_string();
        let mut bytes = [0; 4];
        let value = loop {
            SystemRandom::new()
                .fill(&mut bytes)
                .map_err(|_| "OS random failed")?;
            let value = u32::from_be_bytes(bytes);
            if value < 4_200_000_000 {
                break value % 100_000_000;
            }
        };
        let pin = format!("{value:08}");
        let inner = self.begin()?;
        {
            let mut state = inner.lock().unwrap();
            state.status.phase = "waiting".into();
            state.status.pin = Some(pin.clone());
            state.status.endpoint = Some(endpoint);
        }
        thread::spawn(move || {
            let deadline = Instant::now() + WINDOW;
            let result = (|| {
                let announcer = if let Some(ip) = announcement_ip(host, announce) {
                    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
                    match crate::discovery::Announcer::new(ip, local, name.clone(), port) {
                        Ok(announcer) => Some(announcer),
                        Err(error) => {
                            inner.lock().unwrap().status.discovery_message =
                                format!("配对地址可手动使用，局域网广播不可用：{error}");
                            None
                        }
                    }
                } else {
                    None
                };
                let mut next_announcement = Instant::now();
                let socket = loop {
                    check(&inner, deadline)?;
                    if Instant::now() >= next_announcement {
                        if let Some(announcer) = &announcer
                            && let Err(error) = announcer.announce()
                        {
                            inner.lock().unwrap().status.discovery_message =
                                format!("配对地址可手动使用，局域网广播未成功：{error}");
                        }
                        next_announcement = Instant::now() + Duration::from_secs(1);
                    }
                    match listener.accept() {
                        Ok((socket, _)) => break socket,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(100))
                        }
                        Err(e) => return Err(e.to_string()),
                    }
                };
                // Manual hosting has no discovery/retry window. Close the accept
                // socket now so additional joiners cannot queue behind this peer.
                // Preserve the existing default welcome listener lifetime.
                if !announce {
                    drop(listener);
                }
                // One accepted attempt consumes this PIN window, including malformed/wrong PIN attempts.
                run(
                    inner.clone(),
                    socket,
                    EnrollmentAttempt {
                        local_id: local,
                        name,
                        pin,
                        initiator: false,
                        paired,
                        prefix,
                        deadline,
                    },
                )
            })();
            finish(&inner, result);
        });
        Ok(self.status())
    }

    pub fn join(
        &mut self,
        local: NodeId,
        name: String,
        endpoint: SocketAddr,
        pin: String,
        paired: BTreeSet<NodeId>,
        prefix: &'static str,
    ) -> Result<EnrollmentStatus, String> {
        validate_address(endpoint.ip())?;
        if pin.len() != 8 || !pin.bytes().all(|b| b.is_ascii_digit()) {
            return Err("请输入对方显示的 8 位 PIN".into());
        }
        let inner = self.begin()?;
        {
            let mut state = inner.lock().unwrap();
            state.status.phase = "authenticating".into();
            state.status.endpoint = Some(endpoint.to_string());
        }
        thread::spawn(move || {
            let deadline = Instant::now() + WINDOW;
            let result = TcpStream::connect_timeout(&endpoint, Duration::from_secs(10))
                .map_err(|e| e.to_string())
                .and_then(|socket| {
                    run(
                        inner.clone(),
                        socket,
                        EnrollmentAttempt {
                            local_id: local,
                            name,
                            pin,
                            initiator: true,
                            paired,
                            prefix,
                            deadline,
                        },
                    )
                });
            finish(&inner, result);
        });
        Ok(self.status())
    }

    pub fn approve(&self) -> Result<(), String> {
        let mut state = self.inner.lock().unwrap();
        if state.status.phase != "confirm" {
            return Err("当前没有可确认的配对请求".into());
        }
        state.approved = true;
        state.status.phase = "waiting_peer".into();
        Ok(())
    }
    pub fn cancel(&self) {
        let mut state = self.inner.lock().unwrap();
        state.cancelled = true;
        if state.status.discovery_state == "scanning" {
            state.status.discovery_state = "idle".into();
        }
        if let Some(socket) = state.socket.take() {
            let _ = socket.shutdown(Shutdown::Both);
        }
        if let Some(mut bytes) = state.installation.take() {
            bytes.fill(0);
        }
        state.status.pin = None;
        if !matches!(state.status.phase.as_str(), "idle" | "complete" | "failed") {
            state.status.phase = "cancelled".into();
        }
    }
    pub fn fail_persistence(&self, message: String) {
        self.cancel();
        let mut state = self.inner.lock().unwrap();
        state.status.phase = "failed".into();
        state.status.message = message;
    }
    /// Native-only: never expose this local credential package to the WebView or network.
    pub fn stage_with(
        &self,
        stage: impl FnOnce(&[u8]) -> Result<(), String>,
    ) -> Result<bool, String> {
        let mut state = self.inner.lock().unwrap();
        if state.cancelled || state.status.phase != "preparing" || state.prepared {
            return Ok(false);
        }
        stage(
            state
                .installation
                .as_ref()
                .ok_or("missing local enrollment candidate")?,
        )?;
        state.prepared = true;
        state.status.phase = "waiting_prepared".into();
        Ok(true)
    }

    pub fn persist_with(
        &self,
        persist: impl FnOnce(&[u8]) -> Result<(), String>,
    ) -> Result<bool, String> {
        let mut state = self.inner.lock().unwrap();
        if state.cancelled || state.status.phase != "saving" || state.saved {
            return Ok(false);
        }
        let bytes = state
            .installation
            .as_ref()
            .ok_or("missing local enrollment candidate")?;
        persist(bytes)?;
        if let Some(mut bytes) = state.installation.take() {
            bytes.fill(0);
        }
        state.saved = true;
        state.status.phase = "waiting_receipt".into();
        Ok(true)
    }
}

fn validate_address(ip: IpAddr) -> Result<(), String> {
    if ip.is_unspecified() || ip.is_multicast() {
        return Err("请选择具体的本机或对端 IP 地址".into());
    }
    Ok(())
}
fn check(inner: &Arc<Mutex<Inner>>, deadline: Instant) -> Result<(), String> {
    if inner.lock().unwrap().cancelled {
        return Err("配对已取消".into());
    }
    if Instant::now() >= deadline {
        return Err("配对已超时，请重新开启配对窗口".into());
    }
    Ok(())
}
fn wait(
    inner: &Arc<Mutex<Inner>>,
    deadline: Instant,
    ready: impl Fn(&Inner) -> bool,
) -> Result<(), String> {
    loop {
        check(inner, deadline)?;
        if ready(&inner.lock().unwrap()) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
}
// Fixed-size incremental framing preserves bytes across socket timeouts. The
// peer may approve first, but we keep observing closure until local consent.
fn exchange_approvals(
    channel: &mut (impl Read + Write),
    mut locally_approved: impl FnMut() -> Result<bool, String>,
) -> Result<(), String> {
    const APPROVAL: &[u8] = b"\0\0\0\x09approve/1";
    let mut received = 0;
    let mut sent = false;
    loop {
        if locally_approved()? && !sent {
            send(channel, b"approve/1")?;
            sent = true;
        }
        if sent && received == APPROVAL.len() {
            return Ok(());
        }
        let mut byte = [0];
        match channel.read(&mut byte) {
            Ok(0) => return Err("peer closed during pairing approval".into()),
            Ok(_) if APPROVAL.get(received) == Some(&byte[0]) => received += 1,
            Ok(_) => return Err("invalid peer pairing approval".into()),
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted
                ) => {}
            Err(_) => return Err("peer transport failed during pairing approval".into()),
        }
    }
}

fn finish(inner: &Arc<Mutex<Inner>>, result: Result<(), String>) {
    let mut state = inner.lock().unwrap();
    state.status.pin = None;
    if let Some(socket) = state.socket.take() {
        let _ = socket.shutdown(Shutdown::Both);
    }
    if let Some(mut bytes) = state.installation.take() {
        bytes.fill(0);
    }
    if state.cancelled {
        return;
    }
    match result {
        Ok(()) => {
            state.status.phase = "complete".into();
            state.status.message = "双方已保存信任，可连接并选择共享资源。".into();
        }
        Err(error) => {
            state.status.phase = "failed".into();
            state.status.message = if state.saved {
                format!(
                    "本机已保存双方确认的配对，但未收到对端完成回执。请先尝试正常连接，不要重新生成证书。{error}"
                )
            } else {
                error
            };
        }
    }
}

struct EnrollmentAttempt {
    local_id: NodeId,
    name: String,
    pin: String,
    initiator: bool,
    paired: BTreeSet<NodeId>,
    prefix: &'static str,
    deadline: Instant,
}

fn run(
    inner: Arc<Mutex<Inner>>,
    socket: TcpStream,
    attempt: EnrollmentAttempt,
) -> Result<(), String> {
    let EnrollmentAttempt {
        local_id,
        name,
        mut pin,
        initiator,
        paired,
        prefix,
        deadline,
    } = attempt;
    check(&inner, deadline)?;
    let peer_host = socket
        .peer_addr()
        .map_err(|e| e.to_string())?
        .ip()
        .to_string();
    socket.set_nonblocking(false).map_err(|e| e.to_string())?;
    socket
        .set_read_timeout(Some(Duration::from_secs(15)))
        .map_err(|e| e.to_string())?;
    socket
        .set_write_timeout(Some(Duration::from_secs(15)))
        .map_err(|e| e.to_string())?;
    socket.set_nodelay(true).map_err(|e| e.to_string())?;
    {
        let mut state = inner.lock().unwrap();
        state.socket = Some(socket.try_clone().map_err(|e| e.to_string())?);
        state.status.phase = "authenticating".into();
    }
    let local = LocalIdentity::generate(local_id, name)?;
    let result = authenticate(socket, &local, &pin, initiator);
    pin.clear();
    let (mut channel, peer) = result?;
    if paired.contains(&peer.node_id) {
        return Err(
            "此节点已有配对；不会通过首次配对覆盖旧证书。请使用原配对连接或明确的恢复流程。".into(),
        );
    }
    {
        let mut state = inner.lock().unwrap();
        state.status.phase = "confirm".into();
        state.status.pin = None;
        state.status.peer_node_id = Some(peer.node_id.to_string());
        state.status.peer_name = Some(peer.name.clone());
    }
    // Read even before local approval so peer cancellation cannot leave a stale
    // confirmation window alive. Receiving approval never implies local consent.
    channel.set_read_timeout(Duration::from_millis(100))?;
    exchange_approvals(&mut channel, || {
        check(&inner, deadline)?;
        Ok(inner.lock().unwrap().approved)
    })?;
    // Subsequent receipts retain the existing finite overall deadline.
    let timeout = deadline
        .saturating_duration_since(Instant::now())
        .max(Duration::from_millis(1));
    channel.set_read_timeout(timeout)?;
    let mut bundle = local
        .credential_for(&peer)?
        .encode()
        .map_err(|e| e.to_string())?;
    let digest = Sha256::digest(&bundle);
    let handle = CredentialHandle::new(format!(
        "{prefix}{}",
        digest
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    ))
    .map_err(|e| e.to_string())?;
    let now: u64 = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis()
        .try_into()
        .map_err(|_| "invalid time")?;
    // The welcoming endpoint selects a shared pairing time; no shared clock is assumed.
    let paired_at = if initiator {
        let bytes = receive(&mut channel)?;
        u64::from_be_bytes(bytes.try_into().map_err(|_| "invalid pairing timestamp")?)
    } else {
        send(&mut channel, &now.to_be_bytes())?;
        now
    };
    let mut directory = PairedPeerDirectory::new(local_id).map_err(|e| e.to_string())?;
    directory
        .add(
            PairedPeerRecord::new(
                peer.node_id,
                peer.name.clone(),
                peer.server_name(),
                handle,
                peer.fingerprint()?,
                paired_at,
            )
            .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    let invitation = PairingInvitation::new(
        local_id,
        directory.encode_json().map_err(|e| e.to_string())?,
        bundle.clone(),
    )
    .map_err(|e| e.to_string())?;
    bundle.fill(0);
    {
        let mut state = inner.lock().unwrap();
        state.installation = Some(invitation.encode().map_err(|e| e.to_string())?);
        state.status.phase = "preparing".into();
    }
    wait(&inner, deadline, |s| s.prepared)?;
    send(&mut channel, b"prepared/1")?;
    if receive(&mut channel)? != b"prepared/1" {
        return Err("对端尚未保存恢复记录".into());
    }
    inner.lock().unwrap().status.phase = "saving".into();
    wait(&inner, deadline, |s| s.saved)?;
    send(&mut channel, b"saved/1")?;
    if receive(&mut channel)? != b"saved/1" {
        return Err("对端尚未完成保存".into());
    }
    wait(&inner, deadline, |s| s.control_ready)?;
    let handoff = ControlHandoff {
        port: inner.lock().unwrap().control_port,
    };
    send(
        &mut channel,
        &serde_json::to_vec(&handoff).map_err(|e| e.to_string())?,
    )?;
    let remote: ControlHandoff = serde_json::from_slice(&receive(&mut channel)?)
        .map_err(|_| "invalid control connection handoff")?;
    if let Some(port) = remote.port {
        let mut state = inner.lock().unwrap();
        state.status.peer_control_host = Some(peer_host);
        state.status.peer_control_port = Some(port.get());
    }
    Ok(())
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ControlHandoff {
    port: Option<std::num::NonZeroU16>,
}
