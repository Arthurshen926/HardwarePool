#!/usr/bin/env python3
"""Hosted private PipeWire playback/virtual-Source integration. Never installs tools."""
import argparse
import array
import json
import math
import os
from pathlib import Path
import signal
import resource
import subprocess
import sys
import tempfile
import time
import uuid

LIMIT = 2 * 1024 * 1024
SPA = '''context.spa-libs = {
 audio.convert.* = audioconvert/libspa-audioconvert
 support.* = support/libspa-support
}
'''
CLIENT = SPA + '''context.properties = { support.dbus = false }
context.modules = [
 { name = libpipewire-module-protocol-native }
 { name = libpipewire-module-client-node }
 { name = libpipewire-module-adapter }
 { name = libpipewire-module-metadata }
]
stream.properties = {
 adapter.auto-port-config = { mode = dsp monitor = false position = preserve }
}
'''
DAEMON = SPA + '''context.properties = {
 core.daemon = true
 core.name = capyio-test
 support.dbus = false
 default.clock.rate = 48000
 default.clock.allowed-rates = [ 48000 ]
 default.clock.quantum = 480
}
context.modules = [
 { name = libpipewire-module-protocol-native }
 { name = libpipewire-module-spa-node-factory }
 { name = libpipewire-module-client-node }
 { name = libpipewire-module-access }
 { name = libpipewire-module-adapter }
 { name = libpipewire-module-link-factory }
 { name = libpipewire-module-metadata }
]
context.objects = [
 { factory = spa-node-factory args = {
   factory.name = support.node.driver
   node.name = Dummy-Driver
   node.group = pipewire.dummy
   priority.driver = 20000
 } }
 { factory = adapter args = {
   factory.name = support.null-audio-sink
   node.name = capyio.test.null
   media.class = Audio/Sink
   audio.position = [ MONO ]
   adapter.auto-port-config = { mode = dsp monitor = true position = preserve }
 } }
]
'''


def publish_marker(path):
    temporary = path.with_name(path.name + '.tmp')
    with temporary.open('x') as marker:
        marker.write('capyio-hosted-pipewire-loopback-v1\n')
    temporary.replace(path)


def verify_signal(raw):
    """Require ordered sustained tone/silence/tone, not mere bytes or energy."""
    if len(raw) > LIMIT or len(raw) % 2:
        raise ValueError('invalid capture size')
    samples = array.array('h', raw)
    if sys.byteorder != 'little':
        samples.byteswap()
    n = 4800
    waves = {f: [(math.cos(2 * math.pi * f * i / 48000),
                  math.sin(2 * math.pi * f * i / 48000)) for i in range(n)]
             for f in (1000, 2000)}
    labels = []
    for offset in range(0, len(samples) - n + 1, n):
        frame = samples[offset:offset + n]
        rms = math.sqrt(sum(x * x for x in frame) / n)
        amps = {f: 2 * math.hypot(sum(x * c for x, (c, _) in zip(frame, wave)),
                                sum(x * s for x, (_, s) in zip(frame, wave))) / n
                for f, wave in waves.items()}
        label = 0 if rms < 100 else -1
        for f, other in ((1000, 2000), (2000, 1000)):
            if 1000 < rms < 16000 and amps[f] > 3000 and amps[other] < amps[f] * .15:
                label = f
        labels.append(label)
    stage = 0
    count = 0
    required = [(1000, 8), (0, 4), (2000, 8)]
    for label in labels:
        wanted, minimum = required[stage]
        count = count + 1 if label == wanted else 0
        if count >= minimum:
            stage += 1
            count = 0
            if stage == len(required):
                return
    raise ValueError('ordered sustained PCM signature absent')


def verify_silence(raw, minimum_samples):
    if len(raw) > LIMIT or len(raw) % 2 or len(raw) < minimum_samples * 2:
        raise ValueError('bounded silence evidence absent')
    samples = array.array('h', raw)
    if sys.byteorder != 'little':
        samples.byteswap()
    if math.sqrt(sum(x * x for x in samples) / len(samples)) >= 100:
        raise ValueError('unexpected audio outside authorized playback')


