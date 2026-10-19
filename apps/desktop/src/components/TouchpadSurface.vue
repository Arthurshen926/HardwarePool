<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from "vue";

import type { TouchpadControlUpdate, UiTouchpadContact, UiTouchpadState } from "../lib/types";
import { fiveFingerExitHoldState, TOUCHPAD_EXIT_HOLD_MS } from "../lib/touchpadExitGesture";

const props = withDefaults(defineProps<{ state: UiTouchpadState; disabled: boolean; immersive?: boolean }>(), {
  immersive: false,
});
const emit = defineEmits<{ update: [value: TouchpadControlUpdate]; exit: [] }>();

const visibleContacts = ref<UiTouchpadContact[]>([]);
const pointerSlots = new Map<number, number>();
const INITIAL_MOVE_SETTLE_MS = 24;
const ADDED_CONTACT_SETTLE_MS = 72;
let pendingFrame: number | null = null;
let suppressMovesUntil = 0;
let primaryPointerId: number | null = null;
let primaryDownAt = 0;
let primaryDownX = 0;
let primaryDownY = 0;
let priorTapAt = 0;
let priorTapX = 0;
let priorTapY = 0;
let dragFeedbackSent = false;
let primaryMaxContacts = 0;
let exitHoldOrigin: UiTouchpadContact[] | null = null;
let exitHoldTimer: ReturnType<typeof setTimeout> | null = null;
let exitHoldCancelled = false;

function clearExitHold(): void {
  if (exitHoldTimer !== null) clearTimeout(exitHoldTimer);
  exitHoldTimer = null;
  exitHoldOrigin = null;
}

function updateExitHold(): void {
  if (!props.immersive || props.disabled) return;
  const status = fiveFingerExitHoldState(visibleContacts.value, exitHoldOrigin);
  if (status === "idle") {
    clearExitHold();
    exitHoldCancelled = false;
  } else if (status === "cancel") {
    clearExitHold();
    exitHoldCancelled = true;
  } else if (status === "arm" && !exitHoldCancelled) {
    exitHoldOrigin = visibleContacts.value.map(contact => ({ ...contact }));
    exitHoldTimer = setTimeout(() => {
      if (fiveFingerExitHoldState(visibleContacts.value, exitHoldOrigin) !== "hold") return;
      cancelAll();
      emit("exit");
    }, TOUCHPAD_EXIT_HOLD_MS);
  }
}

function begin(event: PointerEvent): void {
  if (props.disabled || pointerSlots.has(event.pointerId) || pointerSlots.size >= 5) return;
  event.preventDefault();
  const element = event.currentTarget as HTMLElement;
  element.setPointerCapture(event.pointerId);
  pointerSlots.set(event.pointerId, nextSlot());
  primaryMaxContacts = Math.max(primaryMaxContacts, pointerSlots.size);
  if (pointerSlots.size === 1) {
    suppressMovesUntil = event.timeStamp + INITIAL_MOVE_SETTLE_MS;
    primaryPointerId = event.pointerId;
    primaryDownAt = event.timeStamp;
    primaryDownX = event.clientX;
    primaryDownY = event.clientY;
    dragFeedbackSent = false;
    primaryMaxContacts = 1;
  } else {
    suppressMovesUntil = Math.max(suppressMovesUntil, event.timeStamp + ADDED_CONTACT_SETTLE_MS);
  }
  updateContact(event, true);
}

function cancelPointer(event: PointerEvent): void {
  const slot = pointerSlots.get(event.pointerId);
  if (slot === undefined) return;
  event.preventDefault();
  pointerSlots.delete(event.pointerId);
  visibleContacts.value = visibleContacts.value.filter((contact) => contact.contactId !== slot);
  updateExitHold();
  if (event.pointerId === primaryPointerId) primaryPointerId = null;
  priorTapAt = 0;
  flushImmediately();
}

