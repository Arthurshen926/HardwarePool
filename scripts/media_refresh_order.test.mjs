import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { stripTypeScriptTypes } from 'node:module';

for (const [panel, peersMethod, stateMethod, expression, oldState, newState] of [
  ['AudioUsePanel','getAudioPeers','getAudioUses','uses.value',[],[{direction:'speaker',status:'active',peerNodeId:'pc'}]],
  ['CameraUsePanel','getCameraPeers','getCameraUse','use.value',null,{status:'active',peerNodeId:'pc'}],
  ['DisplayUsePanel','getDisplayPeers','getDisplayUse','state.value',null,{status:'active',peerNodeId:'pc',width:1920,height:1080,refreshMillihertz:60000,bitrateKbps:15000}],
]) {
  for (const staleFailure of [false,true]) test(`${panel}: late old ${staleFailure ? 'failure' : 'state'} cannot replace post-action refresh`, async () => {
    const source = readFileSync(new URL(`../apps/desktop/src/components/${panel}.vue`, import.meta.url),'utf8');
    const end = source.indexOf(panel === 'DisplayUsePanel' ? 'async function start()' : 'async function change(');
    const script = stripTypeScriptTypes(source.slice(source.indexOf('const peers ='),end));
    let finishOld, failOld, calls = 0;
    const oldRead = new Promise((resolve,reject) => {finishOld = resolve;failOld = reject;});
    const api = {getAndroidMicrophoneSettings: async () => null, [peersMethod]:async () => [{nodeId:'pc',directions:['speaker']}], [stateMethod]:() => ++calls === 1 ? oldRead : Promise.resolve(newState)};
    const model = new Function('props','ref','computed',script + `\nreturn {refresh, state:()=>${expression}, error:()=>${panel === 'DisplayUsePanel' ? 'pollError' : 'readError'}.value};`)(
      {api}, value => ({value}), getter => ({get value(){return getter();}}));
    const pending = model.refresh();
    await model.refresh(true);
    assert.deepEqual(model.state(),newState);
    if (staleFailure) failOld(new Error('old read failed')); else finishOld(oldState);
    await pending;
    assert.deepEqual(model.state(),newState);
    assert.equal(model.error(),'');
  });
}
