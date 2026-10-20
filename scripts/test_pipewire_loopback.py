"""Pure checks only: no subprocess, daemon, socket or audio device access."""
import math
import struct
import unittest
from unittest.mock import Mock, patch, call
import signal
from run_pipewire_loopback import LIMIT, verify_signal, select_ports, Harness, interrupted


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
        with self.assertRaises(ValueError):
            select_ports(nodes + ports + [ports[0]])


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
