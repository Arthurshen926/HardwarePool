"""Pure checks only: no subprocess, daemon, socket or audio device access."""
import math
import copy
import io
import json
import struct
import unittest
from unittest.mock import Mock, patch, call
from pathlib import Path
import signal
from run_pipewire_loopback import (
    LIMIT, VIRTUAL_PREFIX, verify_signal, select_ports, Harness, interrupted, verify_silence,
    virtual_expectation, virtual_nodes, select_virtual_source, recorder_port,
    graph_fingerprint, verify_virtual_removed,
    ownership_number, source_process_id, ownership_diagnostic, verify_process_owner,
    CONSOLE_SCENARIOS, CONSOLE_PENDING, console_marker, verify_console_owner,
    verify_playback_process_owner,
)


def tone(hz, seconds):
    return b''.join(struct.pack('<h', round(8192 * math.sin(2 * math.pi * hz * i / 48000)))
                    for i in range(round(seconds * 48000)))


class SignalTests(unittest.TestCase):
    def test_expected_signature_with_leading_silence_and_partial_window(self):
        verify_signal(bytes(1234) + tone(1000, 2) + bytes(96000) + tone(2000, 2))

    def test_silence_wrong_frequency_order_and_noise_fail(self):
        for raw in (bytes(480000), tone(3000, 5),
                    tone(2000, 2) + bytes(96000) + tone(1000, 2),
                    struct.pack('<hh', 12000, -12000) * 120000):
            with self.subTest(length=len(raw)), self.assertRaises(ValueError):
                verify_signal(raw)

    def test_short_loud_and_unbounded_inputs_fail(self):
        for raw in (b'x', bytes(LIMIT + 2), tone(1000, .2),
                    struct.pack('<h', 32767) * 240000):
            with self.assertRaises(ValueError):
                verify_signal(raw)

    def test_preconsent_and_stop_silence_needs_actual_samples(self):
        verify_silence(bytes(9600), minimum_samples=4800)
        for raw in (b'', bytes(9598), tone(1000, .1)):
            with self.assertRaises(ValueError):
                verify_silence(raw, minimum_samples=4800)

    def test_graph_requires_unique_exact_nodes(self):
        with self.assertRaises(ValueError):
            select_ports([])
        nodes = [dict(id=i, type='PipeWire:Interface:Node', info={'props': props})
                 for i, props in enumerate(({'node.name': 'capyio.test.null'},
                                            {'node.name': 'capyio.test.monitor'},
                                            {'target.object': 'capyio.test.null'}))]
        ports = [dict(id=i + 10, type='PipeWire:Interface:Port', info={'props': {
            'node.id': owner, 'port.direction': direction}})
                 for i, (owner, direction) in enumerate(((2, 'out'), (0, 'in'),
                                                        (0, 'out'), (1, 'in')))]
        self.assertEqual(select_ports(nodes + ports), [(10, 11), (12, 13)])
        self.assertEqual(select_ports(nodes[:2] + ports, monitor_only=True), [(12, 13)])
        with self.assertRaises(ValueError):
            select_ports(nodes + ports + [ports[0]])


