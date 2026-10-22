"""Pure secure-stereo evidence/PCM tests: no process, socket, private bus or device."""
import copy
import json
from pathlib import Path
import struct
import unittest
from unittest.mock import patch

from run_pipewire_capture import signal_payload
from run_pipewire_secure_stereo import (
    BOUND_FIELDS, CAUSES, FORMAT, SCENARIOS, TEST, SecureStereoHarness, integer,
    strict_json, validate_bound, validate_pending, validate_pcm_enabled, validate_result, verify_execution,
    verify_stereo,
)


def common(scenario, stage):
    return {'version': 1, 'scenario': scenario, 'stage': stage, 'format': FORMAT,
            'surface': 'reusable_rust_apis', 'owner_pid': 200}


def bound(scenario):
    return common(scenario, 'bound') | {
        'helper_pid': 201, 'source_lease_expires_at_ms': 9000, 'sink_lease_expires_at_ms': 8900,
        'source_consent_end_ms': 9300, 'sink_consent_end_ms': 9400,
        'grant_expires_at_ms': 15000,
    }


def result(scenario):
    counts = 600 if scenario in ('waveform', 'partial_write') else 100
    return bound(scenario) | {'stage': 'stopped',
        'terminal_endpoint': CAUSES[scenario][0], 'terminal_cause': CAUSES[scenario][1],
        'terminal_observed_at_ms': 8900 if scenario == 'expiry' else 8000,
        'injected_at_ms': 7900, 'packets_captured': counts + 2,
        'capture_bytes_read': (counts + 2) * 1920, 'source_records_sent': counts + 1,
        'records_admitted': counts + int(scenario == 'backpressure'),
        'records_accepted': counts, 'pcm_bytes_accepted': counts * 1920,
        'sink_write_calls': 2 * counts if scenario == 'partial_write' else counts + 2,
        'source_session_closed': True, 'sink_session_closed': True, 'leases_released': True,
        'helper_reaped': True, 'sink_stopped': True, 'post_stop_unchanged': True,
    }


