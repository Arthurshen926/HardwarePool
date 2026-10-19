import { test } from 'node:test';
import assert from 'node:assert/strict';
import { initialResourceView, resourceActions, peerResourceGroups, primaryHardwareResources, localResourceBindings, actionPresentation, createResourceNavigation, moveResourceNavigation, backResourceNavigation } from '../apps/desktop/src/lib/resource-navigation.ts';

test('gamepad presentation groups its motion and vibrator without deleting independent nodes', () => {
  const gamepad = { nodeId: 'phone', capabilityClass: 'gamepad', simulated: false, ports: [] };
  const imu = { nodeId: 'phone', capabilityClass: 'imu', ports: [{ direction: 'source', profile: 'capyio.motion.imu-samples/1' }] };
  const haptics = { nodeId: 'phone', capabilityClass: 'haptics', ports: [{ direction: 'sink', profile: 'capyio.haptics.feedback/1' }] };
  const separate = { ...imu, nodeId: 'sensor-node' };
  const input = [gamepad, imu, haptics, separate];
  assert.deepEqual(primaryHardwareResources(input), [gamepad, separate]);
  assert.equal(input.length, 4);
  assert.deepEqual(primaryHardwareResources([imu, haptics]), [imu, haptics]);
  const actions = ['gamepad', 'imu', 'haptics'].map(key => ({ id: `capyio.quick-action.android-${key}`, kind: 'capability_control', simulated: false }));
  assert.deepEqual(resourceActions(actions, 'local').map(a => a.id), ['capyio.quick-action.android-gamepad']);
});

test('camera diagnostic entry is explicitly distinct from system projection', () => {
  const diagnostic = actionPresentation({ id: 'capyio.quick-action.android-camera-receiver' });
  assert.match(diagnostic.title, /诊断/);
  assert.match(diagnostic.summary, /不要同时启动/);
  const action = { id: 'capyio.quick-action.android-camera-receiver', kind: 'capability_control', simulated: false, status: 'idle' };
  assert.deepEqual(resourceActions([action], 'remote'), []);
  assert.equal(resourceActions([{ ...action, status: 'active' }], 'remote').length, 1);
});

test('home preferences and legacy Android entry never restore a hardware operation', () => {
  assert.equal(initialResourceView(null, null), 'remote');
  assert.equal(initialResourceView('quick', null), 'local');
  assert.equal(initialResourceView('quick', 'settings'), 'settings');
  assert.equal(initialResourceView('local', 'settings'), 'local');
  assert.equal(initialResourceView(null, 'quick'), 'remote');
  assert.equal(initialResourceView('unknown', 'controller'), 'remote');
  for (const view of ['controller', 'touchpad', 'workspace', 'display', 'imu']) assert.equal(initialResourceView(view, null), view);
});

test('real resource entry lists isolate fixtures and local lifecycle from remote route controls', () => {
  const actions = [
    { id: 'local', kind: 'capability_control', simulated: false },
    { id: 'remote', kind: 'route_control', simulated: false },
    { id: 'mock', kind: 'route_control', simulated: true },
  ];
  assert.deepEqual(resourceActions(actions, 'local').map(a => a.id), ['local']);
  assert.deepEqual(resourceActions(actions, 'remote').map(a => a.id), ['remote']);
  assert.deepEqual(resourceActions(actions, 'settings'), []);
});

test('peer groups use stable node IDs, retain offline resources, and never mix mock or local hardware', () => {
  const nodes = [{ id: 'self', local: true }, { id: 'a', displayName: 'Same', local: false, online: true }, { id: 'b', displayName: 'Same', local: false, online: false }];
  const capabilities = [
    { id: 'camera-a', nodeId: 'a', local: false, simulated: false },
    { id: 'camera-b', nodeId: 'b', local: false, simulated: false, availability: 'offline' },
    { id: 'mock', nodeId: 'a', local: false, simulated: true },
    { id: 'local', nodeId: 'a', local: true, simulated: false },
    { id: 'orphan', nodeId: 'unknown', local: false, simulated: false },
  ];
  const groups = peerResourceGroups(nodes, capabilities);
  assert.deepEqual(groups.map(g => [g.node.id, g.capabilities.map(c => c.id)]), [['a', ['camera-a']], ['b', ['camera-b']]]);
  assert.equal(groups[1].node.online, false);
  assert.equal(groups[1].capabilities[0].availability, 'offline');
  assert.deepEqual(peerResourceGroups(nodes, []), []);
});