function move(event: PointerEvent): void {
  const slot = pointerSlots.get(event.pointerId);
  if (slot === undefined) return;
  event.preventDefault();
  if (exitHoldOrigin !== null) {
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    const contacts = visibleContacts.value.map(contact => contact.contactId === slot
      ? { ...contact, x: normalize(event.clientX - rect.left, rect.width), y: normalize(event.clientY - rect.top, rect.height) }
      : contact);
    if (fiveFingerExitHoldState(contacts, exitHoldOrigin) === "cancel") {
      clearExitHold();
      exitHoldCancelled = true;
    }
  }
  if (event.timeStamp < suppressMovesUntil) return;
  maybeSignalDrag(event);
  updateContact(event, false);
}

function end(event: PointerEvent): void {
  const slot = pointerSlots.get(event.pointerId);
  if (slot === undefined) return;
  event.preventDefault();
  maybeSignalTap(event);
  pointerSlots.delete(event.pointerId);
  visibleContacts.value = visibleContacts.value.filter((contact) => contact.contactId !== slot);
  updateExitHold();
  flushImmediately();
}

function distance(ax: number, ay: number, bx: number, by: number): number {
  return Math.hypot(bx - ax, by - ay);
}

function maybeSignalDrag(event: PointerEvent): void {
  if (event.pointerId !== primaryPointerId || dragFeedbackSent || primaryMaxContacts !== 1) return;
  const isSecondTap = event.timeStamp - priorTapAt <= 500
    && distance(priorTapX, priorTapY, primaryDownX, primaryDownY) <= 64;
  if (isSecondTap && distance(primaryDownX, primaryDownY, event.clientX, event.clientY) > 18) {
    dragFeedbackSent = true;
    priorTapAt = 0;
    window.CapyIOAndroid?.performTouchpadHaptic?.("drag");
  }
}

function maybeSignalTap(event: PointerEvent): void {
  if (event.pointerId !== primaryPointerId) return;
  const isTap = primaryMaxContacts === 1 && !dragFeedbackSent
    && event.timeStamp - primaryDownAt <= 300
    && distance(primaryDownX, primaryDownY, event.clientX, event.clientY) <= 18;
  if (isTap) {
    priorTapAt = event.timeStamp;
    priorTapX = event.clientX;
    priorTapY = event.clientY;
    window.CapyIOAndroid?.performTouchpadHaptic?.("tap");
  } else if (!dragFeedbackSent) {
    priorTapAt = 0;
  }
  primaryPointerId = null;
}

function cancelAll(): void {
  clearExitHold();
  exitHoldCancelled = false;
  if (pointerSlots.size === 0 && visibleContacts.value.length === 0) return;
  pointerSlots.clear();
  visibleContacts.value = [];
  primaryPointerId = null;
  priorTapAt = 0;
  flushImmediately();
}

function clearLocalState(): void {
  clearExitHold();
  exitHoldCancelled = false;
  suppressMovesUntil = 0;
  if (pendingFrame !== null) cancelAnimationFrame(pendingFrame);
  pendingFrame = null;
  pointerSlots.clear();
  visibleContacts.value = [];
}

function handleVisibility(): void {
  if (document.hidden) cancelAll();
}

function updateContact(event: PointerEvent, immediate: boolean): void {
  const slot = pointerSlots.get(event.pointerId);
  if (slot === undefined) return;
  const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
  const x = normalize(event.clientX - rect.left, rect.width);
  const y = normalize(event.clientY - rect.top, rect.height);
  const pressure = Math.round(Math.max(0, Math.min(1, event.pressure)) * 1024);
  const next = visibleContacts.value.filter((contact) => contact.contactId !== slot);
  next.push({ contactId: slot, x, y, pressure });
  visibleContacts.value = next.sort((left, right) => left.contactId - right.contactId);
  updateExitHold();
  if (immediate) flushImmediately();
  else scheduleFrame();
}

function normalize(value: number, extent: number): number {
  if (!Number.isFinite(value) || !Number.isFinite(extent) || extent <= 0) return 0;
  return Math.round(Math.max(0, Math.min(1, value / extent)) * 10_000);
}

