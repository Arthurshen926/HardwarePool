"""Pure checks only: no subprocess, daemon, socket or audio device access."""
import math
import copy
import struct
import unittest
from unittest.mock import Mock, patch, call
import signal
from run_pipewire_loopback import (
    LIMIT, VIRTUAL_PREFIX, verify_signal, select_ports, Harness, interrupted, verify_silence,
    virtual_expectation, virtual_nodes, select_virtual_source, recorder_port,
    graph_fingerprint, verify_virtual_removed,
)


def tone(hz, seconds):
    return b''.join(struct.pack('<h', round(8192 * math.sin(2 * math.pi * hz * i / 48000)))
                    for i in range(round(seconds * 48000)))


class SignalTests(unittest.TestCase):
    def test_expected_signature_with_leading_silence_and_partial_window(self):
        verify_signal(bytes(1234) + tone(1000, 2) + bytes(96000) + tone(2000, 2))

    def test_silence_wrong_frequency_order_and_noise_fail(self):
        for raw in (bytes(480000), tone(3000, 5),
                    tone(2000, 2) + bytes(96000) + tone(1000, 2),
                    struct.pack('<hh', 12000, -12000) * 120000):
            with self.subTest(length=len(raw)), self.assertRaises(ValueError):
                verify_signal(raw)

    def test_short_loud_and_unbounded_inputs_fail(self):
        for raw in (b'x', bytes(LIMIT + 2), tone(1000, .2),
                    struct.pack('<h', 32767) * 240000):
            with self.assertRaises(ValueError):
                verify_signal(raw)

    def test_preconsent_and_stop_silence_needs_actual_samples(self):
        verify_silence(bytes(9600), minimum_samples=4800)
        for raw in (b'', bytes(9598), tone(1000, .1)):
            with self.assertRaises(ValueError):
                verify_silence(raw, minimum_samples=4800)

    def test_graph_requires_unique_exact_nodes(self):
        with self.assertRaises(ValueError):
            select_ports([])
        nodes = [dict(id=i, type='PipeWire:Interface:Node', info={'props': props})
                 for i, props in enumerate(({'node.name': 'capyio.test.null'},
                                            {'node.name': 'capyio.test.monitor'},
                                            {'target.object': 'capyio.test.null'}))]
        ports = [dict(id=i + 10, type='PipeWire:Interface:Port', info={'props': {
            'node.id': owner, 'port.direction': direction}})
                 for i, (owner, direction) in enumerate(((2, 'out'), (0, 'in'),
                                                        (0, 'out'), (1, 'in')))]
        self.assertEqual(select_ports(nodes + ports), [(10, 11), (12, 13)])
        self.assertEqual(select_ports(nodes[:2] + ports, monitor_only=True), [(12, 13)])
        with self.assertRaises(ValueError):
            select_ports(nodes + ports + [ports[0]])


