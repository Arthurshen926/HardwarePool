import { test } from 'node:test';
import assert from 'node:assert/strict';
import { parseDisplaySenderState, displaySenderCanStart, displaySenderCanStop } from '../apps/desktop/src/lib/display-control.ts';

test('each execution state has only its independent allowed display actions', () => {
  for (const [state, start, stop] of [['unavailable', false, false], ['stopped', true, false], ['starting', false, true], ['running', false, true], ['stopping', false, false], ['failed', true, false]]) {
    const value = parseDisplaySenderState({ schemaVersion: 1, available: state !== 'unavailable', state, generation: 1, problemCode: null });
    assert.equal(displaySenderCanStart(value), start);
    assert.equal(displaySenderCanStop(value), stop);
  }
  assert.equal(parseDisplaySenderState(null), null);
  assert.equal(displaySenderCanStart(null), false);
  assert.equal(displaySenderCanStop(null), false);
});

test('malformed or misleading display status fails closed', () => {
  const valid = { schemaVersion: 1, available: true, state: 'running', generation: 1, problemCode: null };
  for (const patch of [{ schemaVersion: 2 }, { state: 'streaming' }, { available: false }, { generation: -1 }, { generation: Number.MAX_SAFE_INTEGER + 1 }, { problemCode: 'C:\\private\\config.json' }, { problemCode: 'CAPY.DISPLAY.' + 'X'.repeat(200) }]) {
    assert.throws(() => parseDisplaySenderState({ ...valid, ...patch }));
  }
  assert.equal(parseDisplaySenderState({ ...valid, state: 'failed', problemCode: 'CAPY.DISPLAY.HOST_FAILED' }).state, 'failed');
});
