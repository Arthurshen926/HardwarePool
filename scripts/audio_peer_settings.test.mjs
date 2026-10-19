import { test } from 'node:test';
import assert from 'node:assert/strict';
import { parseAudioPeerSettings } from '../apps/desktop/src/lib/audio-peer-settings.ts';

test('saved peer contract accepts exact Java golden and distinguishes absent settings', () => {
  const golden = '{"schemaVersion":1,"direction":"microphone","savedPeer":{"host":"10.0.0.1","localPort":1,"peerPort":65535}}';
  assert.deepEqual(parseAudioPeerSettings(golden, 'microphone').savedPeer, { host: '10.0.0.1', localPort: 1, peerPort: 65535 });
  assert.equal(parseAudioPeerSettings('{"schemaVersion":1,"direction":"speaker","savedPeer":null}', 'speaker').savedPeer, null);
  assert.throws(() => parseAudioPeerSettings(golden, 'speaker'));
});

test('malformed, overbroad and unsafe readback never becomes a saved connection', () => {
  const valid = { schemaVersion: 1, direction: 'speaker', savedPeer: { host: '100.64.0.1', localPort: 40000, peerPort: 40001 } };
  for (const value of ['', 'x'.repeat(513), 'null', '[]', '{}', '{bad']) assert.throws(() => parseAudioPeerSettings(value, 'speaker'));
  for (const patch of [{ schemaVersion: 2 }, { direction: 'camera' }, { secret: 'forbidden' }, { savedPeer: {} }]) assert.throws(() => parseAudioPeerSettings(JSON.stringify({ ...valid, ...patch }), 'speaker'));
  for (const patch of [{ host: '8.8.8.8' }, { host: '100.128.0.1' }, { host: '192.168.001.1' }, { host: '10.0.0.1\n' }, { host: null }, { localPort: 0 }, { peerPort: 65536 }, { peerPort: 1.5 }, { peerPort: '123' }, { token: 'forbidden' }]) {
    assert.throws(() => parseAudioPeerSettings(JSON.stringify({ ...valid, savedPeer: { ...valid.savedPeer, ...patch } }), 'speaker'));
  }
});
