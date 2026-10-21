#!/usr/bin/env python3
"""Actual capyio-node executable, pinned TLS and private synthetic NV12 graph.
No physical devices or inherited desktop graph. A failing fixture never passes.
"""
import argparse
import json
from pathlib import Path
import re
import sys
import tempfile
import time
from types import SimpleNamespace

from run_pipewire_camera import CameraHarness, PREFIX, camera_port, named, no_camera, owned_pid
from run_pipewire_loopback import graph_fingerprint

CONSOLE_PENDING = ('no_approval', 'pretyped', 'mismatched_request', 'mismatched_route', 'mismatched_peer')
CONSOLE_SCENARIOS = ('pixels', *CONSOLE_PENDING, 'expiry', 'remote_eof', 'local_stop',
                     'control_stop', 'source_revoke', 'partial_cavc', 'handshake_timeout')
SCENARIOS = ('pixels', 'local_stop', 'control_stop', 'control_eof',
             'source_revoke', 'partial_cavc', 'expiry', 'handshake_timeout')
ALL_SCENARIOS = tuple(dict.fromkeys((*SCENARIOS, *CONSOLE_SCENARIOS)))
CONSOLE_TEST = 'hosted_linux_camera_console_graph'
PATTERNS = {
    'PAIRED_STORE_AUTH': r'PAIRED_STORE_AUTH verified',
    'LAUNCHER_PID': r'LAUNCHER_PID ([1-9][0-9]*)',
    'SECURE_LISTENING': r'SECURE_LISTENING no_media_children=true',
    'CAMERA_READY': r'CAMERA_READY ([1-9][0-9]*) (capyio\.camera\.[A-Za-z0-9-]+)',
    'DECODER_READY': r'DECODER_READY ([1-9][0-9]*)',
    'VERIFIED': r'VERIFIED (' + '|'.join(ALL_SCENARIOS) + ')',
    'CONSOLE_PID': r'CONSOLE_PID ([1-9][0-9]*)',
    'CONSOLE_CONSENT': r'CONSOLE_CONSENT exact_source_sink=true no_echo=true',
    'CONSOLE_VERIFIED': (r'CONSOLE_VERIFIED (' + '|'.join(CONSOLE_SCENARIOS)
                         + r') actual_tty=true fresh_consent=true secret_redacted=true node_reaped=true'),
    'CAMERA_STOPPED': r'CAMERA_STOPPED',
}


def markers(text, complete=False):
    """Parse only complete lines while polling; reject malformed/duplicate claims."""
    lines = text.splitlines() if complete else text.split('\n')[:-1]
    result = {}
    for line in lines:
        for key, pattern in PATTERNS.items():
            if line.startswith(key):
                match = re.fullmatch(pattern, line)
                if match is None or key in result:
                    raise ValueError('malformed or duplicate ' + key)
                result[key] = match.groups()
    return result


def verify_process(pid, parent_pid, executable):
    if pid <= 1 or pid == parent_pid:
        raise ValueError('invalid owned process identity')
    status = dict(line.split(':', 1) for line in Path('/proc', str(pid), 'status').read_text().splitlines() if ':' in line)
    if int(status['PPid']) != parent_pid or Path('/proc', str(pid), 'exe').resolve() != executable.resolve():
        raise ValueError('owned process executable or direct parent mismatch')


def verify_mode(scenario, paired_store=False, console=False):
    if console and not paired_store:
        raise ValueError('console graph requires explicit paired-store identity')
    if scenario not in (CONSOLE_SCENARIOS if console else SCENARIOS):
        raise ValueError('scenario does not belong to the selected exact fixture mode')


