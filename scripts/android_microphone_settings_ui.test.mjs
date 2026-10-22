import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { parseAndroidMicrophoneSettings } from '../apps/desktop/src/lib/android-microphone-settings.ts';
const base = { schemaVersion: 1, revision: 0, captureProfile: 'legacy', source: 'system_default', eligibility: 'not_requested', permissionReady: false };

test('microphone setting is explicit metadata eligibility, not hardware or playback proof', () => {
  assert.deepEqual(parseAndroidMicrophoneSettings(base), base);
  for (const eligibility of ['eligible', 'unavailable']) {
    const dto = { ...base, revision: 1, captureProfile: 'secure_mono_tls13_v1', eligibility };
    assert.deepEqual(parseAndroidMicrophoneSettings(dto), dto);
    assert.notEqual(parseAndroidMicrophoneSettings(dto), dto);
  }
});
test('microphone schema rejects coercion, downgrade, unexpected grants and unknown sources', () => {
  for (const value of [null, [], {}, { ...base, schemaVersion: 2 }, { ...base, revision: -1 },
    { ...base, revision: 1.5 }, { ...base, revision: Number.MAX_SAFE_INTEGER + 1 },
    { ...base, captureProfile: 'secure' }, { ...base, source: 'bluetooth' },
    { ...base, captureProfile: 'secure_mono_tls13_v1' }, { ...base, eligibility: 'eligible' },
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
  assert.ok(activity.includes('binder == owner && owner.canStartLegacyMicrophone(pendingMicrophoneRevision)'));
  assert.ok(activity.includes('current.setMicrophoneUiVisible(false)'));
  const adapter = java('MicrophoneSourceAdapter');
  assert.equal(adapter.split('AndroidSecureMicrophoneCapture.hasUnreleasedCapture()').length - 1, 3);
});
test('secure UI distinguishes bounded experimental mono from plaintext and Linux readiness', () => {
  const view = readFileSync(new URL('../apps/desktop/src/components/AudioUsePanel.vue', import.meta.url), 'utf8');
  for (const required of ['UDP（明文）', '固定 TLS 1.3', '最多 60 秒', '失败不会退回 UDP',
    '不支持安全扬声器或安全双工', '不保证真机录音', 'Linux 生产启动器的端到端接入尚未完成',
    '取消共享', '不会自动弹出权限请求', '系统默认麦克风']) assert.ok(view.includes(required), required);
});
