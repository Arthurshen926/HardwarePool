// Explicitly targeted Android transport fault gate. No Windows input is injected.
import assert from 'node:assert/strict';
import net from 'node:net';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import { validateDiscoveryUrl } from './android_webview_target.mjs';

const [adb, serial, discovery] = process.argv.slice(2);
if (!adb || !serial || !discovery) throw new Error('Usage: node scripts/touchpad_transport_device_gate.mjs <adb-path> <explicit-serial> <forwarded-json-list-url>');
validateDiscoveryUrl(discovery);
const execute = promisify(execFile);
const runAdb = (...args) => execute(adb, ['-s', serial, ...args], { timeout: 15000, maxBuffer: 65536, windowsHide: true });
const evaluate = async expression => JSON.parse((await execute(process.execPath,
  [fileURLToPath(new URL('./android_webview_cdp_eval.mjs', import.meta.url)), discovery, expression],
  { timeout: 12000, maxBuffer: 65536, windowsHide: true })).stdout);
const id = 'capyio.quick-action.android-touchpad';
const state = () => evaluate(`JSON.parse(CapyIOAndroid.getQuickActionsJson()).find(x=>x.id===${JSON.stringify(id)})`);
const command = operation => evaluate(`CapyIOAndroid.invokeQuickAction(${JSON.stringify(id)},${JSON.stringify(operation)})`);
const until = async (check, description) => {
  const deadline = Date.now() + 2000;
  while (Date.now() < deadline) { if (await check()) return; await delay(30); }
  throw new Error(`Timed out: ${description}`);
};
const initial = await state();
assert.ok(['idle', 'failed'].includes(initial.status), 'Refuse to interrupt an existing touchpad session');
const mappings = (await runAdb('reverse', '--list')).stdout;
assert.ok(!/\btcp:61000\b/.test(mappings), 'Refuse to overwrite an existing reverse mapping');
const sockets = new Set();
let received = 0;
let closed = 0;
let echoAcknowledgements = false;
let heldFrames = 0;
const server = net.createServer(socket => {
  sockets.add(socket);
  let pending = Buffer.alloc(0);
  socket.on('data', data => {
    received += data.length;
    if (!echoAcknowledgements) return; // Deliberately withhold ACK during cancellation cases.
    pending = Buffer.concat([pending, data]);
    if (pending.length > 4096) { socket.destroy(); return; }
    while (pending.length >= 24) {
      const kind = pending[5];
      if (![1, 2, 4].includes(kind)) { socket.destroy(); return; }
      if (kind === 2 && pending.length < 56) return;
      const length = kind === 1 ? 160 : kind === 2 ? 56 + pending[31] * 24 : 24;
      if (pending.length < length) return;
      if (kind === 2) {
        if (pending[31] > 0) heldFrames++;
        const ack = Buffer.from(pending.subarray(0, 24));
        ack[5] = 3;
        socket.write(ack);
      }
      pending = pending.subarray(length);
    }
  });
  socket.on('error', () => {});
  socket.on('close', () => { closed++; sockets.delete(socket); });
});
let ownsReverse = false;
try {
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(61000, '127.0.0.1', resolve); });
  await runAdb('reverse', 'tcp:61000', 'tcp:61000');
  ownsReverse = true;
  for (let cycle = 0; cycle < 3; cycle++) {
    const before = received;
    const current = await state();
    assert.equal(await command(current.status === 'failed' ? 'retry' : 'start'), true);
    await until(() => received - before >= 184, 'Hello and initial Data received');
    assert.equal((await state()).status, 'starting', 'No false Active before ACK');
    const startedAt = Date.now();
    assert.equal(await command('stop'), true);
    await until(async () => (await state()).status === 'idle', 'Stop completes despite withheld ACK');
    await until(() => closed === cycle + 1 && sockets.size === 0, 'peer socket closes');
    console.log(JSON.stringify({ cycle: cycle + 1, stoppedWithinMs: Date.now() - startedAt,
      ackWithheld: true, peerClosed: true, inputInjected: false }));
  }
  echoAcknowledgements = true;
  assert.equal(await command('start'), true);
  await until(async () => (await state()).status === 'active', 'ACK admits sender Active');
  assert.equal(await evaluate('CapyIOAndroid.updateTouchpad("1,1000,1000,500")'), true);
  await until(() => heldFrames >= 3, 'stationary contact sends periodic complete snapshots');
  assert.equal(await command('stop'), true);
  await until(async () => (await state()).status === 'idle', 'active contact Stop completes');
  await until(() => closed === 4 && sockets.size === 0, 'active sender socket closes');
  console.log(JSON.stringify({ stationaryHoldFrames: heldFrames, activeStop: true, inputInjected: false }));
} finally {
  try {
    // Stop only a run that this gate could have started; never touch other capabilities.
    if (ownsReverse) {
      const current = await state().catch(() => null);
      if (current?.availableOperations.includes('stop')) await command('stop');
    }
  } finally {
    for (const socket of sockets) socket.destroy();
    await new Promise(resolve => server.close(resolve));
    if (ownsReverse) await runAdb('reverse', '--remove', 'tcp:61000');
  }
}
