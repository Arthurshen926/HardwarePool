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
CONSOLE_SCENARIOS = ('waveform', 'no_approval', 'pretyped', 'mismatched_request',
                     'mismatched_route', 'mismatched_peer', 'expiry', 'remote_eof')
CONSOLE_PENDING = frozenset(CONSOLE_SCENARIOS[1:6])


def console_marker(mode, scenario):
    if mode not in ('playback', 'virtual_microphone') or scenario not in CONSOLE_SCENARIOS:
        raise ValueError('invalid console evidence identity')
    return (f'capyio_audio_console=passed mode={mode} scenario={scenario}'
            ' actual_tty=true actual_node=true fresh_consent=true protected_read=true'
            ' secret_redacted=true node_reaped=true')


def verify_console_owner(value, mode, scenario):
    """Independently verify the actual console -> actual fixed sibling Node."""
    if set(value) != {'console_pid', 'owner_pid', 'scenario', 'mode'}:
        raise ValueError('unexpected console owner evidence')
    if value['mode'] != mode or value['scenario'] != scenario:
        raise ValueError('console owner mode/scenario mismatch')
    pids = [ownership_number(value[key], (1 << 31) - 1)
            for key in ('console_pid', 'owner_pid')]
    if pids[0] == pids[1] or min(pids) <= 1:
        raise ValueError('distinct owned console and Node required')
    console, node = [Path('/proc', str(pid)) for pid in pids]
    console_exe, node_exe = (console / 'exe').resolve(strict=True), (node / 'exe').resolve(strict=True)
    if (console_exe.name != 'capyio-linux-console' or node_exe.name != 'capyio-node'
            or console_exe.parent != node_exe.parent):
        raise ValueError('actual fixed sibling binaries required')
    status = dict(line.split(':', 1) for line in (node / 'status').read_text().splitlines()
                  if ':' in line)
    if int(status['PPid']) != pids[0]:
        raise ValueError('Node is not the console direct child')
    for process, command in ((console, b'playback'), (node, b'linux-playback')):
        args = (process / 'cmdline').read_bytes().split(b'\0')
        if command not in args or b'--pairing-state' not in args:
            raise ValueError('protected playback invocation required')
    return value


def verify_playback_process_owner(objects, owner_pid):
    players = [o for o in objects if o.get('type') == 'PipeWire:Interface:Node'
               and o.get('info', {}).get('props', {}).get('target.object') == 'capyio.test.null']
    if len(players) != 1:
        raise ValueError('exact selected playback owner required')
    player = players[0]
    expected = {'node_name': player['info']['props']['node.name']}
    pid = source_process_id(objects, player, expected)
    process = Path('/proc', str(pid))
    status = dict(line.split(':', 1) for line in (process / 'status').read_text().splitlines()
                  if ':' in line)
    args = (process / 'cmdline').read_bytes().rstrip(b'\0').split(b'\0')
    if (int(status['PPid']) != owner_pid
            or (process / 'exe').resolve() != Path('/usr/bin/pw-cat').resolve()
            or b'--playback' not in args or b'--target=capyio.test.null' not in args):
        raise ValueError('selected playback is not the actual Node direct pw-cat child')
    owned_ids = {player['id']}
    owned_ids.update(o['id'] for o in objects if o.get('type') == 'PipeWire:Interface:Port'
                     and str(o.get('info', {}).get('props', {}).get('node.id')) == str(player['id']))
    return pid, owned_ids



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


def ownership_number(value, maximum=(1 << 32) - 2):
    if type(value) is int:
        number = value
    elif isinstance(value, str) and value.isascii() and value.isdecimal():
        if len(value) > 10:
            raise ValueError('invalid PipeWire ownership number')
        number = int(value)
        if str(number) != value:
            raise ValueError('noncanonical PipeWire ownership number')
    else:
        raise ValueError('invalid PipeWire ownership number')
    if not 1 <= number <= maximum:
        raise ValueError('PipeWire ownership number out of range')
    return number


