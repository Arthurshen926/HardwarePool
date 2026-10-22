"""Pure long-profile evidence tests. No sockets, PTYs, buses or audio devices."""
import array
import math
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

import run_pipewire_secure_console as short
import run_pipewire_secure_console_long as long
from test_pipewire_secure_console import OWNERS, common, pending, bound, result


def owners(scenario):
    if scenario == 'stop':
        return dict(OWNERS)
    port = '00000000-0000-0000-0000-000000000001'
    return dict(OWNERS) | {'endpoint_mode': 'virtual_microphone', 'sink_port_id': port,
                           'node_name': 'capyio.virtual-microphone.' + port}


def evidence(scenario, stage):
    value = (pending(scenario, stage) if stage in ('before_local', 'before_source')
             else bound(scenario))
    value.update(format=long.FORMAT, stage=stage)
    value.update({key: item for key, item in owners(scenario).items() if key not in OWNERS})
    if stage not in ('before_local', 'before_source'):
        value['source_lease_expires_at_ms'] = 300000
        value['authority'] = {
            'requested_seconds': 300, 'stream_id': 'stream-1', 'epoch': 1,
            'route': {'id': 'route-1', 'session_id': 'session-1', 'source': {'port_id': 'source'},
                      'selected_format': {'id': long.FORMAT, 'parameters': {}}},
            'source_lease': {'port': {'port_id': 'source'}, 'route_id': 'route-1',
                             'session_id': 'session-1', 'expires_at_ms': 300000}}
    return value


def terminal(scenario):
    return (result(scenario) | evidence(scenario, 'stopped') | {
        'packets_written': 29000 if scenario == 'expiry' else 7300,
        'bytes_written': (29000 if scenario == 'expiry' else 7300) * 960,
        'source_records_sent': 29000 if scenario == 'expiry' else 7300,
        'source_records_attempted': 29001 if scenario == 'expiry' else 7300,
        'terminal_observed_at_ms': 300000 if scenario == 'expiry' else 76000,
        'terminal_elapsed_ms': 299000 if scenario == 'expiry' else 75000})


def window(scenario, name):
    elapsed = {'initial': 100, 'late': 65000, 'final': 293000}[name]
    return evidence(scenario, name) | {'elapsed_ms': elapsed,
        'window_observed_at_ms': 294000 if name == 'final' else elapsed + 1000}


