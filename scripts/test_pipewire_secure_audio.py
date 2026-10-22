"""Pure evidence/parser checks only. Never opens sockets, processes or audio."""
import copy
import json
import unittest

from run_pipewire_secure_audio import (
    FORMAT, SCENARIOS, NEGATIVE_SCENARIOS, TEST, strict_json, validate_bound,
    validate_pending, validate_result, verify_execution, no_playback,
)


def common(scenario, stage):
    return {'version': 1, 'scenario': scenario, 'stage': stage, 'format': FORMAT,
            'surface': 'reusable_rust_apis', 'owner_pid': 200}


def bound(scenario):
    return common(scenario, 'bound') | {'helper_pid': 201, 'source_lease_expires_at_ms': 9000}


def result(scenario):
    negative = scenario in NEGATIVE_SCENARIOS
    return common(scenario, 'stopped') | {
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
    }


class SecureAudioEvidenceTests(unittest.TestCase):
    def test_required_profiles_cannot_silently_disappear(self):
        self.assertEqual(set(SCENARIOS), {'waveform', 'stop', 'source_revoke', 'sink_revoke',
                                         'expiry', 'control_eof', 'media_eof',
                                         'no_authority', 'peer_denied'})

    def test_bounded_unique_object_json(self):
        self.assertEqual(strict_json(b'{"version":1}'), {'version': 1})
        for raw in (b'[]', b'null', b'{"a":1,"a":1}', b' ' * 4097, b'{'):
            with self.subTest(raw=raw[:40]), self.assertRaises(ValueError):
                strict_json(raw)

    def test_pending_requires_no_helper_no_lease_no_pcm(self):
        for stage in ('before_local', 'before_source'):
            value = common('waveform', stage) | {
                'helper_absent': True, 'lease_count': 0, 'bytes_written': 0}
            validate_pending(value, 'waveform', stage, 200)
            for field, replacement in [('helper_absent', False), ('lease_count', 1),
                                       ('bytes_written', 1), ('owner_pid', 202),
                                       ('version', True), ('lease_count', False),
                                       ('surface', 'actual_node_cli')]:
                bad = value | {field: replacement}
                with self.subTest(stage=stage, field=field), self.assertRaises(ValueError):
                    validate_pending(bad, 'waveform', stage, 200)

    def test_all_exact_results_and_profiles_accepted(self):
        for scenario in SCENARIOS:
            with self.subTest(scenario=scenario):
                validate_result(result(scenario), scenario, 200,
                                None if scenario in NEGATIVE_SCENARIOS else bound(scenario))

    def test_each_final_field_mandatory_no_extra_fields(self):
        value = result('stop')
        for field in value:
            bad = copy.deepcopy(value)
            del bad[field]
            with self.subTest(field=field), self.assertRaises((ValueError, KeyError)):
                validate_result(bad, 'stop', 200, bound('stop'))
        with self.assertRaises(ValueError):
            validate_result(value | {'extra': True}, 'stop', 200, bound('stop'))

    def test_terminal_failure_cannot_substitute_for_requested_cause(self):
        for field, replacement in [('injected', 'other'), ('verified', 'other'),
                                   ('helper_reaped', False), ('leases_released', False),
                                   ('session_closed', False), ('packets_dropped_busy', 1),
                                   ('bytes_written', 300 * 960 - 1), ('packets_written', 299),
                                   ('source_records_sent', 299), ('source_records_attempted', 300), ('format', 'pcm-s16le-48000-mono')]:
            with self.subTest(field=field), self.assertRaises(ValueError):
                validate_result(result('stop') | {field: replacement}, 'stop', 200, bound('stop'))

    def test_post_write_expiry_uses_exact_attempted_count(self):
        value = result('expiry') | {'source_records_sent': 300,
                                   'source_records_attempted': 301,
                                   'packets_written': 301, 'bytes_written': 301 * 960}
        validate_result(value, 'expiry', 200, bound('expiry'))
        for attempted in (300, 302):
            with self.subTest(attempted=attempted), self.assertRaises(ValueError):
                validate_result(value | {'source_records_attempted': attempted},
                                'expiry', 200, bound('expiry'))

    def test_exact_expiry_has_no_early_allowance(self):
        for observed in (0, 8990, 8999):
            with self.subTest(observed=observed), self.assertRaises(ValueError):
                validate_result(result('expiry') | {'terminal_observed_at_ms': observed},
                                'expiry', 200, bound('expiry'))
        for observed in (9000, 9001):
            validate_result(result('expiry') | {'terminal_observed_at_ms': observed},
                            'expiry', 200, bound('expiry'))

    def test_late_other_failures_do_not_count_as_expiry_or_stop(self):
        for scenario in set(SCENARIOS) - NEGATIVE_SCENARIOS - {'expiry'}:
            with self.subTest(scenario=scenario), self.assertRaises(ValueError):
                validate_result(result(scenario) | {'terminal_observed_at_ms': 9000},
                                scenario, 200, bound(scenario))

    def test_deadline_pinned_before_cleanup_cannot_be_replaced(self):
        for scenario in ('expiry', 'stop'):
            for field in ('helper_pid', 'source_lease_expires_at_ms'):
                bad = result(scenario)
                bad[field] += 1
                with self.subTest(scenario=scenario, field=field), self.assertRaises(ValueError):
                    validate_result(bad, scenario, 200, bound(scenario))
        with self.assertRaises(ValueError):
            validate_result(result('expiry'), 'expiry', 200)

    def test_negative_profiles_never_accept_activation(self):
        for scenario in NEGATIVE_SCENARIOS:
            with self.subTest(scenario=scenario), self.assertRaises(ValueError):
                validate_bound(bound(scenario), scenario, 200)
            for field, value in [('helper_pid', 201), ('source_lease_expires_at_ms', 9000),
                                 ('bytes_written', 960), ('packets_written', 1),
                                 ('source_records_sent', 1), ('source_records_attempted', 1)]:
                with self.subTest(field=field), self.assertRaises(ValueError):
                    validate_result(result(scenario) | {field: value}, scenario, 200)

    def test_bound_rejects_wrong_owner_bad_number_and_alias(self):
        for field, value in [('owner_pid', 201), ('helper_pid', 200), ('helper_pid', True),
                             ('helper_pid', '201'), ('source_lease_expires_at_ms', 0),
                             ('source_lease_expires_at_ms', -1), ('scenario', 'other')]:
            with self.subTest(field=field), self.assertRaises(ValueError):
                validate_bound(bound('stop') | {field: value}, 'stop', 200)

    def test_no_skip_exact_test_execution(self):
        valid = f'test {TEST} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored;'
        verify_execution(valid)
        for text in ('', valid.replace('1 passed', '0 passed'),
                     valid.replace('0 ignored', '1 ignored'), valid.replace(TEST, 'different_test'),
                     valid.replace('... ok', '... ignored')):
            with self.subTest(text=text), self.assertRaises(ValueError):
                verify_execution(text)

    def test_graph_activation_negative_is_independent(self):
        no_playback([])
        no_playback([{'info': {'props': {'node.name': 'capyio.test.null'}}}])
        with self.assertRaises(ValueError):
            no_playback([{'info': {'props': {'target.object': 'capyio.test.null'}}}])


if __name__ == '__main__':
    unittest.main()
