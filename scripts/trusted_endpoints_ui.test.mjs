import { test } from 'node:test';
import assert from 'node:assert/strict';
import { encodeTrustedEndpointHints, parseTrustedEndpointHints, rememberAuthenticatedEndpoint,
  retainActivelyPairedEndpoints, selectAutomaticTrustedEndpoint } from '../apps/desktop/src/lib/trusted-endpoints.ts';

const nodeId = '00000000-0000-4000-8000-000000000001';
const endpoint = { nodeId, host: '100.110.59.35', port: 45987 };
const peer = { nodeId, displayName: 'Tablet', revoked: false };

test('only bounded numeric endpoint hints are persisted, without trust or credential material', () => {
  assert.deepEqual(parseTrustedEndpointHints(null), []);
  assert.deepEqual(parseTrustedEndpointHints(encodeTrustedEndpointHints([endpoint])), [endpoint]);
  assert.deepEqual(parseTrustedEndpointHints(encodeTrustedEndpointHints([{ ...endpoint, host: '2001:db8::1' }])), [{ ...endpoint, host: '2001:db8::1' }]);
  assert.deepEqual(rememberAuthenticatedEndpoint([endpoint], { ...endpoint, port: 43193 }), [{ ...endpoint, port: 43193 }]);
  for (const bad of [{ ...endpoint, host: 'tablet.local' }, { ...endpoint, port: 0 },
    { ...endpoint, nodeId: 'bad' }, { ...endpoint, token: 'secret' }]) assert.throws(() => encodeTrustedEndpointHints([bad]));
  assert.throws(() => parseTrustedEndpointHints(JSON.stringify({ version: 1, endpoints: [endpoint, endpoint] })));
  assert.throws(() => parseTrustedEndpointHints(JSON.stringify({ version: 1, endpoints: [{ ...endpoint, credential: 'x' }] })));
  assert.throws(() => parseTrustedEndpointHints('x'.repeat(8193)));
  assert.throws(() => parseTrustedEndpointHints(JSON.stringify({ version: 2, endpoints: [endpoint] })));
});

test('cached addresses never grant an unpaired or revoked node and respect disconnect/cooldown', () => {
  const hints = [endpoint];
  assert.deepEqual(retainActivelyPairedEndpoints(hints, [peer]), hints);
  assert.deepEqual(retainActivelyPairedEndpoints(hints, [{ ...peer, revoked: true }]), []);
  assert.equal(selectAutomaticTrustedEndpoint(hints, [], new Set(), new Set(), new Map(), 20_000), null);
  assert.equal(selectAutomaticTrustedEndpoint(hints, [{ ...peer, revoked: true }], new Set(), new Set(), new Map(), 20_000), null);
  assert.equal(selectAutomaticTrustedEndpoint(hints, [peer], new Set([nodeId]), new Set(), new Map(), 20_000), null);
  assert.equal(selectAutomaticTrustedEndpoint(hints, [peer], new Set(), new Set([nodeId]), new Map(), 20_000), null);
  assert.equal(selectAutomaticTrustedEndpoint(hints, [peer], new Set(), new Set(), new Map([[nodeId, 15_000]]), 20_000), null);
  assert.deepEqual(selectAutomaticTrustedEndpoint(hints, [peer], new Set(), new Set(), new Map(), 20_000), endpoint);
});
