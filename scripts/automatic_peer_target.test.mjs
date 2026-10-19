import { test } from 'node:test';
import assert from 'node:assert/strict';
import { selectAutomaticPeerTarget as select } from '../apps/desktop/src/lib/automatic-peer-target.ts';

const peers = ['a', 'b'].map(nodeId => ({ nodeId, revoked: false }));
const candidate = { nodeId: 'a', pairingState: 'paired' };
const hints = [{ nodeId: 'a', host: '100.1.1.1', port: 43193 }, { nodeId: 'b', host: '100.1.1.2', port: 43193 }];
const empty = new Set();

test('slow discovered node cannot starve a cached-only node', () => {
  assert.equal(select([candidate], hints, peers, empty, empty, new Map(), 0).nodeId, 'a');
  const attempts = new Map([['a', 0]]);
  assert.equal(select([candidate], hints, peers, empty, empty, attempts, 30_000).nodeId, 'b');
  attempts.set('b', 30_000);
  assert.equal(select([candidate], hints, peers, empty, empty, attempts, 60_000).nodeId, 'a');
});

test('same node retains its cached fallback without duplicating a dial target', () => {
  const target = select([candidate], hints, peers, new Set(['b']), empty, new Map(), 0);
  assert.equal(target.candidate, candidate);
  assert.equal(target.cached, hints[0]);
});

test('trust, manual disconnect, connected state and cooldown remain authoritative', () => {
  assert.equal(select([candidate], hints, [], empty, empty, new Map(), 30_000), null);
  assert.equal(select([candidate], hints, peers, new Set(['a']), new Set(['b']), new Map(), 30_000), null);
  assert.equal(select([candidate], hints, peers, empty, empty, new Map([['a', 25_000], ['b', 25_000]]), 30_000), null);
  assert.equal(select([candidate], hints, peers.map(p => ({ ...p, revoked: true })), empty, empty, new Map(), 30_000), null);
});