def select_ports(objects, monitor_only=False):
    nodes = {o['id']: o.get('info', {}).get('props', {}) for o in objects
             if o.get('type') == 'PipeWire:Interface:Node'}
    def node(predicate):
        matches = [key for key, props in nodes.items() if predicate(props)]
        if len(matches) != 1:
            raise ValueError('graph nodes not uniquely ready')
        return matches[0]
    sink = node(lambda p: p.get('node.name') == 'capyio.test.null')
    recorder = node(lambda p: p.get('node.name') == 'capyio.test.monitor')
    def port(owner, direction):
        matches = [o['id'] for o in objects if o.get('type') == 'PipeWire:Interface:Port'
                   and str(o.get('info', {}).get('props', {}).get('node.id')) == str(owner)
                   and o.get('info', {}).get('props', {}).get('port.direction') == direction]
        if len(matches) != 1:
            raise ValueError('graph ports not uniquely ready')
        return matches[0]
    monitor = (port(sink, 'out'), port(recorder, 'in'))
    if monitor_only:
        return [monitor]
    player = node(lambda p: p.get('target.object') == 'capyio.test.null')
    return [(port(player, 'out'), port(sink, 'in')), monitor]


VIRTUAL_PREFIX = 'capyio.virtual-microphone.'
VIRTUAL_SCENARIOS = ('waveform', 'no_consumer', 'child_exit', 'server_disconnect')


def virtual_expectation(value):
    """Only the fresh production approval's generated local Sink name is usable."""
    port_id = str(uuid.UUID(value['sink_port_id']))
    if value['sink_port_id'] != port_id or value['node_name'] != VIRTUAL_PREFIX + port_id:
        raise ValueError('virtual microphone identity is not the exact local Sink')
    if type(value['owner_pid']) is not int or value['owner_pid'] <= 1:
        raise ValueError('invalid production owner identity')
    return value


def virtual_nodes(objects):
    return [o for o in objects if o.get('type') == 'PipeWire:Interface:Node'
            and (o.get('info', {}).get('props', {}).get('media.class') == 'Audio/Source'
                 or str(o.get('info', {}).get('props', {}).get('node.name', ''))
                 .startswith(VIRTUAL_PREFIX))]


def select_virtual_source(objects, expected):
    """Reject monitors, renamed sinks, duplicate nodes and automatic graph links."""
    candidates = virtual_nodes(objects)
    if len(candidates) != 1:
        raise ValueError('exact virtual microphone node not uniquely ready')
    source = candidates[0]
    props = source.get('info', {}).get('props', {})
    if (props.get('node.name') != expected['node_name']
            or props.get('media.class') != 'Audio/Source'
            or not (props.get('node.virtual') is True or props.get('node.virtual') == 'true')):
        raise ValueError('owned node is not the declared virtual Audio/Source')
    ports = [o for o in objects if o.get('type') == 'PipeWire:Interface:Port'
             and str(o.get('info', {}).get('props', {}).get('node.id')) == str(source['id'])]
    if len(ports) != 1:
        raise ValueError('virtual microphone needs exactly one mono output port')
    port = ports[0]
    port_props = port.get('info', {}).get('props', {})
    if (port_props.get('port.direction') != 'out'
            or port_props.get('port.monitor') not in (None, False, 'false')
            or port_props.get('audio.channel') != 'MONO'):
        raise ValueError('virtual microphone is not a direct mono Source output')
    if any(o.get('type') == 'PipeWire:Interface:Link'
           and (str(o.get('info', {}).get('output-node-id')) == str(source['id'])
                or str(o.get('info', {}).get('input-node-id')) == str(source['id']))
           for o in objects):
        raise ValueError('virtual microphone unexpectedly autolinked')
    return source, port


def recorder_port(objects):
    nodes = [o['id'] for o in objects if o.get('type') == 'PipeWire:Interface:Node'
             and o.get('info', {}).get('props', {}).get('node.name') == 'capyio.test.monitor']
    if len(nodes) != 1:
        raise ValueError('independent app recorder not uniquely ready')
    ports = [o['id'] for o in objects if o.get('type') == 'PipeWire:Interface:Port'
             and str(o.get('info', {}).get('props', {}).get('node.id')) == str(nodes[0])
             and o.get('info', {}).get('props', {}).get('port.direction') == 'in']
    if len(ports) != 1:
        raise ValueError('independent app recorder port not uniquely ready')
    return ports[0]


def graph_fingerprint(objects):
    """Persistent unrelated Node/Port/Link identities, including global serial."""
    return {o['id']: (o['type'], o.get('info', {}).get('props', {}).get('object.serial'))
            for o in objects if o.get('type') in (
                'PipeWire:Interface:Node', 'PipeWire:Interface:Port', 'PipeWire:Interface:Link')}


