"""Pure graph-oracle tests; not native graph evidence."""
import unittest
from unittest.mock import patch
import resource
from run_pipewire_camera import CAMERA_DAEMON_FILE_LIMIT, camera_daemon_limits, camera_port, named, no_camera
from run_pipewire_loopback import LIMIT


def node(number=10, name='capyio.camera.test'):
    return {'id': number, 'type': 'PipeWire:Interface:Node', 'info': {'props': {'node.name': name}}}


def port(direction='out'):
    return {'id': 11, 'type': 'PipeWire:Interface:Port',
            'info': {'props': {'node.id': '10', 'port.direction': direction}}}


class CameraOracleTests(unittest.TestCase):
    def test_camera_memfd_budget_is_finite_without_widening_audio_or_log_limit(self):
        self.assertEqual(LIMIT, 2 * 1024 * 1024)
        self.assertGreater(CAMERA_DAEMON_FILE_LIMIT, 3 * 1382400 + 4096)
        self.assertEqual(CAMERA_DAEMON_FILE_LIMIT, 16 * 1024 * 1024)
        with patch('run_pipewire_camera.resource.setrlimit') as set_limit:
            camera_daemon_limits()
        set_limit.assert_called_once_with(resource.RLIMIT_FSIZE,
                                          (CAMERA_DAEMON_FILE_LIMIT, CAMERA_DAEMON_FILE_LIMIT))

    def test_exact_named_node_and_single_directional_port(self):
        objects = [node(), port()]
        self.assertEqual(named(objects, 'capyio.camera.test')['id'], 10)
        self.assertEqual(camera_port(objects, node(), 'out')['id'], 11)
        self.assertFalse(no_camera(objects))
        self.assertTrue(no_camera([node(name='sentinel')]))

    def test_duplicate_or_missing_node_rejected(self):
        for objects in ([], [node(), node(12)]):
            with self.assertRaises(ValueError):
                named(objects, 'capyio.camera.test')

    def test_wrong_missing_or_multiple_ports_rejected(self):
        for ports in ([], [port('in')], [port(), port()]):
            with self.assertRaises(ValueError):
                camera_port([node()] + ports, node(), 'out')


if __name__ == '__main__':
    unittest.main()
