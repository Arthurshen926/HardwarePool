import hashlib
import json
import os
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch
from system_components import ROOT, inspect_bundle, is_link, load_model, report


class ComponentsTest(unittest.TestCase):
    def test_plans_are_dependency_ordered_and_never_deploy_system_components(self):
        model = load_model(ROOT / 'release/system-components.json')
        install = report(model, 'install', 'all')['components']
        remove = report(model, 'remove', 'all')['components']
        self.assertEqual([c['id'] for c in remove], list(reversed([c['id'] for c in install])))
        self.assertEqual([c['id'] for c in install if c['handler']], ['app'])
        self.assertTrue(all(c['execution'] == 'plan-only' for c in install))

    def test_missing_and_modified_files_do_not_claim_verified_installation(self):
        model = load_model(ROOT / 'release/system-components.json')
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            exe = root / 'capyio-desktop.exe'
            exe.write_bytes(b'fixture')
            (root / 'manifest.json').write_text(json.dumps({'schemaVersion': 3, 'artifacts': [
                {'file': exe.name, 'bytes': 7, 'sha256': hashlib.sha256(b'fixture').hexdigest()}]}))
            result = report(model, 'verify', 'core', root)
            self.assertEqual(result['components'][0]['artifactState'], 'verified')
            self.assertIsNone(result['components'][0]['installedVersion'])
            self.assertEqual(result['components'][0]['systemState'], 'not-probed')
            self.assertEqual(result['components'][2]['artifactState'], 'not-verified')
            exe.write_bytes(b'changed')
            self.assertFalse(report(model, 'verify', 'core', root)['bundleIntegrity'])


