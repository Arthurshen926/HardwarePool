#!/usr/bin/env python3
"""Hosted private PipeWire capture -> real pinned TLS -> Android Rust Sink APIs.

Synthetic graph and portable Sink only: no physical device, AudioTrack, Node CLI,
paired store, session bus, user PipeWire server, or audibility claim.
"""
import argparse
import json
import os
from pathlib import Path
import signal
import sys
import tempfile
import time

from run_pipewire_capture import (
    CaptureHarness, SOURCE, capture_nodes, capture_process_id, parse_pipewire_version,
    pipewire_binary_identity, select_capture, source_process_id, verify_capture_removed,
    verify_stereo,
)
from run_pipewire_loopback import graph_fingerprint, interrupted

SCENARIOS = ('waveform', 'stop', 'source_revoke', 'sink_revoke', 'control_eof',
             'media_eof', 'expiry', 'backpressure', 'partial_write')
TEST = 'hosted_secure_stereo_linux_capture_android_sink'
FORMAT = 'pcm-s16le-48000-stereo-tls13-v1'
PREFIX = '.capyio-secure-stereo-'
COMMON = {'version', 'scenario', 'stage', 'format', 'surface', 'owner_pid'}
BOUND_FIELDS = {'helper_pid', 'source_lease_expires_at_ms', 'sink_lease_expires_at_ms',
                'source_consent_end_ms', 'sink_consent_end_ms', 'grant_expires_at_ms'}
CAUSES = {'waveform': ('source', 'local_stop'), 'stop': ('sink', 'peer_stop'),
          'source_revoke': ('source', 'source_revoked'),
          'sink_revoke': ('sink', 'sink_lease_changed'),
          'control_eof': ('sink', 'control_closed'), 'media_eof': ('sink', 'media_closed'),
          'expiry': ('source', 'expired'),
          'backpressure': ('sink', 'sink_backpressure'),
          'partial_write': ('source', 'local_stop')}


def strict_json(raw):
    if not isinstance(raw, bytes) or len(raw) > 8192:
        raise ValueError('bounded stereo JSON evidence required')
    def pairs(items):
        value = {}
        for key, item in items:
            if key in value:
                raise ValueError('duplicate stereo evidence field')
            value[key] = item
        return value
    value = json.loads(raw, object_pairs_hook=pairs)
    if not isinstance(value, dict):
        raise ValueError('stereo evidence object required')
    return value


def integer(value, minimum=0):
    if type(value) is not int or not minimum <= value <= (1 << 63) - 1:
        raise ValueError('invalid bounded stereo evidence integer')
    return value


def identity(value, scenario, stage, owner_pid):
    if (scenario not in SCENARIOS or value.get('scenario') != scenario
            or type(value.get('version')) is not int or value['version'] != 1
            or value.get('stage') != stage or value.get('format') != FORMAT
            or value.get('surface') != 'reusable_rust_apis'
            or integer(value.get('owner_pid'), 2) != owner_pid):
        raise ValueError('stereo evidence identity mismatch')


def validate_pending(value, scenario, stage, owner_pid):
    identity(value, scenario, stage, owner_pid)
    if (stage not in ('before_source', 'before_sink')
            or set(value) != COMMON | {'helper_absent', 'lease_count', 'pcm_bytes_accepted'}
            or value['helper_absent'] is not True
            or integer(value['lease_count']) != 0
            or integer(value['pcm_bytes_accepted']) != 0):
        raise ValueError('stereo capture preceded independent consent')
    return value


def validate_bound(value, scenario, owner_pid):
    identity(value, scenario, 'bound', owner_pid)
    if set(value) != COMMON | BOUND_FIELDS:
        raise ValueError('unexpected stereo bound fields')
    if integer(value['helper_pid'], 2) == owner_pid:
        raise ValueError('stereo helper must be a distinct owned child')
    for key in BOUND_FIELDS - {'helper_pid'}:
        integer(value[key], 1)
    cap = min(value['source_consent_end_ms'], value['sink_consent_end_ms'],
              value['grant_expires_at_ms'])
    if any(value[key] > cap for key in ('source_lease_expires_at_ms', 'sink_lease_expires_at_ms')):
        raise ValueError('stereo claim exceeded original consent or grant')
    if scenario == 'expiry' and cap >= value['grant_expires_at_ms']:
        raise ValueError('expiry must exercise original consent, not just later grant')
    return value



def validate_pcm_enabled(value, scenario, owner_pid, bound):
    identity(value, scenario, 'pcm_enabled', owner_pid)
    validate_bound(bound, scenario, owner_pid)
    if (set(value) != COMMON | {'helper_pid', 'records_admitted', 'pcm_bytes_accepted'}
            or integer(value['helper_pid'], 2) != bound['helper_pid']
            or integer(value['records_admitted']) != 0
            or integer(value['pcm_bytes_accepted']) != 0):
        raise ValueError('first stereo read permit requires exact helper and no prior PCM')
    return value

