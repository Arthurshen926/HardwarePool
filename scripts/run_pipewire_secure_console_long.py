#!/usr/bin/env python3
"""Separate hosted-only finite 300s console gate. Never execute on user devices."""
import argparse
import array
import math
import os
from pathlib import Path
import re
import resource
import signal
import subprocess
import sys
import tempfile
import time

import run_pipewire_secure_console as short
import run_pipewire_secure_audio as secure
from run_pipewire_secure_console import (
    process_ids, endpoint_identity, verify_console_owner, recorder_properties,
    select_console_graph, verify_links, verify_reaped, strict_json,
)
from run_pipewire_loopback import (
    graph_fingerprint, interrupted, publish_marker, select_ports, verify_silence,
    virtual_nodes, recorder_port, verify_virtual_removed,
)

TEST = 'secure_console_long::hosted_secure_audio_console_long_graph'
PREFIX = '.capyio-secure-console-long-'
FORMAT = 'pcm-s16le-48000-mono-tls13-300s-v1'
SCENARIOS = ('stop', 'expiry')
NEGATIVE_SCENARIOS = frozenset()
ENDPOINT_MODES = short.ENDPOINT_MODES
PCM_RATE = 96000
CAPTURE_LIMIT = 32 * 1024 * 1024
LOG_LIMIT = 2 * 1024 * 1024
WINDOW_BYTES = 4 * PCM_RATE
OUTER_SECONDS = 345
WINDOW_TONES = {'initial': (1000, 2000), 'late': (1500, 2500), 'final': (1750, 2750)}


def profile(scenario, mode):
    if (scenario, mode) not in (('stop', 'playback'), ('expiry', 'virtual_microphone')):
        raise ValueError('exact long-only scenario and endpoint required')


def projected(value):
    if value.get('format') != FORMAT:
        raise ValueError('exact opt-in 300-second format required')
    return {key: (short.FORMAT if key == 'format' else item)
            for key, item in value.items() if key != 'authority'}


def validate_pending(value, scenario, stage, owners):
    profile(scenario, owners.get('endpoint_mode', 'playback'))
    if 'authority' in value:
        raise ValueError('authority before consent')
    return short.validate_pending(projected(value), scenario, stage, owners)


def validate_bound(value, scenario, owners):
    profile(scenario, owners.get('endpoint_mode', 'playback'))
    short.validate_bound(projected(value), scenario, owners)
    authority = value.get('authority')
    if (not isinstance(authority, dict) or set(authority) != {
            'route', 'source_lease', 'stream_id', 'epoch', 'requested_seconds'}
            or type(authority['requested_seconds']) is not int
            or authority['requested_seconds'] != 300):
        raise ValueError('original full authority required')
    route, lease = authority['route'], authority['source_lease']
    if not isinstance(route, dict) or not isinstance(lease, dict):
        raise ValueError('original route and lease required')
    if (route.get('selected_format') != {'id': FORMAT, 'parameters': {}}
            or lease.get('expires_at_ms') != value['source_lease_expires_at_ms']
            or lease.get('route_id') != route.get('id')
            or lease.get('session_id') != route.get('session_id')
            or lease.get('port') != route.get('source')
            or not isinstance(authority['stream_id'], str) or not authority['stream_id']
            or type(authority['epoch']) is not int or authority['epoch'] < 1):
        raise ValueError('original route, profile, stream and lease mismatch')
    return value


def same_bound(value, scenario, owners, bound, stage):
    if bound is None:
        raise ValueError('original bound checkpoint required')
    validate_bound(bound, scenario, owners)
    short.identity(projected(value), scenario, stage, owners)
    for key in ('authority', 'helper_pid', 'source_lease_expires_at_ms'):
        if value.get(key) != bound.get(key):
            raise ValueError('original authority or helper changed')


