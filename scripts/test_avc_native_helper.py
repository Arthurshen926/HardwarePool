#!/usr/bin/env python3
"""Actual native decoder tests. Only synthetic fixture + anonymous owned pipes.

Usage: python3 scripts/test_avc_native_helper.py /absolute/capyio-linux-avc-helper
No media tool, network, capture, user's PipeWire graph, or physical device is used.
"""
import hashlib
import json
import pathlib
import signal
import struct
import subprocess
import sys
import time
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
HELPER = None
HEADER = struct.Struct(">4sIIIQQ")
NV12 = 1280 * 720 * 3 // 2


def record(kind, payload=b"", flags=0, sequence=0, pts=0):
    return HEADER.pack(b"CAD1", kind, len(payload), flags, sequence, pts) + payload


def records(data):
    result = []
    while data:
        if len(data) < 32:
            raise AssertionError("partial output header")
        magic, kind, n, reserved, seq, pts = HEADER.unpack(data[:32])
        assert magic == b"CAN1" and not reserved and n in (0, NV12)
        assert len(data) >= 32 + n
        result.append((kind, seq, pts, data[32:32+n]))
        data = data[32+n:]
    return result


def fixture():
    data = (ROOT / "fixtures/linux-avc/solid-720p30.cavc").read_bytes()
    provenance = json.loads((ROOT / "fixtures/linux-avc/provenance.json").read_text())
    assert hashlib.sha256(data).hexdigest() == provenance["cavc_sha256"]
    out = []
    while data:
        h = data[:56]
        n = int.from_bytes(h[52:56], "big")
        payload, data = data[56:56+n], data[56+n:]
        seq, pts = struct.unpack(">QQ", h[36:52])
        if h[6] == 1:
            out.append(record(1, payload[28:]))
        elif h[7] & 2:
            out.append(record(3, sequence=seq, pts=pts))
        else:
            out.append(record(2, payload, (h[7]&1) | ((h[7]&4)>>1), seq, pts))
    return out


def ue(n):
    value = bin(n + 1)[2:]
    return "0" * (len(value) - 1) + value


def make_sps(width_mbs=79, height_mbs=44, progressive=True, profile=66,
             depth=0, full_range=False, primaries=1, reorder=0):
    """Generate a bounded SPS, not a complete encoded picture, for parser negatives."""
    b = f"{profile:08b}" + "11000000" + f"{31:08b}" + ue(0)
    if profile == 100:
        b += ue(1) + ue(depth) + ue(depth) + "00"
    b += ue(0) + ue(2) + ue(1) + "0" + ue(width_mbs) + ue(height_mbs)
    b += str(int(progressive))
    if not progressive:
        b += "0"
    b += "1" + "0" + "1"  # direct_8x8, no crop, VUI
    b += "00" + "1" + "101" + str(int(full_range)) + "1"
    b += f"{primaries:08b}" + f"{1:08b}" + f"{1:08b}"
    b += "0" + "1" + f"{1:032b}" + f"{60:032b}" + "1"
    b += "000" + "1" + "1" + ue(0) + ue(0) + ue(16) + ue(16) + ue(reorder) + ue(1)
    b += "1"
    b += "0" * ((-len(b)) % 8)
    raw = bytes(int(b[i:i+8], 2) for i in range(0, len(b), 8))
    escaped = bytearray(); zeros = 0
    for byte in raw:
        if zeros >= 2 and byte <= 3:
            escaped.append(3); zeros = 0
        escaped.append(byte); zeros = zeros + 1 if byte == 0 else 0
    return b"\0\0\0\1\x67" + escaped


class NativeDecoderTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if HELPER is None:
            raise unittest.SkipTest("explicit native decoder gate requires built helper")
        if not HELPER.is_file():
            raise RuntimeError("provide built native helper executable path")
        cls.wire = fixture()
        cls.expected = json.loads((ROOT / "fixtures/linux-avc/provenance.json").read_text())["frames"]
        config = cls.wire[0][32:]
        pps_start = config.index(b"\0\0\0\1\x68")
        cls.pps = config[pps_start:]

    def run_helper(self, data, lifetime=2000):
        return subprocess.run([str(HELPER), "--lifetime-ms", str(lifetime)],
                              input=data, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=4)

    def reject(self, data, expected_frames=0):
        result = self.run_helper(data)
        self.assertNotEqual(result.returncode, 0)
        self.assertLess(len(result.stderr), 4096)
        output = records(result.stdout)
        self.assertEqual(sum(r[0] == 2 for r in output), expected_frames)
        return output

    def test_exact_pixels_order_pts_eos(self):
        result = self.run_helper(b"".join(self.wire))
        self.assertEqual(result.returncode, 0, result.stderr)
        out = records(result.stdout)
        self.assertEqual([r[0] for r in out], [1, 2, 2, 2, 3])
        for frame, oracle in zip(out[1:4], self.expected):
            self.assertEqual(frame[1:3], (oracle["sequence"], oracle["pts_us"]))
            self.assertEqual(hashlib.sha256(frame[3]).hexdigest(), oracle["nv12_sha256"])

    def test_fragmented_record_is_not_dropped(self):
        proc = subprocess.Popen([str(HELPER), "--lifetime-ms", "2000"], stdin=subprocess.PIPE,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        first = self.wire[0]
        for part in (first[:3], first[3:31], first[31:]):
            proc.stdin.write(part); proc.stdin.flush()
        out, err = proc.communicate(b"".join(self.wire[1:]), timeout=4)
        self.assertEqual(proc.returncode, 0, err)
        self.assertEqual([r[0] for r in records(out)], [1, 2, 2, 2, 3])

    def test_truncation_oversize_and_bad_header(self):
        for data in (self.wire[0][:7], self.wire[0][:-1], b"BAD!"+self.wire[0][4:],
                     HEADER.pack(b"CAD1", 2, 4*1024*1024+1, 0, 1, 0)):
            with self.subTest(length=len(data)): self.reject(data)

    def test_csd_malformed_duplicate_unknown_and_reconfiguration(self):
        cfg = self.wire[0][32:]
        for data in (record(1, b"\0\0\0\1\x67"), record(1, cfg+cfg), record(1, b"\0\0\1\x69\x80"),
                     self.wire[0]+self.wire[0]):
            self.reject(data)
        # A different SPS hidden in an otherwise valid key AU is not a new epoch.
        self.reject(self.wire[0]+record(2, make_sps()+self.wire[1][32:], 1, 1, 0))

    def test_actual_sps_rejects_dimensions_depth_interlace_color_and_reorder(self):
        # Positive config validation controls the generated-SPS negative oracle.
        out = self.reject(record(1, make_sps()+self.pps))
        self.assertEqual([r[0] for r in out], [1])  # ready, then missing EOS
        for kwargs in ({"width_mbs": 4095}, {"height_mbs": 45}, {"profile": 100, "depth": 2},
                       {"progressive": False}, {"full_range": True}, {"primaries": 9}, {"reorder": 1}):
            with self.subTest(**kwargs):
                self.assertEqual(self.reject(record(1, make_sps(**kwargs)+self.pps)), [])

    def test_false_keyframe_duplicate_gap_and_pts_overflow(self):
        first = self.wire[1]
        payload = first[32:]
        for item in (record(2, payload, 0, 1, 0), record(2, payload, 1, 2, 0),
                     record(2, payload, 1, 1, 2**63), record(2, b"\0\0\1\x65", 1, 1, 0)):
            self.reject(self.wire[0]+item)
        self.reject(self.wire[0]+first+first, expected_frames=1)
        # Non-IDR slice cannot become a recovery point because wire says key.
        self.reject(self.wire[0]+record(2, b"\0\0\1\x41\xb8\x80", 1, 1, 0))
        # Actual B-slice header is rejected even when disguised as a key packet.
        self.reject(self.wire[0]+record(2, b"\0\0\1\x65\xa8\x80", 1, 1, 0))

    def test_bitstream_rotation_cannot_override_zero_rotation_contract(self):
        # H.264 display_orientation SEI: no flip, anticlockwise quarter turn.
        bits = "000" + f"{16384:016b}" + ue(0) + "0" + "1"
        bits += "0" * ((-len(bits)) % 8)
        payload = bytes(int(bits[i:i+8], 2) for i in range(0, len(bits), 8))
        sei = b"\0\0\0\1\x06" + bytes([47, len(payload)]) + payload + b"\x80"
        self.reject(self.wire[0]+record(2, sei+self.wire[1][32:], 1, 1, 0))

    def test_empty_or_multiple_picture_au_and_eos_state(self):
        self.reject(record(3, sequence=1))
        self.reject(self.wire[0]+record(2, b"\0\0\1\x09\xf0", 1, 1, 0))
        self.reject(self.wire[0]+record(2, self.wire[1][32:]*2, 1, 1, 0))
        self.reject(self.wire[0]+record(3, sequence=1))

    def test_independent_deadline_kills_idle_or_backpressured_child(self):
        for data in (b"", self.wire[0]+self.wire[1]):
            proc = subprocess.Popen([str(HELPER), "--lifetime-ms", "150"], stdin=subprocess.PIPE,
                                    stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            start = time.monotonic()
            if data: proc.stdin.write(data); proc.stdin.flush()
            proc.wait(timeout=2)
            self.assertEqual(proc.returncode, -signal.SIGALRM)
            self.assertLess(time.monotonic()-start, 1.5)
            proc.stdin.close(); proc.stdout.close(); proc.stderr.close()

    def test_cli_deadline_is_mandatory_and_bounded(self):
        for args in ([], ["--lifetime-ms", "0"], ["--lifetime-ms", "60001"], ["--url", "invalid"]):
            result = subprocess.run([str(HELPER), *args], capture_output=True, timeout=2)
            self.assertEqual(result.returncode, 2)
            self.assertEqual(result.stdout, b"")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: test_avc_native_helper.py /absolute/built-helper")
    HELPER = pathlib.Path(sys.argv.pop(1)).resolve()
    unittest.main()
