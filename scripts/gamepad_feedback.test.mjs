import assert from 'node:assert/strict';
import test from 'node:test';
import { GAMEPAD_FEEDBACK_SOURCE, ownedGamepadFeedback } from '../apps/desktop/src/lib/gamepad-feedback.ts';

const route = (id, state, extra = {}) => ({
  id, state, sourceCapabilityId: GAMEPAD_FEEDBACK_SOURCE,
  sinkCapabilityId: 'tablet', profile: 'capyio.haptics.feedback/1',
  authorization: 'authorized', ...extra,
});

test('recreated UI derives feedback ownership from current runtime, including pending consent', () => {
  const persistedSnapshot = JSON.parse(JSON.stringify([
    route('running', 'active'), route('pending', 'draft', { authorization: 'pending' }),
    route('prepared', 'prepared'), route('done', 'stopped'), route('lost', 'offline'),
    route('revoked', 'draft', { authorization: 'revoked' }),
  ]));
  assert.deepEqual(ownedGamepadFeedback(persistedSnapshot).map(r => r.id), ['running', 'pending', 'prepared']);
});

test('stop never adopts another actuator source or another profile', () => {
  assert.deepEqual(ownedGamepadFeedback([
    route('foreign', 'active', { sourceCapabilityId: 'another-source' }),
    route('touchpad', 'active', { profile: 'capyio.input.touchpad-frames/1' }),
    route('absent', 'active', { sourceCapabilityId: null }),
  ]), []);
});
