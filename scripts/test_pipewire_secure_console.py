"""Pure evidence and mocked /proc checks. No socket, PTY, bus or audio opens."""
from pathlib import Path
import signal
import tempfile
import unittest
from unittest import mock

import run_pipewire_secure_audio as secure
from run_pipewire_secure_console import (
    CLEANUP_FIELDS, FORMAT, NEGATIVE_SCENARIOS, PREFIX, SCENARIOS, TEST,
    SecureConsoleHarness, api_evidence, execution_marker, process_ids, strict_json,
    validate_bound, validate_pending, validate_result, verify_console_owner,
    verify_execution, verify_reaped,
)

OWNERS = {'fixture_pid': 100, 'console_pid': 150, 'owner_pid': 200}


def common(scenario, stage):
    return {'version': 1, 'scenario': scenario, 'stage': stage, 'format': FORMAT,
            'surface': 'production_console'} | OWNERS


def pending(scenario, stage='before_local'):
    return common(scenario, stage) | {
        'helper_absent': True, 'lease_count': 0, 'bytes_written': 0}


def bound(scenario):
    return common(scenario, 'bound') | {'helper_pid': 201, 'source_lease_expires_at_ms': 9000}


def result(scenario):
    if scenario == 'local_eof':
        return common(scenario, 'stopped') | {
            'helper_pid': 201, 'source_lease_expires_at_ms': 9000,
            'terminal_observed_at_ms': None, 'terminal_cause': None,
            'counter_observation': 'pre_input_eof_status',
            'status_packets_written': 300, 'status_bytes_written': 300 * 960,
            'status_observed_at_ms': 7800, 'input_eof_at_ms': 7900,
            'source_control_closed_observed_at_ms': 8000,
            'source_control_closed': True,
            'source_records_sent': 301, 'source_records_attempted': 301,
            'injected': scenario, 'verified': scenario,
            'session_closed': True, 'leases_released': True, 'helper_reaped': True,
            'console_reaped': True, 'node_reaped': True,
            'protected_read': True, 'secret_redacted': True,
        }
    negative = scenario in NEGATIVE_SCENARIOS
    return common(scenario, 'stopped') | {
        'terminal_cause': ('not_started' if negative else {
            'stop': 'local_stop', 'source_revoke': 'media_closed', 'media_eof': 'media_closed',
            'control_eof': 'control_closed', 'expiry': 'failure'}[scenario]),
        'helper_pid': None if negative else 201,
        'source_lease_expires_at_ms': None if negative else 9000,
        'terminal_observed_at_ms': 9000 if scenario == 'expiry' else 8000,
        'bytes_written': 0 if negative else 300 * 960,
        'packets_written': 0 if negative else 300,
        'packets_dropped_busy': 0,
        'source_records_sent': 0 if negative else 301,
        'source_records_attempted': 0 if negative else 301 + int(scenario in ('source_revoke', 'expiry')),
        'injected': scenario, 'verified': scenario,
        'session_closed': True, 'leases_released': True, 'helper_reaped': True,
        'console_reaped': True, 'node_reaped': True,
        'protected_read': True, 'secret_redacted': True,
    }


def stdout():
    return (f'running 1 test\ntest {TEST} ... ok\n'
            'test result: ok. 1 passed; 0 failed; 0 ignored;'
            ' 0 measured; 2 filtered out; finished in 3.01s\n')


