"""Hardware-free inventory/negative cases; no staged ELF is ever executed."""
import copy
import json
import os
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch

import linux_staging as staging


class InventoryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.directory = Path(self.temp.name)
        # Copy only a known system ELF for synthetic metadata tests; never run it.
        for name in staging.FILES:
            shutil.copyfile('/usr/bin/true', self.directory / name)
            (self.directory / name).chmod(0o700)
        records = {name: {'sha256': staging.digest(self.directory / name),
                          'size': (self.directory / name).stat().st_size,
                          'elf': staging.elf_metadata(self.directory / name)}
                   for name in staging.FILES}
        target = next(t for t, v in staging.ARCHES.items()
                      if v == (records['capyio-node']['elf']['machine'], 2))
        self.manifest = {'schema': 1, 'purpose': 'local-staging-not-release',
                         'source': {'kind': 'export-unverified', 'sha': None, 'worktree': 'unverified'},
                         'target': target, 'features': copy.deepcopy(staging.FEATURES),
                         'files': records,
                         'ffmpeg': {name: {'path': '/synthetic/' + name, 'sha256': '0' * 64,
                                           'pkg_config_version': 'synthetic',
                                           'provenance': 'pkg-config-selected-development-library (not runtime resolution)',
                                           'configuration': {'status': 'unverified', 'reason': 'synthetic'},
                                           'license': {'status': 'unverified', 'reason': 'synthetic'}}
                                    for name in ('libavcodec', 'libavutil')},
                         'pw_cat': {'supported_versions': staging.PW_VERSIONS,
                                    'build_host': {'status': 'unverified', 'reason': 'synthetic test'}},
                         'libraries_bundled': False}
        self.write()

    def write(self):
        (self.directory / staging.MANIFEST).write_text(json.dumps(self.manifest))

    def result(self):
        # This suite tests inventory; a separate test checks passive environment.
        with patch.object(staging, 'runtime_metadata', return_value={'status': 'unverified', 'reason': 'synthetic test'}):
            return staging.preflight(self.directory)

    def failed(self, text):
        result = self.result()
        self.assertEqual(result['inventory'], 'failed', result)
        self.assertIn(text, ' '.join(result['errors']))

    def test_valid_inventory_is_not_runtime_ready(self):
        result = self.result()
        self.assertEqual(result['inventory'], 'passed')
        self.assertEqual(result['environment']['status'], 'unverified')

    def test_missing_helper(self):
        (self.directory / 'capyio-linux-avc-helper').unlink()
        self.failed('missing or extra')

    def test_extra_fixture(self):
        (self.directory / 'capyio-linux-touchpad-fixture-helper').write_text('fixture')
        self.failed('missing or extra')

    def test_symlink_helper(self):
        p = self.directory / 'capyio-linux-avc-helper'
        p.unlink()
        p.symlink_to('/usr/bin/true')
        self.failed('non-symlink')

    def test_symlink_manifest(self):
        p = self.directory / staging.MANIFEST
        backup = self.directory.parent / (self.directory.name + '-manifest')
        backup.write_text(p.read_text())
        self.addCleanup(backup.unlink)
        p.unlink()
        p.symlink_to(backup)
        self.failed('non-symlink')

    def test_tamper(self):
        with (self.directory / 'capyio-node').open('ab') as out:
            out.write(b'changed')
        self.failed('hash/size')

    def test_writable_helper(self):
        (self.directory / 'capyio-node').chmod(0o720)
        self.failed('unsafe owner')

    def test_wrong_owner(self):
        with patch.object(staging, 'safe_directory', return_value=os.getuid() + 1):
            self.failed('directory replaced')

    def test_writable_directory(self):
        self.directory.chmod(0o777)
        self.failed('unsafe directory')

    def test_symlink_directory(self):
        link = self.directory.parent / (self.directory.name + '-link')
        link.symlink_to(self.directory, target_is_directory=True)
        self.addCleanup(link.unlink)
        self.assertEqual(staging.preflight(link)['inventory'], 'failed')

    def test_wrong_architecture(self):
        self.manifest['target'] = next(t for t in staging.ARCHES if t != self.manifest['target'])
        self.write()
        self.failed('architecture mismatch')

    def test_forged_elf_metadata(self):
        self.manifest['files']['capyio-node']['elf']['needed'] = []
        self.write()
        self.failed('ELF metadata')

    def test_export_cannot_claim_clean_sha(self):
        self.manifest['source']['sha'] = 'a' * 40
        self.manifest['source']['worktree'] = 'clean'
        self.write()
        self.failed('export cannot claim')

    def test_extra_features_rejected(self):
        self.manifest['features']['capyio-node'].append('touchpad-test-fixture')
        self.write()
        self.failed('features')

    def test_manifest_duplicates_rejected(self):
        p = self.directory / staging.MANIFEST
        p.write_text(p.read_text()[:-1] + ', "schema": 1}')
        self.failed('duplicate')

    def test_ffmpeg_license_not_guessed(self):
        self.assertEqual(self.result()['inventory'], 'passed')
        self.assertEqual(self.manifest['ffmpeg']['libavcodec']['license']['status'], 'unverified')

    def test_export_without_git_remains_unverified(self):
        self.assertEqual(staging.source_state(self.directory), self.manifest['source'])

    def test_preflight_only_runs_fixed_read_only_tools(self):
        calls = []
        original = staging.run
        def observed(args, **kwargs):
            calls.append(args)
            return original(args, **kwargs)
        with patch.object(staging, 'run', side_effect=observed):
            result = staging.preflight(self.directory)
        self.assertEqual(result['inventory'], 'passed')
        self.assertTrue(calls)
        self.assertTrue(all(args[0] in ('/usr/bin/readelf', '/sbin/ldconfig', '/usr/sbin/ldconfig') for args in calls), calls)
        self.assertNotEqual(result['environment']['status'], 'passed')

    def test_missing_dependencies_are_failed_not_unverified(self):
        with patch.object(staging, 'run', return_value='0 libs found in cache'):
            result = staging.runtime_metadata(self.manifest)
        self.assertEqual(result['status'], 'failed')
        self.assertTrue(result['missing'])

    def test_foreign_directory_owner_rejected(self):
        with patch.object(staging.os, 'getuid', return_value=os.getuid() + 123):
            if self.directory.stat().st_uid != 0:
                self.failed('owner is not')

    def test_malformed_nested_manifest_fails_structurally(self):
        cases = [('schema', True), ('pw_cat', []), ('files', None),
                 ('source', []), ('target', []), ('ffmpeg', [])]
        baseline = copy.deepcopy(self.manifest)
        for field, value in cases:
            with self.subTest(field=field):
                self.manifest = copy.deepcopy(baseline)
                self.manifest[field] = value
                self.write()
                self.assertEqual(self.result()['inventory'], 'failed')

    def test_nested_extra_fields_rejected(self):
        self.manifest['pw_cat']['build_host']['secret'] = 'not permitted'
        self.write()
        self.failed('metadata')

    def test_elf_nested_type_rejected(self):
        self.manifest['files']['capyio-node']['elf']['needed'] = 'not a list'
        self.write()
        self.failed('dependency')

    def test_huge_binary_limit(self):
        self.manifest['files']['capyio-node']['size'] = staging.MAX_BINARY_SIZE + 1
        self.write()
        self.failed('size')

    def test_swap_after_check_never_follows_symlink(self):
        original = staging.safe_file
        target = self.directory / 'capyio-node'
        def swapped(path, owner, executable=False):
            value = original(path, owner, executable)
            if path.name == 'capyio-node':
                path.unlink()
                path.symlink_to('/usr/bin/true')
            return value
        with patch.object(staging, 'safe_file', side_effect=swapped):
            self.assertEqual(self.result()['inventory'], 'failed')

    def test_replacement_after_open_is_detected(self):
        original = staging.elf_metadata
        def swapped(path, fd=None):
            metadata = original(path, fd)
            if path.name == 'capyio-node':
                path.unlink()
                shutil.copyfile('/usr/bin/true', path)
                path.chmod(0o700)
            return metadata
        with patch.object(staging, 'elf_metadata', side_effect=swapped):
            self.failed('during inspection')

    def test_manifest_replacement_after_parse_detected(self):
        original = staging.validate_manifest
        def replaced(manifest):
            original(manifest)
            path = self.directory / staging.MANIFEST
            path.unlink()
            path.write_text('{}')
        with patch.object(staging, 'validate_manifest', side_effect=replaced):
            self.failed('manifest changed or replaced')

    def test_deep_json_fails_structurally(self):
        (self.directory / staging.MANIFEST).write_text('[' * 2000 + '0' + ']' * 2000)
        self.assertEqual(self.result()['inventory'], 'failed')

    def test_metadata_output_bound(self):
        with patch.object(staging, 'MAX_TOOL_OUTPUT', 2):
            with self.assertRaisesRegex(ValueError, 'output exceeds'):
                staging.run(['/usr/bin/readelf', '--version'])

    def test_build_commands_locked_exact_features_and_no_fixture(self):
        with patch.object(staging, 'source_state', return_value=self.manifest['source']), \
             patch.object(staging, 'ffmpeg_metadata', return_value=self.manifest['ffmpeg']), \
             patch.object(staging, 'pw_metadata', return_value={'status': 'unverified', 'reason': 'synthetic test'}), \
             patch.object(staging, 'run', return_value='host: ' + self.manifest['target']), \
             patch.object(staging.subprocess, 'run') as command, \
             patch.object(staging.shutil, 'copyfile', side_effect=RuntimeError('stop after builds')):
            with self.assertRaisesRegex(RuntimeError, 'stop after builds'):
                staging.build(self.directory, self.directory / 'out')
        self.assertEqual(len(command.call_args_list), 4)
        for call, (binary, (package, features)) in zip(command.call_args_list, staging.FILES.items()):
            args = call.args[0]
            self.assertIn('--locked', args)
            self.assertIn('--no-default-features', args)
            self.assertEqual(args[args.index('--features') + 1], features)
            self.assertEqual(args[args.index('--bin') + 1], binary)
            self.assertNotIn('test-fixture', str(args))


if __name__ == '__main__':
    unittest.main()
