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
import tempfile
import sys
import time
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
HELPER = None
HEADER = struct.Struct(">4sIIIQQ")
NV12 = 1280 * 720 * 3 // 2
FINITE_PROFILE = "cavc-avc-finite-720p30-v1"


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

    def run_helper(self, data, lifetime=2000, profile=None):
        args = [str(HELPER), "--lifetime-ms", str(lifetime)]
        if profile is not None:
            args += ["--profile", profile]
        return subprocess.run(args,
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

    def test_finite_complete_record_rate_rejected_before_payload_read(self):
        # There is deliberately no payload. Budget rejection, not EOF, must win.
        header = HEADER.pack(b"CAD1", 2, 4*1024*1024, 1, 1, 0)
        result = self.run_helper(header, profile=FINITE_PROFILE)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, b"")
        self.assertIn(b"input budget", result.stderr)
        self.assertNotIn(b"truncated payload", result.stderr)

    def test_legacy_121st_header_is_rejected_without_reading_payload(self):
        proc = subprocess.Popen([str(HELPER), "--lifetime-ms", "10000"],
                                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        try:
            self.exchange_one(proc, self.wire[0], 0)
            for n in range(1, 121):
                self.exchange_one(proc, record(2, self.wire[1][32:], 1, n, n*33333), NV12)
            proc.stdin.write(HEADER.pack(b"CAD1", 2, len(self.wire[1])-32, 1, 121, 121*33333))
            proc.stdin.flush()
            proc.wait(timeout=2)
            self.assertNotEqual(proc.returncode, 0)
            self.assertIn(b"input budget", proc.stderr.read())
            self.assertEqual(proc.stdout.read(), b"")
        finally:
            if proc.poll() is None: proc.kill(); proc.wait(timeout=2)
            proc.stdin.close(); proc.stdout.close(); proc.stderr.close()

    @staticmethod
    def exchange_one(proc, data, size):
        # At most one AU and one returned frame are retained, never the session.
        proc.stdin.write(data); proc.stdin.flush()
        header = proc.stdout.read(32)
        if len(header) != 32:
            raise AssertionError("decoder exited before complete response")
        magic, kind, n, reserved, seq, pts = HEADER.unpack(header)
        if magic != b"CAN1" or reserved or n != size:
            raise AssertionError("invalid response header")
        payload = proc.stdout.read(n)
        if len(payload) != n:
            raise AssertionError("partial decoded frame")
        return kind, seq, pts, payload

    def test_explicit_finite_profile_decodes_121_paced_frames(self):
        proc = subprocess.Popen([str(HELPER), "--lifetime-ms", "10000", "--profile", FINITE_PROFILE],
                                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        try:
            self.exchange_one(proc, self.wire[0], 0)
            next_frame = time.monotonic()
            for n in range(1, 122):
                time.sleep(max(0, next_frame - time.monotonic()))
                # Never catch up after a scheduling stall by bursting old work.
                next_frame = time.monotonic() + 1/30
                # Discontinuity is an ordinary key recovery point, not rate refill.
                flags = 3 if n == 100 else 1
                kind, seq, pts, pixels = self.exchange_one(
                    proc, record(2, self.wire[1][32:], flags, n, n*33333), NV12)
                self.assertEqual((kind, seq, pts), (2, n, n*33333))
                self.assertEqual(hashlib.sha256(pixels).hexdigest(), self.expected[0]["nv12_sha256"])
            self.exchange_one(proc, record(3, sequence=122, pts=121*33333), 0)
            proc.wait(timeout=2)
            self.assertEqual(proc.returncode, 0, proc.stderr.read())
        finally:
            if proc.poll() is None: proc.kill(); proc.wait(timeout=2)
            proc.stdin.close(); proc.stdout.close(); proc.stderr.close()

    def test_cli_deadline_is_mandatory_and_bounded(self):
        for args in ([], ["--lifetime-ms", "0"], ["--lifetime-ms", "60001"], ["--url", "invalid"],
                     ["--lifetime-ms", "4294967296"],
                     ["--lifetime-ms", "2000", "--profile", "finite"],
                     ["--lifetime-ms", "2000", "--max-frames", "1800"],
                     ["--lifetime-ms", "2000", "--profile", FINITE_PROFILE, "--profile", "legacy"]):
            result = subprocess.run([str(HELPER), *args], capture_output=True, timeout=2)
            self.assertEqual(result.returncode, 2)
            self.assertEqual(result.stdout, b"")


class NativeBudgetArithmeticTests(unittest.TestCase):
    def test_independent_native_budget_arithmetic(self):
        source = r"""
#include "avc_protocol.h"
#include <assert.h>
#include <limits.h>
int main(void) {
  struct dec_budget b;
  assert(dec_budget_init(&b, 2, 1000, 0));
  assert(dec_budget_init(&b, 1, 0, 0));
  assert(dec_budget_init(&b, 1, 60001, 0));
  assert(dec_budget_init(&b, 1, UINT32_MAX, 0));
  const uint32_t ms[] = {1, 33, 34, 1000, 4000, 4001, 14000, 60000};
  const uint32_t frames[] = {1, 1, 2, 30, 120, 121, 420, 1800};
  const uint64_t bytes[] = {4196401, 4263510, 4265607, 6291456,
                            12582912, 12585009, DEC_MAX_INPUT, DEC_MAX_INPUT};
  for (unsigned i = 0; i < sizeof(ms)/sizeof(ms[0]); i++) {
    assert(!dec_budget_init(&b, 1, ms[i], 0));
    assert(b.max_frames == frames[i] && b.max_input == bytes[i]);
  }
  assert(!dec_budget_init(&b, 0, 60000, 0));
  assert(b.max_frames == 120 && b.max_input == DEC_MAX_INPUT);
  for (unsigned i = 0; i < 120; i++) assert(!dec_budget_charge(&b, 2, 1, 0));
  assert(dec_budget_charge(&b, 2, 1, 0));
  assert(b.total == 120); /* Legacy counts payload only and has no rate limit. */
  assert(!dec_budget_charge(&b, 3, 0, 0));
  assert(b.total == 120);
  assert(!dec_budget_init(&b, 1, 5000, 0));
  for (unsigned i = 0; i < 150; i++)
    assert(!dec_budget_charge(&b, 2, 1, (uint64_t)i * 33333334));
  assert(dec_budget_charge(&b, 2, 1, 6000000000));
  assert(b.frames == 150 && b.total == 150 * 57);
  assert(!dec_budget_init(&b, 1, 60000, 0));
  assert(!dec_budget_charge(&b, 1, 10, 0));
  for (unsigned i = 0; i < 3; i++) assert(!dec_budget_charge(&b, 2, 1, 0));
  assert(dec_budget_charge(&b, 2, 1, 0));
  assert(dec_budget_charge(&b, 2, 1, 33333333));
  assert(!dec_budget_charge(&b, 2, 1, 33333334));
  assert(dec_budget_charge(&b, 2, 1, 33333334));
  /* No PTS/discontinuity inputs exist in the budget, so neither can refill it. */
  assert(b.total == 94 + 4 * 57 && b.frames == 4);
  assert(!dec_budget_charge(&b, 3, 0, 33333334));
  assert(b.total == 94 + 4 * 57 + 56 && b.frames == 4);
  assert(!dec_budget_init(&b, 1, 60000, 0));
  assert(dec_budget_charge(&b, 2, DEC_MAX_AU, 0)); /* Full CAVC exceeds burst. */
  assert(!dec_budget_charge(&b, 1, 10, 0));
  assert(!dec_budget_charge(&b, 2, DEC_FINITE_BYTE_BURST - 94 - 56, 0));
  assert(dec_budget_charge(&b, 3, 0, 0));
  assert(!dec_budget_charge(&b, 3, 0, 1000000));
  assert(b.total == DEC_FINITE_BYTE_BURST + 56 && b.frames == 1);
  assert(!dec_budget_init(&b, 1, 1000, 1));
  assert(dec_bucket_charge(&b.bytes_rate, UINT64_MAX, 1));
  assert(dec_bucket_charge(&b.bytes_rate, 0, 0));
  assert(dec_bucket_charge(&b.bytes_rate, 0, UINT64_MAX));
  assert(b.bytes_rate.tokens == DEC_FINITE_BYTE_BURST * DEC_TOKEN_SCALE);
  b.total = UINT64_MAX;
  assert(dec_budget_charge(&b, 2, 1, 1));
  return 0;
}
"""
        with tempfile.TemporaryDirectory(prefix="capyio-avc-budget-") as tmp:
            code = pathlib.Path(tmp) / "budget.c"
            binary = pathlib.Path(tmp) / "budget"
            code.write_text(source)
            compiled = subprocess.run(["cc", "-std=c11", "-Wall", "-Wextra", "-Werror",
                                       "-I", str(ROOT / "platform/linux/capyio-avc/native"),
                                       str(code), "-o", str(binary)], capture_output=True, timeout=20)
            self.assertEqual(compiled.returncode, 0, compiled.stderr)
            result = subprocess.run([str(binary)], capture_output=True, timeout=2)
            self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: test_avc_native_helper.py /absolute/built-helper")
    HELPER = pathlib.Path(sys.argv.pop(1)).resolve()
    unittest.main()
