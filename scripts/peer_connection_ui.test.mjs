import { test } from 'node:test';
import assert from 'node:assert/strict';
import { requiresConnectionAttention } from '../apps/desktop/src/lib/peer-connection.ts';

test('automatic retry pauses for credentials but not temporary network failures', () => {
  for (const detail of ['invalid peer certificate: BadSignature', 'Windows DPAPI unprotection failed',
    'credential could not be loaded', 'pairing directory refresh failed', 'peer revoked']) {
    assert.equal(requiresConnectionAttention(new Error(detail)), true);
  }
  for (const detail of ['connection refused', 'os error 10060', 'connection timed out', 'catalog receive failed']) {
    assert.equal(requiresConnectionAttention(detail), false);
  }
});
import { parsePairedPeers, controlConnectionError } from '../apps/desktop/src/lib/peer-connection.ts';

const peer = { nodeId: '00000000-0000-4000-8000-000000000001', displayName: 'Tablet', revoked: false };

test('paired metadata remains usable without multicast and exposes no credential fields', () => {
  assert.deepEqual(parsePairedPeers([]), []);
  assert.deepEqual(parsePairedPeers([peer]), [peer]);
  assert.deepEqual(parsePairedPeers([{ ...peer, revoked: true }]), [{ ...peer, revoked: true }]);
  const timedPeer = { ...peer, pairedAtMs: 1_789_809_857_492 };
  assert.deepEqual(parsePairedPeers([timedPeer]), [timedPeer]);
  assert.throws(() => parsePairedPeers([{ ...peer, credentialHandle: 'secret' }]));
  for (const extra of ['credentialHandle', 'privateKey', 'peerCertificateSha256', 'serverName']) {
    assert.throws(() => parsePairedPeers([{ ...timedPeer, [extra]: 'not-for-ui' }]));
  }
});

test('optional pairing timestamp is a bounded integer and old hosts remain compatible', () => {
  assert.deepEqual(parsePairedPeers([peer]), [peer]);
  assert.equal(parsePairedPeers([{ ...peer, pairedAtMs: 0 }])[0].pairedAtMs, 0);
  for (const pairedAtMs of [undefined, null, -1, 1.5, '1789809857492', NaN, Infinity, 8_640_000_000_000_001, Number.MAX_SAFE_INTEGER + 1]) {
    assert.throws(() => parsePairedPeers([{ ...peer, pairedAtMs }]));
  }
});

test('invalid bounded paired inventory fails closed', () => {
  for (const value of [null, {}, [null], [peer, peer], Array(65).fill(peer)]) assert.throws(() => parsePairedPeers(value));
  for (const patch of [{ nodeId: '' }, { nodeId: '00000000-0000-0000-0000-000000000000' },
    { displayName: '' }, { displayName: 'x'.repeat(129) }, { displayName: '\nTablet' }, { revoked: 'false' }]) {
    assert.throws(() => parsePairedPeers([{ ...peer, ...patch }]));
  }
});

test('connection errors distinguish credentials, trust, endpoint and transport without hiding evidence', () => {
  for (const [reason, expected] of [
    ['Windows DPAPI unprotection failed', '同一用户'],
    ['manual endpoint belongs to an unpaired Node', '尚未完成配对'],
    ['manual endpoint belongs to a revoked Node', '吊销'],
    ['manual control endpoint is invalid', '无线调试端口'],
    ['authenticated control connection failed: refused', '不是 ADB 端口'],
    ['authenticated control connection failed: invalid peer certificate: BadSignature', 'Windows 配对身份诊断'],
    ['pairing directory refresh failed: malformed JSON', '未使用旧凭据'],
    ['peer already has an authenticated control connection', '已经连接'],
    ['unexpected error', '下方原因'],
  ]) {
    const message = controlConnectionError(new Error(reason));
    assert.ok(message.includes(expected), message);
    assert.ok(message.includes(reason), message);
  }
});

test('certificate mismatch guidance does not equate matching SID/path with matching trust', () => {
  const message = controlConnectionError(new Error('invalid peer certificate: BadSignature'));
  for (const hint of ['对照两端显示的配对时间', '不同进程读取了不同文件', '正常启动的 Windows 窗口', '不要复制 Windows 私钥']) {
    assert.ok(message.includes(hint), message);
  }
});

test('first trust points to PIN while network failures do not request re-pairing', () => {
  assert.match(controlConnectionError('unpaired Node'), /连接一台新设备.*PIN/);
  assert.match(controlConnectionError('os error 10061'), /端口拒绝连接/);
  assert.match(controlConnectionError('os error 10060'), /先排查网络/);
  assert.match(controlConnectionError('invalid peer certificate: BadSignature; timeout'), /证书与当前窗口/);
});
