<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from "vue";

import type { TouchpadControlUpdate, UiTouchpadContact, UiTouchpadState } from "../lib/types";

const props = defineProps<{ state: UiTouchpadState; disabled: boolean }>();
const emit = defineEmits<{ update: [value: TouchpadControlUpdate] }>();

const visibleContacts = ref<UiTouchpadContact[]>([]);
const pointerSlots = new Map<number, number>();
let pendingFrame: number | null = null;

function begin(event: PointerEvent): void {
  if (props.disabled || pointerSlots.has(event.pointerId) || pointerSlots.size >= 5) return;
  event.preventDefault();
  const element = event.currentTarget as HTMLElement;
  element.setPointerCapture(event.pointerId);
  pointerSlots.set(event.pointerId, nextSlot());
  updateContact(event, true);
}

function move(event: PointerEvent): void {
  if (!pointerSlots.has(event.pointerId)) return;
  event.preventDefault();
  updateContact(event, false);
}

function end(event: PointerEvent): void {
  const slot = pointerSlots.get(event.pointerId);
  if (slot === undefined) return;
  event.preventDefault();
  pointerSlots.delete(event.pointerId);
  visibleContacts.value = visibleContacts.value.filter((contact) => contact.contactId !== slot);
  flushImmediately();
}

function cancelAll(): void {
  if (pointerSlots.size === 0 && visibleContacts.value.length === 0) return;
  pointerSlots.clear();
  visibleContacts.value = [];
  flushImmediately();
}

function clearLocalState(): void {
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
  window.removeEventListener("blur", cancelAll);
  document.removeEventListener("visibilitychange", handleVisibility);
  if (pendingFrame !== null) cancelAnimationFrame(pendingFrame);
  if (!props.disabled) emit("update", { contacts: [] });
});
</script>

<template>
  <div
    class="touchpad-surface"
    :class="{ 'touchpad-surface--disabled': disabled }"
    role="application"
    aria-label="Android 本机多点触控板"
    tabindex="0"
    @pointerdown="begin"
    @pointermove="move"
    @pointerup="end"
    @pointercancel="end"
    @lostpointercapture="end"
    @contextmenu.prevent
    @blur="cancelAll"
  >
    <div class="touchpad-surface__copy">
      <strong>{{ disabled ? "请先启动 Touchpad surface" : "1–5 指触控区域" }}</strong>
      <span>完整触点帧 · sequence {{ state.sequence }}</span>
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
.touchpad-surface__copy { position: absolute; inset: 0; display: grid; place-content: center; gap: 8px; text-align: center; color: #9fb9ad; pointer-events: none; }
.touchpad-surface__copy strong { color: #eefbf5; font-size: 22px; }
.touchpad-surface__copy span { font-size: 12px; letter-spacing: .08em; }
.touchpad-surface i { position: absolute; display: grid; width: 56px; height: 56px; place-items: center; translate: -50% -50%; border-radius: 50%; color: #153126; background: #72e0ae; box-shadow: 0 0 0 9px rgba(114,224,174,.14),0 10px 22px rgba(0,0,0,.28); font-style: normal; font-weight: 900; pointer-events: none; }
@media (max-width: 540px) { .touchpad-surface { min-height: 58vh; border-radius: 20px; }.touchpad-surface__copy strong { font-size: 18px; } }
</style>