class VirtualMicrophoneTests(unittest.TestCase):
    def setUp(self):
        self.expected = {
            'sink_port_id': 'b3ed3c7c-f630-407b-923a-d3770e0ee624',
            'node_name': VIRTUAL_PREFIX + 'b3ed3c7c-f630-407b-923a-d3770e0ee624',
            'owner_pid': 123,
        }
        self.source = dict(id=50, type='PipeWire:Interface:Node', info={'props': {
            'node.name': self.expected['node_name'], 'media.class': 'Audio/Source',
            'node.virtual': True, 'client.id': 40, 'application.process.id': 124,
            'object.serial': 150}})
        self.client = dict(id=40, type='PipeWire:Interface:Client', info={'props': {
            'application.process.id': 124, 'pipewire.sec.pid': 124, 'object.serial': 140}})
        self.output = dict(id=51, type='PipeWire:Interface:Port', info={'props': {
            'node.id': '50', 'port.direction': 'out', 'audio.channel': 'MONO',
            'object.serial': 151}})
        self.recorder = dict(id=60, type='PipeWire:Interface:Node', info={'props': {
            'node.name': 'capyio.test.monitor', 'object.serial': 160}})
        self.input = dict(id=61, type='PipeWire:Interface:Port', info={'props': {
            'node.id': 60, 'port.direction': 'in', 'object.serial': 161}})
        self.unrelated = [self.recorder, self.input]
        self.graph = [self.source, self.output] + self.unrelated

    def test_generated_name_is_exact_canonical_local_sink_not_external_target(self):
        self.assertEqual(virtual_expectation(self.expected), self.expected)
        for updates in (
                {'sink_port_id': self.expected['sink_port_id'].upper()},
                {'node_name': 'capyio.test.null'},
                {'node_name': VIRTUAL_PREFIX + 'c3ed3c7c-f630-407b-923a-d3770e0ee624'},
                {'sink_port_id': 'not-a-port-id'},
                {'owner_pid': True}, {'owner_pid': 0}, {'owner_pid': '123'}):
            with self.subTest(updates=updates), self.assertRaises(ValueError):
                virtual_expectation(self.expected | updates)

    def test_direct_audio_source_not_sink_monitor_is_selected(self):
        self.assertEqual(select_virtual_source(self.graph, self.expected),
                         (self.source, self.output))
        self.assertEqual(recorder_port(self.graph), 61)
        self.assertEqual(virtual_nodes(self.unrelated), [])
        for props in (
                {'media.class': 'Audio/Sink'}, {'media.class': 'Stream/Output/Audio'},
                {'node.name': 'renamed.null.monitor'}, {'node.virtual': False},
                {'node.virtual': 1}, {'node.virtual': 'false'}):
            graph = copy.deepcopy(self.graph)
            graph[0]['info']['props'].update(props)
            with self.subTest(props=props), self.assertRaises(ValueError):
                select_virtual_source(graph, self.expected)
        for props in (
                {'port.direction': 'in'}, {'port.monitor': True},
                {'port.monitor': 'true'}, {'audio.channel': 'FL'},
                {'node.id': 60}):
            graph = copy.deepcopy(self.graph)
            graph[1]['info']['props'].update(props)
            with self.subTest(props=props), self.assertRaises(ValueError):
                select_virtual_source(graph, self.expected)

    def test_duplicate_source_extra_ports_and_automatic_links_are_rejected(self):
        link = dict(id=70, type='PipeWire:Interface:Link', info={
            'output-node-id': 50, 'input-node-id': 60,
            'output-port-id': 51, 'input-port-id': 61})
        another_source = dict(id=80, type='PipeWire:Interface:Node', info={'props': {
            'node.name': 'unrelated.source', 'media.class': 'Audio/Source'}})
        for extra in (self.source, self.output, link, another_source):
            with self.subTest(extra=extra), self.assertRaises(ValueError):
                select_virtual_source(self.graph + [extra], self.expected)
        for graph in ([], [self.source], self.graph + [self.recorder], self.graph + [self.input]):
            with self.assertRaises(ValueError):
                recorder_port(graph)

    def test_stop_requires_exact_owned_objects_gone_and_unrelated_objects_retained(self):
        unrelated = graph_fingerprint(self.unrelated)
        owned = graph_fingerprint([self.source, self.output])
        verify_virtual_removed(self.unrelated, owned, unrelated)
        # PipeWire may reuse global IDs, but an old object.serial must be gone.
        reused = copy.deepcopy(self.output)
        reused['info']['props'].update({'node.id': 60, 'object.serial': 999})
        verify_virtual_removed(self.unrelated + [reused], owned, unrelated)
        link = dict(id=70, type='PipeWire:Interface:Link',
                    info={'props': {'object.serial': 170}})
        owned.update(graph_fingerprint([link]))
        changed_unrelated = copy.deepcopy(self.unrelated)
        changed_unrelated[0]['info']['props']['object.serial'] = 260
        for graph in (self.graph, self.unrelated + [self.output], self.unrelated + [link],
                      [self.recorder], changed_unrelated):
            with self.subTest(graph=graph), self.assertRaises(ValueError):
                verify_virtual_removed(graph, owned, unrelated)
        # A fresh post-Stop Source must fail even with entirely new IDs/serials.
        fresh = copy.deepcopy(self.source)
        fresh['id'] = 90
        fresh['info']['props']['object.serial'] = 190
        with self.assertRaises(ValueError):
            verify_virtual_removed(self.unrelated + [fresh], owned, unrelated)

    def test_source_process_identity_comes_from_exact_owning_client(self):
        # The upstream Node shape need not repeat Client process properties.
        source = copy.deepcopy(self.source)
        del source['info']['props']['application.process.id']
        graph = [source, self.client]
        self.assertEqual(source_process_id(graph, source, self.expected), 124)
        self.assertEqual(source_process_id([self.client], self.source, self.expected), 124)
        client = copy.deepcopy(self.client)
        client['info']['props'].pop('pipewire.sec.pid')
        client['info']['props']['application.process.id'] = '124'
        source['info']['props']['client.id'] = '40'
        self.assertEqual(source_process_id([client], source, self.expected), 124)

    def test_missing_duplicate_or_mistyped_client_cannot_fallback_to_node_pid(self):
        wrong_type = copy.deepcopy(self.client)
        wrong_type['type'] = 'PipeWire:Interface:Node'
        wrong_id = copy.deepcopy(self.client)
        wrong_id['id'] = 41
        wrong_id_type = copy.deepcopy(self.client)
        wrong_id_type['id'] = '40'
        no_pid = copy.deepcopy(self.client)
        del no_pid['info']['props']['application.process.id']
        for graph in ([], [self.source], [self.client, self.client],
                      [self.client, wrong_type], [wrong_type], [wrong_id],
                      [wrong_id_type], [no_pid]):
            with self.subTest(graph=graph), self.assertRaises(ValueError):
                source_process_id(graph, self.source, self.expected)
        source, client = copy.deepcopy(self.source), copy.deepcopy(self.client)
        source['info']['props']['client.id'] = 1
        client['id'] = True
        with self.assertRaises(ValueError):
            source_process_id([client], source, self.expected)

    def test_invalid_ownership_numbers_and_conflicting_identities_fail_closed(self):
        for value in (None, True, False, -1, 0, 1.0, '', ' 124', '0124',
                      '1e2', '１２４', '9' * 1000, [], {}, 1 << 32):
            with self.subTest(value=value), self.assertRaises(ValueError):
                ownership_number(value)
            source = copy.deepcopy(self.source)
            source['info']['props']['client.id'] = value
            with self.assertRaises(ValueError):
                source_process_id([self.client], source, self.expected)
        for value in (None, True, False, -1, 0, 1, 124.0, '', '0124', 1 << 31):
            client = copy.deepcopy(self.client)
            client['info']['props']['application.process.id'] = value
            with self.subTest(pid=value), self.assertRaises(ValueError):
                source_process_id([client], self.source, self.expected)
        for target, key, value in (
                ('node', 'application.process.id', 125),
                ('node', 'application.process.id', None),
                ('node', 'node.name', 'different.source'),
                ('client', 'pipewire.sec.pid', 125),
                ('client', 'pipewire.sec.pid', None)):
            source, client = copy.deepcopy(self.source), copy.deepcopy(self.client)
            (source if target == 'node' else client)['info']['props'][key] = value
            with self.subTest(target=target, key=key), self.assertRaises(ValueError):
                source_process_id([client], source, self.expected)

    def test_proc_executable_parent_and_no_autolink_checks_still_required(self):
        graph = [self.source, self.client]
        stderr = io.StringIO()
        with patch('run_pipewire_loopback.Path.read_text', return_value='PPid:\t123\n'), \
             patch('run_pipewire_loopback.Path.read_bytes',
                   return_value=b'/usr/bin/pw-cat\0--playback\0--target=0\0-'), \
             patch('run_pipewire_loopback.Path.resolve', return_value=Path('/usr/bin/pw-cat')), \
             patch('run_pipewire_loopback.sys.stderr', stderr):
            self.assertEqual(verify_process_owner(graph, self.source, self.expected), 124)
            with patch('run_pipewire_loopback.Path.read_text', return_value='PPid:\t999\n'), \
                 self.assertRaises(ValueError):
                verify_process_owner(graph, self.source, self.expected)
            with patch('run_pipewire_loopback.Path.resolve',
                       side_effect=[Path('/usr/bin/unrelated'), Path('/usr/bin/pw-cat')]), \
                 self.assertRaises(ValueError):
                verify_process_owner(graph, self.source, self.expected)
            for args in (b'', b'/usr/bin/pw-cat\0--record\0--target=0\0-',
                         b'/usr/bin/pw-cat\0--playback\0--target=auto\0-'):
                with patch('run_pipewire_loopback.Path.read_bytes', return_value=args), \
                     self.assertRaises(ValueError):
                    verify_process_owner(graph, self.source, self.expected)
            with patch('run_pipewire_loopback.Path.read_text', side_effect=FileNotFoundError), \
                 self.assertRaises(FileNotFoundError):
                verify_process_owner(graph, self.source, self.expected)
        self.assertEqual(stderr.getvalue().count('capyio_pipewire_ownership='), 6)

    def test_ownership_failure_diagnostics_are_bounded_numeric_allowlist_only(self):
        secret = 'PRIVATE_GRAPH_VALUE_MUST_NOT_ESCAPE'
        source, client = copy.deepcopy(self.source), copy.deepcopy(self.client)
        source['info']['props'].update({'application.process.id': secret, 'secret': secret})
        client['info']['props'].update({'application.process.id': secret, 'secret': secret})
        diagnostic = ownership_diagnostic([client] * 100, source)
        self.assertEqual(set(diagnostic),
                         {'node_id', 'node_client_id', 'node_process_id', 'matching_clients'})
        self.assertEqual(len(diagnostic['matching_clients']), 4)
        self.assertIsNone(diagnostic['node_process_id'])
        for item in diagnostic['matching_clients']:
            self.assertEqual(item, {'id': 40, 'process_id': None})
        encoded = json.dumps(diagnostic)
        self.assertNotIn(secret, encoded)
        self.assertNotIn('node.name', encoded)
        self.assertLess(len(encoded), 512)


