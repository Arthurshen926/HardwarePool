<script setup lang="ts">
import { onUnmounted } from "vue";

import type { GamepadButton, GamepadControlUpdate, UiGamepadState } from "../lib/types";

const props = defineProps<{ state: UiGamepadState; disabled?: boolean }>();
const emit = defineEmits<{ update: [value: GamepadControlUpdate] }>();

type StickName = "left" | "right";
type TriggerName = "left" | "right";

const buttonPointers = new Map<GamepadButton, Set<number>>();
const dpadPointers = new Map<number, { x: number; y: number }>();
const stickOwners = new Map<StickName, number>();
const triggerOwners = new Map<TriggerName, number>();
const pendingStick = new Map<StickName, { x: number; y: number }>();
const stickFrames = new Map<StickName, number>();

function beginButton(button: GamepadButton, event: PointerEvent): void {
  if (props.disabled) return;
  event.preventDefault();
  capture(event);
  const pointers = buttonPointers.get(button) ?? new Set<number>();
  const wasIdle = pointers.size === 0;
  pointers.add(event.pointerId);
  buttonPointers.set(button, pointers);
  if (wasIdle) emit("update", { kind: "button", button, pressed: true });
}

function endButton(button: GamepadButton, event: PointerEvent): void {
  const pointers = buttonPointers.get(button);
  if (!pointers?.delete(event.pointerId)) return;
  if (pointers.size === 0) {
    buttonPointers.delete(button);
    emit("update", { kind: "button", button, pressed: false });
  }
}

function keyboardButton(button: GamepadButton, pressed: boolean, event: KeyboardEvent): void {
  if (props.disabled || (event.key !== " " && event.key !== "Enter")) return;
  if (event.repeat) return;
  event.preventDefault();
  emit("update", { kind: "button", button, pressed });
}

function beginDpad(x: number, y: number, event: PointerEvent): void {
  if (props.disabled) return;
  event.preventDefault();
  capture(event);
  dpadPointers.set(event.pointerId, { x, y });
  publishDpad();
}

function endDpad(event: PointerEvent): void {
  if (!dpadPointers.delete(event.pointerId)) return;
  publishDpad();
}

function keyboardDpad(x: number, y: number, pressed: boolean, event: KeyboardEvent): void {
  if (props.disabled || (event.key !== " " && event.key !== "Enter")) return;
  if (event.repeat) return;
  event.preventDefault();
  emit("update", { kind: "dpad", x: pressed ? x : 0, y: pressed ? y : 0 });
}

function publishDpad(): void {
  let x = 0;
  let y = 0;
  for (const value of dpadPointers.values()) {
    x = Math.max(-1, Math.min(1, x + value.x));
    y = Math.max(-1, Math.min(1, y + value.y));
  }
  emit("update", { kind: "dpad", x, y });
}

function beginStick(stick: StickName, event: PointerEvent): void {
  if (props.disabled || stickOwners.has(stick)) return;
  event.preventDefault();
  capture(event);
  stickOwners.set(stick, event.pointerId);
  updateStick(stick, event);
}

function moveStick(stick: StickName, event: PointerEvent): void {
  if (stickOwners.get(stick) !== event.pointerId) return;
  event.preventDefault();
  updateStick(stick, event);
}

function endStick(stick: StickName, event: PointerEvent): void {
  if (stickOwners.get(stick) !== event.pointerId) return;
  stickOwners.delete(stick);
  const frame = stickFrames.get(stick);
  if (frame !== undefined) cancelAnimationFrame(frame);
  stickFrames.delete(stick);
  pendingStick.delete(stick);
  emit("update", { kind: "stick", stick, x: 0, y: 0 });
}

function updateStick(stick: StickName, event: PointerEvent): void {
  const element = event.currentTarget as HTMLElement;
  const rect = element.getBoundingClientRect();
  const rawX = (event.clientX - (rect.left + rect.width / 2)) / (rect.width / 2);
  const rawY = -((event.clientY - (rect.top + rect.height / 2)) / (rect.height / 2));
  const magnitude = Math.hypot(rawX, rawY);
  const deadzone = 0.08;
  let normalizedX = 0;
  let normalizedY = 0;
  if (magnitude > deadzone) {
    const clipped = Math.min(1, magnitude);
    const scaled = (clipped - deadzone) / (1 - deadzone);
    normalizedX = (rawX / magnitude) * scaled;
    normalizedY = (rawY / magnitude) * scaled;
  }
  pendingStick.set(stick, {
    x: Math.round(normalizedX * 32767),
    y: Math.round(normalizedY * 32767),
  });
  if (stickFrames.has(stick)) return;
  stickFrames.set(stick, requestAnimationFrame(() => {
    stickFrames.delete(stick);
    const pending = pendingStick.get(stick);
    if (pending) emit("update", { kind: "stick", stick, ...pending });
  }));
}

