import type { TouchpadControlUpdate } from "./types";

export const MAX_PENDING_TOUCHPAD_UPDATES = 8;

function sameContacts(left: TouchpadControlUpdate, right: TouchpadControlUpdate): boolean {
  return left.contacts.length === right.contacts.length
    && left.contacts.every((contact, index) => contact.contactId === right.contacts[index]?.contactId);
}

/** Complete snapshots: motion may coalesce, but the newest contact set must survive pressure. */
export function enqueueTouchpadUpdate(queue: TouchpadControlUpdate[], update: TouchpadControlUpdate): void {
  const stable = {
    contacts: [...update.contacts]
      .sort((left, right) => left.contactId - right.contactId)
      .map(contact => ({ ...contact })),
  };
  const tail = queue.at(-1);
  if (tail && sameContacts(tail, stable)) {
    queue[queue.length - 1] = stable;
  } else if (queue.length < MAX_PENDING_TOUCHPAD_UPDATES) {
    queue.push(stable);
  } else if (stable.contacts.length === 0) {
    queue.splice(0, queue.length, stable);
  } else {
    // The worker may already have submitted an older topology. Reset it before
    // rebuilding the latest snapshot; never silently discard a new finger.
    queue.splice(0, queue.length, { contacts: [] }, stable);
  }
}
