import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { fileURLToPath, pathToFileURL } from 'node:url';

const require = createRequire(new URL('../apps/desktop/package.json', import.meta.url));
const { createServer } = await import(pathToFileURL(require.resolve('vite')).href);
const { createRenderer, ssrContextKey } = require('vue');
const deferred = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
};
// Render nothing: exercise the real component setup/lifecycle without DOM or devices.
const renderer = createRenderer({
  createComment: () => ({}), insert() {}, remove() {}, parentNode: () => null,
  nextSibling: () => null,
});

test('pairing commands supersede in-flight polls and unmount suppresses completion', async () => {
  const server = await createServer({ root: fileURLToPath(new URL('../apps/desktop', import.meta.url)), server: { middlewareMode: true, hmr: false }, appType: 'custom' });
  const apps = [];
  try {
    const { default: Panel } = await server.ssrLoadModule('/src/components/PeerEnrollmentPanel.vue');
    for (const staleFails of [false, true]) {
      const poll = deferred();
      const progress = [];
      const app = renderer.createApp({ ...Panel, render: () => null }, {
        api: { getEnrollmentStatus: () => poll.promise, getEnrollmentAddresses: async () => [],
          enrollmentAction: async () => ({ phase: 'cancelled', message: '' }) },
        onProgress: value => progress.push(value.phase),
      });
      app.provide(ssrContextKey, {});
      apps.push(app);
      const vm = app.mount({});
      await vm.$.setupState.action({ action: 'cancel' });
      if (staleFails) poll.reject(new Error('stale poll failure'));
      else poll.resolve({ phase: 'waiting', pin: '12345678', message: '' });
      await Promise.resolve();
      assert.equal(vm.$.setupState.state.phase, 'cancelled');
      assert.equal(vm.$.setupState.error, '');
      assert.deepEqual(progress, ['cancelled']);
      app.unmount();
      apps.pop();
    }
    const command = deferred();
    const events = [];
    const app = renderer.createApp({ ...Panel, render: () => null }, {
      api: { getEnrollmentStatus: async () => ({ phase: 'idle', message: '' }),
        getEnrollmentAddresses: async () => [], enrollmentAction: () => command.promise },
      onProgress: value => events.push(value.phase), onPaired: () => events.push('paired'),
    });
    app.provide(ssrContextKey, {});
    apps.push(app);
    const vm = app.mount({});
    await Promise.resolve();
    events.length = 0;
    const pending = vm.$.setupState.action({ action: 'approve' });
    app.unmount();
    apps.pop();
    command.resolve({ phase: 'complete', message: '' });
    await pending;
    assert.deepEqual(events, []);
  } finally {
    for (const app of apps) app.unmount();
    await server.close();
  }
});


test('media commands ignore old polls and do not refresh after leaving', async () => {
  const server = await createServer({ root: fileURLToPath(new URL('../apps/desktop', import.meta.url)), server: { middlewareMode: true, hmr: false }, appType: 'custom' });
  const previousDocument = globalThis.document;
  globalThis.document = { hidden: false, addEventListener() {}, removeEventListener() {} };
  let mounted;
  try {
    for (const kind of ['Audio', 'Camera', 'Display']) {
      const { default: Panel } = await server.ssrLoadModule(`/src/components/${kind}UsePanel.vue`);
      const stale = deferred(), command = deferred();
      let reads = 0, stops = 0;
      const active = { status: 'active', peerNodeId: 'peer', direction: 'microphone', width: 1920, height: 1080, refreshMillihertz: 60000, bitrateKbps: 15000 };
      const api = {
        [`get${kind}Peers`]: async () => [{ nodeId: 'peer', displayName: 'Computer', directions: ['microphone'] }],
        [kind === 'Audio' ? 'getAudioUses' : `get${kind}Use`]: () => {
          reads++;
          return reads === 1 ? Promise.resolve(kind === 'Audio' ? [active] : active) : stale.promise;
        },
        [`stop${kind}Use`]: () => { stops++; return command.promise; },
      };
      const app = renderer.createApp({ ...Panel, render: () => null }, { api });
      app.provide(ssrContextKey, {});
      mounted = app;
      const vm = app.mount({});
      // Allow the mount's Promise.all inventory read to settle.
      await new Promise(resolve => setImmediate(resolve));
      const state = vm.$.setupState;
      const poll = state.refresh();
      const change = kind === 'Display' ? state.stop() : state.change('microphone');
      await state.refresh();
      assert.equal(reads, 2, 'ordinary polling pauses during a command');
      stale.resolve(kind === 'Audio' ? [] : null);
      await poll;
      assert.equal(kind === 'Audio' ? state.uses[0].status : (kind === 'Display' ? state.state.status : state.use.status), 'active');
      app.unmount();
      mounted = undefined;
      command.resolve();
      await change;
      assert.equal(reads, 2, 'command completion after unmount makes no new query');
      assert.equal(stops, 1, 'unmount does not stop resources again');
    }
  } finally {
    mounted?.unmount();
    if (previousDocument === undefined) delete globalThis.document;
    else globalThis.document = previousDocument;
    await server.close();
  }
});


