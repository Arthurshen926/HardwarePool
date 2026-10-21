#!/usr/bin/env python3
"""Synthetic-only fixture generator. ffmpeg/libx264 is a test tool, not shipped.
Exact reproduction requires the version/config saved in provenance.json.
No camera, network, file input, user media or hardware encoder is accessed.
"""
import hashlib
import json
from pathlib import Path
import re
import struct
import subprocess

ROOT = Path(__file__).resolve().parents[1]
DEST = ROOT / 'fixtures/linux-avc'
COLORS = [(32, 80, 160), (96, 100, 140), (160, 120, 120)]
STREAM = bytes(range(1, 17))

def run(args, data=None):
    return subprocess.run(args, input=data, stdout=subprocess.PIPE,
                          stderr=subprocess.PIPE, check=True).stdout

def record(kind, flags, sequence, pts, payload):
    return struct.pack('>4sBBBBHH16sQQQI', b'CAVC', 1, 1, kind, flags,
                       56, 0, STREAM, 1, sequence, pts, len(payload)) + payload

def main():
    raw = b''.join(bytes([y]) * 921600 + bytes([u]) * 230400 + bytes([v]) * 230400
                   for y, u, v in COLORS)
    args = ['ffmpeg', '-v', 'error', '-f', 'rawvideo', '-pixel_format', 'yuv420p',
            '-video_size', '1280x720', '-framerate', '30', '-i', 'pipe:0',
            '-frames:v', '3', '-an', '-c:v', 'libx264', '-threads', '1',
            '-preset', 'veryslow', '-profile:v', 'baseline', '-level:v', '3.1',
            '-qp', '20', '-x264-params',
            'keyint=1:min-keyint=1:scenecut=0:bframes=0:ref=1:aud=1:repeat-headers=1:fps=30/1:force-cfr=1:colorprim=bt709:transfer=bt709:colormatrix=bt709:range=tv',
            '-color_range', 'tv', '-colorspace', 'bt709', '-color_primaries',
            'bt709', '-color_trc', 'bt709', '-f', 'h264', 'pipe:1']
    encoded = run(args, raw)
    nals = [n for n in re.split(b'\x00\x00(?:\x00)?\x01', encoded) if n]
    csd = [n for n in nals if n[0] & 31 in (7, 8)][:2]
    assert [n[0] & 31 for n in csd] == [7, 8]
    aus = []
    for nal in nals:
        typ = nal[0] & 31
        if typ == 9:
            aus.append(bytearray())
        if typ not in (7, 8):
            aus[-1] += b'\x00\x00\x00\x01' + nal
    assert len(aus) == 3
    c0, c1 = [b'\x00\x00\x00\x01' + n for n in csd]
    config = struct.pack('>HHHHIBBBBBB2xII', 1280, 720, 30, 0, 4000000,
                         1, 1, 1, 1, 1, 0, len(c0), len(c1)) + c0 + c1
    cavc = record(1, 0, 0, 0, config)
    cavc += b''.join(record(2, 1, i+1, i*33333, au) for i, au in enumerate(aus))
    cavc += record(2, 2, 4, 99999, b'')
    decoded = run(['ffmpeg', '-v', 'error', '-threads', '1', '-f', 'h264', '-i',
                   'pipe:0', '-f', 'rawvideo', '-pix_fmt', 'nv12', 'pipe:1'], encoded)
    assert len(decoded) == 3*1382400
    oracle = []
    for i in range(3):
        frame = decoded[i*1382400:(i+1)*1382400]
        y, u, v = frame[0], frame[921600], frame[921601]
        assert frame == bytes([y])*921600 + bytes([u,v])*230400
        oracle.append({'sequence':i+1, 'pts_us':i*33333, 'yuv':[y,u,v],
                       'nv12_sha256':hashlib.sha256(frame).hexdigest()})
    DEST.mkdir(exist_ok=True)
    (DEST/'solid-720p30.cavc').write_bytes(cavc)
    provenance = {'synthetic':True, 'encoder_command':args,
                  'ffmpeg_version_configuration':run(['ffmpeg','-version']).decode(),
                  'input_yuv420p_sha256':hashlib.sha256(raw).hexdigest(),
                  'cavc_sha256':hashlib.sha256(cavc).hexdigest(), 'frames':oracle,
                  'license_note':'This Debian FFmpeg tool is GPL-enabled. No executable/library is redistributed. Fixture contains only generated solid color video.'}
    (DEST/'provenance.json').write_text(json.dumps(provenance, indent=2)+'\n')

if __name__ == '__main__':
    main()
