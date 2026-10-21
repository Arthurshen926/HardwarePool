"""Pure launcher assertions, never presented as native graph evidence."""
from pathlib import Path
import tempfile
from types import SimpleNamespace
from unittest import TestCase, mock

from run_pipewire_camera_launcher import LauncherHarness, SCENARIOS, markers, verify_completion, verify_process


class LauncherTests(TestCase):
    base = ('LAUNCHER_PID 20\nSECURE_LISTENING no_media_children=true\n'
            'CAMERA_READY 21 capyio.camera.fixture\nDECODER_READY 22\n')

    def test_all_expected_scenarios(self):
        self.assertEqual(set(SCENARIOS), {'pixels', 'local_stop', 'control_stop', 'control_eof', 'source_revoke', 'partial_cavc', 'expiry', 'handshake_timeout'})

    def test_completion_requires_exact_success_evidence_even_for_negative_cases(self):
        for scenario in SCENARIOS:
            base = self.base if scenario != 'handshake_timeout' else self.base.split('CAMERA_READY')[0]
            text = base + 'VERIFIED ' + scenario + '\nCAMERA_STOPPED\n'
            with self.subTest(scenario=scenario):
                verify_completion(text, scenario, 0)
                for code in (1, 2, -15):
                    with self.assertRaises(ValueError):
                        verify_completion(text, scenario, code)
                for line in text.splitlines():
                    with self.assertRaises(ValueError):
                        verify_completion(text.replace(line + '\n', ''), scenario, 0)
                with self.assertRaises(ValueError):
                    verify_completion(text.replace('VERIFIED ', 'NOT_VERIFIED '), scenario, 0)
                with self.assertRaises(ValueError):
                    verify_completion(text.replace('VERIFIED ' + scenario, 'VERIFIED wrong'), scenario, 0)

    def test_paired_store_requires_exact_authentication_mode_evidence(self):
        legacy = self.base + 'VERIFIED pixels\nCAMERA_STOPPED\n'
        paired = 'PAIRED_STORE_AUTH verified\n' + legacy
        verify_completion(legacy, 'pixels', 0)
        verify_completion(paired, 'pixels', 0, paired_store=True)
        with self.assertRaises(ValueError):
            verify_completion(legacy, 'pixels', 0, paired_store=True)
        with self.assertRaises(ValueError):
            verify_completion(paired, 'pixels', 0)
        for invalid in ('PAIRED_STORE_AUTH skipped', 'PAIRED_STORE_AUTH verified extra'):
            with self.assertRaises(ValueError):
                verify_completion(invalid + '\n' + legacy, 'pixels', 0, paired_store=True)
        with self.assertRaises(ValueError):
            verify_completion('PAIRED_STORE_AUTH verified\n' + paired, 'pixels', 0, paired_store=True)

    def test_duplicate_and_malformed_markers_rejected(self):
        valid = self.base + 'VERIFIED pixels\nCAMERA_STOPPED\n'
        for line in valid.splitlines():
            with self.subTest(line=line):
                with self.assertRaises(ValueError):
                    markers(valid + line + '\n')
                with self.assertRaises(ValueError):
                    markers(valid.replace(line, line + ' extra'))
        for line in ('LAUNCHER_PID 0', 'LAUNCHER_PID -1', 'CAMERA_READY x capyio.camera.x', 'CAMERA_READY 21 other.x', 'DECODER_READY nan'):
            with self.assertRaises(ValueError):
                markers(line + '\n')

    def test_incomplete_line_waits_but_final_parse_rejects(self):
        self.assertEqual(markers('DECODER_READY '), {})
        with self.assertRaises(ValueError):
            markers('DECODER_READY ', complete=True)

    def test_handshake_cannot_claim_media_readiness(self):
        with self.assertRaises(ValueError):
            verify_completion(self.base + 'VERIFIED handshake_timeout\nCAMERA_STOPPED\n', 'handshake_timeout', 0)

    def test_readiness_waits_for_both_orders_and_uses_actual_node_parent(self):
        with tempfile.TemporaryDirectory() as temporary:
            h = object.__new__(LauncherHarness)
            h.root = Path(temporary)
            h.node, h.helper, h.decoder = Path('/node'), Path('/camera'), Path('/decoder')
            child = SimpleNamespace(pid=10, poll=lambda: None)
            prefix = 'LAUNCHER_PID 20\nSECURE_LISTENING no_media_children=true\n'
            camera, decoder = 'CAMERA_READY 21 capyio.camera.fixture\n', 'DECODER_READY 22\n'
            for first, second in ((camera, decoder), (decoder, camera)):
                (h.root / 'owner.out').write_text(prefix + first)
                with mock.patch('run_pipewire_camera_launcher.verify_process') as verify:
                    self.assertIsNone(h.ready_line(child))
                    verify.assert_not_called()
                    (h.root / 'owner.out').write_text(prefix + first + second)
                    self.assertEqual(h.ready_line(child), ('CAMERA_READY', '21', 'capyio.camera.fixture'))
                    self.assertEqual(verify.call_args_list, [mock.call(20, 10, h.node), mock.call(21, 20, h.helper), mock.call(22, 20, h.decoder)])

    def test_readiness_rejects_aliases_and_premature_exit(self):
        with tempfile.TemporaryDirectory() as temporary:
            h = object.__new__(LauncherHarness)
            h.root = Path(temporary)
            (h.root / 'owner.out').write_text(self.base.replace('DECODER_READY 22', 'DECODER_READY 21'))
            with self.assertRaises(ValueError):
                h.ready_line(SimpleNamespace(pid=10, poll=lambda: None))
            (h.root / 'owner.out').write_text(self.base.split('CAMERA_READY')[0])
            with self.assertRaises(ValueError):
                h.ready_line(SimpleNamespace(pid=10, poll=lambda: 0))

    def test_process_requires_exact_executable_and_direct_parent(self):
        with mock.patch.object(Path, 'read_text', return_value='Name:\tchild\nPPid:\t20\n'), mock.patch.object(Path, 'resolve', return_value=Path('/helper')):
            verify_process(21, 20, Path('/helper'))
            with self.assertRaises(ValueError):
                verify_process(21, 10, Path('/helper'))
        with mock.patch.object(Path, 'read_text', return_value='PPid:\t20\n'), mock.patch.object(Path, 'resolve', side_effect=[Path('/other'), Path('/helper')]):
            with self.assertRaises(ValueError):
                verify_process(21, 20, Path('/helper'))
        for pid in (0, 1, 20):
            with self.assertRaises(ValueError):
                verify_process(pid, 20, Path('/helper'))
