import assert from 'node:assert/strict';
import test from 'node:test';

import { enqueueTouchpadUpdate, MAX_PENDING_TOUCHPAD_UPDATES } from '../apps/desktop/src/lib/touchpadQueue.ts';

const frame = (...ids) => ({ contacts: ids.map((contactId) => ({ contactId, x: contactId, y: 0, pressure: 1 })) });

test('motion coalesces without erasing contact-set transitions', () => {
  const queue = [];
  enqueueTouchpadUpdate(queue, frame(0));
  enqueueTouchpadUpdate(queue, { contacts: [{ contactId: 0, x: 99, y: 0, pressure: 1 }] });
  enqueueTouchpadUpdate(queue, frame(0, 1));
  assert.deepEqual(queue.map(item => item.contacts.map(contact => contact.contactId)), [[0], [0, 1]]);
  assert.equal(queue[0].contacts[0].x, 99);
});

test('burst pressure preserves the newest multi-finger topology after a reset', () => {
  const queue = [];
  for (let index = 0; index < MAX_PENDING_TOUCHPAD_UPDATES; index += 1) {
    enqueueTouchpadUpdate(queue, frame(index % 2));
  }
  enqueueTouchpadUpdate(queue, frame(0, 1, 2, 3));
  assert.deepEqual(queue.map(item => item.contacts.map(contact => contact.contactId)), [[], [0, 1, 2, 3]]);
  enqueueTouchpadUpdate(queue, frame());
  assert.deepEqual(queue.map(item => item.contacts.map(contact => contact.contactId)), [[], [0, 1, 2, 3], []]);
});

test('a release wins when the bounded queue is full', () => {
  const queue = [];
  for (let index = 0; index < MAX_PENDING_TOUCHPAD_UPDATES; index += 1) {
    enqueueTouchpadUpdate(queue, frame(index % 2));
  }
  enqueueTouchpadUpdate(queue, frame());
  assert.deepEqual(queue, [frame()]);
});
