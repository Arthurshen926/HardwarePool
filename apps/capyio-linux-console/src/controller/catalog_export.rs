//! Two fresh local decisions export only existing nonsecret launcher schemas.
//! This module never spawns a Node or creates resource authority or devices.
use super::*;
use capyio_core::{CapabilityId, PortId};
use std::{ffi::CString, os::fd::FromRawFd, path::Path};

const CONSENT: Duration = Duration::from_secs(30);
// `save ` + a canonical MessageId + ` ` leaves 214 bytes in Terminal's 256.
const MAX_OUTPUT_PATH: usize = 214;
const LIMITS: &str = "Catalog snapshot may already be stale; the later camera launcher revalidates it.\r\nExport is not resource authorization: no Routes, grants, leases, media or devices are created.\r\nSaving does not Start a camera; a later explicit camera command needs fresh independent consent.\r\nThe fixed 120-frame limit is approximately 4 seconds at 30 fps, even with the 60-second duration ceiling.\r\n";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ExportKind {
    Camera,
    Playback,
    VirtualMicrophone,
    Capture,
    VirtualSpeaker,
    Touchpad,
}

impl ExportKind {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Camera => "camera",
            Self::Playback => "playback",
            Self::VirtualMicrophone => "virtual microphone",
            Self::Capture => "capture",
            Self::VirtualSpeaker => "virtual speaker",
            Self::Touchpad => "touchpad",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Camera => "Camera",
            Self::Playback => "Playback",
            Self::VirtualMicrophone => "Virtual microphone",
            Self::Capture => "Capture",
            Self::VirtualSpeaker => "Virtual speaker",
            Self::Touchpad => "Touchpad",
        }
    }

    fn capture(self) -> bool {
        matches!(self, Self::Capture | Self::VirtualSpeaker)
    }

    fn audio(self) -> bool {
        !matches!(self, Self::Camera | Self::Touchpad)
    }

    fn limits(self) -> String {
        if self == Self::Camera {
            return LIMITS.into();
        }
        let mut text = format!(
            "Catalog snapshot may already be stale; the later {} launcher revalidates it.\r\nExport is not resource authorization: no Routes, grants, leases, media or devices are created.\r\nSaving does not Start a {} session; a later explicit command needs fresh independent consent.\r\n",
            self.name(),
            self.name()
        );
        if self.audio() {
            text.push_str("WARNING: plaintext unauthenticated UDP media; trusted isolated lab only.\r\nMedia security: unauthenticated_udp_trusted_lab_only.\r\n");
            let format = if self.capture() {
                "pcm-s16le-48000-stereo"
            } else {
                "pcm-s16le-48000-mono"
            };
            let direction = if self.capture() {
                "local Source -> remote Sink"
            } else {
                "remote Source -> local Sink"
            };
            text.push_str(&format!("Audio direction: {direction}.\r\nAudio format: {format}; duration 1..300 seconds.\r\n"));
        } else {
            text.push_str("Touchpad format: touchpad-frame-v1; five contacts (max_contacts=5), 120 x 70 mm, pressure supported, not clickable.\r\nMedia security: tls13_mtls_v1; duration 3..60 seconds.\r\nExport proves neither frame delivery nor OS gestures.\r\n");
        }
        text
    }
}

#[derive(Clone, Debug)]
pub(super) struct ExportOptions {
    pub(super) kind: ExportKind,
    pub(super) media_security: media::MediaSecurity,
    pub(super) listen: SocketAddr,
    pub(super) media: Option<SocketAddr>,
    pub(super) target: Option<String>,
    pub(super) duration_secs: u64,
}

pub(super) struct Candidate {
    pub(super) peer: NodeId,
    pub(super) capability: CapabilityId,
    pub(super) port: PortId,
    pub(super) display: String,
}

#[cfg(test)]
type CameraCandidate = Candidate;

impl Candidate {
    fn command(&self, request: MessageId) -> String {
        format!(
            "select {request} {} {} {}",
            self.peer, self.capability, self.port
        )
    }
}

fn output_text(output: &Path) -> io::Result<&str> {
    let text = output.to_str().ok_or_else(invalid)?;
    if !output.is_absolute()
        || text.len() > MAX_OUTPUT_PATH
        || !text.bytes().all(|byte| (32..=126).contains(&byte))
        || text[1..]
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(invalid());
    }
    Ok(text)
}

pub(super) fn validate_export_options(output: &Path, options: &ExportOptions) -> io::Result<()> {
    output_text(output)?;
    let SocketAddr::V4(mut listen) = options.listen else {
        return Err(invalid());
    };
    if options.kind.audio() {
        let Some(SocketAddr::V4(mut media)) = options.media else {
            return Err(invalid());
        };
        // Match the existing audio parser/Host policy, including its port-1
        // placeholder for an explicitly requested ephemeral TCP listener.
        if listen.port() == 0 {
            listen.set_port(1);
        }
        let lab_address = |address: std::net::SocketAddrV4| {
            let ip = address.ip();
            let octets = ip.octets();
            let overlay = octets[0] == 100 && (64..=127).contains(&octets[1]);
            address.port() != 0
                && (ip.is_private() || ip.is_loopback() || overlay)
                && octets[3] != 255
        };
        let secure = options.media_security.is_secure();
        if secure {
            if !matches!(
                options.kind,
                ExportKind::Playback | ExportKind::VirtualMicrophone
            ) || media.port() != 0
                || options.duration_secs > options.media_security.max_duration_secs()
            {
                return Err(invalid());
            }
            media.set_port(1);
        }
        if !(1..=300).contains(&options.duration_secs)
            || listen.ip() != media.ip()
            || !lab_address(listen)
            || !lab_address(media)
            || (!secure && listen == media)
        {
            return Err(invalid());
        }
        match (options.kind, options.target.as_deref()) {
            (ExportKind::Playback | ExportKind::Capture, Some(target))
                if !target.is_empty()
                    && target.len() <= 128
                    && !target.starts_with('-')
                    && target != "0"
                    && !target.eq_ignore_ascii_case("auto")
                    && !target.eq_ignore_ascii_case("default")
                    && target
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"_.:-".contains(&byte)) =>
            {
                Ok(())
            }
            (ExportKind::VirtualMicrophone | ExportKind::VirtualSpeaker, None) => Ok(()),
            _ => Err(invalid()),
        }
    } else {
        let ip = listen.ip();
        if options.media_security != media::MediaSecurity::Legacy
            || options.media.is_some()
            || options.target.is_some()
            || !(3..=60).contains(&options.duration_secs)
            || !(ip.is_loopback() || ip.is_private() || ip.is_link_local())
            || ip.is_broadcast()
            || ip.is_unspecified()
        {
            return Err(invalid());
        }
        Ok(())
    }
}

