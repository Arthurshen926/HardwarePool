#!/usr/bin/env python3
"""Hosted real PTY console -> paired Node -> pinned TLS -> private PipeWire.

Only synthetic identities, a private Secret Service fixture and synthetic PCM
are used. This adds a production-console gate without changing either the old
secure API gate or the legacy PCM/graph oracle. It is not physical audio proof.
"""
import argparse
import os
from pathlib import Path
import re
import signal
import sys
import tempfile
import time

import run_pipewire_secure_audio as secure
from run_pipewire_loopback import (
    Harness, graph_fingerprint, interrupted, publish_marker, select_ports,
    verify_playback_process_owner, verify_signal, verify_silence,
)

SCENARIOS = ('stop', 'local_eof', 'no_approval', 'pretyped', 'wrong_request', 'wrong_route',
             'wrong_peer', 'source_revoke', 'expiry', 'control_eof', 'media_eof')
NEGATIVE_SCENARIOS = frozenset(('no_approval', 'pretyped', 'wrong_request',
                                'wrong_route', 'wrong_peer'))
TEST = 'secure_console::hosted_secure_audio_console_graph'
FORMAT = secure.FORMAT
PREFIX = '.capyio-secure-console-'
PROCESS_FIELDS = frozenset(('fixture_pid', 'console_pid', 'owner_pid'))
CLEANUP_FIELDS = frozenset(('console_reaped', 'node_reaped', 'protected_read',
                           'secret_redacted'))
TERMINAL_CAUSES = {scenario: frozenset(('not_started',)) for scenario in NEGATIVE_SCENARIOS} | {
    'stop': frozenset(('local_stop',)),
    'source_revoke': frozenset(('media_closed',)),
    'media_eof': frozenset(('media_closed',)),
    'control_eof': frozenset(('control_closed',)),
    # At the original authority boundary the receiver may observe its deadline
    # or the sender closing first. The unchanged oracle still requires the
    # exact original Source expiry, with no early allowance for any cause.
    'expiry': frozenset(('failure', 'media_timeout', 'media_closed')),
}
COMMON = secure.COMMON | {'console_pid', 'fixture_pid'}
strict_json = secure.strict_json


def process_ids(value):
    """Never alias the fixture, controlling terminal and actual Node owner."""
    result = {key: secure.integer(value.get(key), 2) for key in PROCESS_FIELDS}
    if max(result.values()) > (1 << 31) - 1 or len(set(result.values())) != 3:
        raise ValueError('distinct bounded fixture, console and Node identities required')
    return result


def identity(value, scenario, stage, owners):
    if (scenario not in SCENARIOS or value.get('scenario') != scenario
            or value.get('stage') != stage or value.get('surface') != 'production_console'
            or value.get('format') != FORMAT or type(value.get('version')) is not int
            or value['version'] != 1 or process_ids(value) != process_ids(owners)):
        raise ValueError('secure console evidence identity mismatch')


def api_evidence(value):
    """Project into the unchanged secure API oracle, after console validation.

    All five terminal-consent denials share the old no-authority invariant.
    Only the validated console identity, cleanup and terminal-cause fields are removed; unknown fields
    remain present so the old exact-key checks still reject them.
    """
    result = {key: item for key, item in value.items()
              if key not in CLEANUP_FIELDS | {'console_pid', 'fixture_pid', 'terminal_cause'}}
    result['surface'] = 'reusable_rust_apis'
    if value['scenario'] in NEGATIVE_SCENARIOS:
        result['scenario'] = 'no_authority'
        for key in ('injected', 'verified'):
            if key in result:
                result[key] = 'no_authority' if result[key] == value['scenario'] else result[key]
    elif value['scenario'] == 'local_eof':
        # Only Pending/Bound use this projection. The terminal-EOF result has
        # deliberately different snapshot semantics and its own validator.
        if value.get('stage') == 'stopped':
            raise ValueError('local input EOF must not use the terminal-counter oracle')
        result['scenario'] = 'stop'
    return result


def validate_pending(value, scenario, stage, owners):
    identity(value, scenario, stage, owners)
    if (stage not in ('before_local', 'before_source')
            or stage == 'before_source' and scenario in NEGATIVE_SCENARIOS
            or set(value) != COMMON | {'helper_absent', 'lease_count', 'bytes_written'}):
        raise ValueError('unexpected secure console pending checkpoint')
    projected = api_evidence(value)
    secure.validate_pending(projected, projected['scenario'], stage, owners['owner_pid'])
    return value


