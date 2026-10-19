import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { stripTypeScriptTypes } from 'node:module';

const source = readFileSync(new URL('../apps/desktop/src/components/DisplayUsePanel.vue', import.meta.url), 'utf8');
const script = source.slice(source.indexOf('const peers ='), source.indexOf('async function start()'));
async function panel(states) {
  const model = new Function('props', 'ref', 'computed', stripTypeScriptTypes(script) + '\nreturn {refresh,peer,mode,automatic,refreshMillihertz,bitrateKbps};')(
    { api: { getDisplayPeers: async () => [{nodeId:'windows'}], getDisplayUse: async () => states.shift() } },
    value => ({value}), getter => ({get value() {return getter();}}));
  await model.refresh();
  return model;
}
test('remote-started display restores actual quality and retains it after stop', async () => {
  const model = await panel([{status:'active',peerNodeId:'windows',automatic:true,width:2554,height:1600,refreshMillihertz:60000,bitrateKbps:29422},null]);
  assert.equal(model.peer.value,'windows');
  assert.equal(model.mode.value,'2554x1600');
  assert.equal(model.automatic.value,true);
  assert.equal(model.bitrateKbps.value,29422);
  await model.refresh();
  assert.equal(model.mode.value,'2554x1600');
  assert.equal(model.bitrateKbps.value,29422);
});
test('manual remote session restores frame rate without inventing automatic mode', async () => {
  const model = await panel([{status:'active',peerNodeId:'windows',width:1280,height:720,refreshMillihertz:30000,bitrateKbps:5000}]);
  assert.equal(model.automatic.value,false);
  assert.equal(model.mode.value,'1280x720');
  assert.equal(model.refreshMillihertz.value,30000);
});