function beginTrigger(trigger: TriggerName, event: PointerEvent): void {
  if (props.disabled || triggerOwners.has(trigger)) return;
  event.preventDefault();
  capture(event);
  triggerOwners.set(trigger, event.pointerId);
  updateTrigger(trigger, event);
}

function moveTrigger(trigger: TriggerName, event: PointerEvent): void {
  if (triggerOwners.get(trigger) !== event.pointerId) return;
  event.preventDefault();
  updateTrigger(trigger, event);
}

function endTrigger(trigger: TriggerName, event: PointerEvent): void {
  if (triggerOwners.get(trigger) !== event.pointerId) return;
  triggerOwners.delete(trigger);
  emit("update", { kind: "trigger", trigger, value: 0 });
}

function updateTrigger(trigger: TriggerName, event: PointerEvent): void {
  const element = event.currentTarget as HTMLElement;
  const rect = element.getBoundingClientRect();
  const normalized = 1 - Math.max(0, Math.min(1, (event.clientY - rect.top) / rect.height));
  emit("update", { kind: "trigger", trigger, value: Math.round(normalized * 65535) });
}

function capture(event: PointerEvent): void {
  (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
}

function stickOffset(stick: StickName): Record<string, string> {
  const value = stick === "left" ? props.state.leftStick : props.state.rightStick;
  return {
    transform: `translate(${(value.x / 32767) * 35}px, ${(-value.y / 32767) * 35}px)`,
  };
}

function isPressed(button: GamepadButton): boolean {
  return props.state.pressedButtons.includes(button);
}

onUnmounted(() => {
  for (const frame of stickFrames.values()) cancelAnimationFrame(frame);
  emit("update", { kind: "reset" });
});
</script>

<template>
  <div class="gamepad-surface" :class="{ 'gamepad-surface--disabled': disabled }" aria-label="虚拟触控手柄测试源">
    <div class="gamepad-shoulders">
      <button class="shoulder" :class="{ pressed: isPressed('left_shoulder') }" type="button" @pointerdown="beginButton('left_shoulder', $event)" @pointerup="endButton('left_shoulder', $event)" @pointercancel="endButton('left_shoulder', $event)" @lostpointercapture="endButton('left_shoulder', $event)" @keydown="keyboardButton('left_shoulder', true, $event)" @keyup="keyboardButton('left_shoulder', false, $event)">L1</button>
      <div class="trigger" role="slider" aria-label="L2" aria-valuemin="0" aria-valuemax="65535" :aria-valuenow="state.leftTrigger" tabindex="0" @pointerdown="beginTrigger('left', $event)" @pointermove="moveTrigger('left', $event)" @pointerup="endTrigger('left', $event)" @pointercancel="endTrigger('left', $event)" @lostpointercapture="endTrigger('left', $event)"><i :style="{ height: `${state.leftTrigger / 655.35}%` }"></i><span>L2</span></div>
      <div class="controller-wordmark">CAPY PAD <small>SIMULATED</small></div>
      <div class="trigger" role="slider" aria-label="R2" aria-valuemin="0" aria-valuemax="65535" :aria-valuenow="state.rightTrigger" tabindex="0" @pointerdown="beginTrigger('right', $event)" @pointermove="moveTrigger('right', $event)" @pointerup="endTrigger('right', $event)" @pointercancel="endTrigger('right', $event)" @lostpointercapture="endTrigger('right', $event)"><i :style="{ height: `${state.rightTrigger / 655.35}%` }"></i><span>R2</span></div>
      <button class="shoulder" :class="{ pressed: isPressed('right_shoulder') }" type="button" @pointerdown="beginButton('right_shoulder', $event)" @pointerup="endButton('right_shoulder', $event)" @pointercancel="endButton('right_shoulder', $event)" @lostpointercapture="endButton('right_shoulder', $event)" @keydown="keyboardButton('right_shoulder', true, $event)" @keyup="keyboardButton('right_shoulder', false, $event)">R1</button>
    </div>

    <div class="gamepad-body">
      <div class="gamepad-side gamepad-side--left">
        <div class="dpad" aria-label="方向键">
          <button class="dpad__up" type="button" @pointerdown="beginDpad(0, 1, $event)" @pointerup="endDpad($event)" @pointercancel="endDpad($event)" @lostpointercapture="endDpad($event)" @keydown="keyboardDpad(0, 1, true, $event)" @keyup="keyboardDpad(0, 1, false, $event)">▲</button>
          <button class="dpad__left" type="button" @pointerdown="beginDpad(-1, 0, $event)" @pointerup="endDpad($event)" @pointercancel="endDpad($event)" @lostpointercapture="endDpad($event)" @keydown="keyboardDpad(-1, 0, true, $event)" @keyup="keyboardDpad(-1, 0, false, $event)">◀</button>
          <span></span>
          <button class="dpad__right" type="button" @pointerdown="beginDpad(1, 0, $event)" @pointerup="endDpad($event)" @pointercancel="endDpad($event)" @lostpointercapture="endDpad($event)" @keydown="keyboardDpad(1, 0, true, $event)" @keyup="keyboardDpad(1, 0, false, $event)">▶</button>
          <button class="dpad__down" type="button" @pointerdown="beginDpad(0, -1, $event)" @pointerup="endDpad($event)" @pointercancel="endDpad($event)" @lostpointercapture="endDpad($event)" @keydown="keyboardDpad(0, -1, true, $event)" @keyup="keyboardDpad(0, -1, false, $event)">▼</button>
        </div>
        <div class="touch-stick" role="slider" aria-label="左摇杆" tabindex="0" @pointerdown="beginStick('left', $event)" @pointermove="moveStick('left', $event)" @pointerup="endStick('left', $event)" @pointercancel="endStick('left', $event)" @lostpointercapture="endStick('left', $event)"><span :style="stickOffset('left')">L3</span></div>
      </div>

      <div class="gamepad-center">
        <button type="button" :class="{ pressed: isPressed('select') }" @pointerdown="beginButton('select', $event)" @pointerup="endButton('select', $event)" @pointercancel="endButton('select', $event)" @lostpointercapture="endButton('select', $event)">SELECT</button>
        <button class="guide" type="button" :class="{ pressed: isPressed('guide') }" @pointerdown="beginButton('guide', $event)" @pointerup="endButton('guide', $event)" @pointercancel="endButton('guide', $event)" @lostpointercapture="endButton('guide', $event)">IO</button>
        <button type="button" :class="{ pressed: isPressed('start') }" @pointerdown="beginButton('start', $event)" @pointerup="endButton('start', $event)" @pointercancel="endButton('start', $event)" @lostpointercapture="endButton('start', $event)">START</button>
      </div>

      <div class="gamepad-side gamepad-side--right">
        <div class="face-buttons" aria-label="功能按键">
          <button class="face face--north" :class="{ pressed: isPressed('north') }" type="button" @pointerdown="beginButton('north', $event)" @pointerup="endButton('north', $event)" @pointercancel="endButton('north', $event)" @lostpointercapture="endButton('north', $event)">Y</button>
          <button class="face face--west" :class="{ pressed: isPressed('west') }" type="button" @pointerdown="beginButton('west', $event)" @pointerup="endButton('west', $event)" @pointercancel="endButton('west', $event)" @lostpointercapture="endButton('west', $event)">X</button>
          <button class="face face--east" :class="{ pressed: isPressed('east') }" type="button" @pointerdown="beginButton('east', $event)" @pointerup="endButton('east', $event)" @pointercancel="endButton('east', $event)" @lostpointercapture="endButton('east', $event)">B</button>
          <button class="face face--south" :class="{ pressed: isPressed('south') }" type="button" @pointerdown="beginButton('south', $event)" @pointerup="endButton('south', $event)" @pointercancel="endButton('south', $event)" @lostpointercapture="endButton('south', $event)">A</button>
        </div>
        <div class="touch-stick" role="slider" aria-label="右摇杆" tabindex="0" @pointerdown="beginStick('right', $event)" @pointermove="moveStick('right', $event)" @pointerup="endStick('right', $event)" @pointercancel="endStick('right', $event)" @lostpointercapture="endStick('right', $event)"><span :style="stickOffset('right')">R3</span></div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.gamepad-surface { user-select: none; touch-action: none; color: #eefbf5; border: 1px solid rgba(121, 227, 179, .18); border-radius: 28px; background: radial-gradient(circle at 50% 20%, #25483a, #11231c 66%); box-shadow: inset 0 1px rgba(255,255,255,.08), 0 24px 50px rgba(13,34,25,.22); overflow: hidden; }
.gamepad-surface--disabled { opacity: .55; pointer-events: none; }
.gamepad-shoulders { display: grid; grid-template-columns: 1fr 58px minmax(100px, .7fr) 58px 1fr; gap: 10px; padding: 14px 18px 0; align-items: stretch; }
.shoulder, .gamepad-center button { border: 1px solid rgba(255,255,255,.14); color: inherit; background: rgba(255,255,255,.07); font-weight: 800; }
.shoulder { min-height: 44px; border-radius: 14px 14px 8px 8px; }
.trigger { position: relative; display: grid; min-height: 44px; place-items: center; overflow: hidden; border: 1px solid rgba(255,255,255,.14); border-radius: 10px; background: rgba(255,255,255,.05); }
.trigger i { position: absolute; right: 0; bottom: 0; left: 0; background: #65d6a5; }
.trigger span { position: relative; z-index: 1; font-size: 11px; font-weight: 900; }
.controller-wordmark { align-self: center; text-align: center; color: #9fb9ad; font-size: 11px; font-weight: 900; letter-spacing: .15em; }
.controller-wordmark small { display: block; margin-top: 3px; color: #5f7b6e; font-size: 8px; }
.gamepad-body { display: grid; grid-template-columns: 1fr minmax(126px, .5fr) 1fr; gap: 20px; min-height: 330px; padding: 24px; }
.gamepad-side { display: grid; gap: 22px; align-content: center; justify-items: center; }
.dpad, .face-buttons { display: grid; grid-template-columns: repeat(3, 58px); grid-template-rows: repeat(3, 58px); }
.dpad button, .face { border: 0; color: #f4fff9; background: #2c3b35; box-shadow: inset 0 -4px rgba(0,0,0,.22); font-weight: 900; touch-action: none; }
.dpad__up, .face--north { grid-column: 2; grid-row: 1; border-radius: 12px 12px 4px 4px !important; }
.dpad__left, .face--west { grid-column: 1; grid-row: 2; border-radius: 12px 4px 4px 12px !important; }
.dpad__right, .face--east { grid-column: 3; grid-row: 2; border-radius: 4px 12px 12px 4px !important; }
.dpad__down, .face--south { grid-column: 2; grid-row: 3; border-radius: 4px 4px 12px 12px !important; }
.face { width: 52px; height: 52px; align-self: center; justify-self: center; border-radius: 50% !important; }
.face--north { color: #ffcc72; }.face--west { color: #75c8ff; }.face--east { color: #ff8a8a; }.face--south { color: #7ee0ad; }
.touch-stick { display: grid; width: 132px; aspect-ratio: 1; place-items: center; border: 1px solid rgba(255,255,255,.14); border-radius: 50%; background: radial-gradient(circle, rgba(255,255,255,.04) 0 20%, transparent 21%), repeating-radial-gradient(circle, transparent 0 32%, rgba(255,255,255,.06) 33% 34%); touch-action: none; }
.touch-stick span { display: grid; width: 58px; aspect-ratio: 1; place-items: center; border-radius: 50%; color: #b8d0c5; background: #30443b; box-shadow: 0 9px 16px rgba(0,0,0,.25), inset 0 1px rgba(255,255,255,.12); font-size: 11px; font-weight: 900; will-change: transform; }
.gamepad-center { display: grid; grid-template-columns: 1fr 52px 1fr; gap: 8px; align-content: center; align-items: center; }
.gamepad-center button { min-height: 34px; border-radius: 10px; font-size: 9px; }
.gamepad-center .guide { width: 52px; height: 52px; border-radius: 50%; color: #173328; background: #70dfad; font-size: 13px; }
button.pressed, button:active { color: #163126; background: #72e0ae; box-shadow: 0 0 0 4px rgba(114,224,174,.16); transform: translateY(1px); }
button:focus-visible, [role="slider"]:focus-visible { outline: 3px solid rgba(121,227,179,.42); outline-offset: 3px; }
@media (max-width: 820px) { .gamepad-body { grid-template-columns: 1fr 1fr; }.gamepad-center { grid-column: 1 / -1; grid-row: 1; }.gamepad-side { grid-row: 2; }.controller-wordmark { display: none; }.gamepad-shoulders { grid-template-columns: 1fr 54px 54px 1fr; }.gamepad-shoulders .trigger:nth-of-type(2) { grid-column: 3; } }
@media (max-width: 540px) { .gamepad-body { gap: 8px; padding: 16px 8px 20px; }.dpad, .face-buttons { transform: scale(.82); margin: -16px; }.touch-stick { width: 108px; }.gamepad-shoulders { padding-inline: 10px; }.gamepad-center { grid-template-columns: 1fr 46px 1fr; }.gamepad-center .guide { width: 46px; height: 46px; } }
</style>
