#!/usr/bin/env python3
"""Offline structural validation for the CapyIO bootstrap repository.

This script deliberately avoids dependency installation, compilation, device access and
privileged operations. It catches repository-shape, parser, compatibility and hygiene errors
before the full Rust/Tauri/platform toolchains are available.
"""

from __future__ import annotations

import json
import os
import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

REQUIRED_FILES = [
    "Cargo.toml",
    "rust-toolchain.toml",
    "AGENTS.md",
    "docs/PRODUCT_REQUIREMENTS.md",
    "docs/ARCHITECTURE.md",
    "docs/AUDIO_PROFILE.md",
    "docs/DATA_PLANE.md",
    "docs/PROTOCOL.md",
    "docs/SECURITY_MODEL.md",
    "docs/TESTING.md",
    "docs/BACKLOG.md",
    "docs/CAPY_AUDIO_NATIVE_001E_REPORT.md",
    "docs/REQUIREMENTS_TRACEABILITY.md",
    "docs/plans/TEMPLATE.md",
    "docs/adr/0046-connect-native-speaker-lab-path.md",
    "crates/capyio-core/src/lib.rs",
    "crates/capyio-data-plane/src/lib.rs",
    "adapters/sensor-server/src/lib.rs",
    "adapters/micyou/src/lib.rs",
    "adapters/native-audio-lan/Cargo.toml",
    "adapters/native-audio-lan/src/lib.rs",
    "adapters/native-audio-lan/src/codec.rs",
    "adapters/native-audio-lan/src/endpoint.rs",
    "adapters/native-audio-lan/src/lab.rs",
    "adapters/native-audio-lan/src/reassembly.rs",
    "adapters/native-audio-lan/src/bin/capyio-native-audio-tone.rs",
    "adapters/native-audio-lan/src/bin/capyio-native-virtual-speaker.rs",
    "adapters/native-audio-lan/src/bin/capyio-native-virtual-microphone.rs",
    "platform/windows/process-presence/Cargo.toml",
    "platform/windows/process-presence/src/lib.rs",
    "platform/windows/render-ring/Cargo.toml",
    "platform/windows/render-ring/src/lib.rs",
    "platform/windows/capture-ring/Cargo.toml",
    "platform/windows/capture-ring/src/lib.rs",
    "platform/windows/micyou-host-config/Cargo.toml",
    "platform/windows/micyou-host-config/src/lib.rs",
    "platform/windows/micyou-host-config/src/bin/capyio-micyou-config.rs",
    "crates/capyio-audio/src/lib.rs",
    "crates/capyio-runtime/src/lib.rs",
    "crates/capyio-protocol/src/lib.rs",
    "protocol/proto/capyio/v1/common.proto",
    "protocol/proto/capyio/v1/capability.proto",
    "protocol/proto/capyio/v1/control.proto",
    "fixtures/imu/imu_samples_v1.jsonl",
    "fixtures/sensor-server/accelerometer.json",
    "fixtures/sensor-server/gyroscope.json",
    "fixtures/audio/native_lan_v1_opus_single.hex",
    "apps/desktop/package.json",
    "apps/desktop/src/App.vue",
    "apps/desktop/src-tauri/tauri.conf.json",
    "apps/desktop/src-tauri/src/micyou_runtime.rs",
    "apps/desktop/src-tauri/src/microphone_quick_action.rs",
    "platform/android/gradlew",
    "platform/android/gradlew.bat",
    "platform/android/gradle/wrapper/gradle-wrapper.jar",
    "platform/android/gradle/wrapper/gradle-wrapper.properties",
    "platform/android/app/src/main/AndroidManifest.xml",
    "platform/android/app/src/main/java/dev/capyio/android/AudioNodeService.java",
    "platform/android/app/src/main/java/dev/capyio/android/NativeSpeakerConfigLoader.java",
    "platform/android/app/src/main/java/dev/capyio/android/NativeMicrophoneConfigLoader.java",
    "platform/android/app/src/main/java/dev/capyio/android/SpeakerSinkAdapter.java",
    "platform/android/node-contract/src/main/java/dev/capyio/android/contract/AudioNodeController.java",
    "platform/android/native-lan-contract/src/main/java/dev/capyio/android/lan/NativeLanPacketCodec.java",
    "platform/android/native-lan-contract/src/main/java/dev/capyio/android/lan/NativeLanPacketQueue.java",
    "platform/android/native-lan-contract/src/main/java/dev/capyio/android/lan/NativeLanPacketReassembler.java",
    "platform/android/native-lan-contract/src/main/java/dev/capyio/android/lan/NativeLanPcmPacketizer.java",
    "platform/android/native-lan-contract/src/main/java/dev/capyio/android/lan/NativeLanPcmSinkWorker.java",
    "platform/android/native-lan-contract/src/main/java/dev/capyio/android/lan/NativeLanSenderWorker.java",
    "platform/android/native-lan-contract/src/main/java/dev/capyio/android/lan/NativeLanReceiverWorker.java",
    "platform/android/native-lan-contract/src/main/java/dev/capyio/android/lan/NativeLanUdpEndpoint.java",
    "platform/android/native-lan-contract/src/main/java/dev/capyio/android/lan/NativeLanSpeakerSessionConfig.java",
    "platform/android/native-lan-contract/src/main/java/dev/capyio/android/lan/NativeLanMicrophoneSessionConfig.java",
]

REQUIREMENT_ID_RE = re.compile(r"(?:FR|NFR)-[A-Z]+-\d{3}")
REQUIREMENT_LIKE_RE = re.compile(
    r"(?<![A-Za-z0-9])(?:N?FR)[-_][A-Za-z0-9_-]+", re.IGNORECASE
)
REQUIREMENT_DEFINITION_RE = re.compile(
    r"^\s*-\s+\*\*((?P<id>(?:FR|NFR)-[A-Z]+-\d{3}))\*\*:"
)
TRACEABILITY_ROW_RE = re.compile(
    r"^\|\s*`?(?P<id>(?:FR|NFR)-[A-Z]+-\d{3})`?\s*"
    r"\|\s*(?P<status>planned|implemented|verified)\s*"
    r"\|\s*(?P<gate>[^|]+?)\s*\|\s*(?P<evidence>[^|]+?)\s*\|\s*$"
)
GATE_EVIDENCE_ROW_RE = re.compile(
    r"^\|\s*`?(?P<id>G0-3-\d{2})`?\s*\|\s*(?P<evidence>[^|]+?)\s*\|\s*$"
)
TARGET_GATE_RE = re.compile(r"^Gate(?:s)?\s+(?P<first>\d+)(?:[\-–](?P<last>\d+))?$")
FOUNDATION_ACCEPTANCE_IDS = {f"G0-3-{number:02d}" for number in range(1, 10)}
ACTIVE_IMPLEMENTATION_GATES = {5, 7, 8}

JSON_FILES = [
    "package.json",
    "apps/desktop/package.json",
    "apps/desktop/src-tauri/tauri.conf.json",
    "apps/desktop/src-tauri/capabilities/default.json",
    "protocol/examples/ui-snapshot.json",
    ".vscode/extensions.json",
    ".vscode/tasks.json",
]

TEXT_SUFFIXES = {
    ".c",
    ".cc",
    ".cpp",
    ".css",
    ".h",
    ".hpp",
    ".html",
    ".java",
    ".json",
    ".md",
    ".gradle",
    ".properties",
    ".proto",
    ".ps1",
    ".py",
    ".rs",
    ".toml",
    ".ts",
    ".tsx",
    ".vue",
    ".yaml",
    ".yml",
    ".xml",
}
TEXT_NAMES = {
    ".editorconfig",
    ".gitattributes",
    ".gitignore",
    "LICENSE",
}

