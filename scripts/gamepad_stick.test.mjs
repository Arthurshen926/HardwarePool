import assert from 'node:assert/strict';
import test from 'node:test';
import { stickAxes } from '../apps/desktop/src/lib/gamepad-stick.ts';

test('floating landing is neutral, with radial deadzone and bounded axes', () => {
  const center = { x: 120, y: 80, radius: 50 };
  assert.deepEqual(stickAxes(center, 120, 80, false), { x: 0, y: 0 });
  assert.deepEqual(stickAxes(center, 123, 80, false), { x: 0, y: 0 });
  assert.deepEqual(stickAxes(center, 220, 80, false), { x: 32767, y: 0 });
  const diagonal = stickAxes(center, 220, -20, false);
  assert.ok(Math.abs(Math.hypot(diagonal.x, diagonal.y) - 32767) < 1);
  assert.equal(center.x, 120);
});

test('follow moves the base only outside travel radius; reversing returns neutral', () => {
  const center = { x: 100, y: 100, radius: 50 };
  assert.equal(stickAxes(center, 180, 100, true).x, 32767);
  assert.equal(center.x, 130);
  assert.deepEqual(stickAxes(center, 130, 100, true), { x: 0, y: 0 });
  const other = { x: 400, y: 100, radius: 50 };
  stickAxes(other, 400, 180, true);
  assert.equal(center.x, 130);
  assert.equal(center.y, 100);
});
