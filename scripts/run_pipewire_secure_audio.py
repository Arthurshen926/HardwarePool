#!/usr/bin/env python3
"""Hosted synthetic Rust sender -> pinned TLS -> owned PipeWire playback.

This exercises reusable Rust APIs, not capyio-node's CLI or Android AudioRecord.
No user PipeWire server, physical microphone, pairing store, or keyring is used.
"""
import argparse
import json
from pathlib import Path
import signal
import sys
import tempfile
import time

from run_pipewire_loopback import (
    Harness, graph_fingerprint, interrupted, publish_marker, select_ports,
    verify_playback_process_owner, verify_signal, verify_silence,
)

SCENARIOS = ('waveform', 'stop', 'source_revoke', 'sink_revoke', 'expiry',
             'control_eof', 'media_eof', 'no_authority', 'peer_denied')
NEGATIVE_SCENARIOS = frozenset(('no_authority', 'peer_denied'))
TEST = 'hosted_secure_audio_android_sender_graph'
FORMAT = 'pcm-s16le-48000-mono-tls13-v1'
PREFIX = '.capyio-secure-audio-'


def strict_json(raw):
    """Evidence is bounded, exact-key JSON, including duplicate rejection."""
    if len(raw) > 4096:
        raise ValueError('secure audio evidence exceeds bound')
    def pairs(items):
        value = {}
        for key, item in items:
            if key in value:
                raise ValueError('duplicate secure audio evidence field')
            value[key] = item
        return value
    value = json.loads(raw, object_pairs_hook=pairs)
    if not isinstance(value, dict):
        raise ValueError('secure audio evidence must be an object')
    return value


def integer(value, minimum=0):
    if type(value) is not int or not minimum <= value <= (1 << 63) - 1:
        raise ValueError('invalid secure audio evidence number')
    return value


def identity(value, scenario, stage):
    if scenario not in SCENARIOS or value.get('scenario') != scenario:
        raise ValueError('secure audio scenario mismatch')
    if (value.get('version') != 1 or type(value.get('version')) is not int
            or value.get('stage') != stage or value.get('format') != FORMAT
            or value.get('surface') != 'reusable_rust_apis'):
        raise ValueError('secure audio evidence identity mismatch')
    integer(value.get('owner_pid'), 2)


COMMON = {'version', 'scenario', 'stage', 'format', 'surface', 'owner_pid'}


def validate_pending(value, scenario, stage, owner_pid):
    identity(value, scenario, stage)
    if (set(value) != COMMON | {'helper_absent', 'lease_count', 'bytes_written'}
            or value['owner_pid'] != owner_pid or value['helper_absent'] is not True
            or integer(value['lease_count']) != 0 or integer(value['bytes_written']) != 0):
        raise ValueError('secure audio preconsent authority or helper appeared')
    return value


def validate_bound(value, scenario, owner_pid):
    identity(value, scenario, 'bound')
    if (scenario in NEGATIVE_SCENARIOS
            or set(value) != COMMON | {'helper_pid', 'source_lease_expires_at_ms'}
            or value['owner_pid'] != owner_pid):
        raise ValueError('unexpected secure audio media activation')
    helper = integer(value['helper_pid'], 2)
    if helper == owner_pid:
        raise ValueError('secure audio helper must be a distinct child')
    integer(value['source_lease_expires_at_ms'], 1)
    return value