class CleanupTests(unittest.TestCase):
    def test_exited_leader_still_kills_descendant_group(self):
        child = Mock(pid=123)
        with patch('run_pipewire_loopback.os.killpg',
                   side_effect=[None, None, ProcessLookupError]) as kill:
            Harness.stop(child)
        self.assertEqual(kill.call_args_list,
                         [call(123, signal.SIGTERM), call(123, signal.SIGKILL), call(123, 0)])
        self.assertEqual(child.wait.call_count, 2)

    def test_stuck_group_cannot_report_cleanup_success(self):
        with patch('run_pipewire_loopback.os.killpg'), \
             patch('run_pipewire_loopback.time.monotonic', side_effect=[0, 2]):
            with self.assertRaises(RuntimeError):
                Harness.stop(Mock(pid=123))

    def test_external_termination_unwinds_finally(self):
        cleanup = Mock()
        with self.assertRaises(SystemExit):
            try:
                interrupted(signal.SIGTERM, None)
            finally:
                cleanup()
        cleanup.assert_called_once()


class PairedStoreHarnessTests(unittest.TestCase):
    def test_explicit_paired_store_requires_real_launcher_and_private_environment(self):
        root = Path('/synthetic-not-opened')
        with self.assertRaises(ValueError):
            Harness(root, paired_store=True)
        legacy = Harness(root, e2e=True)
        paired = Harness(root, e2e=True, paired_store=True)
        self.assertNotIn('CAPYIO_AUDIO_PAIRED_STORE', legacy.env)
        self.assertEqual(paired.env['CAPYIO_AUDIO_PAIRED_STORE'], '1')
        self.assertNotIn('DBUS_SESSION_BUS_ADDRESS', paired.env)
        self.assertEqual(paired.env['HOME'], str(root))

    def test_paired_evidence_requires_actual_read_and_exact_unskipped_execution(self):
        harness = Harness(Path('/synthetic-not-opened'), e2e=True, paired_store=True)
        evidence = 'capyio_audio_paired_store=passed protected_read=true secret_redacted=true\n'
        passed = 'test result: ok. 1 passed; 0 failed; 0 ignored; 20 filtered out;\n'
        with patch.object(Path, 'read_text', side_effect=[evidence, passed]):
            harness.verify_pairing()
        for logs in ([evidence.replace('true', 'false'), passed],
                     [evidence + evidence, passed],
                     [evidence + 'capyio_audio_paired_store=malformed\n', passed],
                     ['', passed], [evidence, passed.replace('1 passed', '0 passed')],
                     [evidence, passed.replace('0 ignored', '1 ignored')]):
            with self.subTest(logs=logs), patch.object(Path, 'read_text', side_effect=logs):
                with self.assertRaises(ValueError):
                    harness.verify_pairing()
        # A mode/feature mismatch cannot pass by claiming paired evidence in legacy mode.
        legacy = Harness(Path('/synthetic-not-opened'), e2e=True)
        with patch.object(Path, 'read_text', return_value='unrelated fixture log\n'):
            legacy.verify_pairing()
        for unexpected in (evidence, 'capyio_audio_paired_store=malformed\n'):
            with patch.object(Path, 'read_text', return_value=unexpected):
                with self.assertRaises(ValueError):
                    legacy.verify_pairing()


