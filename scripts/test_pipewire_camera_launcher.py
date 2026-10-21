"""Pure launcher assertions, never presented as native graph evidence."""
from pathlib import Path
import tempfile
from types import SimpleNamespace
from unittest import TestCase, mock

from run_pipewire_camera_launcher import (
    CONSOLE_PENDING, CONSOLE_SCENARIOS, CONSOLE_TEST, LauncherHarness, SCENARIOS,
    FINITE_NO_MEDIA, FINITE_SCENARIOS, FINITE_TEST,
    markers, verify_completion, verify_finite_pixels, verify_mode, verify_process,
)


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


class ConsoleLauncherTests(TestCase):
    def transcript(self, scenario):
        text = ('CONSOLE_PID 15\nLAUNCHER_PID 20\nPAIRED_STORE_AUTH verified\n'
                'CONSOLE_CONSENT exact_source_sink=true no_echo=true\n')
        if scenario not in CONSOLE_PENDING:
            text += 'SECURE_LISTENING no_media_children=true\n'
        if scenario not in (*CONSOLE_PENDING, 'handshake_timeout'):
            text += 'CAMERA_READY 21 capyio.camera.fixture\nDECODER_READY 22\n'
        return (text + f'CONSOLE_VERIFIED {scenario} actual_tty=true fresh_consent=true '
                'secret_redacted=true node_reaped=true\n'
                f'VERIFIED {scenario}\nCAMERA_STOPPED\n'
                f'test {CONSOLE_TEST} ... ok\n'
                'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 15 filtered out\n')

    def test_thirteen_explicit_profiles_preserve_eight_legacy_scenarios(self):
        self.assertEqual(len(SCENARIOS), 8)
        self.assertEqual(set(CONSOLE_SCENARIOS), {
            'pixels', 'no_approval', 'pretyped', 'mismatched_request',
            'mismatched_route', 'mismatched_peer', 'expiry', 'remote_eof',
            'local_stop', 'control_stop', 'source_revoke', 'partial_cavc', 'handshake_timeout'})
        for scenario in CONSOLE_SCENARIOS:
            verify_mode(scenario, paired_store=True, console=True)
        for args in (('pixels', False, True), ('remote_eof', True, False),
                     ('control_eof', True, True), ('pretyped', True, False),
                     ('unknown', True, True)):
            with self.subTest(args=args), self.assertRaises(ValueError):
                verify_mode(*args)

    def test_all_console_profiles_require_every_exact_claim_and_success(self):
        for scenario in CONSOLE_SCENARIOS:
            text = self.transcript(scenario)
            with self.subTest(scenario=scenario):
                verify_completion(text, scenario, 0, paired_store=True, console=True)
                for line in text.splitlines():
                    with self.assertRaises(ValueError):
                        verify_completion(text.replace(line + '\n', ''), scenario, 0,
                                          paired_store=True, console=True)
                for code in (1, 2, -15):
                    with self.assertRaises(ValueError):
                        verify_completion(text, scenario, code, paired_store=True, console=True)

    def test_no_fixture_substitution_skip_or_mode_confusion(self):
        text = self.transcript('pixels')
        for before, after in ((CONSOLE_TEST, 'hosted_linux_camera_graph'),
                              ('1 passed; 0 failed; 0 ignored', '0 passed; 0 failed; 1 ignored'),
                              ('CONSOLE_VERIFIED pixels', 'CONSOLE_VERIFIED local_stop'),
                              ('PAIRED_STORE_AUTH verified\n', '')):
            with self.subTest(after=after), self.assertRaises(ValueError):
                verify_completion(text.replace(before, after), 'pixels', 0,
                                  paired_store=True, console=True)
        with self.assertRaises(ValueError):
            verify_completion(text, 'pixels', 0, paired_store=True)
        with self.assertRaises(ValueError):
            verify_completion(text, 'pixels', 0, console=True)
        legacy = LauncherTests.base + 'VERIFIED pixels\nCAMERA_STOPPED\n'
        with self.assertRaises(ValueError):
            verify_completion(legacy, 'pixels', 0, paired_store=True, console=True)

    def test_unapproved_or_unbound_console_cannot_claim_media_or_lease(self):
        for scenario in (*CONSOLE_PENDING, 'handshake_timeout'):
            for extra in ('CAMERA_READY 21 capyio.camera.fixture\n', 'DECODER_READY 22\n'):
                with self.subTest(scenario=scenario, extra=extra), self.assertRaises(ValueError):
                    verify_completion(extra + self.transcript(scenario), scenario, 0,
                                      paired_store=True, console=True)
        for scenario in CONSOLE_PENDING:
            with self.assertRaises(ValueError):
                verify_completion('SECURE_LISTENING no_media_children=true\n' + self.transcript(scenario),
                                  scenario, 0, paired_store=True, console=True)

    def test_console_markers_reject_duplicate_malformed_and_partial_final_lines(self):
        text = self.transcript('pixels')
        for line in text.splitlines():
            if not line.startswith('CONSOLE_'):
                continue
            with self.subTest(line=line):
                with self.assertRaises(ValueError):
                    markers(text + line + '\n')
                with self.assertRaises(ValueError):
                    markers(text.replace(line, line + ' extra'))
        self.assertNotIn('CONSOLE_PID', markers('CONSOLE_PID '))
        with self.assertRaises(ValueError):
            markers('CONSOLE_PID ', complete=True)

    def harness(self, root):
        h = object.__new__(LauncherHarness)
        h.root, h.console = Path(root), True
        h.node = Path('/fixed/capyio-node')
        h.helper, h.decoder = Path('/fixed/camera-helper'), Path('/fixed/camera-decoder')
        return h

    def test_real_ownership_is_fixture_console_node_helpers_with_exact_commands(self):
        with tempfile.TemporaryDirectory() as root:
            h = self.harness(root)
            (h.root / 'owner.out').write_text(self.transcript('pixels'))
            with mock.patch('run_pipewire_camera_launcher.verify_process') as verify, \
                    mock.patch.object(Path, 'read_bytes', side_effect=[
                        b'capyio-linux-console\0camera\0--pairing-state\0file',
                        b'capyio-node\0linux-camera\0--pairing-state\0file']):
                self.assertEqual(h.ready_line(SimpleNamespace(pid=10, poll=lambda: None)),
                                 ('CAMERA_READY', '21', 'capyio.camera.fixture'))
                self.assertEqual(verify.call_args_list, [
                    mock.call(15, 10, Path('/fixed/capyio-linux-console')),
                    mock.call(20, 15, h.node), mock.call(21, 20, h.helper),
                    mock.call(22, 20, h.decoder)])

    def test_ownership_rejects_missing_pairing_wrong_operation_or_identity_alias(self):
        with tempfile.TemporaryDirectory() as root:
            h = self.harness(root)
            result = markers(self.transcript('pixels'))
            child = SimpleNamespace(pid=10)
            for argv in (b'camera\0file', b'playback\0--pairing-state',
                         b'linux-camera\0--pairing-state'):
                with mock.patch('run_pipewire_camera_launcher.verify_process'), \
                        mock.patch.object(Path, 'read_bytes', return_value=argv), self.assertRaises(ValueError):
                    h.verify_owner(child, result)
            for value in (10, 20):
                with self.assertRaises(ValueError):
                    h.verify_owner(child, dict(result, CONSOLE_PID=(str(value),)))

    def test_consent_checkpoint_independently_rejects_graph_changes_or_media_claim(self):
        with tempfile.TemporaryDirectory() as root:
            h = self.harness(root)
            prefix = self.transcript('no_approval').split('CONSOLE_VERIFIED')[0]
            (h.root / 'owner.out').write_text(prefix)
            child = SimpleNamespace(pid=10, poll=lambda: None)
            with mock.patch.object(h, 'verify_owner'), mock.patch.object(h, 'dump', return_value=[]), \
                    mock.patch.object(h, 'check'), mock.patch('run_pipewire_camera_launcher.time.sleep'):
                self.assertTrue(h.consent_checkpoint(child, {}))
                with self.assertRaises(ValueError):
                    h.consent_checkpoint(child, {9: ('node', 'sentinel')})
                (h.root / 'owner.out').write_text(prefix + 'SECURE_LISTENING no_media_children=true\n')
                with self.assertRaises(ValueError):
                    h.consent_checkpoint(child, {})

    def test_cleanup_requires_console_node_and_decoder_gone(self):
        h = self.harness('/unused')
        h.console_pid, h.node_pid, h.decoder_pid = 15, 20, 22
        with mock.patch.object(Path, 'exists', return_value=False) as exists:
            h.assert_owner_removed()
            self.assertEqual(exists.call_count, 3)
        for live in (15, 20, 22):
            with mock.patch.object(Path, 'exists', autospec=True,
                                   side_effect=lambda path: path.name == str(live)), self.assertRaises(ValueError):
                h.assert_owner_removed()

    def test_invalid_console_mode_fails_before_daemon_or_process_start(self):
        h = self.harness('/unused')
        with mock.patch.object(h, 'prepare_daemon') as prepare, self.assertRaises(ValueError):
            h.run_launcher(*(Path('/unused') for _ in range(5)), 'pixels', console=True)
        prepare.assert_not_called()

    def test_console_pending_orchestration_selects_actual_pty_fixture_and_checks_consent(self):
        with tempfile.TemporaryDirectory() as root:
            h = self.harness(root)
            h.env = {}
            (h.root / 'owner.out').write_text(self.transcript('no_approval'))
            graph = [{'id': 1, 'type': 'PipeWire:Interface:Node', 'info': {'props': {
                'node.name': 'capyio.test.null', 'object.serial': '1'}}}]
            child = SimpleNamespace(pid=10, stdin=mock.Mock(), wait=mock.Mock(return_value=0))
            with mock.patch.object(h, 'prepare_daemon'), mock.patch.object(h, 'dump', return_value=graph), \
                    mock.patch.object(h, 'launch', return_value=child) as launch, \
                    mock.patch.object(h, 'wait_for', side_effect=lambda predicate: predicate()), \
                    mock.patch.object(h, 'consent_checkpoint', return_value=True) as consent, \
                    mock.patch.object(h, 'assert_removed') as removed, \
                    mock.patch.object(h, 'assert_owner_removed') as reaped, \
                    mock.patch.object(h, 'check'), mock.patch('run_pipewire_camera_launcher.time.sleep'):
                h.node_pid = 20
                paths = [Path('/fixed/' + name) for name in ('driver', 'capyio-node', 'helper', 'decoder', 'consumer')]
                h.run_launcher(*paths, 'no_approval', paired_store=True, console=True)
                launch.assert_called_once_with([str(paths[0]), '--exact', CONSOLE_TEST,
                                                '--ignored', '--nocapture'], 'owner')
                consent.assert_called_once()
                self.assertEqual(child.stdin.write.call_args_list,
                                 [mock.call(b'approve synthetic camera\n'), mock.call(b'check consent\n')])
                self.assertEqual(h.env['CAPYIO_CAMERA_CONSOLE'], '1')
                self.assertEqual(h.env['CAPYIO_CAMERA_PAIRED_STORE'], '1')
                removed.assert_called_once()
                reaped.assert_called_once()


