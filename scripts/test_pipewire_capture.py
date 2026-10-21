"""Pure stereo capture acceptance checks: no subprocess, socket or audio devices."""
import copy
import struct
import unittest
from pathlib import Path
from unittest.mock import patch

from run_pipewire_capture import (
    SOURCE, SCENARIOS, LIMIT, MARKER, CaptureHarness, capture_nodes,
    capture_process_id, graph_fingerprint, select_capture, signal_payload,
    stereo_ports, stereo_tone, verify_capture_removed, verify_stereo,
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
                b'--format=s16', b'--raw', b'--channel-map=stereo', b'--target=' + SOURCE.encode(), b'-']
        def verify(arguments=args, parent=100, executable=Path('/usr/bin/pw-cat')):
            with patch.object(Path, 'read_text', return_value='PPid:\t' + str(parent)), \
                 patch.object(Path, 'read_bytes', return_value=b'\0'.join(arguments) + b'\0'), \
                 patch.object(Path, 'resolve', side_effect=[executable, Path('/usr/bin/pw-cat')]):
                return capture_process_id(self.graph, self.capture, 100)
        self.assertEqual(verify(), 123)
        for args_bad in ([arg for arg in args if arg != b'--record'],
                         [arg for arg in args if arg != b'--raw'],
                         [b'--target=0' if arg.startswith(b'--target=') else arg for arg in args],
                         [b'--channels=1' if arg == b'--channels=2' else arg for arg in args]):
            with self.assertRaises(ValueError):
                verify(args_bad)
        with self.assertRaises(ValueError):
            verify(parent=999)
        with self.assertRaises(ValueError):
            verify(executable=Path('/tmp/not-pw-cat'))
        with self.assertRaises(ValueError):
            capture_process_id(self.graph, self.capture, True)

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


if __name__ == '__main__':
    unittest.main()