def validate_bound(value, scenario, owners):
    identity(value, scenario, 'bound', owners)
    if set(value) != COMMON | {'helper_pid', 'source_lease_expires_at_ms'}:
        raise ValueError('unexpected secure console bound evidence')
    if value['helper_pid'] in process_ids(owners).values():
        raise ValueError('secure console helper must be a distinct Node child')
    projected = api_evidence(value)
    secure.validate_bound(projected, projected['scenario'], owners['owner_pid'])
    if value['helper_pid'] > (1 << 31) - 1:
        raise ValueError('secure console helper PID out of range')
    return value


def validate_result(value, scenario, owners, bound=None):
    identity(value, scenario, 'stopped', owners)
    if scenario == 'local_eof':
        return validate_local_eof_result(value, owners, bound)
    if (any(value.get(key) is not True for key in CLEANUP_FIELDS)
            or value.get('injected') != scenario or value.get('verified') != scenario
            or not isinstance(value.get('terminal_cause'), str)
            or value['terminal_cause'] not in TERMINAL_CAUSES[scenario]):
        raise ValueError('secure console terminal cause, protected read or cleanup unverified')
    if bound is not None:
        validate_bound(bound, scenario, owners)
    projected = api_evidence(value)
    secure.validate_result(projected, projected['scenario'], owners['owner_pid'],
                           None if bound is None else api_evidence(bound))
    return value


def validate_local_eof_result(value, owners, bound):
    """Ctrl-D abort is not an invented graceful Stop or owner terminal sample.

    The controller unwinds via best-effort Stop/EOF. Preserve the last actual
    status as a labeled pre-input-EOF snapshot and prove Source control closure
    and graph/process cleanup independently. This is neither a graceful Stop
    nor a media-EOF claim; media_eof separately proves typed receiver closure.
    """
    identity(value, 'local_eof', 'stopped', owners)
    fields = {'helper_pid', 'source_lease_expires_at_ms', 'terminal_observed_at_ms',
              'terminal_cause', 'counter_observation', 'status_packets_written',
              'status_bytes_written', 'status_observed_at_ms', 'input_eof_at_ms',
              'source_control_closed_observed_at_ms', 'source_control_closed',
              'source_records_sent', 'source_records_attempted', 'injected', 'verified',
              'session_closed', 'leases_released', 'helper_reaped'} | CLEANUP_FIELDS
    if (set(value) != COMMON | fields or value.get('injected') != 'local_eof'
            or value.get('verified') != 'local_eof'
            or value.get('terminal_observed_at_ms') is not None
            or value.get('terminal_cause') is not None
            or value.get('counter_observation') != 'pre_input_eof_status'
            or any(value.get(key) is not True for key in CLEANUP_FIELDS | {
                'session_closed', 'leases_released', 'helper_reaped',
                'source_control_closed'})):
        raise ValueError('local input EOF needs exact snapshot, Source control closure and cleanup evidence')
    if bound is None:
        raise ValueError('local input EOF activation evidence absent')
    validate_bound(bound, 'local_eof', owners)
    if any(value[key] != bound[key] for key in ('helper_pid', 'source_lease_expires_at_ms')):
        raise ValueError('local input EOF changed the helper or original Source lease')
    for key in ('status_packets_written', 'status_bytes_written', 'status_observed_at_ms',
                'input_eof_at_ms', 'source_control_closed_observed_at_ms',
                'source_records_sent', 'source_records_attempted'):
        secure.integer(value[key], 1)
    if (value['status_packets_written'] < 300
            or value['status_bytes_written'] != value['status_packets_written'] * 960
            or value['source_records_sent'] < value['status_packets_written']
            or value['source_records_attempted'] != value['source_records_sent']
            or not value['status_observed_at_ms'] <= value['input_eof_at_ms']
                   <= value['source_control_closed_observed_at_ms'] < bound['source_lease_expires_at_ms']):
        raise ValueError('local input EOF snapshot, closure accounting or original deadline mismatch')
    return value


def execution_marker(scenario):
    if scenario not in SCENARIOS:
        raise ValueError('unsupported secure console scenario')
    return (f'capyio_secure_audio_console=passed scenario={scenario}'
            ' actual_tty=true actual_node=true fresh_consent=true protected_read=true'
            ' secret_redacted=true node_reaped=true')


def verify_execution(stdout, stderr, scenario):
    test_lines = [line for line in stdout.splitlines() if line.startswith('test ')
                  and not line.startswith('test result:')]
    summaries = [line for line in stdout.splitlines() if line.startswith('test result:')]
    markers = [line for line in stderr.splitlines()
               if line.startswith('capyio_secure_audio_console')]
    if (test_lines != [f'test {TEST} ... ok'] or len(summaries) != 1
            or re.fullmatch(r'test result: ok\. 1 passed; 0 failed; 0 ignored;'
                            r'(?: \d+ measured; \d+ filtered out; finished in [0-9.]+s)?',
                            summaries[0]) is None
            or markers != [execution_marker(scenario)]):
        raise ValueError('exact production console test and mandatory marker must execute once')


