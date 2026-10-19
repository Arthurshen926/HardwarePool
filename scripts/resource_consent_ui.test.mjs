import { test } from 'node:test';
import assert from 'node:assert/strict';
import { parseResourceRequests } from '../apps/desktop/src/lib/resource-consent.ts';
const request = { routeId: '00000000-0000-4000-8000-000000000001', peerNodeId: '00000000-0000-4000-8000-000000000002', peerName: 'PC', capabilityName: 'Touchpad', expiresAtMs: 60000 };
test('audio consent retains separate microphone and speaker scope', () => {
  for (const resourceKind of ['microphone', 'speaker']) {
    const audio = { ...request, capabilityId: '00000000-0000-4000-8000-000000000005', capabilityName: resourceKind, resourceKind };
    assert.deepEqual(parseResourceRequests([audio]), [audio]);
  }
});
test('gamepad consent retains its exact resource scope', () => {
  const gamepad = { ...request, capabilityId: '00000000-0000-4000-8000-000000000004', capabilityName: 'Controller', resourceKind: 'gamepad' };
  assert.deepEqual(parseResourceRequests([gamepad]), [gamepad]);
});
test('camera consent remains separate from microphone scope', () => {
  const camera = { ...request, capabilityId: '00000000-0000-4000-8000-000000000006', capabilityName: 'Camera', resourceKind: 'camera' };
  assert.deepEqual(parseResourceRequests([camera]), [camera]);
});
test('actuator consent carries a separate resource identity', () => {
  const haptics = { ...request, capabilityId: '00000000-0000-4000-8000-000000000003', capabilityName: 'Phone vibration', resourceKind: 'haptics' };
  assert.deepEqual(parseResourceRequests([haptics]), [haptics]);
  for (const invalid of [{ ...haptics, resourceKind: 'all' }, { ...haptics, capabilityId: 'invalid' },
    { ...request, resourceKind: 'haptics' }]) assert.throws(() => parseResourceRequests([invalid]));
});
test('consent is a bounded typed request, not arbitrary command authority', () => {
  assert.deepEqual(parseResourceRequests([request]), [request]);
  assert.deepEqual(parseResourceRequests([]), []);
  for (const value of [null, [request, request], Array(17).fill(request), [{ ...request, allow: true }],
    [{ ...request, routeId: 'invalid' }], [{ ...request, peerNodeId: '00000000-0000-0000-0000-000000000000' }],
    [{ ...request, peerName: 'x'.repeat(257) }], [{ ...request, expiresAtMs: -1 }], [{ ...request, expiresAtMs: Infinity }]]) {
    assert.throws(() => parseResourceRequests(value));
  }
});
