import { test } from 'node:test';
import assert from 'node:assert/strict';
import { automaticConnectionStatus } from '../apps/desktop/src/lib/automatic-connection-status.ts';

const base = { enabled: true, listening: false, connected: false, paired: true, hasEndpoint: false, hasInterface: false };
test('automatic connection exposes each waiting condition without granting trust', () => {
  for (const [patch, expected] of [
    [{ enabled: false, connected: true }, '自动连接已关闭'],
    [{ listening: true, hasEndpoint: true }, '暂不自动拨号'],
    [{ connected: true }, '硬件仍需单独申请'],
    [{ paired: false, hasEndpoint: true }, '尚无有效配对'],
    [{ hasEndpoint: true }, '不依赖组播发现'],
    [{ hasInterface: true }, '等待发现'],
    [{}, '目前没有可用于自动连接的地址'],
  ]) assert.ok(automaticConnectionStatus({ ...base, ...patch }).includes(expected));
});
test('status presentation leaves settings unchanged', () => {
  const state = Object.freeze({ ...base });
  automaticConnectionStatus(state);
  assert.deepEqual(state, base);
});
