#!/usr/bin/env python3
"""Hosted Linux capture -> Android policy/shared-Rust receiver, simulated app only."""
import argparse
import array
import json
import math
import os
from pathlib import Path
import resource
import signal
import subprocess
import sys
import tempfile
import time
import uuid

from run_pipewire_loopback import (
    CLIENT, DAEMON, LIMIT, Harness, graph_fingerprint, ownership_number,
    source_process_id, interrupted, CONSOLE_SCENARIOS, CONSOLE_TESTS,
)

SOURCE = 'capyio.test.stereo.source'
MARKER = 'capyio-hosted-pipewire-capture-v1\n'
SCENARIOS = ('waveform', 'child_exit', 'server_disconnect', 'control_disconnect', 'timeout')
VIRTUAL_SCENARIOS = ('waveform', 'no_input', 'child_exit', 'server_disconnect',
                     'control_disconnect', 'expiry')
VIRTUAL_PREFIX = 'capyio.virtual-speaker.'
POISONED_ENV = {
    'PIPEWIRE_NODE': 'capyio.poisoned.node',
    'PIPEWIRE_AUTOCONNECT': 'true',
    'PIPEWIRE_PROPS': '{"node.name":"capyio.poisoned.override","media.class":"Audio/Source",'
                      '"node.autoconnect":true,"node.virtual":false}',
    'PIPEWIRE_QUANTUM': '128/44100',
    'PIPEWIRE_LATENCY': '128/44100',
    'PIPEWIRE_RATE': '1/44100',
}
PRIVATE_ENV = ('PIPEWIRE_REMOTE', 'PIPEWIRE_CONFIG_DIR', 'PIPEWIRE_RUNTIME_DIR',
               'XDG_RUNTIME_DIR', 'XDG_CONFIG_HOME', 'HOME')
VIRTUAL_SUCCESS = ('capyio_pipewire_virtual_speaker=passed scenario={scenario} pinned_tls=true '
                   'independent_consent=true dynamic_udp=true stereo=true real_sink=true '
                   'simulated_app=true environment_sanitized=true stop=true cleanup=true')
SUCCESS = ('capyio_pipewire_capture=passed scenario={scenario} pinned_tls=true '
           'independent_consent=true stereo=true real_graph=true simulated_app=true '
           'stop=true cleanup=true')


def publish_marker(path):
    temporary = path.with_name(path.name + '.tmp')
    with temporary.open('x') as marker:
        marker.write(MARKER)
    temporary.replace(path)


def stereo_tone(left, right, frames):
    samples = array.array('h')
    for i in range(frames):
        samples.extend((round(8192 * math.sin(2 * math.pi * left * i / 48000)),
                        round(6144 * math.sin(2 * math.pi * right * i / 48000))))
    if sys.byteorder != 'little':
        samples.byteswap()
    return samples.tobytes()


def signal_payload():
    return (stereo_tone(1000, 1500, 96000) + bytes(48000 * 4)
            + stereo_tone(2000, 3000, 96000))


def verify_stereo(raw):
    """Require bit-exact ordered five-second L/R signal AND independent spectra."""
    if not raw or len(raw) > LIMIT or len(raw) % 4:
        raise ValueError('invalid bounded interleaved stereo capture')
    expected = signal_payload()
    start = raw.find(expected)
    if start < 0 or start % 4:
        raise ValueError('exact ordered stereo payload absent or channel alignment changed')
    samples = array.array('h', raw[start:start + len(expected)])
    if sys.byteorder != 'little':
        samples.byteswap()
    for offset, wanted in ((0, (1000, 1500)), (3 * 48000, (2000, 3000))):
        for channel, frequency in enumerate(wanted):
            frame = samples[offset * 2 + channel:(offset + 4800) * 2:2]
            amps = {f: 2 * math.hypot(
                sum(x * math.cos(2 * math.pi * f * i / 48000) for i, x in enumerate(frame)),
                sum(x * math.sin(2 * math.pi * f * i / 48000) for i, x in enumerate(frame)))
                / len(frame) for f in (1000, 1500, 2000, 3000)}
            if amps[frequency] < 6000 or any(
                    value > amps[frequency] * .01 for f, value in amps.items() if f != frequency):
                raise ValueError('independent stereo frequency signature absent')
    if any(samples[2 * 48000 * 2:3 * 48000 * 2]):
        raise ValueError('middle stereo segment is not exact silence')


