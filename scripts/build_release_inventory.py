"""Generate a pre-build CycloneDX inventory and locally available license texts.

Requires Python 3.11+, existing PyYAML, Cargo and installed pnpm dependencies.
No dependency install, signing, publishing or device operation is performed.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tomllib
import uuid
from datetime import datetime, timezone

import yaml

ROOT = Path(__file__).resolve().parents[1]


def command(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True, encoding="utf-8").strip()


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    options = parser.parse_args()
    output = options.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    metadata = json.loads(command("cargo", "metadata", "--locked", "--offline", "--format-version", "1"))
    lock = tomllib.loads((ROOT / "Cargo.lock").read_text(encoding="utf-8"))
    checksums = {(p["name"], p["version"], p.get("source")): p.get("checksum") for p in lock["package"]}
    pnpm = yaml.safe_load((ROOT / "pnpm-lock.yaml").read_text(encoding="utf-8"))
    third_party = yaml.safe_load((ROOT / "third_party/THIRD_PARTY.yml").read_text(encoding="utf-8"))
    components, dependencies, licenses, missing = [], [], [], []

    def add(ref, name, version, declared_license=None, directory=None, checksum=None):
        item = {"type": "library", "bom-ref": ref, "name": name, "version": version}
        if declared_license:
            # Preserve upstream declarations; do not infer an SPDX expression or legal conclusion.
            item["licenses"] = [{"license": {"name": str(declared_license)}}]
        if checksum:
            item["hashes"] = [{"alg": "SHA-256", "content": checksum}]
        components.append(item)
        found = []
        if directory and directory.is_dir():
            for source in sorted(directory.iterdir()):
                if source.is_file() and source.name.upper().startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE")):
                    relative = Path("licenses") / hashlib.sha256(ref.encode()).hexdigest()[:16] / source.name
                    target = output / relative
                    target.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copyfile(source, target)
                    found.append({"file": relative.as_posix(), "sha256": digest(target)})
        licenses.append({"component": ref, "declaredLicense": declared_license, "texts": found})
        if not declared_license or not found:
            missing.append(ref)

    cargo_ids = {p["id"]: "cargo:" + p["name"] + "@" + p["version"] + ":" + hashlib.sha256((p.get("source") or "workspace").encode()).hexdigest()[:8] for p in metadata["packages"]}
    for package in metadata["packages"]:
        directory = Path(package["manifest_path"]).parent
        if package["id"] in metadata["workspace_members"] and package.get("license") == "Apache-2.0":
            directory = ROOT  # Workspace packages inherit the repository license.
        add(cargo_ids[package["id"]], package["name"], package["version"], package.get("license"),
            directory,
            checksums.get((package["name"], package["version"], package.get("source"))))
    for node in metadata["resolve"]["nodes"]:
        dependencies.append({"ref": cargo_ids[node["id"]], "dependsOn": sorted({cargo_ids[p] for p in node["dependencies"]})})

    installed = {}
    for path in (ROOT / "node_modules/.pnpm").glob("*/node_modules/*/package.json"):
        data = json.loads(path.read_text(encoding="utf-8"))
        installed[data["name"] + "@" + data["version"]] = (data, path.parent)
    for path in (ROOT / "node_modules/.pnpm").glob("*/node_modules/@*/*/package.json"):
        data = json.loads(path.read_text(encoding="utf-8"))
        installed[data["name"] + "@" + data["version"]] = (data, path.parent)
    for key in sorted(pnpm["packages"]):
        name, version = key.rsplit("@", 1)
        data, directory = installed.get(key, ({}, None))
        add("npm:" + key, name, version, data.get("license"), directory)
    for key, snapshot in pnpm.get("snapshots", {}).items():
        base = key.split("(", 1)[0]
        refs = set()
        for group in ("dependencies", "optionalDependencies"):
            for name, version in snapshot.get(group, {}).items():
                target = name + "@" + str(version).split("(", 1)[0]
                if target in pnpm["packages"]:
                    refs.add("npm:" + target)
        dependencies.append({"ref": "npm:" + base, "dependsOn": sorted(refs)})
    # Peer-resolution variants share one component but may have different edges.
    edges = {}
    for dependency in dependencies:
        edges.setdefault(dependency["ref"], set()).update(dependency["dependsOn"])
    dependencies = [{"ref": ref, "dependsOn": sorted(values)} for ref, values in sorted(edges.items())]

    for integration in third_party.get("planned_integrations", []):
        add("external:" + integration["id"], integration["id"], str(integration.get("pinned_release") or integration.get("pinned_revision") or "unrecorded"), integration.get("license"))
    android_path = ROOT / "target/release-evidence/android-runtime.json"
    android = json.loads(android_path.read_text(encoding="utf-8-sig"))
    for module in android["components"]:
        if module["id"].startswith("project "):
            continue
        add("maven:" + module["id"], module["name"] or module["id"], module["version"] or "unrecorded")

    refs = {component["bom-ref"] for component in components}
    if len(refs) != len(components):
        raise ValueError("duplicate component reference")
    for edge in dependencies:
        if edge["ref"] not in refs or any(ref not in refs for ref in edge["dependsOn"]):
            raise ValueError("unresolved component reference")

    inputs = ["Cargo.lock", "pnpm-lock.yaml", "third_party/THIRD_PARTY.yml", "release/public-alpha.json"]
    evidence = {"revision": command("git", "rev-parse", "HEAD"),
                "dirty": bool(command("git", "status", "--porcelain")),
                "inputs": {name: digest(ROOT / name) for name in inputs},
                "androidRuntimeSha256": digest(android_path),
                "componentCount": len(components), "licenseTextCount": sum(len(p["texts"]) for p in licenses),
                "licenseEvidenceIncomplete": sorted(missing),
                "publicationApproved": False,
                "gaps": ["Pre-build workspace inventory includes development and target-specific dependencies; not an exact shipped-binary BOM.",
                         "Third-party integrations are declarations, not proof that every integration is shipped.",
                         "External native binaries, JNI transitive dependencies, corresponding source/patches and final binary mapping require package-specific review.",
                         "Android platform, Gradle plugins and build tools are outside this inventory.",
                         "License texts collected from local caches are evidence, not completed redistribution approval.",
                         "Physical, system-installation and production-signing acceptance remains open."]}
    bom = {"bomFormat": "CycloneDX", "specVersion": "1.5", "serialNumber": "urn:uuid:" + str(uuid.uuid4()), "version": 1,
           "metadata": {"timestamp": datetime.now(timezone.utc).isoformat(), "lifecycles": [{"phase": "pre-build"}],
                        "properties": [{"name": "capyio:scope", "value": "workspace Rust/npm + Android runtime resolution + declared external integrations; see evidence.json"}]},
           "components": components, "dependencies": dependencies}
    for name, data in [("sbom.cdx.json", bom), ("license-index.json", licenses), ("evidence.json", evidence), ("android-runtime.json", android), ("third-party-records.json", third_party)]:
        (output / name).write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n", encoding="utf-8", newline="\n")
    (output / "README.md").write_text("# CapyIO pre-build release evidence\n\nNot approved for publication. Read evidence.json for coverage and unresolved gaps.\n\nSBOM format: https://cyclonedx.org/docs/1.5/json/\n\nLicense declarations and copied upstream texts are indexed in license-index.json.\nThird-party source revisions and local modification records are in third-party-records.json.\n", encoding="utf-8", newline="\n")
    print(json.dumps({"output": str(output), "components": len(components), "licenseTexts": evidence["licenseTextCount"], "incompleteLicenseEvidence": len(missing)}, ensure_ascii=False))


if __name__ == "__main__":
    main()