def validate_result(value, scenario, owner_pid, bound=None):
    identity(value, scenario, 'stopped')
    fields = {'helper_pid', 'source_lease_expires_at_ms', 'terminal_observed_at_ms',
              'bytes_written', 'packets_written', 'packets_dropped_busy',
              'source_records_sent', 'source_records_attempted', 'injected', 'verified', 'session_closed',
              'leases_released', 'helper_reaped'}
    if set(value) != COMMON | fields or value['owner_pid'] != owner_pid:
        raise ValueError('unexpected secure audio final evidence shape')
    for key in ('bytes_written', 'packets_written', 'packets_dropped_busy',
                'source_records_sent', 'source_records_attempted', 'terminal_observed_at_ms'):
        integer(value[key])
    if (value['injected'] != scenario or value['verified'] != scenario
            or any(value[key] is not True for key in
                   ('session_closed', 'leases_released', 'helper_reaped'))
            or value['packets_dropped_busy'] != 0):
        raise ValueError('secure audio terminal reason or cleanup unverified')
    if scenario in NEGATIVE_SCENARIOS:
        if (bound is not None or value['helper_pid'] is not None
                or value['source_lease_expires_at_ms'] is not None
                or any(value[key] for key in
                       ('bytes_written', 'packets_written', 'source_records_sent',
                        'source_records_attempted'))):
            raise ValueError('unauthorized secure audio produced media')
    else:
        if bound is None:
            raise ValueError('secure audio activation evidence absent')
        validate_bound(bound, scenario, owner_pid)
        for key in ('helper_pid', 'source_lease_expires_at_ms'):
            if value[key] != bound[key]:
                raise ValueError('secure audio identity or pinned lease changed')
        # A failed post-write expiry check may follow a delivered final record.
        # Track actual calls, not an invented tolerance on received PCM/deadlines.
        if (value['packets_written'] < 300
                or value['bytes_written'] != value['packets_written'] * 960
                or value['source_records_sent'] < 300
                or value['source_records_attempted'] < value['packets_written']
                or value['source_records_attempted'] != value['source_records_sent']
                   + int(scenario in ('source_revoke', 'expiry'))):
            raise ValueError('complete bounded PCM write evidence absent')
        expiry = bound['source_lease_expires_at_ms']
        observed = value['terminal_observed_at_ms']
        if scenario == 'expiry' and observed < expiry:
            raise ValueError('secure audio ended before the exact negotiated Source lease')
        if scenario != 'expiry' and observed >= expiry:
            raise ValueError('incidental expiry cannot prove the requested terminal cause')
    return value


def verify_execution(text):
    if (f'test {TEST} ... ok' not in text
            or 'test result: ok. 1 passed; 0 failed; 0 ignored;' not in text):
        raise ValueError('exact hosted secure audio test must execute, not skip')


def no_playback(objects):
    if any(o.get('info', {}).get('props', {}).get('target.object') == 'capyio.test.null'
           for o in objects):
        raise ValueError('secure audio playback exists outside authorization')


def no_children(owner_pid):
    for path in Path('/proc').iterdir():
        if not path.name.isdecimal():
            continue
        try:
            fields = dict(line.split(':', 1) for line in (path / 'status').read_text().splitlines()
                          if ':' in line)
        except (FileNotFoundError, ProcessLookupError):
            continue
        if int(fields.get('PPid', '0')) == owner_pid:
            raise ValueError('secure audio owns a child outside authorization')