def validate_result(value, scenario, owner_pid, bound):
    identity(value, scenario, 'stopped', owner_pid)
    fields = {'terminal_endpoint', 'terminal_cause', 'terminal_observed_at_ms', 'injected_at_ms',
              'packets_captured', 'capture_bytes_read', 'source_records_sent',
              'records_admitted', 'records_accepted', 'pcm_bytes_accepted', 'sink_write_calls',
              'source_session_closed', 'sink_session_closed', 'leases_released',
              'helper_reaped', 'sink_stopped', 'post_stop_unchanged'}
    if set(value) != COMMON | BOUND_FIELDS | fields:
        raise ValueError('unexpected stereo final evidence fields')
    validate_bound(bound, scenario, owner_pid)
    if any(value[key] != bound[key] for key in BOUND_FIELDS):
        raise ValueError('stereo original helper, claims or consent changed')
    if (value['terminal_endpoint'], value['terminal_cause']) != CAUSES[scenario]:
        raise ValueError('requested stereo terminal cause not observed')
    for key in fields - {'terminal_endpoint', 'terminal_cause', 'source_session_closed',
                         'sink_session_closed', 'leases_released', 'helper_reaped',
                         'sink_stopped', 'post_stop_unchanged'}:
        integer(value[key])
    if any(value[key] is not True for key in ('source_session_closed', 'sink_session_closed',
           'leases_released', 'helper_reaped', 'sink_stopped', 'post_stop_unchanged')):
        raise ValueError('stereo terminal cleanup not proven')
    observed = value['terminal_observed_at_ms']
    expiry = min(bound[key] for key in BOUND_FIELDS - {'helper_pid'})
    if observed < value['injected_at_ms']:
        raise ValueError('terminal observation preceded requested injection')
    if scenario == 'expiry':
        if observed < expiry:
            raise ValueError('expiry occurred before exact earliest original claim')
    elif observed >= expiry:
        raise ValueError('incidental expiry cannot prove the requested stereo cause')
    # The Source has one fixed outstanding TLS write. A receiver may admit that
    # record before its completion event is consumed; this is an explicit
    # single-permit bound, not timing/PCM tolerance or invented delivered bytes.
    if (not 1 <= value['records_accepted'] <= value['records_admitted'] <= value['source_records_sent'] + 1
            or value['source_records_sent'] > value['packets_captured']
            or not value['packets_captured'] * 1920 <= value['capture_bytes_read']
                    < (value['packets_captured'] + 1) * 1920
            or not value['records_accepted'] * 1920 <= value['pcm_bytes_accepted']
                    <= value['records_admitted'] * 1920
            or value['sink_write_calls'] < value['records_accepted']):
        raise ValueError('actual bounded stereo record/PCM accounting absent')
    if scenario in ('waveform', 'partial_write') and value['records_accepted'] < 600:
        raise ValueError('complete ordered stereo waveform window absent')
    if scenario == 'partial_write' and value['sink_write_calls'] < 2 * value['records_accepted']:
        raise ValueError('aligned partial Sink writes were not exercised')
    if scenario == 'backpressure' and value['records_admitted'] != value['records_accepted'] + 1:
        raise ValueError('backpressure must retain exactly one bounded pending record')
    return value


def verify_execution(raw):
    if (f'test {TEST} ... ok' not in raw
            or 'test result: ok. 1 passed; 0 failed; 0 ignored;' not in raw):
        raise ValueError('exact hosted stereo case must execute, not skip')


