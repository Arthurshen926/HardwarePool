import { test } from 'node:test';
import assert from 'node:assert/strict';
import { clearConnectionProfile, isPrivateOrOverlayIpv4, parseConnectionProfile, readConnectionProfile, saveConnectionProfile } from '../apps/desktop/src/lib/connection-profile.ts';

function memoryStorage() {
  const values = new Map();
  return {
    getItem(key) { return values.has(key) ? values.get(key) : null; },
    setItem(key, value) { values.set(key, value); },
    removeItem(key) { values.delete(key); },
  };
}

test('one strict local profile persists only a preferred private or overlay IPv4', () => {
  const storage = memoryStorage();
  assert.equal(readConnectionProfile(storage), null);
  assert.deepEqual(saveConnectionProfile(storage, ' 100.64.0.9 '), { schemaVersion: 1, preferredPeerIpv4: '100.64.0.9' });
  assert.deepEqual(readConnectionProfile(storage), { schemaVersion: 1, preferredPeerIpv4: '100.64.0.9' });
  clearConnectionProfile(storage);
  assert.equal(readConnectionProfile(storage), null);
});

test('connection profile rejects public, noncanonical and expanded data', () => {
  for (const host of ['8.8.8.8', '100.128.0.1', '192.168.001.1', '10.0.0.1x', '127.0.0.1', '', 'host.local']) {
    assert.equal(isPrivateOrOverlayIpv4(host), false);
    assert.throws(() => saveConnectionProfile(memoryStorage(), host));
  }
  for (const host of ['10.0.0.1', '172.16.0.1', '172.31.255.255', '192.168.1.2', '100.127.255.254']) {
    assert.equal(isPrivateOrOverlayIpv4(host), true);
  }
  for (const encoded of ['', 'x'.repeat(129), 'null', '[]', '{}', '{bad',
    '{"schemaVersion":2,"preferredPeerIpv4":"10.0.0.1"}',
    '{"schemaVersion":1,"preferredPeerIpv4":"10.0.0.1","token":"forbidden"}']) {
    assert.throws(() => parseConnectionProfile(encoded));
  }
});