class SecureStereoEvidenceTests(unittest.TestCase):
    def test_required_profiles_are_distinct_and_exhaustive(self):
        self.assertEqual(set(SCENARIOS), {'waveform', 'stop', 'source_revoke', 'sink_revoke',
            'control_eof', 'media_eof', 'expiry', 'backpressure', 'partial_write'})
        self.assertEqual(set(CAUSES), set(SCENARIOS))
        self.assertEqual(CAUSES['stop'], ('sink', 'peer_stop'))
        self.assertNotEqual(CAUSES['control_eof'], CAUSES['media_eof'])

    def test_json_is_bounded_unique_and_object_only(self):
        self.assertEqual(strict_json(b'{"version":1}'), {'version': 1})
        for raw in (b'[]', b'null', b'{"a":1,"a":2}', b' ' * 8193, b'{', b'{"a":{ "b":1,"b":2}}'):
            with self.subTest(raw=raw[:40]), self.assertRaises(ValueError):
                strict_json(raw)
        for value in (False, True, -1, 2.0, '3', 1 << 63, None):
            with self.subTest(value=value), self.assertRaises(ValueError):
                integer(value)

    def test_both_independent_pending_checkpoints_forbid_helper_leases_pcm(self):
        for stage in ('before_source', 'before_sink'):
            value = common('waveform', stage) | {'helper_absent': True, 'lease_count': 0,
                                                'pcm_bytes_accepted': 0}
            validate_pending(value, 'waveform', stage, 200)
            for field, replacement in [('helper_absent', False), ('lease_count', 1),
                                       ('pcm_bytes_accepted', 1), ('owner_pid', 201),
                                       ('version', True), ('lease_count', False),
                                       ('surface', 'actual_android_audiotrack'), ('format', 'legacy_udp')]:
                with self.subTest(stage=stage, field=field), self.assertRaises(ValueError):
                    validate_pending(value | {field: replacement}, 'waveform', stage, 200)
            with self.assertRaises(ValueError):
                validate_pending(value | {'unexpected': True}, 'waveform', stage, 200)

    def test_first_read_is_enabled_before_links_flow_without_prior_pcm(self):
        value = common('waveform', 'pcm_enabled') | {'helper_pid': 201,
            'records_admitted': 0, 'pcm_bytes_accepted': 0}
        validate_pcm_enabled(value, 'waveform', 200, bound('waveform'))
        for field, item in [('helper_pid', 202), ('records_admitted', 1),
                            ('pcm_bytes_accepted', 4), ('records_admitted', False),
                            ('stage', 'bound'), ('unexpected', True)]:
            with self.subTest(field=field), self.assertRaises(ValueError):
                validate_pcm_enabled(value | {field: item}, 'waveform', 200, bound('waveform'))

    def test_all_scenarios_require_exact_shape_and_successful_cleanup(self):
        for scenario in SCENARIOS:
            with self.subTest(scenario=scenario):
                validate_bound(bound(scenario), scenario, 200)
                validate_result(result(scenario), scenario, 200, bound(scenario))
        value = result('stop')
        for field in value:
            bad = copy.deepcopy(value)
            del bad[field]
            with self.subTest(missing=field), self.assertRaises((ValueError, KeyError)):
                validate_result(bad, 'stop', 200, bound('stop'))
        with self.assertRaises(ValueError):
            validate_result(value | {'extra': True}, 'stop', 200, bound('stop'))

    def test_pinned_helper_and_every_authority_cap_remain_exact(self):
        for field in BOUND_FIELDS:
            with self.subTest(field=field), self.assertRaises(ValueError):
                validate_result(result('stop') | {field: result('stop')[field] + 1},
                                'stop', 200, bound('stop'))
        for field in ('source_consent_end_ms', 'sink_consent_end_ms', 'grant_expires_at_ms'):
            with self.subTest(field=field), self.assertRaises(ValueError):
                validate_bound(bound('waveform') | {field: 8999}, 'waveform', 200)
        for field in ('source_lease_expires_at_ms', 'sink_lease_expires_at_ms'):
            with self.subTest(field=field), self.assertRaises(ValueError):
                validate_bound(bound('waveform') | {field: 9301}, 'waveform', 200)
        with self.assertRaises(ValueError):
            validate_bound(bound('waveform') | {'helper_pid': 200}, 'waveform', 200)

    def test_earliest_of_both_original_claims_drives_exact_expiry(self):
        # The Sink claim is earlier here, unlike the historical mono fixture.
        validate_result(result('expiry'), 'expiry', 200, bound('expiry'))
        for observed in (0, 8890, 8899):
            with self.subTest(observed=observed), self.assertRaises(ValueError):
                validate_result(result('expiry') | {'terminal_observed_at_ms': observed},
                                'expiry', 200, bound('expiry'))
        other = bound('expiry') | {'source_lease_expires_at_ms': 8800}
        current = result('expiry') | {'source_lease_expires_at_ms': 8800, 'terminal_observed_at_ms': 8800}
        validate_result(current, 'expiry', 200, other)
        with self.assertRaises(ValueError):
            validate_result(current | {'terminal_observed_at_ms': 8799}, 'expiry', 200, other)
        with self.assertRaises(ValueError):
            validate_bound(bound('expiry') | {'grant_expires_at_ms': 9000}, 'expiry', 200)

    def test_unrelated_failure_or_late_expiry_cannot_substitute_for_injected_cause(self):
        for scenario in SCENARIOS:
            for field, value in [('terminal_cause', 'failure'), ('terminal_cause', 'media_timeout'),
                                 ('terminal_endpoint', 'unverified'), ('terminal_observed_at_ms', 7899)]:
                with self.subTest(scenario=scenario, field=field), self.assertRaises(ValueError):
                    validate_result(result(scenario) | {field: value}, scenario, 200, bound(scenario))
            if scenario != 'expiry':
                with self.subTest(scenario=scenario), self.assertRaises(ValueError):
                    validate_result(result(scenario) | {'terminal_observed_at_ms': 8900},
                                    scenario, 200, bound(scenario))

    def test_actual_counters_not_queued_or_audibility_claims(self):
        for field, value in [('records_accepted', 0), ('records_admitted', 99),
                             ('source_records_sent', 98), ('source_records_sent', 103),
                             ('capture_bytes_read', 102 * 1920 - 1),
                             ('capture_bytes_read', 103 * 1920), ('pcm_bytes_accepted', 100 * 1920 - 1),
                             ('pcm_bytes_accepted', 100 * 1920 + 1), ('sink_write_calls', 99),
                             ('source_session_closed', False), ('sink_session_closed', False),
                             ('leases_released', False), ('helper_reaped', False),
                             ('sink_stopped', False), ('post_stop_unchanged', False)]:
            with self.subTest(field=field), self.assertRaises(ValueError):
                validate_result(result('stop') | {field: value}, 'stop', 200, bound('stop'))
        with self.assertRaises(ValueError):
            validate_result(result('partial_write') | {'sink_write_calls': 1199},
                            'partial_write', 200, bound('partial_write'))
        with self.assertRaises(ValueError):
            validate_result(result('backpressure') | {'records_admitted': 100},
                            'backpressure', 200, bound('backpressure'))
        with self.assertRaises(ValueError):
            validate_result(result('backpressure') | {'records_admitted': 102},
                            'backpressure', 200, bound('backpressure'))

    def test_exact_test_execution_cannot_be_compilation_listing_or_ignored(self):
        good = f'test {TEST} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored;'
        verify_execution(good)
        for text in ('', f'{TEST}: test', good.replace('... ok', '... ignored'),
                     good.replace('1 passed', '0 passed'), good.replace('0 ignored', '1 ignored')):
            with self.subTest(text=text), self.assertRaises(ValueError):
                verify_execution(text)

    def test_harness_uses_private_capture_primitives_but_distinct_opt_in(self):
        with patch('run_pipewire_secure_stereo.CaptureHarness.__init__',
                   lambda instance, root, scenario: setattr(instance, 'env', {})):
            for scenario in SCENARIOS:
                harness = SecureStereoHarness(Path('/unused-private-root'), scenario)
                self.assertEqual(harness.secure_scenario, scenario)
                self.assertEqual(harness.env, {'CAPYIO_SECURE_STEREO_HOSTED': '1',
                                              'CAPYIO_SECURE_STEREO_SCENARIO': scenario})
            with self.assertRaises(ValueError):
                SecureStereoHarness(Path('/unused-private-root'), 'legacy_udp')


class IndependentStereoOracleTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.signal = signal_payload()

    def test_complete_generated_tone_silence_second_tone_is_independently_decoded(self):
        verify_stereo(bytes(480 * 4) + self.signal + bytes(480 * 4))

    def test_channel_swap_mono_drop_corrupt_reorder_and_incomplete_fail(self):
        pairs = list(struct.iter_unpack('<hh', self.signal))
        swapped = b''.join(struct.pack('<hh', right, left) for left, right in pairs)
        mono = b''.join(struct.pack('<hh', left, left) for left, _ in pairs)
        corruption = bytearray(self.signal)
        corruption[4000] ^= 1
        for raw in (swapped, mono, self.signal[4:], bytes(corruption),
                    self.signal[3 * 48000 * 4:] + self.signal[:3 * 48000 * 4],
                    bytes(len(self.signal)), b'\0\0' + self.signal + b'\0\0'):
            with self.subTest(length=len(raw)), self.assertRaises(ValueError):
                verify_stereo(raw)


if __name__ == '__main__':
    unittest.main()
