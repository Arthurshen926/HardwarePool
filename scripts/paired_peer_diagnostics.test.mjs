import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { fileURLToPath, pathToFileURL } from 'node:url';

const require = createRequire(new URL('../apps/desktop/package.json', import.meta.url));
const { createServer } = await import(pathToFileURL(require.resolve('vite')).href);
const { createSSRApp } = require('vue');
const { renderToString } = require('vue/server-renderer');

test('pairing diagnostics show comparable UTC invitation times without inventing a legacy timestamp', async () => {
  const server = await createServer({ root: fileURLToPath(new URL('../apps/desktop', import.meta.url)), server: { middlewareMode: true, hmr: false }, appType: 'custom' });
  try {
    const { default: Diagnostics } = await server.ssrLoadModule('/src/components/PairedPeerDiagnostics.vue');
    const peer = { nodeId: '00000000-0000-4000-8000-000000000001', displayName: 'Tablet', revoked: false };
    const render = (value) => renderToString(createSSRApp(Diagnostics, { peer: value }));
    const timed = await render({ ...peer, pairedAtMs: Date.UTC(2026, 8, 19, 8, 4, 17, 492) });
    assert.match(timed, /datetime="2026-09-19T08:04:17.492Z"/);
    assert.match(timed, /2026-09-19 08:04:17.492 UTC/);
    assert.match(timed, /导入同一份新邀请/);
    assert.match(timed, /仍需通过证书校验/);
    const legacy = await render(peer);
    assert.match(legacy, /旧版未提供/);
    assert.doesNotMatch(legacy, /<time|1970/);
    assert.doesNotMatch(await render(undefined), /配对时间/);
  } finally {
    await server.close();
  }
});