class SecureStereoHarness(CaptureHarness):
    def __init__(self, root, scenario):
        if scenario not in SCENARIOS:
            raise ValueError('explicit secure stereo scenario required')
        # Reuse only the existing private graph/tool/process primitives. The old
        # capture fixture and every old acceptance scenario remain unchanged.
        super().__init__(root, 'waveform')
        self.secure_scenario = scenario
        self.env.update(CAPYIO_SECURE_STEREO_HOSTED='1', CAPYIO_SECURE_STEREO_SCENARIO=scenario)

    def evidence(self, producer, stage):
        self.wait_checkpoint(producer, PREFIX + stage + '.json')
        with (self.root / (PREFIX + stage + '.json')).open('rb') as stream:
            return strict_json(stream.read(8193))

    def checkpoint(self, name):
        target = self.root / (PREFIX + name)
        temporary = target.with_suffix('.tmp')
        temporary.write_text('capyio-hosted-secure-stereo-v1\n')
        temporary.replace(target)

    def run_secure(self, fixture):
        scenario = self.secure_scenario
        binary = Path('/usr/bin/pw-cat')
        self.pipewire_identity = pipewire_binary_identity(binary)
        self.pipewire_version = parse_pipewire_version(self.command([str(binary), '--version']))
        if pipewire_binary_identity(binary) != self.pipewire_identity:
            raise ValueError('PipeWire binary changed during version probe')
        daemon = self.prepare_daemon()
        source = self.start_source()
        producer = self.spawn([str(fixture), '--ignored', '--exact', TEST,
                               '--nocapture', '--test-threads=1'], 'producer')
        self.diagnostic_processes = {'daemon': daemon.pid, 'source': source.pid,
                                     'fixture': producer.pid, 'owner': producer.pid}
        for stage, checked in (('before_source', 'source_checked'), ('before_sink', 'sink_checked')):
            validate_pending(self.evidence(producer, stage), scenario, stage, producer.pid)
            for _ in range(2):
                if capture_nodes(self.dump()):
                    raise ValueError('stereo capture appeared before both independent consents')
                time.sleep(.05)
            self.checkpoint(checked)
        bound = validate_bound(self.evidence(producer, 'bound'), scenario, producer.pid)
        ready = time.monotonic() + 5
        objects = []
        while True:
            self.check()
            if any(child.poll() is not None for child in (daemon, source, producer)):
                self.report_graph_failure(objects)
                raise ValueError('secure stereo owner exited before graph readiness')
            objects = self.dump()
            try:
                capture, links = select_capture(objects)
                break
            except ValueError:
                if time.monotonic() >= ready:
                    self.report_graph_failure(objects)
                    raise
                time.sleep(.01)
        helper = capture_process_id(objects, capture, producer.pid,
                                    self.pipewire_version, self.pipewire_identity)
        if helper != bound['helper_pid']:
            raise ValueError('graph capture is not the exact authenticated owner helper')
        app = next(item for item in objects if item.get('type') == 'PipeWire:Interface:Node'
                   and item.get('info', {}).get('props', {}).get('node.name') == SOURCE)
        if source_process_id(objects, app, {'node_name': SOURCE}) != source.pid:
            raise ValueError('stereo signal must originate in the independent owned synthetic process')
        fingerprint = graph_fingerprint(objects)
        own_ids = {capture['id']} | {target for _, target in links}
        owned = {key: value for key, value in fingerprint.items() if key in own_ids}
        unrelated = {key: value for key, value in fingerprint.items() if key not in own_ids}
        # Finish identity/port inspection before opening the first read permit.
        # Start real Sink consumption BEFORE audio can flow, so the independent
        # post-link pw-dump cannot accumulate an unconsumed startup burst.
        self.checkpoint('sink_ready')
        validate_pcm_enabled(self.evidence(producer, 'pcm_enabled'), scenario, producer.pid, bound)
        for output, target in links:
            self.command(['/usr/bin/pw-link', str(output), str(target)])
        actual_links = [item for item in self.dump() if item.get('type') == 'PipeWire:Interface:Link']
        if (len(actual_links) != 2 or {(item.get('info', {}).get('output-port-id'),
              item.get('info', {}).get('input-port-id')) for item in actual_links} != set(links)):
            raise ValueError('secure stereo requires only the two exact direct channel links')
        owned.update(graph_fingerprint(actual_links))
        self.checkpoint('links_ready')
        result = validate_result(self.evidence(producer, 'stopped'), scenario, producer.pid, bound)
        if Path('/proc', str(helper)).exists():
            raise ValueError('exact production stereo helper was not reaped')
        end = time.monotonic() + .35
        while time.monotonic() < end:
            verify_capture_removed(self.dump(), owned, unrelated)
            self.check()
            time.sleep(.02)
        if scenario in ('waveform', 'partial_write'):
            pcm = self.root / 'received-secure-stereo.pcm'
            if pcm.stat().st_size != result['pcm_bytes_accepted']:
                raise ValueError('actual independent Sink file and accepted bytes disagree')
            verify_stereo(pcm.read_bytes())
        self.checkpoint('stop_checked')
        while producer.poll() is None:
            self.check()
            time.sleep(.01)
        if producer.returncode:
            raise ValueError('secure stereo Rust lifecycle fixture failed')
        verify_execution((self.root / 'producer.out').read_text())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--hosted-pipewire-secure-stereo', action='store_true', required=True)
    parser.add_argument('--fixture', required=True, type=Path)
    parser.add_argument('--scenario', choices=SCENARIOS, required=True)
    args = parser.parse_args()
    if os.environ.get('GITHUB_ACTIONS') != 'true':
        parser.error('requires explicit disposable hosted CI environment')
    for tool in ('pipewire', 'pw-cat', 'pw-dump', 'pw-link'):
        if not os.access('/usr/bin/' + tool, os.X_OK):
            parser.error('required official PipeWire tool absent; no installation fallback')
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    with tempfile.TemporaryDirectory(prefix='capyio-secure-stereo-') as directory:
        harness = SecureStereoHarness(Path(directory), args.scenario)
        try:
            harness.run_secure(args.fixture.resolve(strict=True))
        except Exception:
            for name in ('daemon.err', 'producer.err', 'source.err', 'command.err'):
                path = Path(directory) / name
                if path.is_file():
                    with path.open('rb') as stream:
                        print(name + ': ' + stream.read(4096).decode('utf-8', errors='replace'),
                              file=sys.stderr)
            raise
        finally:
            signal.signal(signal.SIGTERM, signal.SIG_IGN)
            signal.signal(signal.SIGINT, signal.SIG_IGN)
            harness.close()
    print(f'capyio_pipewire_secure_stereo=passed scenario={args.scenario}'
          ' pinned_tls=true independent_consent=true exact_claims=true stereo=true'
          ' actual_rust_apis=true simulated_sink=true cleanup=true')


if __name__ == '__main__':
    main()