function nextSlot(): number {
  const used = new Set(pointerSlots.values());
  for (let slot = 0; slot < 5; slot += 1) {
    if (!used.has(slot)) return slot;
  }
  return 0;
}

function scheduleFrame(): void {
  if (pendingFrame !== null) return;
  pendingFrame = requestAnimationFrame(() => {
    pendingFrame = null;
    publish();
  });
}

function flushImmediately(): void {
  if (pendingFrame !== null) cancelAnimationFrame(pendingFrame);
  pendingFrame = null;
  publish();
}

function publish(): void {
  emit("update", { contacts: visibleContacts.value.map((contact) => ({ ...contact })) });
}

function position(contact: UiTouchpadContact): Record<string, string> {
  return { left: `${contact.x / 100}%`, top: `${contact.y / 100}%` };
}

watch(() => props.disabled, (disabled) => {
  if (disabled) clearLocalState();
});

onMounted(() => {
  window.addEventListener("blur", cancelAll);
  document.addEventListener("visibilitychange", handleVisibility);
});

onUnmounted(() => {
  clearExitHold();
  window.removeEventListener("blur", cancelAll);
  document.removeEventListener("visibilitychange", handleVisibility);
  if (pendingFrame !== null) cancelAnimationFrame(pendingFrame);
  if (!props.disabled) emit("update", { contacts: [] });
});
</script>

<template>
  <div
    class="touchpad-surface"
    :class="{ 'touchpad-surface--disabled': disabled, 'touchpad-surface--immersive': immersive }"
    role="application"
    aria-label="Android 本机多点触控板"
    tabindex="0"
    @pointerdown="begin"
    @pointermove="move"
    @pointerup="end"
    @pointercancel="cancelPointer"
    @lostpointercapture="cancelPointer"
    @contextmenu.prevent
    @blur="cancelAll"
  >
    <div class="touchpad-surface__copy">
      <strong>{{ disabled ? "请先启动 Touchpad surface" : "1–5 指触控区域" }}</strong>
      <span>{{ immersive ? "五指静止长按 2 秒退出并停止使用" : `完整触点帧 · sequence ${state.sequence}` }}</span>
    </div>
    <i
      v-for="contact in visibleContacts"
      :key="contact.contactId"
      :style="position(contact)"
      :aria-label="`触点 ${contact.contactId + 1}`"
    >{{ contact.contactId + 1 }}</i>
  </div>
</template>

<style scoped>
.touchpad-surface { position: relative; min-height: 430px; overflow: hidden; touch-action: none; user-select: none; border: 1px solid rgba(121,227,179,.28); border-radius: 28px; background: radial-gradient(circle at 50% 35%, rgba(58,111,88,.44), transparent 45%), linear-gradient(145deg,#172a22,#0d1914); box-shadow: inset 0 1px rgba(255,255,255,.08),0 24px 50px rgba(13,34,25,.22); }
.touchpad-surface--disabled { opacity: .55; pointer-events: none; }
.touchpad-surface--immersive { position: fixed; inset: 0; z-index: 1000; width: 100dvw; height: 100dvh; min-height: 100dvh; border: 0; border-radius: 0; }
.touchpad-surface__copy { position: absolute; inset: 0; display: grid; place-content: center; gap: 8px; text-align: center; color: #9fb9ad; pointer-events: none; }
.touchpad-surface__copy strong { color: #eefbf5; font-size: 22px; }
.touchpad-surface__copy span { font-size: 12px; letter-spacing: .08em; }
.touchpad-surface i { position: absolute; display: grid; width: 56px; height: 56px; place-items: center; translate: -50% -50%; border-radius: 50%; color: #153126; background: #72e0ae; box-shadow: 0 0 0 9px rgba(114,224,174,.14),0 10px 22px rgba(0,0,0,.28); font-style: normal; font-weight: 900; pointer-events: none; }
@media (max-width: 540px) { .touchpad-surface { min-height: 58vh; border-radius: 20px; }.touchpad-surface__copy strong { font-size: 18px; } }
</style>
