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


def validate_requirement_ids() -> None:
    path = ROOT / "docs/PRODUCT_REQUIREMENTS.md"
    pattern = re.compile(r"\*\*((?:FR|NFR)-[A-Z]+-\d{3})\*\*")
    occurrences: dict[str, list[int]] = {}
    for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        for requirement_id in pattern.findall(line):
            occurrences.setdefault(requirement_id, []).append(line_number)

    duplicates = {
        requirement_id: lines
        for requirement_id, lines in occurrences.items()
        if len(lines) > 1
    }
    if duplicates:
        formatted = ", ".join(
            f"{requirement_id}@{lines}" for requirement_id, lines in sorted(duplicates.items())
        )
        fail(f"duplicate normative requirement IDs: {formatted}")
    if len(occurrences) < 40:
        fail(f"unexpectedly few normative requirement IDs: {len(occurrences)}")


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
    validate_required_files()
    validate_no_symlinks()
    validate_utf8_and_line_endings()
    validate_json()
    validate_yaml()
    validate_python()
    validate_toml_and_workspace()
    validate_requirement_ids()
    validate_proto_field_numbers()
    validate_architecture_dependencies()
    validate_local_markdown_links()
    validate_no_obvious_secrets()
    print("Repository structural validation: PASS")


if __name__ == "__main__":
    main()
