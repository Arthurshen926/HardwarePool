//! Read-only, finite catalog inspection through the fixed sibling Node.
//! Authentication and catalog admission remain Node-owned. No resource command,
//! grant, lease, media worker or device is created by this controller. A separate
//! explicit two-confirmation action may export only a nonsecret launcher manifest.
use super::catalog_export::{Candidate, ExportKind, ExportOptions};
use super::*;
use capyio_core::{Availability, CapabilityId, PortDirection, PortId};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

// Includes the capability preflight. Node's existing Drop guard reserves at most
// another two seconds for cancellation/EOF, followed by killing/reaping its child.
const WORK_LIFETIME: Duration = Duration::from_secs(10);
const MAX_PORTS: usize = 64;
const MAX_EVENTS: usize = MAX_PORTS + 2;
const MAX_TOTAL: usize = 65_536;
const MAX_TEXT: usize = 512;
const MAX_FORMATS: usize = 16;
const MAX_PARAMETERS: usize = 16;
const NO_AUTHORITY: &str = "Read-only: no Routes, resource grants, leases, media or devices are created.\r\nCatalog visibility is not resource authorization, compatibility or delivery proof.\r\n";

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    name: String,
    major: u16,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Format {
    id: String,
    parameters: BTreeMap<String, String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum TextEncoding {
    AsciiEscaped,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum CompatibilityStatus {
    NeedsLocalSelection,
    NotSupported,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum CompatibilityReason {
    ResourceUnavailable,
    DirectionNotSupported,
    InteroperabilityNotSupported,
    BackendNotSupported,
    QosNotSupported,
    ProfileNotSupported,
    PlatformOrClassNotSupported,
    FormatNotSupported,
    AudioPlaybackNeedsLocalSelection,
    AudioCaptureNeedsLocalSelection,
    CameraNeedsLocalSelection,
    TouchpadNeedsLocalSelection,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Compatibility {
    status: CompatibilityStatus,
    reason: CompatibilityReason,
}
impl Compatibility {
    fn validate(&self) -> io::Result<()> {
        use CompatibilityReason::*;
        let selection = matches!(
            self.reason,
            AudioPlaybackNeedsLocalSelection
                | AudioCaptureNeedsLocalSelection
                | CameraNeedsLocalSelection
                | TouchpadNeedsLocalSelection
        );
        if selection != matches!(self.status, CompatibilityStatus::NeedsLocalSelection) {
            return Err(invalid());
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Begin {
    version: u32,
    peer_node_id: NodeId,
    port_count: usize,
    text_encoding: TextEncoding,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Port {
    version: u32,
    peer_node_id: NodeId,
    capability_id: CapabilityId,
    port_id: PortId,
    direction: PortDirection,
    profile: Profile,
    formats: Vec<Format>,
    capability_availability: Availability,
    port_availability: Availability,
    compatibility: Compatibility,
}
impl Port {
    fn validate(&self) -> io::Result<()> {
        if self.capability_id.as_uuid().is_nil()
            || self.port_id.as_uuid().is_nil()
            || self.profile.major == 0
            || !safe_text(&self.profile.name, false)
            || self.formats.len() > MAX_FORMATS
        {
            return Err(invalid());
        }
        for format in &self.formats {
            if !safe_text(&format.id, false)
                || format.parameters.len() > MAX_PARAMETERS
                || format
                    .parameters
                    .iter()
                    .any(|(key, value)| !safe_text(key, false) || !safe_text(value, true))
            {
                return Err(invalid());
            }
        }
        self.compatibility.validate()
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Complete {
    version: u32,
    peer_node_id: NodeId,
    port_count: usize,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Failed {
    version: u32,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
enum Event {
    CatalogBegin(Begin),
    CatalogPort(Port),
    CatalogComplete(Complete),
    Failed(Failed),
}
impl Event {
    fn parse(value: Value) -> io::Result<Self> {
        // Frames uses UniqueValue first, including every nested parameter map,
        // so duplicate JSON fields cannot disappear during typed decoding.
        let event: Self = serde_json::from_value(value).map_err(|_| invalid())?;
        let (version, peer) = match &event {
            Self::CatalogBegin(value) => {
                if value.port_count > MAX_PORTS {
                    return Err(invalid());
                }
                (value.version, Some(value.peer_node_id))
            }
            Self::CatalogPort(value) => {
                value.validate()?;
                (value.version, Some(value.peer_node_id))
            }
            Self::CatalogComplete(value) => {
                if value.port_count > MAX_PORTS {
                    return Err(invalid());
                }
                (value.version, Some(value.peer_node_id))
            }
            Self::Failed(value) => (value.version, None),
        };
        if version != 1 || peer.is_some_and(|id| id.as_uuid().is_nil()) {
            return Err(invalid());
        }
        Ok(event)
    }

    fn canonical(&self) -> io::Result<String> {
        let result = serde_json::to_string(self).map_err(|_| invalid())?;
        // Host escapes text before the boundary; the console never unescapes
        // it for display. This second check also excludes DEL, C1 controls,
        // bidi/zero-width Unicode and all raw terminal escape sequences.
        if result.len() > MAX_FRAME || !result.bytes().all(|byte| (32..=126).contains(&byte)) {
            return Err(invalid());
        }
        Ok(result)
    }
}

fn safe_text(value: &str, empty: bool) -> bool {
    value.len() <= MAX_TEXT
        && (empty || !value.trim().is_empty())
        && value.bytes().all(|byte| (32..=126).contains(&byte))
}

struct Catalog {
    peer: NodeId,
    expected: Option<usize>,
    ports: Vec<Event>,
    identifiers: BTreeSet<(CapabilityId, PortId)>,
    complete: Option<Event>,
}
impl Catalog {
    fn new(peer: NodeId) -> Self {
        Self {
            peer,
            expected: None,
            ports: Vec::new(),
            identifiers: BTreeSet::new(),
            complete: None,
        }
    }

    fn accept(&mut self, event: Event) -> io::Result<()> {
        if self.complete.is_some() {
            return Err(invalid());
        }
        match &event {
            Event::CatalogBegin(value) => {
                if self.expected.is_some() || value.peer_node_id != self.peer {
                    return Err(invalid());
                }
                self.expected = Some(value.port_count);
            }
            Event::CatalogPort(value) => {
                if value.peer_node_id != self.peer
                    || self.expected.is_none_or(|count| self.ports.len() >= count)
                    || !self
                        .identifiers
                        .insert((value.capability_id, value.port_id))
                {
                    return Err(invalid());
                }
                // Retain only bounded, strictly decoded public objects, and
                // expose none of a partial or subsequently rejected catalog.
                self.ports.push(event);
            }
            Event::CatalogComplete(value) => {
                if value.peer_node_id != self.peer
                    || self.expected != Some(value.port_count)
                    || self.ports.len() != value.port_count
                {
                    return Err(invalid());
                }
                self.complete = Some(event);
            }
            Event::Failed(_) => return Err(invalid()),
        }
        Ok(())
    }

    #[cfg(test)]
    fn candidates(&self, kind: ExportKind) -> io::Result<Vec<Candidate>> {
        self.security_candidates(kind, media::MediaSecurity::Legacy)
    }

    fn security_candidates(
        &self,
        kind: ExportKind,
        security: media::MediaSecurity,
    ) -> io::Result<Vec<Candidate>> {
        if security.is_secure()
            && !(if security == media::MediaSecurity::SecureStereo {
                kind == ExportKind::Capture
            } else {
                matches!(kind, ExportKind::Playback | ExportKind::VirtualMicrophone)
            })
        {
            return Err(invalid());
        }
        // No partial or subsequently rejected snapshot may become selectable.
        self.lines()?;
        let (direction, profile, format, reason) = match kind {
            ExportKind::Camera => (
                PortDirection::Source,
                "capyio.video.frames",
                "cavc-avc-access-unit-v1",
                CompatibilityReason::CameraNeedsLocalSelection,
            ),
            ExportKind::Playback | ExportKind::VirtualMicrophone => (
                PortDirection::Source,
                "capyio.audio.frames",
                "pcm-s16le-48000-mono",
                CompatibilityReason::AudioPlaybackNeedsLocalSelection,
            ),
            ExportKind::Capture | ExportKind::VirtualSpeaker => (
                PortDirection::Sink,
                "capyio.audio.frames",
                "pcm-s16le-48000-stereo",
                CompatibilityReason::AudioCaptureNeedsLocalSelection,
            ),
            ExportKind::Touchpad => (
                PortDirection::Source,
                "capyio.input.touchpad-frames",
                "touchpad-frame-v1",
                CompatibilityReason::TouchpadNeedsLocalSelection,
            ),
        };
        let format = security.secure_format().unwrap_or(format);
        self.ports
            .iter()
            .filter_map(|event| match event {
                Event::CatalogPort(port)
                    if port.peer_node_id == self.peer
                        && port.direction == direction
                        && port.profile.name == profile
                        && port.profile.major == 1
                        && matches!(port.capability_availability, Availability::Available)
                        && matches!(port.port_availability, Availability::Available)
                        && matches!(
                            port.compatibility.status,
                            CompatibilityStatus::NeedsLocalSelection
                        )
                        && port.compatibility.reason == reason
                        && (!security.is_secure() || port.formats.len() == 1)
                        && port.formats.iter().any(|candidate| {
                            candidate.id == format && candidate.parameters.is_empty()
                        }) =>
                {
                    Some(event.canonical().map(|display| Candidate {
                        peer: port.peer_node_id,
                        capability: port.capability_id,
                        port: port.port_id,
                        display,
                    }))
                }
                _ => None,
            })
            .collect()
    }

    #[cfg(test)]
    fn camera_candidates(&self) -> io::Result<Vec<Candidate>> {
        self.candidates(ExportKind::Camera)
    }

    fn lines(&self) -> io::Result<Vec<String>> {
        let complete = self.complete.as_ref().ok_or_else(invalid)?;
        let mut lines = Vec::with_capacity(self.ports.len() + 1);
        for port in &self.ports {
            lines.push(format!("Catalog port: {}\r\n", port.canonical()?));
        }
        lines.push(format!("Catalog complete: {}\r\n", complete.canonical()?));
        Ok(lines)
    }
}

// Account for original bytes, not reserialized JSON: whitespace and prefixes
// count too. Probe only one byte past the aggregate bound, then fail closed.
struct Counted<'a, R> {
    input: &'a mut R,
    total: &'a mut usize,
}
impl<R: Read> Read for Counted<'_, R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if *self.total > MAX_TOTAL {
            return Err(invalid());
        }
        let available = output.len().min(MAX_TOTAL - *self.total + 1);
        let count = self.input.read(&mut output[..available])?;
        *self.total += count;
        if *self.total > MAX_TOTAL {
            return Err(invalid());
        }
        Ok(count)
    }
}

fn poll_node(node: &mut Node, total: &mut usize, deadline: Instant) -> io::Result<Option<Value>> {
    let mut discard = [0; 1024];
    while let Some(error) = &mut node.error {
        check(deadline)?;
        match error.read(&mut discard) {
            Ok(0) => {
                node.error = None;
                break;
            }
            Ok(count) => {
                node.error_bytes += count;
                if node.error_bytes > 8192 {
                    discard.fill(0);
                    return Err(invalid());
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => {
                discard.fill(0);
                return Err(invalid());
            }
        }
    }
    discard.fill(0);
    node.frames.poll(&mut Counted {
        input: &mut node.output,
        total,
    })
}

fn validate_selection(local: NodeId, peer: NodeId, endpoint: SocketAddr) -> io::Result<()> {
    let SocketAddr::V4(address) = endpoint else {
        return Err(invalid());
    };
    let ip = address.ip();
    if local.as_uuid().is_nil()
        || peer.as_uuid().is_nil()
        || local == peer
        || address.port() == 0
        || !(ip.is_loopback() || ip.is_private() || ip.is_link_local())
        || ip.is_broadcast()
        || ip.is_unspecified()
    {
        return Err(invalid());
    }
    Ok(())
}

fn cancellation(line: &[u8]) -> io::Result<()> {
    if line == b"cancel" {
        Ok(())
    } else {
        // There is deliberately no approval, status, selection or Start command.
        Err(invalid())
    }
}

fn check_cancellation(tty: &mut Terminal) -> io::Result<()> {
    if let Some(line) = tty.poll_line()? {
        cancellation(line.as_bytes())?;
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "catalog inspection cancelled",
        ));
    }
    Ok(())
}

// This is the sole actual catalog read path for inspection and preparation.
// It returns only after successful complete/EOF validation and child reap; all
// pipes and the Node are dropped before the caller can offer any selection.
fn read_catalog(
    args: &Args,
    peer: NodeId,
    endpoint: SocketAddr,
    tty: &mut Terminal,
    deadline: Instant,
) -> io::Result<Catalog> {
    let mut node = Node::spawn_for(args, "linux-catalog", Some(peer))?;
    // Any extra input cancels this read-only Node operation. Use the existing
    // cancel/EOF guard rather than sending a media Stop or resource command.
    node.stop_on_drop = false;
    node.set_frame_limit(MAX_EVENTS)?;
    let bootstrap = deadline.min(Instant::now() + Duration::from_secs(1));
    node.send(
        &json!({"version":1,"local_node_id":args.pairing_node,"peer_node_id":peer,"endpoint":endpoint}),
        bootstrap,
    )?;
    node.send(
        &json!({"version":1,"credential_source":"paired-store"}),
        bootstrap,
    )?;
    let mut total = 0;
    let mut catalog = Catalog::new(peer);
    let mut exit_status = None;
    loop {
        check(deadline)?;
        if let Some(value) = poll_node(&mut node, &mut total, deadline)? {
            catalog.accept(Event::parse(value)?)?;
        }
        check_cancellation(tty)?;
        if exit_status.is_none()
            && let Some(status) = node.child.try_wait()?
        {
            node.reaped = true;
            exit_status = Some(status);
        }
        if let Some(status) = exit_status {
            if !status.success() {
                return Err(invalid());
            }
            // Require actual EOF on both inherited outputs, strict successful
            // completion and no trailing complete/partial event before display.
            if node.frames.eof && node.error.is_none() {
                if !node.frames.bytes.is_empty() {
                    return Err(invalid());
                }
                catalog.lines()?;
                drop(node);
                return Ok(catalog);
            }
        }
        thread::sleep(Duration::from_millis(5));
    }
}

pub(super) fn run(args: &Args, peer: NodeId, endpoint: SocketAddr) -> io::Result<()> {
    let deadline = Instant::now() + WORK_LIFETIME;
    validate_selection(args.pairing_node, peer, endpoint)?;
    let _signals = Signals::install()?;
    require_node_capability_for("linux-catalog")?;
    check(deadline)?;
    let mut tty = Terminal::open()?;
    tty.begin_prompt(
        &format!(
            "Read-only paired catalog inspection.\r\nRequested peer: {peer}\r\nExplicit endpoint: {endpoint}\r\n{NO_AUTHORITY}Peer text is ASCII-escaped and displayed literally.\r\nType cancel, Ctrl-C or Ctrl-D to stop: "
        ),
        deadline.min(Instant::now() + Duration::from_secs(1)),
    )?;
    let catalog = read_catalog(args, peer, endpoint, &mut tty, deadline)?;
    tty.write("\r\n", deadline)?;
    for line in catalog.lines()? {
        check_cancellation(&mut tty)?;
        tty.write(&line, deadline)?;
    }
    tty.write(NO_AUTHORITY, deadline)?;
    Ok(())
}

pub(super) fn run_prepare_camera(
    args: &Args,
    peer: NodeId,
    endpoint: SocketAddr,
    output: &Path,
    listen: SocketAddr,
    duration_secs: u64,
) -> io::Result<()> {
    run_prepare(
        args,
        peer,
        endpoint,
        output,
        &super::catalog_export::camera_options(listen, duration_secs),
    )
}

pub(super) fn run_prepare(
    args: &Args,
    peer: NodeId,
    endpoint: SocketAddr,
    output: &Path,
    options: &ExportOptions,
) -> io::Result<()> {
    let deadline = Instant::now() + WORK_LIFETIME;
    validate_selection(args.pairing_node, peer, endpoint)?;
    // Reject syntactic output-path, mode, target, address and duration errors
    // before spawning a Node, opening credentials or connecting. Filesystem
    // checks remain after selection and before the Save prompt or file create.
    super::catalog_export::validate_export_options(output, options)?;
    let _signals = Signals::install()?;
    require_node_capability_for("linux-catalog")?;
    check(deadline)?;
    let mut tty = Terminal::open()?;
    let name = options.kind.name();
    tty.begin_prompt(
        &format!(
            "Prepare nonsecret {name} manifest from one authenticated catalog snapshot.\r\nRequested peer: {peer}\r\nExplicit endpoint: {endpoint}\r\n{NO_AUTHORITY}Peer text is ASCII-escaped and displayed literally.\r\nType cancel, Ctrl-C or Ctrl-D to stop: "
        ),
        deadline.min(Instant::now() + Duration::from_secs(1)),
    )?;
    let catalog = read_catalog(args, peer, endpoint, &mut tty, deadline)?;
    let candidates = catalog.security_candidates(options.kind, options.media_security)?;
    super::catalog_export::prepare(
        &mut tty,
        args.pairing_node,
        peer,
        &candidates,
        output,
        options,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    const LOCAL: &str = "11111111-1111-4111-8111-111111111111";
    const PEER: &str = "22222222-2222-4222-8222-222222222222";
    const CAPABILITY: &str = "33333333-3333-4333-8333-333333333333";
    const PORT: &str = "44444444-4444-4444-8444-444444444444";

    fn peer() -> NodeId {
        PEER.parse().unwrap()
    }

    fn begin(count: usize) -> Value {
        json!({"version":1,"event":"catalog_begin","peer_node_id":PEER,"port_count":count,"text_encoding":"ascii_escaped"})
    }

    fn port() -> Value {
        json!({
            "version":1,"event":"catalog_port","peer_node_id":PEER,
            "capability_id":CAPABILITY,"port_id":PORT,"direction":"source",
            "profile":{"name":"capyio.audio.frames","major":1},
            "formats":[{"id":"pcm-s16le-48000-mono","parameters":{"rate":"48000"}}],
            "capability_availability":"available","port_availability":"available",
            "compatibility":{"status":"needs_local_selection","reason":"audio_playback_needs_local_selection"}
        })
    }

    fn complete(count: usize) -> Value {
        json!({"version":1,"event":"catalog_complete","peer_node_id":PEER,"port_count":count})
    }

    fn accept(catalog: &mut Catalog, value: Value) -> io::Result<()> {
        catalog.accept(Event::parse(value)?)
    }

    fn frame(bytes: &[u8]) -> Vec<u8> {
        let mut result = (bytes.len() as u32).to_be_bytes().to_vec();
        result.extend_from_slice(bytes);
        result
    }

    fn decode(bytes: &[u8]) -> io::Result<Option<Value>> {
        Frames::default().poll(&mut Cursor::new(bytes))
    }

    #[test]
    fn pure_catalog_empty_and_complete_catalog_are_buffered_and_read_only() {
        for count in [0, 1] {
            let mut catalog = Catalog::new(peer());
            assert!(catalog.lines().is_err());
            accept(&mut catalog, begin(count)).unwrap();
            if count == 1 {
                accept(&mut catalog, port()).unwrap();
            }
            assert!(catalog.lines().is_err());
            accept(&mut catalog, complete(count)).unwrap();
            let lines = catalog.lines().unwrap();
            assert_eq!(lines.len(), count + 1);
            assert!(lines.last().unwrap().starts_with("Catalog complete: "));
            assert!(lines.iter().all(|line| !line.contains("\"command\"")));
            assert!(NO_AUTHORITY.contains("no Routes, resource grants, leases, media or devices"));
            assert!(
                NO_AUTHORITY
                    .contains("not resource authorization, compatibility or delivery proof")
            );
            assert!(accept(&mut catalog, complete(count)).is_err());
        }
        assert!(cancellation(b"cancel").is_ok());
        for input in [
            b"approve".as_slice(),
            b"start",
            b"select",
            b"status",
            b"yes",
        ] {
            assert!(cancellation(input).is_err());
        }
    }

    #[test]
    fn pure_catalog_controls_unicode_and_unknown_nested_fields_fail_closed() {
        for text in [
            "\x1b[2J",
            "\x7f",
            "\u{009b}31m",
            "\u{202e}spoof",
            "\u{200b}",
            "\n",
        ] {
            for path in [
                "/profile/name",
                "/formats/0/id",
                "/formats/0/parameters/rate",
            ] {
                let mut value = port();
                *value.pointer_mut(path).unwrap() = json!(text);
                assert!(Event::parse(value).is_err());
            }
        }
        for path in ["", "/profile", "/formats/0", "/compatibility"] {
            let mut value = port();
            value.pointer_mut(path).unwrap()["extra"] = json!(true);
            assert!(Event::parse(value).is_err());
        }
        let mut value = port();
        value["formats"][0]["parameters"] = json!({"\u{202e}":"value"});
        assert!(Event::parse(value).is_err());
        let mut value = port();
        value["profile"]["name"] = json!("escaped\\u{202e}\\n\\x1b[2J");
        let canonical = Event::parse(value).unwrap().canonical().unwrap();
        assert!(canonical.is_ascii());
        assert!(canonical.bytes().all(|byte| (32..=126).contains(&byte)));
        assert!(canonical.contains("escaped\\\\u{202e}"));
    }

    #[test]
    fn pure_catalog_rejects_duplicate_json_keys_and_port_identifiers() {
        for bytes in [
            br#"{"version":1,"version":1,"event":"failed"}"#.as_slice(),
            br#"{"formats":[{"parameters":{"key":"one","key":"two"}}]}"#,
        ] {
            assert!(decode(&frame(bytes)).is_err());
        }
        let mut catalog = Catalog::new(peer());
        accept(&mut catalog, begin(2)).unwrap();
        accept(&mut catalog, port()).unwrap();
        assert!(accept(&mut catalog, port()).is_err());
        assert!(catalog.lines().is_err());
        let mut value = port();
        value["formats"] = json!([value["formats"][0], value["formats"][0]]);
        // A format ID may advertise more than one exact parameter candidate.
        assert!(Event::parse(value).is_ok());
    }

    #[test]
    fn pure_catalog_rejects_wrong_peer_order_count_and_authority_claims() {
        for mut value in [begin(1), port(), complete(1)] {
            value["peer_node_id"] = json!(LOCAL);
            let mut catalog = Catalog::new(peer());
            if value["event"] != "catalog_begin" {
                accept(&mut catalog, begin(1)).unwrap();
            }
            assert!(accept(&mut catalog, value).is_err());
        }
        let mut catalog = Catalog::new(peer());
        assert!(accept(&mut catalog, port()).is_err());
        assert!(accept(&mut catalog, complete(0)).is_err());
        accept(&mut catalog, begin(1)).unwrap();
        assert!(accept(&mut catalog, begin(1)).is_err());
        assert!(accept(&mut catalog, complete(1)).is_err());
        accept(&mut catalog, port()).unwrap();
        assert!(accept(&mut catalog, complete(0)).is_err());
        for status in ["ready", "compatible", "authorized", "active"] {
            let mut value = port();
            value["compatibility"]["status"] = json!(status);
            assert!(Event::parse(value).is_err());
        }
        let mut value = port();
        value["compatibility"]["status"] = json!("not_supported");
        assert!(Event::parse(value).is_err());
        assert!(accept(&mut catalog, json!({"version":1,"event":"failed"})).is_err());
    }

    #[test]
    fn pure_catalog_truncated_frames_and_missing_completion_are_not_rendered() {
        let bytes = frame(&serde_json::to_vec(&complete(0)).unwrap());
        for length in [1, 3, 4, bytes.len() - 1] {
            let mut frames = Frames::default();
            let mut input = Cursor::new(&bytes[..length]);
            assert!(frames.poll(&mut input).unwrap().is_none());
            assert!(frames.poll(&mut input).is_err());
        }
        let mut catalog = Catalog::new(peer());
        accept(&mut catalog, begin(1)).unwrap();
        accept(&mut catalog, port()).unwrap();
        assert!(catalog.lines().is_err());
        assert!(decode(&((MAX_FRAME + 1) as u32).to_be_bytes()).is_err());
        assert!(decode(&0_u32.to_be_bytes()).is_err());
    }

    #[test]
    fn pure_catalog_aggregate_bound_counts_original_whitespace_and_prefixes() {
        let payload = br#"{"version":1,"event":"failed"}"#;
        let framed = frame(payload);
        let mut frames = Frames::default();
        let mut total = MAX_TOTAL - framed.len();
        let mut input = Cursor::new(&framed);
        assert!(
            frames
                .poll(&mut Counted {
                    input: &mut input,
                    total: &mut total
                })
                .unwrap()
                .is_some()
        );
        assert_eq!(total, MAX_TOTAL);
        assert!(
            frames
                .poll(&mut Counted {
                    input: &mut input,
                    total: &mut total
                })
                .unwrap()
                .is_none()
        );
        let mut padded = payload.to_vec();
        padded.push(b' ');
        let mut frames = Frames::default();
        let mut total = MAX_TOTAL - framed.len();
        let mut input = Cursor::new(frame(&padded));
        assert!(
            frames
                .poll(&mut Counted {
                    input: &mut input,
                    total: &mut total
                })
                .is_err()
        );
    }

    #[test]
    fn pure_catalog_accepts_all_sixty_four_ports_with_exact_framed_bounds() {
        let mut values = vec![begin(MAX_PORTS)];
        for index in 0..MAX_PORTS {
            let mut value = port();
            value["port_id"] = json!(format!("44444444-4444-4444-8444-{:012x}", index + 1));
            values.push(value);
        }
        values.push(complete(MAX_PORTS));
        let bytes = values
            .iter()
            .flat_map(|value| frame(&serde_json::to_vec(value).unwrap()))
            .collect::<Vec<_>>();
        assert!(bytes.len() <= MAX_TOTAL);
        let mut input = Cursor::new(bytes);
        let mut frames = Frames {
            limit: MAX_EVENTS,
            ..Frames::default()
        };
        let mut catalog = Catalog::new(peer());
        let mut total = 0;
        while !frames.eof {
            if let Some(value) = frames
                .poll(&mut Counted {
                    input: &mut input,
                    total: &mut total,
                })
                .unwrap()
            {
                accept(&mut catalog, value).unwrap();
            }
        }
        assert_eq!(frames.count, MAX_EVENTS);
        assert_eq!(total, input.get_ref().len());
        assert_eq!(catalog.lines().unwrap().len(), MAX_PORTS + 1);
        assert!(frames.bytes.is_empty());
    }

    #[test]
    fn pure_catalog_schema_bounds_and_explicit_selection_are_strict() {
        assert!(Event::parse(begin(MAX_PORTS + 1)).is_err());
        assert!(Event::parse(complete(MAX_PORTS + 1)).is_err());
        for path in [
            "/profile/name",
            "/formats/0/id",
            "/formats/0/parameters/rate",
        ] {
            let mut value = port();
            *value.pointer_mut(path).unwrap() = json!("x".repeat(MAX_TEXT + 1));
            assert!(Event::parse(value).is_err());
        }
        let mut value = port();
        value["formats"] = json!(
            (0..=MAX_FORMATS)
                .map(|n| json!({"id":n.to_string(),"parameters":{}}))
                .collect::<Vec<_>>()
        );
        assert!(Event::parse(value).is_err());
        let mut value = port();
        value["formats"][0]["parameters"] = json!(
            (0..=MAX_PARAMETERS)
                .map(|n| (n.to_string(), "value"))
                .collect::<BTreeMap<_, _>>()
        );
        assert!(Event::parse(value).is_err());
        let local = LOCAL.parse().unwrap();
        for endpoint in ["127.0.0.1:443", "192.168.1.2:443", "169.254.1.2:443"] {
            assert!(validate_selection(local, peer(), endpoint.parse().unwrap()).is_ok());
        }
        for endpoint in [
            "127.0.0.1:0",
            "0.0.0.0:443",
            "255.255.255.255:443",
            "224.0.0.1:443",
            "8.8.8.8:443",
            "[::1]:443",
        ] {
            assert!(validate_selection(local, peer(), endpoint.parse().unwrap()).is_err());
        }
        assert!(validate_selection(local, local, "127.0.0.1:443".parse().unwrap()).is_err());
    }

    fn camera_port() -> Value {
        let mut value = port();
        value["profile"] = json!({"name":"capyio.video.frames","major":1});
        value["formats"] = json!([{"id":"cavc-avc-access-unit-v1","parameters":{}}]);
        value["compatibility"]["reason"] = json!("camera_needs_local_selection");
        value
    }

    #[test]
    fn pure_catalog_camera_export_requires_complete_exact_eligible_rows() {
        let mut catalog = Catalog::new(peer());
        assert!(catalog.camera_candidates().is_err());
        accept(&mut catalog, begin(2)).unwrap();
        accept(&mut catalog, camera_port()).unwrap();
        assert!(catalog.camera_candidates().is_err());
        let mut second = camera_port();
        second["port_id"] = json!("55555555-5555-4555-8555-555555555555");
        accept(&mut catalog, second).unwrap();
        accept(&mut catalog, complete(2)).unwrap();
        let candidates = catalog.camera_candidates().unwrap();
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].peer, peer());
        assert_eq!(candidates[0].port.to_string(), PORT);
        assert_ne!(candidates[0].port, candidates[1].port);
        for (pointer, replacement) in [
            ("/direction", json!("sink")),
            ("/direction", json!("control")),
            ("/profile/name", json!("capyio.audio.frames")),
            ("/profile/major", json!(2)),
            ("/capability_availability", json!("busy")),
            ("/capability_availability", json!("permission_required")),
            ("/port_availability", json!("offline")),
            ("/port_availability", json!("busy")),
            ("/formats/0/id", json!("raw-nv12")),
            ("/formats/0/parameters", json!({"ignored":"not-empty"})),
            ("/formats", json!([])),
            (
                "/compatibility/reason",
                json!("audio_playback_needs_local_selection"),
            ),
            (
                "/compatibility",
                json!({"status":"not_supported","reason":"format_not_supported"}),
            ),
        ] {
            let mut value = camera_port();
            *value.pointer_mut(pointer).unwrap() = replacement;
            let mut catalog = Catalog::new(peer());
            accept(&mut catalog, begin(1)).unwrap();
            accept(&mut catalog, value).unwrap();
            accept(&mut catalog, complete(1)).unwrap();
            assert!(catalog.camera_candidates().unwrap().is_empty(), "{pointer}");
        }
        let mut empty = Catalog::new(peer());
        accept(&mut empty, begin(0)).unwrap();
        accept(&mut empty, complete(0)).unwrap();
        assert!(empty.camera_candidates().unwrap().is_empty());
    }

    fn export_port(kind: ExportKind) -> Value {
        let mut value = port();
        match kind {
            ExportKind::Camera => return camera_port(),
            ExportKind::Playback | ExportKind::VirtualMicrophone => {
                value["formats"] = json!([{"id":"pcm-s16le-48000-mono","parameters":{}}]);
            }
            ExportKind::Capture | ExportKind::VirtualSpeaker => {
                value["direction"] = json!("sink");
                value["formats"] = json!([{"id":"pcm-s16le-48000-stereo","parameters":{}}]);
                value["compatibility"]["reason"] = json!("audio_capture_needs_local_selection");
            }
            ExportKind::Touchpad => {
                value["profile"]["name"] = json!("capyio.input.touchpad-frames");
                value["formats"] = json!([{"id":"touchpad-frame-v1","parameters":{}}]);
                value["compatibility"]["reason"] = json!("touchpad_needs_local_selection");
            }
        }
        value
    }

    const EXPORT_KINDS: [ExportKind; 6] = [
        ExportKind::Camera,
        ExportKind::Playback,
        ExportKind::VirtualMicrophone,
        ExportKind::Capture,
        ExportKind::VirtualSpeaker,
        ExportKind::Touchpad,
    ];

    #[test]
    fn pure_secure_stereo_candidates_require_exact_single_sink_and_hint() {
        for (formats, expected) in [
            (
                json!([{"id":capyio_protocol::SECURE_AUDIO_STEREO_FORMAT,"parameters":{}}]),
                1,
            ),
            (
                json!([{"id":capyio_protocol::SECURE_AUDIO_STEREO_FORMAT,"parameters":{"x":"y"}}]),
                0,
            ),
            (
                json!([{"id":capyio_protocol::SECURE_AUDIO_STEREO_FORMAT,"parameters":{}},{"id":"pcm-s16le-48000-stereo","parameters":{}}]),
                0,
            ),
            (json!([{"id":"pcm-s16le-48000-stereo","parameters":{}}]), 0),
        ] {
            let mut value = export_port(ExportKind::Capture);
            value["formats"] = formats;
            let mut catalog = Catalog::new(peer());
            accept(&mut catalog, begin(1)).unwrap();
            accept(&mut catalog, value).unwrap();
            accept(&mut catalog, complete(1)).unwrap();
            assert_eq!(
                catalog
                    .security_candidates(ExportKind::Capture, media::MediaSecurity::SecureStereo)
                    .unwrap()
                    .len(),
                expected
            );
            assert!(
                catalog
                    .security_candidates(
                        ExportKind::VirtualSpeaker,
                        media::MediaSecurity::SecureStereo
                    )
                    .is_err()
            );
            assert!(
                catalog
                    .security_candidates(ExportKind::Playback, media::MediaSecurity::SecureStereo)
                    .is_err()
            );
        }
    }

    #[test]
    fn pure_secure_candidates_never_downgrade_or_select_parameterized_format() {
        for (format, parameters, legacy_count, secure_count) in [
            ("pcm-s16le-48000-mono", json!({}), 1, 0),
            (capyio_protocol::SECURE_AUDIO_MONO_FORMAT, json!({}), 0, 1),
            (
                capyio_protocol::SECURE_AUDIO_MONO_FORMAT,
                json!({"unknown":"required"}),
                0,
                0,
            ),
        ] {
            let mut catalog = Catalog::new(peer());
            accept(&mut catalog, begin(1)).unwrap();
            let mut port = export_port(ExportKind::Playback);
            port["formats"] = json!([{"id":format,"parameters":parameters}]);
            accept(&mut catalog, port).unwrap();
            accept(&mut catalog, complete(1)).unwrap();
            for kind in [ExportKind::Playback, ExportKind::VirtualMicrophone] {
                assert_eq!(catalog.candidates(kind).unwrap().len(), legacy_count);
                assert_eq!(
                    catalog
                        .security_candidates(kind, media::MediaSecurity::Secure)
                        .unwrap()
                        .len(),
                    secure_count
                );
            }
            for kind in [
                ExportKind::Camera,
                ExportKind::Capture,
                ExportKind::VirtualSpeaker,
                ExportKind::Touchpad,
            ] {
                assert!(
                    catalog
                        .security_candidates(kind, media::MediaSecurity::Secure)
                        .is_err()
                );
            }
        }
    }

    #[test]
    fn pure_secure_300_candidates_require_exact_singleton_empty_parameter_source() {
        let long = capyio_protocol::SECURE_AUDIO_MONO_300S_FORMAT;
        for (formats, expected) in [
            (json!([{"id":long,"parameters":{}}]), 1),
            (json!([{"id":long,"parameters":{"duration":"300"}}]), 0),
            (
                json!([{"id":capyio_protocol::SECURE_AUDIO_MONO_FORMAT,"parameters":{}}]),
                0,
            ),
            (
                json!([{"id":long,"parameters":{}},{"id":capyio_protocol::SECURE_AUDIO_MONO_FORMAT,"parameters":{}}]),
                0,
            ),
            (json!([{"id":"unknown-secure-profile","parameters":{}}]), 0),
        ] {
            let mut catalog = Catalog::new(peer());
            accept(&mut catalog, begin(1)).unwrap();
            let mut port = export_port(ExportKind::Playback);
            port["formats"] = formats;
            accept(&mut catalog, port).unwrap();
            accept(&mut catalog, complete(1)).unwrap();
            for kind in [ExportKind::Playback, ExportKind::VirtualMicrophone] {
                assert_eq!(
                    catalog
                        .security_candidates(kind, media::MediaSecurity::Secure300)
                        .unwrap()
                        .len(),
                    expected
                );
            }
            for kind in [
                ExportKind::Capture,
                ExportKind::VirtualSpeaker,
                ExportKind::Camera,
                ExportKind::Touchpad,
            ] {
                assert!(
                    catalog
                        .security_candidates(kind, media::MediaSecurity::Secure300)
                        .is_err()
                );
            }
        }
    }

    #[test]
    fn pure_secure_candidates_reject_mixed_format_publication() {
        let mut catalog = Catalog::new(peer());
        accept(&mut catalog, begin(1)).unwrap();
        let mut port = export_port(ExportKind::Playback);
        port["formats"] = json!([
            {"id":"pcm-s16le-48000-mono","parameters":{}},
            {"id":capyio_protocol::SECURE_AUDIO_MONO_FORMAT,"parameters":{}}
        ]);
        accept(&mut catalog, port).unwrap();
        accept(&mut catalog, complete(1)).unwrap();
        for kind in [ExportKind::Playback, ExportKind::VirtualMicrophone] {
            assert_eq!(catalog.candidates(kind).unwrap().len(), 1);
            assert!(
                catalog
                    .security_candidates(kind, media::MediaSecurity::Secure)
                    .unwrap()
                    .is_empty()
            );
        }
    }

    #[test]
    fn pure_secure_virtual_candidates_require_exact_source_contract() {
        for (pointer, replacement) in [
            ("/direction", json!("sink")),
            ("/profile/name", json!("unsupported.profile")),
            ("/profile/major", json!(2)),
            ("/capability_availability", json!("busy")),
            ("/port_availability", json!("offline")),
            (
                "/compatibility",
                json!({"status":"not_supported","reason":"format_not_supported"}),
            ),
            (
                "/compatibility/reason",
                json!("audio_capture_needs_local_selection"),
            ),
        ] {
            let mut port = export_port(ExportKind::VirtualMicrophone);
            port["formats"] =
                json!([{"id":capyio_protocol::SECURE_AUDIO_MONO_FORMAT,"parameters":{}}]);
            *port.pointer_mut(pointer).unwrap() = replacement;
            let mut catalog = Catalog::new(peer());
            accept(&mut catalog, begin(1)).unwrap();
            accept(&mut catalog, port).unwrap();
            assert!(
                catalog
                    .security_candidates(
                        ExportKind::VirtualMicrophone,
                        media::MediaSecurity::Secure
                    )
                    .is_err()
            );
            accept(&mut catalog, complete(1)).unwrap();
            assert!(
                catalog
                    .security_candidates(
                        ExportKind::VirtualMicrophone,
                        media::MediaSecurity::Secure
                    )
                    .unwrap()
                    .is_empty(),
                "{pointer}"
            );
        }
    }

    #[test]
    fn pure_catalog_all_export_modes_require_complete_matching_rows() {
        for kind in EXPORT_KINDS {
            let mut catalog = Catalog::new(peer());
            assert!(catalog.candidates(kind).is_err());
            accept(&mut catalog, begin(2)).unwrap();
            accept(&mut catalog, export_port(kind)).unwrap();
            assert!(catalog.candidates(kind).is_err());
            let mut second = export_port(kind);
            second["port_id"] = json!("55555555-5555-4555-8555-555555555555");
            accept(&mut catalog, second).unwrap();
            assert!(catalog.candidates(kind).is_err());
            accept(&mut catalog, complete(2)).unwrap();
            let candidates = catalog.candidates(kind).unwrap();
            assert_eq!(candidates.len(), 2);
            assert_eq!(candidates[0].peer, peer());
            assert_eq!(candidates[0].port.to_string(), PORT);
            assert_ne!(candidates[0].port, candidates[1].port);
            assert!(candidates.iter().all(|candidate| {
                candidate
                    .display
                    .bytes()
                    .all(|byte| (32..=126).contains(&byte))
                    && candidate.display.contains("\"event\":\"catalog_port\"")
            }));
            for other in EXPORT_KINDS {
                // Physical and virtual endpoints intentionally share one exact
                // remote contract, but no other format/direction may cross over.
                let same_contract = other == kind
                    || matches!(
                        (kind, other),
                        (ExportKind::Playback, ExportKind::VirtualMicrophone)
                            | (ExportKind::VirtualMicrophone, ExportKind::Playback)
                            | (ExportKind::Capture, ExportKind::VirtualSpeaker)
                            | (ExportKind::VirtualSpeaker, ExportKind::Capture)
                    );
                assert_eq!(
                    catalog.candidates(other).unwrap().len(),
                    if same_contract { 2 } else { 0 }
                );
            }
            let mut empty = Catalog::new(peer());
            accept(&mut empty, begin(0)).unwrap();
            accept(&mut empty, complete(0)).unwrap();
            assert!(empty.candidates(kind).unwrap().is_empty());
        }
    }

    #[test]
    fn pure_catalog_all_export_modes_reject_ineligible_hints_and_contracts() {
        for kind in EXPORT_KINDS {
            let original = export_port(kind);
            for (pointer, replacement) in [
                ("/direction", json!("control")),
                (
                    "/direction",
                    json!(if original["direction"] == "source" {
                        "sink"
                    } else {
                        "source"
                    }),
                ),
                ("/profile/name", json!("unsupported.profile")),
                ("/profile/major", json!(2)),
                ("/capability_availability", json!("busy")),
                ("/capability_availability", json!("permission_required")),
                ("/port_availability", json!("offline")),
                ("/port_availability", json!("busy")),
                ("/formats/0/id", json!("wrong-format")),
                ("/formats/0/parameters", json!({"rate":"48000"})),
                ("/formats", json!([])),
                (
                    "/compatibility/reason",
                    json!(if kind == ExportKind::Camera {
                        "touchpad_needs_local_selection"
                    } else {
                        "camera_needs_local_selection"
                    }),
                ),
                (
                    "/compatibility",
                    json!({"status":"not_supported","reason":"format_not_supported"}),
                ),
            ] {
                let mut value = export_port(kind);
                *value.pointer_mut(pointer).unwrap() = replacement;
                let mut catalog = Catalog::new(peer());
                accept(&mut catalog, begin(1)).unwrap();
                accept(&mut catalog, value).unwrap();
                accept(&mut catalog, complete(1)).unwrap();
                assert!(
                    catalog.candidates(kind).unwrap().is_empty(),
                    "{kind:?}: {pointer}"
                );
            }
        }
    }
}