class BundleIntegrityTest(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.payload = self.root / 'capyio-desktop.exe'
        self.payload.write_bytes(b'fixture')
        self.entry = {'file': self.payload.name, 'bytes': 7,
                      'sha256': hashlib.sha256(b'fixture').hexdigest()}
        self.model = load_model(ROOT / 'release/system-components.json')

    def write_manifest(self, entries):
        (self.root / 'manifest.json').write_text(json.dumps(
            {'schemaVersion': 3, 'artifacts': entries}), encoding='utf-8')

    def integrity(self):
        return report(self.model, 'verify', 'core', self.root)['bundleIntegrity']

    def test_exact_nested_inventory_and_uppercase_hash_remain_supported(self):
        nested = self.root / 'android' / 'CapyIO-debug.apk'
        nested.parent.mkdir()
        nested.write_bytes(b'')
        self.write_manifest([{**self.entry, 'sha256': self.entry['sha256'].upper()},
                             {'file': 'android/CapyIO-debug.apk', 'bytes': 0,
                              'sha256': hashlib.sha256(b'').hexdigest()}])
        self.assertTrue(self.integrity())
        result = report(self.model, 'verify', 'core', self.root)
        self.assertFalse(result['publicationApproved'])
        self.assertTrue(all(c['systemState'] == 'not-probed' for c in result['components']))

    def test_empty_and_invalid_inventory_is_rejected(self):
        for entries in ([], None, {}, [None]):
            with self.subTest(entries=entries):
                self.write_manifest(entries)
                with self.assertRaises(ValueError):
                    self.integrity()

    def test_missing_size_or_hash_mismatch_never_verifies(self):
        self.write_manifest([self.entry])
        self.payload.unlink()
        self.assertFalse(self.integrity())
        self.assertEqual(inspect_bundle(self.root)[1][self.payload.name]['state'], 'missing')
        self.payload.write_bytes(b'fixture')
        self.write_manifest([{**self.entry, 'bytes': 8}])
        self.assertFalse(self.integrity())
        self.write_manifest([self.entry])
        self.payload.write_bytes(b'changed')
        self.assertFalse(self.integrity())

    def test_unlisted_file_is_rejected(self):
        self.write_manifest([self.entry])
        (self.root / 'extra.dll').write_bytes(b'extra')
        with self.assertRaisesRegex(ValueError, 'unlisted'):
            self.integrity()

    def test_duplicate_and_case_alias_are_rejected(self):
        for name in (self.payload.name, self.payload.name.upper()):
            with self.subTest(name=name):
                self.write_manifest([self.entry, {**self.entry, 'file': name}])
                with self.assertRaisesRegex(ValueError, 'duplicate'):
                    self.integrity()

    def test_unsafe_and_manifest_self_paths_are_rejected(self):
        for name in ('../outside', '/outside', 'C:/outside', 'C:outside',
                     'folder/../outside', './outside', 'folder//outside',
                     'folder\\outside', 'manifest.json', 'MANIFEST.JSON', '',
                     'folder/', 'bad\x00file', None):
            with self.subTest(name=name):
                self.write_manifest([{**self.entry, 'file': name}])
                with self.assertRaisesRegex(ValueError, 'path'):
                    self.integrity()

    def test_malformed_hash_and_size_are_rejected(self):
        for field, value in (('bytes', None), ('bytes', -1), ('bytes', True),
                             ('bytes', '7'), ('sha256', None), ('sha256', 'bad')):
            with self.subTest(field=field, value=value):
                self.write_manifest([{**self.entry, field: value}])
                with self.assertRaisesRegex(ValueError, 'size or SHA256'):
                    self.integrity()

    def make_link(self, link, target, directory=False):
        try:
            link.symlink_to(target, target_is_directory=directory)
        except (OSError, NotImplementedError) as error:
            self.skipTest(f'Symbolic links unavailable: {error}')

    def test_file_link_rejected_even_when_target_is_inside_bundle(self):
        self.make_link(self.root / 'alias.exe', self.payload)
        self.write_manifest([self.entry, {**self.entry, 'file': 'alias.exe'}])
        with self.assertRaisesRegex(ValueError, 'links'):
            self.integrity()

    def test_directory_and_dangling_links_are_rejected(self):
        self.write_manifest([self.entry])
        for target, directory in ((self.root, True), (self.root / 'absent', False)):
            with self.subTest(directory=directory):
                link = self.root / 'linked'
                self.make_link(link, target, directory)
                try:
                    with self.assertRaisesRegex(ValueError, 'links'):
                        self.integrity()
                finally:
                    link.unlink()

    def test_manifest_link_is_rejected_before_read(self):
        self.make_link(self.root / 'manifest.json', self.payload)
        with self.assertRaisesRegex(ValueError, 'links'):
            self.integrity()

    def test_root_link_is_rejected(self):
        self.write_manifest([self.entry])
        self.make_link(self.root / 'alias', self.root, True)
        with self.assertRaisesRegex(ValueError, 'ordinary directory'):
            inspect_bundle(self.root / 'alias')

    def test_windows_reparse_attributes_without_is_junction_api(self):
        for mode in (stat.S_IFDIR, stat.S_IFREG):
            with self.subTest(mode=mode):
                path = SimpleNamespace(lstat=lambda: SimpleNamespace(
                    st_mode=mode, st_file_attributes=stat.FILE_ATTRIBUTE_REPARSE_POINT))
                self.assertTrue(is_link(path))
        ordinary = SimpleNamespace(lstat=lambda: SimpleNamespace(
            st_mode=stat.S_IFDIR, st_file_attributes=stat.FILE_ATTRIBUTE_DIRECTORY))
        self.assertFalse(is_link(ordinary))

    def test_reparse_directory_is_rejected_before_descending_or_reading(self):
        child = self.root / 'junction'
        child.mkdir()
        self.write_manifest([self.entry])
        original_lstat = Path.lstat

        def attributes(path, *args, **kwargs):
            if path == child:
                return SimpleNamespace(st_mode=stat.S_IFDIR,
                                       st_file_attributes=stat.FILE_ATTRIBUTE_REPARSE_POINT)
            return original_lstat(path, *args, **kwargs)

        with patch.object(Path, 'lstat', attributes), patch.object(Path, 'open') as opened:
            with self.assertRaisesRegex(ValueError, 'links'):
                self.integrity()
            opened.assert_not_called()

    @unittest.skipUnless(os.name == 'nt', 'Real directory junctions require Windows')
    def test_real_windows_junction_rejected(self):
        self.write_manifest([self.entry])
        with tempfile.TemporaryDirectory() as target:
            junction = self.root / 'junction'
            result = subprocess.run(['cmd', '/c', 'mklink', '/J', str(junction), target],
                                    capture_output=True, text=True, check=False)
            if result.returncode:
                self.skipTest('Temporary Windows junction creation unavailable')
            try:
                with self.assertRaisesRegex(ValueError, 'links'):
                    self.integrity()
            finally:
                junction.rmdir()


if __name__ == '__main__':
    unittest.main()
