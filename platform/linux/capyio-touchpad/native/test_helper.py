#!/usr/bin/env python3
"""Software-only production-loop tests; fake backend cannot open /dev/uinput."""
import os
import pathlib
import struct
import subprocess
import tempfile
import time

HERE = pathlib.Path(__file__).resolve().parent

def start(lifetime=1500, idle=1000, deadline=None):
    if deadline is None:
        deadline = int(time.time() * 1000) + lifetime
    return b'HPT1' + struct.pack('<IIIQ', lifetime, idle, 0, deadline)

def frame(events):
    return b'HPF1' + struct.pack('<HH', len(events), 0) + b''.join(struct.pack('<HHi', *e) for e in events)

SYN = (0, 0, 0)
STOP = frame([])

with tempfile.TemporaryDirectory() as directory:
    binary = pathlib.Path(directory) / 'fake-touchpad-helper'
    subprocess.run(['cc', '-std=c11', '-Wall', '-Wextra', '-Werror', '-O2',
                    '-DCAPYIO_FAKE_UINPUT', str(HERE / 'touchpad_helper.c'), '-o', str(binary)], check=True)
    def run(payload, env=None):
        return subprocess.run([str(binary)], input=payload, capture_output=True, timeout=4,
                              env={**os.environ, **(env or {})})
    def failed(payload, env=None, created=True):
        result = run(payload, env)
        assert result.returncode != 0, result
        assert result.stdout == (b'\x01' if created else b''), result
        assert (b'CREATE\n' in result.stderr) == created, result
        assert (b'DESTROY\n' in result.stderr) == created, result
        return result
    failed(b'', created=False)
    failed(b'HPT1', created=False)
    failed(start(lifetime=300001), created=False)
    failed(start(deadline=1), created=False)
    failed(start(idle=2001), created=False)
    failed(start(), {'CAPYIO_FAKE_FAIL_CREATE': 'permission'}, created=False)
    result = failed(start())  # EOF/parent death: release all before destruction
    expected_flags = os.O_WRONLY | os.O_CLOEXEC | os.O_NONBLOCK
    assert f'OPEN /dev/uinput {expected_flags}\n'.encode() in result.stderr
    assert b'PROPERTY 0\n' in result.stderr
    assert {line for line in result.stderr.splitlines() if line.startswith(b'KEY ')} == {
        b'KEY 330', b'KEY 325', b'KEY 333', b'KEY 334', b'KEY 335', b'KEY 328'}
    assert {line for line in result.stderr.splitlines() if line.startswith(b'AXIS ')} == {
        b'AXIS 47 4 0', b'AXIS 57 2147483647 0', b'AXIS 53 12000 100',
        b'AXIS 54 7000 100', b'AXIS 58 65535 0', b'AXIS 0 12000 100',
        b'AXIS 1 7000 100', b'AXIS 24 65535 0'}
    assert next(line for line in result.stderr.splitlines() if line.startswith(b'EVENT ')) == b'EVENT 1 330 0' 
    result = run(start() + frame([(3, 47, 0), (3, 57, 100), (3, 53, 12000), (3, 54, 7000), (3, 58, 65535), (1, 330, 1), SYN]) + STOP)
    assert result.returncode == 0 and result.stdout == b'\x01\x01\x01', result
    assert result.stderr.endswith(b'EVENT 3 24 0\nEVENT 0 0 0\nDESTROY\n'), result
    assert result.stderr.count(b'EVENT 3 57 -1\n') == 5, result
    for events in [[(3, 47, 5), SYN], [(3, 53, 12001), SYN], [(3, 54, 7001), SYN],
                   [(3, 58, 65536), SYN], [(3, 57, -2), SYN], [(1, 272, 1), SYN],
                   [(1, 330, 2), SYN], [(2, 0, 1), SYN], [SYN, SYN], [(0, 0, 1)],
                   [(3, 53, 3)]]:
        failed(start() + frame(events))
    failed(start() + b'HPF1' + struct.pack('<HH', 65, 0))
    failed(start() + b'HPF1' + struct.pack('<HH', 1, 1))
    failed(start() + frame([SYN])[:-1])
    result = failed(start() + frame([(1, 330, 1), SYN]), {'CAPYIO_FAKE_FAIL_WRITE': '2'})
    assert b'WRITE_FAILED\n' in result.stderr  # partial native submission never ACKed
    result = run(start() + frame([SYN]) * 241)
    assert result.returncode != 0 and len(result.stdout) == 241, result
    assert result.stderr.endswith(b'DESTROY\n'), result
    # Idle and absolute owned deadlines fire without EOF from a living parent.
    for make_payload in (lambda: start(idle=50), lambda: start(lifetime=50), lambda: start(deadline=int(time.time() * 1000) + 50)):
        payload = make_payload()
        child = subprocess.Popen([str(binary)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        child.stdin.write(payload); child.stdin.flush()
        assert child.stdout.read(1) == b'\x01'
        child.wait(timeout=2)
        assert child.returncode != 0 and child.stderr.read().endswith(b'DESTROY\n')
        child.stdin.close(); child.stdout.close(); child.stderr.close()
    # Slow partial frame cannot extend the idle clock.
    child = subprocess.Popen([str(binary)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    child.stdin.write(start(idle=50) + b'HPF'); child.stdin.flush()
    assert child.stdout.read(1) == b'\x01'
    child.wait(timeout=2)
    assert child.returncode != 0 and b'DESTROY\n' in child.stderr.read()
    child.stdin.close(); child.stdout.close(); child.stderr.close()
    # SIGTERM performs release + sync + destroy even with a blocked read.
    child = subprocess.Popen([str(binary)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    child.stdin.write(start()); child.stdin.flush()
    assert child.stdout.read(1) == b'\x01'
    child.terminate(); child.wait(timeout=2)
    assert child.stderr.read().endswith(b'EVENT 0 0 0\nDESTROY\n')
    child.stdin.close(); child.stdout.close(); child.stderr.close()
print('native helper fake-backend contracts passed (no kernel device accessed)')