test('media inventory failures remain distinct from an empty successful read and recover', async () => {
  const server = await createServer({ root: fileURLToPath(new URL('../apps/desktop', import.meta.url)), server: { middlewareMode: true, hmr: false }, appType: 'custom' });
  const previousDocument = globalThis.document;
  globalThis.document = { hidden: false, addEventListener() {}, removeEventListener() {} };
  let mounted;
  try {
    for (const kind of ['Audio', 'Camera', 'Display']) {
      const { default: Panel } = await server.ssrLoadModule(`/src/components/${kind}UsePanel.vue`);
      let failed = true, starts = 0;
      const app = renderer.createApp({ ...Panel, render: () => null }, { api: {
        [`get${kind}Peers`]: async () => { if (failed) throw new Error('inventory unavailable'); return []; },
        [kind === 'Audio' ? 'getAudioUses' : `get${kind}Use`]: async () => kind === 'Audio' ? [] : null,
        startDisplayUse: async () => { starts++; },
      } });
      app.provide(ssrContextKey, {});
      mounted = app;
      const state = app.mount({}).$.setupState;
      assert.equal(state.loaded, false);
      await new Promise(resolve => setImmediate(resolve));
      assert.equal(state.loaded, false);
      assert.match(kind === 'Display' ? state.pollError : state.readError, /inventory unavailable/);
      if (kind === 'Display') { await state.start(); assert.equal(starts, 0); }
      failed = false;
      await state.refresh();
      assert.equal(state.loaded, true);
      assert.equal(kind === 'Display' ? state.pollError : state.readError, '');
      assert.deepEqual(state.peers, []);
      app.unmount(); mounted = undefined;
    }
  } finally {
    mounted?.unmount();
    if (previousDocument === undefined) delete globalThis.document;
    else globalThis.document = previousDocument;
    await server.close();
  }
});


test('display compatibility preserves independent state, viewing edits and stop access after read failure', async () => {
  const server = await createServer({ root: fileURLToPath(new URL('../apps/desktop', import.meta.url)), server: { middlewareMode: true, hmr: false }, appType: 'custom' });
  const previousWindow = globalThis.window;
  globalThis.window = { addEventListener() {}, removeEventListener() {} };
  let app;
  try {
    const { default: Panel } = await server.ssrLoadModule('/src/components/DisplayReceiverPanel.vue');
    let receiverFails = true, senderFails = false, starts = 0, stops = 0;
    app = renderer.createApp({ ...Panel, render: () => null }, { api: {
      getDisplayReceiverState: async () => {
        if (receiverFails) throw new Error('receiver unavailable');
        return { viewingOptions: { fullscreen: true, keepAwake: true, showStatistics: false } };
      },
      getDisplaySenderState: async () => {
        if (senderFails) throw new Error('sender unavailable');
        return { state: 'running', available: true };
      },
      startDisplaySender: async () => { starts++; },
      stopDisplaySender: async () => { stops++; return { state: 'stopped', available: true }; },
    } });
    app.provide(ssrContextKey, {});
    const state = app.mount({}).$.setupState;
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(state.sender.state, 'running');
    assert.match(state.receiverReadError, /receiver unavailable/);
    receiverFails = false;
    await state.refresh();
    state.viewing.showStatistics = true;
    state.viewingDirty = true;
    senderFails = true;
    await state.refresh();
    assert.equal(state.viewing.showStatistics, true, 'polling must not overwrite draft viewing options');
    assert.equal(state.sender.state, 'running', 'failed sender query must retain the stop target');
    await state.controlSender(true);
    assert.equal(starts, 0, 'stale state cannot start a sender');
    await state.controlSender(false);
    assert.equal(stops, 1, 'failed reads do not block stop');
    assert.equal(state.sender.state, 'stopped');
  } finally {
    app?.unmount();
    if (previousWindow === undefined) delete globalThis.window;
    else globalThis.window = previousWindow;
    await server.close();
  }
});