def process_parent(pid):
    fields = dict(line.split(':', 1) for line in
                  (Path('/proc', str(pid)) / 'status').read_text().splitlines() if ':' in line)
    return int(fields['PPid'])


def verify_console_owner(owners):
    """Independently inspect the fixture -> fixed console -> fixed Node chain."""
    owners = process_ids(owners)
    console = Path('/proc', str(owners['console_pid']))
    node = Path('/proc', str(owners['owner_pid']))
    console_exe = (console / 'exe').resolve(strict=True)
    node_exe = (node / 'exe').resolve(strict=True)
    if (console_exe.name != 'capyio-linux-console' or node_exe.name != 'capyio-node'
            or console_exe.parent != node_exe.parent
            or process_parent(owners['console_pid']) != owners['fixture_pid']
            or process_parent(owners['owner_pid']) != owners['console_pid']
            or os.getpgid(owners['console_pid']) != owners['console_pid']
            or os.getpgid(owners['owner_pid']) != owners['console_pid']):
        raise ValueError('actual owned console and fixed sibling Node required')
    for process, command in ((console, b'playback'), (node, b'linux-playback')):
        args = (process / 'cmdline').read_bytes().rstrip(b'\0').split(b'\0')
        if command not in args or b'--pairing-state' not in args:
            raise ValueError('protected production playback invocation required')
    return owners


def verify_reaped(owners, bound):
    # The fixture's private dbus daemon is deliberately still alive here. It is
    # not an audio child, and dies when the fixture completes after stop_checked.
    pids = [owners['console_pid'], owners['owner_pid']]
    if bound is not None:
        pids.append(bound['helper_pid'])
    if any(Path('/proc', str(pid)).exists() for pid in pids):
        raise ValueError('actual secure console, Node or playback helper survived cleanup')


