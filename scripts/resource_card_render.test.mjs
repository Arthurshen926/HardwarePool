import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { fileURLToPath, pathToFileURL } from 'node:url';

const require = createRequire(new URL('../apps/desktop/package.json', import.meta.url));
const { createServer } = await import(pathToFileURL(require.resolve('vite')).href);
const { createSSRApp } = require('vue');
const { renderToString } = require('vue/server-renderer');

test('resource cards retain independent publication control and hide operations for unbound inventory', async () => {
  const server = await createServer({ root: fileURLToPath(new URL('../apps/desktop', import.meta.url)), server: { middlewareMode: true, hmr: false }, appType: 'custom' });
  try {
    const { default: Catalog } = await server.ssrLoadModule('/src/components/CapabilityCatalog.vue');
    const capability = { id: 'self:android.microphone', nodeId: 'self', nodeName: 'Tablet', local: true, simulated: false, displayName: 'Phone microphone', capabilityClass: 'microphone', availability: 'available', permissionRequirement: 'user_confirmation', selectableForSharing: true, selectedForSharing: false, ports: [{ id: 'mic-port', direction: 'source', displayName: 'Mic', profile: 'audio', formats: [] }] };
    const action = { id: 'capyio.quick-action.android-microphone', kind: 'capability_control', simulated: false, status: 'idle', title: 'Phone microphone', summary: 'Capture' };
    const props = { catalogId: 'test', eyebrow: 'Local', title: 'Resources', description: 'Test', capabilities: [capability], busyId: null };
    const html = await renderToString(createSSRApp(Catalog, { ...props, actionBindings: { [capability.id]: action } }));
    assert.match(html, /麦克风：打开使用/);
    assert.match(html, /type="checkbox"/);
    assert.match(html, /选择共享此资源/);
    assert.match(html, /未启动/);
    assert.doesNotMatch(html, /<details open/);
    const active = await renderToString(createSSRApp(Catalog, { ...props, actionBindings: { [capability.id]: { ...action, status: 'active' } } }));
    assert.match(active, /麦克风：管理使用/);
    assert.match(html, /aria-label="Phone microphone：选择共享此资源"/);
    const unbound = await renderToString(createSSRApp(Catalog, props));
    assert.doesNotMatch(unbound, /配置与使用/);
    assert.match(unbound, /Phone microphone：查看与设置/);
    assert.match(unbound, /查看与设置/);
    assert.match(unbound, /Phone microphone/);
    const selected = await renderToString(createSSRApp(Catalog, { ...props, capabilities: [{ ...capability, selectedForSharing: true }], busyId: capability.id }));
    assert.match(selected, /checked disabled/);
    assert.match(selected, /已选为共享资源/);
    const remoteTouchpad = { ...capability, id: 'remote-touchpad', local: false, selectableForSharing: false,
      capabilityClass: 'touchpad', displayName: '平板触控板',
      ports: [{ id: 'touchpad-port', direction: 'source', displayName: 'Touchpad', profile: 'capyio.input.touchpad-frames/1', formats: ['touchpad-frame-v1'] }] };
    const remoteCamera = { ...capability, id: 'remote-camera', local: false, selectableForSharing: false,
      capabilityClass: 'camera', displayName: '平板摄像头' };
    const remoteHtml = await renderToString(createSSRApp(Catalog, { ...props, capabilities: [remoteTouchpad, remoteCamera] }));
    assert.match(remoteHtml, /触控板可申请/);
    assert.match(remoteHtml, /需通过实验功能入口配置/);
    assert.match(remoteHtml, /尚未接入统一连接的一键使用/);
    assert.match(remoteHtml, /平板摄像头：选择使用/);
    assert.doesNotMatch(remoteHtml, /可从远端使用/);
    const { default: AudioConnection } = await server.ssrLoadModule('/src/components/AndroidAudioConnection.vue');
    const audioState = { microphone: { state: 'stopped' }, speaker: { state: 'active' } };
    const audioHtml = await renderToString(createSSRApp(AudioConnection, { api: {}, state: audioState, suggestedHost: '100.64.0.9', initialDirection: 'speaker', expanded: true }));
    assert.match(audioHtml, /<details[^>]* open/);
    assert.match(audioHtml, /<option value="speaker"[^>]* selected>/);
    assert.match(audioHtml, /value="40000"/);
    assert.match(audioHtml, /value="100.64.0.9"/);
    assert.match(audioHtml, /请先停止当前选择的音频功能/);
    assert.match(audioHtml, /尚未读取配置/);
  } finally {
    await server.close();
  }
});