# These directories contain local VCS state, downloaded dependencies, build
# products or test artifacts. They are intentionally excluded by .gitignore and
# are not part of the repository source that this offline validator audits.
IGNORED_DIRECTORY_NAMES = {
    ".agent-cache",
    ".codex",
    ".git",
    ".gradle",
    ".pnpm-store",
    ".vite",
    "artifacts",
    "build",
    "dist",
    "node_modules",
    "target",
    "test-results",
}


def validate_android_audio_shell() -> None:
    manifest = (ROOT / "platform/android/app/src/main/AndroidManifest.xml").read_text(
        encoding="utf-8"
    )
    required_manifest = [
        'android.permission.RECORD_AUDIO',
        'android.permission.POST_NOTIFICATIONS',
        'android.permission.FOREGROUND_SERVICE',
        'android.permission.FOREGROUND_SERVICE_MICROPHONE',
        'android.permission.FOREGROUND_SERVICE_MEDIA_PLAYBACK',
        'android.permission.INTERNET',
        'android:foregroundServiceType="microphone|mediaPlayback"',
        'android:exported="false"',
        'android:stopWithTask="false"',
        'android:usesCleartextTraffic="false"',
        'android:allowBackup="false"',
        'dev.capyio.android.SPEAKER_LAB_ENABLED',
        'dev.capyio.android.SPEAKER_LAB_PEER_IPV4',
        'dev.capyio.android.SPEAKER_LAB_STREAM_EPOCH',
        'dev.capyio.android.MICROPHONE_LAB_ENABLED',
        'dev.capyio.android.MICROPHONE_LAB_PEER_IPV4',
        'dev.capyio.android.MICROPHONE_LAB_STREAM_EPOCH',
    ]
    missing = [token for token in required_manifest if token not in manifest]
    if missing:
        fail(f"Android audio shell manifest is incomplete: {missing}")
    root_build = (ROOT / "platform/android/build.gradle").read_text(encoding="utf-8")
    if "com.android.application' version '9.3.1'" not in root_build:
        fail("Android Gradle Plugin pin changed without updating the 001C contract")
    app_build = (ROOT / "platform/android/app/build.gradle").read_text(encoding="utf-8")
    build_required = [
        "main.java.srcDir 'src/main/java'",
        "verifyDebugEntrypoints",
        "dev/capyio/android/MainActivity.class",
        "dev/capyio/android/AudioNodeService.class",
    ]
    missing_build = [token for token in build_required if token not in app_build]
    if missing_build:
        fail(f"Android application entrypoint gate is incomplete: {missing_build}")

    wrapper = (
        ROOT / "platform/android/gradle/wrapper/gradle-wrapper.properties"
    ).read_text(encoding="utf-8")
    wrapper_required = [
        "gradle-9.5.0-all.zip",
        "distributionSha256Sum="
        "a3c4ba4aca8f0075688b9c5b18939fd28e8cb4357c227da5c1d9f38343791439",
        "validateDistributionUrl=true",
    ]
    missing_wrapper = [token for token in wrapper_required if token not in wrapper]
    if missing_wrapper:
        fail(f"Android Gradle wrapper pin is incomplete: {missing_wrapper}")

    sources = "\n".join(
        (ROOT / relative_path).read_text(encoding="utf-8")
        for relative_path in [
            "platform/android/app/src/main/java/dev/capyio/android/AudioNodeService.java",
            "platform/android/app/src/main/java/dev/capyio/android/MicrophoneSourceAdapter.java",
            "platform/android/app/src/main/java/dev/capyio/android/SpeakerSinkAdapter.java",
        ]
    )
    source_required = [
        "START_NOT_STICKY",
        "startForeground(",
        "new AudioRecord.Builder()",
        "new AudioTrack.Builder()",
        "AudioTrack.WRITE_NON_BLOCKING",
        "NativeLanReceiverWorker",
        "NativeLanPcmSinkWorker",
        "NativeLanPcmPacketizer",
        "NativeLanSenderWorker",
    ]
    missing_source = [token for token in source_required if token not in sources]
    if missing_source:
        fail(f"Android platform audio shell is incomplete: {missing_source}")
    if "android.util.Log" in sources or "java.net." in sources:
        fail("Android audio callbacks must not log or use a network API")
    activity = (ROOT / "platform/android/app/src/main/java/dev/capyio/android/MainActivity.java").read_text(
        encoding="utf-8"
    )
    if "SPEAKER_LAB_" in activity or "MICROPHONE_LAB_" in activity or "InetSocketAddress" in activity:
        fail("Android Activity must not own native audio Route/network authority")
    for token in [
        "speakerTransportMetrics",
        "microphoneTransportMetrics",
        "AtomicBoolean refreshQueued",
        "MainActivity.this::scheduleRefresh",
        "snapshot.microphone().state().ownsForegroundLifecycle()",
        "snapshot.speaker().state().ownsForegroundLifecycle()",
    ]:
        if token not in activity:
            fail(f"Android audio UI lacks bounded diagnostics/refresh behavior: {token}")
    if "MainActivity.this::renderLatest" in activity:
        fail("Android audio UI must coalesce worker-driven state notifications")