class SecureConsoleEvidenceTests(unittest.TestCase):
    def test_exact_new_scenarios_leave_all_old_scenarios_unchanged(self):
        self.assertEqual(set(SCENARIOS), {'stop', 'local_eof', 'no_approval', 'pretyped',
                                         'wrong_request', 'wrong_route', 'wrong_peer',
                                         'source_revoke', 'expiry', 'control_eof', 'media_eof'})
        self.assertEqual(len(SCENARIOS), 11)
        self.assertEqual(set(secure.SCENARIOS), {'waveform', 'stop', 'source_revoke',
                                               'sink_revoke', 'expiry', 'control_eof',
                                               'media_eof', 'no_authority', 'peer_denied'})
        self.assertEqual(NEGATIVE_SCENARIOS, {'no_approval', 'pretyped', 'wrong_request',
                                             'wrong_route', 'wrong_peer'})
        self.assertNotEqual(TEST, secure.TEST)
        self.assertNotEqual(PREFIX, secure.PREFIX)

    def test_bounded_unique_object_json_reuses_original_parser(self):
        self.assertIs(strict_json, secure.strict_json)
        for raw in (b'[]', b'null', b'{"owner_pid":200,"owner_pid":200}',
                    b' ' * 4097, b'{'):
            with self.subTest(raw=raw[:40]), self.assertRaises(ValueError):
                strict_json(raw)

    def test_processes_require_distinct_bounded_integer_identities(self):
        self.assertEqual(process_ids(OWNERS), OWNERS)
        for key in OWNERS:
            for replacement in (None, True, False, 0, 1, -1, '200', 2.5, 1 << 31):
                with self.subTest(key=key, replacement=replacement), self.assertRaises(ValueError):
                    process_ids(OWNERS | {key: replacement})
            for other in OWNERS:
                if key != other:
                    with self.subTest(key=key, other=other), self.assertRaises(ValueError):
                        process_ids(OWNERS | {key: OWNERS[other]})

    def test_all_exact_console_scenarios_accepted(self):
        for scenario in SCENARIOS:
            with self.subTest(scenario=scenario):
                validate_pending(pending(scenario), scenario, 'before_local', OWNERS)
                if scenario not in NEGATIVE_SCENARIOS:
                    validate_pending(pending(scenario, 'before_source'),
                                     scenario, 'before_source', OWNERS)
                    validate_bound(bound(scenario), scenario, OWNERS)
                validate_result(result(scenario), scenario, OWNERS,
                                None if scenario in NEGATIVE_SCENARIOS else bound(scenario))
                verify_execution(stdout(), execution_marker(scenario) + '\n', scenario)

    def test_pending_requires_no_node_helper_lease_or_pcm(self):
        for stage in ('before_local', 'before_source'):
            for key, replacement in (('helper_absent', False), ('lease_count', 1),
                                      ('lease_count', False), ('bytes_written', 1),
                                      ('bytes_written', True), ('owner_pid', 202),
                                      ('fixture_pid', 101), ('console_pid', 151),
                                      ('version', True), ('format', 'pcm-s16le-48000-mono'),
                                      ('surface', 'reusable_rust_apis')):
                with self.subTest(stage=stage, key=key), self.assertRaises(ValueError):
                    validate_pending(pending('stop', stage) | {key: replacement},
                                     'stop', stage, OWNERS)

    def test_pending_exact_keys_and_known_stage(self):
        value = pending('stop')
        for field in value:
            bad = dict(value)
            del bad[field]
            with self.subTest(field=field), self.assertRaises(ValueError):
                validate_pending(bad, 'stop', 'before_local', OWNERS)
        for bad in (value | {'extra': True}, pending('stop', 'invented')):
            with self.assertRaises(ValueError):
                validate_pending(bad, 'stop', bad['stage'], OWNERS)

    def test_denials_cannot_reach_source_grant_checkpoint(self):
        for scenario in NEGATIVE_SCENARIOS:
            with self.subTest(scenario=scenario), self.assertRaises(ValueError):
                validate_pending(pending(scenario, 'before_source'),
                                 scenario, 'before_source', OWNERS)

    def test_final_exact_fields_no_omission_or_extra(self):
        value = result('stop')
        for field in value:
            bad = dict(value)
            del bad[field]
            with self.subTest(field=field), self.assertRaises((ValueError, KeyError)):
                validate_result(bad, 'stop', OWNERS, bound('stop'))
        with self.assertRaises(ValueError):
            validate_result(value | {'extra': True}, 'stop', OWNERS, bound('stop'))

    def test_cleanup_and_credential_checks_require_true_boolean(self):
        for key in CLEANUP_FIELDS | {'helper_reaped', 'session_closed', 'leases_released'}:
            for replacement in (False, 1, 'true', None):
                with self.subTest(key=key, replacement=replacement), self.assertRaises(ValueError):
                    validate_result(result('stop') | {key: replacement},
                                    'stop', OWNERS, bound('stop'))

    def test_negative_scenarios_cannot_claim_activation_or_records(self):
        for scenario in NEGATIVE_SCENARIOS:
            with self.subTest(scenario=scenario), self.assertRaises(ValueError):
                validate_bound(bound(scenario), scenario, OWNERS)
            for field, replacement in (('helper_pid', 201), ('source_lease_expires_at_ms', 9000),
                                       ('bytes_written', 960), ('packets_written', 1),
                                       ('source_records_sent', 1), ('source_records_attempted', 1)):
                with self.subTest(scenario=scenario, field=field), self.assertRaises(ValueError):
                    validate_result(result(scenario) | {field: replacement}, scenario, OWNERS)

    def test_negative_translation_cannot_hide_wrong_terminal_cause(self):
        for scenario in NEGATIVE_SCENARIOS:
            for key in ('injected', 'verified'):
                for wrong in ('no_authority', 'peer_denied', 'stop', 'wrong_scenario'):
                    with self.subTest(scenario=scenario, key=key, wrong=wrong), self.assertRaises(ValueError):
                        validate_result(result(scenario) | {key: wrong}, scenario, OWNERS)

    def test_terminal_cause_is_exact_and_cannot_be_replaced_by_timeout(self):
        causes = {'not_started', 'local_stop', 'peer_stop', 'media_closed',
                  'media_timeout', 'media_failure', 'control_closed', 'failure',
                  'unknown', 'raw error text', None, True}
        for scenario in SCENARIOS:
            value = result(scenario)
            accepted = ({'failure', 'media_timeout', 'media_closed'} if scenario == 'expiry'
                        else {value['terminal_cause']})
            for wrong in causes - accepted:
                with self.subTest(scenario=scenario, wrong=wrong), self.assertRaises(ValueError):
                    validate_result(value | {'terminal_cause': wrong}, scenario, OWNERS,
                                    None if scenario in NEGATIVE_SCENARIOS else bound(scenario))

    def test_each_expiry_boundary_cause_requires_original_deadline_not_early_observation(self):
        for cause in ('failure', 'media_timeout', 'media_closed'):
            for observed in (9000, 9001):
                validate_result(result('expiry') | {'terminal_cause': cause,
                                                   'terminal_observed_at_ms': observed},
                                'expiry', OWNERS, bound('expiry'))
            for observed in (0, 8990, 8999):
                with self.subTest(cause=cause, observed=observed), self.assertRaises(ValueError):
                    validate_result(result('expiry') | {'terminal_cause': cause,
                                                       'terminal_observed_at_ms': observed},
                                    'expiry', OWNERS, bound('expiry'))

    def test_terminal_cause_is_not_accepted_on_pending_or_bound_checkpoints(self):
        with self.assertRaises(ValueError):
            validate_pending(pending('stop') | {'terminal_cause': 'not_started'},
                             'stop', 'before_local', OWNERS)
        with self.assertRaises(ValueError):
            validate_bound(bound('stop') | {'terminal_cause': 'local_stop'}, 'stop', OWNERS)

    def test_local_eof_uses_distinct_snapshot_not_old_terminal_counter_oracle(self):
        with self.assertRaises(ValueError):
            api_evidence(result('local_eof'))
        with mock.patch.object(secure, 'validate_result') as terminal_oracle:
            validate_result(result('local_eof'), 'local_eof', OWNERS, bound('local_eof'))
            terminal_oracle.assert_not_called()
        for extra in ('bytes_written', 'packets_written', 'packets_dropped_busy',
                      'source_media_closed', 'source_closed_observed_at_ms'):
            with self.subTest(extra=extra), self.assertRaises(ValueError):
                validate_result(result('local_eof') | {extra: 0},
                                'local_eof', OWNERS, bound('local_eof'))
        for key, wrong in (('terminal_observed_at_ms', 8000), ('terminal_cause', 'local_stop'),
                           ('terminal_cause', 'failure'), ('counter_observation', 'terminal'),
                           ('counter_observation', 'post_input_eof_status')):
            with self.subTest(key=key, wrong=wrong), self.assertRaises(ValueError):
                validate_result(result('local_eof') | {key: wrong},
                                'local_eof', OWNERS, bound('local_eof'))

    def test_local_eof_requires_each_exact_field_and_current_process_binding(self):
        value = result('local_eof')
        for key in value:
            bad = dict(value)
            del bad[key]
            with self.subTest(key=key), self.assertRaises((ValueError, KeyError)):
                validate_result(bad, 'local_eof', OWNERS, bound('local_eof'))
        for key in ('helper_pid', 'source_lease_expires_at_ms', *OWNERS):
            with self.subTest(key=key), self.assertRaises(ValueError):
                validate_result(value | {key: value[key] + 1},
                                'local_eof', OWNERS, bound('local_eof'))
        with self.assertRaises(ValueError):
            validate_result(value, 'local_eof', OWNERS)

    def test_local_eof_requires_observed_source_control_closure_and_true_cleanup(self):
        for key in CLEANUP_FIELDS | {'session_closed', 'leases_released', 'helper_reaped',
                                     'source_control_closed'}:
            for wrong in (False, 1, 'true', None):
                with self.subTest(key=key, wrong=wrong), self.assertRaises(ValueError):
                    validate_result(result('local_eof') | {key: wrong},
                                    'local_eof', OWNERS, bound('local_eof'))

    def test_local_eof_snapshot_has_exact_accounting_without_media_closure_probes(self):
        for key, wrong in (('status_packets_written', 299), ('status_bytes_written', 300 * 960 - 1),
                           ('source_records_sent', 299), ('source_records_attempted', 300),
                           ('source_records_attempted', 302), ('status_packets_written', True),
                           ('status_bytes_written', 300 * 960.0)):
            with self.subTest(key=key, wrong=wrong), self.assertRaises(ValueError):
                validate_result(result('local_eof') | {key: wrong},
                                'local_eof', OWNERS, bound('local_eof'))

    def test_local_eof_ordered_observations_must_precede_exact_original_expiry(self):
        for key, wrong in (('status_observed_at_ms', 7901), ('input_eof_at_ms', 7799),
                           ('input_eof_at_ms', 8001), ('source_control_closed_observed_at_ms', 7899),
                           ('source_control_closed_observed_at_ms', 9000),
                           ('source_control_closed_observed_at_ms', 9001), ('input_eof_at_ms', True),
                           ('status_observed_at_ms', 0)):
            with self.subTest(key=key, wrong=wrong), self.assertRaises(ValueError):
                validate_result(result('local_eof') | {key: wrong},
                                'local_eof', OWNERS, bound('local_eof'))
        validate_result(result('local_eof') | {'status_observed_at_ms': 8999,
                                             'input_eof_at_ms': 8999,
                                             'source_control_closed_observed_at_ms': 8999},
                        'local_eof', OWNERS, bound('local_eof'))

    def test_positive_needs_exact_pinned_bound_process_and_lease(self):
        for key in ('helper_pid', 'source_lease_expires_at_ms', *OWNERS):
            changed = result('stop') | {key: result('stop')[key] + 1}
            with self.subTest(key=key), self.assertRaises(ValueError):
                validate_result(changed, 'stop', OWNERS, bound('stop'))
        with self.assertRaises(ValueError):
            validate_result(result('stop'), 'stop', OWNERS)
        for pid in (*OWNERS.values(), True, '201', 0, 1, 1 << 31):
            with self.subTest(pid=pid), self.assertRaises(ValueError):
                validate_bound(bound('stop') | {'helper_pid': pid}, 'stop', OWNERS)

    def test_positive_record_accounting_keeps_old_exact_oracle(self):
        for field, replacement in (('injected', 'other'), ('verified', 'other'),
                                   ('bytes_written', 300 * 960 - 1), ('packets_written', 299),
                                   ('packets_dropped_busy', 1), ('source_records_sent', 299),
                                   ('source_records_attempted', 300)):
            with self.subTest(field=field), self.assertRaises(ValueError):
                validate_result(result('stop') | {field: replacement}, 'stop', OWNERS, bound('stop'))
        value = result('expiry') | {'source_records_sent': 300,
                                   'source_records_attempted': 301,
                                   'packets_written': 301, 'bytes_written': 301 * 960}
        validate_result(value, 'expiry', OWNERS, bound('expiry'))
        for attempted in (300, 302):
            with self.subTest(attempted=attempted), self.assertRaises(ValueError):
                validate_result(value | {'source_records_attempted': attempted},
                                'expiry', OWNERS, bound('expiry'))

    def test_exact_source_expiry_has_no_early_tolerance(self):
        for observed in (0, 8990, 8999):
            with self.subTest(observed=observed), self.assertRaises(ValueError):
                validate_result(result('expiry') | {'terminal_observed_at_ms': observed},
                                'expiry', OWNERS, bound('expiry'))
        for observed in (9000, 9001):
            validate_result(result('expiry') | {'terminal_observed_at_ms': observed},
                            'expiry', OWNERS, bound('expiry'))

    def test_other_requested_causes_cannot_pass_from_incidental_expiry(self):
        for scenario in set(SCENARIOS) - NEGATIVE_SCENARIOS - {'expiry'}:
            with self.subTest(scenario=scenario), self.assertRaises(ValueError):
                validate_result(result(scenario) | {'terminal_observed_at_ms': 9000},
                                scenario, OWNERS, bound(scenario))

    def test_no_skip_api_substitution_or_duplicate_execution(self):
        good = stdout()
        marker = execution_marker('stop')
        for text in ('', good.replace('1 passed', '0 passed'),
                     good.replace('0 ignored', '1 ignored'),
                     good.replace(TEST, secure.TEST), good.replace('... ok', '... ignored'),
                     good + good, good + f'test different ... ok\n',
                     good.replace('0 failed', '1 failed')):
            with self.subTest(text=text), self.assertRaises(ValueError):
                verify_execution(text, marker, 'stop')

    def test_mandatory_execution_marker_cannot_be_missing_duplicated_or_weakened(self):
        marker = execution_marker('stop')
        for text in ('', marker + '\n' + marker, marker + ' extra',
                     execution_marker('expiry'), marker.replace('actual_tty=true', 'actual_tty=false'),
                     marker.replace('fresh_consent=true ', ''), marker.replace('protected_read=true', 'protected_read=false')):
            with self.subTest(text=text), self.assertRaises(ValueError):
                verify_execution(stdout(), text, 'stop')


