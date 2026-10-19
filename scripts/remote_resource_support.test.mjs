import assert from 'node:assert/strict';
import test from 'node:test';
import { hasIntegratedRemoteDataBinding, remoteResourceUseHint, knownRouteUsage } from '../apps/desktop/src/lib/remote-resource-support.ts';

const touchpad = {
  id: 'remote-touchpad', local: false, simulated: false, capabilityClass: 'touchpad',
  availability: 'available', displayName: '任意设备名称',
  ports: [{ direction: 'source', profile: 'capyio.input.touchpad-frames/1', formats: [] }],
};

test('integrated use requires a real remote touchpad Source with the supported profile', () => {
  assert.equal(hasIntegratedRemoteDataBinding(touchpad), true);
  for (const variant of [
    { local: true }, { simulated: true }, { capabilityClass: 'camera' },
    { ports: [{ direction: 'sink', profile: 'capyio.input.touchpad-frames/1' }] },
    { ports: [{ direction: 'source', profile: 'capyio.input.touchpad-frames/2' }] },
    { ports: [{ direction: 'source', profile: 'capyio.input.touch-events/1' }] },
  ]) assert.equal(hasIntegratedRemoteDataBinding({ ...touchpad, ...variant }), false);
});

test('catalog hints distinguish integrated, offline, experimental and simulated resources', () => {
  assert.equal(remoteResourceUseHint(touchpad).label, '触控板可申请');
  assert.equal(remoteResourceUseHint({ ...touchpad, availability: 'offline' }).label, '触控板当前离线');
  assert.equal(remoteResourceUseHint({ ...touchpad, local: true }), null);
  assert.equal(remoteResourceUseHint({ ...touchpad, simulated: true }).label, '演示资源');
  for (const capabilityClass of ['microphone', 'speaker', 'camera', 'imu', 'display', 'haptics', 'future-profile']) {
    const resource = { ...touchpad, capabilityClass };
    assert.match(remoteResourceUseHint(resource).label, /实验功能入口/);
    assert.match(remoteResourceUseHint(resource).detail, /尚未接入/);
  }
});

test('prepared and starting routes are visible without claiming media is active', () => {
  assert.match(knownRouteUsage([{ state: 'draft' }, { state: 'prepared' }, { state: 'starting' }], 0), /申请中.*已准备.*启动中／等待媒体/);
  assert.doesNotMatch(knownRouteUsage([{ state: 'starting' }], 1), /没有已知 Route|使用中/);
  assert.match(knownRouteUsage([], 0), /没有已知 Route/);
});

test('gamepad points to its real one-click panel, not generic resource dispatch', () => {
  const gamepad = { ...touchpad, capabilityClass: 'gamepad' };
  assert.equal(hasIntegratedRemoteDataBinding(gamepad), false);
  assert.match(remoteResourceUseHint(gamepad).label, /手柄与体感/);
  assert.match(remoteResourceUseHint(gamepad).detail, /选择已认证节点/);
});

test('reverse vibration is a real remote haptics Sink, not an input grant', () => {
  const sink = { ...touchpad, capabilityClass: 'haptics', ports: [{ direction: 'sink', profile: 'capyio.haptics.feedback/1' }] };
  assert.equal(hasIntegratedRemoteDataBinding(sink), true);
  assert.match(remoteResourceUseHint(sink).label, /振动/);
  assert.equal(hasIntegratedRemoteDataBinding({ ...sink, ports: [{ direction: 'source', profile: 'capyio.haptics.feedback/1' }] }), false);
  assert.equal(hasIntegratedRemoteDataBinding({ ...sink, simulated: true }), false);
});

test('audio entry preserves real microphone Source and speaker Sink directions', () => {
  for (const [capabilityClass, direction] of [['microphone', 'source'], ['speaker', 'sink']]) {
    const audio = { ...touchpad, capabilityClass, ports: [{ direction, profile: 'capyio.audio.frames/1' }] };
    assert.equal(hasIntegratedRemoteDataBinding(audio), true);
    assert.match(remoteResourceUseHint(audio).label, /音频/);
    assert.equal(hasIntegratedRemoteDataBinding({ ...audio, simulated: true }), false);
    assert.equal(hasIntegratedRemoteDataBinding({ ...audio, ports: [{ direction: direction === 'source' ? 'sink' : 'source', profile: 'capyio.audio.frames/1' }] }), false);
  }
});

test('camera entry requires a real remote video Source', () => {
  const camera = { ...touchpad, capabilityClass: 'camera', ports: [{ direction: 'source', profile: 'capyio.video.frames/1' }] };
  assert.equal(hasIntegratedRemoteDataBinding(camera), true);
  assert.match(remoteResourceUseHint(camera).label, /相机/);
  assert.equal(hasIntegratedRemoteDataBinding({ ...camera, simulated: true }), false);
  assert.equal(hasIntegratedRemoteDataBinding({ ...camera, ports: [{ direction: 'sink', profile: 'capyio.video.frames/1' }] }), false);
});