def validate_native_audio_lan_backend() -> None:
    manifest = (ROOT / "adapters/native-audio-lan/Cargo.toml").read_text(
        encoding="utf-8"
    )
    for dependency in ["capyio-audio", "capyio-core", "thiserror.workspace", "uuid.workspace"]:
        if dependency not in manifest:
            fail(f"native audio LAN manifest lacks reviewed dependency: {dependency}")
    forbidden_dependencies = ["tokio", "quinn", "webrtc", "reqwest", "aoo"]
    present = [name for name in forbidden_dependencies if name in manifest.lower()]
    if present:
        fail(f"native audio LAN reference added an unreviewed dependency: {present}")

    rust_sources = "\n".join(
        (ROOT / relative_path).read_text(encoding="utf-8")
        for relative_path in [
            "adapters/native-audio-lan/src/lib.rs",
            "adapters/native-audio-lan/src/codec.rs",
            "adapters/native-audio-lan/src/endpoint.rs",
            "adapters/native-audio-lan/src/lab.rs",
            "adapters/native-audio-lan/src/reassembly.rs",
            "adapters/native-audio-lan/src/bin/capyio-native-audio-tone.rs",
            "adapters/native-audio-lan/src/bin/capyio-native-virtual-speaker.rs",
            "adapters/native-audio-lan/src/bin/capyio-native-virtual-microphone.rs",
        ]
    )
    rust_required = [
        '"dev.capyio.audio.lan-lab/1"',
        "AudioTransportInteroperability::AdapterManaged",
        "MAX_NATIVE_LAN_DATAGRAM_BYTES: usize = 1_200",
        "NATIVE_LAN_HEADER_BYTES: usize = 104",
        "MAX_NATIVE_LAN_FRAGMENTS: usize = 64",
        "MAX_NATIVE_LAN_INFLIGHT_PACKETS: usize = 8",
        "peer_authenticated: false",
        "confidentiality: false",
        "replay_protection: false",
        "UdpSocket",
        "NATIVE_SPEAKER_LAB_STREAM_EPOCH",
        "NATIVE_MICROPHONE_LAB_STREAM_EPOCH",
        "capyio-native-virtual-speaker",
        "capyio-native-virtual-microphone",
        "Global\\\\CapyIO.RenderRing.v1",
        "Global\\\\CapyIO.CaptureRing.v1",
    ]
    missing = [token for token in rust_required if token not in rust_sources]
    if missing:
        fail(f"native audio LAN Rust boundary is incomplete: {missing}")

    java_sources = "\n".join(
        (ROOT / relative_path).read_text(encoding="utf-8")
        for relative_path in [
            "platform/android/native-lan-contract/src/main/java/dev/capyio/android/lan/NativeLanPacketCodec.java",
            "platform/android/native-lan-contract/src/main/java/dev/capyio/android/lan/NativeLanPacketQueue.java",
            "platform/android/native-lan-contract/src/main/java/dev/capyio/android/lan/NativeLanPacketReassembler.java",
            "platform/android/native-lan-contract/src/main/java/dev/capyio/android/lan/NativeLanPcmPacketizer.java",
            "platform/android/native-lan-contract/src/main/java/dev/capyio/android/lan/NativeLanPcmSinkWorker.java",
            "platform/android/native-lan-contract/src/main/java/dev/capyio/android/lan/NativeLanSenderWorker.java",
            "platform/android/native-lan-contract/src/main/java/dev/capyio/android/lan/NativeLanReceiverWorker.java",
            "platform/android/native-lan-contract/src/main/java/dev/capyio/android/lan/NativeLanUdpEndpoint.java",
            "platform/android/native-lan-contract/src/main/java/dev/capyio/android/lan/NativeLanSpeakerSessionConfig.java",
            "platform/android/native-lan-contract/src/main/java/dev/capyio/android/lan/NativeLanMicrophoneSessionConfig.java",
        ]
    )
    java_required = [
        '"dev.capyio.audio.lan-lab/1"',
        "MAX_DATAGRAM_BYTES = 1_200",
        "HEADER_BYTES = 104",
        "MAX_FRAGMENTS = 64",
        "MAX_PACKET_CAPACITY = 128",
        "MAX_AGGREGATE_BYTES = 4 * 1024 * 1024",
        "MAX_INFLIGHT_PACKETS = 8",
        "MAX_PUSH_BYTES = 256 * 1024",
        "DatagramSocket",
        "Android audio callbacks never call",
        "peer.isUnresolved()",
        "capyio-native-lan-sender",
        "capyio-native-lan-receiver",
        "thread.join(2_000)",
        "pendingDiscontinuity = true",
        "capyio-native-lan-pcm-sink",
        "MAX_ZERO_WRITES = 1_000",
        "speaker peer must be a concrete unicast IPv4",
        "microphone peer must be a concrete unicast IPv4",
    ]
    missing = [token for token in java_required if token not in java_sources]
    if missing:
        fail(f"native audio LAN Android boundary is incomplete: {missing}")

    golden = (ROOT / "fixtures/audio/native_lan_v1_opus_single.hex").read_text(
        encoding="utf-8"
    ).split()
    if len(golden) != 120 or any(re.fullmatch(r"[0-9a-fA-F]{2}", byte) is None for byte in golden):
        fail("native audio LAN shared golden datagram must contain exactly 120 hex bytes")


def fail(message: str) -> None:
    print(f"ERROR: {message}", file=sys.stderr)
    raise SystemExit(1)


