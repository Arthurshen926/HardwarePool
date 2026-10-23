"""Pure secure-stereo evidence/PCM tests: no process, socket, private bus or device."""
import copy
import json
from pathlib import Path
import struct
import unittest
from unittest.mock import patch

from run_pipewire_capture import signal_payload
from run_pipewire_secure_stereo import (
    ASSIGNED_TEST, ASSIGNED_JOB_TEST, BOUND_FIELDS, CAUSES, FORMAT, SCENARIOS, TEST, SecureStereoHarness, integer,
    strict_json, validate_bound, validate_pending, validate_pcm_enabled, validate_result, verify_execution,
    verify_stereo,
    JOB_SCENARIOS, JOB_TEST, JOB_PREFIX, JOB_MARKER, JOB_MARKER_CONTENT,
    JOB_AUTHORITY, JOB_COUNTERS, JOB_TRUE_FIELDS, SecureStereoJobHarness,
    validate_job_pending, validate_job_bound, validate_job_pcm_enabled,
    validate_job_result, verify_job_execution,
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


def job_common(scenario, stage):
    return common(scenario, stage) | {'surface': 'shared_runtime_receive_job'}


def job_bound(scenario):
    return bound(scenario) | job_common(scenario, 'bound') | {
        'control_generation': 17, 'job_generation': 23, 'route_state': 'Starting'}


def job_result(scenario):
    records = 600
    partial = scenario == 'partial_ack'
    return job_bound(scenario) | {'stage': 'stopped', 'terminal_state': 'Stopped',
        'stopped_at_ms': 8000, 'packets_captured': records + 2,
        'capture_bytes_read': (records + 2) * 1920, 'source_records_sent': records + 1,
        'records_admitted': records, 'records_accepted': records,
        'pcm_bytes_accepted': records * 1920, 'actual_sink_bytes': records * 1920,
        'sink_write_calls': records * (2 if partial else 1),
        'read_permits': records * (2 if partial else 1),
        'pcm_bytes_copied': records * (2880 if partial else 1920),
        'acknowledgements': records * (2 if partial else 1), 'busy_ack_retries': 0,
        'partial_writes': records if partial else 0,
    } | dict.fromkeys(JOB_TRUE_FIELDS, True)


class SharedStereoReceiveJobEvidenceTests(unittest.TestCase):
    def test_bridge_has_distinct_opt_in_test_surface_and_markers(self):
        self.assertEqual(JOB_SCENARIOS, ('waveform', 'partial_ack'))
        self.assertNotEqual(JOB_TEST, TEST)
        self.assertEqual(JOB_PREFIX, '.capyio-secure-stereo-job-')
        self.assertEqual(JOB_MARKER, '.capyio-secure-stereo-job')
        self.assertEqual(JOB_MARKER_CONTENT, 'capyio-hosted-secure-stereo-job-v1\n')
        with patch('run_pipewire_secure_stereo.CaptureHarness.__init__',
                   lambda instance, root, scenario: setattr(instance, 'env', {})):
            for scenario in JOB_SCENARIOS:
                harness = SecureStereoJobHarness(Path('/unused-private-root'), scenario)
                self.assertEqual(harness.bridge_scenario, scenario)
                self.assertEqual(harness.env, {'CAPYIO_SECURE_STEREO_JOB_HOSTED': '1',
                    'CAPYIO_SECURE_STEREO_JOB_SCENARIO': scenario})
            for scenario in ('partial_write', 'stop', 'legacy_udp'):
                with self.subTest(scenario=scenario), self.assertRaises(ValueError):
                    SecureStereoJobHarness(Path('/unused-private-root'), scenario)

    def test_pending_and_bound_forbid_early_pcm_or_unrelated_generation(self):
        for scenario in JOB_SCENARIOS:
            for stage in ('before_source', 'before_sink'):
                value = job_common(scenario, stage) | {'helper_absent': True,
                    'lease_count': 0, 'pcm_bytes_accepted': 0}
                validate_job_pending(value, scenario, stage, 200)
                for key, replacement in [('surface', 'reusable_rust_apis'), ('helper_absent', False),
                                          ('lease_count', 1), ('pcm_bytes_accepted', 4), ('version', True)]:
                    with self.subTest(key=key), self.assertRaises(ValueError):
                        validate_job_pending(value | {key: replacement}, scenario, stage, 200)
            validate_job_bound(job_bound(scenario), scenario, 200)
            for key, replacement in [('control_generation', 0), ('job_generation', False),
                    ('route_state', 'Active'), ('helper_pid', 200), ('source_consent_end_ms', 8999),
                    ('sink_lease_expires_at_ms', 15001), ('surface', 'reusable_rust_apis')]:
                with self.subTest(key=key), self.assertRaises(ValueError):
                    validate_job_bound(job_bound(scenario) | {key: replacement}, scenario, 200)
            enabled = job_common(scenario, 'pcm_enabled') | {'helper_pid': 201,
                'records_admitted': 0, 'pcm_bytes_accepted': 0, 'job_generation': 23}
            validate_job_pcm_enabled(enabled, scenario, 200, job_bound(scenario))
            for key, replacement in [('job_generation', 24), ('helper_pid', 202),
                                     ('records_admitted', 1), ('pcm_bytes_accepted', 4)]:
                with self.subTest(key=key), self.assertRaises(ValueError):
                    validate_job_pcm_enabled(enabled | {key: replacement}, scenario, 200, job_bound(scenario))

    def test_exact_whole_and_partial_ack_results_require_all_fields_and_cleanup(self):
        for scenario in JOB_SCENARIOS:
            value = job_result(scenario)
            validate_job_result(value, scenario, 200, job_bound(scenario))
            for key in value:
                bad = dict(value)
                del bad[key]
                with self.subTest(scenario=scenario, missing=key), self.assertRaises((ValueError, KeyError)):
                    validate_job_result(bad, scenario, 200, job_bound(scenario))
            for key in JOB_TRUE_FIELDS:
                with self.subTest(key=key), self.assertRaises(ValueError):
                    validate_job_result(value | {key: False}, scenario, 200, job_bound(scenario))
            for key in JOB_COUNTERS:
                with self.subTest(key=key), self.assertRaises(ValueError):
                    validate_job_result(value | {key: True}, scenario, 200, job_bound(scenario))
            for key in BOUND_FIELDS | (JOB_AUTHORITY - {'route_state'}):
                with self.subTest(key=key), self.assertRaises(ValueError):
                    validate_job_result(value | {key: value[key] + 1}, scenario, 200, job_bound(scenario))
            for key, replacement in [('extra', True), ('route_state', 'Active'),
                    ('terminal_state', 'Failed'), ('stopped_at_ms', 8900),
                    ('terminal_cause', 'media_timeout'), ('surface', 'reusable_rust_apis')]:
                with self.subTest(key=key), self.assertRaises(ValueError):
                    validate_job_result(value | {key: replacement}, scenario, 200, job_bound(scenario))

    def test_copies_or_submitted_acks_never_substitute_for_actual_confirmed_writes(self):
        for scenario in JOB_SCENARIOS:
            value = job_result(scenario)
            for key in ('pcm_bytes_accepted', 'actual_sink_bytes', 'records_accepted',
                        'sink_write_calls', 'read_permits', 'pcm_bytes_copied', 'acknowledgements',
                        'partial_writes'):
                with self.subTest(scenario=scenario, key=key), self.assertRaises(ValueError):
                    validate_job_result(value | {key: value[key] + 4}, scenario, 200, job_bound(scenario))
            # Busy ACK handling is valid with or without observed contention;
            # no retry creates another write, read permit, or successful ACK.
            validate_job_result(value | {'busy_ack_retries': 3}, scenario, 200, job_bound(scenario))
            for key, replacement in [('records_accepted', 599), ('records_admitted', 599),
                    ('source_records_sent', 598), ('source_records_sent', 603),
                    ('capture_bytes_read', 602 * 1920 - 1), ('capture_bytes_read', 603 * 1920)]:
                with self.subTest(key=key), self.assertRaises(ValueError):
                    validate_job_result(value | {key: replacement}, scenario, 200, job_bound(scenario))

    def test_reusable_owner_results_and_wrong_or_skipped_execution_cannot_pass(self):
        for scenario in JOB_SCENARIOS:
            with self.assertRaises(ValueError):
                validate_result(job_result(scenario), scenario, 200, job_bound(scenario))
        with self.assertRaises(ValueError):
            validate_job_result(result('waveform'), 'waveform', 200, bound('waveform'))
        good = f'test {JOB_TEST} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored;'
        verify_job_execution(good)
        for raw in ('', good.replace(JOB_TEST, TEST), good.replace('... ok', '... ignored'),
                    good.replace('1 passed', '0 passed'), good.replace('0 ignored', '1 ignored')):
            with self.subTest(raw=raw), self.assertRaises(ValueError):
                verify_job_execution(raw)


class ReceiverAssignedExecutionTests(unittest.TestCase):
    def test_distinct_exact_cases_must_execute(self):
        for verifier, old, assigned in (
                (verify_execution, TEST, ASSIGNED_TEST),
                (verify_job_execution, JOB_TEST, ASSIGNED_JOB_TEST)):
            good = f'test {assigned} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored;'
            verifier(good, True)
            verifier(good.replace(assigned, old))
            for bad in ('', good.replace(assigned, old),
                        good.replace('... ok', '... ignored'),
                        good.replace('1 passed', '0 passed'),
                        good.replace('0 ignored', '1 ignored')):
                with self.subTest(bad=bad), self.assertRaises(ValueError):
                    verifier(bad, True)
            with self.assertRaises(ValueError):
                verifier(good)

    def test_assigned_and_strict_harness_modes_remain_separate(self):
        for harness_type in (SecureStereoHarness, SecureStereoJobHarness):
            with patch('run_pipewire_capture.CaptureHarness.__init__', return_value=None):
                with patch.object(harness_type, 'env', {}, create=True):
                    self.assertFalse(harness_type(Path('/tmp/unused'), 'waveform').receiver_assigned)
                    self.assertTrue(harness_type(Path('/tmp/unused'), 'waveform', True).receiver_assigned)


if __name__ == '__main__':
    unittest.main()