class SecureAudioHarness(Harness):
    def evidence(self, name):
        return strict_json((self.root / (PREFIX + name + '.json')).read_bytes())

    def checkpoint(self, producer, name):
        self.wait_checkpoint(producer, PREFIX + name + '.json')
        return self.evidence(name)

    def run_secure(self, fixture, scenario):
        if scenario not in SCENARIOS:
            raise ValueError('unsupported secure audio scenario')
        self.env.update(CAPYIO_SECURE_AUDIO_HOSTED='1', CAPYIO_SECURE_AUDIO_SCENARIO=scenario)
        self.prepare_daemon()
        record = self.spawn(['/usr/bin/pw-cat', '--record', '--rate=48000', '--channels=1',
                             '--format=s16', '--channel-map=mono', '--latency=20ms', '--target=0',
                             '--properties={"node.name":"capyio.test.monitor"}', '-'], 'capture')
        producer = self.spawn([str(fixture), '--ignored', '--exact', TEST,
                               '--nocapture', '--test-threads=1'], 'producer')
        pending = self.checkpoint(producer, 'before_local')
        validate_pending(pending, scenario, 'before_local', producer.pid)
        ready = time.monotonic() + 5
        while True:
            self.check()
            objects = self.dump()
            no_playback(objects)
            no_children(producer.pid)
            try:
                monitor_links = select_ports(objects, monitor_only=True)
                break
            except ValueError:
                if time.monotonic() >= ready:
                    raise
                time.sleep(.02)
        for source, target in monitor_links:
            self.command(['/usr/bin/pw-link', str(source), str(target)])
        baseline = graph_fingerprint(self.dump())
        self.observe_pending(producer, baseline)
        publish_marker(self.root / (PREFIX + 'local_checked'))
        if scenario != 'no_authority':
            pending = self.checkpoint(producer, 'before_source')
            validate_pending(pending, scenario, 'before_source', producer.pid)
            self.observe_pending(producer, baseline)
            publish_marker(self.root / (PREFIX + 'source_checked'))
        bound = None
        owned = {}
        if scenario not in NEGATIVE_SCENARIOS:
            bound = self.checkpoint(producer, 'bound')
            validate_bound(bound, scenario, producer.pid)
            ready = time.monotonic() + 5
            while True:
                self.check()
                objects = self.dump()
                try:
                    links = select_ports(objects)
                    helper, owned_ids = verify_playback_process_owner(objects, producer.pid)
                    break
                except ValueError:
                    if time.monotonic() >= ready or producer.poll() is not None:
                        raise
                    time.sleep(.02)
            if helper != bound['helper_pid']:
                raise ValueError('graph is not the exact secure owner helper')
            for source, target in links:
                if (source, target) not in monitor_links:
                    self.command(['/usr/bin/pw-link', str(source), str(target)])
            objects = self.dump()
            actual = {(o.get('info', {}).get('output-port-id'), o.get('info', {}).get('input-port-id'))
                      for o in objects if o.get('type') == 'PipeWire:Interface:Link'}
            if not set(links).issubset(actual):
                raise ValueError('explicit secure playback links absent')
            owned_ids.update(o['id'] for o in objects if o.get('type') == 'PipeWire:Interface:Link'
                             and (o.get('info', {}).get('output-port-id') in owned_ids
                                  or o.get('info', {}).get('input-port-id') in owned_ids))
            owned = {key: value for key, value in graph_fingerprint(objects).items() if key in owned_ids}
            if any(graph_fingerprint(objects).get(key) != value for key, value in baseline.items()):
                raise ValueError('secure playback changed an unrelated graph object')
            publish_marker(self.root / (PREFIX + 'links_ready'))
        result = self.checkpoint(producer, 'stopped')
        validate_result(result, scenario, producer.pid, bound)
        for _ in range(3):
            objects = self.dump()
            current = graph_fingerprint(objects)
            no_playback(objects)
            no_children(producer.pid)
            if bound and Path('/proc', str(bound['helper_pid'])).exists():
                raise ValueError('secure playback helper was not actually reaped')
            if (any(current.get(key) == value for key, value in owned.items())
                    or any(current.get(key) != value for key, value in baseline.items())):
                raise ValueError('secure owned graph survived or unrelated graph changed')
            time.sleep(.1)
        # Old queued quantum may drain, but new post-cleanup samples must be silent.
        offset = (self.root / 'capture.out').stat().st_size
        time.sleep(.5)
        self.check()
        raw = (self.root / 'capture.out').read_bytes()
        verify_silence(raw[offset:], minimum_samples=14400)
        if scenario in NEGATIVE_SCENARIOS:
            verify_silence(raw, minimum_samples=4800)
        else:
            verify_signal(raw)
        publish_marker(self.root / (PREFIX + 'stop_checked'))
        while producer.poll() is None:
            self.check()
            time.sleep(.02)
        if producer.returncode:
            raise ValueError('secure audio Rust fixture failed')
        verify_execution((self.root / 'producer.out').read_text())
        self.stop(record)
        self.children.remove(record)

    def observe_pending(self, producer, baseline):
        offset = (self.root / 'capture.out').stat().st_size
        time.sleep(.4)
        self.check()
        if producer.poll() is not None:
            raise ValueError('secure audio fixture exited before consent observation')
        objects = self.dump()
        no_playback(objects)
        no_children(producer.pid)
        if graph_fingerprint(objects) != baseline:
            raise ValueError('secure audio modified graph before both grants and leases')
        verify_silence((self.root / 'capture.out').read_bytes()[offset:], minimum_samples=4800)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--hosted-private-secure-audio', action='store_true', required=True)
    parser.add_argument('--fixture', type=Path, required=True)
    parser.add_argument('--scenario', choices=SCENARIOS, required=True)
    args = parser.parse_args()
    for sig in (signal.SIGTERM, signal.SIGINT):
        signal.signal(sig, interrupted)
    with tempfile.TemporaryDirectory(prefix='capyio-secure-audio-') as temporary:
        root = Path(temporary)
        root.chmod(0o700)
        harness = SecureAudioHarness(root, e2e=True)
        try:
            harness.run_secure(args.fixture.resolve(strict=True), args.scenario)
        except Exception:
            for name in ('producer.out', 'producer.err', 'daemon.err', 'capture.err'):
                path = root / name
                if path.is_file():
                    print(name + ': ' + path.read_text(errors='replace')[-4096:], file=sys.stderr)
            raise
        finally:
            harness.close()
    print(f'capyio_pipewire_secure_audio=passed scenario={args.scenario}'
          ' surface=reusable_rust_apis capture=synthetic real_graph=true owned_cleanup=true')


if __name__ == '__main__':
    main()
