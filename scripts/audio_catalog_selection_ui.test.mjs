import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
const panel = readFileSync(new URL('../apps/desktop/src/components/AudioUsePanel.vue', import.meta.url), 'utf8');
const types = readFileSync(new URL('../apps/desktop/src/lib/types.ts', import.meta.url), 'utf8');
const native = readFileSync(new URL('../platform/android/capyio-jni/src/android_ffi/audio_use.rs', import.meta.url), 'utf8');
test('unsupported audio directions are optional and displayed separately from actionable candidates', () => {
  assert.match(types, /unavailableDirections\?:/);
  assert.match(panel, /peer\.unavailableDirections \?\? \[\]/);
  assert.match(panel, /function candidates\(direction: AudioUseDirection\) \{ return peers\.value\.filter\(peer => peer\.directions\.includes\(direction\)\); \}/);
  assert.match(panel, /v-for="peer in unavailable\(direction\)"/);
  assert.match(panel, /!candidates\(direction\)\.some\(peer => peer\.nodeId === selected\[direction\]\)/);
});
test('JNI binds Start from a fresh snapshot through the tested selector', () => {
  assert.match(native, /select_route\(&snapshot\.local_node, peer_node, &snapshot\.sessions, direction\)/);
  assert.match(native, /create_route\(session,source,sink,RouteBackend::AdapterManaged\)/);
  assert.doesNotMatch(native, /d41251b3|suffix|等待 Windows/);
  assert.match(native, /supports_inbound_draft\(peer, endpoint, direction\)/);
  assert.match(native, /unavailableDirections/);
});