def capture_nodes(objects):
    return [o for o in objects if o.get('type') == 'PipeWire:Interface:Node'
            and o.get('info', {}).get('props', {}).get('target.object') == SOURCE]


def stereo_ports(objects, owner, direction):
    ports = [o for o in objects if o.get('type') == 'PipeWire:Interface:Port'
             and str(o.get('info', {}).get('props', {}).get('node.id')) == str(owner)]
    if len(ports) != 2:
        raise ValueError('exactly two stereo ports required')
    by_channel = {}
    for port in ports:
        props = port.get('info', {}).get('props', {})
        channel = props.get('audio.channel')
        if (props.get('port.direction') != direction
                or (props.get('port.monitor') is not None
                    and not strict_bool(props.get('port.monitor'), False))
                or channel not in ('FL', 'FR') or channel in by_channel):
            raise ValueError('direct distinct FL/FR ports required')
        by_channel[channel] = port['id']
    return by_channel


def select_capture(objects):
    sources = [o for o in objects if o.get('type') == 'PipeWire:Interface:Node'
               and o.get('info', {}).get('props', {}).get('node.name') == SOURCE]
    captures = capture_nodes(objects)
    if len(sources) != 1 or len(captures) != 1:
        raise ValueError('exact synthetic source and production capture not uniquely ready')
    source, capture = sources[0], captures[0]
    props = source.get('info', {}).get('props', {})
    if (props.get('media.class') != 'Audio/Source'
            or props.get('node.virtual') not in (True, 'true')
            or type(props.get('node.virtual')) is int):
        raise ValueError('synthetic target is not a direct virtual Audio/Source')
    outputs = stereo_ports(objects, source['id'], 'out')
    inputs = stereo_ports(objects, capture['id'], 'in')
    if any(o.get('type') == 'PipeWire:Interface:Link' and
           str(o.get('info', {}).get('input-node-id')) == str(capture['id']) for o in objects):
        raise ValueError('capture unexpectedly autolinked before explicit graph setup')
    return capture, [(outputs[channel], inputs[channel]) for channel in ('FL', 'FR')]


def virtual_expectation(value):
    """Accept only the exact fresh approval's local network Source Port identity."""
    if not isinstance(value, dict) or set(value) != {
            'owner_pid', 'source_port_id', 'node_name', 'endpoint_mode'}:
        raise ValueError('exact virtual speaker ownership record required')
    try:
        port_id = str(uuid.UUID(value['source_port_id']))
    except (ValueError, TypeError, AttributeError):
        raise ValueError('canonical virtual speaker Source Port UUID required') from None
    if (value['source_port_id'] != port_id or uuid.UUID(port_id).int == 0
            or value['node_name'] != VIRTUAL_PREFIX + port_id
            or value['endpoint_mode'] != 'virtual_speaker'
            or type(value['owner_pid']) is not int or value['owner_pid'] <= 1):
        raise ValueError('virtual speaker identity is not the exact owned Source Port')
    ownership_number(value['owner_pid'], (1 << 31) - 1)
    return value


def virtual_nodes(objects):
    # The fixed unrelated null sink is a sentinel, never our endpoint. Treat any
    # other Sink or prefixed node as a candidate so renames/class changes fail shut.
    return [o for o in objects if o.get('type') == 'PipeWire:Interface:Node'
            and (str(o.get('info', {}).get('props', {}).get('node.name', '')).startswith(VIRTUAL_PREFIX)
                 or (o.get('info', {}).get('props', {}).get('media.class') == 'Audio/Sink'
                     and o.get('info', {}).get('props', {}).get('node.name') != 'capyio.test.null'))]


def strict_bool(value, wanted):
    return value is wanted or (isinstance(value, str) and value == str(wanted).lower())