#[cfg(test)]
fn validate_options(output: &Path, listen: SocketAddr, duration_secs: u64) -> io::Result<()> {
    validate_export_options(output, &camera_options(listen, duration_secs))
}

pub(super) fn camera_options(listen: SocketAddr, duration_secs: u64) -> ExportOptions {
    ExportOptions {
        kind: ExportKind::Camera,
        media_security: media::MediaSecurity::Legacy,
        listen,
        media: None,
        target: None,
        duration_secs,
    }
}

fn select<'a>(
    line: &[u8],
    request: MessageId,
    peer: NodeId,
    candidates: &'a [Candidate],
) -> io::Result<&'a Candidate> {
    if request.as_uuid().is_nil() || peer.as_uuid().is_nil() {
        return Err(invalid());
    }
    let mut matching = candidates.iter().filter(|candidate| {
        candidate.peer == peer
            && !candidate.capability.as_uuid().is_nil()
            && !candidate.port.as_uuid().is_nil()
            && line == candidate.command(request).as_bytes()
    });
    let chosen = matching.next().ok_or_else(invalid)?;
    if matching.next().is_some() {
        return Err(invalid());
    }
    Ok(chosen)
}

fn build_manifest(
    local: NodeId,
    peer: NodeId,
    chosen: &Candidate,
    options: &ExportOptions,
) -> io::Result<Vec<u8>> {
    let remote =
        json!({"node_id": chosen.peer, "capability_id": chosen.capability, "port_id": chosen.port});
    let mut value = json!({
        "version": 1,
        "node_id": local,
        "listen": options.listen,
        "duration_secs": options.duration_secs,
    });
    value[if options.kind.capture() {
        "sink"
    } else {
        "source"
    }] = remote;
    if options.kind.audio() {
        value["media"] = json!(options.media);
        if options.media_security.is_secure() {
            value["media_security"] = json!(options.media_security);
        }
        value["target"] = json!(options.target);
        value["endpoint_mode"] = json!(match options.kind {
            ExportKind::Playback => "playback",
            ExportKind::VirtualMicrophone => "virtual_microphone",
            ExportKind::Capture => "selected_source",
            ExportKind::VirtualSpeaker => "virtual_speaker",
            _ => return Err(invalid()),
        });
        if !options.kind.capture() {
            value["proposal_mode"] = json!("linux_led");
        }
    } else if options.media.is_some() || options.target.is_some() {
        return Err(invalid());
    }
    let bytes = serde_json::to_vec(&value).map_err(|_| invalid())?;
    // The existing launcher's parser, not the catalog hint, owns each schema.
    match options.kind {
        ExportKind::Camera => super::camera::validate_export(&bytes, local, peer)?,
        ExportKind::Touchpad => super::touchpad::validate_export(&bytes, local, peer)?,
        _ => super::media::validate_export(&bytes, local, peer, options.kind.capture())?,
    }
    Ok(bytes)
}

#[cfg(test)]
fn manifest(
    local: NodeId,
    peer: NodeId,
    chosen: &CameraCandidate,
    listen: SocketAddr,
    duration_secs: u64,
) -> io::Result<Vec<u8>> {
    build_manifest(local, peer, chosen, &camera_options(listen, duration_secs))
}

fn save_command(request: MessageId, output: &Path) -> io::Result<String> {
    if request.as_uuid().is_nil() {
        return Err(invalid());
    }
    Ok(format!("save {request} {}", output_text(output)?))
}

fn save_matches(line: &[u8], request: MessageId, output: &Path) -> io::Result<()> {
    if line != save_command(request, output)?.as_bytes() {
        return Err(invalid());
    }
    Ok(())
}

fn response(tty: &mut Terminal, deadline: Instant) -> io::Result<crate::terminal::SecretLine> {
    loop {
        check(deadline)?;
        if let Some(line) = tty.poll_line()? {
            // Reject pasted second lines and partial extra input before making
            // either decision. Terminal also rejects Ctrl-C/EOF in that read.
            if tty.poll_line()?.is_some() || line.as_bytes() == b"cancel" {
                return Err(invalid());
            }
            check(deadline)?;
            return Ok(line);
        }
        thread::sleep(Duration::from_millis(5));
    }
}

