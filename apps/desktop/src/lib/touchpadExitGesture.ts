import type { UiTouchpadContact } from "./types";

export const TOUCHPAD_EXIT_HOLD_MS = 2000;
const MAX_STATIONARY_TRAVEL = 150; // normalized 0..10,000 surface coordinates

export function fiveFingerExitHoldState(
  contacts: UiTouchpadContact[],
  origin: UiTouchpadContact[] | null,
): "idle" | "arm" | "hold" | "cancel" {
  if (contacts.length !== 5) return "idle";
  if (origin === null) return "arm";
  if (origin.length !== 5) return "cancel";
  return contacts.every((contact, index) => {
    const first = origin[index];
    return first?.contactId === contact.contactId
      && Math.hypot(contact.x - first.x, contact.y - first.y) <= MAX_STATIONARY_TRAVEL;
  }) ? "hold" : "cancel";
}
