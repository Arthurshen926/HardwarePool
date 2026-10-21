"""Pure readiness parsing tests; these are not actual graph evidence."""
import tempfile
from pathlib import Path
from unittest import TestCase, mock
from run_pipewire_avc import AvcHarness

class ReadinessTests(TestCase):
    def test_camera_line_before_decoder_line_is_pending(self):
        with tempfile.TemporaryDirectory() as tmp:
            h=object.__new__(AvcHarness);h.root=Path(tmp)
            (h.root/'owner.out').write_text('CAMERA_READY 123 capyio.camera.token\n')
            child=mock.Mock();child.poll.return_value=None
            self.assertIsNone(h.ready_line(child))
            child.poll.return_value=1
            with self.assertRaises(ValueError):h.ready_line(child)

    def test_duplicate_decoder_readiness_is_not_ambiguous(self):
        with tempfile.TemporaryDirectory() as tmp:
            h=object.__new__(AvcHarness);h.root=Path(tmp)
            (h.root/'owner.out').write_text('CAMERA_READY 123 capyio.camera.token\nDECODER_READY 124\nDECODER_READY 125\n')
            child=mock.Mock();child.poll.return_value=None
            with self.assertRaises(ValueError):h.ready_line(child)
