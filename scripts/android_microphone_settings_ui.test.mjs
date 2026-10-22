import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { stripTypeScriptTypes } from 'node:module';
import { parseAndroidMicrophoneSettings } from '../apps/desktop/src/lib/android-microphone-settings.ts';
const base = { schemaVersion: 1, revision: 0, captureProfile: 'legacy', source: 'system_default', eligibility: 'not_requested', permissionReady: false };

test('microphone setting is explicit metadata eligibility, not hardware or playback proof', () => {
  assert.deepEqual(parseAndroidMicrophoneSettings(base), base);
  for (const captureProfile of ['secure_mono_tls13_v1', 'secure_mono_tls13_300s_v1']) for (const eligibility of ['eligible', 'unavailable']) {
    const dto = { ...base, revision: 1, captureProfile, eligibility };
    assert.deepEqual(parseAndroidMicrophoneSettings(dto), dto);
    assert.notEqual(parseAndroidMicrophoneSettings(dto), dto);
  }
});
test('microphone schema rejects coercion, downgrade, unexpected grants and unknown sources', () => {
  for (const value of [null, [], {}, { ...base, schemaVersion: 2 }, { ...base, revision: -1 },
    { ...base, revision: 1.5 }, { ...base, revision: Number.MAX_SAFE_INTEGER + 1 },
    { ...base, captureProfile: 'secure' }, { ...base, captureProfile: 'secure_mono_tls13_301s_v1' },
    { ...base, captureProfile: 'secure_mono_tls13_300s_v1 ' }, { ...base, source: 'bluetooth' },
    { ...base, captureProfile: 'secure_mono_tls13_v1' }, { ...base, captureProfile: 'secure_mono_tls13_300s_v1' },
    { ...base, durationSeconds: 300 }, { ...base, format: 'pcm-s16le-48000-mono-tls13-300s-v1' },
    { ...base, eligibility: 'eligible' }, { ...base, eligibility: 'streaming' },
    { ...base, permissionReady: 1 }, { ...base, host: '127.0.0.1' }, { ...base, grant: {} },
    { ...base, token: 'secret' }, { ...base, eligibility: 'verified' }]) {
    assert.throws(() => parseAndroidMicrophoneSettings(value));
  }
  for (const key of Object.keys(base)) { const dto = { ...base }; delete dto[key]; assert.throws(() => parseAndroidMicrophoneSettings(dto)); }
});
const java = file => readFileSync(new URL(`../platform/android/app/src/main/java/dev/capyio/android/${file}.java`, import.meta.url), 'utf8');
test('secure branch precedes any legacy job reuse and DatagramSocket construction', () => {
  const source = java('AndroidAudioUseModule');
  const dispatch = source.indexOf('synchronizeSecure(runtime, use, config)');
  assert.ok(dispatch > 0 && dispatch < source.indexOf('AudioRouteJob job=jobs.get(id)'));
  assert.ok(dispatch < source.indexOf('new java.net.DatagramSocket'));
  assert.ok(source.indexOf('AndroidAudioRouteTransport.requireLegacy') < source.indexOf('AudioRouteJob job=jobs.get(id)'));
  assert.ok(source.includes('!job.launch.equals(launch)'));
  assert.ok(source.includes('!job.capture.snapshot().cleanupComplete()'));
  assert.ok(source.includes('runtime.audioSecureState(job.generation) == 4'));
  const secureActive = source.slice(source.indexOf('if (capture.state() == SecureMonoCaptureSession.State.ACTIVE'));
  assert.ok(secureActive.indexOf('runtime.audioSecureState(job.generation) == 4') < secureActive.indexOf('runtime.audioUse("active", id'));
  assert.ok(secureActive.indexOf('runtime.audioUse("active", id') < secureActive.indexOf('controller.completeStart'));
  assert.ok(!source.includes('.awaitStopped('));
  assert.ok(source.includes('Math.multiplyExact(capture.acceptedFrames(), 480L)'));
  assert.ok(source.includes('controller.failStop('));
  assert.ok(source.includes('AndroidSecureAudioStartup.start('));
  assert.ok(source.includes('AndroidSecureMicrophoneCapture.hasUnreleasedCapture()'));
});
test('settings stop native and Java generations and unpublish before edit or saved peer mutation', () => {
  const service = java('AudioNodeService');
  const invalidate = service.slice(service.indexOf('private boolean invalidateMicrophoneSettings()'), service.indexOf('public boolean selectMicrophoneProfile'));
  assert.ok(invalidate.indexOf('audioUse.invalidateMicrophone') < invalidate.indexOf('catalogModule.select'));
  const update = service.slice(service.indexOf('public boolean selectMicrophoneProfile'), service.indexOf('public boolean startAudioUse'));
  assert.ok(update.indexOf('expectedRevision != microphoneSettings.revision()') < update.indexOf('microphoneSettings.select'));
  assert.ok(update.indexOf('invalidateMicrophoneSettings()') < update.indexOf('microphoneSettings.select'));
  assert.ok(update.indexOf('microphoneSettings.select') < update.indexOf('refreshNativeCatalog'));
  const activity = java('MainActivity');
  const inOrder = (source, ...tokens) => {
    let after = -1;
    for (const token of tokens) {
      const at = source.indexOf(token, after + 1);
      assert.ok(at > after, `missing or reordered: ${token}`);
      after = at;
    }
  };
  const callback = activity.slice(activity.indexOf('AudioCapabilityKind requested = pendingStart;'), activity.indexOf('protected void onActivityResult'));
  inOrder(callback, 'AudioCapabilityKind requested = pendingStart;', 'pendingStart = null;',
    'AudioNodeService.NodeBinder owner = pendingAudioOwner;', 'pendingAudioOwner = null;',
    'AndroidMicrophoneSettings.Selection selection = pendingAudioPermission.finish();',
    'requiredPermissionsGranted(requested)', 'binder == owner && owner.canStartLegacyMicrophone(selection)',
    'startCapability(requested, selection == null ? -1 : selection.revision());');
  assert.doesNotMatch(callback, /microphoneSelection\(\)/, 'callback must not fetch a replacement selection');
  const request = activity.slice(activity.indexOf('private void requestStart(AudioCapabilityKind kind)'), activity.indexOf('private boolean requiredPermissionsGranted'));
  inOrder(request, 'AndroidMicrophoneSettings.Selection selection = owner == null ? null : owner.microphoneSelection();',
    'requestStart(kind, owner, selection);', 'if (pendingAudioPermission.awaiting()) return;',
    'owner.canStartLegacyMicrophone(selection)', 'if (!pendingAudioPermission.begin(selection)) return;',
    'pendingStart = kind;', 'pendingAudioOwner = owner;', 'requestPermissions(');
  const postedRequest = activity.slice(activity.indexOf('AndroidMicrophoneSettings.Selection selection = current.microphoneSelection();'),
    activity.indexOf('CapabilityLifecycleSnapshot lifecycle;'));
  inOrder(postedRequest, 'AndroidMicrophoneSettings.Selection selection = current.microphoneSelection();',
    'current.canStartLegacyMicrophone(selection)', 'handler.post(() -> {', 'pendingStart = null;',
    'pendingAudioOwner = null;', 'pendingAudioPermission.cancel();', 'AudioNodeService.stopMicrophone(MainActivity.this);',
    'requestStart(audioCommand.target(), current, selection);');
  const settings = java('AndroidMicrophoneSettings');
  const pending = settings.slice(settings.indexOf('static final class PendingPermission'), settings.indexOf('// The capture worker reads'));
  inOrder(pending, 'boolean begin(Selection selection)', 'if (awaiting) return false;',
    'awaiting = true;', 'selected = selection;', 'return true;');
  assert.match(pending, /void cancel\(\) \{ selected = null; \}/, 'cancel must not make the outstanding callback reusable');
  inOrder(pending, 'Selection finish()', 'Selection original = selected;', 'selected = null;',
    'awaiting = false;', 'return original;');
  assert.ok(activity.includes('current.setMicrophoneUiVisible(false)'));
  const adapter = java('MicrophoneSourceAdapter');
  assert.equal(adapter.split('AndroidSecureMicrophoneCapture.hasUnreleasedCapture()').length - 1, 3);
});
test('secure UI distinguishes explicit finite options, per-use consent and synthetic evidence', () => {
  const view = readFileSync(new URL('../apps/desktop/src/components/AudioUsePanel.vue', import.meta.url), 'utf8');
  for (const required of ['value="legacy"', 'value="secure_mono_tls13_v1"', 'value="secure_mono_tls13_300s_v1"',
    'UDP（明文）', '固定 TLS 1.3', '最多 60 秒', '最多 5 分钟（300 秒）', '失败不会退回 UDP',
    '不会自动升级', '原始授权或租约期限可能更短', '独立的有限会话授权', '不会开始录音、延长授权或自动续期',
    '选择配置不表示正在传输', '不支持安全扬声器或安全双工', '合成测试不保证真机录音', '当前不提供 Windows 安全音频',
    '取消共享', '不会自动弹出权限请求', '系统默认麦克风']) assert.ok(view.includes(required), required);
  assert.doesNotMatch(view, /Linux 生产启动器的端到端接入尚未完成|Linux 当前仅支持指定目标播放|Streaming/);
});