class VirtualMicrophoneTests(unittest.TestCase):
    def setUp(self):
        self.expected = {
            'sink_port_id': 'b3ed3c7c-f630-407b-923a-d3770e0ee624',
            'node_name': VIRTUAL_PREFIX + 'b3ed3c7c-f630-407b-923a-d3770e0ee624',
            'owner_pid': 123,
        }
        self.source = dict(id=50, type='PipeWire:Interface:Node', info={'props': {
            'node.name': self.expected['node_name'], 'media.class': 'Audio/Source',
            'node.virtual': True, 'application.process.id': 124, 'object.serial': 150}})
        self.output = dict(id=51, type='PipeWire:Interface:Port', info={'props': {
            'node.id': '50', 'port.direction': 'out', 'audio.channel': 'MONO',
            'object.serial': 151}})
        self.recorder = dict(id=60, type='PipeWire:Interface:Node', info={'props': {
            'node.name': 'capyio.test.monitor', 'object.serial': 160}})
        self.input = dict(id=61, type='PipeWire:Interface:Port', info={'props': {
            'node.id': 60, 'port.direction': 'in', 'object.serial': 161}})
        self.unrelated = [self.recorder, self.input]
        self.graph = [self.source, self.output] + self.unrelated

    def test_generated_name_is_exact_canonical_local_sink_not_external_target(self):
        self.assertEqual(virtual_expectation(self.expected), self.expected)
        for updates in (
                {'sink_port_id': self.expected['sink_port_id'].upper()},
                {'node_name': 'capyio.test.null'},
                {'node_name': VIRTUAL_PREFIX + 'c3ed3c7c-f630-407b-923a-d3770e0ee624'},
                {'sink_port_id': 'not-a-port-id'},
                {'owner_pid': True}, {'owner_pid': 0}, {'owner_pid': '123'}):
            with self.subTest(updates=updates), self.assertRaises(ValueError):
                virtual_expectation(self.expected | updates)

    def test_direct_audio_source_not_sink_monitor_is_selected(self):
        self.assertEqual(select_virtual_source(self.graph, self.expected),
                         (self.source, self.output))
        self.assertEqual(recorder_port(self.graph), 61)
        self.assertEqual(virtual_nodes(self.unrelated), [])
        for props in (
                {'media.class': 'Audio/Sink'}, {'media.class': 'Stream/Output/Audio'},
                {'node.name': 'renamed.null.monitor'}, {'node.virtual': False},
                {'node.virtual': 1}, {'node.virtual': 'false'}):
            graph = copy.deepcopy(self.graph)
            graph[0]['info']['props'].update(props)
            with self.subTest(props=props), self.assertRaises(ValueError):
                select_virtual_source(graph, self.expected)
        for props in (
                {'port.direction': 'in'}, {'port.monitor': True},
                {'port.monitor': 'true'}, {'audio.channel': 'FL'},
                {'node.id': 60}):
            graph = copy.deepcopy(self.graph)
            graph[1]['info']['props'].update(props)
            with self.subTest(props=props), self.assertRaises(ValueError):
                select_virtual_source(graph, self.expected)

    def test_duplicate_source_extra_ports_and_automatic_links_are_rejected(self):
        link = dict(id=70, type='PipeWire:Interface:Link', info={
            'output-node-id': 50, 'input-node-id': 60,
            'output-port-id': 51, 'input-port-id': 61})
        another_source = dict(id=80, type='PipeWire:Interface:Node', info={'props': {
            'node.name': 'unrelated.source', 'media.class': 'Audio/Source'}})
        for extra in (self.source, self.output, link, another_source):
            with self.subTest(extra=extra), self.assertRaises(ValueError):
                select_virtual_source(self.graph + [extra], self.expected)
        for graph in ([], [self.source], self.graph + [self.recorder], self.graph + [self.input]):
            with self.assertRaises(ValueError):
                recorder_port(graph)

    def test_stop_requires_exact_owned_objects_gone_and_unrelated_objects_retained(self):
        unrelated = graph_fingerprint(self.unrelated)
        owned = graph_fingerprint([self.source, self.output])
        verify_virtual_removed(self.unrelated, owned, unrelated)
        # PipeWire may reuse global IDs, but an old object.serial must be gone.
        reused = copy.deepcopy(self.output)
        reused['info']['props'].update({'node.id': 60, 'object.serial': 999})
        verify_virtual_removed(self.unrelated + [reused], owned, unrelated)
        link = dict(id=70, type='PipeWire:Interface:Link',
                    info={'props': {'object.serial': 170}})
        owned.update(graph_fingerprint([link]))
        changed_unrelated = copy.deepcopy(self.unrelated)
        changed_unrelated[0]['info']['props']['object.serial'] = 260
        for graph in (self.graph, self.unrelated + [self.output], self.unrelated + [link],
                      [self.recorder], changed_unrelated):
            with self.subTest(graph=graph), self.assertRaises(ValueError):
                verify_virtual_removed(graph, owned, unrelated)
        # A fresh post-Stop Source must fail even with entirely new IDs/serials.
        fresh = copy.deepcopy(self.source)
        fresh['id'] = 90
        fresh['info']['props']['object.serial'] = 190
        with self.assertRaises(ValueError):
            verify_virtual_removed(self.unrelated + [fresh], owned, unrelated)


class CleanupTests(unittest.TestCase):
    def test_exited_leader_still_kills_descendant_group(self):
        child = Mock(pid=123)
        with patch('run_pipewire_loopback.os.killpg',
                   side_effect=[None, None, ProcessLookupError]) as kill:
            Harness.stop(child)
        self.assertEqual(kill.call_args_list,
                         [call(123, signal.SIGTERM), call(123, signal.SIGKILL), call(123, 0)])
        self.assertEqual(child.wait.call_count, 2)

    def test_stuck_group_cannot_report_cleanup_success(self):
        with patch('run_pipewire_loopback.os.killpg'), \
             patch('run_pipewire_loopback.time.monotonic', side_effect=[0, 2]):
            with self.assertRaises(RuntimeError):
                Harness.stop(Mock(pid=123))

    def test_external_termination_unwinds_finally(self):
        cleanup = Mock()
        with self.assertRaises(SystemExit):
            try:
                interrupted(signal.SIGTERM, None)
            finally:
                cleanup()
        cleanup.assert_called_once()


if __name__ == '__main__':
    unittest.main()