def validate_window(value, scenario, window, owners, bound):
    if window not in WINDOW_TONES or window == 'final' and scenario != 'expiry':
        raise ValueError('invalid long-only window')
    same_bound(value, scenario, owners, bound, window)
    if set(value) != set(bound) | {'elapsed_ms', 'window_observed_at_ms'}:
        raise ValueError('exact window evidence required')
    elapsed = secure.integer(value['elapsed_ms'], 0)
    observed = secure.integer(value['window_observed_at_ms'], 1)
    if (window == 'late' and elapsed < 65000
            or window == 'final' and (elapsed < 280000 or not 3000 <= bound['source_lease_expires_at_ms'] - observed <= 6000)
            or observed >= bound['source_lease_expires_at_ms']):
        raise ValueError('late ordered window must precede original expiry')


def validate_result(value, scenario, owners, bound):
    same_bound(value, scenario, owners, bound, 'stopped')
    required = short.common_fields(owners) | short.CLEANUP_FIELDS | {
        'authority', 'helper_pid', 'source_lease_expires_at_ms', 'terminal_observed_at_ms',
        'terminal_cause', 'bytes_written', 'packets_written', 'packets_dropped_busy',
        'source_records_sent', 'source_records_attempted', 'injected', 'verified',
        'session_closed', 'leases_released', 'helper_reaped', 'terminal_elapsed_ms'}
    if (set(value) != required or any(value.get(key) is not True for key in
            short.CLEANUP_FIELDS | {'session_closed', 'leases_released', 'helper_reaped'})
            or value['injected'] != scenario or value['verified'] != scenario
            or value['terminal_cause'] not in short.TERMINAL_CAUSES[scenario]):
        raise ValueError('exact terminal and cleanup evidence required')
    counts = {key: secure.integer(value[key], 0) for key in (
        'bytes_written', 'packets_written', 'packets_dropped_busy',
        'source_records_sent', 'source_records_attempted', 'terminal_observed_at_ms')}
    packets, sent = counts['packets_written'], counts['source_records_sent']
    minimum = 6000 if scenario == 'stop' else 25000
    if (not minimum <= packets <= 30000 or not packets - 1 <= sent <= 30000
            or counts['bytes_written'] != packets * 960 or counts['packets_dropped_busy'] != 0
            or counts['source_records_attempted'] != sent + int(scenario == 'expiry')):
        raise ValueError('finite long-profile record accounting mismatch')
    elapsed = secure.integer(value['terminal_elapsed_ms'], 0)
    if elapsed < (75000 if scenario == 'stop' else 290000) or elapsed > 310000:
        raise ValueError('original long session elapsed lifetime absent')
    observed, expiry = counts['terminal_observed_at_ms'], bound['source_lease_expires_at_ms']
    if ((scenario == 'expiry' and not expiry <= observed <= expiry + 2000)
            or (scenario == 'stop' and observed >= expiry)):
        raise ValueError('terminal must match exact original expiry without early tolerance')


def verify_timeline(window, elapsed, byte_delta):
    if window not in ('late', 'final'):
        raise ValueError('late recorder window required')
    minimum = 60 if window == 'late' else 280
    if elapsed < minimum or byte_delta < (minimum - 1) * PCM_RATE:
        raise ValueError('independent late recorder timeline absent')


def read_window(path, offset, size):
    if (type(offset) is not int or type(size) is not int or offset < 0 or offset % 2
            or size < 2 or size > WINDOW_BYTES or size % 2):
        raise ValueError('bounded aligned capture window required')
    with path.open('rb') as stream:
        total = os.fstat(stream.fileno()).st_size
        if total > CAPTURE_LIMIT or total % 2 or offset + size > total:
            raise ValueError('oversize or truncated capture')
        stream.seek(offset)
        raw = stream.read(size)
    if len(raw) != size:
        raise ValueError('truncated capture window')
    return raw