def waveform(name):
    first, second = long.WINDOW_TONES[name]
    samples = array.array('h')
    for index in range(long.WINDOW_BYTES // 2):
        seconds = index / 48000
        hz = first if .2 <= seconds < 1.4 else second if 2 <= seconds < 3.2 else 0
        samples.append(round(8192 * math.sin(2 * math.pi * hz * index / 48000)))
    if sys.byteorder != 'little':
        samples.byteswap()
    return samples.tobytes()


class LongEvidenceTests(unittest.TestCase):
    def test_only_two_explicit_long_modes(self):
        long.profile('stop', 'playback')
        long.profile('expiry', 'virtual_microphone')
        for scenario, mode in [('stop', 'virtual_microphone'), ('expiry', 'playback'),
                               ('waveform', 'playback'), ('expiry', 'other')]:
            with self.subTest(scenario=scenario, mode=mode), self.assertRaises(ValueError):
                long.profile(scenario, mode)

    def test_old_constants_and_scenarios_unchanged(self):
        self.assertEqual(len(short.SCENARIOS), 11)
        self.assertEqual(short.FORMAT, 'pcm-s16le-48000-mono-tls13-v1')
        self.assertNotEqual(short.TEST, long.TEST)
        self.assertNotEqual(short.PREFIX, long.PREFIX)
        self.assertEqual(long.OUTER_SECONDS, 345)
        self.assertEqual(long.CAPTURE_LIMIT, 32 * 1024 * 1024)
        self.assertEqual(long.WINDOW_BYTES, 384000)

    def test_exact_approved_pending_bound_terminal(self):
        for scenario in long.SCENARIOS:
            own = owners(scenario)
            for stage in ('before_local', 'before_source'):
                long.validate_pending(evidence(scenario, stage), scenario, stage, own)
            long.validate_bound(evidence(scenario, 'bound'), scenario, own)
            long.validate_result(terminal(scenario), scenario, own, evidence(scenario, 'bound'))

    def test_no_authority_before_either_approval(self):
        for stage in ('before_local', 'before_source'):
            for change in ({'authority': {}}, {'bytes_written': 1}, {'lease_count': 1},
                           {'helper_absent': False}, {'format': short.FORMAT}):
                with self.subTest(stage=stage, change=change), self.assertRaises(ValueError):
                    long.validate_pending(evidence('stop', stage) | change, 'stop', stage, owners('stop'))

    def test_all_windows_require_same_original_authority(self):
        for scenario in long.SCENARIOS:
            for name in ('initial', 'late') + (('final',) if scenario == 'expiry' else ()):
                long.validate_window(window(scenario, name), scenario, name, owners(scenario), evidence(scenario, 'bound'))
                for key, bad in [('helper_pid', 202), ('source_lease_expires_at_ms', 300001),
                                 ('format', short.FORMAT), ('authority', {})]:
                    with self.subTest(scenario=scenario, name=name, key=key), self.assertRaises(ValueError):
                        long.validate_window(window(scenario, name) | {key: bad}, scenario, name,
                                             owners(scenario), evidence(scenario, 'bound'))

    def test_late_window_cannot_use_early_timestamp(self):
        for name, elapsed in [('late', 1000), ('final', 65000)]:
            with self.assertRaises(ValueError):
                long.validate_window(window('expiry', name) | {'elapsed_ms': elapsed}, 'expiry', name,
                                     owners('expiry'), evidence('expiry', 'bound'))

    def test_near_expiry_is_anchored_to_original_not_renewed_deadline(self):
        for timestamp in (1000, 293999, 298000, 300000, 300001):
            with self.subTest(timestamp=timestamp), self.assertRaises(ValueError):
                long.validate_window(window('expiry', 'final') | {'window_observed_at_ms': timestamp},
                                     'expiry', 'final', owners('expiry'), evidence('expiry', 'bound'))

    def test_exact_expiry_no_early_tolerance(self):
        for observed in (299999, 299000, 302001):
            with self.subTest(observed=observed), self.assertRaises(ValueError):
                long.validate_result(terminal('expiry') | {'terminal_observed_at_ms': observed},
                                     'expiry', owners('expiry'), evidence('expiry', 'bound'))

    def test_stop_cannot_be_incidental_expiry_or_failure(self):
        for change in ({'terminal_observed_at_ms': 300000}, {'terminal_cause': 'media_timeout'},
                       {'terminal_cause': 'failure'}):
            with self.assertRaises(ValueError):
                long.validate_result(terminal('stop') | change, 'stop', owners('stop'), evidence('stop', 'bound'))

    def test_no_short_counts_or_oversize_counts(self):
        for scenario in long.SCENARIOS:
            for packets in (300, 30001):
                value = terminal(scenario) | {'packets_written': packets, 'bytes_written': packets * 960,
                    'source_records_sent': packets, 'source_records_attempted': packets + int(scenario == 'expiry')}
                with self.subTest(scenario=scenario, packets=packets), self.assertRaises(ValueError):
                    long.validate_result(value, scenario, owners(scenario), evidence(scenario, 'bound'))

    def test_all_terminal_fields_required_no_extra(self):
        for key in terminal('stop'):
            value = terminal('stop')
            del value[key]
            with self.subTest(key=key), self.assertRaises((ValueError, KeyError)):
                long.validate_result(value, 'stop', owners('stop'), evidence('stop', 'bound'))
        with self.assertRaises(ValueError):
            long.validate_result(terminal('stop') | {'extra': True}, 'stop', owners('stop'), evidence('stop', 'bound'))

    def test_cleanup_requires_true_booleans(self):
        for key in short.CLEANUP_FIELDS | {'helper_reaped', 'session_closed', 'leases_released'}:
            for bad in (False, 1, None):
                with self.subTest(key=key, bad=bad), self.assertRaises(ValueError):
                    long.validate_result(terminal('stop') | {key: bad}, 'stop', owners('stop'), evidence('stop', 'bound'))

    def test_exact_profile_empty_parameters_and_full_original_lease(self):
        for key, bad in [('requested_seconds', 60), ('epoch', True), ('stream_id', ''),
                         ('source_lease', {}), ('route', {}), ('extra', True)]:
            value = evidence('stop', 'bound')
            value['authority'][key] = bad
            with self.subTest(key=key), self.assertRaises(ValueError):
                long.validate_bound(value, 'stop', owners('stop'))
        for descriptor in ({'id': short.FORMAT, 'parameters': {}},
                           {'id': long.FORMAT, 'parameters': {'fallback': 'true'}}):
            value = evidence('stop', 'bound')
            value['authority']['route']['selected_format'] = descriptor
            with self.assertRaises(ValueError):
                long.validate_bound(value, 'stop', owners('stop'))

    def test_exact_long_execution_no_short_substitution_or_skip(self):
        output = f'test {long.TEST} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored;\n'
        marker = long.execution_marker('stop', 'playback')
        long.verify_execution(output, marker, 'stop', 'playback')
        for text in (output.replace(long.TEST, short.TEST), output.replace('1 passed', '0 passed'), output + output):
            with self.assertRaises(ValueError):
                long.verify_execution(text, marker, 'stop', 'playback')
        with self.assertRaises(ValueError):
            long.verify_execution(output, short.execution_marker('stop'), 'stop', 'playback')

    def test_late_recording_needs_elapsed_time_and_new_file_offset(self):
        for name, seconds in [('late', 65), ('final', 290)]:
            long.verify_timeline(name, seconds, seconds * long.PCM_RATE)
            for elapsed, offset in [(1, seconds * long.PCM_RATE), (seconds, 0)]:
                with self.assertRaises(ValueError):
                    long.verify_timeline(name, elapsed, offset)

    def test_terminal_elapsed_cannot_claim_short_session(self):
        for scenario in long.SCENARIOS:
            with self.assertRaises(ValueError):
                long.validate_result(terminal(scenario) | {'terminal_elapsed_ms': 59000},
                                     scenario, owners(scenario), evidence(scenario, 'bound'))

    def test_final_markers_are_profile_and_mode_specific(self):
        self.assertIn('stop_after_75s=true', long.result_marker('stop', 'playback'))
        marker = long.result_marker('expiry', 'virtual_microphone')
        self.assertIn('original_expiry_observed=true', marker)
        self.assertIn('requested_seconds=300', marker)
        self.assertIn('profile=pinned_tls13_mono_300s_v1', marker)


class BoundedRecorderTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.waves = {name: waveform(name) for name in long.WINDOW_TONES}

    def test_all_distinct_ordered_windows_pass(self):
        for name, raw in self.waves.items():
            long.verify_window_signal(raw, name)

    def test_early_waveform_cannot_substitute_late_or_final(self):
        for name in ('late', 'final'):
            with self.subTest(name=name), self.assertRaises(ValueError):
                long.verify_window_signal(self.waves['initial'], name)

    def test_late_waveform_cannot_substitute_near_expiry(self):
        with self.assertRaises(ValueError):
            long.verify_window_signal(self.waves['late'], 'final')

    def test_truncated_oversize_and_unknown_window_rejected(self):
        for raw in (self.waves['late'][:-2], self.waves['late'] + b'\0\0', b''):
            with self.assertRaises(ValueError):
                long.verify_window_signal(raw, 'late')
        with self.assertRaises(ValueError):
            long.verify_window_signal(self.waves['late'], 'other')

    def test_window_reads_seek_without_accumulating_whole_capture(self):
        with tempfile.TemporaryDirectory() as root:
            path = Path(root) / 'capture.out'
            with path.open('wb') as stream:
                stream.seek(65 * long.PCM_RATE)
                stream.write(self.waves['late'])
            self.assertEqual(long.read_window(path, 65 * long.PCM_RATE, long.WINDOW_BYTES), self.waves['late'])
            with self.assertRaises(ValueError):
                long.read_window(path, 65 * long.PCM_RATE + 2, long.WINDOW_BYTES)

    def test_capture_size_cap_and_odd_output(self):
        with tempfile.TemporaryDirectory() as root:
            path = Path(root) / 'capture.out'
            for size in (long.CAPTURE_LIMIT + 2, long.WINDOW_BYTES + 1):
                with path.open('wb') as stream:
                    stream.truncate(size)
                with self.assertRaises(ValueError):
                    long.read_window(path, 0, long.WINDOW_BYTES)

    def test_only_capture_gets_separate_long_output_cap(self):
        with tempfile.TemporaryDirectory() as root:
            harness = long.LongConsoleHarness(Path(root), 'playback')
            capture = Path(root) / 'capture.out'
            with capture.open('wb') as stream:
                stream.truncate(long.LOG_LIMIT + 2)
            harness.check()
            log = Path(root) / 'producer.err'
            with log.open('wb') as stream:
                stream.truncate(long.LOG_LIMIT + 2)
            with self.assertRaises(ValueError):
                harness.check()
            log.unlink()
            with capture.open('wb') as stream:
                stream.truncate(long.CAPTURE_LIMIT + 2)
            with self.assertRaises(ValueError):
                harness.check()

    def test_invalid_offsets_and_sizes_fail_before_open(self):
        path = mock.Mock()
        for offset, size in [(True, 2), (-2, 2), (1, 2), (0, 0), (0, 3),
                             (0, long.WINDOW_BYTES + 2)]:
            with self.assertRaises(ValueError):
                long.read_window(path, offset, size)
        path.open.assert_not_called()


if __name__ == '__main__':
    unittest.main()