def verify_completion(text, scenario, code, paired_store=False, console=False):
    verify_mode(scenario, paired_store, console)
    result = markers(text, complete=True)
    required = {'LAUNCHER_PID', 'VERIFIED', 'CAMERA_STOPPED'}
    if paired_store:
        required.add('PAIRED_STORE_AUTH')
    elif 'PAIRED_STORE_AUTH' in result:
        raise ValueError('unexpected paired-store authentication mode')
    console_keys = {'CONSOLE_PID', 'CONSOLE_CONSENT', 'CONSOLE_VERIFIED'}
    if console:
        required.update(console_keys)
        if (result.get('CONSOLE_VERIFIED') != (scenario,)
                or f'test {CONSOLE_TEST} ... ok' not in text
                or 'test result: ok. 1 passed; 0 failed; 0 ignored;' not in text):
            raise ValueError('exact actual-console test must execute successfully, never skip or substitute')
    elif console_keys.intersection(result):
        raise ValueError('unexpected console-mode evidence')
    no_media = scenario == 'handshake_timeout' or scenario in CONSOLE_PENDING
    if scenario not in CONSOLE_PENDING:
        required.add('SECURE_LISTENING')
    elif 'SECURE_LISTENING' in result:
        raise ValueError('unapproved console acquired secure media authority')
    if not no_media:
        required.update(('CAMERA_READY', 'DECODER_READY'))
    elif 'CAMERA_READY' in result or 'DECODER_READY' in result:
        raise ValueError('unapproved or unbound camera spawned media children')
    if code != 0 or not required.issubset(result) or result['VERIFIED'] != (scenario,):
        raise ValueError('fixture did not verify the exact expected terminal outcome')
    return result