def verify_window_signal(raw, window):
    if window not in WINDOW_TONES or len(raw) != WINDOW_BYTES:
        raise ValueError('exact bounded long-only waveform window required')
    samples = array.array('h', raw)
    if sys.byteorder != 'little':
        samples.byteswap()
    n = 4800
    first, second = WINDOW_TONES[window]
    waves = {f: [(math.cos(2 * math.pi * f * i / 48000),
                  math.sin(2 * math.pi * f * i / 48000)) for i in range(n)]
             for f in (first, second)}
    stage, count = 0, 0
    required = [(first, 8), (0, 4), (second, 8)]
    for offset in range(0, len(samples) - n + 1, n):
        frame = samples[offset:offset + n]
        rms = math.sqrt(sum(x * x for x in frame) / n)
        amps = {f: 2 * math.hypot(sum(x * c for x, (c, _) in zip(frame, wave)),
                                sum(x * s for x, (_, s) in zip(frame, wave))) / n
                for f, wave in waves.items()}
        label = 0 if rms < 100 else -1
        for frequency, other in ((first, second), (second, first)):
            if 1000 < rms < 16000 and amps[frequency] > 3000 and amps[other] < amps[frequency] * .15:
                label = frequency
        wanted, minimum = required[stage]
        count = count + 1 if label == wanted else 0
        if count >= minimum:
            stage += 1
            count = 0
            if stage == len(required):
                return
    raise ValueError('distinct late ordered tones absent')


def execution_marker(scenario, endpoint_mode):
    profile(scenario, endpoint_mode)
    return short.execution_marker(scenario, endpoint_mode).replace(
        'capyio_secure_audio_console=', 'capyio_secure_audio_console_long=')


def result_marker(scenario, endpoint_mode):
    profile(scenario, endpoint_mode)
    return (f'capyio_pipewire_secure_console_long=passed scenario={scenario}'
            f' endpoint_mode={endpoint_mode} profile=pinned_tls13_mono_300s_v1'
            ' requested_seconds=300 surface=production_console capture=synthetic'
            ' real_graph=true late_after_60s=true'
            + (' near_original_expiry=true original_expiry_observed=true' if scenario == 'expiry'
               else ' stop_after_75s=true')
            + ' post_terminal_silence=true owned_cleanup=true')


def verify_execution(stdout, stderr, scenario, endpoint_mode):
    # Use the unchanged strict execution oracle after exact long-only mapping.
    profile(scenario, endpoint_mode)
    if short.TEST in stdout or 'capyio_secure_audio_console=' in stderr:
        raise ValueError('short test cannot substitute long evidence')
    short.verify_execution(stdout.replace(TEST, short.TEST), stderr.replace(
        'capyio_secure_audio_console_long=', 'capyio_secure_audio_console='), scenario, endpoint_mode)