class ConsoleHarnessTests(unittest.TestCase):
    def test_console_profile_requires_explicit_protected_executable_and_scenario(self):
        root = Path('/synthetic-not-opened')
        self.assertEqual(len(CONSOLE_SCENARIOS), 8)
        self.assertEqual(len(CONSOLE_PENDING), 5)
        for kwargs in ({'console': True},
                       {'console': True, 'e2e': True, 'console_scenario': 'waveform'},
                       {'console': True, 'e2e': True, 'paired_store': True},
                       {'console_scenario': 'waveform'},
                       {'console': True, 'e2e': True, 'paired_store': True,
                        'console_scenario': 'waveform', 'virtual_scenario': 'child_exit'}):
            with self.subTest(kwargs=kwargs), self.assertRaises(ValueError):
                Harness(root, **kwargs)
        for scenario in CONSOLE_SCENARIOS:
            for virtual in (None, 'waveform'):
                harness = Harness(root, e2e=True, paired_store=True, console=True,
                                  console_scenario=scenario, virtual_scenario=virtual)
                self.assertEqual(harness.env['CAPYIO_AUDIO_CONSOLE'], '1')
                self.assertEqual(harness.env['CAPYIO_AUDIO_CONSOLE_SCENARIO'], scenario)
                self.assertEqual(harness.env['CAPYIO_AUDIO_PAIRED_STORE'], '1')
                self.assertNotIn('DBUS_SESSION_BUS_ADDRESS', harness.env)
                self.assertEqual(harness.pending_console(), scenario in CONSOLE_PENDING)

    def test_exact_console_marker_requires_exact_test_not_legacy_paired_substitution(self):
        for mode, virtual, name in (
                ('playback', None, 'hosted_console_playback_end_to_end'),
                ('virtual_microphone', 'waveform', 'hosted_console_virtual_microphone_end_to_end')):
            for scenario in CONSOLE_SCENARIOS:
                harness = Harness(Path('/synthetic-not-opened'), e2e=True, paired_store=True,
                                  console=True, console_scenario=scenario, virtual_scenario=virtual)
                evidence = console_marker(mode, scenario) + '\n'
                passed = f'test {name} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 20 filtered out;\n'
                with patch.object(Path, 'read_text', side_effect=[evidence, passed]):
                    harness.verify_console()
                for logs in ([evidence + evidence, passed], ['', passed],
                             [evidence.replace('actual_tty=true', 'actual_tty=false'), passed],
                             [evidence.replace('scenario=' + scenario, 'scenario=unknown'), passed],
                             [evidence, passed.replace(name, 'hosted_pipewire_android_source_end_to_end')],
                             [evidence, passed.replace('1 passed', '0 passed')],
                             [evidence, passed.replace('0 ignored', '1 ignored')]):
                    with self.subTest(mode=mode, scenario=scenario, logs=logs), \
                            patch.object(Path, 'read_text', side_effect=logs), self.assertRaises(ValueError):
                        harness.verify_console()
        legacy = Harness(Path('/synthetic-not-opened'), e2e=True)
        with patch.object(Path, 'read_text', return_value='unrelated\n'):
            legacy.verify_console()
        with patch.object(Path, 'read_text', return_value=console_marker('playback', 'waveform')), \
                self.assertRaises(ValueError):
            legacy.verify_console()

    def test_console_ownership_requires_actual_fixed_siblings_and_direct_parent(self):
        value = {'console_pid': 123, 'owner_pid': 124, 'mode': 'playback', 'scenario': 'waveform'}
        resolves = [Path('/owned/capyio-linux-console'), Path('/owned/capyio-node')]
        args = [b'console\0--pairing-state\0/private/state\0playback\0',
                b'node\0linux-playback\0--pairing-state\0/private/state\0']
        with patch.object(Path, 'resolve', side_effect=resolves), \
                patch.object(Path, 'read_text', return_value='PPid:\t123\n'), \
                patch.object(Path, 'read_bytes', side_effect=args):
            self.assertEqual(verify_console_owner(value, 'playback', 'waveform'), value)
        for changes in ({'console_pid': True}, {'owner_pid': 123}, {'owner_pid': 1},
                        {'mode': 'virtual_microphone'}, {'scenario': 'expiry'}, {'extra': 1}):
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                verify_console_owner(value | changes, 'playback', 'waveform')
        for binaries in ([Path('/owned/fake-console'), resolves[1]],
                         [resolves[0], Path('/other/capyio-node')]):
            with patch.object(Path, 'resolve', side_effect=binaries), self.assertRaises(ValueError):
                verify_console_owner(value, 'playback', 'waveform')
        with patch.object(Path, 'resolve', side_effect=resolves), \
                patch.object(Path, 'read_text', return_value='PPid:\t999\n'), self.assertRaises(ValueError):
            verify_console_owner(value, 'playback', 'waveform')
        with patch.object(Path, 'resolve', side_effect=resolves), \
                patch.object(Path, 'read_text', return_value='PPid:\t123\n'), \
                patch.object(Path, 'read_bytes', side_effect=[args[0], b'node\0linux-playback\0']), \
                self.assertRaises(ValueError):
            verify_console_owner(value, 'playback', 'waveform')

    def test_console_cleanup_signals_only_recorded_owned_session_before_fixture(self):
        harness = Harness(Path('/synthetic-not-opened'), e2e=True, paired_store=True,
                          console=True, console_scenario='waveform')
        child = Mock()
        harness.children = [child]
        with patch.object(Path, 'is_file', return_value=True), \
                patch.object(Path, 'read_text', return_value='{"console_pid":123}'), \
                patch.object(Path, 'exists', side_effect=[True, False, False]), \
                patch.object(Path, 'resolve', return_value=Path('/owned/capyio-linux-console')), \
                patch('run_pipewire_loopback.os.getpgid', return_value=123), \
                patch('run_pipewire_loopback.os.killpg') as kill, \
                patch.object(harness, 'stop') as stop:
            harness.close()
        kill.assert_called_once_with(123, signal.SIGTERM)
        stop.assert_called_once_with(child)

    def test_console_cleanup_refuses_changed_binary_or_process_group(self):
        for binary, group in ((Path('/owned/unrelated'), 123),
                              (Path('/owned/capyio-linux-console'), 999)):
            harness = Harness(Path('/synthetic-not-opened'), e2e=True, paired_store=True,
                              console=True, console_scenario='waveform')
            with patch.object(Path, 'is_file', return_value=True), \
                    patch.object(Path, 'read_text', return_value='{"console_pid":123}'), \
                    patch.object(Path, 'exists', return_value=True), \
                    patch.object(Path, 'resolve', return_value=binary), \
                    patch('run_pipewire_loopback.os.getpgid', return_value=group), \
                    patch('run_pipewire_loopback.os.killpg') as kill, self.assertRaises(RuntimeError):
                harness.close()
            kill.assert_not_called()

    def test_playback_helper_ownership_cannot_use_name_only_or_wrong_target(self):
        player = dict(id=50, type='PipeWire:Interface:Node', info={'props': {
            'node.name': 'synthetic.player', 'target.object': 'capyio.test.null', 'client.id': 40}})
        client = dict(id=40, type='PipeWire:Interface:Client', info={'props': {
            'application.process.id': 125, 'pipewire.sec.pid': 125}})
        port = dict(id=51, type='PipeWire:Interface:Port', info={'props': {'node.id': 50}})
        graph = [player, client, port]
        with patch.object(Path, 'read_text', return_value='PPid:\t124\n'), \
                patch.object(Path, 'read_bytes', return_value=b'/usr/bin/pw-cat\0--playback\0--target=capyio.test.null\0-'), \
                patch.object(Path, 'resolve', return_value=Path('/usr/bin/pw-cat')):
            self.assertEqual(verify_playback_process_owner(graph, 124), (125, {50, 51}))
            with self.assertRaises(ValueError):
                verify_playback_process_owner(graph, 999)
            with patch.object(Path, 'read_bytes', return_value=b'pw-cat\0--playback\0--target=0\0-'), \
                    self.assertRaises(ValueError):
                verify_playback_process_owner(graph, 124)
            for bad in ([], [player, port], graph + [player]):
                with self.assertRaises(ValueError):
                    verify_playback_process_owner(bad, 124)


if __name__ == '__main__':
    unittest.main()
