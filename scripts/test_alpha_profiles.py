import hashlib
import json
import tempfile
import unittest
import zipfile
from pathlib import Path
from build_alpha_profiles import verify_core_apk, source_build_inputs, CORE_FILES
from verify_alpha_extras import verify


class ProfileTest(unittest.TestCase):
    def test_source_build_rejects_stale_or_incomplete_evidence(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            binary = root / 'cargo-target/debug'
            binary.mkdir(parents=True)
            paths = [binary / name for name in CORE_FILES] + [root / 'core.apk']
            for path in paths:
                path.write_bytes(b'fixture')
            report = root / 'result.json'
            evidence = {'steps': [{'name': name, 'passed': True} for name in
                                 ('dependencies', 'frontend', 'desktop', 'helpers', 'android')],
                        'sourceInputsUnchanged': True, 'sourceManifestSha256': 'fixture',
                        'artifacts': [{'file': p.relative_to(root).as_posix(),
                                       'sha256': hashlib.sha256(b'fixture').hexdigest()} for p in paths]}
            report.write_text(json.dumps(evidence))
            source_build_inputs(report, paths[-1], paths[0])
            evidence['androidSignerSha256'] = 'a' * 64
            report.write_text(json.dumps(evidence))
            with self.assertRaises(ValueError):
                source_build_inputs(report, paths[-1], paths[0], 'b' * 64)
            paths[1].write_bytes(b'stale')
            with self.assertRaises(ValueError):
                source_build_inputs(report, paths[-1], paths[0])
            evidence['sourceInputsUnchanged'] = False
            report.write_text(json.dumps(evidence))
            with self.assertRaises(ValueError):
                source_build_inputs(report, paths[-1], paths[0])

    def test_retained_outputs_package_without_compiler_cache(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            binary = root / 'artifacts/windows'
            binary.mkdir(parents=True)
            paths = [binary / name for name in CORE_FILES] + [root / 'artifacts/core.apk']
            for path in paths:
                path.write_bytes(b'fixture')
            report = root / 'result.json'
            evidence = {'artifactLayout': 'retained-v1',
                        'steps': [{'name': name, 'passed': True} for name in
                                  ('dependencies', 'frontend', 'desktop', 'helpers', 'android')],
                        'sourceInputsUnchanged': True, 'sourceManifestSha256': 'fixture',
                        'artifacts': [{'file': p.relative_to(root).as_posix(),
                                       'sha256': hashlib.sha256(b'fixture').hexdigest()} for p in paths]}
            report.write_text(json.dumps(evidence))
            sources, _ = source_build_inputs(report, paths[-1], paths[0])
            self.assertEqual(sources['capyio-desktop.exe'], paths[0])
            self.assertFalse((root / 'cargo-target').exists())
            paths[1].write_bytes(b'changed')
            with self.assertRaises(ValueError):
                source_build_inputs(report, paths[-1], paths[0])

    def test_core_rejects_experimental_native_payload(self):
        with tempfile.TemporaryDirectory() as folder:
            apk = Path(folder) / 'fixture.apk'
            with zipfile.ZipFile(apk, 'w') as archive:
                archive.writestr('lib/arm64-v8a/libcapyio_android_jni.so', b'fixture')
            verify_core_apk(apk)
            with zipfile.ZipFile(apk, 'a') as archive:
                archive.writestr('lib/arm64-v8a/libcapyio-gamestream-client.so', b'fixture')
            with self.assertRaises(ValueError):
                verify_core_apk(apk)

    def test_extras_requires_matching_core_and_rejects_tampering(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            extra, core = root / 'Extras', root / 'Core'
            extra.mkdir()
            core.mkdir()
            (core / 'manifest.json').write_bytes(b'core')
            (extra / 'worker.dll').write_bytes(b'fixture')
            manifest = {'schemaVersion': 1, 'kind': 'capyio-local-experimental-extras',
                        'publicationApproved': False, 'automaticOverlaySupported': False,
                        'requiresCoreManifestSha256': hashlib.sha256(b'core').hexdigest(),
                        'artifacts': [{'file': 'worker.dll', 'bytes': 7,
                                       'sha256': hashlib.sha256(b'fixture').hexdigest()}]}
            (extra / 'manifest.json').write_text(json.dumps(manifest))
            self.assertEqual(verify(extra, core), 1)
            (core / 'manifest.json').write_bytes(b'other')
            with self.assertRaises(ValueError):
                verify(extra, core)
            (extra / 'worker.dll').write_bytes(b'changed')
            with self.assertRaises(ValueError):
                verify(extra)


if __name__ == '__main__':
    unittest.main()