def select_virtual_sink(objects, expected):
    expected = virtual_expectation(expected)
    candidates = virtual_nodes(objects)
    sources = [o for o in objects if o.get('type') == 'PipeWire:Interface:Node'
               and o.get('info', {}).get('props', {}).get('node.name') == SOURCE]
    if len(candidates) != 1 or len(sources) != 1:
        raise ValueError('exact virtual speaker and independent app not uniquely ready')
    sink, source = candidates[0], sources[0]
    props = sink.get('info', {}).get('props', {})
    if (props.get('node.name') != expected['node_name']
            or props.get('node.description') != 'CapyIO Virtual Speaker'
            or props.get('media.class') != 'Audio/Sink'
            or ('node.rate' in props and props['node.rate'] != '1/48000')
            or ('audio.rate' in props and props['audio.rate'] not in (48000, '48000'))
            or ('audio.channels' in props and props['audio.channels'] not in (2, '2'))
            or props.get('node.force-rate') in (44100, '44100')
            or props.get('node.force-quantum') in (128, '128')
            or props.get('node.latency') == '128/44100'
            or not strict_bool(props.get('node.virtual'), True)
            or not strict_bool(props.get('node.autoconnect'), False)
            or any(not strict_bool(props.get(key), True) for key in
                   ('node.dont-fallback', 'node.dont-reconnect', 'node.dont-move'))
            or (props.get('target.object') is not None
                and not (props.get('target.object') == '0'
                         or (type(props.get('target.object')) is int
                             and props.get('target.object') == 0)))):
        raise ValueError('owned endpoint must be an exact no-autoconnect virtual Audio/Sink')
    if source.get('info', {}).get('props', {}).get('media.class') != 'Stream/Output/Audio':
        raise ValueError('independent synthetic application output required, not a monitor')
    outputs = stereo_ports(objects, source['id'], 'out')
    inputs = stereo_ports(objects, sink['id'], 'in')
    if any(o.get('type') == 'PipeWire:Interface:Link'
           and (str(o.get('info', {}).get('input-node-id')) == str(sink['id'])
                or str(o.get('info', {}).get('output-node-id')) == str(sink['id']))
           for o in objects):
        raise ValueError('virtual speaker unexpectedly autolinked before explicit app routing')
    return sink, [(outputs[channel], inputs[channel]) for channel in ('FL', 'FR')]


def read_environment(pid):
    pid = ownership_number(pid, (1 << 31) - 1)
    with (Path('/proc') / str(pid) / 'environ').open('rb') as stream:
        raw = stream.read(32769)
    if len(raw) > 32768 or not raw.endswith(b'\0'):
        raise ValueError('bounded owned process environment required')
    items = raw[:-1].split(b'\0')
    if any(b'=' not in item for item in items):
        raise ValueError('invalid owned environment entry')
    pairs = [item.split(b'=', 1) for item in items]
    if len({key for key, _ in pairs}) != len(pairs):
        raise ValueError('duplicate owned environment key')
    return dict(pairs)


def verify_capture_environment(owner_env, capture_env, private_env):
    # Verify the poison really reached the production owner, then that its actual
    # pw-cat child removed every override without losing the private daemon/config.
    for key, value in POISONED_ENV.items():
        if owner_env.get(key.encode()) != value.encode() or key.encode() in capture_env:
            raise ValueError('all six poisoned PipeWire overrides must be removed by production')
    for key in PRIVATE_ENV:
        expected = private_env[key].encode()
        if owner_env.get(key.encode()) != expected or capture_env.get(key.encode()) != expected:
            raise ValueError('owned capture must preserve the exact private routing environment')


def verify_virtual_removed(objects, owned, unrelated):
    current = graph_fingerprint(objects)
    if virtual_nodes(objects) or any(current.get(key) == value for key, value in owned.items()):
        raise ValueError('owned virtual speaker node, ports or links survived Stop')
    if any(current.get(key) != value for key, value in unrelated.items()):
        raise ValueError('unrelated private graph object changed during virtual speaker Stop')


def parse_pipewire_version(raw):
    """Recognize the exact same compiled/linked CLI contract as production."""
    if not isinstance(raw, bytes) or len(raw) > 1024:
        raise ValueError('bounded PipeWire version response required')
    lines = raw.splitlines()
    for version in ('1.0.5', '1.6.9'):
        expected = [b'pw-cat', b'Compiled with libpipewire ' + version.encode(),
                    b'Linked with libpipewire ' + version.encode()]
        if lines == expected:
            return version
    raise ValueError('unknown or mixed PipeWire CLI version')


def pipewire_binary_identity(path):
    stat = path.stat()
    return (stat.st_dev, stat.st_ino, stat.st_size, stat.st_mtime_ns, stat.st_ctime_ns)