def relative(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


def is_ignored(path: Path) -> bool:
    parts = path.relative_to(ROOT).parts
    if any(part in IGNORED_DIRECTORY_NAMES for part in parts):
        return True
    return len(parts) >= 3 and parts[:2] == ("drivers", "windows-audio") and "x64" in parts


def repository_files() -> list[Path]:
    return sorted(path for path in repository_entries() if path.is_file())


def repository_entries():
    for directory, names, filenames in os.walk(ROOT, topdown=True, followlinks=False):
        parent = Path(directory)
        retained_names = []
        for name in names:
            path = parent / name
            if is_ignored(path):
                continue
            retained_names.append(name)
            yield path
        names[:] = retained_names
        for name in filenames:
            path = parent / name
            if not is_ignored(path):
                yield path


def validate_required_files() -> None:
    missing = [path for path in REQUIRED_FILES if not (ROOT / path).is_file()]
    if missing:
        fail(f"missing required files: {', '.join(missing)}")


def validate_no_symlinks() -> None:
    symlinks = [
        relative(path)
        for path in repository_entries()
        if path.is_symlink()
    ]
    if symlinks:
        fail(f"archive must not contain symlinks: {', '.join(symlinks)}")


def validate_utf8_and_line_endings() -> None:
    for path in repository_files():
        if path.suffix.lower() not in TEXT_SUFFIXES and path.name not in TEXT_NAMES:
            continue
        data = path.read_bytes()
        if b"\x00" in data:
            fail(f"unexpected NUL byte in text file {relative(path)}")
        try:
            data.decode("utf-8")
        except UnicodeDecodeError as error:
            fail(f"text file is not UTF-8: {relative(path)}: {error}")
        if b"\r\n" in data or b"\r" in data:
            fail(f"text file must use LF line endings: {relative(path)}")


def validate_json() -> None:
    for relative_path in JSON_FILES:
        path = ROOT / relative_path
        try:
            json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as error:
            fail(f"invalid JSON {relative_path}: {error}")


def validate_yaml() -> None:
    try:
        import yaml  # type: ignore[import-untyped]
    except ImportError:
        print("YAML validation: SKIP (PyYAML not installed)")
        return

    for path in sorted(
        path
        for path in repository_files()
        if path.suffix.lower() in {".yml", ".yaml"}
    ):
        try:
            yaml.safe_load(path.read_text(encoding="utf-8"))
        except (OSError, yaml.YAMLError) as error:
            fail(f"invalid YAML {relative(path)}: {error}")


def validate_python() -> None:
    for path in (path for path in repository_files() if path.suffix.lower() == ".py"):
        try:
            compile(path.read_text(encoding="utf-8"), relative(path), "exec")
        except (OSError, SyntaxError) as error:
            fail(f"invalid Python {relative(path)}: {error}")


def validate_toml_and_workspace() -> None:
    manifests: dict[Path, dict] = {}
    for path in (path for path in repository_files() if path.suffix.lower() == ".toml"):
        try:
            with path.open("rb") as handle:
                manifests[path] = tomllib.load(handle)
        except (OSError, tomllib.TOMLDecodeError) as error:
            fail(f"invalid TOML {relative(path)}: {error}")

    root_manifest = manifests.get(ROOT / "Cargo.toml")
    if root_manifest is None:
        fail("root Cargo.toml was not parsed")
    members = root_manifest.get("workspace", {}).get("members", [])
    if not isinstance(members, list) or not members:
        fail("Cargo workspace must declare members")
    if len(members) != len(set(members)):
        fail("Cargo workspace contains duplicate members")
    for member in members:
        member_manifest = ROOT / member / "Cargo.toml"
        if not member_manifest.is_file():
            fail(f"workspace member lacks Cargo.toml: {member}")


def parse_requirement_ids(text: str, source: str) -> dict[str, list[int]]:
    occurrences: dict[str, list[int]] = {}
    malformed: list[str] = []
    for line_number, line in enumerate(text.splitlines(), 1):
        candidates = REQUIREMENT_LIKE_RE.findall(line)
        definition = REQUIREMENT_DEFINITION_RE.match(line)
        for candidate in candidates:
            if REQUIREMENT_ID_RE.fullmatch(candidate) is None:
                malformed.append(f"{candidate}@{line_number}")
            elif definition is not None and definition.group("id") == candidate:
                continue
            elif re.match(
                rf"^\s*-\s+\**{re.escape(candidate)}\**(?:\s|:|$)", line
            ):
                malformed.append(f"{candidate}@{line_number} (not a canonical definition)")
        if definition is not None:
            requirement_id = definition.group("id")
            occurrences.setdefault(requirement_id, []).append(line_number)

    if malformed:
        raise ValueError(f"malformed Requirement IDs in {source}: {', '.join(malformed)}")

    duplicates = {
        requirement_id: lines
        for requirement_id, lines in occurrences.items()
        if len(lines) > 1
    }
    if duplicates:
        formatted = ", ".join(
            f"{requirement_id}@{lines}" for requirement_id, lines in sorted(duplicates.items())
        )
        raise ValueError(f"duplicate normative Requirement IDs in {source}: {formatted}")
    return occurrences


def parse_traceability_rows(
    text: str,
) -> tuple[dict[str, tuple[str, str, str, int]], dict[str, tuple[str, int]]]:
    requirements: dict[str, tuple[str, str, str, int]] = {}
    gate_evidence: dict[str, tuple[str, int]] = {}
    for line_number, line in enumerate(text.splitlines(), 1):
        if match := TRACEABILITY_ROW_RE.match(line):
            requirement_id = match.group("id")
            if requirement_id in requirements:
                previous = requirements[requirement_id][3]
                raise ValueError(
                    f"duplicate traceability row {requirement_id}@[{previous}, {line_number}]"
                )
            requirements[requirement_id] = (
                match.group("status"),
                match.group("gate").strip(),
                match.group("evidence").strip(),
                line_number,
            )
        elif match := GATE_EVIDENCE_ROW_RE.match(line):
            evidence_id = match.group("id")
            if evidence_id in gate_evidence:
                previous = gate_evidence[evidence_id][1]
                raise ValueError(
                    f"duplicate Gate evidence row {evidence_id}@[{previous}, {line_number}]"
                )
            gate_evidence[evidence_id] = (match.group("evidence").strip(), line_number)
    return requirements, gate_evidence


def validate_traceability_report(requirement_ids: set[str]) -> None:
    path = ROOT / "docs/REQUIREMENTS_TRACEABILITY.md"
    rows, gate_evidence = parse_traceability_rows(path.read_text(encoding="utf-8"))
    traced_ids = set(rows)
    missing = sorted(requirement_ids - traced_ids)
    unknown = sorted(traced_ids - requirement_ids)
    if missing or unknown:
        details = []
        if missing:
            details.append(f"missing: {', '.join(missing)}")
        if unknown:
            details.append(f"unknown: {', '.join(unknown)}")
        fail(f"Requirement traceability coverage mismatch ({'; '.join(details)})")

    for requirement_id, (status, target_gate, evidence, line_number) in rows.items():
        gate_match = TARGET_GATE_RE.fullmatch(target_gate)
        if gate_match is None:
            fail(
                f"invalid target Gate for {requirement_id} at "
                f"docs/REQUIREMENTS_TRACEABILITY.md:{line_number}: {target_gate}"
            )
        first_gate = int(gate_match.group("first"))
        last_gate = int(gate_match.group("last") or first_gate)
        if last_gate < first_gate:
            fail(f"reversed target Gate range for {requirement_id}: {target_gate}")
        if last_gate <= 3 and status not in {"implemented", "verified"}:
            fail(
                f"foundation Requirement {requirement_id} must be implemented or verified, "
                f"not {status}"
            )
        if (
            first_gate >= 4
            and first_gate not in ACTIVE_IMPLEMENTATION_GATES
            and status != "planned"
        ):
            fail(f"future Requirement {requirement_id} must be planned, not {status}")
        if not evidence or evidence in {"-", "—"}:
            fail(f"traceability evidence is empty for {requirement_id}")

    gate_evidence_ids = set(gate_evidence)
    missing_gate_evidence = sorted(FOUNDATION_ACCEPTANCE_IDS - gate_evidence_ids)
    unknown_gate_evidence = sorted(gate_evidence_ids - FOUNDATION_ACCEPTANCE_IDS)
    if missing_gate_evidence or unknown_gate_evidence:
        fail(
            "Gate 0–3 acceptance evidence mismatch "
            f"(missing={missing_gate_evidence}, unknown={unknown_gate_evidence})"
        )
    for evidence_id, (evidence, _) in gate_evidence.items():
        if not evidence or evidence in {"-", "—"}:
            fail(f"Gate acceptance evidence is empty for {evidence_id}")


def validate_requirement_ids() -> int:
    path = ROOT / "docs/PRODUCT_REQUIREMENTS.md"
    try:
        occurrences = parse_requirement_ids(path.read_text(encoding="utf-8"), relative(path))
    except ValueError as error:
        fail(str(error))
    if len(occurrences) < 40:
        fail(f"unexpectedly few normative Requirement IDs: {len(occurrences)}")
    validate_traceability_report(set(occurrences))
    return len(occurrences)


def run_requirement_parser_self_tests() -> None:
    duplicate = "\n".join(
        ["- **FR-TEST-001**: first", "- **FR-TEST-001**: duplicate"]
    )
    malformed = "- **FR-test-001**: lowercase category is invalid"
    unstructured = "- FR-TEST-001: missing canonical bold definition syntax"
    for label, fixture, expected in [
        ("duplicate", duplicate, "duplicate normative Requirement IDs"),
        ("malformed", malformed, "malformed Requirement IDs"),
        ("unstructured", unstructured, "not a canonical definition"),
    ]:
        try:
            parse_requirement_ids(fixture, f"self-test:{label}")
        except ValueError as error:
            if expected not in str(error):
                fail(f"Requirement parser {label} self-test returned the wrong error: {error}")
        else:
            fail(f"Requirement parser {label} self-test accepted invalid input")

    valid = parse_requirement_ids(
        "- **FR-TEST-001**: valid\n- **NFR-SAFE-002**: also valid",
        "self-test:valid",
    )
    if set(valid) != {"FR-TEST-001", "NFR-SAFE-002"}:
        fail("Requirement parser valid self-test returned the wrong IDs")
    print("Requirement parser self-tests: PASS (duplicate, malformed, canonical syntax)")


def validate_proto_field_numbers() -> None:
    message_re = re.compile(r"^\s*message\s+(\w+)\s*\{")
    field_re = re.compile(
        r"^\s*(?:repeated\s+|optional\s+)?[.\w<> ,]+\s+(\w+)\s*=\s*(\d+)\s*;"
    )

    for path in sorted((ROOT / "protocol/proto").rglob("*.proto")):
        text = path.read_text(encoding="utf-8")
        if 'syntax = "proto3";' not in text:
            fail(f"protobuf file does not declare proto3 syntax: {relative(path)}")
        if "package capyio.v1;" not in text:
            fail(f"protobuf file uses an unexpected package: {relative(path)}")

        stack: list[tuple[str, dict[int, str], int]] = []
        depth = 0
        for line in text.splitlines():
            match = message_re.match(line)
            if match:
                stack.append((match.group(1), {}, depth))
            if stack:
                field = field_re.match(line)
                if field:
                    field_name, number_text = field.groups()
                    number = int(number_text)
                    if number <= 0 or 19_000 <= number <= 19_999:
                        fail(
                            f"invalid/reserved protobuf field number {number} in "
                            f"{relative(path)}"
                        )
                    message_name, seen, start_depth = stack[-1]
                    previous = seen.get(number)
                    if previous is not None:
                        fail(
                            f"duplicate protobuf field {number} in {relative(path)} "
                            f"message {message_name}: {previous} and {field_name}"
                        )
                    seen[number] = field_name
                    stack[-1] = (message_name, seen, start_depth)
            depth += line.count("{") - line.count("}")
            while stack and depth <= stack[-1][2]:
                stack.pop()
        if depth != 0:
            fail(f"unbalanced protobuf braces: {relative(path)}")


def validate_architecture_dependencies() -> None:
    core_manifest = (ROOT / "crates/capyio-core/Cargo.toml").read_text(
        encoding="utf-8"
    )
    forbidden = ["tauri", "tokio", "windows", "android", "prost", "cpal", "oboe"]
    lowered = core_manifest.lower()
    violations = [name for name in forbidden if name in lowered]
    if violations:
        fail(f"Core manifest contains forbidden platform/mechanism dependencies: {violations}")

    data_plane_manifest = (ROOT / "crates/capyio-data-plane/Cargo.toml").read_text(
        encoding="utf-8"
    )
    data_plane_forbidden = ["tauri", "tokio", "windows", "android", "quinn", "webrtc"]
    data_plane_violations = [
        name for name in data_plane_forbidden if name in data_plane_manifest.lower()
    ]
    if data_plane_violations:
        fail(
            "Data-plane manifest contains forbidden platform/transport dependencies: "
            f"{data_plane_violations}"
        )

    driver_text = "\n".join(
        path.read_text(encoding="utf-8").lower()
        for path in (ROOT / "drivers/windows-audio").rglob("*")
        if path.is_file()
        and "x64" not in path.relative_to(ROOT / "drivers/windows-audio").parts
    )
    required_driver_rules = [
        "network",
        "protobuf",
        "json",
        "codec",
        "capyio speaker",
        "wasapi",
        "render apo",
        "bounded",
        "shared-memory",
        "isolated",
    ]
    missing_rules = [rule for rule in required_driver_rules if rule not in driver_text]
    if missing_rules:
        fail(f"Windows driver boundary docs lack rules: {missing_rules}")

    root_manifest = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    if "your-org" in root_manifest or "example.com" in root_manifest:
        fail("root Cargo metadata contains an unresolved repository placeholder")


def validate_sensor_server_provenance() -> None:
    provenance = (ROOT / "third_party/THIRD_PARTY.yml").read_text(encoding="utf-8")
    required = [
        "upstream_repository: https://github.com/UmerCodez/SensorServer",
        "pinned_revision: 5ae401780d99debcabb8dc259256c2652dada0a6",
        "license: GPL-3.0-only",
        "integration_mode: external_service_protocol_adapter",
        "source_imported: false",
        "binary_imported: false",
        "imported_paths: []",
    ]
    missing = [entry for entry in required if entry not in provenance]
    if missing:
        fail(f"SensorServer provenance is incomplete: {missing}")

    manifest = (ROOT / "adapters/sensor-server/Cargo.toml").read_text(
        encoding="utf-8"
    ).lower()
    required_dependency = (
        'tungstenite.workspace = true' in manifest
        and 'tungstenite = { version = "0.30.0", default-features = false, '
        'features = ["handshake"] }'
        in (ROOT / "Cargo.toml").read_text(encoding="utf-8").lower()
    )
    if not required_dependency:
        fail("SensorServer must use the reviewed minimal tungstenite 0.30.0 dependency")
    forbidden_transport = ["tokio", "native-tls", "rustls", "reqwest"]
    present = [name for name in forbidden_transport if name in manifest]
    if present:
        fail(
            "SensorServer Adapter contains an unreviewed transport/TLS dependency: "
            f"{present}"
        )


def validate_micyou_adapter_contract() -> None:
    provenance = (ROOT / "third_party/THIRD_PARTY.yml").read_text(encoding="utf-8")
    provenance_required = [
        "upstream_repository: https://github.com/LanRhyme/MicYou",
        "pinned_revision: b22c41fff3d3d1169c04c8acd1db7266cf9d4c62",
        "license: GPL-3.0-only",
        "integration_mode: external_process_adapter",
        "device-stable-id-v1",
        "tauri-app/crates/micyou-cli/src/capyio_windows_devices.rs",
        "name: windows",
        "version: 0.61.3",
    ]
    missing_provenance = [
        entry for entry in provenance_required if entry not in provenance
    ]
    if missing_provenance:
        fail(f"MicYou provenance/stable selector record is incomplete: {missing_provenance}")

    adapter = (ROOT / "adapters/micyou/src/lib.rs").read_text(encoding="utf-8")
    fixture = (
        ROOT / "adapters/micyou/src/bin/capyio-micyou-fixture.rs"
    ).read_text(encoding="utf-8")
    readme = (ROOT / "adapters/micyou/README.md").read_text(encoding="utf-8")
    required_adapter_markers = [
        'REQUIRED_MICYOU_CAPABILITY: &str = "device-stable-id-v1"',
        "output_device_id: String",
        "pub id: String",
        '"--device-id".to_owned()',
        "resolve_configured_device",
        'Some("audio output devices v2:")',
        "MAX_DEVICE_ID_BYTES",
    ]
    missing_adapter = [
        marker for marker in required_adapter_markers if marker not in adapter
    ]
    if missing_adapter:
        fail(f"MicYou Adapter stable endpoint contract is incomplete: {missing_adapter}")
    if "pub output_device_index:" in adapter:
        fail("MicYou persisted configuration must not contain an output-device index")
    if "device-stable-id-v1" not in fixture or "audio output devices v2:" not in fixture:
        fail("MicYou fixture must expose the stable endpoint inventory contract")
    if "device-stable-id-v1" not in readme or "stable endpoint ID" not in readme:
        fail("MicYou README must document the stable endpoint identity contract")

    adapter_manifest = (ROOT / "adapters/micyou/Cargo.toml").read_text(encoding="utf-8")
    if 'capyio-process-presence = { path = "../../platform/windows/process-presence" }' not in adapter_manifest:
        fail("MicYou Adapter must use the shared safe process-presence boundary")
    if "windows-sys" in adapter_manifest:
        fail("MicYou Adapter must not own unsafe Windows TCP table bindings")

    presence = (
        ROOT / "platform/windows/process-presence/src/lib.rs"
    ).read_text(encoding="utf-8")
    for marker in (
        "GetExtendedTcpTable",
        "MAX_TCP_TABLE_BYTES: usize = 16 * 1024 * 1024",
        "row.dwOwningPid == process_id",
        "u16::from_be(row.dwLocalPort as u16) == port",
        "TcpPeerPresence::Established { connection_count }",
    ):
        if marker not in presence:
            fail(f"process-presence boundary lacks required marker: {marker}")
    if "dwRemoteAddr" in presence or "dwRemotePort" in presence:
        fail("process-presence boundary must not retain or inspect peer addresses")

    microphone_runtime = (
        ROOT / "apps/desktop/src-tauri/src/micyou_runtime.rs"
    ).read_text(encoding="utf-8")
    for marker in (
        "DEFAULT_STABLE_PHONE_POLLS: u8 = 3",
        "DEFAULT_PHONE_WAIT_POLLS: u16 = 120",
        "RouteBackend::AdapterManaged",
        "CAPY.MICYOU.PHONE_TCP_LOST",
        "CAPY.MICYOU.PHONE_WAIT_EXHAUSTED",
        "CAPY.MICYOU.ENDPOINT_UNAVAILABLE",
        "PermissionRequirement::ForegroundService",
    ):
        if marker not in microphone_runtime:
            fail(f"MicYou Runtime composition lacks required marker: {marker}")

    quick_action = (
        ROOT / "apps/desktop/src-tauri/src/microphone_quick_action.rs"
    ).read_text(encoding="utf-8")
    for marker in (
        'MICYOU_ACTION_ID: &str = "capyio.quick-action.remote-microphone"',
        '"stable_phone_tcp_presence"',
        "TrustedMicYouHostConfig",
        "connection_hint",
    ):
        if marker not in quick_action:
            fail(f"MicYou Quick Action lacks required marker: {marker}")

    host_config = (
        ROOT / "platform/windows/micyou-host-config/src/lib.rs"
    ).read_text(encoding="utf-8")
    host_config_cli = (
        ROOT
        / "platform/windows/micyou-host-config/src/bin/capyio-micyou-config.rs"
    ).read_text(encoding="utf-8")
    for marker in (
        'CONFIG_SCHEMA_VERSION: u8 = 1',
        '["CapyIO", "host", "micyou-v1.json"]',
        'deny_unknown_fields',
        'TrustedConfigSource::EnvironmentOverride',
        'TrustedConfigSource::UserConfigFile',
        'ConfigAlreadyExists',
        'provision_from_inventory',
        '"<redacted>"',
    ):
        if marker not in host_config:
            fail(f"MicYou trusted-host configuration lacks required marker: {marker}")
    for marker in (
        'Some("provision")',
        'Some("validate")',
        '"--endpoint-id"',
        'write_new_default_config',
    ):
        if marker not in host_config_cli:
            fail(f"MicYou host configuration CLI lacks required marker: {marker}")
    if "output_device_index" in host_config:
        fail("MicYou trusted host configuration must not persist an endpoint index")
    if "load_trusted_host_config" not in quick_action:
        fail("MicYou Quick Action must automatically load trusted host configuration")

    ui_types = (ROOT / "apps/desktop/src/lib/types.ts").read_text(encoding="utf-8")
    browser_mock = (ROOT / "apps/desktop/src/lib/mock.ts").read_text(encoding="utf-8")
    quick_action_card = (
        ROOT / "apps/desktop/src/components/QuickActionCard.vue"
    ).read_text(encoding="utf-8")
    if "schemaVersion: 2" not in ui_types or "connectionHint: string | null" not in ui_types:
        fail("Quick Action TypeScript contract must expose schema v2 connection guidance")
    if "capyio.quick-action.remote-microphone" not in browser_mock:
        fail("Browser Mock must expose the matching blocked microphone Quick Action")
    if "isSpeakerAction && audioEndpoints?.supported" not in quick_action_card:
        fail("Speaker endpoint selection must not appear on other Quick Actions")
    if "isMicrophoneAction" not in quick_action_card or "CapyIO 麦克风输入" not in quick_action_card:
        fail("Microphone Quick Action must explain ordinary Windows input selection")

    microphone_guide = (
        ROOT / "docs/MICROPHONE_SHARING_WINDOWS_ANDROID.md"
    ).read_text(encoding="utf-8")
    for marker in (
        "capyio-micyou-config -- validate",
        "corepack pnpm tauri dev",
        "CapyIO Speaker with Render Bridge",
        "341.33 ms",
        "functional acceptance, not release qualification",
    ):
        if marker not in microphone_guide:
            fail(f"Microphone operator guide lacks required marker: {marker}")
    frontend = "\n".join(
        path.read_text(encoding="utf-8")
        for path in (ROOT / "apps/desktop/src").rglob("*")
        if path.is_file()
    )
    for secret_host_field in ("CAPYIO_MICYOU_CLI", "CAPYIO_MICYOU_ENDPOINT_ID"):
        if secret_host_field in frontend:
            fail(f"WebView source must not receive trusted MicYou field {secret_host_field}")


def validate_windows_audio_provenance() -> None:
    provenance = (ROOT / "third_party/THIRD_PARTY.yml").read_text(encoding="utf-8")
    required = [
        "id: microsoft-sysvad",
        "upstream_repository: https://github.com/microsoft/Windows-driver-samples",
        "pinned_revision: 717778a20ba4dd2440fe609f69153a1f8a64f597",
        "upstream_path: audio/sysvad",
        "license: MS-PL",
        "integration_mode: driver_derivative",
        "source_imported: true",
        "binary_imported: false",
        "drivers/windows-audio/sysvad/APO/SwapAPO",
        "drivers/windows-audio/sysvad/TabletAudioSample",
    ]
    missing = [entry for entry in required if entry not in provenance]
    if missing:
        fail(f"Microsoft SysVAD provenance is incomplete: {missing}")

    candidate_required = [
        "id: virtualdrivers-virtual-audio-driver",
        "pinned_revision: bb34fba15faf569a6ae9bdea360bc1cf4821354e",
        "revision: 191d307c858cb7c2749bc849060849d2dac18d3b",
        "archive_sha256: 24e07b8a4b82ec6fe136c079e73443a09a185d8cd17fbf03d9a7722992c4edff",
        "id: scream",
        "pinned_revision: d789743c248b11d1df7e5ecc546b1bc60b90cd91",
        "archive_sha256: 59dcd9889ba80d781745b3421facc7a5bdffefb77bf8543d606ac6abf0bc6ebdc",
        "integration_mode: research_reference_only",
    ]
    candidate_missing = [
        entry for entry in candidate_required if entry not in provenance
    ]
    if candidate_missing:
        fail(f"Windows audio candidate provenance is incomplete: {candidate_missing}")

    bridge_adr = (
        ROOT / "docs/adr/0028-render-apo-bounded-bridge.md"
    ).read_text(encoding="utf-8").lower()
    bridge_required = [
        "status: accepted",
        "synthetic tone",
        "render apo",
        "bounded shared-memory/spsc",
        "must not allocate",
        "research reference only",
        "driver ipc",
    ]
    bridge_missing = [entry for entry in bridge_required if entry not in bridge_adr]
    if bridge_missing:
        fail(f"Windows render APO decision is incomplete: {bridge_missing}")

    imported_driver_sources = [
        path
        for path in (ROOT / "drivers/windows-audio").rglob("*")
        if path.is_file() and path.suffix.lower() in {".c", ".cpp", ".inx", ".vcxproj"}
    ]
    if not imported_driver_sources:
        fail(
            "SysVAD provenance declares imported source but no Windows driver "
            "source files are present"
        )


def validate_windows_audio_endpoint_contract() -> None:
    tablet = ROOT / "drivers/windows-audio/sysvad/TabletAudioSample"
    inx_paths = [
        tablet / "ComponentizedApoSample.inx",
        tablet / "ComponentizedAudioSample.inx",
        tablet / "ComponentizedAudioSampleExtension.inx",
    ]
    versions = []
    for path in inx_paths:
        match = re.search(
            r"^DriverVer\s*=\s*\d{2}/\d{2}/\d{4},(?P<version>\d+\.\d+\.\d+\.\d+)\s*$",
            path.read_text(encoding="utf-8"),
            re.MULTILINE,
        )
        if match is None:
            fail(f"Windows audio INF lacks a deterministic DriverVer: {relative(path)}")
        versions.append(match.group("version"))
    if len(set(versions)) != 1:
        fail(f"Windows audio component DriverVer values disagree: {versions}")

    project = (tablet / "TabletAudioSample.vcxproj").read_text(encoding="utf-8")
    if f"<TimeStamp>{versions[0]}</TimeStamp>" not in project:
        fail("Windows audio project TimeStamp does not match the three component INFs")

    package_project = (
        ROOT / "drivers/windows-audio/sysvad/Package/package.VcxProj"
    ).read_text(encoding="utf-8")
    if '<Configuration Condition="\'$(Configuration)\' == \'\'">Debug</Configuration>' not in package_project:
        fail("Windows audio package project must not override an explicit Release build")

    apo_inf = inx_paths[0].read_text(encoding="utf-8")
    base_inf = inx_paths[1].read_text(encoding="utf-8")
    extension_inf = inx_paths[2].read_text(encoding="utf-8")
    microphone_apo_header = (
        ROOT / "drivers/windows-audio/sysvad/APO/SwapAPO/SwapAPO.h"
    ).read_text(encoding="utf-8")
    microphone_apo_source = (
        ROOT / "drivers/windows-audio/sysvad/APO/SwapAPO/swapapomfx.cpp"
    ).read_text(encoding="utf-8")
    shared_bridge_source = (
        ROOT / "drivers/windows-audio/sysvad/APO/SwapAPO/swapaposfx.cpp"
    ).read_text(encoding="utf-8")
    capture_ring_source = (
        ROOT / "drivers/windows-audio/sysvad/APO/SwapAPO/CapyIOCaptureRing.cpp"
    ).read_text(encoding="utf-8")
    topology = (
        ROOT / "drivers/windows-audio/sysvad/EndpointsCommon/speakertoptable.h"
    ).read_text(encoding="utf-8")
    microphone_topology = (tablet / "micintoptable.h").read_text(encoding="utf-8")
    required_endpoint_names = {
        "c2ae0cd6-c228-41a6-8b0f-8b13773556a0": "CapyIO Speaker",
        "ba3df3f3-aa52-4d39-b9d7-d2a44a50510a": "CapyIO Microphone",
    }
    driver_sources = f"{topology}\n{microphone_topology}".lower()
    for guid, endpoint_name in required_endpoint_names.items():
        if guid not in driver_sources:
            fail(f"Windows audio bridge-pin GUID is missing from topology source: {guid}")
        if guid not in base_inf.lower() or f'= "{endpoint_name}"' not in base_inf:
            fail(f"Windows audio endpoint name is not registered in the base INF: {endpoint_name}")
    minipairs = (tablet / "minipairs.h").read_text(encoding="utf-8")
    render_endpoints = minipairs.split(
        "static PENDPOINT_MINIPAIR g_RenderEndpoints[] =", 1
    )[-1].split("};", 1)[0]
    if render_endpoints.count("&CapyIoSpeakerMiniports") != 1:
        fail("Windows audio must expose exactly one CapyIO Speaker render endpoint")
    if "MicrophoneIngress" in render_endpoints:
        fail("native microphone transport must not expose the retired render ingress endpoint")
    if "ENDPOINT_OFFLOAD_SUPPORTED | ENDPOINT_SYNTHETIC_LOOPBACK_DISABLED" not in minipairs:
        fail("CapyIO Speaker must reject the inherited SysVAD synthetic loopback path")
    common_header = (
        ROOT / "drivers/windows-audio/sysvad/common.h"
    ).read_text(encoding="utf-8")
    wave_header = (
        ROOT / "drivers/windows-audio/sysvad/EndpointsCommon/minwavert.h"
    ).read_text(encoding="utf-8")
    wave_source = (
        ROOT / "drivers/windows-audio/sysvad/EndpointsCommon/minwavert.cpp"
    ).read_text(encoding="utf-8")
    if "ENDPOINT_SYNTHETIC_LOOPBACK_DISABLED" not in common_header:
        fail("Windows audio contract lacks the synthetic-loopback disable flag")
    if "m_DeviceFlags & ENDPOINT_SYNTHETIC_LOOPBACK_DISABLED" not in wave_header:
        fail("Windows audio loopback capability must honor the disable flag")
    loopback_validation = wave_source.split("CMiniportWaveRT::ValidateStreamCreate", 1)[-1].split(
        "//=============================================================================", 1
    )[0]
    if "if (IsLoopbackSupported())" not in loopback_validation:
        fail("Windows audio stream creation must reject disabled loopback pins")
    for retired_ingress_token in (
        "KSNAME_WaveMicIngress",
        "KSNAME_TopologyMicIngress",
        "CapyIO.I.WaveMicIngress",
        "CapyIO.I.TopologyMicIngress",
        "CapyIO.MicrophoneIngressCategoryGuid",
    ):
        if retired_ingress_token in base_inf or retired_ingress_token in extension_inf:
            fail(f"Windows audio INF still exposes retired ingress token {retired_ingress_token}")
    required_associations = {
        "%KSNODETYPE_SPEAKER%": "speaker",
        "%KSNODETYPE_MICROPHONE%": "microphone",
    }
    for category, endpoint in required_associations.items():
        association = f"HKR,EP\\0,%PKEY_AudioEndpoint_Association%,,{category}"
        if association not in base_inf:
            fail(f"Windows audio {endpoint} must declare its explicit endpoint association")
    if "HKR,EP\\0,%PKEY_AudioEndpoint_Association%,,%KSNODETYPE_ANY%" in base_inf:
        fail("Windows audio endpoint associations must not fall back to KSNODETYPE_ANY")
    capture_fx = extension_inf.split("[CapyIOMicrophoneExtension.Interface.AddReg]", 1)[-1].split("[", 1)[0]
    if extension_inf.count("TopologyMicIn%,CapyIOMicrophoneExtension.Interface") != 2:
        fail("microphone capture bridge must cover both audio and topology interfaces")
    if "%CAPYIO_RENDER_MFX_CLSID%" not in capture_fx:
        fail("microphone capture must use the proven shared render bridge APO class")
    capture_fx_cleanup = extension_inf.split("[CapyIOMicrophoneExtension.Interface.DelReg]", 1)[-1].split("[", 1)[0]
    for stale_key in (
        "%PKEY_CompositeFX_StreamEffectClsid%",
        "%PKEY_SFX_ProcessingModes_Supported_For_Streaming%",
        "%PKEY_CompositeFX_ModeEffectClsid%",
        "%PKEY_MFX_ProcessingModes_Supported_For_Streaming%",
        "%PKEY_CompositeFX_EndpointEffectClsid%",
        "%PKEY_EFX_ProcessingModes_Supported_For_Streaming%",
    ):
        if stale_key not in capture_fx_cleanup:
            fail(f"microphone capture bridge must delete stale FX key {stale_key}")
    mic_wave_table = (tablet / "micinwavtable.h").read_text(encoding="utf-8")
    capture_range = mic_wave_table.split("MicInPinDataRangesStream[]", 1)[-1].split("};", 1)[0]
    if "KSDATARANGE_ATTRIBUTES" not in capture_range:
        fail("microphone capture must retain the pinned SysVAD processing-mode attributes")
    capture_range_pointers = mic_wave_table.split("MicInPinDataRangePointersStream[]", 1)[-1].split("};", 1)[0]
    if "&PinDataRangeAttributeList" not in capture_range_pointers:
        fail("microphone capture must retain the pinned SysVAD processing-mode attribute list")
    for mode_format in (
        "STATIC_AUDIO_SIGNALPROCESSINGMODE_SPEECH,\n        &MicInPinSupportedDeviceFormats[2].DataFormat",
        "STATIC_AUDIO_SIGNALPROCESSINGMODE_COMMUNICATIONS,\n        &MicInPinSupportedDeviceFormats[4].DataFormat",
        "STATIC_AUDIO_SIGNALPROCESSINGMODE_FAR_FIELD_SPEECH,\n        &MicInPinSupportedDeviceFormats[2].DataFormat",
    ):
        if mode_format not in mic_wave_table:
            fail(f"microphone capture must retain pinned SysVAD mode mapping {mode_format}")
    standard_apo_interface = '"APOInterface0",,"{FD7F2B29-24D0-4B5C-B177-592C39F9CA10}"'
    if apo_inf.count(standard_apo_interface) != 2:
        fail("both CapyIO APO registrations must declare the standard IAudioProcessingObject IID")
    if "COM_INTERFACE_ENTRY(IAudioSystemEffectsCustomFormats)" in microphone_apo_header:
        fail("microphone bridge must not advertise SysVAD custom formats")
    if "CBaseAudioProcessingObject::IsOutputFormatSupported(" not in microphone_apo_source:
        fail("microphone bridge format negotiation must delegate to the base APO contract")
    if "m_fEnableSwapMFX = FALSE" not in microphone_apo_source:
        fail("microphone capture bridge must keep inherited SysVAD DSP disabled")
    for role_marker in (
        "GetMicrophoneBridgeRole(candidate.get())",
        "MicrophoneBridgeRole::CaptureConsumer",
        "value.vt == VT_LPWSTR",
        "CLSIDFromString(value.pwszVal, &association)",
        "for (UINT32 index = 0; index < deviceCount; ++index)",
    ):
        if role_marker not in shared_bridge_source:
            fail(f"shared audio bridge must retain role marker {role_marker}")
    for retired_producer_token in (
        "MicrophoneBridgeRole::IngressProducer",
        "kCapyIoMicrophoneIngressCategory",
        "m_captureProducer",
    ):
        if retired_producer_token in microphone_apo_header or retired_producer_token in shared_bridge_source or retired_producer_token in microphone_apo_source:
            fail(f"retired microphone ingress producer remains in the APO: {retired_producer_token}")
    if "m_microphoneBridgeRole != MicrophoneBridgeRole::Detached" not in shared_bridge_source:
        fail("microphone bridge roles must not register render endpoint-volume notifications")
    for capture_lock_marker in (
        "m_microphoneBridgeRole == MicrophoneBridgeRole::CaptureConsumer",
        "outputFormat.dwSamplesPerFrame != capyio::capture_ring::kChannels",
        "outputFormat.fFramesPerSecond",
        "candidateOutput.dwSamplesPerFrame == capyio::capture_ring::kChannels",
        "STDMETHODIMP CSwapAPOSFX::IsOutputFormatSupported",
        "accept only the project's fixed mono 48 kHz engine-side contract",
        "RecordDiagnostic(500, S_OK)",
        "RecordDiagnostic(401, S_OK)",
        "RecordDiagnostic(300, static_cast<LONG>(cbDataSize))",
    ):
        if capture_lock_marker not in shared_bridge_source:
            fail(f"capture bridge lock contract must retain {capture_lock_marker}")
    for fresh_attach_marker in (
        "A microphone capture session is a live view, not a recording backlog.",
        "InterlockedExchange64(&header_->read_frame_sequence, write)",
    ):
        if fresh_attach_marker not in capture_ring_source:
            fail(f"capture ring must discard stale pre-attach frames: {fresh_attach_marker}")
def workflow_has_pull_request_branch(text: str, expected: str) -> bool:
    lines = text.splitlines()
    for index, line in enumerate(lines):
        if line != "  pull_request:":
            continue
        for nested in lines[index + 1 :]:
            if nested.strip() and len(nested) - len(nested.lstrip()) <= 2:
                break
            match = re.fullmatch(r"\s*branches:\s*\[(?P<branches>[^]]*)]\s*", nested)
            if match is None:
                continue
            branches = {
                branch.strip().strip("'\"")
                for branch in match.group("branches").split(",")
            }
            return expected in branches
        return False
    return False


def validate_hosted_ci_contract() -> None:
    workflow_paths = [
        ROOT / ".github/workflows/core.yml",
        ROOT / ".github/workflows/static.yml",
        ROOT / ".github/workflows/ui.yml",
        ROOT / ".github/workflows/tauri-smoke.yml",
    ]
    workflows = {
        path.name: path.read_text(encoding="utf-8") for path in workflow_paths
    }
    exact_head = "ref: ${{ github.event.pull_request.head.sha || github.sha }}"
    for name, text in workflows.items():
        if not workflow_has_pull_request_branch(text, "main"):
            fail(f"hosted workflow {name} is not a pull-request gate for main")
        if exact_head not in text:
            fail(f"hosted workflow {name} does not check out the exact PR head")

    core = workflows["core.yml"]
    for runner in ["ubuntu-latest", "windows-latest", "macos-latest"]:
        if runner not in core:
            fail(f"Rust/Adapter matrix is missing {runner}")
    for command in [
        "cargo fmt --all -- --check",
        "cargo check --workspace --exclude capyio-desktop --all-targets",
        "cargo clippy --workspace --exclude capyio-desktop --all-targets -- -D warnings",
        "cargo test --workspace --exclude capyio-desktop",
        "cargo xtask validate-docs",
        "cargo xtask validate-manifests",
        "cargo xtask adapter-smoke",
        "cargo xtask imu-demo",
    ]:
        if command not in core:
            fail(f"Rust/Adapter matrix is missing merge-gate command: {command}")

    ui = workflows["ui.yml"]
    tauri = workflows["tauri-smoke.yml"]
    if "pnpm install --frozen-lockfile" not in ui or "pnpm install --frozen-lockfile" not in tauri:
        fail("UI and Tauri workflows must use the frozen pnpm lockfile")
    if any("--no-frozen-lockfile" in text for text in workflows.values()):
        fail("merge-gate workflow uses --no-frozen-lockfile")
    for command in [
        "pnpm --filter @capyio/desktop typecheck",
        "pnpm --filter @capyio/desktop build",
        "cargo check -p capyio-desktop",
        "cargo build -p capyio-desktop",
    ]:
        if command not in tauri:
            fail(f"Windows Tauri workflow is missing merge-gate command: {command}")
    if "non-Windows explicitly skipped by policy" not in tauri:
        fail("Tauri workflow does not label its non-Windows platform skip")


def validate_current_foundation_labels() -> None:
    app = (ROOT / "apps/desktop/src/App.vue").read_text(encoding="utf-8")
    if "Mock Gate 2" in app or "Gate 2 · Generic mock vertical slices" in app:
        fail("desktop UI still advertises the completed foundation as Gate 2")
    protocol = (ROOT / "docs/PROTOCOL.md").read_text(encoding="utf-8")
    if "`route.status` and unsolicited Adapter events remain specified future" in protocol:
        fail("protocol documentation still reports implemented route.status as future work")


def validate_local_markdown_links() -> None:
    link_pattern = re.compile(r"\[[^\]]*\]\(([^)]+)\)")
    for path in (path for path in repository_files() if path.suffix.lower() == ".md"):
        for target in link_pattern.findall(path.read_text(encoding="utf-8")):
            target = target.strip().split("#", 1)[0]
            if not target or target.startswith(("http://", "https://", "mailto:", "#")):
                continue
            candidate = (path.parent / target).resolve()
            try:
                candidate.relative_to(ROOT.resolve())
            except ValueError:
                fail(f"Markdown link escapes repository in {relative(path)}: {target}")
            if not candidate.exists():
                fail(f"broken local Markdown link in {relative(path)}: {target}")


def validate_no_obvious_secrets() -> None:
    suspicious = re.compile(
        r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----|AKIA[0-9A-Z]{16}",
        re.MULTILINE,
    )
    for path in repository_files():
        if path.suffix.lower() in {".png", ".ico", ".zip", ".wav"}:
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            continue
        if suspicious.search(text):
            fail(f"possible secret material in {relative(path)}")


def main() -> None:
    if sys.argv[1:] == ["--self-test"]:
        run_requirement_parser_self_tests()
        return
    if sys.argv[1:] == ["--validate-docs"]:
        run_requirement_parser_self_tests()
        requirement_count = validate_requirement_ids()
        print(
            "Documentation traceability validation: PASS "
            f"({requirement_count} unique Requirement IDs)"
        )
        return
    if sys.argv[1:]:
        fail(f"unknown arguments: {' '.join(sys.argv[1:])}")

    validate_required_files()
    validate_no_symlinks()
    validate_utf8_and_line_endings()
    validate_json()
    validate_yaml()
    validate_python()
    validate_toml_and_workspace()
    requirement_count = validate_requirement_ids()
    validate_proto_field_numbers()
    validate_architecture_dependencies()
    validate_sensor_server_provenance()
    validate_micyou_adapter_contract()
    validate_android_audio_shell()
    validate_native_audio_lan_backend()
    validate_windows_audio_provenance()
    validate_windows_audio_endpoint_contract()
    validate_hosted_ci_contract()
    validate_current_foundation_labels()
    validate_local_markdown_links()
    validate_no_obvious_secrets()
    print(
        "Repository structural validation: PASS "
        f"({requirement_count} unique Requirement IDs traced)"
    )


if __name__ == "__main__":
    main()
