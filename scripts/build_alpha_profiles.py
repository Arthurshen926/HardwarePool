"""Create local-debug Core/Extras from a verified integrated bundle, never install."""
import argparse
import hashlib
import json
import shutil
import subprocess
import zipfile
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CORE_FILES = (
    'capyio-desktop.exe', 'capyio-windows-service.exe',
    'capyio-native-virtual-microphone.exe', 'capyio-native-virtual-speaker.exe',
    'capyio-ptp-adb-lab.exe', 'capyio-avc-lab-receiver.exe',
    'capyio-camera-session-host.exe', 'capyio-camera-virtual-lab.exe',
)
TOOLS = ('verify_local_debug_bundle.ps1', 'update_local_debug_bundle.ps1',
         'install_local_debug_bundle.ps1', 'remove_local_debug_bundle.ps1',
         'check_local_prerequisites.ps1')


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def native_libraries(path):
    with zipfile.ZipFile(path) as archive:
        return sorted(n for n in archive.namelist() if n.startswith('lib/') and n.endswith('.so'))


def verify_core_apk(path):
    if native_libraries(path) != ['lib/arm64-v8a/libcapyio_android_jni.so']:
        raise ValueError('Core APK must contain only the first-party arm64 JNI library')


def copy_inventory(destination, sources):
    entries = []
    for name, source in sources.items():
        expected = digest(source)
        target = destination / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)
        if digest(target) != expected:
            raise ValueError('Input changed while packaging')
        entries.append({'file': name, 'bytes': target.stat().st_size, 'sha256': expected})
    return entries


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n', encoding='utf-8', newline='\n')


def source_build_inputs(report, apk, desktop, signer=None):
    evidence = json.loads(report.read_text(encoding='utf-8-sig'))
    if signer and evidence.get('androidSignerSha256', signer).lower() != signer.lower():
        raise ValueError('APK signer differs from source build evidence')
    passed = {step['name'] for step in evidence['steps'] if step['passed']}
    if not {'dependencies', 'frontend', 'desktop', 'helpers', 'android'} <= passed or not evidence.get('sourceInputsUnchanged'):
        raise ValueError('Source build evidence is incomplete')
    artifacts = {(report.parent / entry['file']).resolve(): entry for entry in evidence['artifacts']}
    layout = evidence.get('artifactLayout')
    if layout not in (None, 'retained-v1'):
        raise ValueError('Unsupported source build artifact layout')
    binary_directory = 'artifacts/windows' if layout == 'retained-v1' else 'cargo-target/debug'
    sources = {name: (report.parent / binary_directory / name).resolve() for name in CORE_FILES}
    if sources['capyio-desktop.exe'] != desktop.resolve():
        raise ValueError('Desktop does not belong to source build')
    for path in [*sources.values(), apk.resolve()]:
        entry = artifacts.get(path)
        if not entry or digest(path) != entry['sha256'].lower():
            raise ValueError('Artifact differs from source build evidence')
    return sources, evidence['sourceManifestSha256']


