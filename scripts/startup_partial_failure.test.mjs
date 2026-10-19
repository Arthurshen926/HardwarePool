import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { stripTypeScriptTypes } from 'node:module';

const source = readFileSync(new URL('../apps/desktop/src/App.vue', import.meta.url), 'utf8');
const code = stripTypeScriptTypes(source.slice(source.indexOf('async function load()'), source.indexOf('async function refreshCatalog()')));
function model(api) {
  const refs = Object.fromEntries(['loading', 'snapshot', 'errorMessage', 'initializationProblems', 'catalogUpdatedAt', 'liveImu', 'gamepad', 'touchpad', 'androidAudio', 'androidCamera', 'quickActions', 'audioEndpoints'].map(name => [name, { value: null }]));
  const load = new Function('api', 'normalizeError', ...Object.keys(refs), `${code}; return load;`)(api, String, ...Object.values(refs));
  return { ...refs, load };
}
const api = () => Object.fromEntries(['getSnapshot','getLiveImu','getGamepadState','getTouchpadState','getAndroidAudioState','getAndroidCameraSettings','getQuickActions','getAudioEndpoints'].map(name => [name, async () => ({ name })]));

test('optional startup failure retains the catalog and other capabilities; explicit retry recovers', async () => {
  const backend = api();
  backend.getLiveImu = async () => { throw new Error('sensor unavailable'); };
  const state = model(backend);
  await state.load();
  assert.deepEqual(state.snapshot.value, { name: 'getSnapshot' });
  assert.deepEqual(state.gamepad.value, { name: 'getGamepadState' });
  assert.deepEqual(state.quickActions.value, { name: 'getQuickActions' });
  assert.equal(state.errorMessage.value, '');
  assert.deepEqual(state.initializationProblems.value, [{ label: '体感状态', detail: 'Error: sensor unavailable' }]);
  backend.getLiveImu = async () => null;
  await state.load();
  assert.deepEqual(state.initializationProblems.value, []);
  assert.equal(state.loading.value, false);
});

test('required node failure stays visible and does not query optional resources', async () => {
  const backend = api();
  let optionalReads = 0;
  backend.getSnapshot = async () => { throw new Error('node unavailable'); };
  backend.getGamepadState = async () => { optionalReads++; };
  const state = model(backend);
  await state.load();
  assert.equal(state.snapshot.value, null);
  assert.match(state.errorMessage.value, /node unavailable/);
  assert.equal(optionalReads, 0);
  assert.equal(state.loading.value, false);
});