class SecureConsoleOwnershipTests(unittest.TestCase):
    def owner_patches(self, parent=(100, 150), binaries=None, groups=(150, 150), commands=None):
        binaries = binaries or [Path('/build/capyio-linux-console'), Path('/build/capyio-node')]
        commands = commands or [b'capyio-linux-console\0playback\0--pairing-state\0file\0',
                                b'capyio-node\0linux-playback\0--pairing-state\0file\0']
        patches = [mock.patch.object(Path, 'resolve', side_effect=binaries),
                   mock.patch('run_pipewire_secure_console.process_parent', side_effect=parent),
                   mock.patch('run_pipewire_secure_console.os.getpgid', side_effect=groups),
                   mock.patch.object(Path, 'read_bytes', side_effect=commands)]
        for patch in patches:
            patch.start()
            self.addCleanup(patch.stop)

    def test_actual_owned_fixed_sibling_chain(self):
        self.owner_patches()
        self.assertEqual(verify_console_owner(OWNERS), OWNERS)

    def test_console_must_be_fixture_direct_child(self):
        self.owner_patches(parent=(999, 150))
        with self.assertRaises(ValueError):
            verify_console_owner(OWNERS)

    def test_node_must_be_console_direct_child_not_fixture(self):
        self.owner_patches(parent=(100, 100))
        with self.assertRaises(ValueError):
            verify_console_owner(OWNERS)

    def test_named_binaries_from_different_directories_rejected(self):
        self.owner_patches(binaries=[Path('/a/capyio-linux-console'), Path('/b/capyio-node')])
        with self.assertRaises(ValueError):
            verify_console_owner(OWNERS)

    def test_fixture_binary_cannot_substitute_for_actual_node(self):
        self.owner_patches(binaries=[Path('/build/capyio-linux-console'), Path('/build/fixture')])
        with self.assertRaises(ValueError):
            verify_console_owner(OWNERS)

    def test_console_must_lead_owned_group(self):
        self.owner_patches(groups=(99, 150))
        with self.assertRaises(ValueError):
            verify_console_owner(OWNERS)

    def test_node_cannot_escape_owned_group(self):
        self.owner_patches(groups=(150, 200))
        with self.assertRaises(ValueError):
            verify_console_owner(OWNERS)

    def test_direction_and_protected_store_required(self):
        self.owner_patches(commands=[b'capyio-linux-console\0playback\0--pairing-state\0file',
                                     b'capyio-node\0linux-capture\0--pairing-state\0file'])
        with self.assertRaises(ValueError):
            verify_console_owner(OWNERS)

    def test_raw_config_cannot_substitute_for_protected_store(self):
        self.owner_patches(commands=[b'capyio-linux-console\0playback\0--pairing-state\0file',
                                     b'capyio-node\0linux-playback\0--raw-key\0file'])
        with self.assertRaises(ValueError):
            verify_console_owner(OWNERS)

    def test_cleanup_checks_node_console_helper_not_private_bus_fixture(self):
        checked = []
        def exists(path):
            checked.append(path)
            return path == Path('/proc/100')  # Live fixture is expected here.
        with mock.patch.object(Path, 'exists', exists):
            verify_reaped(OWNERS, bound('stop'))
        self.assertEqual(set(checked), {Path('/proc/150'), Path('/proc/200'), Path('/proc/201')})

    def test_any_unreaped_owned_process_fails(self):
        for pid in (150, 200, 201):
            with self.subTest(pid=pid), mock.patch.object(Path, 'exists', lambda path: path == Path('/proc', str(pid))):
                with self.assertRaises(ValueError):
                    verify_reaped(OWNERS, bound('stop'))

    def test_negative_cleanup_checks_console_node_without_inventing_helper(self):
        checked = []
        with mock.patch.object(Path, 'exists', lambda path: checked.append(path) or False):
            verify_reaped(OWNERS, None)
        self.assertEqual(set(checked), {Path('/proc/150'), Path('/proc/200')})

    def harness(self, root):
        h = SecureConsoleHarness(Path(root))
        h.fixture_pid = OWNERS['fixture_pid']
        h.owners = dict(OWNERS)
        h.console_chain_verified = True
        return h

    def test_emergency_teardown_uses_start_marker_before_fixture_cleanup(self):
        with tempfile.TemporaryDirectory() as root:
            h = self.harness(root)
            (h.root / '.capyio-console-start.json').write_text('{"console_pid":150}')
            with mock.patch.object(Path, 'exists', return_value=True), \
                    mock.patch.object(Path, 'resolve', return_value=Path('/build/capyio-linux-console')), \
                    mock.patch('run_pipewire_secure_console.process_parent', return_value=100), \
                    mock.patch('run_pipewire_secure_console.os.getpgid', return_value=150), \
                    mock.patch('run_pipewire_secure_console.os.killpg', side_effect=[None, ProcessLookupError]) as kill:
                h.cleanup_console()
            self.assertEqual(kill.call_args_list, [mock.call(150, signal.SIGTERM), mock.call(150, 0)])

    def test_emergency_teardown_rejects_changed_marker_or_wrong_binary(self):
        with tempfile.TemporaryDirectory() as root:
            h = self.harness(root)
            for text in ('{"console_pid":151}', '{"console_pid":true}',
                         '{"console_pid":150,"extra":true}',
                         '{"console_pid":150,"console_pid":150}'):
                (h.root / '.capyio-console-start.json').write_text(text)
                with mock.patch('run_pipewire_secure_console.os.killpg') as kill:
                    with self.subTest(text=text), self.assertRaises(ValueError):
                        h.cleanup_console()
                    kill.assert_not_called()
            (h.root / '.capyio-console-start.json').write_text('{"console_pid":150}')
            with mock.patch.object(Path, 'exists', return_value=True), \
                    mock.patch.object(Path, 'resolve', return_value=Path('/bin/unrelated')), \
                    mock.patch('run_pipewire_secure_console.os.killpg') as kill:
                with self.assertRaises(ValueError):
                    h.cleanup_console()
                kill.assert_not_called()

    def test_outer_cleanup_still_runs_when_console_cleanup_fails(self):
        with tempfile.TemporaryDirectory() as root:
            h = self.harness(root)
            with mock.patch.object(h, 'cleanup_console', side_effect=ValueError('bad identity')), \
                    mock.patch('run_pipewire_secure_console.Harness.close') as close:
                with self.assertRaises(ValueError):
                    h.close()
                close.assert_called_once()

    def test_group_cleanup_runs_even_after_verified_console_leader_exits(self):
        with tempfile.TemporaryDirectory() as root:
            h = self.harness(root)
            (h.root / '.capyio-console-start.json').write_text('{"console_pid":150}')
            with mock.patch.object(Path, 'exists', return_value=False), \
                    mock.patch('run_pipewire_secure_console.os.killpg', side_effect=[None, ProcessLookupError]) as kill:
                h.cleanup_console()
            self.assertEqual(kill.call_args_list, [mock.call(150, signal.SIGTERM), mock.call(150, 0)])

    def test_unverified_stale_marker_cannot_signal_unrelated_group(self):
        with tempfile.TemporaryDirectory() as root:
            h = self.harness(root)
            h.console_chain_verified = False
            (h.root / '.capyio-console-start.json').write_text('{"console_pid":150}')
            with mock.patch.object(Path, 'exists', return_value=False), \
                    mock.patch('run_pipewire_secure_console.os.killpg') as kill:
                h.cleanup_console()
                kill.assert_not_called()


if __name__ == '__main__':
    unittest.main()