def source_process_id(objects, source, expected):
    """Resolve the exact Node -> owning Client -> PID, never another pw-cat."""
    props = source.get('info', {}).get('props', {})
    if (source.get('type') != 'PipeWire:Interface:Node'
            or props.get('node.name') != expected['node_name']):
        raise ValueError('unexpected Source ownership target')
    # PipeWire 1.0.5 module-client-node/client-node.c sets client.id from the
    # owning Client global ID. Client context identity is not a Node property.
    client_id = ownership_number(props.get('client.id'))
    matches = [o for o in objects if o.get('id') == client_id]
    if (len(matches) != 1 or type(matches[0].get('id')) is not int
            or matches[0].get('type') != 'PipeWire:Interface:Client'):
        raise ValueError('Source owning Client is absent, duplicate or mistyped')
    client_props = matches[0].get('info', {}).get('props', {})
    pid = ownership_number(client_props.get('application.process.id'), (1 << 31) - 1)
    if pid <= 1:
        raise ValueError('invalid Source owning Client PID')
    # If either redundant identity is present it must agree. Native Linux
    # pipewire.sec.pid is the daemon's SO_PEERCRED identity, not a fallback.
    for properties, key in ((props, 'application.process.id'),
                            (client_props, 'pipewire.sec.pid')):
        if key in properties and ownership_number(properties[key], (1 << 31) - 1) != pid:
            raise ValueError('conflicting Source owning process identities')
    return pid


def ownership_diagnostic(objects, source):
    """Bounded allowlist only; never print arbitrary graph properties or paths."""
    def number(value):
        try:
            return ownership_number(value)
        except ValueError:
            return None
    props = source.get('info', {}).get('props', {})
    client_id = number(props.get('client.id'))
    matches = [o for o in objects if client_id is not None and o.get('id') == client_id]
    return {
        'node_id': number(source.get('id')),
        'node_client_id': client_id,
        'node_process_id': number(props.get('application.process.id')),
        'matching_clients': [{
            'id': number(o.get('id')),
            'process_id': number(o.get('info', {}).get('props', {}).get('application.process.id')),
        } for o in matches[:4]],
    }


def verify_process_owner(objects, source, expected):
    """A matching graph name alone cannot prove production pw-cat ownership."""
    try:
        pid = source_process_id(objects, source, expected)
        process = Path('/proc') / str(pid)
        status = dict(line.split(':', 1) for line in (process / 'status').read_text().splitlines()
                      if ':' in line)
        args = (process / 'cmdline').read_bytes().rstrip(b'\0').split(b'\0')
        if (int(status['PPid']) != expected['owner_pid']
                or (process / 'exe').resolve() != Path('/usr/bin/pw-cat').resolve()
                or not args or b'--playback' not in args or b'--target=0' not in args):
            raise ValueError('Source is not the production owner direct no-autolink pw-cat child')
        return pid
    except (ValueError, OSError, KeyError):
        print('capyio_pipewire_ownership='
              + json.dumps(ownership_diagnostic(objects, source), separators=(',', ':')),
              file=sys.stderr)
        raise


