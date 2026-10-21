"""Synthetic harness assertions only, not native graph evidence."""
import tempfile
from pathlib import Path
from unittest import TestCase, mock
from run_pipewire_secure_camera import SecureCameraHarness, SCENARIOS

class SecureHarnessTests(TestCase):
    def test_required_secure_scenarios_are_explicit(self):
        self.assertTrue({'pixels', 'no_consumer', 'control_stop', 'control_eof', 'expiry', 'partial_cavc', 'source_revoke', 'sink_revoke'}.issubset(SCENARIOS))

    def test_backpressure_requires_actual_rejection_marker(self):
        with tempfile.TemporaryDirectory() as tmp:
            h = object.__new__(SecureCameraHarness)
            h.root = Path(tmp)
            (h.root / 'owner.out').write_text('CAMERA_STOPPED dropped=0\n')
            with mock.patch.object(h, 'run_avc'):
                with self.assertRaises(ValueError):
                    h.run_secure(None, None, None, None, 'no_consumer')
                (h.root / 'owner.out').write_text('SECURE_LISTENING no_media_children=true\nCAMERA_STOPPED expected_backpressure=true dropped=1\n')
                h.run_secure(None, None, None, None, 'no_consumer')

    def test_negative_exit_cannot_replace_verified_failure_markers(self):
        required = {
            'control_stop': ['VERIFIED control_stop'],
            'sink_revoke': ['INJECTED sink_revoke', 'VERIFIED sink_revoke'],
            'source_revoke': ['INJECTED source_revoke'],
            'control_eof': ['INJECTED control_eof'],
            'partial_cavc': ['INJECTED partial_cavc'],
            'expiry': ['FINITE_GRANT_EXPIRED elapsed_ms=4000'],
        }
        with tempfile.TemporaryDirectory() as tmp:
            h = object.__new__(SecureCameraHarness)
            h.root = Path(tmp)
            for scenario, markers in required.items():
                with self.subTest(scenario=scenario), mock.patch.object(h, 'run_avc'):
                    base = 'SECURE_LISTENING no_media_children=true\n'
                    for omitted in markers:
                        (h.root / 'owner.out').write_text(base + '\n'.join(m for m in markers if m != omitted))
                        with self.assertRaises(ValueError):
                            h.run_secure(None, None, None, None, scenario)
                    (h.root / 'owner.out').write_text(base + '\n'.join('NOT_' + m for m in markers))
                    with self.assertRaises(ValueError):
                        h.run_secure(None, None, None, None, scenario)
                    (h.root / 'owner.out').write_text(base + '\n'.join(markers))
                    h.run_secure(None, None, None, None, scenario)
