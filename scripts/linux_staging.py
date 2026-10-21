#!/usr/bin/env python3
"""Build-only Linux staging and passive, non-executing inventory verification.

The manifest is an integrity inventory, NOT a signature or deployment approval.
Only build invokes cargo and build-host metadata tools. Preflight never
executes staged programs, pw-cat, ldd, or loads a shared library.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import selectors
import time
import stat
import struct
import subprocess
import tempfile

FILES = {
    "capyio-node": ("capyio-node", "native-camera,native-touchpad"),
    "capyio-linux-avc-helper": ("capyio-linux-avc", "native-pipewire"),
    "capyio-linux-camera-helper": ("capyio-linux-camera", "native-pipewire"),
    "capyio-linux-touchpad-helper": ("capyio-linux-touchpad", "native-uinput"),
}
FEATURES = {package: features.split(",") for package, features in FILES.values()}
ARCHES = {"x86_64-unknown-linux-gnu": (62, 2), "aarch64-unknown-linux-gnu": (183, 2)}
PW_VERSIONS = ["1.0.5", "1.6.9"]
MANIFEST = "manifest.json"


MAX_BINARY_SIZE = 256 * 1024 * 1024
MAX_TOOL_OUTPUT = 2 * 1024 * 1024


def run(args, **kwargs):
    timeout = kwargs.pop("timeout", 60)
    with subprocess.Popen(args, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, **kwargs) as child:
        output = bytearray()
        deadline = time.monotonic() + timeout
        with selectors.DefaultSelector() as selector:
            selector.register(child.stdout, selectors.EVENT_READ)
            try:
                while True:
                    remaining = deadline - time.monotonic()
                    if remaining <= 0:
                        raise ValueError("metadata tool timed out")
                    if not selector.select(remaining):
                        raise ValueError("metadata tool timed out")
                    chunk = os.read(child.stdout.fileno(), 65536)
                    if not chunk:
                        break
                    output.extend(chunk)
                    if len(output) > MAX_TOOL_OUTPUT:
                        raise ValueError("metadata tool output exceeds limit")
                child.wait(timeout=max(0.1, deadline - time.monotonic()))
                if child.returncode:
                    raise ValueError("metadata tool failed: " + output.decode("utf-8", "replace")[:2048])
            except BaseException:
                child.kill()
                child.wait()
                raise
        return output.decode("utf-8", "strict").strip()


def digest(path):
    h = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def safe_file(path, owner, executable=False):
    s = path.lstat()
    if not stat.S_ISREG(s.st_mode):
        raise ValueError(f"{path.name}: not a regular non-symlink file")
    if s.st_uid != owner or s.st_mode & 0o022:
        raise ValueError(f"{path.name}: unsafe owner or group/world writable")
    if executable and not s.st_mode & stat.S_IXUSR:
        raise ValueError(f"{path.name}: owner execute bit missing (not repaired)")
    return s


def safe_directory(path):
    # Check the directory and every ancestor without resolving away symlinks.
    path = Path(os.path.abspath(path))
    owner = path.lstat().st_uid
    if owner not in (0, os.getuid()):
        raise ValueError("staging directory owner is not the current user or root")
    for current in [path, *path.parents]:
        s = current.lstat()
        if not stat.S_ISDIR(s.st_mode):
            raise ValueError(f"{current}: symlink or non-directory ancestor")
        # Sticky root-owned /tmp is safe from replacement by a different uid.
        sticky_root = s.st_uid == 0 and s.st_mode & stat.S_ISVTX
        if s.st_uid not in (0, owner) or (s.st_mode & 0o022 and not sticky_root):
            raise ValueError(f"{current}: unsafe directory owner or write permissions")
    return owner


def elf_metadata(path, fd=None):
    if fd is None:
        with path.open("rb") as stream:
            header = stream.read(64)
    else:
        header = os.pread(fd, 64, 0)
    tool_path = str(path) if fd is None else f"/proc/self/fd/{fd}"
    options = {} if fd is None else {"pass_fds": (fd,)}
    if len(header) != 64 or header[:4] != b"\x7fELF" or header[4] not in (1, 2) or header[5] != 1:
        raise ValueError(f"{path.name}: unsupported or invalid little-endian ELF")
    machine = struct.unpack_from("<H", header, 18)[0]
    # readelf parses bytes, unlike ldd it does not invoke the ELF interpreter.
    dynamic = run(["/usr/bin/readelf", "--wide", "--dynamic", tool_path], **options)
    program = run(["/usr/bin/readelf", "--wide", "--program-headers", tool_path], **options)
    needed = sorted(re.findall(r"\(NEEDED\).*\[([^\]]+)\]", dynamic))
    if re.search(r"\((?:RPATH|RUNPATH)\)", dynamic):
        raise ValueError(f"{path.name}: embedded RPATH/RUNPATH is not supported")
    interpreters = re.findall(r"Requesting program interpreter: ([^\]]+)\]", program)
    return {"machine": machine, "class": header[4], "needed": needed,
            "interpreter": interpreters[0] if interpreters else None}


def source_state(root):
    if not (root / ".git").exists():
        return {"kind": "export-unverified", "sha": None, "worktree": "unverified"}
    top = Path(run(["git", "-C", str(root), "rev-parse", "--show-toplevel"]))
    if top.resolve() != root.resolve():
        raise ValueError("source Git root does not match repository root")
    sha = run(["git", "-C", str(root), "rev-parse", "HEAD"])
    dirty = run(["git", "-C", str(root), "status", "--porcelain", "--untracked-files=normal"])
    # Never accept a user supplied SHA, nor export without Git as a clean commit.
    return {"kind": "git", "sha": sha, "worktree": "dirty" if dirty else "clean"}


def ffmpeg_metadata():
    result = {}
    for package in ("libavcodec", "libavutil"):
        libdir = Path(run(["pkg-config", "--variable=libdir", package]))
        library = (libdir / (package + ".so")).resolve(strict=True)
        result[package] = {
            "path": str(library), "sha256": digest(library),
            "pkg_config_version": run(["pkg-config", "--modversion", package]),
            "provenance": "pkg-config-selected-development-library (not runtime resolution)",
            "configuration": {"status": "unverified", "reason": "not available from static pkg-config/ELF metadata"},
            "license": {"status": "unverified", "reason": "package name/version does not prove this library's license or configuration"},
        }
    return result


def pw_metadata():
    path = Path("/usr/bin/pw-cat")
    if not path.exists():
        return {"status": "unverified", "reason": "build host has no /usr/bin/pw-cat"}
    # Read-only version probe at build time, not during passive preflight.
    output = run([str(path), "--version"])
    match = re.fullmatch(r"pw-cat\nCompiled with libpipewire (\S+)\nLinked with libpipewire \1", output)
    version = match.group(1) if match else None
    return {"status": "supported" if version in PW_VERSIONS else "unsupported",
            "version_output": output, "sha256": digest(path), "path": str(path)}


def build(root, output):
    root = root.resolve()
    if output.exists() or output.is_symlink():
        raise ValueError("staging destination must not already exist")
    host = re.search(r"^host: (.+)$", run(["rustc", "-vV"]), re.M).group(1)
    if host not in ARCHES:
        raise ValueError("only native x86_64/aarch64 GNU Linux builds are supported")
    before = source_state(root)
    ffmpeg = ffmpeg_metadata()
    pw_cat = pw_metadata()
    # A fresh private Cargo target prevents old fixture-feature artifacts leaking
    # into production inventory. No arbitrary Cargo feature or binary override.
    with tempfile.TemporaryDirectory(prefix="capyio-build-") as target:
        env = os.environ.copy()
        env["CARGO_TARGET_DIR"] = target
        for binary, (package, features) in FILES.items():
            subprocess.run(["cargo", "build", "--locked", "--release", "--target", host,
                            "-p", package, "--bin", binary, "--no-default-features",
                            "--features", features], cwd=root, env=env, check=True)
        after = source_state(root)
        if before != after:
            raise ValueError("source Git state changed during build; retry from stable source")
        output.mkdir(mode=0o700, parents=False)
        try:
            owner = safe_directory(output)
            records = {}
            for name in FILES:
                destination = output / name
                shutil.copyfile(Path(target) / host / "release" / name, destination)
                # Only newly created output files, never an existing installation.
                destination.chmod(0o700)
                s = safe_file(destination, owner, executable=True)
                records[name] = {"sha256": digest(destination), "size": s.st_size,
                                 "elf": elf_metadata(destination)}
            manifest = {"schema": 1, "purpose": "local-staging-not-release",
                        "source": before, "target": host, "features": FEATURES,
                        "files": records, "ffmpeg": ffmpeg,
                        "pw_cat": {"supported_versions": PW_VERSIONS, "build_host": pw_cat},
                        "libraries_bundled": False}
            (output / MANIFEST).write_text(json.dumps(manifest, indent=2) + "\n")
        except Exception:
            # Retain incomplete output for inspection; never label it verified.
            raise
    return preflight(output)


def exact_object(value, keys):
    return isinstance(value, dict) and set(value) == set(keys)


def sha256(value):
    return isinstance(value, str) and re.fullmatch(r"[0-9a-f]{64}", value) is not None


def validate_manifest(m):
    expected = {"schema", "purpose", "source", "target", "features", "files", "ffmpeg", "pw_cat", "libraries_bundled"}
    if not exact_object(m, expected) or type(m["schema"]) is not int or m["schema"] != 1:
        raise ValueError("invalid manifest schema")
    if m["purpose"] != "local-staging-not-release" or m["libraries_bundled"] is not False:
        raise ValueError("not a supported local staging manifest")
    if not isinstance(m["target"], str) or m["target"] not in ARCHES or m["features"] != FEATURES or not exact_object(m["files"], FILES):
        raise ValueError("unexpected target, features or required binary set")
    source = m["source"]
    if not exact_object(source, {"kind", "sha", "worktree"}):
        raise ValueError("invalid source metadata")
    if source["kind"] == "git":
        if not isinstance(source["sha"], str) or not re.fullmatch(r"[0-9a-f]{40}", source["sha"]) or source["worktree"] not in ("clean", "dirty"):
            raise ValueError("invalid Git metadata")
    elif source != {"kind": "export-unverified", "sha": None, "worktree": "unverified"}:
        raise ValueError("export cannot claim a verified source commit")
    pw = m["pw_cat"]
    if not exact_object(pw, {"supported_versions", "build_host"}) or pw["supported_versions"] != PW_VERSIONS:
        raise ValueError("unsupported pw-cat version contract")
    host = pw["build_host"]
    if not isinstance(host, dict):
        raise ValueError("invalid pw-cat build metadata")
    if host.get("status") == "unverified":
        if not exact_object(host, {"status", "reason"}) or not isinstance(host["reason"], str) or not host["reason"]:
            raise ValueError("invalid unverified pw-cat metadata")
    else:
        if not exact_object(host, {"status", "version_output", "sha256", "path"}) or host["status"] not in ("supported", "unsupported") or not sha256(host["sha256"]) or host["path"] != "/usr/bin/pw-cat" or not isinstance(host["version_output"], str):
            raise ValueError("invalid pw-cat build metadata")
        matched = re.fullmatch(r"pw-cat\nCompiled with libpipewire (\S+)\nLinked with libpipewire \1", host["version_output"])
        supported = bool(matched and matched.group(1) in PW_VERSIONS)
        if (host["status"] == "supported") != supported:
            raise ValueError("pw-cat version/status mismatch")
    for name, record in m["files"].items():
        if not exact_object(record, {"sha256", "size", "elf"}) or not sha256(record["sha256"]):
            raise ValueError(f"{name}: invalid file metadata")
        if type(record["size"]) is not int or not 0 < record["size"] <= MAX_BINARY_SIZE:
            raise ValueError(f"{name}: invalid size")
        elf = record["elf"]
        if not exact_object(elf, {"machine", "class", "needed", "interpreter"}) or type(elf["machine"]) is not int or type(elf["class"]) is not int:
            raise ValueError("invalid ELF metadata shape")
        if not isinstance(elf["needed"], list) or len(elf["needed"]) > 128 or not all(isinstance(lib, str) and re.fullmatch(r"[A-Za-z0-9_.+-]{1,128}", lib) for lib in elf["needed"]):
            raise ValueError("invalid ELF dependency metadata")
        if elf["needed"] != sorted(set(elf["needed"])):
            raise ValueError("duplicate or unsorted ELF dependencies")
        allowed = {"x86_64-unknown-linux-gnu": "/lib64/ld-linux-x86-64.so.2", "aarch64-unknown-linux-gnu": "/lib/ld-linux-aarch64.so.1"}
        if elf["interpreter"] not in (None, allowed[m["target"]]):
            raise ValueError("unexpected ELF interpreter / target architecture mismatch")
    if not exact_object(m["ffmpeg"], {"libavcodec", "libavutil"}):
        raise ValueError("FFmpeg metadata missing")
    for record in m["ffmpeg"].values():
        if not exact_object(record, {"path", "sha256", "pkg_config_version", "provenance", "configuration", "license"}) or not sha256(record["sha256"]):
            raise ValueError("invalid FFmpeg metadata")
        if not all(isinstance(record[key], str) and record[key] for key in ("path", "pkg_config_version")) or record["provenance"] != "pkg-config-selected-development-library (not runtime resolution)":
            raise ValueError("invalid FFmpeg metadata provenance")
        for key in ("configuration", "license"):
            item = record[key]
            if not exact_object(item, {"status", "reason"}) or item["status"] != "unverified" or not isinstance(item["reason"], str) or not item["reason"]:
                raise ValueError("FFmpeg license/configuration must be explicitly unverified")


def unique_json(pairs):
    value = {}
    for key, item in pairs:
        if key in value:
            raise ValueError("duplicate JSON key")
        value[key] = item
    return value


def runtime_metadata(m):
    required = sorted({lib for record in m["files"].values() for lib in record["elf"]["needed"]})
    missing = []
    resolved = {}
    cache_status = "unverified"
    # Fixed read-only cache query. Unlike ldd this never starts a target loader.
    cache_tool = next((p for p in ("/sbin/ldconfig", "/usr/sbin/ldconfig") if Path(p).is_file()), None)
    if cache_tool:
        try:
            cache = run([cache_tool, "-p"])
            for name in required:
                candidates = re.findall(r"^\s*" + re.escape(name) + r"\s+\([^\n]*\) => (\S+)$", cache, re.M)
                for candidate in candidates:
                    path = Path(candidate)
                    try:
                        with path.open("rb") as stream:
                            header = stream.read(64)
                        machine, elf_class = ARCHES[m["target"]]
                        if len(header) == 64 and header[:6] == bytes([127, 69, 76, 70, elf_class, 1]) and struct.unpack_from("<H", header, 18)[0] == machine:
                            resolved[name] = candidate
                            break
                    except OSError:
                        continue
                if name not in resolved:
                    missing.append(name)
            cache_status = "inspected"
        except (OSError, ValueError, subprocess.SubprocessError):
            pass
    for record in m["files"].values():
        interpreter = record["elf"]["interpreter"]
        if interpreter and not Path(interpreter).is_file():
            missing.append(interpreter)
    pw_path = Path("/usr/bin/pw-cat")
    if not pw_path.is_file():
        missing.append(str(pw_path))
    pw_observation = {"version": "unverified (not executed)", "present": pw_path.is_file()}
    if pw_path.is_file():
        recorded = m["pw_cat"]["build_host"]
        pw_observation["matches_build_host_file"] = digest(pw_path) == recorded.get("sha256")
        if pw_observation["matches_build_host_file"] and recorded.get("status") == "unsupported":
            missing.append("pw-cat build-host version was unsupported")
    return {"status": "failed" if missing else "unverified",
            "reason": "missing prerequisites" if missing else "static metadata only: runtime symbol resolution, pw-cat linked version, server and devices are unverified",
            "missing": sorted(set(missing)), "dependency_cache": cache_status,
            "required_sonames": required, "cache_candidates": resolved,
            "transitive_dependencies_and_symbol_versions": "unverified",
            "pw_cat": pw_observation, "pw_cat_supported_versions": PW_VERSIONS,
            "credentials": "not read", "devices": "not opened"}


def preflight(output):
    output = Path(os.path.abspath(output))
    errors = []
    directory_fd = None
    try:
        owner = safe_directory(output)
        if set(p.name for p in output.iterdir()) != set(FILES) | {MANIFEST}:
            raise ValueError("staging inventory has missing or extra files")
        directory_fd = os.open(output, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
        directory_info = os.fstat(directory_fd)
        current_directory = output.lstat()
        if (directory_info.st_dev, directory_info.st_ino, directory_info.st_uid) != (current_directory.st_dev, current_directory.st_ino, owner):
            raise ValueError("directory replaced during inspection")

        def pinned(name, limit):
            fd = os.open(name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=directory_fd)
            info = os.fstat(fd)
            if not stat.S_ISREG(info.st_mode) or info.st_uid != owner or info.st_mode & 0o022 or not 0 < info.st_size <= limit:
                os.close(fd)
                raise ValueError(f"{name}: unsafe owner, permissions, type or size")
            return fd, info

        def unchanged(fd, before):
            after = os.fstat(fd)
            if (before.st_size, before.st_mtime_ns, before.st_ctime_ns) != (after.st_size, after.st_mtime_ns, after.st_ctime_ns):
                raise ValueError("file changed during inspection")

        safe_file(output / MANIFEST, owner)
        fd, info = pinned(MANIFEST, 1024 * 1024)
        try:
            m = json.loads(os.pread(fd, info.st_size, 0), object_pairs_hook=unique_json)
            unchanged(fd, info)
        finally:
            os.close(fd)
        manifest_info = info
        validate_manifest(m)
        for name, expected in m["files"].items():
            path = output / name
            safe_file(path, owner, executable=True)
            fd, info = pinned(name, MAX_BINARY_SIZE)
            try:
                if not info.st_mode & stat.S_IXUSR:
                    raise ValueError(f"{name}: owner execute bit missing")
                h = hashlib.sha256()
                offset = 0
                while offset < info.st_size:
                    chunk = os.pread(fd, min(1024 * 1024, info.st_size - offset), offset)
                    if not chunk:
                        raise ValueError("file shortened during inspection")
                    offset += len(chunk)
                    h.update(chunk)
                if info.st_size != expected["size"] or h.hexdigest() != expected["sha256"]:
                    raise ValueError(f"{name}: content hash/size mismatch")
                actual = elf_metadata(path, fd=fd)
                unchanged(fd, info)
                current = os.stat(name, dir_fd=directory_fd, follow_symlinks=False)
                if (current.st_dev, current.st_ino) != (info.st_dev, info.st_ino):
                    raise ValueError("file replaced during inspection")
            finally:
                os.close(fd)
            if actual != expected["elf"]:
                raise ValueError(f"{name}: ELF metadata mismatch")
            if (actual["machine"], actual["class"]) != ARCHES[m["target"]]:
                raise ValueError(f"{name}: target architecture mismatch")
        manifest_now = os.stat(MANIFEST, dir_fd=directory_fd, follow_symlinks=False)
        if (manifest_info.st_dev, manifest_info.st_ino, manifest_info.st_size, manifest_info.st_mtime_ns, manifest_info.st_ctime_ns) != (manifest_now.st_dev, manifest_now.st_ino, manifest_now.st_size, manifest_now.st_mtime_ns, manifest_now.st_ctime_ns):
            raise ValueError("manifest changed or replaced during inspection")
        current_directory = output.lstat()
        if (directory_info.st_dev, directory_info.st_ino) != (current_directory.st_dev, current_directory.st_ino):
            raise ValueError("directory replaced during inspection")
        if set(os.listdir(directory_fd)) != set(FILES) | {MANIFEST}:
            raise ValueError("inventory changed during inspection")
        return {"inventory": "passed", "source": m["source"],
                "environment": runtime_metadata(m),
                "warning": "Unsigned inventory only; not an authenticity, authorization, legal compliance or readiness claim."}
    except (OSError, ValueError, TypeError, KeyError, RecursionError, subprocess.SubprocessError) as error:
        errors.append(str(error))
    finally:
        if directory_fd is not None:
            os.close(directory_fd)
    return {"inventory": "failed", "errors": errors,
            "environment": {"status": "unverified", "reason": "inventory failed; no runtime checks performed"}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    builder = sub.add_parser("build", help="compile and stage only; never install or run devices")
    builder.add_argument("--output", type=Path, required=True)
    checker = sub.add_parser("preflight", help="passive inventory inspection; never execute staged binaries")
    checker.add_argument("directory", type=Path)
    args = parser.parse_args()
    try:
        result = build(Path(__file__).resolve().parents[1], args.output.absolute()) if args.command == "build" else preflight(args.directory)
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        result = {"inventory": "failed", "errors": [str(error)]}
    print(json.dumps(result, indent=2))
    return 0 if result["inventory"] == "passed" and result.get("environment", {}).get("status") != "failed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