def capture_process_id(objects, capture, owner_pid, version, binary_identity, expected=None):
    owner_pid = ownership_number(owner_pid, (1 << 31) - 1)
    name = capture.get('info', {}).get('props', {}).get('node.name')
    candidates = virtual_nodes(objects) if expected is not None else capture_nodes(objects)
    if (not isinstance(name, str) or not name or capture not in candidates
            or (expected is not None and name != virtual_expectation(expected)['node_name'])):
        raise ValueError('unexpected production capture identity')
    pid = source_process_id(objects, capture, {'node_name': name})
    process = Path('/proc') / str(pid)
    status = dict(line.split(':', 1) for line in (process / 'status').read_text().splitlines()
                  if ':' in line)
    args = (process / 'cmdline').read_bytes().rstrip(b'\0').split(b'\0')
    # These are different raw-stdio CLI contracts, never interchangeable guesses.
    if version not in ('1.0.5', '1.6.9'):
        raise ValueError('capture ownership requires a recognized exact CLI version')
    required = [b'--record', b'--rate=48000', b'--channels=2',
                b'--channel-map=stereo', b'--format=s16',
                b'--target=' + (b'0' if expected is not None else SOURCE.encode())]
    if (int(status['PPid']) != owner_pid
            or (process / 'exe').resolve() != Path('/usr/bin/pw-cat').resolve()
            or pipewire_binary_identity(process / 'exe') != binary_identity
            or pipewire_binary_identity(Path('/usr/bin/pw-cat')) != binary_identity
            or args.count(b'--raw') != (1 if version == '1.6.9' else 0)
            or not args or args[-1] != b'-'
            or b'--playback' in args or b'--monitor' in args
            or any(args.count(arg) != 1 for arg in required)
            or any(sum(arg.startswith(prefix) for arg in args) != 1 for prefix in
                   (b'--target=', b'--rate=', b'--channels=', b'--channel-map=', b'--format='))):
        raise ValueError('recorder is not production owner direct exact-target stereo pw-cat')
    if expected is not None:
        properties = [arg.split(b'=', 1)[1] for arg in args if arg.startswith(b'--properties=')]
        wanted = {'media.class': 'Audio/Sink', 'node.virtual': True,
                  'node.name': expected['node_name'], 'node.description': 'CapyIO Virtual Speaker',
                  'node.autoconnect': False, 'node.dont-fallback': True,
                  'node.dont-reconnect': True, 'node.dont-move': True}
        if (len(properties) != 1 or args.count(b'--latency=20ms') != 1
                or sum(arg.startswith(b'--latency=') for arg in args) != 1):
            raise ValueError('virtual speaker requires exact owned arguments')
        try:
            actual = json.loads(properties[0])
        except (ValueError, UnicodeError):
            raise ValueError('virtual speaker requires exact owned properties') from None
        if actual != wanted or any(type(actual[key]) is not type(value) for key, value in wanted.items()):
            raise ValueError('virtual speaker requires exact owned properties')
    return pid


def verify_capture_removed(objects, owned, unrelated):
    current = graph_fingerprint(objects)
    if capture_nodes(objects) or any(current.get(key) == value for key, value in owned.items()):
        raise ValueError('owned capture node, ports or links survived Stop')
    if any(current.get(key) != value for key, value in unrelated.items()):
        raise ValueError('unrelated private graph object changed during owned Stop')


def diagnostic_number(value):
    try:
        return ownership_number(value, (1 << 31) - 1)
    except ValueError:
        return None


def diagnostic_choice(value, allowed):
    # No arbitrary graph names, paths, argv strings or properties enter evidence.
    return value if isinstance(value, str) and value in allowed else None


