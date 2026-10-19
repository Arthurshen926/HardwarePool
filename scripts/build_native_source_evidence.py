"""Archive hash-verified Moonlight build inputs and bind them to a local APK.

This is evidence of the recorded build inputs, not public redistribution approval
or proof that prebuilt OpenSSL archives can be independently reproduced.
"""
import argparse
import hashlib
import json
from pathlib import Path
import zipfile

ROOT = Path(__file__).resolve().parents[1]
JNI = ROOT / "platform/android/capyio-display-android/display-app/src/main/jni"
PATCH = ROOT / "third_party/patches/moonlight-common-c-874ac954/0001-capyio-video-only-client.patch"


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--stage", type=Path, required=True)
    parser.add_argument("--apk", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--openssl-source", type=Path, help="Optional pinned OpenSSL 4.0.2 source ZIP")
    args = parser.parse_args()
    stage = args.stage.resolve()
    manifest_bytes = (stage / "build-manifest.json").read_bytes()
    manifest = json.loads(manifest_bytes)
    if manifest["schemaVersion"] != 1 or manifest["distribution"] != "local-debug-only":
        raise ValueError("unsupported local build record")
    patch = PATCH.read_bytes()
    if sha(patch) != manifest["patchSha256"]:
        raise ValueError("recorded patch does not match repository patch")
    artifact = next(item for item in manifest["artifacts"] if item["abi"] == "arm64-v8a")
    library = stage / "jniLibs/arm64-v8a/libcapyio-gamestream-client.so"
    library_hash = sha(library.read_bytes())
    if library_hash != artifact["sha256"]:
        raise ValueError("staged library differs from build record")
    commands = (stage / "native-build/arm64-v8a/compile_commands.json").read_bytes()
    if sha(commands) != artifact["compileCommandsSha256"]:
        raise ValueError("compiler record differs from build record")
    apk_bytes = args.apk.read_bytes()
    with zipfile.ZipFile(args.apk) as apk:
        entry = "lib/arm64-v8a/libcapyio-gamestream-client.so"
        if sha(apk.read(entry)) != library_hash:
            raise ValueError("APK library differs from recorded build")

    openssl = None
    recipe = None
    if args.openssl_source:
        openssl = args.openssl_source.read_bytes()
        if sha(openssl) != "73b9138088c21c3dca17efca874f171684027bb87a3ef083598aa313a309ade7":
            raise ValueError("OpenSSL source differs from the pinned distribution plan")
        reference = stage / "downloads/moonlight-android.zip"
        if sha(reference.read_bytes()) != "1e79b4bf448d01dde9e78933bddc4d6ec2dc913c61055eb8df7d08ff7311f450":
            raise ValueError("Moonlight reference archive differs from its pinned revision")
        with zipfile.ZipFile(reference) as upstream:
            recipe = upstream.read("moonlight-android-b48494cb96bff23d8886c4775cc4f39a1075495d/app/src/main/jni/moonlight-core/build-openssl.sh")

    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    records = []
    archive_path = output / "recorded-native-inputs.zip"
    with zipfile.ZipFile(archive_path, "x", compression=zipfile.ZIP_DEFLATED) as archive:
        for base, group, prefix in [(stage / "source", "sourceFiles", "source"), (JNI, "integrationFiles", "integration")]:
            base = base.resolve()
            for item in manifest[group]:
                source = (base / item["path"]).resolve()
                if not source.is_relative_to(base):
                    raise ValueError("recorded source path escapes source root")
                data = source.read_bytes()
                if sha(data) != item["sha256"]:
                    raise ValueError(f"recorded input changed: {group}/{item['path']}")
                name = prefix + "/" + item["path"]
                archive.writestr(name, data)
                records.append({"file": name, "sha256": sha(data), "bytes": len(data)})
        if openssl is not None:
            for name, data in [("corresponding-source/openssl-4.0.2.zip", openssl),
                               ("corresponding-source/upstream-build-openssl.sh", recipe)]:
                archive.writestr(name, data)
                records.append({"file": name, "sha256": sha(data), "bytes": len(data)})
        archive.writestr("build-manifest.json", manifest_bytes)
        archive.writestr("compile_commands.json", commands)
        archive.writestr("patches/0001-capyio-video-only-client.patch", patch)
        for name in ["scripts/stage_moonlight_android_receiver.ps1", "scripts/build_moonlight_android_receiver.ps1", "docs/MOONLIGHT_ANDROID_DISTRIBUTION_PLAN.md"]:
            data = (ROOT / name).read_bytes()
            archive.writestr("current-recipe/" + name, data)
            records.append({"file": "current-recipe/" + name, "sha256": sha(data), "bytes": len(data)})
    report = {
        "kind": "capyio-recorded-native-input-evidence", "publicationApproved": False,
        "apkSha256": sha(apk_bytes), "apkEntry": entry, "librarySha256": library_hash,
        "buildManifestSha256": sha(manifest_bytes), "patchSha256": sha(patch),
        "opensslSourceIncluded": openssl is not None,
        "archiveSha256": sha(archive_path.read_bytes()), "ndkVersion": manifest["ndkVersion"],
        "verifiedSourceFiles": len(manifest["sourceFiles"]),
        "verifiedIntegrationFiles": len(manifest["integrationFiles"]), "files": records,
        "gaps": [
            ("Pinned complete OpenSSL source and upstream Linux/NDK recipe included; original static-library toolchain and independent rebuild remain unverified."
             if openssl is not None else "Recorded OpenSSL headers/static archives are not complete OpenSSL corresponding source or proof of its cross-build recipe."),
            "Current recipe copies are preserved for review; the historical manifest did not hash these scripts.",
            "This checks recorded inputs and APK byte identity, not an independent reproducible rebuild.",
            "Windows display engine and its linked/runtime dependencies require separate binary-to-source evidence.",
            "Source/notice/patent and final distribution review remains open; physical acceptance is separate."]}
    (output / "evidence.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8", newline="\n")
    print(json.dumps({key: report[key] for key in ("apkSha256", "librarySha256", "verifiedSourceFiles", "verifiedIntegrationFiles", "archiveSha256")}))


if __name__ == "__main__":
    main()