test('seven Android resources bind only exact local catalog keys and independent capability operations', () => {
  for (const key of ['microphone', 'speaker', 'camera', 'imu', 'gamepad', 'touchpad', 'haptics']) {
    const capability = { id: `self:android.${key}`, nodeId: 'self', local: true, simulated: false, inventorySource: 'android_node_catalog', capabilityClass: key };
    const action = { id: `capyio.quick-action.android-${key}`, kind: 'capability_control', simulated: false, title: 'Original', summary: 'Original summary' };
    assert.equal(localResourceBindings('self', [capability], [action])[capability.id], action);
    assert.notEqual(actionPresentation(action).title, 'Original');
    for (const patch of [{ local: false }, { simulated: true }, { nodeId: 'peer' }, { id: `peer:android.${key}` }, { inventorySource: 'runtime_catalog' }, { capabilityClass: 'unknown' }]) {
      assert.deepEqual(localResourceBindings('self', [{ ...capability, ...patch }], [action]), {});
    }
    for (const patch of [{ simulated: true }, { kind: 'route_control' }, { id: 'unknown' }]) {
      assert.deepEqual(localResourceBindings('self', [capability], [{ ...action, ...patch }]), {});
    }
    assert.deepEqual(localResourceBindings(null, [capability], [action]), {});
    assert.deepEqual(localResourceBindings('self', [capability, capability], [action]), {});
    assert.deepEqual(localResourceBindings('self', [capability], [action, action]), {});
    assert.deepEqual(localResourceBindings('self', [capability], []), {});
  }
  assert.deepEqual(actionPresentation({ id: 'future-resource', title: 'Future', summary: 'Unknown resource' }), { title: 'Future', summary: 'Unknown resource' });
});

test('resource panels and connection settings return to the exact resource without restarting it', () => {
  for (const [actionId, panel] of [['microphone', 'audio-settings'], ['speaker', 'audio-settings'], ['gamepad', 'controller'], ['imu', 'controller'], ['touchpad', 'touchpad']]) {
    let state = createResourceNavigation('local');
    state = moveResourceNavigation(state, 'quick', actionId);
    state = moveResourceNavigation(state, panel);
    assert.equal(state.home, 'local');
    assert.deepEqual(backResourceNavigation(state).current, { view: 'quick', actionId });
    assert.deepEqual(backResourceNavigation(backResourceNavigation(state)).current, { view: 'local', actionId: null });
    const settings = moveResourceNavigation(state, 'settings');
    assert.deepEqual(settings.history, []);
    assert.equal(settings.current.actionId, null);
    assert.equal(settings.home, 'settings');
  }
});

test('catalog capability details retain exact resource identity and parent page', () => {
  let remote = createResourceNavigation('remote');
  remote = moveResourceNavigation(remote, 'capability', 'peer-a:camera');
  assert.deepEqual(remote.current, { view: 'capability', actionId: 'peer-a:camera' });
  assert.equal(remote.home, 'remote');
  assert.deepEqual(backResourceNavigation(remote).current, { view: 'remote', actionId: null });

  let local = createResourceNavigation('local');
  local = moveResourceNavigation(local, 'capability', 'self:future-resource');
  assert.equal(local.home, 'local');
  assert.deepEqual(backResourceNavigation(local).current, { view: 'local', actionId: null });
});

test('direct entry, same-location clicks and bounded history have deterministic back behavior', () => {
  assert.equal(backResourceNavigation(createResourceNavigation('workspace')).current.view, 'settings');
  assert.equal(backResourceNavigation(createResourceNavigation('touchpad')).current.view, 'local');
  let state = createResourceNavigation('remote');
  state = moveResourceNavigation(state, 'controller');
  assert.equal(moveResourceNavigation(state, 'controller'), state);
  for (let i = 0; i < 30; i++) state = moveResourceNavigation(state, 'quick', `action-${i}`);
  assert.equal(state.history.length, 8);
  for (let i = 0; i < 9; i++) state = backResourceNavigation(state);
  assert.equal(state.current.view, 'remote');
});

test('legacy audio stays out of normal homes but an active operation remains stoppable', () => {
  for (const id of ['capyio.quick-action.remote-speaker', 'capyio.quick-action.remote-microphone']) {
    const action = { id, kind: 'route_control', simulated: false, status: 'idle' };
    assert.deepEqual(resourceActions([action], 'remote'), []);
    assert.equal(resourceActions([{ ...action, status: 'active' }], 'remote').length, 1);
  }
});
