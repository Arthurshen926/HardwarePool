import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { stripTypeScriptTypes } from 'node:module';

const app = readFileSync(new URL('../apps/desktop/src/App.vue', import.meta.url), 'utf8');
const source = app.slice(app.indexOf('let enrollmentNeedsHandoff ='), app.indexOf('async function startPairedListener()'));
const complete = { phase: 'complete', peerNodeId: 'peer', peerControlHost: '100.1.1.1', peerControlPort: 43193 };
function harness() {
  const ref = value => ({ value });
  let dials = 0, refreshes = 0;
  const scope = {
    document: { hidden: false }, touchpadImmersive: ref(false), gamepadImmersive: ref(false),
    manualControlBusy: ref(false), peerConnectionBusy: ref(false), pairedListenerBusy: ref(false),
    pairedListenerActive: ref(false), autoConnectPaired: ref(true), connectedPeerIds: ref(new Set()),
    manuallyDisconnectedPeerIds: new Set(), manualControlNodeId: ref(''), manualControlHost: ref(''),
    manualControlPort: ref(0), manualControlMessage: ref(''),
    api: { getEnrollmentStatus: async () => complete },
    refreshPairedPeers: async () => { refreshes++; },
    connectManualControlEndpoint: async () => { dials++; },
  };
  const methods = new Function(...Object.keys(scope), stripTypeScriptTypes(source) +
    '\nreturn { completeFilelessPairing, refreshEnrollmentHandoff, enrollmentProgress };')(...Object.values(scope));
  return { ...methods, scope, counts: () => ({ dials, refreshes }) };
}
test('completion survives leaving settings and is handled once', async () => {
  const h = harness();
  h.enrollmentProgress({ phase: 'waiting_peer' });
  await h.refreshEnrollmentHandoff();
  await h.completeFilelessPairing(complete);
  assert.deepEqual(h.counts(), { dials: 1, refreshes: 1 });
  assert.equal(h.scope.manualControlHost.value, complete.peerControlHost);
});
test('busy connection defers completion without losing it', async () => {
  const h = harness(); h.scope.manualControlBusy.value = true;
  await h.refreshEnrollmentHandoff();
  assert.equal(h.counts().dials, 0);
  h.scope.manualControlBusy.value = false;
  await h.refreshEnrollmentHandoff();
  assert.equal(h.counts().dials, 1);
});
test('listener, manual disconnect and disabled auto-connect prohibit implicit dialing', async () => {
  for (const configure of [s => s.pairedListenerActive.value = true,
    s => s.manuallyDisconnectedPeerIds.add('peer'), s => s.autoConnectPaired.value = false]) {
    const h = harness(); configure(h.scope);
    await h.refreshEnrollmentHandoff();
    assert.deepEqual(h.counts(), { dials: 0, refreshes: 1 });
  }
});
test('local recovery refreshes trust but never implies a remote connection', async () => {
  const h = harness(); await h.completeFilelessPairing({ ...complete, phase: 'recovered' });
  assert.deepEqual(h.counts(), { dials: 0, refreshes: 1 });
});
