import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { stripTypeScriptTypes } from 'node:module';
import { hasIntegratedRemoteDataBinding, hasLegacyRemoteAudioBinding, remoteResourceUseHint, knownRouteUsage } from '../apps/desktop/src/lib/remote-resource-support.ts';

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

test('audio entry preserves every native legacy format and exact Source/Sink direction', () => {
  const native = readFileSync(new URL('../apps/desktop/src-tauri/src/native_audio_route.rs', import.meta.url), 'utf8');
  const descriptorFunction = native.slice(native.indexOf('fn format('), native.indexOf('fn local_port('));
  assert.deepEqual([...descriptorFunction.matchAll(/"(pcm-[^"]+)"/g)].map(match => match[1]).sort(),
    ['pcm-s16le-48000-mono', 'pcm-s16le-48000-stereo']);
  for (const [capabilityClass, direction, format] of [['microphone', 'source', 'pcm-s16le-48000-mono'], ['speaker', 'sink', 'pcm-s16le-48000-stereo']]) {
    const audio = { ...touchpad, capabilityClass, ports: [{ direction, profile: 'capyio.audio.frames/1', formats: [format] }] };
    assert.equal(hasIntegratedRemoteDataBinding(audio), true);
    assert.equal(hasLegacyRemoteAudioBinding(audio), true);
    assert.match(remoteResourceUseHint(audio).label, /音频/);
    assert.match(remoteResourceUseHint(audio).detail, /UDP 明文/);
    assert.doesNotMatch(remoteResourceUseHint(audio).detail, /安全音频|加密/);
    assert.equal(hasIntegratedRemoteDataBinding({ ...audio, simulated: true }), false);
    for (const port of [{ ...audio.ports[0], direction: direction === 'source' ? 'sink' : 'source' },
      { ...audio.ports[0], profile: 'capyio.audio.frames/2' }, { ...audio.ports[0], formats: [] },
      { ...audio.ports[0], formats: [format === 'pcm-s16le-48000-mono' ? 'pcm-s16le-48000-stereo' : 'pcm-s16le-48000-mono'] }]) {
      assert.equal(hasIntegratedRemoteDataBinding({ ...audio, ports: [port] }), false);
    }
  }
});

const secureAudio = format => ({ ...touchpad, capabilityClass: 'microphone',
  ports: [{ direction: 'source', profile: 'capyio.audio.frames/1', formats: [format] }] });

test('exact secure descriptors are limited catalog hints, never Windows readiness or encrypted session state', () => {
  for (const [format, maximum] of [['pcm-s16le-48000-mono-tls13-v1', /最多 60 秒/],
    ['pcm-s16le-48000-mono-tls13-300s-v1', /最多 5 分钟（300 秒）/]]) {
    const audio = secureAudio(format);
    assert.equal(hasIntegratedRemoteDataBinding(audio), false);
    assert.equal(hasLegacyRemoteAudioBinding(audio), false);
    const hint = remoteResourceUseHint(audio);
    assert.equal(hint.label, '安全音频需匹配接收端');
    assert.match(hint.detail, maximum);
    assert.match(hint.detail, /原始授权或租约可能更短/);
    assert.match(hint.detail, /目录不表示已经授权、加密传输或播放/);
    assert.match(hint.detail, /不支持 Windows 一键使用/);
    assert.match(hint.detail, /不会退回 UDP/);
    assert.match(hint.detail, /合成验证不保证真机兼容/);
    assert.doesNotMatch(hint.label, /Streaming|使用中|已加密/);
    assert.equal(remoteResourceUseHint({ ...audio, availability: 'offline' }).label, '安全音频当前离线');
    assert.equal(remoteResourceUseHint({ ...audio, simulated: true }).label, '演示资源');
    assert.equal(remoteResourceUseHint({ ...audio, local: true }), null);
  }
});