def build(args):
    source = args.integrated.resolve()
    subprocess.run(['pwsh', '-NoProfile', '-File', str(ROOT / 'scripts/verify_local_debug_bundle.ps1'),
                    '-PackageRoot', str(source)], check=True, stdout=subprocess.DEVNULL)
    original = json.loads((source / 'manifest.json').read_text(encoding='utf-8-sig'))
    if original.get('profile', 'integrated') != 'integrated':
        raise ValueError('Expected integrated source bundle')
    if args.android_signer_sha256.lower() != original.get('androidSignerSha256', '').lower():
        raise ValueError('Core and Extras APK signing identities must agree')
    verify_core_apk(args.core_apk)
    if digest(args.core_apk) != args.core_apk_sha256.lower():
        raise ValueError('Core APK differs from separately validated APK')
    fresh = source_build_inputs(args.first_party_build.resolve(), args.core_apk, args.desktop, args.android_signer_sha256) if args.first_party_build else None
    stamp = datetime.now(timezone.utc).strftime('%Y%m%d-%H%M%S-%f')
    output = ROOT / 'target/alpha-profiles' / stamp
    output.mkdir(parents=True, exist_ok=False)
    core, extras = output / 'Core', output / 'Experimental-Extras'
    sources = {name: source / name for name in CORE_FILES}
    sources['capyio-desktop.exe'] = args.desktop.resolve()
    if fresh:
        sources = fresh[0]
    sources.update({name: ROOT / 'scripts' / name for name in TOOLS})
    sources.update({'android/CapyIO-debug.apk': args.core_apk.resolve(),
                    'README.md': ROOT / 'docs/CORE_ALPHA_PACKAGE.md', 'LICENSE': ROOT / 'LICENSE'})
    inventory = copy_inventory(core, sources)
    common = {'productionRelease': False, 'publicationApproved': False,
              'distribution': 'local-debug-only', 'packagedAtUtc': datetime.now(timezone.utc).isoformat(),
              'sourceRevision': None, 'buildProvenance': 'mixed-inputs-explicit-hashes-not-clean-source-proof',
              'sourcePackageManifestSha256': digest(source / 'manifest.json')}
    manifest = {**common, 'schemaVersion': 3, 'kind': 'capyio-local-debug-bundle',
                'profile': 'core', 'launch': 'capyio-desktop.exe',
                'androidSignerSha256': args.android_signer_sha256,
                'artifacts': inventory}
    if fresh:
        manifest.update(buildProvenance='first-party-source-candidate-build-offline-cache',
                        sourceCandidateManifestSha256=fresh[1],
                        firstPartyBuildReportSha256=digest(args.first_party_build))
    write_json(core / 'manifest.json', manifest)
    subprocess.run(['pwsh', '-NoProfile', '-File', str(core / TOOLS[0]),
                    '-PackageRoot', str(core)], check=True)
    # Only optional binary/resources from the validated source. No arbitrary files.
    optional = {e['file']: source / e['file'] for e in original['artifacts']
                if e['file'] not in CORE_FILES and (e['file'].endswith(('.exe', '.dll', '.hlsl'))
                                                   or e['file'] == 'assets/apps.json')}
    experimental_apk = source / 'android/CapyIO-debug.apk'
    if 'lib/arm64-v8a/libcapyio-gamestream-client.so' not in native_libraries(experimental_apk):
        raise ValueError('Source Extras APK lacks the expected experimental receiver')
    optional['android/CapyIO-experimental-debug.apk'] = experimental_apk
    optional['README.md'] = ROOT / 'docs/EXPERIMENTAL_EXTRAS_PACKAGE.md'
    optional['verify_alpha_extras.py'] = ROOT / 'scripts/verify_alpha_extras.py'
    optional['THIRD_PARTY.yml'] = ROOT / 'third_party/THIRD_PARTY.yml'
    extra_inventory = copy_inventory(extras, optional)
    write_json(extras / 'manifest.json', {**common, 'schemaVersion': 1,
               'kind': 'capyio-local-experimental-extras', 'profile': 'extras',
               'buildProvenance': 'reused-verified-integrated-artifacts',
               'automaticOverlaySupported': False, 'requiresCoreManifestSha256': digest(core / 'manifest.json'),
               'androidSignerSha256': original.get('androidSignerSha256'), 'artifacts': extra_inventory})
    archives = []
    for directory in (core, extras):
        archive = Path(shutil.make_archive(str(directory), 'zip', directory))
        archives.append({'file': archive.name, 'sha256': digest(archive)})
    write_json(output / 'checksums.json', archives)
    print(output)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--integrated', type=Path, required=True)
    parser.add_argument('--core-apk', type=Path, required=True)
    parser.add_argument('--core-apk-sha256', required=True)
    parser.add_argument('--android-signer-sha256', required=True,
                        help='Identity from separate apksigner verification; this tool does not authenticate publishers')
    parser.add_argument('--desktop', type=Path, required=True)
    parser.add_argument('--first-party-build', type=Path, help='Hash-bound source build result.json for all Core binaries')
    build(parser.parse_args())
