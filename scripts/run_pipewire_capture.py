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

from run_pipewire_loopback import (
    CLIENT, DAEMON, LIMIT, Harness, graph_fingerprint, ownership_number,
    source_process_id, interrupted,
)

SOURCE = 'capyio.test.stereo.source'
MARKER = 'capyio-hosted-pipewire-capture-v1\n'
SCENARIOS = ('waveform', 'child_exit', 'server_disconnect', 'control_disconnect', 'timeout')
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
                or props.get('port.monitor') not in (None, False, 'false')
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


def capture_process_id(objects, capture, owner_pid):
    owner_pid = ownership_number(owner_pid, (1 << 31) - 1)
    name = capture.get('info', {}).get('props', {}).get('node.name')
    if not isinstance(name, str) or not name or capture not in capture_nodes(objects):
        raise ValueError('unexpected production capture identity')
    pid = source_process_id(objects, capture, {'node_name': name})
    process = Path('/proc') / str(pid)
    status = dict(line.split(':', 1) for line in (process / 'status').read_text().splitlines()
                  if ':' in line)
    args = (process / 'cmdline').read_bytes().rstrip(b'\0').split(b'\0')
    required = [b'--record', b'--raw', b'--rate=48000', b'--channels=2',
                b'--channel-map=stereo', b'--format=s16',
                b'--target=' + SOURCE.encode()]
    if (int(status['PPid']) != owner_pid
            or (process / 'exe').resolve() != Path('/usr/bin/pw-cat').resolve()
            or any(arg not in args for arg in required)):
        raise ValueError('recorder is not production owner direct exact-target stereo pw-cat')
    return pid


def verify_capture_removed(objects, owned, unrelated):
    current = graph_fingerprint(objects)
    if capture_nodes(objects) or any(current.get(key) == value for key, value in owned.items()):
        raise ValueError('owned capture node, ports or links survived Stop')
    if any(current.get(key) != value for key, value in unrelated.items()):
        raise ValueError('unrelated private graph object changed during owned Stop')


class CaptureHarness(Harness):
    def __init__(self, root, scenario):
        super().__init__(root, e2e=True)
        self.scenario = scenario
        self.env['CAPYIO_PIPEWIRE_CAPTURE'] = scenario

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
        child = subprocess.Popen([
            '/usr/bin/pw-cat', '--playback', '--rate=48000', '--channels=2',
            '--format=s16', '--channel-map=stereo', '--latency=20ms', '--target=0',
            '--properties=' + json.dumps({'node.name': SOURCE, 'media.class': 'Audio/Source',
                                          'node.virtual': True, 'node.dont-fallback': True,
                                          'node.dont-reconnect': True, 'node.dont-move': True}), '-'],
            env=self.env, stdin=source_input, stdout=out, stderr=err, start_new_session=True,
            preexec_fn=lambda: resource.setrlimit(resource.RLIMIT_FSIZE, (LIMIT, LIMIT)))
        self.children.append(child)
        return child

    def run_capture(self, fixture):
        daemon = self.prepare_daemon()
        source = self.start_source()
        producer = self.spawn([str(fixture), '--ignored', '--exact',
                               'hosted_pipewire_linux_capture_android_sink_end_to_end',
                               '--nocapture', '--test-threads=1'], 'producer')
        for announced, checked in [
                ('.capyio-before-consent', '.capyio-consent-checked'),
                ('.capyio-before-android-consent', '.capyio-android-consent-checked')]:
            self.wait_checkpoint(producer, announced)
            for _ in range(2):
                if capture_nodes(self.dump()):
                    raise ValueError('capture process appeared before both independent consents')
                time.sleep(.1)
            publish_marker(self.root / checked)
        self.wait_checkpoint(producer, '.capyio-capture-owner-ready')
        owner = json.loads((self.root / '.capyio-capture-owner.json').read_text())
        ready = time.monotonic() + 8
        while True:
            self.check()
            if any(child.poll() is not None for child in (daemon, source, producer)):
                raise ValueError('private capture fixture exited before graph readiness')
            objects = self.dump()
            try:
                capture, links = select_capture(objects)
                break
            except ValueError:
                if time.monotonic() >= ready:
                    raise
                time.sleep(.03)
        pid = capture_process_id(objects, capture, owner['owner_pid'])
        fingerprint = graph_fingerprint(objects)
        own_ids = {capture['id']} | {target for _, target in links}
        owned = {key: value for key, value in fingerprint.items() if key in own_ids}
        unrelated = {key: value for key, value in fingerprint.items() if key not in own_ids}
        for output, target in links:
            self.command(['/usr/bin/pw-link', str(output), str(target)])
        objects = self.dump()
        actual_links = [o for o in objects if o.get('type') == 'PipeWire:Interface:Link']
        actual_pairs = {(o.get('info', {}).get('output-port-id'),
                         o.get('info', {}).get('input-port-id')) for o in actual_links}
        if actual_pairs != set(links):
            raise ValueError('expected only explicit independent left/right capture links')
        owned.update(graph_fingerprint(actual_links))
        publish_marker(self.root / '.capyio-links-ready')
        if self.scenario in ('child_exit', 'server_disconnect'):
            self.wait_checkpoint(producer, '.capyio-fault-ready')
            if self.scenario == 'child_exit':
                current = self.dump()
                candidates = capture_nodes(current)
                if len(candidates) != 1 or capture_process_id(current, candidates[0], owner['owner_pid']) != pid:
                    raise ValueError('capture child identity changed before fault injection')
                os.kill(pid, signal.SIGTERM)
            else:
                self.stop(daemon)
                self.children.remove(daemon)
        self.wait_checkpoint(producer, '.capyio-after-stop')
        if Path('/proc', str(pid)).exists():
            raise ValueError('production capture child not reaped')
        if self.scenario == 'server_disconnect':
            # Lost-server objects cannot be claimed preserved. A fresh private daemon
            # must still not recover/reconnect the terminated production recorder.
            daemon = self.start_daemon()
            owned, unrelated = {}, {}
        end = time.monotonic() + .35
        while time.monotonic() < end:
            verify_capture_removed(self.dump(), owned, unrelated)
            self.check()
            time.sleep(.03)
        publish_marker(self.root / '.capyio-stop-checked')
        while producer.poll() is None:
            self.check()
            time.sleep(.02)
        marker = ('capyio_capture_fixture=passed scenario=' + self.scenario
                  + ' android_policy=true shared_rust_receiver=true simulated_app=true fresh_post_stop=true')
        if producer.returncode or marker not in (self.root / 'producer.err').read_text():
            raise ValueError('integrated capture/Android lifecycle evidence absent')
        if self.scenario == 'waveform':
            verify_stereo((self.root / 'received-stereo.pcm').read_bytes())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--hosted-pipewire-capture', action='store_true', required=True)
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
    with tempfile.TemporaryDirectory(prefix='capyio-capture-') as directory:
        harness = CaptureHarness(Path(directory), args.scenario)
        try:
            harness.run_capture(args.fixture.resolve(strict=True))
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
    print(SUCCESS.format(scenario=args.scenario))


if __name__ == '__main__':
    main()
