#!/usr/bin/env python3
"""Explicit private native camera acceptance. No installations or physical devices."""
import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time

from run_pipewire_loopback import CLIENT, DAEMON, Harness, graph_fingerprint, source_process_id

SCENARIOS = ('pixels', 'no_consumer', 'expiry', 'child_exit', 'server_disconnect', 'malformed', 'partial')
PREFIX = 'capyio.camera.'


def named(objects, name):
    nodes = [o for o in objects if o.get('type') == 'PipeWire:Interface:Node'
             and o.get('info', {}).get('props', {}).get('node.name') == name]
    if len(nodes) != 1:
        raise ValueError('exact camera node missing or ambiguous')
    return nodes[0]


def camera_port(objects, node, direction):
    ports = [o for o in objects if o.get('type') == 'PipeWire:Interface:Port'
             and str(o.get('info', {}).get('props', {}).get('node.id')) == str(node['id'])]
    if len(ports) != 1 or ports[0]['info']['props'].get('port.direction') != direction:
        raise ValueError('exact single camera port required')
    return ports[0]


def owned_pid(objects, node, producer, helper):
    pid = source_process_id(objects, node, {'node_name': node['info']['props']['node.name']})
    status = dict(line.split(':', 1) for line in Path('/proc', str(pid), 'status').read_text().splitlines() if ':' in line)
    if int(status['PPid']) != producer.pid or Path('/proc', str(pid), 'exe').resolve() != helper.resolve():
        raise ValueError('camera is not exact fixed sibling child of production Runtime owner')
    return pid


def no_camera(objects):
    return not any(o.get('type') == 'PipeWire:Interface:Node'
                   and str(o.get('info', {}).get('props', {}).get('node.name', '')).startswith(PREFIX)
                   for o in objects)