pub(super) fn prepare(
    tty: &mut Terminal,
    local: NodeId,
    peer: NodeId,
    candidates: &[Candidate],
    output: &Path,
    options: &ExportOptions,
) -> io::Result<()> {
    validate_export_options(output, options)?;
    let limits = if options.media_security.is_secure() {
        format!(
            "Catalog snapshot may already be stale; later launch revalidates it.\r\nExport is not resource authorization: no Routes, grants, leases, media or devices are created.\r\nSaving does not Start a {} session; fresh independent consent is required.\r\n{}Format: {}; media security: {}.\r\n",
            options.kind.name(),
            options.media_security.warning(),
            options.media_security.secure_format().ok_or_else(invalid)?,
            options.media_security.name()
        )
    } else {
        options.kind.limits()
    };
    let label = options.kind.label();
    let name = options.kind.name();
    let display_deadline = Instant::now() + Duration::from_secs(2);
    tty.write(&format!("\r\n{limits}"), display_deadline)?;
    if candidates.is_empty() {
        tty.write(
            &format!(
                "No eligible {name} {} in this complete snapshot.\r\n",
                if options.kind.capture() {
                    "Sink"
                } else {
                    "Source"
                }
            ),
            display_deadline,
        )?;
        return Err(invalid());
    }
    if candidates.len() > 64 {
        return Err(invalid());
    }
    let request = MessageId::new();
    let mut prompt = format!("Selection request: {request}\r\n");
    for candidate in candidates {
        tty.write(
            &format!("{label} candidate: {}\r\n", candidate.display),
            display_deadline,
        )?;
        prompt.push_str(&format!("Type exactly: {}\r\n", candidate.command(request)));
    }
    prompt.push_str("Selection (30 seconds; cancel, Ctrl-C or Ctrl-D abort): ");
    let selection_deadline = Instant::now() + CONSENT;
    tty.begin_prompt(&prompt, selection_deadline.min(display_deadline))?;
    let line = response(tty, selection_deadline)?;
    let chosen = select(line.as_bytes(), request, peer, candidates)?;
    drop(line);
    let bytes = build_manifest(local, peer, chosen, options)?;
    let destination = PinnedOutput::open(output)?;
    // No file has been created. Selection is not permission to save, and the
    // second begin_prompt discards all input left from the first challenge.
    let request = MessageId::new();
    let command = save_command(request, output)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| invalid())?;
    let prompt = format!(
        "\r\n{limits}Output path: {}\r\n{label} config: {text}\r\nSave request: {request}\r\nType exactly: {command}\r\nSave (30 seconds; cancel, Ctrl-C or Ctrl-D abort): ",
        output_text(output)?
    );
    let save_deadline = Instant::now() + CONSENT;
    tty.begin_prompt(
        &prompt,
        save_deadline.min(Instant::now() + Duration::from_secs(1)),
    )?;
    let line = response(tty, save_deadline)?;
    save_matches(line.as_bytes(), request, output)?;
    check(save_deadline)?;
    destination.create(&bytes, save_deadline)?;
    tty.write(
        &format!("{label} manifest saved; no {name} session started.\r\n"),
        Instant::now() + Duration::from_secs(1),
    )
}

struct PinnedOutput {
    // Keep the entire checked chain open. All subsequent traversal and create
    // operations are relative to these descriptors, not a reparsed path.
    directories: Vec<fs::File>,
    components: Vec<CString>,
    name: CString,
    owner: libc::uid_t,
}

fn directory(parent: Option<&fs::File>, name: &std::ffi::CStr) -> io::Result<fs::File> {
    let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
    // SAFETY: names are NUL-terminated; the parent is a live pinned descriptor.
    let raw = unsafe {
        match parent {
            Some(parent) => libc::openat(parent.as_raw_fd(), name.as_ptr(), flags),
            None => libc::open(name.as_ptr(), flags),
        }
    };
    if raw < 0 {
        return Err(invalid());
    }
    // SAFETY: open/openat returned a fresh owned file descriptor.
    Ok(unsafe { fs::File::from_raw_fd(raw) })
}

fn safe_directory(metadata: &fs::Metadata, owner: libc::uid_t, final_dir: bool) -> bool {
    directory_policy(
        metadata.is_dir(),
        metadata.uid(),
        metadata.mode(),
        owner,
        final_dir,
    )
}

fn directory_policy(
    is_dir: bool,
    uid: libc::uid_t,
    mode: u32,
    owner: libc::uid_t,
    final_dir: bool,
) -> bool {
    let trusted_owner = uid == owner || uid == 0;
    let safe_mode = mode & 0o022 == 0;
    let root_sticky_ancestor = !final_dir && uid == 0 && mode & 0o1000 != 0;
    is_dir
        && trusted_owner
        && (safe_mode || root_sticky_ancestor)
        && (!final_dir || (uid == owner && safe_mode))
}

fn same_inode(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    left.dev() == right.dev() && left.ino() == right.ino()
}

impl PinnedOutput {
    fn open(output: &Path) -> io::Result<Self> {
        let text = output_text(output)?;
        let mut parts = text[1..]
            .split('/')
            .map(|part| CString::new(part).map_err(|_| invalid()))
            .collect::<io::Result<Vec<_>>>()?;
        let name = parts.pop().ok_or_else(invalid)?;
        // SAFETY: geteuid reads only the current process identity.
        let owner = unsafe { libc::geteuid() };
        let mut directories = vec![directory(None, c"/")?];
        for part in &parts {
            let current = directories.last().ok_or_else(invalid)?;
            if !safe_directory(&current.metadata()?, owner, false) {
                return Err(invalid());
            }
            directories.push(directory(Some(current), part)?);
        }
        let result = Self {
            directories,
            components: parts,
            name,
            owner,
        };
        result.revalidate()?;
        Ok(result)
    }

