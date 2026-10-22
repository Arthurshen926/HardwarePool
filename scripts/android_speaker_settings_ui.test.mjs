import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { stripTypeScriptTypes } from 'node:module';
import { parseAndroidSpeakerSettings } from '../apps/desktop/src/lib/android-speaker-settings.ts';
import { hasLegacyRemoteAudioBinding, hasIntegratedRemoteDataBinding, remoteResourceUseHint } from '../apps/desktop/src/lib/remote-resource-support.ts';
const base = { schemaVersion: 1, revision: '0', playbackProfile: 'legacy', source: 'system_default', eligibility: 'not_requested', permissionReady: false };
const secure = { ...base, playbackProfile: 'secure_stereo_tls13_v1', eligibility: 'eligible', permissionReady: true };
const view = readFileSync(new URL('../apps/desktop/src/components/AudioUsePanel.vue', import.meta.url), 'utf8');
function model(api) {
  const script = stripTypeScriptTypes(view.slice(view.indexOf('const peers ='), view.indexOf('function resumed()')));
  return new Function('props', 'ref', 'computed', script + '\nreturn { refresh, selectProfile, selectSpeakerProfile, change, speakerSettings, microphoneSettings, speakerDirection, secureStartBlocked, statusLabel, speakerProfileError };')(
    { api }, value => ({ value }), getter => ({ get value() { return getter(); } }));
}
function fixture(initial = base) {
  let settings = initial;
  const selections = [], starts = [], stops = [];
  const microphone = { schemaVersion: 1, revision: 9, captureProfile: 'secure_mono_tls13_300s_v1', source: 'system_default', eligibility: 'eligible', permissionReady: true };
  const api = {
    getAudioPeers: async () => [], getAudioUses: async () => [], getAndroidMicrophoneSettings: async () => microphone,
    getAndroidSpeakerSettings: async () => settings,
    selectAndroidSpeakerProfile: async (profile, revision) => {
      selections.push([profile, revision]);
      assert.equal(revision, settings.revision);
      settings = { ...settings, playbackProfile: profile, revision: String(BigInt(revision) + 1n), eligibility: profile === 'legacy' ? 'not_requested' : 'eligible' };
      return settings;
    },
    startAudioUse: async (...args) => starts.push(args), stopAudioUse: async (...args) => stops.push(args),
  };
  return { api, selections, starts, stops, microphone };
}
test('closed speaker metadata accepts only explicit legacy and fixed secure stereo', () => {
  for (const dto of [base, secure, { ...secure, eligibility: 'unavailable', revision: '9007199254740991' }]) {
    assert.deepEqual(parseAndroidSpeakerSettings(dto), dto);
    assert.notEqual(parseAndroidSpeakerSettings(dto), dto);
  }
  for (const revision of [0, -1, null, '', '01', '-1', '+1', '1.0', '1e2', ' 1', '1 ', '9007199254740992', '999999999999999999999999']) {
    assert.throws(() => parseAndroidSpeakerSettings({ ...base, revision }));
  }
  for (const bad of [null, [], {}, { ...base, schemaVersion: 2 }, { ...base, playbackProfile: 'secure' },
    { ...base, playbackProfile: 'secure_stereo_tls13_300s_v1' }, { ...base, playbackProfile: 'secure_mono_tls13_v1' },
    { ...base, playbackProfile: 'secure_stereo_tls13_v1' }, { ...secure, eligibility: 'not_requested' },
    { ...base, eligibility: 'eligible' }, { ...base, source: 'bluetooth' }, { ...base, permissionReady: 1 },
    { ...base, transport: 'udp' }, { ...base, durationSeconds: 300 }, { ...base, grant: {} }, { ...base, host: 'localhost' },
    { ...base, token: 'secret' }, { ...secure, eligibility: 'audible' }]) assert.throws(() => parseAndroidSpeakerSettings(bad));
  for (const key of Object.keys(base)) { const dto = { ...base }; delete dto[key]; assert.throws(() => parseAndroidSpeakerSettings(dto)); }
});
test('repeated independent speaker edits preserve exact revision and microphone 300s choice without starts or grants', async () => {
  const f = fixture(); const ui = model(f.api); await ui.refresh();
  for (const profile of ['secure_stereo_tls13_v1', 'legacy', 'secure_stereo_tls13_v1']) await ui.selectSpeakerProfile({ target: { value: profile } });
  assert.deepEqual(f.selections, [['secure_stereo_tls13_v1', '0'], ['legacy', '1'], ['secure_stereo_tls13_v1', '2']]);
  assert.deepEqual(ui.microphoneSettings.value, f.microphone);
  assert.equal(ui.speakerSettings.value.revision, '3');
  assert.equal(ui.speakerDirection.value, 'speaker-secure');
  assert.equal(ui.secureStartBlocked('speaker-secure'), true);
  await ui.change('speaker-secure');
  await ui.selectSpeakerProfile({ target: { value: 'secure_stereo_tls13_300s_v1' } });
  assert.equal(f.selections.length, 3); assert.deepEqual(f.starts, []); assert.deepEqual(f.stops, []);
});
test('late pre-edit polling snapshot cannot restore a stale speaker profile', async () => {
  const f = fixture(); const ui = model(f.api); await ui.refresh();
  let resolveOld; const currentRead = f.api.getAndroidSpeakerSettings;
  f.api.getAndroidSpeakerSettings = () => new Promise(resolve => { resolveOld = resolve; });
  const oldRead = ui.refresh();
  f.api.getAndroidSpeakerSettings = currentRead;
  await ui.selectSpeakerProfile({ target: { value: 'secure_stereo_tls13_v1' } });
  resolveOld(base); await oldRead;
  assert.equal(ui.speakerSettings.value.playbackProfile, 'secure_stereo_tls13_v1');
  assert.equal(ui.speakerSettings.value.revision, '1');
});
test('busy selection rejects overlapping edits; failed stale CAS refreshes without retry/start', async () => {
  const f = fixture(); const ui = model(f.api); await ui.refresh();
  let release; f.api.selectAndroidSpeakerProfile = () => new Promise((_, reject) => { release = reject; });
  const pending = ui.selectSpeakerProfile({ target: { value: 'secure_stereo_tls13_v1' } });
  await ui.selectSpeakerProfile({ target: { value: 'legacy' } });
  release(new Error('stale revision')); await pending;
  assert.match(ui.speakerProfileError.value, /stale revision/);
  assert.deepEqual(ui.speakerSettings.value, base); assert.deepEqual(f.starts, []);
});
test('secure inbound use permits Stop but never reports active PCM as audibility', async () => {
  const f = fixture(secure); f.api.getAudioUses = async () => [{ direction: 'speaker-secure', peerNodeId: 'peer', status: 'active', message: 'playing' }];
  const ui = model(f.api); await ui.refresh();
  assert.match(ui.statusLabel('speaker-secure'), /Starting.*不证明可听见/);
  await ui.change('speaker-secure'); assert.deepEqual(f.stops, [['speaker-secure']]); assert.deepEqual(f.starts, []);
  for (const text of ['兼容 Source API', 'Linux 捕获启动器／终端目前仍是 Legacy', '最多 60 秒', '不会自动启动、重连或续期', '不会退回 UDP']) assert.ok(view.includes(text), text);
});
test('Windows audio action still admits only exact legacy and rejects secure or mixed stereo', () => {
  const make = formats => ({ local: false, simulated: false, capabilityClass: 'speaker', availability: 'available', ports: [{ direction: 'sink', profile: 'capyio.audio.frames/1', formats }] });
  assert.equal(hasLegacyRemoteAudioBinding(make(['pcm-s16le-48000-stereo'])), true);
  for (const formats of [['pcm-s16le-48000-stereo-tls13-v1'], ['pcm-s16le-48000-stereo', 'pcm-s16le-48000-stereo-tls13-v1'], ['unknown']]) {
    assert.equal(hasLegacyRemoteAudioBinding(make(formats)), false); assert.equal(hasIntegratedRemoteDataBinding(make(formats)), false);
  }
  const hint = remoteResourceUseHint(make(['pcm-s16le-48000-stereo-tls13-v1']));
  assert.match(hint.label, /Source API/); assert.match(hint.detail, /不支持 Windows 一键使用/); assert.match(hint.detail, /不会退回 UDP/);
  assert.equal(remoteResourceUseHint(make(['pcm-s16le-48000-stereo-tls13-v1', 'pcm-s16le-48000-stereo'])).label, '音频配置不受支持');
});

test('Android API rejects outbound secure speaker before invoking the WebView', async () => {
  const source = readFileSync(new URL('../apps/desktop/src/lib/api.ts', import.meta.url), 'utf8');
  const method = stripTypeScriptTypes('const api = {' + source.slice(source.indexOf('  async startAudioUse(peerNodeId:'), source.indexOf('  async stopAudioUse(direction:')) + '};');
  const calls = [];
  const api = new Function('window', method + 'return api;')({ CapyIOAndroid: { startAudioUse: (...args) => { calls.push(args); return true; } } });
  await assert.rejects(api.startAudioUse('peer', 'speaker-secure'), /Source API/);
  assert.deepEqual(calls, []);
  await api.startAudioUse('peer', 'speaker');
  assert.deepEqual(calls, [['peer', 'speaker']]);
});