function panelModel(api) {
  const view = readFileSync(new URL('../apps/desktop/src/components/AudioUsePanel.vue', import.meta.url), 'utf8');
  const script = stripTypeScriptTypes(view.slice(view.indexOf('const peers ='), view.indexOf('function resumed()')));
  return new Function('props', 'ref', 'computed', script + '\nreturn { refresh, selectProfile, change, selected, microphoneSettings, microphoneDirection, secureStartBlocked, statusLabel, errors };')(
    { api }, value => ({ value }), getter => ({ get value() { return getter(); } }));
}

test('restoring either secure setting is metadata only; explicit selection preserves exact revision without starting or renewing', async () => {
  for (const initial of ['legacy', 'secure_mono_tls13_v1', 'secure_mono_tls13_300s_v1']) {
    let settings = { ...base, revision: 7, captureProfile: initial, eligibility: initial === 'legacy' ? 'not_requested' : 'eligible', permissionReady: true };
    const selections = [], starts = [], stops = [];
    const model = panelModel({
      getAudioPeers: async () => [], getAudioUses: async () => [], getAndroidMicrophoneSettings: async () => settings,
      selectAndroidMicrophoneProfile: async (profile, revision) => {
        selections.push([profile, revision]);
        settings = { ...settings, captureProfile: profile, revision: revision + 1, eligibility: profile === 'legacy' ? 'not_requested' : 'eligible' };
        return settings;
      },
      startAudioUse: async (...args) => starts.push(args), stopAudioUse: async (...args) => stops.push(args),
    });
    await model.refresh();
    assert.equal(model.microphoneSettings.value.captureProfile, initial);
    assert.equal(model.microphoneDirection.value, initial === 'legacy' ? 'microphone' : 'microphone-secure');
    assert.deepEqual(selections, []);
    assert.equal(model.statusLabel(model.microphoneDirection.value), '未使用');
    await model.selectProfile({ target: { value: 'secure_mono_tls13_300s_v1' } });
    assert.deepEqual(selections, [['secure_mono_tls13_300s_v1', 7]]);
    assert.equal(model.microphoneDirection.value, 'microphone-secure');
    assert.equal(model.microphoneSettings.value.revision, 8);
    assert.equal(model.secureStartBlocked('microphone-secure'), false);
    model.microphoneSettings.value.permissionReady = false;
    assert.equal(model.secureStartBlocked('microphone-secure'), true);
    model.microphoneSettings.value.permissionReady = true;
    model.microphoneSettings.value.eligibility = 'unavailable';
    assert.equal(model.secureStartBlocked('microphone-secure'), true);
    await model.selectProfile({ target: { value: 'secure_mono_tls13_301s_v1' } });
    assert.equal(selections.length, 1);
    assert.deepEqual(starts, []);
    assert.deepEqual(stops, []);
  }
});
