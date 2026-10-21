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

SCENARIOS = ('pixels', 'local_stop', 'control_stop', 'control_eof',
             'source_revoke', 'partial_cavc', 'expiry', 'handshake_timeout')
PATTERNS = {
    'LAUNCHER_PID': r'LAUNCHER_PID ([1-9][0-9]*)',
    'SECURE_LISTENING': r'SECURE_LISTENING no_media_children=true',
    'CAMERA_READY': r'CAMERA_READY ([1-9][0-9]*) (capyio\.camera\.[A-Za-z0-9-]+)',
    'DECODER_READY': r'DECODER_READY ([1-9][0-9]*)',
    'VERIFIED': r'VERIFIED (' + '|'.join(SCENARIOS) + ')',
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


def verify_completion(text, scenario, code):
    result = markers(text, complete=True)
    required = {'LAUNCHER_PID', 'SECURE_LISTENING', 'VERIFIED', 'CAMERA_STOPPED'}
    if scenario != 'handshake_timeout':
        required.update(('CAMERA_READY', 'DECODER_READY'))
    elif 'CAMERA_READY' in result or 'DECODER_READY' in result:
        raise ValueError('handshake timeout spawned media children')
    if code != 0 or not required.issubset(result) or result['VERIFIED'] != (scenario,):
        raise ValueError('fixture did not verify the exact expected terminal outcome')
    return result


class LauncherHarness(CameraHarness):
    def read_markers(self):
        return markers((self.root / 'owner.out').read_text())

    def ready_line(self, child):
        result = self.read_markers()
        if all(key in result for key in ('LAUNCHER_PID', 'SECURE_LISTENING', 'CAMERA_READY', 'DECODER_READY')):
            node_pid = int(result['LAUNCHER_PID'][0])
            helper_pid, name = result['CAMERA_READY']
            decoder_pid = int(result['DECODER_READY'][0])
            if len({node_pid, int(helper_pid), decoder_pid, child.pid}) != 4:
                raise ValueError('launcher and helpers must have distinct identities')
            verify_process(node_pid, child.pid, self.node)
            verify_process(int(helper_pid), node_pid, self.helper)
            verify_process(decoder_pid, node_pid, self.decoder)
            self.node_pid, self.decoder_pid = node_pid, decoder_pid
            return 'CAMERA_READY', helper_pid, name
        if child.poll() is not None:
            raise ValueError('launcher fixture exited before both media readiness markers')
        return None

    def run_launcher(self, driver, node_binary, helper, decoder, consumer, scenario):
        self.node, self.helper, self.decoder = node_binary, helper, decoder
        self.env.update(CAPYIO_CAMERA_LAUNCHER_HOSTED='1', CAPYIO_CAMERA_SCENARIO=scenario)
        self.prepare_daemon()
        baseline = graph_fingerprint(self.dump())
        named(self.dump(), 'capyio.test.null')
        if not baseline:
            raise ValueError('sentinel absent')
        owner = self.launch([str(driver), '--exact', 'hosted_linux_camera_graph', '--ignored', '--nocapture'], 'owner')
        for _ in range(2):
            if not no_camera(self.dump()):
                raise ValueError('camera created before local fixture approval')
            time.sleep(.05)
        owner.stdin.write(b'approve synthetic camera\n')
        owner.stdin.flush()
        if scenario == 'handshake_timeout':
            def listening():
                found = self.read_markers()
                if 'LAUNCHER_PID' in found and 'SECURE_LISTENING' in found:
                    pid = int(found['LAUNCHER_PID'][0])
                    verify_process(pid, owner.pid, node_binary)
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
            verify_completion((self.root / 'owner.out').read_text(), scenario, code)
            self.assert_removed(baseline, {}, pid)
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
        verify_completion((self.root / 'owner.out').read_text(), scenario, code)
        if app is not None:
            if app.wait(timeout=5) != 0:
                raise ValueError('independent native pixel consumer failed')
            result = json.loads((self.root / 'consumer.out').read_text())
            if result['result'] != 0 or result['verified_frames'] != 20 or result['last_sequence'] < 20:
                raise ValueError('exact native NV12 byte/format/order evidence absent')
        self.assert_removed(baseline, owned, pid)
        for owned_pid_value in (self.node_pid, self.decoder_pid):
            if Path('/proc', str(owned_pid_value)).exists():
                raise ValueError('owned launcher or decoder not reaped')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--hosted-private-camera-launcher', action='store_true', required=True)
    for name in ('driver', 'node', 'helper', 'decoder', 'consumer'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--scenario', choices=SCENARIOS, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='capyio-camera-launcher-') as temporary:
        root = Path(temporary)
        root.chmod(0o700)
        harness = LauncherHarness(root, e2e=True)
        try:
            harness.run_launcher(*(getattr(args, name).resolve() for name in ('driver', 'node', 'helper', 'decoder', 'consumer')), args.scenario)
        except Exception:
            for label in ('daemon.err', 'owner.out', 'owner.err', 'consumer.out', 'consumer.err', 'command.err'):
                path = root / label
                if path.exists():
                    print(label + ': ' + path.read_text(errors='replace')[-4096:], file=sys.stderr)
            raise
        finally:
            harness.close()
    print(f'capyio_pipewire_camera_launcher=passed scenario={args.scenario} actual_executable=true real_graph={str(args.scenario != "handshake_timeout").lower()} owned_cleanup=true')


if __name__ == '__main__':
    main()