class LauncherHarness(CameraHarness):
    def read_markers(self):
        return markers((self.root / 'owner.out').read_text())

    def verify_owner(self, child, result):
        node_pid = int(result['LAUNCHER_PID'][0])
        if getattr(self, 'console', False):
            console_pid = int(result['CONSOLE_PID'][0])
            if len({node_pid, console_pid, child.pid}) != 3:
                raise ValueError('fixture, console and Node require distinct identities')
            verify_process(console_pid, child.pid, self.node.parent / 'capyio-linux-console')
            verify_process(node_pid, console_pid, self.node)
            for pid, operation in ((console_pid, b'camera'), (node_pid, b'linux-camera')):
                argv = Path('/proc', str(pid), 'cmdline').read_bytes().split(b'\0')
                if operation not in argv or b'--pairing-state' not in argv:
                    raise ValueError('exact paired camera invocation required')
            self.console_pid = console_pid
        else:
            verify_process(node_pid, child.pid, self.node)
        self.node_pid = node_pid
        return node_pid

    def consent_checkpoint(self, child, baseline):
        result = self.read_markers()
        if all(key in result for key in ('LAUNCHER_PID', 'CONSOLE_PID', 'CONSOLE_CONSENT', 'PAIRED_STORE_AUTH')):
            self.verify_owner(child, result)
            if any(key in result for key in ('SECURE_LISTENING', 'CAMERA_READY', 'DECODER_READY')):
                raise ValueError('media authority before fresh terminal consent')
            # Hold the real prompt pending while independently sampling the
            # entire private graph, not just trusting a fixture success marker.
            for _ in range(3):
                objects = self.dump()
                if not no_camera(objects) or graph_fingerprint(objects) != baseline:
                    raise ValueError('unapproved console changed private graph')
                self.check()
                time.sleep(.05)
            return True
        if child.poll() is not None:
            raise ValueError('console fixture exited before fresh consent checkpoint')
        return False

    def assert_owner_removed(self):
        ids = [self.node_pid]
        if getattr(self, 'console', False):
            ids.append(self.console_pid)
        if hasattr(self, 'decoder_pid'):
            ids.append(self.decoder_pid)
        if any(Path('/proc', str(pid)).exists() for pid in ids):
            raise ValueError('owned console, Node or decoder not reaped')

    def ready_line(self, child):
        result = self.read_markers()
        if all(key in result for key in ('LAUNCHER_PID', 'SECURE_LISTENING', 'CAMERA_READY', 'DECODER_READY')):
            node_pid = int(result['LAUNCHER_PID'][0])
            helper_pid, name = result['CAMERA_READY']
            decoder_pid = int(result['DECODER_READY'][0])
            if len({node_pid, int(helper_pid), decoder_pid, child.pid}) != 4:
                raise ValueError('launcher and helpers must have distinct identities')
            self.verify_owner(child, result)
            if getattr(self, 'console', False) and self.console_pid in (int(helper_pid), decoder_pid):
                raise ValueError('console and media helpers must have distinct identities')
            verify_process(int(helper_pid), node_pid, self.helper)
            verify_process(decoder_pid, node_pid, self.decoder)
            self.node_pid, self.decoder_pid = node_pid, decoder_pid
            return 'CAMERA_READY', helper_pid, name
        if child.poll() is not None:
            raise ValueError('launcher fixture exited before both media readiness markers')
        return None

    def run_launcher(self, driver, node_binary, helper, decoder, consumer, scenario, paired_store=False, console=False):
        verify_mode(scenario, paired_store, console)
        self.console = console  # Reuse exact owned-PTY-session cleanup on all failures.
        self.node, self.helper, self.decoder = node_binary, helper, decoder
        self.env.update(CAPYIO_CAMERA_LAUNCHER_HOSTED='1', CAPYIO_CAMERA_SCENARIO=scenario,
                        CAPYIO_CAMERA_PAIRED_STORE='1' if paired_store else '0',
                        CAPYIO_CAMERA_CONSOLE='1' if console else '0')
        self.prepare_daemon()
        baseline = graph_fingerprint(self.dump())
        named(self.dump(), 'capyio.test.null')
        if not baseline:
            raise ValueError('sentinel absent')
        test = CONSOLE_TEST if console else 'hosted_linux_camera_graph'
        owner = self.launch([str(driver), '--exact', test, '--ignored', '--nocapture'], 'owner')
        for _ in range(2):
            if not no_camera(self.dump()):
                raise ValueError('camera created before local fixture approval')
            time.sleep(.05)
        owner.stdin.write(b'approve synthetic camera\n')
        owner.stdin.flush()
        if console:
            self.wait_for(lambda: self.consent_checkpoint(owner, baseline))
            owner.stdin.write(b'check consent\n')
            owner.stdin.flush()
        if scenario in CONSOLE_PENDING:
            code = owner.wait(timeout=10)
            self.check()
            verify_completion((self.root / 'owner.out').read_text(), scenario, code,
                              paired_store=paired_store, console=console)
            for _ in range(3):
                objects = self.dump()
                if not no_camera(objects) or graph_fingerprint(objects) != baseline:
                    raise ValueError('unapproved or rejected console left private graph changes')
                time.sleep(.05)
            self.assert_removed(baseline, {}, self.node_pid)
            self.assert_owner_removed()
            return
        if scenario == 'handshake_timeout':
            def listening():
                found = self.read_markers()
                if 'LAUNCHER_PID' in found and 'SECURE_LISTENING' in found:
                    pid = int(found['LAUNCHER_PID'][0])
                    self.verify_owner(owner, found)
                    return pid
                if owner.poll() is not None:
                    raise ValueError('fixture exited before listening checkpoint')
                return None
            pid = self.wait_for(listening)
            if not no_camera(self.dump()):
                raise ValueError('media graph exists before handshake')
            owner.stdin.write(b'start frames\n')
            owner.stdin.flush()
            code = owner.wait(timeout=15)
            self.check()
            verify_completion((self.root / 'owner.out').read_text(), scenario, code, paired_store=paired_store, console=console)
            self.assert_removed(baseline, {}, pid)
            self.assert_owner_removed()
            return
        _, announced_pid, name = self.wait_for(lambda: self.ready_line(owner))
        objects, node, port = self.wait_for(lambda: self.source_ready(name))
        props = node['info']['props']
        if props.get('media.class') != 'Video/Source' or props.get('node.virtual') not in (True, 'true'):
            raise ValueError('not a native virtual Video/Source')
        pid = owned_pid(objects, node, SimpleNamespace(pid=self.node_pid), helper)
        if pid != int(announced_pid):
            raise ValueError('announced helper PID differs from graph owning Client')
        if any(o.get('type') == 'PipeWire:Interface:Link' for o in objects):
            raise ValueError('camera unexpectedly autoconnected')
        owned = graph_fingerprint([node, port])
        app = None
        if scenario == 'pixels':
            token = name.removeprefix(PREFIX)
            app = self.launch([str(consumer), '--token', token, '--frames', '20'], 'consumer')
            def ready_consumer():
                try:
                    graph = self.dump()
                    return camera_port(graph, named(graph, 'capyio.camera-test.' + token), 'in')
                except ValueError:
                    return None
            target = self.wait_for(ready_consumer)
            self.command(['/usr/bin/pw-link', str(port['id']), str(target['id'])])
            links = [o for o in self.dump() if o.get('type') == 'PipeWire:Interface:Link']
            if len(links) != 1 or links[0]['info'].get('output-port-id') != port['id'] or links[0]['info'].get('input-port-id') != target['id']:
                raise ValueError('exact explicit camera link missing')
            owned.update(graph_fingerprint(links))
        owner.stdin.write(b'start frames\n')
        owner.stdin.flush()
        code = owner.wait(timeout=15)
        self.check()
        verify_completion((self.root / 'owner.out').read_text(), scenario, code, paired_store=paired_store, console=console)
        if app is not None:
            if app.wait(timeout=5) != 0:
                raise ValueError('independent native pixel consumer failed')
            result = json.loads((self.root / 'consumer.out').read_text())
            if result['result'] != 0 or result['verified_frames'] != 20 or result['last_sequence'] < 20:
                raise ValueError('exact native NV12 byte/format/order evidence absent')
        self.assert_removed(baseline, owned, pid)
        self.assert_owner_removed()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--hosted-private-camera-launcher', action='store_true', required=True)
    for name in ('driver', 'node', 'helper', 'decoder', 'consumer'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--scenario', choices=ALL_SCENARIOS, required=True)
    parser.add_argument('--paired-store', action='store_true',
                        help='Require a native-camera,paired-store feature-enabled driver and private fake keyring authentication')
    parser.add_argument('--console', action='store_true',
                        help='Require the actual paired console, private PTY and fixed sibling Node')
    args = parser.parse_args()
    verify_mode(args.scenario, args.paired_store, args.console)
    with tempfile.TemporaryDirectory(prefix='capyio-camera-launcher-') as temporary:
        root = Path(temporary)
        root.chmod(0o700)
        harness = LauncherHarness(root, e2e=True)
        try:
            harness.run_launcher(*(getattr(args, name).resolve() for name in ('driver', 'node', 'helper', 'decoder', 'consumer')), args.scenario, paired_store=args.paired_store, console=args.console)
        except Exception:
            for label in ('daemon.err', 'owner.out', 'owner.err', 'consumer.out', 'consumer.err', 'command.err'):
                path = root / label
                if path.exists():
                    print(label + ': ' + path.read_text(errors='replace')[-4096:], file=sys.stderr)
            raise
        finally:
            harness.close()
    real_graph = args.scenario != 'handshake_timeout' and args.scenario not in CONSOLE_PENDING
    if args.console:
        print(f'capyio_pipewire_camera_console=passed scenario={args.scenario} actual_console=true paired_store=true real_graph={str(real_graph).lower()} owned_cleanup=true')
    else:
        print(f'capyio_pipewire_camera_launcher=passed scenario={args.scenario} actual_executable=true paired_store={str(args.paired_store).lower()} real_graph={str(real_graph).lower()} owned_cleanup=true')


if __name__ == '__main__':
    main()