class Harness:
    def __init__(self, root, e2e=False, virtual_scenario=None, paired_store=False,
                 console=False, console_scenario=None):
        if paired_store and not e2e:
            raise ValueError('paired store requires an actual executable fixture')
        if console and (not paired_store or not e2e or console_scenario not in CONSOLE_SCENARIOS):
            raise ValueError('console requires explicit paired executable and scenario')
        if console and virtual_scenario not in (None, 'waveform'):
            raise ValueError('console profiles retain distinct exact lifecycle scenarios')
        if not console and console_scenario is not None:
            raise ValueError('console scenario requires the console profile')
        self.console = console
        self.console_scenario = console_scenario
        self.console_owner = None
        self.paired_store = paired_store
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
        if paired_store:
            self.env['CAPYIO_AUDIO_PAIRED_STORE'] = '1'
        if console:
            self.env['CAPYIO_AUDIO_CONSOLE'] = '1'
            self.env['CAPYIO_AUDIO_CONSOLE_SCENARIO'] = console_scenario
        if virtual_scenario is not None:
            self.env['CAPYIO_PIPEWIRE_VIRTUAL_MICROPHONE'] = virtual_scenario

    def verify_pairing(self):
        evidence = (self.root / 'producer.err').read_text().splitlines()
        markers = [line for line in evidence if line.startswith('capyio_audio_paired_store')]
        expected = ['capyio_audio_paired_store=passed protected_read=true secret_redacted=true']
        if markers != (expected if self.paired_store else []):
            raise ValueError('paired-store evidence missing, duplicated, malformed or unexpected')
        if self.paired_store:
            result = (self.root / 'producer.out').read_text()
            if 'test result: ok. 1 passed; 0 failed; 0 ignored;' not in result:
                raise ValueError('exact paired-store integration test must run, not skip')

    def verify_console(self):
        evidence = (self.root / 'producer.err').read_text().splitlines()
        markers = [line for line in evidence if line.startswith('capyio_audio_console')]
        mode = 'virtual_microphone' if self.virtual_scenario else 'playback'
        expected = [console_marker(mode, self.console_scenario)] if self.console else []
        if markers != expected:
            raise ValueError('console evidence missing, duplicated, malformed or unexpected')
        if self.console:
            result = (self.root / 'producer.out').read_text()
            test = ('hosted_console_virtual_microphone_end_to_end' if self.virtual_scenario
                    else 'hosted_console_playback_end_to_end')
            if (f'test {test} ... ok' not in result
                    or 'test result: ok. 1 passed; 0 failed; 0 ignored;' not in result):
                raise ValueError('exact console test must execute, not skip or substitute a Node fixture')

    def inspect_console_owner(self):
        if self.console:
            self.console_owner = verify_console_owner(json.loads(
                (self.root / '.capyio-console-owner.json').read_text()),
                'virtual_microphone' if self.virtual_scenario else 'playback', self.console_scenario)

    def verify_console_reaped(self):
        if self.console_owner and Path('/proc', str(self.console_owner['owner_pid'])).exists():
            raise ValueError('actual console-owned Node was not reaped')

    def pending_console(self):
        return self.console and self.console_scenario in CONSOLE_PENDING

    def finish_pending_console(self, producer, record):
        # Called after independent no-activation observation, before allowing
        # the fixture to send its exact mismatched input or explicit Stop.
        before = graph_fingerprint(self.dump())
        publish_marker(self.root / '.capyio-consent-checked')
        self.wait_checkpoint(producer, '.capyio-after-stop')
        self.verify_console_reaped()
        for _ in range(3):
            objects = self.dump()
            if (virtual_nodes(objects) or any(o.get('info', {}).get('props', {}).get('target.object')
                                             == 'capyio.test.null' for o in objects)):
                raise ValueError('unapproved console activated an audio helper')
            if graph_fingerprint(objects) != before:
                raise ValueError('unapproved console changed the private audio graph')
            self.check()
            time.sleep(.1)
        publish_marker(self.root / '.capyio-stop-checked')
        while producer.poll() is None:
            self.check()
            time.sleep(.02)
        if producer.returncode:
            raise ValueError('console rejection fixture failed')
        self.stop(record)
        self.children.remove(record)

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
        if self.console:
            # The controlling PTY requires a distinct session. Terminate that
            # exact recorded console group before its Rust fixture, so normal
            # Node/helper cleanup gets a chance even after an oracle failure.
            evidence = self.root / '.capyio-console-start.json'
            if evidence.is_file():
                try:
                    value = json.loads(evidence.read_text())
                    if set(value) != {'console_pid'}:
                        raise ValueError('invalid owned console cleanup identity')
                    pid = ownership_number(value['console_pid'], (1 << 31) - 1)
                    process = Path('/proc', str(pid))
                    if process.exists():
                        if ((process / 'exe').resolve(strict=True).name != 'capyio-linux-console'
                                or os.getpgid(pid) != pid):
                            raise ValueError('console cleanup target identity changed')
                        os.killpg(pid, signal.SIGTERM)
                        until = time.monotonic() + 3
                        while process.exists() and time.monotonic() < until:
                            time.sleep(.02)
                        if process.exists():
                            os.killpg(pid, signal.SIGKILL)
                            errors.append(RuntimeError('console required forced cleanup'))
                except (OSError, ValueError, RuntimeError) as error:
                    errors.append(error)
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
                               ('hosted_console_virtual_microphone_end_to_end' if self.console else
                                'hosted_pipewire_virtual_microphone_end_to_end'),
                               '--nocapture', '--test-threads=1'], 'producer')
        self.wait_checkpoint(producer, '.capyio-before-consent')
        expected = virtual_expectation(json.loads(
            (self.root / '.capyio-virtual-microphone.json').read_text()))
        self.inspect_console_owner()
        if self.console and expected['owner_pid'] != self.console_owner['owner_pid']:
            raise ValueError('virtual source owner differs from actual console Node')
        if self.pending_console():
            for _ in range(2):
                if virtual_nodes(self.dump()):
                    raise ValueError('virtual microphone appeared without exact console consent')
                time.sleep(.1)
            self.finish_pending_console(producer, record)
            return
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
        source_pid = verify_process_owner(objects, source, expected)
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
            current_objects = self.dump()
            current_source, _ = select_virtual_source(current_objects, expected)
            if verify_process_owner(current_objects, current_source, expected) != source_pid:
                raise ValueError('owned pw-cat identity changed before fault injection')
            os.kill(source_pid, signal.SIGTERM)
        elif self.virtual_scenario == 'server_disconnect':
            self.wait_checkpoint(producer, '.capyio-fault-ready')
            self.stop(daemon)
            self.children.remove(daemon)
        self.wait_checkpoint(producer, '.capyio-after-stop')
        self.verify_console_reaped()
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
        if producer.returncode or (not self.console and expected_marker not in (self.root / 'producer.err').read_text()):
            raise ValueError('integrated virtual microphone lifecycle evidence absent')
        self.stop(record)
        self.children.remove(record)
        if self.virtual_scenario == 'waveform' and (not self.console or self.console_scenario == 'waveform'):
            verify_signal((self.root / 'capture.out').read_bytes())

    def run(self, fixture):
        daemon = self.prepare_daemon()
        record = self.spawn(['/usr/bin/pw-cat', '--record', '--rate=48000', '--channels=1',
                             '--format=s16', '--channel-map=mono', '--latency=20ms', '--target=0',
                             '--properties={"node.name":"capyio.test.monitor"}', '-'], 'capture')
        producer_args = ([str(fixture), '--ignored', '--exact',
                          ('hosted_console_playback_end_to_end' if self.console else
                           'hosted_pipewire_android_source_end_to_end'), '--nocapture',
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
            self.inspect_console_owner()
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
            if self.pending_console():
                self.finish_pending_console(producer, record)
                return
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
        console_helper = None
        if self.console:
            console_helper, owned_ids = verify_playback_process_owner(objects, self.console_owner['owner_pid'])
            fingerprint = graph_fingerprint(objects)
            console_owned = {key: value for key, value in fingerprint.items() if key in owned_ids}
            console_unrelated = {key: value for key, value in fingerprint.items() if key not in owned_ids}
        for source, target in links:
            if (source, target) in existing_links:
                continue
            self.command(['/usr/bin/pw-link', str(source), str(target)])
        objects = json.loads(self.command(['/usr/bin/pw-dump']))
        actual = {(o.get('info', {}).get('output-port-id'), o.get('info', {}).get('input-port-id'))
                  for o in objects if o.get('type') == 'PipeWire:Interface:Link'}
        if not set(links).issubset(actual):
            raise ValueError('explicit links absent')
        if self.console:
            for obj in objects:
                if obj.get('type') == 'PipeWire:Interface:Link' and (
                        obj.get('info', {}).get('output-port-id') in owned_ids
                        or obj.get('info', {}).get('input-port-id') in owned_ids):
                    console_owned.update(graph_fingerprint([obj]))
        publish_marker(self.root / '.capyio-links-ready')
        if self.console:
            self.wait_checkpoint(producer, '.capyio-after-stop')
            self.verify_console_reaped()
            if Path('/proc', str(console_helper)).exists():
                raise ValueError('console playback helper was not reaped')
            for _ in range(3):
                objects = self.dump()
                verify_virtual_removed(objects, console_owned, console_unrelated)
                if any(o.get('info', {}).get('props', {}).get('target.object') == 'capyio.test.null' for o in objects):
                    raise ValueError('console playback reactivated after termination')
                time.sleep(.1)
            publish_marker(self.root / '.capyio-stop-checked')
        while producer.poll() is None:
            self.check()
            if daemon.poll() is not None or record.poll() is not None:
                raise ValueError('audio graph exited during playback')
            time.sleep(.02)
        if producer.returncode or (not self.console and 'child_reaped=true' not in (self.root / 'producer.err').read_text()):
            raise ValueError('production fixture failed or did not reap pw-cat')
        if self.e2e:
            marker = 'capyio_pipewire_e2e=passed authorized_udp=true stop=true child_reaped=true'
            if not self.console and marker not in (self.root / 'producer.err').read_text():
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
        if not self.console or self.console_scenario == 'waveform':
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
    parser.add_argument('--paired-store', action='store_true')
    parser.add_argument('--console', action='store_true')
    parser.add_argument('--console-scenario', choices=CONSOLE_SCENARIOS)
    args = parser.parse_args()
    if args.console and (not args.paired_store or args.fixture is not None or args.console_scenario is None):
        parser.error('console requires --paired-store, actual executable fixture and explicit --console-scenario')
    if not args.console and args.console_scenario is not None:
        parser.error('console scenario requires --console')
    if args.paired_store and args.fixture is not None:
        parser.error('paired store requires an actual executable fixture')
    if args.virtual_microphone_fixture is None and args.virtual_microphone_scenario is not None:
        parser.error('virtual microphone scenario requires its explicit fixture')
    if args.virtual_microphone_scenario is None:
        args.virtual_microphone_scenario = 'waveform'
    if args.console and args.virtual_microphone_scenario != 'waveform':
        parser.error('console uses --console-scenario; virtual graph mode must stay waveform')
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
                          virtual_scenario=args.virtual_microphone_scenario if virtual else None,
                          paired_store=args.paired_store, console=args.console,
                          console_scenario=args.console_scenario)
        try:
            fixture = (args.fixture or args.e2e_fixture or args.virtual_microphone_fixture)
            if virtual:
                harness.run_virtual(fixture.resolve(strict=True))
            else:
                harness.run(fixture.resolve(strict=True))
            harness.verify_pairing()
            harness.verify_console()
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
    if args.console:
        mode = 'virtual_microphone' if args.virtual_microphone_fixture else 'playback'
        print(f'capyio_audio_console_graph=passed mode={mode} scenario={args.console_scenario}'
              ' actual_console=true protected_read=true independent_graph=true cleanup=true')
        return
    if args.paired_store:
        mode = 'virtual_microphone' if args.virtual_microphone_fixture else 'playback'
        scenario = args.virtual_microphone_scenario if args.virtual_microphone_fixture else 'waveform'
        print(f'capyio_audio_paired_graph=passed mode={mode} scenario={scenario} protected_read=true cleanup=true')
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
