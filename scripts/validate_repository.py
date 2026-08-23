#!/usr/bin/env python3
"""Offline structural validation for the CapyIO bootstrap repository.

This script deliberately avoids dependency installation, compilation, device access and
privileged operations. It catches repository-shape, parser, compatibility and hygiene errors
before the full Rust/Tauri/platform toolchains are available.
"""

from __future__ import annotations

import json
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
    "docs/REQUIREMENTS_TRACEABILITY.md",
    "docs/plans/TEMPLATE.md",
    "crates/capyio-core/src/lib.rs",
    "crates/capyio-audio/src/lib.rs",
    "crates/capyio-runtime/src/lib.rs",
    "crates/capyio-protocol/src/lib.rs",
    "protocol/proto/capyio/v1/common.proto",
    "protocol/proto/capyio/v1/capability.proto",
    "protocol/proto/capyio/v1/control.proto",
    "apps/desktop/package.json",
    "apps/desktop/src/App.vue",
    "apps/desktop/src-tauri/tauri.conf.json",
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
    ".json",
    ".md",
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
    ".pnpm-store",
    ".vite",
    "artifacts",
    "dist",
    "node_modules",
    "target",
    "test-results",
}


def fail(message: str) -> None:
    print(f"ERROR: {message}", file=sys.stderr)
    raise SystemExit(1)


def relative(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


def is_ignored(path: Path) -> bool:
    return any(
        part in IGNORED_DIRECTORY_NAMES for part in path.relative_to(ROOT).parts
    )


def repository_files() -> list[Path]:
    return sorted(
        path
        for path in ROOT.rglob("*")
        if path.is_file() and not is_ignored(path)
    )


def validate_required_files() -> None:
    missing = [path for path in REQUIRED_FILES if not (ROOT / path).is_file()]
    if missing:
        fail(f"missing required files: {', '.join(missing)}")


def validate_no_symlinks() -> None:
    symlinks = [
        relative(path)
        for path in ROOT.rglob("*")
        if path.is_symlink() and not is_ignored(path)
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
        for path in [*ROOT.rglob("*.yml"), *ROOT.rglob("*.yaml")]
        if not is_ignored(path)
    ):
        try:
            yaml.safe_load(path.read_text(encoding="utf-8"))
        except (OSError, yaml.YAMLError) as error:
            fail(f"invalid YAML {relative(path)}: {error}")


def validate_python() -> None:
    for path in sorted(path for path in ROOT.rglob("*.py") if not is_ignored(path)):
        try:
            compile(path.read_text(encoding="utf-8"), relative(path), "exec")
        except (OSError, SyntaxError) as error:
            fail(f"invalid Python {relative(path)}: {error}")


def validate_toml_and_workspace() -> None:
    manifests: dict[Path, dict] = {}
    for path in sorted(path for path in ROOT.rglob("*.toml") if not is_ignored(path)):
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
        if first_gate >= 4 and status != "planned":
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

    driver_text = "\n".join(
        path.read_text(encoding="utf-8").lower()
        for path in (ROOT / "drivers/windows-audio").rglob("*")
        if path.is_file()
    )
    required_driver_rules = ["network", "protobuf", "json", "codec"]
    missing_rules = [rule for rule in required_driver_rules if rule not in driver_text]
    if missing_rules:
        fail(f"Windows driver boundary docs lack rules: {missing_rules}")

    root_manifest = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    if "your-org" in root_manifest or "example.com" in root_manifest:
        fail("root Cargo metadata contains an unresolved repository placeholder")


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
    for path in sorted(path for path in ROOT.rglob("*.md") if not is_ignored(path)):
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
