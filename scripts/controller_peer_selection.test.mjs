import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { stripTypeScriptTypes } from 'node:module';
const text = readFileSync(new URL('../apps/desktop/src/components/ControllerUsePanel.vue', import.meta.url), 'utf8');
const script = text.slice(text.indexOf('const peer ='), text.indexOf('const labels ='));
function selection(props) {
  return new Function('props', 'ref', 'computed', 'watch', stripTypeScriptTypes(script) + '\nreturn {peer:peer.value,mode:mode.value};')(
    props, value => ({ value }), fn => ({ get value() { return fn(); } }), (_, fn) => fn());
}
test('card node is selected among multiple connected peers', () => {
  assert.equal(selection({ nodes: [{id:'a'}, {id:'b'}], preferredPeer:'b', state:null }).peer, 'b');
});
test('missing card node does not silently select another node', () => {
  assert.equal(selection({ nodes:[{id:'a'}], preferredPeer:'b', state:null }).peer, '');
});
test('an active session retains its actual peer and mode', () => {
  assert.deepEqual(selection({ nodes:[{id:'a'}, {id:'b'}], preferredPeer:'b', state:{status:'active',peerNodeId:'a',mode:'dsu'} }), {peer:'a',mode:'dsu'});
});
test('preferred card mode initializes the idle form but never overrides an active mode', () => {
  assert.equal(selection({nodes:[{id:'a'}],state:null,preferredMode:'xbox360'}).mode, 'xbox360');
  assert.equal(selection({nodes:[{id:'a'}],preferredMode:'xbox360',state:{status:'active',peerNodeId:'a',mode:'dsu'}}).mode, 'dsu');
});