class FiniteStreamLauncherTests(TestCase):
    def transcript(self, scenario):
        text = ('LAUNCHER_PID 20\nPAIRED_STORE_AUTH verified\n'
                'FINITE_STREAM profile=finite_720p30_v1 source=synthetic_tls\n')
        if scenario in FINITE_NO_MEDIA:
            text += f'FINITE_REJECT_PENDING {scenario} no_data_listener=true no_media_children=true\n'
        else:
            text += LauncherTests.base.split('LAUNCHER_PID 20\n')[1]
        if scenario in ('sustained', 'backpressure'):
            text += 'FINITE_SENT frames=300 elapsed_ms=10035 queued=270 dropped=30\n'
            text += 'FINITE_MEMORY decoder_vm_hwm_kib=30000 helper_vm_hwm_kib=15000 node_vm_hwm_kib=20000\n'
        if scenario == 'oversize':
            text += 'FINITE_ATTACK byte_burst complete_cavc=true wire_oversize=false\n'
        return (text + f'VERIFIED {scenario}\nCAMERA_STOPPED\n'
                f'test {FINITE_TEST} ... ok\n'
                'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 15 filtered out\n')

    def test_eleven_explicit_finite_scenarios_do_not_replace_legacy_or_console(self):
        self.assertEqual(len(SCENARIOS), 8)
        self.assertEqual(len(CONSOLE_SCENARIOS), 13)
        self.assertEqual(set(FINITE_SCENARIOS), {
            'sustained', 'local_stop', 'control_stop', 'control_eof', 'source_revoke',
            'expiry', 'backpressure', 'malicious_rate', 'oversize', 'profile_mismatch',
            'old_source_guard'})
        for scenario in FINITE_SCENARIOS:
            verify_mode(scenario, paired_store=True, finite_stream=True)
            for paired, console in ((False, False), (True, True), (False, True)):
                with self.assertRaises(ValueError):
                    verify_mode(scenario, paired, console, finite_stream=True)
        for scenario in ('pixels', 'remote_eof', 'no_approval', 'unknown'):
            with self.assertRaises(ValueError):
                verify_mode(scenario, True, False, True)

    def test_every_finite_profile_requires_exact_actual_test_and_all_evidence(self):
        for scenario in FINITE_SCENARIOS:
            text = self.transcript(scenario)
            with self.subTest(scenario=scenario):
                verify_completion(text, scenario, 0, paired_store=True, finite_stream=True)
                for line in text.splitlines():
                    with self.assertRaises(ValueError):
                        verify_completion(text.replace(line + '\n', ''), scenario, 0,
                                          paired_store=True, finite_stream=True)
                for code in (1, 2, -15):
                    with self.assertRaises(ValueError):
                        verify_completion(text, scenario, code, paired_store=True, finite_stream=True)

    def test_finite_skip_substitution_or_mode_confusion_fails_closed(self):
        text = self.transcript('local_stop')
        for before, after in ((FINITE_TEST, CONSOLE_TEST),
                              ('1 passed; 0 failed; 0 ignored', '0 passed; 0 failed; 1 ignored'),
                              ('VERIFIED local_stop', 'VERIFIED expiry'),
                              ('source=synthetic_tls', 'source=android_production')):
            with self.subTest(after=after), self.assertRaises(ValueError):
                verify_completion(text.replace(before, after), 'local_stop', 0,
                                  paired_store=True, finite_stream=True)
        with self.assertRaises(ValueError):
            verify_completion(text, 'local_stop', 0, paired_store=True)
        with self.assertRaises(ValueError):
            verify_completion(LauncherTests.base + 'VERIFIED local_stop\nCAMERA_STOPPED\n',
                              'local_stop', 0, paired_store=True, finite_stream=True)

    def test_incompatible_profile_never_claims_listener_or_helpers(self):
        for scenario in FINITE_NO_MEDIA:
            for extra in ('SECURE_LISTENING no_media_children=true\n',
                          'CAMERA_READY 21 capyio.camera.fixture\n', 'DECODER_READY 22\n'):
                with self.subTest(scenario=scenario, extra=extra), self.assertRaises(ValueError):
                    verify_completion(extra + self.transcript(scenario), scenario, 0,
                                      paired_store=True, finite_stream=True)

    def test_sustained_submission_is_bounded_and_backpressure_must_be_observed(self):
        for before, after in (('frames=300', 'frames=120'), ('elapsed_ms=10035', 'elapsed_ms=9999'),
                              ('elapsed_ms=10035', 'elapsed_ms=12000'), ('queued=270', 'queued=269'),
                              ('decoder_vm_hwm_kib=30000', 'decoder_vm_hwm_kib=524288'),
                              ('queued=270 dropped=30', 'queued=300 dropped=0')):
            with self.subTest(after=after), self.assertRaises(ValueError):
                verify_completion(self.transcript('backpressure').replace(before, after),
                                  'backpressure', 0, paired_store=True, finite_stream=True)

    def pixels(self):
        return dict(result=0, verified_frames=182, first_sequence=2, last_sequence=240,
                    sequence_gaps=30, observed_span_ms=7935,
                    pts_span_us=(240 - 2) * 33333, finite_stream=True)

    def test_independent_full_pixel_stream_requires_count_late_sequence_and_both_spans(self):
        for scenario in ('sustained', 'backpressure'):
            verify_finite_pixels(self.pixels(), scenario)
        for key, value in (('verified_frames', 120), ('verified_frames', 301),
                           ('verified_frames', True), ('last_sequence', 239),
                           ('last_sequence', 301), ('first_sequence', 240),
                           ('sequence_gaps', 182), ('sequence_gaps', -1),
                           ('observed_span_ms', 4999), ('observed_span_ms', 12000),
                           ('pts_span_us', 5000000), ('finite_stream', False), ('result', 1)):
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                verify_finite_pixels(dict(self.pixels(), **{key: value}), 'sustained')
        for key in self.pixels():
            result = self.pixels()
            del result[key]
            with self.assertRaises(ValueError):
                verify_finite_pixels(result, 'sustained')
        # Three hundred queued frames are never independent consumption evidence.
        with self.assertRaises(ValueError):
            verify_finite_pixels(dict(result=0, queued=300, dropped=0), 'sustained')

    def test_finite_markers_reject_duplicates_malformed_and_incomplete_final_lines(self):
        for scenario in ('backpressure', 'old_source_guard'):
            text = self.transcript(scenario)
            for line in text.splitlines():
                if line.startswith('FINITE_'):
                    with self.assertRaises(ValueError):
                        markers(text + line + '\n')
                    with self.assertRaises(ValueError):
                        markers(text.replace(line, line + ' extra'))
        self.assertNotIn('FINITE_STREAM', markers('FINITE_STREAM '))
        with self.assertRaises(ValueError):
            markers('FINITE_STREAM ', complete=True)

    def test_invalid_finite_mode_fails_before_any_daemon_or_process_start(self):
        h = object.__new__(LauncherHarness)
        with mock.patch.object(h, 'prepare_daemon') as prepare, self.assertRaises(ValueError):
            h.run_launcher(*(Path('/unused') for _ in range(5)), 'sustained', finite_stream=True)
        prepare.assert_not_called()
