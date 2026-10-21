"""Pure stereo capture acceptance checks: no subprocess, socket or audio devices."""
import copy
import io
import json
import struct
import unittest
from pathlib import Path
from unittest.mock import patch

from run_pipewire_capture import (
    SOURCE, SCENARIOS, LIMIT, MARKER, CaptureHarness, capture_nodes,
    capture_process_id, graph_fingerprint, select_capture, signal_payload,
    stereo_ports, stereo_tone, verify_capture_removed, verify_stereo,
    parse_pipewire_version, graph_diagnostic, process_diagnostic,
    VIRTUAL_PREFIX, VIRTUAL_SCENARIOS, POISONED_ENV, PRIVATE_ENV, virtual_expectation,
    virtual_nodes, select_virtual_sink, verify_virtual_removed, read_environment,
    verify_capture_environment,
)


def node(identity, properties):
    return {'id': identity, 'type': 'PipeWire:Interface:Node', 'info': {'props': properties}}


def port(identity, owner, channel, direction):
    return {'id': identity, 'type': 'PipeWire:Interface:Port', 'info': {'props': {
        'node.id': owner, 'audio.channel': channel, 'port.direction': direction,
        'object.serial': identity + 100}}}


class StereoSignalTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.signal = signal_payload()

    def test_exact_stereo_sequence_and_distinct_frequencies(self):
        self.assertEqual(len(self.signal), 48000 * 4 * 5)
        verify_stereo(bytes(123 * 4) + self.signal + bytes(480 * 4))

    def test_swapped_mono_crossfed_corrupt_reordered_and_unbounded_fail(self):
        pairs = list(struct.iter_unpack('<hh', self.signal))
        swapped = b''.join(struct.pack('<hh', right, left) for left, right in pairs)
        mono = b''.join(struct.pack('<hh', left, left) for left, _ in pairs)
        crossfed = b''.join(struct.pack('<hh', (left + right) // 2,
                                       (left + right) // 2) for left, right in pairs)
        corrupt = bytearray(self.signal)
        corrupt[4000] ^= 1
        wrong_order = self.signal[48000 * 4 * 3:] + bytes(48000 * 4) + self.signal[:48000 * 4 * 2]
        for raw in (swapped, mono, crossfed, corrupt, wrong_order, b'', b'123',
                    self.signal[4:], bytes(len(self.signal)), bytes(LIMIT + 4)):
            with self.subTest(size=len(raw)), self.assertRaises(ValueError):
                verify_stereo(raw)

    def test_channel_alignment_must_not_match_at_odd_sample_offset(self):
        with self.assertRaises(ValueError):
            verify_stereo(b'\0\0' + self.signal + b'\0\0')

    def test_tones_are_stereo_little_endian_and_bounded(self):
        tone = stereo_tone(1000, 1500, 480)
        self.assertEqual(len(tone), 1920)
        pairs = list(struct.iter_unpack('<hh', tone))
        self.assertEqual(pairs[0], (0, 0))
        self.assertEqual(pairs[12][0], 8192)
        self.assertNotEqual(pairs[12][0], pairs[12][1])
        self.assertTrue(all(abs(left) <= 8192 and abs(right) <= 6144 for left, right in pairs))


class StereoGraphTests(unittest.TestCase):
    def setUp(self):
        self.source = node(10, {'node.name': SOURCE, 'media.class': 'Audio/Source',
                                'node.virtual': True, 'object.serial': 110})
        self.capture = node(20, {'node.name': 'pw-cat', 'target.object': SOURCE,
                                 'client.id': 19, 'object.serial': 120})
        self.client = {'id': 19, 'type': 'PipeWire:Interface:Client', 'info': {'props': {
            'application.process.id': 123, 'pipewire.sec.pid': 123}}}
        self.ports = [port(11, 10, 'FL', 'out'), port(12, 10, 'FR', 'out'),
                      port(21, 20, 'FL', 'in'), port(22, 20, 'FR', 'in')]
        self.unrelated = node(30, {'node.name': 'unrelated.graph', 'object.serial': 130})
        self.graph = [self.source, self.capture, self.client, self.unrelated] + self.ports

    def test_exact_direct_stereo_channels_and_target_are_selected(self):
        self.assertEqual(select_capture(self.graph), (self.capture, [(11, 21), (12, 22)]))
        self.assertEqual(stereo_ports(self.graph, 10, 'out'), {'FL': 11, 'FR': 12})
        self.assertEqual(capture_nodes([self.source, self.unrelated]), [])

    def test_duplicate_mono_swapped_monitor_default_and_autolinked_fail(self):
        for changes in ({'audio.channel': 'MONO'}, {'audio.channel': 'FR'},
                        {'port.direction': 'in'}, {'port.monitor': True}, {'node.id': 999}):
            graph = copy.deepcopy(self.graph)
            graph[4]['info']['props'].update(changes)
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                select_capture(graph)
        for changes in ({'media.class': 'Audio/Sink'}, {'node.virtual': False},
                        {'node.virtual': 1}, {'node.name': 'default'}):
            graph = copy.deepcopy(self.graph)
            graph[0]['info']['props'].update(changes)
            with self.assertRaises(ValueError):
                select_capture(graph)
        link = {'id': 50, 'type': 'PipeWire:Interface:Link', 'info': {'input-node-id': 20}}
        for extra in (self.source, self.capture, self.ports[0], link):
            with self.assertRaises(ValueError):
                select_capture(self.graph + [extra])
        graph = copy.deepcopy(self.graph)
        graph[1]['info']['props']['target.object'] = '0'
        with self.assertRaises(ValueError):
            select_capture(graph)

    def test_stop_removes_only_exact_owned_graph_and_preserves_unrelated_identity(self):
        owned_objects = [self.capture] + self.ports[2:]
        unrelated_objects = [self.source, self.unrelated] + self.ports[:2]
        owned = graph_fingerprint(owned_objects)
        unrelated = graph_fingerprint(unrelated_objects)
        verify_capture_removed(unrelated_objects, owned, unrelated)
        for extra in owned_objects:
            with self.assertRaises(ValueError):
                verify_capture_removed(unrelated_objects + [extra], owned, unrelated)
        with self.assertRaises(ValueError):
            verify_capture_removed(unrelated_objects[1:], owned, unrelated)
        reused = copy.deepcopy(self.capture)
        reused['id'] = 999
        reused['info']['props']['object.serial'] = 1999
        with self.assertRaises(ValueError):
            verify_capture_removed(unrelated_objects + [reused], owned, unrelated)
        changed = copy.deepcopy(unrelated_objects)
        changed[0]['info']['props']['object.serial'] += 1
        with self.assertRaises(ValueError):
            verify_capture_removed(changed, owned, unrelated)

    def test_pid_requires_exact_client_parent_executable_record_and_target(self):
        args = [b'/usr/bin/pw-cat', b'--record', b'--rate=48000', b'--channels=2',
                b'--format=s16', b'--channel-map=stereo', b'--target=' + SOURCE.encode(), b'-']
        def verify(arguments=args, parent=100, executable=Path('/usr/bin/pw-cat'),
                   version='1.0.5', identity=(1, 2, 3, 4, 5)):
            with patch.object(Path, 'read_text', return_value='PPid:\t' + str(parent)), \
                 patch.object(Path, 'read_bytes', return_value=b'\0'.join(arguments) + b'\0'), \
                 patch.object(Path, 'resolve', side_effect=[executable, Path('/usr/bin/pw-cat')]), \
                 patch('run_pipewire_capture.pipewire_binary_identity', return_value=identity):
                return capture_process_id(self.graph, self.capture, 100, version, (1, 2, 3, 4, 5))
        self.assertEqual(verify(), 123)
        for args_bad in ([arg for arg in args if arg != b'--record'],
                         args[:-1] + [b'--raw', b'-'],
                         args[:-1] + [b'output.wav'],
                         [b'--target=0' if arg.startswith(b'--target=') else arg for arg in args],
                         [b'--channels=1' if arg == b'--channels=2' else arg for arg in args]):
            with self.assertRaises(ValueError):
                verify(args_bad)
        with self.assertRaises(ValueError):
            verify(parent=999)
        with self.assertRaises(ValueError):
            verify(executable=Path('/tmp/not-pw-cat'))
        with self.assertRaises(ValueError):
            verify(identity=(9, 9, 9, 9, 9))
        with self.assertRaises(ValueError):
            verify(version='1.5.0')
        with self.assertRaises(ValueError):
            verify(version='1.6.9')
        self.assertEqual(verify(args[:-1] + [b'--raw', b'-'], version='1.6.9'), 123)
        with self.assertRaises(ValueError):
            verify(args[:-1] + [b'--raw', b'--raw', b'-'], version='1.6.9')
        with self.assertRaises(ValueError):
            capture_process_id(self.graph, self.capture, True, '1.0.5', (1, 2, 3, 4, 5))

    def test_exact_old_and_new_cli_versions_do_not_allow_unknown_or_mixed_builds(self):
        for version in ('1.0.5', '1.6.9'):
            value = ('pw-cat\nCompiled with libpipewire ' + version +
                     '\nLinked with libpipewire ' + version + '\n').encode()
            self.assertEqual(parse_pipewire_version(value), version)
        for raw in (b'', b'x' * 1025, b'pw-cat\nCompiled with libpipewire 1.0.5\nLinked with libpipewire 1.6.9\n',
                    b'pw-cat\nCompiled with libpipewire 1.5.0\nLinked with libpipewire 1.5.0\n',
                    b'pw-cat\nCompiled with libpipewire 1.0.5-custom\nLinked with libpipewire 1.0.5-custom\n',
                    b'pw-cat\nCompiled with libpipewire 1.0.5\nLinked with libpipewire 1.0.5\nextra',
                    b'pw-cat\nCompiled with libpipewire 1.0.5\nLinked with libpipewire 1.0.5 \n'):
            with self.subTest(raw=raw[:100]), self.assertRaises(ValueError):
                parse_pipewire_version(raw)

    def test_graph_failure_evidence_is_allowlisted_and_bounded_without_relaxing_selection(self):
        secret = 'PRIVATE-PATH-OR-SECRET-DO-NOT-RETAIN'
        graph = copy.deepcopy(self.graph)
        graph[1]['info']['props'].update({'target.object': secret, 'node.name': secret,
                                        'unrelated.secret': secret, 'media.class': secret})
        graph[2]['info']['props']['application.process.id'] = secret
        graph += [node(999, {'node.name': secret, 'object.serial': secret})] * 1000
        report = graph_diagnostic(graph)
        encoded = json.dumps(report)
        self.assertNotIn(secret, encoded)
        self.assertLess(len(encoded), 32768)
        self.assertTrue(report['truncated'])
        self.assertEqual(len(report['nodes']), 32)
        self.assertEqual(report['counts']['Node'], 1003)
        self.assertFalse(report['nodes'][1]['target_is_exact_source'])
        self.assertTrue(report['nodes'][1]['target_present'])
        self.assertIsNone(report['nodes'][1]['target_numeric'])
        with self.assertRaises(ValueError):
            select_capture(graph)

    def test_owned_process_evidence_reports_only_safe_state_and_bounded_argv_flags(self):
        secret = b'PRIVATE-PATH-OR-SECRET-DO-NOT-RETAIN'
        files = [io.BytesIO(b'Name: ' + secret + b'\nPPid:\t100\nState:\tS (sleeping)\n'),
                 io.BytesIO(secret + b'\0--record\0--raw\0--channels=2\0--channel-map=stereo\0' +
                            b'--target=' + SOURCE.encode() + b'\0-\0'),
                 io.BytesIO(b'124 125')]
        with patch.object(Path, 'open', side_effect=files):
            report = process_diagnostic(123)
        self.assertNotIn(secret.decode(), json.dumps(report))
        self.assertTrue(report['readable'])
        self.assertEqual(report['ppid'], 100)
        self.assertEqual(report['state'], 'S')
        self.assertTrue(report['raw_option'])
        self.assertTrue(report['target_exact'])
        self.assertEqual(report['children'], [124, 125])
        self.assertFalse(process_diagnostic(None)['readable'])
        with patch.object(Path, 'open', side_effect=FileNotFoundError):
            self.assertFalse(process_diagnostic(123)['readable'])
        with patch.object(Path, 'open', side_effect=[io.BytesIO(b'x' * 8193), io.BytesIO(b'')]):
            self.assertTrue(process_diagnostic(123)['truncated'])

    def test_failure_report_uses_owned_processes_and_never_dumps_raw_graph(self):
        harness = CaptureHarness(Path('/tmp/owned-private-test'), 'waveform')
        harness.pipewire_version = '1.0.5'
        harness.diagnostic_processes = {'source': 90, 'owner': 100}
        with patch('run_pipewire_capture.process_diagnostic', side_effect=[
                {'pid': 90, 'readable': True}, {'pid': 100, 'readable': False}]), \
             patch('sys.stderr', new_callable=io.StringIO) as stderr:
            harness.report_graph_failure(self.graph)
        marker, text = stderr.getvalue().strip().split('=', 1)
        self.assertEqual(marker, 'capyio_capture_graph_failure')
        report = json.loads(text)
        self.assertEqual(report['pipewire_version'], '1.0.5')
        self.assertEqual(report['processes']['owner']['pid'], 100)
        self.assertFalse(report['processes']['owner']['readable'])
        self.assertEqual(report['owner_children'], [])
        self.assertLess(len(text), 32768)

    def test_private_environment_and_scenarios_do_not_inherit_user_defaults(self):
        self.assertEqual(SCENARIOS, ('waveform', 'child_exit', 'server_disconnect',
                                     'control_disconnect', 'timeout'))
        harness = CaptureHarness(Path('/tmp/owned-private-test'), 'waveform')
        self.assertEqual(harness.env['PIPEWIRE_REMOTE'], 'capyio-test')
        self.assertEqual(harness.env['CAPYIO_PIPEWIRE_CAPTURE'], 'waveform')
        self.assertEqual(harness.env['HOME'], '/tmp/owned-private-test')
        self.assertNotIn('PIPEWIRE_NODE', harness.env)
        self.assertNotIn('LD_PRELOAD', harness.env)
        self.assertNotIn('DBUS_SESSION_BUS_ADDRESS', harness.env)
        self.assertEqual(MARKER, 'capyio-hosted-pipewire-capture-v1\n')


class VirtualSpeakerGraphTests(unittest.TestCase):
    def setUp(self):
        self.expected = {'owner_pid': 100,
                         'source_port_id': '71a22bb9-1240-40ec-85d3-c832502a2f93',
                         'node_name': VIRTUAL_PREFIX + '71a22bb9-1240-40ec-85d3-c832502a2f93',
                         'endpoint_mode': 'virtual_speaker'}
        self.source = node(10, {'node.name': SOURCE, 'media.class': 'Stream/Output/Audio',
                                'client.id': 9, 'object.serial': 110})
        self.sink = node(20, {'node.name': self.expected['node_name'],
                              'node.description': 'CapyIO Virtual Speaker',
                              'media.class': 'Audio/Sink', 'node.virtual': True,
                              'node.autoconnect': False, 'node.dont-fallback': True,
                              'node.dont-reconnect': True, 'node.dont-move': True,
                              'client.id': 19, 'object.serial': 120})
        self.client = {'id': 19, 'type': 'PipeWire:Interface:Client', 'info': {'props': {
            'application.process.id': 123, 'pipewire.sec.pid': 123}}}
        self.app_client = {'id': 9, 'type': 'PipeWire:Interface:Client', 'info': {'props': {
            'application.process.id': 90, 'pipewire.sec.pid': 90}}}
        self.sentinel = node(30, {'node.name': 'capyio.test.null', 'media.class': 'Audio/Sink',
                                 'object.serial': 130})
        self.ports = [port(11, 10, 'FL', 'out'), port(12, 10, 'FR', 'out'),
                      port(21, 20, 'FL', 'in'), port(22, 20, 'FR', 'in')]
        self.graph = [self.source, self.sink, self.client, self.app_client, self.sentinel] + self.ports

    def test_exact_owned_sink_input_ports_not_sentinel_or_monitor_are_selected(self):
        self.assertEqual(virtual_expectation(self.expected), self.expected)
        self.assertEqual(virtual_nodes(self.graph), [self.sink])
        self.assertEqual(select_virtual_sink(self.graph, self.expected),
                         (self.sink, [(11, 21), (12, 22)]))
        self.assertEqual(virtual_nodes([self.source, self.sentinel]), [])
        graph = copy.deepcopy(self.graph)
        for key in ('node.virtual', 'node.autoconnect', 'node.dont-fallback',
                    'node.dont-reconnect', 'node.dont-move'):
            graph[1]['info']['props'][key] = str(graph[1]['info']['props'][key]).lower()
        self.assertEqual(select_virtual_sink(graph, self.expected)[1], [(11, 21), (12, 22)])

    def test_expectation_rejects_unowned_noncanonical_nil_and_wrong_mode(self):
        for changes in ({'owner_pid': True}, {'owner_pid': 1}, {'owner_pid': '100'},
                        {'owner_pid': 1 << 31}, {'source_port_id': None},
                        {'source_port_id': self.expected['source_port_id'].upper()},
                        {'source_port_id': '00000000-0000-0000-0000-000000000000',
                         'node_name': VIRTUAL_PREFIX + '00000000-0000-0000-0000-000000000000'},
                        {'node_name': VIRTUAL_PREFIX + 'different'},
                        {'node_name': SOURCE}, {'endpoint_mode': 'selected_source'},
                        {'target': 'default'}):
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                virtual_expectation(self.expected | changes)
        for missing in self.expected:
            with self.assertRaises(ValueError):
                virtual_expectation({key: value for key, value in self.expected.items() if key != missing})

    def test_properties_cannot_turn_sink_into_source_monitor_or_autoconnect(self):
        for changes in ({'node.name': 'pw-cat'}, {'media.class': 'Audio/Source'},
                        {'media.class': 'Stream/Input/Audio'}, {'node.virtual': False},
                        {'node.virtual': 1}, {'node.autoconnect': True}, {'node.autoconnect': 0},
                        {'node.dont-fallback': False}, {'node.dont-reconnect': False},
                        {'node.dont-move': False}, {'node.description': 'other'},
                        {'target.object': SOURCE}, {'target.object': False},
                        {'node.rate': '1/44100'}, {'audio.rate': 44100},
                        {'audio.channels': 1}, {'node.force-rate': 44100},
                        {'node.force-rate': '44100'}, {'node.force-quantum': 128},
                        {'node.force-quantum': '128'}, {'node.latency': '128/44100'}):
            graph = copy.deepcopy(self.graph)
            graph[1]['info']['props'].update(changes)
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                select_virtual_sink(graph, self.expected)
        for field in ('node.virtual', 'node.autoconnect', 'node.dont-fallback',
                      'node.dont-reconnect', 'node.dont-move'):
            graph = copy.deepcopy(self.graph)
            del graph[1]['info']['props'][field]
            with self.assertRaises(ValueError):
                select_virtual_sink(graph, self.expected)
        graph = copy.deepcopy(self.graph)
        graph[0]['info']['props']['media.class'] = 'Audio/Source'
        with self.assertRaises(ValueError):
            select_virtual_sink(graph, self.expected)

    def test_duplicate_missing_swapped_mono_and_monitor_ports_and_any_autolink_fail(self):
        for changes in ({'audio.channel': 'MONO'}, {'audio.channel': 'FR'},
                        {'port.direction': 'out'}, {'port.monitor': True},
                        {'port.monitor': 0}, {'node.id': 999}):
            graph = copy.deepcopy(self.graph)
            graph[7]['info']['props'].update(changes)
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                select_virtual_sink(graph, self.expected)
        for graph in (self.graph[:-1], self.graph + [self.sink], self.graph + [self.source],
                      self.graph + [self.ports[2]], self.graph + [port(25, 20, 'FL', 'out')]):
            with self.assertRaises(ValueError):
                select_virtual_sink(graph, self.expected)
        for direction in ('input-node-id', 'output-node-id'):
            link = {'id': 50, 'type': 'PipeWire:Interface:Link', 'info': {direction: 20}}
            with self.assertRaises(ValueError):
                select_virtual_sink(self.graph + [link], self.expected)

    def test_stop_removes_sink_ports_links_but_preserves_sentinel_and_independent_app(self):
        links = [{'id': 50, 'type': 'PipeWire:Interface:Link', 'info': {
            'input-node-id': 20, 'output-node-id': 10, 'props': {'object.serial': 150}}}]
        owned_objects = [self.sink] + self.ports[2:] + links
        unrelated_objects = [self.source, self.sentinel] + self.ports[:2]
        owned = graph_fingerprint(owned_objects)
        unrelated = graph_fingerprint(unrelated_objects)
        verify_virtual_removed(unrelated_objects, owned, unrelated)
        for survivor in owned_objects:
            with self.assertRaises(ValueError):
                verify_virtual_removed(unrelated_objects + [survivor], owned, unrelated)
        renamed = copy.deepcopy(self.sink)
        renamed['id'] = 999
        renamed['info']['props']['node.name'] = 'renamed.sink'
        with self.assertRaises(ValueError):
            verify_virtual_removed(unrelated_objects + [renamed], owned, unrelated)
        for removed in unrelated_objects:
            with self.assertRaises(ValueError):
                verify_virtual_removed([o for o in unrelated_objects if o != removed], owned, unrelated)
        replaced = copy.deepcopy(unrelated_objects)
        replaced[1]['info']['props']['object.serial'] += 1
        with self.assertRaises(ValueError):
            verify_virtual_removed(replaced, owned, unrelated)
        # After server loss, do not falsely claim sentinel preservation.
        verify_virtual_removed([self.sentinel], {}, {})

    def test_actual_child_requires_exact_client_parent_binary_and_owned_arguments(self):
        properties = {key: value for key, value in self.sink['info']['props'].items()
                      if key not in ('client.id', 'object.serial')}
        args = [b'/usr/bin/pw-cat', b'--record', b'--rate=48000', b'--channels=2',
                b'--channel-map=stereo', b'--format=s16', b'--latency=20ms',
                b'--properties=' + json.dumps(properties).encode(), b'--target=0', b'-']
        def verify(arguments=args, graph=None, parent=100, version='1.0.5',
                   executable=Path('/usr/bin/pw-cat'), identity=(1, 2, 3, 4, 5)):
            with patch.object(Path, 'read_text', return_value='PPid:\t' + str(parent)), \
                 patch.object(Path, 'read_bytes', return_value=b'\0'.join(arguments) + b'\0'), \
                 patch.object(Path, 'resolve', side_effect=[executable, Path('/usr/bin/pw-cat')]), \
                 patch('run_pipewire_capture.pipewire_binary_identity', return_value=identity):
                return capture_process_id(self.graph if graph is None else graph, self.sink,
                                          100, version, (1, 2, 3, 4, 5), self.expected)
        self.assertEqual(verify(), 123)
        self.assertEqual(verify(args[:-1] + [b'--raw', b'-'], version='1.6.9'), 123)
        for bad in ([arg for arg in args if arg != b'--record'],
                    args[:-1] + [b'--playback', b'-'], args[:-1] + [b'--monitor', b'-'],
                    args[:-1] + [b'--target=' + SOURCE.encode(), b'-'],
                    args[:-1] + [b'--rate=44100', b'-'],
                    args[:-1] + [b'--latency=128/44100', b'-'],
                    args[:-1] + [b'--raw', b'-'],
                    args[:-1] + [b'--properties={}', b'-'],
                    [b'--properties={}' if arg.startswith(b'--properties=') else arg for arg in args]):
            with self.subTest(args=bad), self.assertRaises(ValueError):
                verify(bad)
        for kwargs in ({'parent': 99}, {'executable': Path('/tmp/fake-pw-cat')},
                       {'identity': (9, 9, 9, 9, 9)}, {'version': '1.6.9'}):
            with self.assertRaises(ValueError):
                verify(**kwargs)
        for changed in ({'application.process.id': 90}, {'application.process.id': True},
                        {'pipewire.sec.pid': 90}):
            graph = copy.deepcopy(self.graph)
            graph[2]['info']['props'].update(changed)
            with self.assertRaises(ValueError):
                verify(graph=graph)
        for graph in (self.graph[:2] + self.graph[3:], self.graph + [self.client]):
            with self.assertRaises(ValueError):
                verify(graph=graph)

    def test_all_six_poison_values_reach_owner_but_not_child_and_private_routing_survives(self):
        harness = CaptureHarness(Path('/tmp/owned-private-test'), 'waveform', 'virtual_speaker')
        clean = {key.encode(): harness.env[key].encode() for key in PRIVATE_ENV}
        owner = clean | {key.encode(): value.encode() for key, value in POISONED_ENV.items()}
        verify_capture_environment(owner, clean, harness.env)
        self.assertEqual(set(POISONED_ENV), {'PIPEWIRE_NODE', 'PIPEWIRE_AUTOCONNECT',
                                           'PIPEWIRE_PROPS', 'PIPEWIRE_QUANTUM',
                                           'PIPEWIRE_LATENCY', 'PIPEWIRE_RATE'})
        for key, value in POISONED_ENV.items():
            with self.subTest(key=key), self.assertRaises(ValueError):
                verify_capture_environment(owner, clean | {key.encode(): value.encode()}, harness.env)
            with self.assertRaises(ValueError):
                verify_capture_environment({k: v for k, v in owner.items() if k != key.encode()},
                                           clean, harness.env)
            with self.assertRaises(ValueError):
                verify_capture_environment(owner | {key.encode(): b'not-poisoned'}, clean, harness.env)
        for key in PRIVATE_ENV:
            with self.assertRaises(ValueError):
                verify_capture_environment(owner, clean | {key.encode(): b'wrong'}, harness.env)
            with self.assertRaises(ValueError):
                verify_capture_environment(owner | {key.encode(): b'wrong'}, clean, harness.env)
        self.assertFalse(set(POISONED_ENV) & set(harness.env))

    def test_environment_reads_are_bounded_owned_and_never_logged(self):
        with patch.object(Path, 'open', return_value=io.BytesIO(b'A=B\0C=D\0')):
            self.assertEqual(read_environment(123), {b'A': b'B', b'C': b'D'})
        for raw in (b'A=B', b'A=B\0A=C\0', b'INVALID\0', b'A=' + b'x' * 32768 + b'\0'):
            with patch.object(Path, 'open', return_value=io.BytesIO(raw)), self.assertRaises(ValueError):
                read_environment(123)
        with self.assertRaises(ValueError):
            read_environment(True)

    def test_virtual_scenarios_are_explicit_and_selected_capture_stays_default(self):
        root = Path('/tmp/owned-private-test')
        self.assertEqual(VIRTUAL_SCENARIOS, ('waveform', 'no_input', 'child_exit',
                                           'server_disconnect', 'control_disconnect', 'expiry'))
        for scenario in VIRTUAL_SCENARIOS:
            harness = CaptureHarness(root, scenario, 'virtual_speaker')
            self.assertEqual(harness.env['CAPYIO_PIPEWIRE_ENDPOINT_MODE'], 'virtual_speaker')
            self.assertEqual(harness.env['CAPYIO_PIPEWIRE_CAPTURE'], scenario)
            self.assertTrue(harness.virtual)
        self.assertFalse(CaptureHarness(root, 'waveform').virtual)
        self.assertEqual(CaptureHarness(root, 'waveform').endpoint_mode, 'selected_source')
        for scenario, mode in (('no_input', 'selected_source'), ('expiry', 'selected_source'),
                               ('timeout', 'virtual_speaker'), ('waveform', 'automatic')):
            with self.assertRaises(ValueError):
                CaptureHarness(root, scenario, mode)


class PairedCaptureHarnessTests(unittest.TestCase):
    def test_all_existing_scenarios_support_explicit_paired_store_without_ambient_bus(self):
        for mode, scenarios in [('selected_source', SCENARIOS), ('virtual_speaker', VIRTUAL_SCENARIOS)]:
            for scenario in scenarios:
                with self.subTest(mode=mode, scenario=scenario):
                    legacy = CaptureHarness(Path('/synthetic-not-opened'), scenario, mode)
                    paired = CaptureHarness(Path('/synthetic-not-opened'), scenario, mode,
                                            paired_store=True)
                    self.assertNotIn('CAPYIO_AUDIO_PAIRED_STORE', legacy.env)
                    self.assertEqual(paired.env['CAPYIO_AUDIO_PAIRED_STORE'], '1')
                    self.assertNotIn('DBUS_SESSION_BUS_ADDRESS', paired.env)
                    self.assertEqual(paired.env['CAPYIO_PIPEWIRE_CAPTURE'], scenario)
                    self.assertEqual(paired.env['CAPYIO_PIPEWIRE_ENDPOINT_MODE'], mode)


if __name__ == '__main__':
    unittest.main()