class SecureConsoleHarness(Harness):
    def __init__(self, root):
        super().__init__(root, e2e=True)
        self.owners = None
        self.console_chain_verified = False
        self.fixture_pid = None

    def checkpoint(self, producer, name):
        self.wait_checkpoint(producer, PREFIX + name + '.json')
        return strict_json((self.root / (PREFIX + name + '.json')).read_bytes())

    def observe_pending(self, producer, baseline):
        offset = (self.root / 'capture.out').stat().st_size
        time.sleep(.4)
        self.check()
        if producer.poll() is not None:
            raise ValueError('secure console fixture exited before consent observation')
        verify_console_owner(self.owners)
        secure.no_children(self.owners['owner_pid'])
        objects = self.dump()
        secure.no_playback(objects)
        if graph_fingerprint(objects) != baseline:
            raise ValueError('secure console changed graph before both grants and leases')
        verify_silence((self.root / 'capture.out').read_bytes()[offset:], minimum_samples=4800)

    def run_secure(self, fixture, scenario):
        if scenario not in SCENARIOS:
            raise ValueError('unsupported secure console scenario')
        self.env.update(CAPYIO_SECURE_CONSOLE_HOSTED='1',
                        CAPYIO_SECURE_CONSOLE_SCENARIO=scenario)
        self.prepare_daemon()
        record = self.spawn(['/usr/bin/pw-cat', '--record', '--rate=48000', '--channels=1',
                             '--format=s16', '--channel-map=mono', '--latency=20ms', '--target=0',
                             '--properties={"node.name":"capyio.test.monitor"}', '-'], 'capture')
        producer = self.spawn([str(fixture), '--ignored', '--exact', TEST,
                               '--nocapture', '--test-threads=1'], 'producer')
        self.fixture_pid = producer.pid
        pending = self.checkpoint(producer, 'before_local')
        self.owners = process_ids(pending)
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
                    links = select_ports(objects)
                    verify_console_owner(self.owners)
                    helper, owned_ids = verify_playback_process_owner(objects, self.owners['owner_pid'])
                    if helper != bound['helper_pid'] or os.getpgid(helper) != self.owners['console_pid']:
                        raise ValueError('graph is not the exact console-owned Node helper')
                    break
                except ValueError:
                    if time.monotonic() >= ready or producer.poll() is not None:
                        raise
                    time.sleep(.02)
            for source, target in links:
                if (source, target) not in monitor_links:
                    self.command(['/usr/bin/pw-link', str(source), str(target)])
            objects = self.dump()
            actual = {(o.get('info', {}).get('output-port-id'), o.get('info', {}).get('input-port-id'))
                      for o in objects if o.get('type') == 'PipeWire:Interface:Link'}
            if not set(links).issubset(actual):
                raise ValueError('explicit secure console playback links absent')
            owned_ids.update(o['id'] for o in objects if o.get('type') == 'PipeWire:Interface:Link'
                             and (o.get('info', {}).get('output-port-id') in owned_ids
                                  or o.get('info', {}).get('input-port-id') in owned_ids))
            current = graph_fingerprint(objects)
            owned = {key: value for key, value in current.items() if key in owned_ids}
            if any(current.get(key) != value for key, value in baseline.items()):
                raise ValueError('secure console changed an unrelated graph object')
            publish_marker(self.root / (PREFIX + 'links_ready'))
        result = self.checkpoint(producer, 'stopped')
        validate_result(result, scenario, self.owners, bound)
        for _ in range(3):
            self.check()
            objects = self.dump()
            current = graph_fingerprint(objects)
            secure.no_playback(objects)
            verify_reaped(self.owners, bound)
            if (any(current.get(key) == value for key, value in owned.items())
                    or any(current.get(key) != value for key, value in baseline.items())):
                raise ValueError('secure console graph survived or unrelated graph changed')
            time.sleep(.1)
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
            raise ValueError('secure console Rust fixture failed')
        verify_execution((self.root / 'producer.out').read_text(),
                         (self.root / 'producer.err').read_text(), scenario)
        self.stop(record)
        self.children.remove(record)

    def cleanup_console(self):
        """Stop only the owned PTY session, before killing its fixture/DBus."""
        path = self.root / '.capyio-console-start.json'
        if not path.is_file():
            return
        value = strict_json(path.read_bytes())
        if set(value) != {'console_pid'}:
            raise ValueError('invalid owned console emergency identity')
        pid = secure.integer(value['console_pid'], 2)
        if pid > (1 << 31) - 1:
            raise ValueError('console cleanup PID out of range')
        process = Path('/proc', str(pid))
        if self.owners and self.owners['console_pid'] != pid:
            raise ValueError('console emergency identity differs from observed owner')
        if process.exists():
            if ((process / 'exe').resolve(strict=True).name != 'capyio-linux-console'
                    or (process_parent(pid) != self.fixture_pid
                        and not self.console_chain_verified)
                    or os.getpgid(pid) != pid):
                raise ValueError('console emergency cleanup identity changed')
        elif not self.console_chain_verified:
            # Without a live process or previously verified chain, never signal
            # an arbitrary group using only an unverified stale numeric marker.
            return
        try:
            os.killpg(pid, signal.SIGTERM)
        except ProcessLookupError:
            return
        until = time.monotonic() + 3
        while time.monotonic() < until:
            try:
                os.killpg(pid, 0)
            except ProcessLookupError:
                return
            time.sleep(.02)
        try:
            os.killpg(pid, signal.SIGKILL)
        except ProcessLookupError:
            return
        raise RuntimeError('owned console group required forced cleanup')

    def close(self):
        try:
            self.cleanup_console()
        finally:
            super().close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--hosted-private-secure-console', action='store_true', required=True)
    parser.add_argument('--fixture', type=Path, required=True)
    parser.add_argument('--scenario', choices=SCENARIOS, required=True)
    args = parser.parse_args()
    if os.environ.get('GITHUB_ACTIONS') != 'true':
        parser.error('requires explicit disposable hosted CI environment')
    for tool in ('pipewire', 'pw-cat', 'pw-dump', 'pw-link', 'dbus-daemon'):
        if not os.access('/usr/bin/' + tool, os.X_OK):
            parser.error('required official private graph/Secret Service tool absent')
    for sig in (signal.SIGTERM, signal.SIGINT):
        signal.signal(sig, interrupted)
    with tempfile.TemporaryDirectory(prefix='capyio-secure-console-') as temporary:
        root = Path(temporary)
        root.chmod(0o700)
        harness = SecureConsoleHarness(root)
        try:
            harness.run_secure(args.fixture.resolve(strict=True), args.scenario)
        finally:
            for sig in (signal.SIGTERM, signal.SIGINT):
                signal.signal(sig, signal.SIG_IGN)
            harness.close()
    print(f'capyio_pipewire_secure_console=passed scenario={args.scenario}'
          ' surface=production_console capture=synthetic real_graph=true owned_cleanup=true')


if __name__ == '__main__':
    main()