def verify_virtual_removed(objects, owned, unrelated):
    current = graph_fingerprint(objects)
    if virtual_nodes(objects) or any(current.get(key) == value for key, value in owned.items()):
        raise ValueError('owned virtual microphone node, ports or links survived Stop')
    if any(current.get(key) != value for key, value in unrelated.items()):
        raise ValueError('unrelated graph object changed during owned Stop')


def verify_process_owner(source, expected):
    """A matching graph name alone cannot prove production pw-cat ownership."""
    pid = int(source.get('info', {}).get('props', {}).get('application.process.id', 0))
    if pid <= 1:
        raise ValueError('virtual microphone process identity absent')
    process = Path('/proc') / str(pid)
    status = dict(line.split(':', 1) for line in (process / 'status').read_text().splitlines()
                  if ':' in line)
    args = (process / 'cmdline').read_bytes().rstrip(b'\0').split(b'\0')
    if (int(status['PPid']) != expected['owner_pid']
            or (process / 'exe').resolve() != Path('/usr/bin/pw-cat').resolve()
            or not args or b'--playback' not in args or b'--target=0' not in args):
        raise ValueError('Source is not the production owner direct no-autolink pw-cat child')
    return pid


class Harness:
    def __init__(self, root, e2e=False, virtual_scenario=None):
        self.root = root
        self.e2e = e2e
        self.virtual_scenario = virtual_scenario
        self.children = []
        self.files = []
        self.deadline = time.monotonic() + (45 if e2e else 30)
        # No inherited PipeWire config, remote, preload, session bus or user home.
        self.env = {'PATH': '/usr/bin:/bin', 'LANG': 'C.UTF-8',
                    'HOME': str(root), 'XDG_CONFIG_HOME': str(root),
                    'XDG_RUNTIME_DIR': str(root), 'PIPEWIRE_RUNTIME_DIR': str(root),
                    'PIPEWIRE_CONFIG_DIR': str(root), 'PIPEWIRE_REMOTE': 'capyio-test',
                    'CAPYIO_PIPEWIRE_TEST': '1', 'CAPYIO_PIPEWIRE_E2E': '1'}
        if virtual_scenario is not None:
            self.env['CAPYIO_PIPEWIRE_VIRTUAL_MICROPHONE'] = virtual_scenario

    def check(self):
        if time.monotonic() >= self.deadline:
            raise TimeoutError('whole loopback deadline')
        for path in self.root.iterdir():
            if path.is_file() and path.stat().st_size > LIMIT:
                raise ValueError('bounded output exceeded')

    def spawn(self, args, name):
        out = open(self.root / (name + '.out'), 'wb')
        err = open(self.root / (name + '.err'), 'wb')
        self.files.extend([out, err])
        child = subprocess.Popen(args, env=self.env, stdin=subprocess.DEVNULL,
                                 stdout=out, stderr=err, start_new_session=True,
                                 preexec_fn=lambda: resource.setrlimit(resource.RLIMIT_FSIZE, (LIMIT, LIMIT)))
        self.children.append(child)
        return child

    def command(self, args):
        self.check()
        child = self.spawn(args, 'command')
        try:
            status = child.wait(timeout=min(2, max(.01, self.deadline - time.monotonic())))
        except subprocess.TimeoutExpired:
            self.stop(child)
            self.children.remove(child)
            raise TimeoutError('graph command deadline') from None
        self.children.remove(child)
        self.check()
        if status:
            raise ValueError('graph command failed')
        return (self.root / 'command.out').read_bytes()

    @staticmethod
    def stop(child):
        # Leader exit does not imply descendant exit. Always kill the owned group.
        try:
            os.killpg(child.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            child.wait(timeout=.5)
        except subprocess.TimeoutExpired:
            pass
        try:
            os.killpg(child.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        child.wait(timeout=1)
        deadline = time.monotonic() + 1
        while True:
            try:
                os.killpg(child.pid, 0)
            except ProcessLookupError:
                return
            if time.monotonic() >= deadline:
                raise RuntimeError('owned process group survived cleanup')
            time.sleep(.01)

    def close(self):
        errors = []
        for child in reversed(self.children):
            try:
                self.stop(child)
            except (OSError, RuntimeError, subprocess.TimeoutExpired) as error:
                errors.append(error)
        for stream in self.files:
            stream.close()
        if errors:
            raise RuntimeError('owned process cleanup failed')

    def start_daemon(self):
        daemon = self.spawn(['/usr/bin/pipewire', '-c', 'daemon.conf'], 'daemon')
        ready = time.monotonic() + 5
        while not (self.root / 'capyio-test').exists():
            self.check()
            if daemon.poll() is not None or time.monotonic() > ready:
                raise ValueError('private daemon failed to become ready')
            time.sleep(.02)
        return daemon

    def prepare_daemon(self):
        for name, content in [('daemon.conf', DAEMON), ('client.conf', CLIENT),
                              ('client-rt.conf', CLIENT)]:
            (self.root / name).write_text(content)
        (self.root / '.capyio-pipewire-loopback').write_text('capyio-hosted-pipewire-loopback-v1\n')
        return self.start_daemon()

    def wait_checkpoint(self, producer, marker):
        ready = time.monotonic() + 10
        while not (self.root / marker).exists():
            self.check()
            if producer.poll() is not None or time.monotonic() >= ready:
                raise ValueError('bounded integrated graph checkpoint absent: ' + marker)
            time.sleep(.02)

    def dump(self):
        return json.loads(self.command(['/usr/bin/pw-dump']))

    def run_virtual(self, fixture):
        daemon = self.prepare_daemon()
        record = self.spawn(['/usr/bin/pw-cat', '--record', '--rate=48000', '--channels=1',
                             '--format=s16', '--channel-map=mono', '--latency=20ms', '--target=0',
                             '--properties={"node.name":"capyio.test.monitor"}', '-'], 'capture')
        producer = self.spawn([str(fixture), '--ignored', '--exact',
                               'hosted_pipewire_virtual_microphone_end_to_end',
                               '--nocapture', '--test-threads=1'], 'producer')
        self.wait_checkpoint(producer, '.capyio-before-consent')
        expected = virtual_expectation(json.loads(
            (self.root / '.capyio-virtual-microphone.json').read_text()))
        for announced, checked in [
                ('.capyio-before-consent', '.capyio-consent-checked'),
                ('.capyio-before-android-consent', '.capyio-android-consent-checked')]:
            self.wait_checkpoint(producer, announced)
            # Two distinct whole graph observations while the exact consent is
            # withheld. No synthetic monitor can stand in for Source absence.
            for _ in range(2):
                if virtual_nodes(self.dump()):
                    raise ValueError('virtual microphone appeared before both consents')
                time.sleep(.1)
            publish_marker(self.root / checked)
        ready = time.monotonic() + 10
        while True:
            self.check()
            if any(c.poll() is not None for c in (daemon, record, producer)):
                raise ValueError('virtual microphone child exited before graph readiness')
            objects = self.dump()
            try:
                source, port = select_virtual_source(objects, expected)
                target = recorder_port(objects)
                break
            except ValueError:
                if time.monotonic() >= ready:
                    raise
                time.sleep(.05)
        source_pid = verify_process_owner(source, expected)
        fingerprint = graph_fingerprint(objects)
        owned = {key: fingerprint[key] for key in (source['id'], port['id'])}
        unrelated = {key: value for key, value in fingerprint.items() if key not in owned}
        if self.virtual_scenario == 'waveform':
            self.command(['/usr/bin/pw-link', str(port['id']), str(target)])
            objects = self.dump()
            links = [o for o in objects if o.get('type') == 'PipeWire:Interface:Link'
                     and (o.get('info', {}).get('output-port-id'),
                          o.get('info', {}).get('input-port-id')) == (port['id'], target)]
            all_links = [o for o in objects if o.get('type') == 'PipeWire:Interface:Link']
            if len(links) != 1 or len(all_links) != 1:
                raise ValueError('expected only direct Source to independent app link')
            owned.update(graph_fingerprint(links))
        publish_marker(self.root / '.capyio-links-ready')
        if self.virtual_scenario == 'child_exit':
            self.wait_checkpoint(producer, '.capyio-fault-ready')
            # PID was verified as the current launcher's direct pw-cat child.
            if verify_process_owner(source, expected) != source_pid:
                raise ValueError('owned pw-cat identity changed before fault injection')
            os.kill(source_pid, signal.SIGTERM)
        elif self.virtual_scenario == 'server_disconnect':
            self.wait_checkpoint(producer, '.capyio-fault-ready')
            self.stop(daemon)
            self.children.remove(daemon)
        self.wait_checkpoint(producer, '.capyio-after-stop')
        if Path('/proc', str(source_pid)).exists():
            raise ValueError('owned pw-cat process not reaped before completion')
        if self.virtual_scenario == 'server_disconnect':
            # A fresh private server must not recover the stopped Source. This
            # is not used as evidence that unrelated old-server objects survived.
            daemon = self.start_daemon()
            unrelated = {}
            owned = {}
        observe_until = time.monotonic() + .35
        while time.monotonic() < observe_until:
            verify_virtual_removed(self.dump(), owned, unrelated)
            self.check()
            time.sleep(.03)
        publish_marker(self.root / '.capyio-stop-checked')
        while producer.poll() is None:
            self.check()
            if daemon.poll() is not None:
                raise ValueError('private graph died during Stop verification')
            time.sleep(.02)
        expected_marker = ('capyio_pipewire_virtual_microphone=passed scenario='
                           + self.virtual_scenario
                           + ' authorized_udp=true stop=true fresh_post_stop=true child_reaped=true')
        if producer.returncode or expected_marker not in (self.root / 'producer.err').read_text():
            raise ValueError('integrated virtual microphone lifecycle evidence absent')
        self.stop(record)
        self.children.remove(record)
        if self.virtual_scenario == 'waveform':
            verify_signal((self.root / 'capture.out').read_bytes())

    def run(self, fixture):
        daemon = self.prepare_daemon()
        record = self.spawn(['/usr/bin/pw-cat', '--record', '--rate=48000', '--channels=1',
                             '--format=s16', '--channel-map=mono', '--latency=20ms', '--target=0',
                             '--properties={"node.name":"capyio.test.monitor"}', '-'], 'capture')
        producer_args = ([str(fixture), '--ignored', '--exact',
                          'hosted_pipewire_android_source_end_to_end', '--nocapture',
                          '--test-threads=1'] if self.e2e else
                         [str(fixture), '--hosted-pipewire-loopback'])
        producer = self.spawn(producer_args, 'producer')
        existing_links = set()
        if self.e2e:
            pending = time.monotonic() + 10
            while not (self.root / '.capyio-before-consent').exists():
                self.check()
                if producer.poll() is not None or time.monotonic() >= pending:
                    raise ValueError('authenticated preconsent checkpoint absent')
                time.sleep(.02)
            objects = json.loads(self.command(['/usr/bin/pw-dump']))
            if any(o.get('info', {}).get('props', {}).get('target.object') == 'capyio.test.null'
                   for o in objects):
                raise ValueError('playback appeared before exact local consent')
            for source, target in select_ports(objects, monitor_only=True):
                self.command(['/usr/bin/pw-link', str(source), str(target)])
                existing_links.add((source, target))
            time.sleep(.4)
            baseline = (self.root / 'capture.out').read_bytes()
            verify_silence(baseline, minimum_samples=4800)
            publish_marker(self.root / '.capyio-consent-checked')
            pending = time.monotonic() + 10
            while not (self.root / '.capyio-before-android-consent').exists():
                self.check()
                if producer.poll() is not None or time.monotonic() >= pending:
                    raise ValueError('independent Android consent checkpoint absent')
                time.sleep(.02)
            objects = json.loads(self.command(['/usr/bin/pw-dump']))
            if any(o.get('info', {}).get('props', {}).get('target.object') == 'capyio.test.null'
                   for o in objects):
                raise ValueError('playback appeared before Android consent')
            pending_offset = (self.root / 'capture.out').stat().st_size
            time.sleep(.4)
            verify_silence((self.root / 'capture.out').read_bytes()[pending_offset:],
                           minimum_samples=4800)
            publish_marker(self.root / '.capyio-android-consent-checked')
        ready = time.monotonic() + (10 if self.e2e else 6)
        while True:
            self.check()
            if any(c.poll() is not None for c in (daemon, record, producer)):
                raise ValueError('audio child exited before graph readiness')
            objects = json.loads(self.command(['/usr/bin/pw-dump']))
            try:
                links = select_ports(objects)
                break
            except ValueError:
                if time.monotonic() >= ready:
                    raise
                time.sleep(.05)
        for source, target in links:
            if (source, target) in existing_links:
                continue
            self.command(['/usr/bin/pw-link', str(source), str(target)])
        objects = json.loads(self.command(['/usr/bin/pw-dump']))
        actual = {(o.get('info', {}).get('output-port-id'), o.get('info', {}).get('input-port-id'))
                  for o in objects if o.get('type') == 'PipeWire:Interface:Link'}
        if not set(links).issubset(actual):
            raise ValueError('explicit links absent')
        publish_marker(self.root / '.capyio-links-ready')
        while producer.poll() is None:
            self.check()
            if daemon.poll() is not None or record.poll() is not None:
                raise ValueError('audio graph exited during playback')
            time.sleep(.02)
        if producer.returncode or 'child_reaped=true' not in (self.root / 'producer.err').read_text():
            raise ValueError('production fixture failed or did not reap pw-cat')
        if self.e2e:
            marker = 'capyio_pipewire_e2e=passed authorized_udp=true stop=true child_reaped=true'
            if marker not in (self.root / 'producer.err').read_text():
                raise ValueError('integrated authorization/Stop evidence absent')
            objects = json.loads(self.command(['/usr/bin/pw-dump']))
            if any(o.get('info', {}).get('props', {}).get('target.object') == 'capyio.test.null'
                   for o in objects):
                raise ValueError('playback graph survived owner Stop')
            # Allow the already queued graph quantum to drain, then require NEW samples.
            time.sleep(.1)
            stopped_offset = (self.root / 'capture.out').stat().st_size
        time.sleep(.5)
        self.check()
        self.stop(record)
        self.children.remove(record)
        capture = (self.root / 'capture.out').read_bytes()
        verify_signal(capture)
        if self.e2e:
            verify_silence(capture[stopped_offset:], minimum_samples=14400)


def interrupted(_signum, _frame):
    raise SystemExit('hosted loopback interrupted')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--hosted-pipewire-loopback', action='store_true', required=True)
    fixtures = parser.add_mutually_exclusive_group(required=True)
    fixtures.add_argument('--fixture', type=Path)
    fixtures.add_argument('--e2e-fixture', type=Path)
    fixtures.add_argument('--virtual-microphone-fixture', type=Path)
    parser.add_argument('--virtual-microphone-scenario', choices=VIRTUAL_SCENARIOS)
    args = parser.parse_args()
    if args.virtual_microphone_fixture is None and args.virtual_microphone_scenario is not None:
        parser.error('virtual microphone scenario requires its explicit fixture')
    if args.virtual_microphone_scenario is None:
        args.virtual_microphone_scenario = 'waveform'
    if os.environ.get('GITHUB_ACTIONS') != 'true':
        parser.error('requires explicit disposable hosted CI environment')
    for tool in ('pipewire', 'pw-cat', 'pw-dump', 'pw-link'):
        if not os.access('/usr/bin/' + tool, os.X_OK):
            parser.error('required official PipeWire tool absent')
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    with tempfile.TemporaryDirectory(prefix='capyio-pipewire-') as directory:
        virtual = args.virtual_microphone_fixture is not None
        harness = Harness(Path(directory), e2e=args.e2e_fixture is not None or virtual,
                          virtual_scenario=args.virtual_microphone_scenario if virtual else None)
        try:
            fixture = (args.fixture or args.e2e_fixture or args.virtual_microphone_fixture)
            if virtual:
                harness.run_virtual(fixture.resolve(strict=True))
            else:
                harness.run(fixture.resolve(strict=True))
        except Exception:
            # All processes/configurations are owned synthetic fixtures, no user data.
            for name in ('daemon.err', 'producer.err', 'capture.err', 'command.err'):
                path = Path(directory) / name
                if path.is_file():
                    with path.open('rb') as log:
                        print(name + ': ' + log.read(4096).decode('utf-8', errors='replace'),
                              file=sys.stderr)
            raise
        finally:
            signal.signal(signal.SIGTERM, signal.SIG_IGN)
            signal.signal(signal.SIGINT, signal.SIG_IGN)
            harness.close()
    if args.virtual_microphone_fixture:
        print('capyio_pipewire_virtual_microphone=passed scenario='
              + args.virtual_microphone_scenario
              + ' pinned_tls=true consent=true dynamic_udp=true real_source=true stop=true cleanup=true')
    elif args.e2e_fixture:
        print('capyio_pipewire_e2e=passed pinned_tls=true consent=true dynamic_udp=true real_graph=true stop=true cleanup=true')
    else:
        print('capyio_pipewire_loopback=passed real_graph=true ordered_pcm=true cleanup=true')


if __name__ == '__main__':
    main()