test('unknown, swapped and mixed audio descriptors cannot imply TLS or silently fall back to legacy', () => {
  for (const formats of [[], ['pcm-s16le-48000-mono-tls13-301s-v1'], ['pcm-s16le-48000-mono-tls13-300s-v1 '],
    ['pcm-s16le-48000-mono-tls13-300s-v1', 'pcm-s16le-48000-mono'],
    ['pcm-s16le-48000-mono-tls13-300s-v1', 'pcm-s16le-48000-mono-tls13-v1']]) {
    const audio = secureAudio(''); audio.ports[0].formats = formats;
    assert.equal(hasIntegratedRemoteDataBinding(audio), false);
    assert.equal(hasLegacyRemoteAudioBinding(audio), false);
    assert.equal(remoteResourceUseHint(audio).label, '音频配置不受支持');
  }
  const wrongDirection = secureAudio('pcm-s16le-48000-mono-tls13-300s-v1');
  wrongDirection.ports[0].direction = 'sink';
  assert.equal(remoteResourceUseHint(wrongDirection).label, '音频配置不受支持');
  assert.equal(remoteResourceUseHint({ ...wrongDirection, capabilityClass: 'speaker' }).label, '音频配置不受支持');
  const separatePorts = secureAudio('pcm-s16le-48000-mono-tls13-300s-v1');
  separatePorts.ports.push({ ...separatePorts.ports[0], formats: ['pcm-s16le-48000-mono'] });
  assert.equal(hasIntegratedRemoteDataBinding(separatePorts), false);
  assert.equal(remoteResourceUseHint(separatePorts).label, '音频配置不受支持');
});

test('secure or unknown audio cannot Start or fall back, but Stop and existing legacy actions remain available', async () => {
  const view = readFileSync(new URL('../apps/desktop/src/App.vue', import.meta.url), 'utf8');
  const compatibility = stripTypeScriptTypes(view.slice(view.indexOf('const selectedCompatibilityAction ='), view.indexOf('const selectedCatalogLeases =')));
  const start = stripTypeScriptTypes(view.slice(view.indexOf('async function toggleRemoteCapabilityUse()'), view.indexOf('async function stopRemoteCapabilityUse(')));
  const secureFormats = ['pcm-s16le-48000-mono-tls13-v1', 'pcm-s16le-48000-mono-tls13-300s-v1', 'future-secure'];
  const cases = secureFormats.flatMap(format => [[secureAudio(format), false, false],
    [secureAudio(format), true, false], [secureAudio(format), false, true]]);
  cases.push([secureAudio('pcm-s16le-48000-mono'), false, false]);
  const speaker = secureAudio('pcm-s16le-48000-stereo');
  speaker.capabilityClass = 'speaker'; speaker.ports[0].direction = 'sink';
  cases.push([speaker, false, false]);
  for (const [capability, inUse, pending] of cases) {
    const calls = [];
    const action = { id: `capyio.quick-action.remote-${capability.capabilityClass}`, simulated: false };
    const model = new Function('computed', 'selectedCatalogCapability', 'selectedCatalogNode', 'quickActions',
      'hasLegacyRemoteAudioBinding', 'hasIntegratedRemoteDataBinding', 'selectedRemoteCapabilityInUse', 'selectedRemoteCapabilityPending',
      'remoteUseBusy', 'errorMessage', 'snapshot', 'api', 'normalizeError', compatibility + start + '\nreturn { selectedCompatibilityAction, toggleRemoteCapabilityUse };')(
      getter => ({ get value() { return getter(); } }), { value: capability }, { value: { platform: 'android' } },
      { value: [action] }, hasLegacyRemoteAudioBinding, hasIntegratedRemoteDataBinding,
      { value: inUse }, { value: pending }, { value: false }, { value: '' }, { value: null }, { setRemoteCapabilityUse: async (...args) => calls.push(args) }, String);
    const legacy = hasLegacyRemoteAudioBinding(capability);
    assert.equal(model.selectedCompatibilityAction.value, legacy ? action : undefined);
    await model.toggleRemoteCapabilityUse();
    assert.deepEqual(calls, inUse || pending ? [[capability.id, false]] : legacy ? [[capability.id, true]] : []);
  }
});

test('camera entry requires a real remote video Source', () => {
  const camera = { ...touchpad, capabilityClass: 'camera', ports: [{ direction: 'source', profile: 'capyio.video.frames/1' }] };
  assert.equal(hasIntegratedRemoteDataBinding(camera), true);
  assert.match(remoteResourceUseHint(camera).label, /相机/);
  assert.equal(hasIntegratedRemoteDataBinding({ ...camera, simulated: true }), false);
  assert.equal(hasIntegratedRemoteDataBinding({ ...camera, ports: [{ direction: 'sink', profile: 'capyio.video.frames/1' }] }), false);
});