def graph_diagnostic(objects):
    """Bounded allowlisted graph evidence; never weakens strict graph selection."""
    types = ('Node', 'Port', 'Client', 'Link')
    groups = {kind: [o for o in objects if o.get('type') == 'PipeWire:Interface:' + kind]
              for kind in types}
    result = {'counts': {kind: len(items) for kind, items in groups.items()},
              'truncated': any(len(items) > 32 for items in groups.values())}
    result['nodes'] = []
    for item in groups['Node'][:32]:
        info = item.get('info', {})
        props = info.get('props', {})
        target = props.get('target.object')
        result['nodes'].append({
            'id': diagnostic_number(item.get('id')),
            'serial': diagnostic_number(props.get('object.serial')),
            'name': diagnostic_choice(props.get('node.name'),
                                      (SOURCE, 'pw-cat', 'Dummy-Driver', 'capyio.test.null')),
            'target_is_exact_source': target == SOURCE,
            'name_is_virtual_speaker': str(props.get('node.name', '')).startswith(VIRTUAL_PREFIX),
            'virtual': strict_bool(props.get('node.virtual'), True),
            'autoconnect_disabled': strict_bool(props.get('node.autoconnect'), False),
            'target_present': 'target.object' in props,
            'target_numeric': diagnostic_number(target),
            'client_id': diagnostic_number(props.get('client.id')),
            'state': diagnostic_choice(info.get('state'),
                                       ('creating', 'suspended', 'idle', 'running', 'error')),
            'media_class': diagnostic_choice(props.get('media.class'),
                                             ('Audio/Source', 'Audio/Sink', 'Stream/Input/Audio',
                                              'Stream/Output/Audio')),
        })
    result['ports'] = []
    for item in groups['Port'][:32]:
        props = item.get('info', {}).get('props', {})
        result['ports'].append({
            'id': diagnostic_number(item.get('id')),
            'node_id': diagnostic_number(props.get('node.id')),
            'direction': diagnostic_choice(props.get('port.direction'), ('in', 'out')),
            'channel': diagnostic_choice(props.get('audio.channel'), ('FL', 'FR', 'MONO')),
            'monitor': props.get('port.monitor') in (True, 'true'),
        })
    result['clients'] = []
    for item in groups['Client'][:32]:
        props = item.get('info', {}).get('props', {})
        result['clients'].append({
            'id': diagnostic_number(item.get('id')),
            'pid': diagnostic_number(props.get('application.process.id')),
            'secure_pid': diagnostic_number(props.get('pipewire.sec.pid')),
        })
    result['links'] = []
    for item in groups['Link'][:32]:
        info = item.get('info', {})
        result['links'].append({
            'id': diagnostic_number(item.get('id')),
            'source_port': diagnostic_number(info.get('output-port-id')),
            'sink_port': diagnostic_number(info.get('input-port-id')),
        })
    return result


def process_diagnostic(pid):
    """Read only an owned fixture PID; report numeric state and option booleans."""
    pid = diagnostic_number(pid)
    result = {'pid': pid, 'readable': False}
    if pid is None:
        return result
    root = Path('/proc') / str(pid)
    try:
        with (root / 'status').open('rb') as file:
            status = file.read(8193)
        with (root / 'cmdline').open('rb') as file:
            command = file.read(8193)
        if len(status) > 8192 or len(command) > 8192:
            return result | {'truncated': True}
        fields = dict(line.split(':', 1) for line in status.decode('ascii', errors='replace').splitlines()
                      if ':' in line)
        args = command.rstrip(b'\0').split(b'\0')
        result.update({
            'readable': True,
            'ppid': diagnostic_number(fields.get('PPid', '').strip()),
            'state': diagnostic_choice(fields.get('State', '').strip()[:1], ('R', 'S', 'D', 'Z', 'T', 'I')),
            'record': b'--record' in args,
            'playback': b'--playback' in args,
            'raw_option': b'--raw' in args,
            'stereo': b'--channels=2' in args and b'--channel-map=stereo' in args,
            'target_exact': b'--target=' + SOURCE.encode() in args,
            'target_disabled': b'--target=0' in args,
            'stdio_pcm': bool(args) and args[-1] == b'-',
        })
        with (root / 'task' / str(pid) / 'children').open('rb') as file:
            children = file.read(1025)
        if len(children) <= 1024:
            result['children'] = [diagnostic_number(value) for value in
                                  children.decode('ascii', errors='replace').split()[:8]]
        else:
            result['children_truncated'] = True
    except OSError:
        pass
    return result