class CameraHarness(Harness):
    def prepare_daemon(self):
        self.env['PIPEWIRE_DEBUG'] = '2'  # Private test processes only; no production env override.
        # Do not inherit the audio harness's DSP port configuration for raw video.
        client = CLIENT.split('stream.properties =', 1)[0]
        for name, content in [('daemon.conf', DAEMON), ('client.conf', client), ('client-rt.conf', client)]:
            content = content.replace(' support.* = support/libspa-support',
                                      ' video.convert.* = videoconvert/libspa-videoconvert\n support.* = support/libspa-support')
            (self.root / name).write_text(content)
        return self.start_daemon()

    def dump(self):
        raw = self.command(['/usr/bin/pw-dump'])
        try:
            objects = json.loads(raw)
        except (json.JSONDecodeError, UnicodeDecodeError) as reason:
            raise ValueError(f'private graph dump invalid; bytes={len(raw)}') from reason
        if not isinstance(objects, list):
            raise ValueError('private graph dump is not an object list')
        return objects

    def launch(self, args, name):
        out = open(self.root / (name + '.out'), 'wb')
        err = open(self.root / (name + '.err'), 'wb')
        self.files.extend([out, err])
        child = subprocess.Popen(args, env=self.env, stdin=subprocess.PIPE, stdout=out, stderr=err,
                                 start_new_session=True)
        self.children.append(child)
        return child

    def wait_for(self, predicate, seconds=5):
        deadline = time.monotonic() + seconds
        while True:
            self.check()
            value = predicate()
            if value:
                return value
            if time.monotonic() >= deadline:
                raise TimeoutError('camera checkpoint expired')
            time.sleep(.02)

    def ready_line(self, child):
        data = (self.root / 'owner.out').read_text()
        for line in data.splitlines():
            if line.startswith('CAMERA_READY '):
                return line.split()
        if child.poll() is not None:
            raise ValueError('Runtime camera owner exited before helper readiness')
        return None

    def source_ready(self, name):
        objects = self.dump()
        try:
            node = named(objects, name)
            port = camera_port(objects, node, 'out')
            return objects, node, port
        except ValueError:
            return None

    def assert_removed(self, baseline, owned, pid):
        self.wait_for(lambda: no_camera(self.dump()))
        objects = self.dump()
        current = graph_fingerprint(objects)
        if any(current.get(k) == v for k, v in owned.items()):
            raise ValueError('owned camera node/port/link survived teardown')
        if any(current.get(k) != v for k, v in baseline.items()):
            raise ValueError('unrelated sentinel graph changed during camera teardown')
        if Path('/proc', str(pid)).exists():
            raise ValueError('owned camera process not reaped')

    def run(self, driver, helper, consumer, scenario):
        daemon = self.prepare_daemon()
        baseline = graph_fingerprint(self.dump())
        if not baseline or named(self.dump(), 'capyio.test.null') is None:
            raise ValueError('sentinel absent')
        if scenario in ('malformed', 'partial'):
            return self.invalid_ipc(helper, scenario, baseline)
        owner = self.launch([str(driver), '--synthetic-private-graph', scenario], 'owner')
        # Independent approval checkpoint before even constructing the Runtime grant.
        for _ in range(2):
            if not no_camera(self.dump()):
                raise ValueError('camera created before local fixture approval')
            time.sleep(.05)
        owner.stdin.write(b'approve synthetic camera\n')
        owner.stdin.flush()
        _, announced_pid, name = self.wait_for(lambda: self.ready_line(owner))
        objects, node, port = self.wait_for(lambda: self.source_ready(name))
        props = node['info']['props']
        if props.get('media.class') != 'Video/Source' or props.get('node.virtual') not in (True, 'true'):
            raise ValueError('not a native virtual Video/Source')
        pid = owned_pid(objects, node, owner, helper)
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
                    target = named(graph, 'capyio.camera-test.' + token)
                    return camera_port(graph, target, 'in')
                except ValueError:
                    return None
            target = self.wait_for(ready_consumer)
            self.command(['/usr/bin/pw-link', str(port['id']), str(target['id'])])
            links = [o for o in self.dump() if o.get('type') == 'PipeWire:Interface:Link']
            if len(links) != 1 or links[0]['info'].get('output-port-id') != port['id'] or links[0]['info'].get('input-port-id') != target['id']:
                raise ValueError('exact explicit camera link missing')
            owned.update(graph_fingerprint(links))
        if scenario == 'no_consumer':
            # Exact owned child only: force a full private pipe to exercise bounded queue/drop/Stop.
            os.kill(pid, signal.SIGSTOP)
        owner.stdin.write(b'start frames\n')
        owner.stdin.flush()
        if scenario == 'child_exit':
            os.kill(pid, signal.SIGTERM)
        elif scenario == 'server_disconnect':
            self.stop(daemon)
            self.children.remove(daemon)
        code = owner.wait(timeout=15)
        if scenario in ('pixels', 'no_consumer'):
            if code != 0 or 'CAMERA_STOPPED' not in (self.root / 'owner.out').read_text():
                raise ValueError('normal camera owner did not stop successfully')
        elif code == 0:
            raise ValueError('injected camera failure reported success')
        if app is not None:
            if app.wait(timeout=5) != 0:
                raise ValueError('independent native pixel consumer failed')
            result = json.loads((self.root / 'consumer.out').read_text())
            if result['result'] != 0 or result['verified_frames'] != 20 or result['last_sequence'] < 20:
                raise ValueError('exact native frame byte/order evidence absent')
        if scenario == 'server_disconnect':
            if Path('/proc', str(pid)).exists():
                raise ValueError('helper survived server disconnect')
            self.start_daemon()
            if not no_camera(self.dump()):
                raise ValueError('camera reconnected after owned server failure')
            named(self.dump(), 'capyio.test.null')  # New server, not old-sentinel continuity.
        else:
            self.assert_removed(baseline, owned, pid)

    def invalid_ipc(self, helper, scenario, baseline):
        token = 'negative-' + scenario
        child = self.launch([str(helper), '--token', token, '--lifetime-ms', '8000',
                             '--deadline-unix-ms', str(int(time.time() * 1000) + 8000)], 'invalid')
        objects, node, port = self.wait_for(lambda: self.source_ready(PREFIX + token))
        pid = source_process_id(objects, node, {'node_name': PREFIX + token})
        if pid != child.pid:
            raise ValueError('negative IPC target ownership mismatch')
        child.stdin.write(b'BAD!' + bytes(28) if scenario == 'malformed' else b'C')
        child.stdin.flush()
        if child.wait(timeout=4) != 2:
            raise ValueError('malformed/partial native record not rejected within deadline')
        self.assert_removed(baseline, graph_fingerprint([node, port]), pid)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--hosted-private-camera', action='store_true', required=True)
    parser.add_argument('--driver', type=Path, required=True)
    parser.add_argument('--helper', type=Path, required=True)
    parser.add_argument('--consumer', type=Path, required=True)
    parser.add_argument('--scenario', choices=SCENARIOS, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='capyio-camera-') as temporary:
        root = Path(temporary)
        root.chmod(0o700)
        harness = CameraHarness(root, e2e=True)
        try:
            harness.run(args.driver.resolve(), args.helper.resolve(), args.consumer.resolve(), args.scenario)
        except Exception:
            for child in harness.children:
                print(f'camera_process name={Path(child.args[0]).name} pid={child.pid} exit={child.poll()}', file=sys.stderr)
            for label in ('daemon.err', 'owner.err', 'consumer.out', 'consumer.err', 'invalid.err', 'command.err', 'command.out'):
                path = root / label
                if path.exists():
                    print(label + ': ' + path.read_text(errors='replace')[-4096:], file=sys.stderr)
            raise
        finally:
            harness.close()
    print(f'capyio_pipewire_camera=passed scenario={args.scenario} real_graph=true owned_cleanup=true')


if __name__ == '__main__':
    main()