class LongConsoleHarness(short.SecureConsoleHarness):
    def __init__(self, root, endpoint_mode):
        super().__init__(root, endpoint_mode)
        self.deadline = time.monotonic() + OUTER_SECONDS

    def check(self):
        if time.monotonic() >= self.deadline:
            raise TimeoutError('finite long console outer deadline')
        for path in self.root.iterdir():
            limit = CAPTURE_LIMIT if path.name == 'capture.out' else LOG_LIMIT
            if path.is_file() and path.stat().st_size > limit:
                raise ValueError('bounded long console output exceeded')

    def spawn(self, args, name):
        limit = CAPTURE_LIMIT if name == 'capture' else LOG_LIMIT
        out = open(self.root / (name + '.out'), 'wb')
        err = open(self.root / (name + '.err'), 'wb')
        self.files.extend([out, err])
        child = subprocess.Popen(args, env=self.env, stdin=subprocess.DEVNULL,
                                 stdout=out, stderr=err, start_new_session=True,
                                 preexec_fn=lambda: resource.setrlimit(resource.RLIMIT_FSIZE, (limit, limit)))
        self.children.append(child)
        return child

    def checkpoint(self, producer, name):
        seconds = {'late': 75, 'final': 240, 'stopped': 15}.get(name, 10)
        ready = time.monotonic() + seconds
        path = self.root / (PREFIX + name + '.json')
        while not path.exists():
            self.check()
            if producer.poll() is not None or time.monotonic() >= ready:
                raise ValueError('finite long console checkpoint absent: ' + name)
            time.sleep(.02)
        with path.open('rb') as stream:
            return strict_json(stream.read(4097))

    def observe_pending(self, producer, baseline):
        offset = (self.root / 'capture.out').stat().st_size
        time.sleep(.4)
        self.check()
        if producer.poll() is not None:
            raise ValueError('long console exited before consent observation')
        verify_console_owner(self.owners)
        secure.no_children(self.owners['owner_pid'])
        objects = self.dump()
        secure.no_playback(objects)
        if virtual_nodes(objects) or graph_fingerprint(objects) != baseline:
            raise ValueError('long console graph changed before both approvals')
        verify_silence(read_window(self.root / 'capture.out', offset, 19200), minimum_samples=4800)

    def run_long(self, fixture, scenario):
        if scenario not in SCENARIOS:
            raise ValueError('unsupported secure console scenario')
        self.env.update(CAPYIO_SECURE_CONSOLE_LONG_HOSTED='1',
                        CAPYIO_SECURE_CONSOLE_LONG_SCENARIO=scenario,
                        CAPYIO_SECURE_CONSOLE_LONG_ENDPOINT_MODE=self.endpoint_mode)
        self.prepare_daemon()
        virtual = self.endpoint_mode == 'virtual_microphone'
        # A private always-processing recorder emits silence while unlinked.
        # Virtual mode never connects the selected-target null-sink monitor.
        properties = recorder_properties(self.endpoint_mode)
        record = self.spawn(['/usr/bin/pw-cat', '--record', '--rate=48000', '--channels=1',
                             '--format=s16', '--channel-map=mono', '--latency=20ms', '--target=0',
                             '--properties=' + properties, '-'], 'capture')
        producer = self.spawn([str(fixture), '--ignored', '--exact', TEST,
                               '--nocapture', '--test-threads=1'], 'producer')
        self.fixture_pid = producer.pid
        pending = self.checkpoint(producer, 'before_local')
        self.owners = process_ids(pending) | endpoint_identity(pending, self.endpoint_mode)
        if self.owners['fixture_pid'] != producer.pid:
            raise ValueError('secure console evidence names a different fixture')
        validate_pending(pending, scenario, 'before_local', self.owners)
        verify_console_owner(self.owners)
        self.console_chain_verified = True
        ready = time.monotonic() + 5
        while True:
            self.check()
            objects = self.dump()
            secure.no_playback(objects)
            secure.no_children(self.owners['owner_pid'])
            try:
                if virtual:
                    if virtual_nodes(objects):
                        raise ValueError("virtual Source appeared before consent")
                    recorder_port(objects)
                    monitor_links = []
                else:
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
        if scenario not in NEGATIVE_SCENARIOS:
            pending = self.checkpoint(producer, 'before_source')
            validate_pending(pending, scenario, 'before_source', self.owners)
            self.observe_pending(producer, baseline)
            publish_marker(self.root / (PREFIX + 'source_checked'))
        bound = None
        owned = {}
        if scenario not in NEGATIVE_SCENARIOS:
            bound = self.checkpoint(producer, 'bound')
            validate_bound(bound, scenario, self.owners)
            ready = time.monotonic() + 5
            while True:
                self.check()
                objects = self.dump()
                try:
                    links, owned_ids = select_console_graph(
                        objects, self.owners, bound, self.endpoint_mode)
                    break
                except ValueError:
                    if time.monotonic() >= ready or producer.poll() is not None:
                        raise
                    time.sleep(.02)
            for source, target in links:
                if (source, target) not in monitor_links:
                    self.command(['/usr/bin/pw-link', str(source), str(target)])
            objects = self.dump()
            verify_links(objects, links, self.endpoint_mode)
            owned_ids.update(o['id'] for o in objects if o.get('type') == 'PipeWire:Interface:Link'
                             and (o.get('info', {}).get('output-port-id') in owned_ids
                                  or o.get('info', {}).get('input-port-id') in owned_ids))
            current = graph_fingerprint(objects)
            owned = {key: value for key, value in current.items() if key in owned_ids}
            if any(current.get(key) != value for key, value in baseline.items()):
                raise ValueError('secure console changed an unrelated graph object')
            publish_marker(self.root / (PREFIX + 'links_ready'))
        windows = []
        initial_time = None
        initial_offset = None
        for window in ('initial', 'late') + (('final',) if scenario == 'expiry' else ()):
            evidence = self.checkpoint(producer, window)
            validate_window(evidence, scenario, window, self.owners, bound)
            observed = time.monotonic()
            offset = (self.root / 'capture.out').stat().st_size
            if initial_time is None:
                initial_time, initial_offset = observed, offset
            else:
                verify_timeline(window, observed - initial_time, offset - initial_offset)
            verify_console_owner(self.owners)
            objects = self.dump()
            verify_links(objects, links, self.endpoint_mode)
            current = graph_fingerprint(objects)
            if any(current.get(key) != value for key, value in owned.items()):
                raise ValueError('original owned graph identity changed')
            offset = (self.root / 'capture.out').stat().st_size
            windows.append((window, offset))
            publish_marker(self.root / (PREFIX + window + '_ready'))
        result = self.checkpoint(producer, 'stopped')
        validate_result(result, scenario, self.owners, bound)
        for _ in range(3):
            self.check()
            objects = self.dump()
            current = graph_fingerprint(objects)
            secure.no_playback(objects)
            if virtual:
                verify_virtual_removed(objects, owned, baseline)
            verify_reaped(self.owners, bound)
            if (any(current.get(key) == value for key, value in owned.items())
                    or any(current.get(key) != value for key, value in baseline.items())):
                raise ValueError('secure console graph survived or unrelated graph changed')
            time.sleep(.1)
        offset = (self.root / 'capture.out').stat().st_size
        time.sleep(.5)
        self.check()
        verify_silence(read_window(self.root / 'capture.out', offset, 38400), minimum_samples=14400)
        for window, offset in windows:
            verify_window_signal(read_window(self.root / 'capture.out', offset, WINDOW_BYTES), window)
        publish_marker(self.root / (PREFIX + 'stop_checked'))
        while producer.poll() is None:
            self.check()
            time.sleep(.02)
        if producer.returncode:
            raise ValueError('secure console Rust fixture failed')
        verify_execution((self.root / 'producer.out').read_text(),
                         (self.root / 'producer.err').read_text(), scenario, self.endpoint_mode)
        self.stop(record)
        self.children.remove(record)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--hosted-private-secure-console-long', action='store_true', required=True)
    parser.add_argument('--fixture', type=Path, required=True)
    parser.add_argument('--scenario', choices=SCENARIOS, required=True)
    parser.add_argument('--endpoint-mode', choices=ENDPOINT_MODES, default='playback')
    args = parser.parse_args()
    profile(args.scenario, args.endpoint_mode)
    if os.environ.get('GITHUB_ACTIONS') != 'true':
        parser.error('requires explicit disposable hosted CI environment')
    for tool in ('pipewire', 'pw-cat', 'pw-dump', 'pw-link', 'dbus-daemon'):
        if not os.access('/usr/bin/' + tool, os.X_OK):
            parser.error('required official private graph/Secret Service tool absent')
    for sig in (signal.SIGTERM, signal.SIGINT):
        signal.signal(sig, interrupted)
    with tempfile.TemporaryDirectory(prefix='capyio-secure-console-long-') as temporary:
        root = Path(temporary)
        root.chmod(0o700)
        harness = LongConsoleHarness(root, args.endpoint_mode)
        try:
            harness.run_long(args.fixture.resolve(strict=True), args.scenario)
        finally:
            for sig in (signal.SIGTERM, signal.SIGINT):
                signal.signal(sig, signal.SIG_IGN)
            harness.close()
    print(result_marker(args.scenario, args.endpoint_mode))


if __name__ == '__main__':
    main()
