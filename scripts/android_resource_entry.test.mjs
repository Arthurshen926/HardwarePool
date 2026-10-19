import { test } from 'node:test';
import assert from 'node:assert/strict';
import { androidResourceEntry as entry, controllerResourceMode } from '../apps/desktop/src/lib/android-resource-entry.ts';
const mic = { id: 'd41251b3-63ed-4e83-807e-4c4ab9b20d21', local: false, simulated: false,
  ports: [{ direction: 'sink', profile: 'capyio.audio.frames/1' }] };
test('controller card selects its exact declared mode', () => {
  for (const [suffix, mode] of [['21','dualshock4'],['22','xbox360'],['23','dsu']]) {
    const c = {...mic, id:'d41251b3-63ed-4e83-807e-4c4ab9b20c'+suffix, ports:[{direction:'sink',profile:'capyio.input.gamepad-state/1'}]};
    assert.equal(controllerResourceMode(c), mode);
    assert.equal(controllerResourceMode({...c, simulated:true}), undefined);
    assert.equal(controllerResourceMode({...c, ports:[]}), undefined);
  }
  assert.equal(controllerResourceMode(mic), undefined);
});
test('native media contracts map to their existing Android workflows', () => {
  assert.equal(entry(mic), 'microphone');
  assert.equal(entry({ ...mic, id: 'd41251b3-63ed-4e83-807e-4c4ab9b20d22', ports: [{ direction: 'source', profile: 'capyio.audio.frames/1' }] }), 'speaker');
  assert.equal(entry({ ...mic, id: 'd41251b3-63ed-4e83-807e-4c4ab9b20e21', ports: [{ direction: 'sink', profile: 'capyio.video.frames/1' }] }), 'camera');
});
test('local, simulated, wrong direction and unknown resources do not get a media action', () => {
  for (const c of [null, { ...mic, local: true }, { ...mic, simulated: true },
    { ...mic, id: 'unknown' }, { ...mic, ports: [{ direction: 'source', profile: 'capyio.audio.frames/1' }] }]) {
    assert.equal(entry(c), null);
  }
});
