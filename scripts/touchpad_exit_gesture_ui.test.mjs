import assert from 'node:assert/strict';
import test from 'node:test';

import { fiveFingerExitHoldState, TOUCHPAD_EXIT_HOLD_MS } from '../apps/desktop/src/lib/touchpadExitGesture.ts';

const contacts = [0, 1, 2, 3, 4].map(contactId => ({ contactId, x: 1000 + contactId * 500, y: 2000, pressure: 500 }));

test('five stationary fingers arm the deliberate exit hold', () => {
  assert.equal(TOUCHPAD_EXIT_HOLD_MS, 2000);
  assert.equal(fiveFingerExitHoldState(contacts, null), 'arm');
  assert.equal(fiveFingerExitHoldState(contacts, contacts), 'hold');
  assert.equal(fiveFingerExitHoldState(contacts.map(contact => ({ ...contact, x: contact.x + 100 })), contacts), 'hold');
});

test('four fingers, replacement fingers, or moving five fingers cannot complete the hold', () => {
  assert.equal(fiveFingerExitHoldState(contacts.slice(0, 4), null), 'idle');
  assert.equal(fiveFingerExitHoldState(contacts.map((contact, index) => index === 4 ? { ...contact, contactId: 8 } : contact), contacts), 'cancel');
  assert.equal(fiveFingerExitHoldState(contacts.map(contact => ({ ...contact, x: contact.x + 151 })), contacts), 'cancel');
});