    fn revalidate(&self) -> io::Result<()> {
        for (index, pinned) in self.directories.iter().enumerate() {
            let metadata = pinned.metadata()?;
            if !safe_directory(&metadata, self.owner, index + 1 == self.directories.len()) {
                return Err(invalid());
            }
            if index > 0 {
                let linked = directory(
                    Some(&self.directories[index - 1]),
                    &self.components[index - 1],
                )?;
                if !same_inode(&metadata, &linked.metadata()?) {
                    return Err(invalid());
                }
            }
        }
        Ok(())
    }

    fn create(&self, bytes: &[u8], deadline: Instant) -> io::Result<()> {
        self.create_with(bytes, deadline, |file, bytes| file.write_all(bytes))
    }

    fn create_with(
        &self,
        bytes: &[u8],
        deadline: Instant,
        write: impl FnOnce(&mut fs::File, &[u8]) -> io::Result<()>,
    ) -> io::Result<()> {
        if bytes.is_empty() || bytes.len() > MAX_FRAME {
            return Err(invalid());
        }
        check(deadline)?;
        self.revalidate()?;
        check(deadline)?;
        let parent = self.directories.last().ok_or_else(invalid)?;
        // SAFETY: parent/name are live; mode is supplied for O_CREAT. Exclusive
        // creation never follows or truncates an existing path, even a symlink.
        let raw = unsafe {
            libc::openat(
                parent.as_raw_fd(),
                self.name.as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0o600,
            )
        };
        if raw < 0 {
            return Err(invalid());
        }
        // SAFETY: openat returned a fresh owned descriptor.
        let mut file = unsafe { fs::File::from_raw_fd(raw) };
        let identity = file.metadata()?;
        let result = (|| {
            if !identity.is_file()
                || identity.uid() != self.owner
                || identity.mode() & 0o777 != 0o600
            {
                return Err(invalid());
            }
            check(deadline)?;
            write(&mut file, bytes)?;
            check(deadline)?;
            if file.metadata()?.len() != bytes.len() as u64 {
                return Err(invalid());
            }
            Ok(())
        })();
        if result.is_err() {
            self.cleanup_created(&identity);
        }
        result
    }

