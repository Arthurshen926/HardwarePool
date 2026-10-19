import { test } from 'node:test';
import assert from 'node:assert/strict';
import { parseDiscoveryCandidates, selectAutomaticPairedCandidate } from '../apps/desktop/src/lib/discovery-candidates.ts';

const candidate = { nodeId: '00000000-0000-4000-8000-000000000001', host: '192.168.1.20', controlPort: 42420, lastSeenAgeMs: 100, pairingState: 'unpaired' };

test('bounded candidates expose pairing metadata but not authenticated trust or grants', () => {
  assert.deepEqual(parseDiscoveryCandidates([]), []);
  assert.deepEqual(parseDiscoveryCandidates([candidate]), [candidate]);
  assert.equal(parseDiscoveryCandidates([{ ...candidate, host: '2001:db8::1', lastSeenAgeMs: 15000 }])[0].host, '2001:db8::1');
  assert.equal(parseDiscoveryCandidates([{ ...candidate, pairingState: 'paired' }])[0].pairingState, 'paired');
  assert.equal(parseDiscoveryCandidates([{ ...candidate, pairingState: 'revoked' }])[0].pairingState, 'revoked');
  assert.throws(() => parseDiscoveryCandidates([{ ...candidate, trusted: true }]));
});

test('malformed, duplicated and stale records fail closed', () => {
  for (const value of [null, {}, [null], [candidate, candidate], Array(65).fill(candidate)]) assert.throws(() => parseDiscoveryCandidates(value));
  for (const patch of [{ nodeId: 'not-a-node' }, { nodeId: '00000000-0000-0000-0000-000000000000' },
    { controlPort: 0 }, { controlPort: 65536 }, { controlPort: 1.5 }, { lastSeenAgeMs: -1 }, { lastSeenAgeMs: 15001 }, { pairingState: 'connected' },
    { lastSeenAgeMs: NaN }, { host: '<img src=x>' }, { host: '192.168.001.2' }, { host: '127.1' },
    { host: '192.168.1.2:1234' }, { host: '999.1.1.1' }, { host: 'face' }, { host: ':::' }, { host: '' }]) {
    assert.throws(() => parseDiscoveryCandidates([{ ...candidate, ...patch }]));
  }
});

test('automatic connection selects only a paired unsuppressed disconnected candidate after cooldown', () => {
  const unpaired = candidate;
  const paired = { ...candidate, nodeId: '00000000-0000-4000-8000-000000000002', pairingState: 'paired' };
  const secondPaired = { ...candidate, nodeId: '00000000-0000-4000-8000-000000000003', pairingState: 'paired' };
  assert.equal(selectAutomaticPairedCandidate([unpaired, paired], new Set(), new Set(), new Map(), 20_000), paired);
  assert.equal(selectAutomaticPairedCandidate([paired], new Set([paired.nodeId]), new Set(), new Map(), 20_000), null);
  assert.equal(selectAutomaticPairedCandidate([paired], new Set(), new Set([paired.nodeId]), new Map(), 20_000), null);
  assert.equal(selectAutomaticPairedCandidate([paired], new Set(), new Set(), new Map([[paired.nodeId, 15_000]]), 20_000), null);
  assert.equal(selectAutomaticPairedCandidate([paired, secondPaired], new Set(), new Set([paired.nodeId]), new Map(), 20_000), secondPaired);
});
