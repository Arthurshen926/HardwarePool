import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

for (const name of ['AudioUsePanel', 'CameraUsePanel', 'DisplayUsePanel']) {
  test(`${name}: pause hidden polling, refresh on return, remove listener on exit`, () => {
    const source = readFileSync(new URL(`../apps/desktop/src/components/${name}.vue`, import.meta.url), 'utf8');
    const lifecycle = source.slice(source.indexOf('function resumed()'), source.indexOf('</script>'));
    let reads = 0, unmount, tick;
    const listeners = new Map();
    const document = {hidden:false, addEventListener:(name, fn) => listeners.set(name, fn), removeEventListener:(name, fn) => {assert.equal(listeners.get(name), fn);listeners.delete(name);}};
    new Function('document', 'refresh', 'onMounted', 'onUnmounted', 'setInterval', 'clearInterval', `let timer, disposed = false, readGeneration = 0; ${lifecycle}`)(
      document, () => reads++, fn => fn(), fn => unmount = fn,
      fn => {tick = fn;return 1;}, id => assert.equal(id, 1));
    assert.equal(reads, 1);
    document.hidden = true;
    tick(); listeners.get('visibilitychange')();
    assert.equal(reads, 1);
    document.hidden = false;
    listeners.get('visibilitychange')();
    assert.equal(reads, 2);
    unmount();
    assert.equal(listeners.size, 0);
  });
}