    fn cleanup_created(&self, identity: &fs::Metadata) {
        let Some(parent) = self.directories.last() else {
            return;
        };
        let mut found = std::mem::MaybeUninit::<libc::stat>::uninit();
        // SAFETY: fstatat initializes found on success; no symlink is followed.
        let result = unsafe {
            libc::fstatat(
                parent.as_raw_fd(),
                self.name.as_ptr(),
                found.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        };
        if result == 0 {
            // SAFETY: the checked fstatat call succeeded.
            let found = unsafe { found.assume_init() };
            if found.st_dev == identity.dev() && found.st_ino == identity.ino() {
                // The protected parent excludes other-user replacement. Under
                // the trusted/cooperative same-UID/root assumption, remove only
                // this verified inode. This stat/unlink pair is not atomic
                // against a malicious same-UID/root concurrent replacement.
                unsafe {
                    libc::unlinkat(parent.as_raw_fd(), self.name.as_ptr(), 0);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt, symlink};

    const LOCAL: &str = "11111111-1111-4111-8111-111111111111";
    const PEER: &str = "22222222-2222-4222-8222-222222222222";
    const CAPABILITY: &str = "33333333-3333-4333-8333-333333333333";
    const PORT: &str = "44444444-4444-4444-8444-444444444444";

    fn candidate(port: &str) -> CameraCandidate {
        CameraCandidate {
            peer: PEER.parse().unwrap(),
            capability: CAPABILITY.parse().unwrap(),
            port: port.parse().unwrap(),
            display: String::new(),
        }
    }

    #[test]
    fn pure_camera_export_selection_is_exact_fresh_and_never_first_by_default() {
        let candidates = [
            candidate(PORT),
            candidate("55555555-5555-4555-8555-555555555555"),
        ];
        let request = MessageId::new();
        let peer = PEER.parse().unwrap();
        let command = candidates[1].command(request);
        assert_eq!(
            select(command.as_bytes(), request, peer, &candidates)
                .unwrap()
                .port,
            candidates[1].port
        );
        for line in [
            String::new(),
            "yes".into(),
            "select".into(),
            "cancel".into(),
            candidates[1].command(MessageId::new()),
            format!("{command} "),
            format!(" {command}"),
            format!("{command}\n{command}"),
            command.replace(PEER, LOCAL),
            command.replace(CAPABILITY, PORT),
            command.replace("select", "start"),
        ] {
            assert!(select(line.as_bytes(), request, peer, &candidates).is_err());
        }
        assert!(select(command.as_bytes(), MessageId::new(), peer, &candidates).is_err());
        assert!(
            select(
                command.as_bytes(),
                request,
                LOCAL.parse().unwrap(),
                &candidates
            )
            .is_err()
        );
        assert!(select(command.as_bytes(), request, peer, &[]).is_err());
        let duplicated = [candidate(PORT), candidate(PORT)];
        assert!(
            select(
                duplicated[0].command(request).as_bytes(),
                request,
                peer,
                &duplicated
            )
            .is_err()
        );
    }

    #[test]
    fn pure_camera_export_save_has_a_separate_exact_path_bound_challenge() {
        let output = Path::new("/tmp/owned/camera.json");
        let request = MessageId::new();
        let command = save_command(request, output).unwrap();
        assert!(save_matches(command.as_bytes(), request, output).is_ok());
        for line in [
            "yes".into(),
            "save".into(),
            "cancel".into(),
            format!("{command}\n{command}"),
            format!("{command} "),
            save_command(MessageId::new(), output).unwrap(),
            command.replace("camera.json", "another.json"),
            candidate(PORT).command(request),
        ] {
            assert!(save_matches(line.as_bytes(), request, output).is_err());
        }
        assert!(save_matches(command.as_bytes(), MessageId::new(), output).is_err());
        let max = format!("/{}", "x".repeat(MAX_OUTPUT_PATH - 1));
        assert_eq!(save_command(request, Path::new(&max)).unwrap().len(), 256);
        assert!(save_command(request, Path::new(&format!("{max}x"))).is_err());
    }

    #[test]
    fn pure_camera_export_matches_existing_schema_and_nothing_secret() {
        let local = LOCAL.parse().unwrap();
        let peer = PEER.parse().unwrap();
        let chosen = candidate(PORT);
        let bytes = manifest(local, peer, &chosen, "127.0.0.1:0".parse().unwrap(), 60).unwrap();
        assert!(bytes.len() <= MAX_FRAME);
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            value,
            json!({
                "version":1,"node_id":LOCAL,
                "source":{"node_id":PEER,"capability_id":CAPABILITY,"port_id":PORT},
                "listen":"127.0.0.1:0","duration_secs":60
            })
        );
        for duration in [0, 2, 61, u64::MAX] {
            assert!(
                manifest(
                    local,
                    peer,
                    &chosen,
                    "127.0.0.1:0".parse().unwrap(),
                    duration
                )
                .is_err()
            );
        }
        assert!(manifest(peer, peer, &chosen, "127.0.0.1:0".parse().unwrap(), 3).is_err());
        assert!(manifest(local, local, &chosen, "127.0.0.1:0".parse().unwrap(), 3).is_err());
        assert!(LIMITS.contains("120-frame"));
        assert!(LIMITS.contains("approximately 4 seconds"));
        assert!(LIMITS.contains("60-second"));
        assert!(LIMITS.contains("snapshot may already be stale"));
        assert!(LIMITS.contains("not resource authorization"));
    }

    #[test]
    fn pure_camera_export_rejects_unsafe_listen_duration_and_ambiguous_paths() {
        let output = Path::new("/tmp/owned/camera.json");
        for address in [
            "127.0.0.1:0",
            "192.168.1.2:1234",
            "169.254.2.3:0",
            "10.1.2.3:65535",
        ] {
            for duration in [3, 60] {
                assert!(validate_options(output, address.parse().unwrap(), duration).is_ok());
            }
        }
        for address in [
            "0.0.0.0:0",
            "8.8.8.8:44",
            "224.0.0.1:44",
            "255.255.255.255:44",
            "[::1]:0",
        ] {
            assert!(validate_options(output, address.parse().unwrap(), 3).is_err());
        }
        for duration in [0, 2, 61] {
            assert!(validate_options(output, "127.0.0.1:0".parse().unwrap(), duration).is_err());
        }
        for path in [
            "",
            "/",
            "relative.json",
            "/tmp/../camera.json",
            "/tmp/./camera.json",
            "/tmp//camera.json",
            "/tmp/camera.json/",
            "/tmp/line\n.json",
            "/tmp/\u{202e}.json",
            "/tmp/\x1b[2J.json",
        ] {
            assert!(output_text(Path::new(path)).is_err(), "{path:?}");
        }
    }

    #[test]
    fn pure_camera_export_directory_ownership_and_sticky_ancestor_policy() {
        assert!(directory_policy(true, 1000, 0o700, 1000, true));
        assert!(directory_policy(true, 0, 0o755, 1000, false));
        assert!(directory_policy(true, 0, 0o1777, 1000, false));
        assert!(!directory_policy(true, 0, 0o1777, 1000, true));
        assert!(!directory_policy(true, 1000, 0o1777, 1000, false));
        assert!(!directory_policy(true, 1001, 0o700, 1000, false));
        assert!(!directory_policy(true, 1001, 0o700, 1000, true));
        assert!(!directory_policy(true, 0, 0o755, 1000, true));
        assert!(!directory_policy(true, 1000, 0o770, 1000, true));
        assert!(!directory_policy(true, 1000, 0o702, 1000, false));
        assert!(!directory_policy(false, 1000, 0o700, 1000, false));
    }

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("capyio-camera-export-{}", MessageId::new()));
            fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
            Self(path)
        }

        fn output(&self) -> PathBuf {
            self.0.join("camera.json")
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            // Only this test's fresh synthetic tree, never a caller path.
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn future() -> Instant {
        Instant::now() + Duration::from_secs(5)
    }

    #[test]
    fn pure_camera_export_creates_only_new_owner_private_bounded_file() {
        let fixture = Fixture::new();
        let output = fixture.output();
        let pinned = PinnedOutput::open(&output).unwrap();
        assert!(!output.exists(), "pinning is not creation");
        for bytes in [vec![], vec![b'x'; MAX_FRAME + 1]] {
            assert!(pinned.create(&bytes, future()).is_err());
            assert!(!output.exists());
        }
        assert!(pinned.create(b"{}", Instant::now()).is_err());
        assert!(!output.exists());
        let bytes = manifest(
            LOCAL.parse().unwrap(),
            PEER.parse().unwrap(),
            &candidate(PORT),
            "127.0.0.1:0".parse().unwrap(),
            3,
        )
        .unwrap();
        pinned.create(&bytes, future()).unwrap();
        let metadata = fs::metadata(&output).unwrap();
        assert_eq!(metadata.mode() & 0o777, 0o600);
        assert_eq!(metadata.uid(), unsafe { libc::geteuid() });
        assert_eq!(fs::read(&output).unwrap(), bytes);
        assert!(pinned.create(b"replacement", future()).is_err());
        assert!(same_inode(&metadata, &fs::metadata(&output).unwrap()));
        assert_eq!(fs::read(&output).unwrap(), bytes);
    }

    #[test]
    fn pure_camera_export_preserves_existing_file_symlink_and_permissions() {
        let fixture = Fixture::new();
        let output = fixture.output();
        fs::write(&output, b"existing").unwrap();
        fs::set_permissions(&output, fs::Permissions::from_mode(0o640)).unwrap();
        let before = fs::metadata(&output).unwrap();
        assert!(
            PinnedOutput::open(&output)
                .unwrap()
                .create(b"{}", future())
                .is_err()
        );
        assert_eq!(fs::read(&output).unwrap(), b"existing");
        assert_eq!(fs::metadata(&output).unwrap().mode(), before.mode());
        assert!(same_inode(&before, &fs::metadata(&output).unwrap()));
        let link = fixture.0.join("link.json");
        symlink(&output, &link).unwrap();
        assert!(
            PinnedOutput::open(&link)
                .unwrap()
                .create(b"{}", future())
                .is_err()
        );
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read(&output).unwrap(), b"existing");
        let parent_link = fixture.0.join("linked-parent");
        symlink(&fixture.0, &parent_link).unwrap();
        assert!(PinnedOutput::open(&parent_link.join("another.json")).is_err());
    }

    #[test]
    fn pure_camera_export_rechecks_pinned_parent_identity_and_mode() {
        let fixture = Fixture::new();
        let parent = fixture.0.join("safe");
        fs::DirBuilder::new().mode(0o700).create(&parent).unwrap();
        let output = parent.join("camera.json");
        let pinned = PinnedOutput::open(&output).unwrap();
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o777)).unwrap();
        assert!(pinned.create(b"{}", future()).is_err());
        assert!(!output.exists());
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o700)).unwrap();
        let moved = fixture.0.join("moved");
        fs::rename(&parent, &moved).unwrap();
        symlink(&moved, &parent).unwrap();
        assert!(pinned.create(b"{}", future()).is_err());
        assert!(!moved.join("camera.json").exists());
        fs::remove_file(&parent).unwrap();
        fs::DirBuilder::new().mode(0o700).create(&parent).unwrap();
        assert!(pinned.create(b"{}", future()).is_err());
        assert!(!output.exists());
        assert!(!moved.join("camera.json").exists());
    }

    #[test]
    fn pure_camera_export_failure_removes_only_its_created_inode() {
        let fixture = Fixture::new();
        let output = fixture.output();
        let pinned = PinnedOutput::open(&output).unwrap();
        assert!(
            pinned
                .create_with(b"{}", future(), |file, _| {
                    file.write_all(b"{")?;
                    Err(invalid())
                })
                .is_err()
        );
        assert!(!output.exists());
        let moved = fixture.0.join("moved.json");
        assert!(
            pinned
                .create_with(b"{}", future(), |file, _| {
                    file.write_all(b"{")?;
                    fs::rename(&output, &moved)?;
                    fs::write(&output, b"replacement")?;
                    Err(invalid())
                })
                .is_err()
        );
        assert_eq!(fs::read(&output).unwrap(), b"replacement");
        assert_eq!(fs::read(&moved).unwrap(), b"{");
    }

    const EXPORT_KINDS: [ExportKind; 6] = [
        ExportKind::Camera,
        ExportKind::Playback,
        ExportKind::VirtualMicrophone,
        ExportKind::Capture,
        ExportKind::VirtualSpeaker,
        ExportKind::Touchpad,
    ];

    fn options(kind: ExportKind) -> ExportOptions {
        ExportOptions {
            kind,
            media_security: media::MediaSecurity::Legacy,
            listen: "127.0.0.1:0".parse().unwrap(),
            media: kind.audio().then(|| "127.0.0.1:43198".parse().unwrap()),
            target: matches!(kind, ExportKind::Playback | ExportKind::Capture)
                .then(|| "synthetic.target".into()),
            duration_secs: 7,
        }
    }

    #[test]
    fn pure_secure_export_roundtrip_and_mode_rejections() {
        let mut selected = options(ExportKind::Playback);
        selected.media_security = media::MediaSecurity::Secure;
        selected.media = Some("127.0.0.1:0".parse().unwrap());
        check_option_parity(&selected, true);
        let manifest: Value = serde_json::from_slice(&built(&selected).unwrap()).unwrap();
        assert_eq!(manifest["media_security"], "pinned_tls13_mono_v1");
        for kind in [
            ExportKind::VirtualMicrophone,
            ExportKind::Capture,
            ExportKind::VirtualSpeaker,
        ] {
            let mut bad = selected.clone();
            bad.kind = kind;
            check_option_parity(&bad, false);
        }
        let mut virtual_mic = selected.clone();
        virtual_mic.kind = ExportKind::VirtualMicrophone;
        virtual_mic.target = None;
        check_option_parity(&virtual_mic, true);
        let manifest: Value = serde_json::from_slice(&built(&virtual_mic).unwrap()).unwrap();
        assert_eq!(manifest["endpoint_mode"], "virtual_microphone");
        assert_eq!(manifest["media_security"], "pinned_tls13_mono_v1");
        assert!(manifest["target"].is_null());
        assert!(manifest.get("node_name").is_none());
        for kind in [ExportKind::Capture, ExportKind::VirtualSpeaker] {
            let mut bad = virtual_mic.clone();
            bad.kind = kind;
            check_option_parity(&bad, false);
        }
        virtual_mic.duration_secs = 61;
        check_option_parity(&virtual_mic, false);
        selected.duration_secs = 61;
        check_option_parity(&selected, false);
    }

    #[test]
    fn pure_secure_300_export_is_explicit_non_authorizing_and_bounded() {
        for kind in EXPORT_KINDS {
            let mut selected = options(kind);
            selected.media_security = media::MediaSecurity::Secure300;
            selected.media = Some("127.0.0.1:0".parse().unwrap());
            for duration in [0, 1, 60, 61, 300, 301, u64::MAX] {
                selected.duration_secs = duration;
                let supported =
                    matches!(kind, ExportKind::Playback | ExportKind::VirtualMicrophone)
                        && (1..=300).contains(&duration);
                check_option_parity(&selected, supported);
                if supported {
                    let manifest: Value =
                        serde_json::from_slice(&built(&selected).unwrap()).unwrap();
                    assert_eq!(manifest["media_security"], "pinned_tls13_mono_300s_v1");
                    assert_eq!(manifest["duration_secs"], duration);
                    for field in [
                        "approval",
                        "grant",
                        "lease",
                        "route_id",
                        "session_id",
                        "credentials",
                    ] {
                        assert!(manifest.get(field).is_none());
                    }
                }
            }
        }
    }

    fn built(options: &ExportOptions) -> io::Result<Vec<u8>> {
        build_manifest(
            LOCAL.parse().unwrap(),
            PEER.parse().unwrap(),
            &candidate(PORT),
            options,
        )
    }

    fn check_option_parity(options: &ExportOptions, expected: bool) {
        assert_eq!(
            validate_export_options(Path::new("/tmp/owned/manifest.json"), options).is_ok(),
            expected,
            "preflight: {options:?}"
        );
        assert_eq!(built(options).is_ok(), expected, "launcher: {options:?}");
    }

    #[test]
    fn pure_manifest_export_builders_match_each_existing_launcher_schema() {
        for kind in EXPORT_KINDS {
            let options = options(kind);
            check_option_parity(&options, true);
            let bytes = built(&options).unwrap();
            assert!(bytes.len() <= MAX_FRAME);
            let value: Value = serde_json::from_slice(&bytes).unwrap();
            let mut expected = json!({
                "version":1,"node_id":LOCAL,"listen":"127.0.0.1:0","duration_secs":7
            });
            expected[if kind.capture() { "sink" } else { "source" }] = json!({
                "node_id":PEER,"capability_id":CAPABILITY,"port_id":PORT
            });
            if kind.audio() {
                expected["media"] = json!("127.0.0.1:43198");
                expected["target"] = json!(options.target);
                expected["endpoint_mode"] = json!(match kind {
                    ExportKind::Playback => "playback",
                    ExportKind::VirtualMicrophone => "virtual_microphone",
                    ExportKind::Capture => "selected_source",
                    ExportKind::VirtualSpeaker => "virtual_speaker",
                    _ => unreachable!(),
                });
                if !kind.capture() {
                    expected["proposal_mode"] = json!("linux_led");
                }
            }
            assert_eq!(value, expected, "{kind:?}");
            for local in [PEER, "00000000-0000-0000-0000-000000000000"] {
                assert!(
                    build_manifest(
                        local.parse().unwrap(),
                        PEER.parse().unwrap(),
                        &candidate(PORT),
                        &options
                    )
                    .is_err()
                );
            }
            let mut wrong = candidate(PORT);
            wrong.peer = LOCAL.parse().unwrap();
            assert!(
                build_manifest(
                    LOCAL.parse().unwrap(),
                    PEER.parse().unwrap(),
                    &wrong,
                    &options
                )
                .is_err()
            );
            let mut nil_capability = candidate(PORT);
            nil_capability.capability = "00000000-0000-0000-0000-000000000000".parse().unwrap();
            assert!(
                build_manifest(
                    LOCAL.parse().unwrap(),
                    PEER.parse().unwrap(),
                    &nil_capability,
                    &options
                )
                .is_err()
            );
            assert!(
                build_manifest(
                    LOCAL.parse().unwrap(),
                    PEER.parse().unwrap(),
                    &candidate("00000000-0000-0000-0000-000000000000"),
                    &options
                )
                .is_err()
            );
        }
    }

    #[test]
    fn pure_manifest_export_audio_preflight_matches_address_and_duration_policy() {
        for kind in [
            ExportKind::Playback,
            ExportKind::VirtualMicrophone,
            ExportKind::Capture,
            ExportKind::VirtualSpeaker,
        ] {
            for ip in [
                "127.0.0.1",
                "192.168.1.2",
                "10.2.3.4",
                "100.64.1.2",
                "100.127.1.2",
            ] {
                for listen_port in [0, 2, 65535] {
                    for duration in [1, 300] {
                        let mut options = options(kind);
                        options.listen = format!("{ip}:{listen_port}").parse().unwrap();
                        options.media = Some(format!("{ip}:43198").parse().unwrap());
                        options.duration_secs = duration;
                        check_option_parity(&options, true);
                    }
                }
            }
            for (listen, media) in [
                ("0.0.0.0:0", "0.0.0.0:43198"),
                ("8.8.8.8:0", "8.8.8.8:43198"),
                ("169.254.1.2:0", "169.254.1.2:43198"),
                ("100.63.1.2:0", "100.63.1.2:43198"),
                ("100.128.1.2:0", "100.128.1.2:43198"),
                ("192.168.1.255:0", "192.168.1.255:43198"),
                ("127.0.0.1:0", "127.0.0.2:43198"),
                ("127.0.0.1:0", "127.0.0.1:0"),
                ("127.0.0.1:0", "127.0.0.1:1"),
                ("127.0.0.1:43198", "127.0.0.1:43198"),
                ("[::1]:0", "[::1]:43198"),
                ("127.0.0.1:0", "[::1]:43198"),
            ] {
                let mut options = options(kind);
                options.listen = listen.parse().unwrap();
                options.media = Some(media.parse().unwrap());
                check_option_parity(&options, false);
            }
            let mut absent = options(kind);
            absent.media = None;
            check_option_parity(&absent, false);
            for duration in [0, 301, u64::MAX] {
                let mut options = options(kind);
                options.duration_secs = duration;
                check_option_parity(&options, false);
            }
        }
    }

    #[test]
    fn pure_manifest_export_requires_physical_targets_and_forbids_virtual_targets() {
        for kind in [ExportKind::Playback, ExportKind::Capture] {
            for target in ["physical.target", "alsa_output.usb-1:2", "12", "a_b-c.d:1"] {
                let mut options = options(kind);
                options.target = Some(target.into());
                check_option_parity(&options, true);
            }
            for target in [
                "",
                "0",
                "auto",
                "AUTO",
                "default",
                "Default",
                "-physical",
                "bad target",
                "bad/target",
                "\u{202e}",
                "\x1b",
            ] {
                let mut options = options(kind);
                options.target = Some(target.into());
                check_option_parity(&options, false);
            }
            let mut missing = options(kind);
            missing.target = None;
            check_option_parity(&missing, false);
            let mut boundary = options(kind);
            boundary.target = Some("a".repeat(128));
            check_option_parity(&boundary, true);
            boundary.target = Some("a".repeat(129));
            check_option_parity(&boundary, false);
        }
        for kind in [ExportKind::VirtualMicrophone, ExportKind::VirtualSpeaker] {
            for target in ["", "synthetic.target", "auto"] {
                let mut options = options(kind);
                options.target = Some(target.into());
                check_option_parity(&options, false);
            }
        }
    }

    #[test]
    fn pure_manifest_export_touchpad_matches_its_distinct_policy_and_fixed_limits() {
        for kind in [ExportKind::Camera, ExportKind::Touchpad] {
            for listen in ["127.0.0.1:0", "192.168.1.2:65535", "169.254.1.2:0"] {
                for duration in [3, 60] {
                    let mut options = options(kind);
                    options.listen = listen.parse().unwrap();
                    options.duration_secs = duration;
                    check_option_parity(&options, true);
                }
            }
            for listen in [
                "0.0.0.0:0",
                "8.8.8.8:44",
                "100.64.1.2:0",
                "224.0.0.1:44",
                "255.255.255.255:44",
                "[::1]:0",
            ] {
                let mut options = options(kind);
                options.listen = listen.parse().unwrap();
                check_option_parity(&options, false);
            }
            for duration in [0, 1, 2, 61, u64::MAX] {
                let mut options = options(kind);
                options.duration_secs = duration;
                check_option_parity(&options, false);
            }
            let mut audio_option = options(kind);
            audio_option.media = Some("127.0.0.1:43198".parse().unwrap());
            check_option_parity(&audio_option, false);
            audio_option.media = None;
            audio_option.target = Some("synthetic.target".into());
            check_option_parity(&audio_option, false);
        }
        let limits = ExportKind::Touchpad.limits();
        for text in [
            "five contacts",
            "max_contacts=5",
            "120 x 70 mm",
            "pressure supported",
            "not clickable",
            "tls13_mtls_v1",
            "neither frame delivery nor OS gestures",
        ] {
            assert!(limits.contains(text), "{text}");
        }
        assert!(!limits.contains("UDP"));
        assert!(!limits.contains("120-frame"));
        assert_eq!(ExportKind::Camera.limits(), LIMITS);
        for kind in [
            ExportKind::Playback,
            ExportKind::VirtualMicrophone,
            ExportKind::Capture,
            ExportKind::VirtualSpeaker,
        ] {
            let limits = kind.limits();
            assert!(limits.contains(
                "WARNING: plaintext unauthenticated UDP media; trusted isolated lab only."
            ));
            assert!(limits.contains("unauthenticated_udp_trusted_lab_only"));
            assert!(limits.contains(if kind.capture() {
                "pcm-s16le-48000-stereo"
            } else {
                "pcm-s16le-48000-mono"
            }));
            assert!(!limits.contains("tls13_mtls_v1"));
        }
    }

    #[test]
    fn pure_manifest_export_all_modes_share_exclusive_owner_private_storage() {
        let fixture = Fixture::new();
        for (index, kind) in EXPORT_KINDS.into_iter().enumerate() {
            let output = fixture.0.join(format!("manifest-{index}.json"));
            let options = options(kind);
            validate_export_options(&output, &options).unwrap();
            let bytes = built(&options).unwrap();
            let pinned = PinnedOutput::open(&output).unwrap();
            assert!(!output.exists());
            pinned.create(&bytes, future()).unwrap();
            assert_eq!(fs::read(&output).unwrap(), bytes);
            assert_eq!(fs::metadata(&output).unwrap().mode() & 0o777, 0o600);
            assert!(pinned.create(b"{}", future()).is_err());
            assert_eq!(fs::read(&output).unwrap(), bytes);
        }
    }
}