class CaptureHarness(Harness):
    def __init__(self, root, scenario, endpoint_mode='selected_source', paired_store=False,
                 console=False, console_scenario=None):
        super().__init__(root, e2e=True, paired_store=paired_store, console=console,
                         console_scenario=console_scenario,
                         console_mode=('virtual_speaker' if endpoint_mode == 'virtual_speaker'
                                       else 'capture') if console else None)
        if console and scenario != 'waveform':
            raise ValueError('console uses its distinct lifecycle scenario; graph profile stays waveform')
        if endpoint_mode not in ('selected_source', 'virtual_speaker'):
            raise ValueError('explicit recognized capture endpoint mode required')
        allowed = VIRTUAL_SCENARIOS if endpoint_mode == 'virtual_speaker' else SCENARIOS
        if scenario not in allowed:
            raise ValueError('scenario is not valid for this endpoint mode')
        self.endpoint_mode = endpoint_mode
        self.virtual = endpoint_mode == 'virtual_speaker'
        self.scenario = scenario
        self.env['CAPYIO_PIPEWIRE_ENDPOINT_MODE'] = endpoint_mode
        self.env['CAPYIO_PIPEWIRE_CAPTURE'] = scenario
        self.diagnostic_processes = {}
        self.pipewire_version = None
        self.pipewire_identity = None

    def report_graph_failure(self, objects):
        report = graph_diagnostic(objects)
        report['pipewire_version'] = self.pipewire_version
        report['processes'] = {role: process_diagnostic(pid)
                               for role, pid in self.diagnostic_processes.items()}
        owner = report['processes'].get('owner', {})
        report['owner_children'] = [process_diagnostic(pid) for pid in owner.get('children', [])]
        # Fixed collection bounds and typed values keep this below 32 KiB even
        # with malformed/unrelated properties. No raw pw-dump or /proc output.
        encoded = json.dumps(report, separators=(',', ':'), sort_keys=True)
        if len(encoded) > 32768:
            raise ValueError('sanitized graph diagnostic exceeded fixed bound')
        print('capyio_capture_graph_failure=' + encoded, file=sys.stderr)

    def prepare_daemon(self):
        for name, content in [('daemon.conf', DAEMON), ('client.conf', CLIENT),
                              ('client-rt.conf', CLIENT)]:
            (self.root / name).write_text(content)
        publish_marker(self.root / '.capyio-pipewire-capture')
        return self.start_daemon()

    def start_source(self):
        # Only this generated public signal enters the private source. No microphone,
        # session monitor, default device, inherited user config or service is used.
        pcm = self.root / 'generated-stereo.pcm'
        pcm.write_bytes(bytes(24000 * 4) + signal_payload() + bytes(120000 * 4))
        source_input = pcm.open('rb')
        self.files.append(source_input)
        out = (self.root / 'source.out').open('wb')
        err = (self.root / 'source.err').open('wb')
        self.files.extend([out, err])
        if self.pipewire_version not in ('1.0.5', '1.6.9'):
            raise ValueError('source requires a recognized exact CLI version')
        raw_option = ['--raw'] if self.pipewire_version == '1.6.9' else []
        child = subprocess.Popen([
            '/usr/bin/pw-cat', '--playback', *raw_option, '--rate=48000', '--channels=2',
            '--format=s16', '--channel-map=stereo', '--latency=20ms', '--target=0',
            '--properties=' + json.dumps({'node.name': SOURCE, 'media.class': ('Stream/Output/Audio' if self.virtual else 'Audio/Source'),
                                          'node.virtual': True, 'node.dont-fallback': True,
                                          'node.dont-reconnect': True, 'node.dont-move': True}), '-'],
            env=self.env, stdin=source_input, stdout=out, stderr=err, start_new_session=True,
            preexec_fn=lambda: resource.setrlimit(resource.RLIMIT_FSIZE, (LIMIT, LIMIT)))
        self.children.append(child)
        return child

    def finish_pending_capture_console(self, producer):
        before = graph_fingerprint(self.dump())
        publish_marker(self.root / '.capyio-consent-checked')
        self.wait_checkpoint(producer, '.capyio-after-stop')
        self.verify_console_reaped()
        for _ in range(3):
            objects = self.dump()
            if capture_nodes(objects) or virtual_nodes(objects):
                raise ValueError('unapproved console activated a capture helper')
            if graph_fingerprint(objects) != before:
                raise ValueError('unapproved capture console changed the private audio graph')
            self.check()
            time.sleep(.1)
        publish_marker(self.root / '.capyio-stop-checked')
        while producer.poll() is None:
            self.check()
            time.sleep(.02)
        if producer.returncode:
            raise ValueError('capture console rejection fixture failed')

    def run_capture(self, fixture):
        # Probe only version, never audio or defaults. The known binary must stay
        # unchanged between this bounded read and later process ownership checks.
        binary = Path('/usr/bin/pw-cat')
        self.pipewire_identity = pipewire_binary_identity(binary)
        self.pipewire_version = parse_pipewire_version(self.command([str(binary), '--version']))
        if pipewire_binary_identity(binary) != self.pipewire_identity:
            raise ValueError('PipeWire binary changed during version probe')
        daemon = self.prepare_daemon()
        source = self.start_source()
        producer = self.spawn([str(fixture), '--ignored', '--exact',
                               (CONSOLE_TESTS[self.console_mode] if self.console else
                                'hosted_pipewire_linux_virtual_speaker_android_sink_end_to_end' if self.virtual
                                else 'hosted_pipewire_linux_capture_android_sink_end_to_end'),
                               '--nocapture', '--test-threads=1'], 'producer')
        self.diagnostic_processes = {'daemon': daemon.pid, 'source': source.pid, 'fixture': producer.pid}
        for announced, checked in [
                ('.capyio-before-consent', '.capyio-consent-checked'),
                ('.capyio-before-android-consent', '.capyio-android-consent-checked')]:
            self.wait_checkpoint(producer, announced)
            if announced == '.capyio-before-consent':
                self.inspect_console_owner()
            for _ in range(2):
                if (virtual_nodes if self.virtual else capture_nodes)(self.dump()):
                    raise ValueError('capture process appeared before both independent consents')
                time.sleep(.1)
            if self.pending_console():
                self.finish_pending_capture_console(producer)
                return
            publish_marker(self.root / checked)
        self.wait_checkpoint(producer, '.capyio-capture-owner-ready')
        owner = json.loads((self.root / '.capyio-capture-owner.json').read_text())
        if self.virtual:
            virtual_expectation(owner)
        if self.console and owner['owner_pid'] != self.console_owner['owner_pid']:
            raise ValueError('capture helper owner differs from actual console Node')
        self.diagnostic_processes['owner'] = owner['owner_pid']
        objects = []
        ready = time.monotonic() + 8
        while True:
            self.check()
            if any(child.poll() is not None for child in (daemon, source, producer)):
                self.report_graph_failure(objects)
                raise ValueError('private capture fixture exited before graph readiness')
            objects = self.dump()
            try:
                capture, links = (select_virtual_sink(objects, owner) if self.virtual
                                  else select_capture(objects))
                break
            except ValueError:
                if time.monotonic() >= ready:
                    self.report_graph_failure(objects)
                    raise
                time.sleep(.03)
        try:
            pid = capture_process_id(objects, capture, owner['owner_pid'],
                                     self.pipewire_version, self.pipewire_identity,
                                     owner if self.virtual else None)
            if self.virtual or self.console:
                app = next(o for o in objects if o.get('type') == 'PipeWire:Interface:Node'
                           and o.get('info', {}).get('props', {}).get('node.name') == SOURCE)
                if (source_process_id(objects, app, {'node_name': SOURCE}) != source.pid
                        or source.pid in (pid, owner['owner_pid'])):
                    raise ValueError('synthetic application must be an independent owned process')
                if self.virtual:
                    verify_capture_environment(read_environment(owner['owner_pid']),
                                               read_environment(pid), self.env)
                sentinels = [o for o in objects if o.get('type') == 'PipeWire:Interface:Node'
                             and o.get('info', {}).get('props', {}).get('node.name') == 'capyio.test.null']
                if len(sentinels) != 1:
                    raise ValueError('unrelated private sentinel must exist before Stop')
        except (OSError, ValueError, KeyError):
            self.report_graph_failure(objects)
            raise
        fingerprint = graph_fingerprint(objects)
        own_ids = {capture['id']} | {target for _, target in links}
        owned = {key: value for key, value in fingerprint.items() if key in own_ids}
        unrelated = {key: value for key, value in fingerprint.items() if key not in own_ids}
        routed = [] if self.scenario == 'no_input' else links
        for output, target in routed:
            self.command(['/usr/bin/pw-link', str(output), str(target)])
        objects = self.dump()
        actual_links = [o for o in objects if o.get('type') == 'PipeWire:Interface:Link']
        actual_pairs = {(o.get('info', {}).get('output-port-id'),
                         o.get('info', {}).get('input-port-id')) for o in actual_links}
        if len(actual_links) != len(routed) or actual_pairs != set(routed):
            raise ValueError('expected only explicit independent left/right capture links')
        owned.update(graph_fingerprint(actual_links))
        publish_marker(self.root / '.capyio-links-ready')
        if self.scenario in ('child_exit', 'server_disconnect'):
            self.wait_checkpoint(producer, '.capyio-fault-ready')
            if self.scenario == 'child_exit':
                current = self.dump()
                candidates = (virtual_nodes if self.virtual else capture_nodes)(current)
                if len(candidates) != 1 or capture_process_id(current, candidates[0], owner['owner_pid'],
                                                                   self.pipewire_version, self.pipewire_identity,
                                                                   owner if self.virtual else None) != pid:
                    raise ValueError('capture child identity changed before fault injection')
                os.kill(pid, signal.SIGTERM)
            else:
                self.stop(daemon)
                self.children.remove(daemon)
        self.wait_checkpoint(producer, '.capyio-after-stop')
        self.verify_console_reaped()
        if Path('/proc', str(pid)).exists():
            raise ValueError('production capture child not reaped')
        if self.scenario == 'server_disconnect':
            # Lost-server objects cannot be claimed preserved. A fresh private daemon
            # must still not recover/reconnect the terminated production recorder.
            daemon = self.start_daemon()
            owned, unrelated = {}, {}
        end = time.monotonic() + .35
        while time.monotonic() < end:
            (verify_virtual_removed if self.virtual else verify_capture_removed)(self.dump(), owned, unrelated)
            self.check()
            time.sleep(.03)
        publish_marker(self.root / '.capyio-stop-checked')
        while producer.poll() is None:
            self.check()
            time.sleep(.02)
        marker = (('capyio_virtual_speaker_fixture' if self.virtual else 'capyio_capture_fixture')
                  + '=passed scenario=' + self.scenario
                  + ' android_policy=true shared_rust_receiver=true simulated_app=true fresh_post_stop=true')
        if producer.returncode or (not self.console and marker not in (self.root / 'producer.err').read_text()):
            raise ValueError('integrated capture/Android lifecycle evidence absent')
        if self.scenario == 'waveform' and (not self.console or self.console_scenario == 'waveform'):
            verify_stereo((self.root / 'received-stereo.pcm').read_bytes())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--hosted-pipewire-capture', action='store_true', required=True)
    parser.add_argument('--fixture', required=True, type=Path)
    parser.add_argument('--scenario', choices=tuple(dict.fromkeys(SCENARIOS + VIRTUAL_SCENARIOS)), required=True)
    parser.add_argument('--endpoint-mode', choices=('selected_source', 'virtual_speaker'),
                        default='selected_source')
    parser.add_argument('--paired-store', action='store_true')
    parser.add_argument('--console', action='store_true')
    parser.add_argument('--console-scenario', choices=CONSOLE_SCENARIOS)
    args = parser.parse_args()
    if args.console and (not args.paired_store or args.console_scenario is None or args.scenario != 'waveform'):
        parser.error('console requires --paired-store, --scenario waveform and explicit --console-scenario')
    if not args.console and args.console_scenario is not None:
        parser.error('console scenario requires --console')
    if os.environ.get('GITHUB_ACTIONS') != 'true':
        parser.error('requires explicit disposable hosted CI environment')
    for tool in ('pipewire', 'pw-cat', 'pw-dump', 'pw-link'):
        if not os.access('/usr/bin/' + tool, os.X_OK):
            parser.error('required official PipeWire tool absent; no installation fallback')
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    with tempfile.TemporaryDirectory(prefix='capyio-capture-') as directory:
        harness = CaptureHarness(Path(directory), args.scenario, args.endpoint_mode,
                                 paired_store=args.paired_store, console=args.console,
                                 console_scenario=args.console_scenario)
        try:
            harness.run_capture(args.fixture.resolve(strict=True))
            harness.verify_pairing()
            harness.verify_console()
        except Exception:
            for name in ('daemon.err', 'producer.err', 'source.err', 'command.err'):
                path = Path(directory) / name
                if path.is_file():
                    with path.open('rb') as log:
                        print(name + ': ' + log.read(4096).decode('utf-8', errors='replace'), file=sys.stderr)
            raise
        finally:
            signal.signal(signal.SIGTERM, signal.SIG_IGN)
            signal.signal(signal.SIGINT, signal.SIG_IGN)
            harness.close()
    if args.console:
        mode = 'virtual_speaker' if args.endpoint_mode == 'virtual_speaker' else 'capture'
        print(f'capyio_audio_console_graph=passed mode={mode} scenario={args.console_scenario}'
              ' actual_console=true protected_read=true independent_graph=true cleanup=true')
        return
    if args.paired_store:
        mode = 'virtual_speaker' if args.endpoint_mode == 'virtual_speaker' else 'capture'
        print(f'capyio_audio_paired_graph=passed mode={mode} scenario={args.scenario} protected_read=true cleanup=true')
    print((VIRTUAL_SUCCESS if args.endpoint_mode == 'virtual_speaker' else SUCCESS)
          .format(scenario=args.scenario))


if __name__ == '__main__':
    main()
