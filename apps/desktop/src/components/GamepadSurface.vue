<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { createCapyIOApi } from '../lib/api';
import { stickAxes, type StickGeometry, type StickMode } from '../lib/gamepad-stick';

import type { GamepadButton, GamepadControlUpdate, UiGamepadState } from "../lib/types";

const props = defineProps<{ state: UiGamepadState; disabled?: boolean; fullscreen?: boolean }>();
const emit = defineEmits<{ update: [value: GamepadControlUpdate] }>();

const layoutLabels = { shoulders: '肩键区', left: '左手区', right: '右手区', center: '中间区',
  l1: 'L1', l2: 'L2', r1: 'R1', r2: 'R2', north: 'Y', west: 'X', east: 'B', south: 'A',
  dpad: '方向键', ls: '左摇杆', rs: '右摇杆', l3: 'L3', r3: 'R3', select: 'SELECT', start: 'START', guide: '主键' } as const;
type LayoutGroup = keyof typeof layoutLabels;
type Placement = { x: number; y: number; scale: number };
const layoutApi = createCapyIOApi();
const defaultLayout = (): Record<LayoutGroup, Placement> => Object.fromEntries(
  Object.keys(layoutLabels).map(key => [key, { x: 0, y: 0, scale: 1 }])) as Record<LayoutGroup, Placement>;
const layout = ref(defaultLayout());
const stickMode = ref<StickMode>('floating');
const stickVisibility = ref<'always' | 'touch' | 'hidden'>('always');
const triggerMode = ref<'button' | 'analog'>('button');
const activeSticks = ref({ left: false, right: false });
const stickCenters = new Map<StickName, StickGeometry>();
const stickVisual = ref<Record<StickName, Record<string, string>>>({ left: {}, right: {} });
async function loadLayout(): Promise<void> {
try {
  const saved = JSON.parse(await layoutApi.getGamepadLayout() ?? 'null');
  if (editing.value) return;
  if (['fixed', 'floating', 'follow'].includes(saved?.stickMode)) stickMode.value = saved.stickMode;
  if (['always', 'touch', 'hidden'].includes(saved?.stickVisibility)) stickVisibility.value = saved.stickVisibility;
  if (saved?.triggerMode === 'analog') triggerMode.value = 'analog';
  for (const key of Object.keys(layout.value) as LayoutGroup[]) {
    const item = saved?.[key];
    if (item && Number.isFinite(item.x) && Number.isFinite(item.y) && Number.isFinite(item.scale)) {
      layout.value[key] = { x: Math.max(-30, Math.min(30, item.x)), y: Math.max(-30, Math.min(30, item.y)), scale: Math.max(.6, Math.min(1.4, item.scale)) };
    }
  }
} catch { /* Use the default layout if local settings cannot be read. */ }
}
const editing = ref(false);
const selectedGroup = ref<LayoutGroup>('left');
const layoutMessage = ref('');
let drag: { id: number; x: number; y: number; start: Placement; width: number; height: number; parentScale: number } | null = null;
function placementStyle(group: LayoutGroup): Record<string, string> {
  if (!props.fullscreen) return {};
  const item = layout.value[group];
  return { transform: `translate(${item.x}vw, ${item.y}vh) scale(${item.scale})`, outline: editing.value ? '1px dashed #70dfad' : 'none',
    position: 'relative', zIndex: editing.value && selectedGroup.value === group ? '4' : 'auto' };
}
function beginLayout(event: PointerEvent): void {
  if (!editing.value) return;
  const target = (event.target as HTMLElement).closest<HTMLElement>('[data-layout]');
  if (!target) return;
  event.preventDefault();
  event.stopPropagation();
  selectedGroup.value = target.dataset.layout as LayoutGroup;
  const root = event.currentTarget as HTMLElement;
  root.setPointerCapture(event.pointerId);
  const parentKey = target.parentElement?.closest<HTMLElement>('[data-layout]')?.dataset.layout as LayoutGroup | undefined;
  drag = { id: event.pointerId, x: event.clientX, y: event.clientY,
    start: { ...layout.value[selectedGroup.value] }, width: window.innerWidth, height: window.innerHeight,
    parentScale: parentKey ? layout.value[parentKey].scale : 1 };
}
function moveLayout(event: PointerEvent): void {
  if (!drag || drag.id !== event.pointerId) return;
  event.preventDefault();
  event.stopPropagation();
  layout.value[selectedGroup.value] = { ...drag.start,
    x: Math.max(-30, Math.min(30, drag.start.x + (event.clientX - drag.x) / drag.width / drag.parentScale * 100)),
    y: Math.max(-30, Math.min(30, drag.start.y + (event.clientY - drag.y) / drag.height / drag.parentScale * 100)) };
}
function endLayout(event: PointerEvent): void {
  if (drag?.id === event.pointerId) { drag = null; event.stopPropagation(); }
}
async function toggleLayout(): Promise<void> {
  releaseInputs();
  drag = null;
  if (editing.value) {
    try {
      const compact = Object.fromEntries(Object.entries(layout.value).map(([key, item]) => [key,
        { x: Math.round(item.x * 10) / 10, y: Math.round(item.y * 10) / 10, scale: Math.round(item.scale * 100) / 100 }]));
      await layoutApi.saveGamepadLayout(JSON.stringify({ ...compact, stickMode: stickMode.value, stickVisibility: stickVisibility.value, triggerMode: triggerMode.value }));
      layoutMessage.value = '';
    }
    catch { layoutMessage.value = '无法保存，请重试；当前布局仍保留在编辑器中'; return; }
  }
  editing.value = !editing.value;
}

type StickName = "left" | "right";
type TriggerName = "left" | "right";

const buttonPointers = new Map<GamepadButton, Set<number>>();
const dpadPointers = new Map<number, { x: number; y: number }>();
const stickOwners = new Map<StickName, number>();
const triggerOwners = new Map<TriggerName, number>();
const pendingStick = new Map<StickName, { x: number; y: number }>();
const stickFrames = new Map<StickName, number>();

function beginButton(button: GamepadButton, event: PointerEvent): void {
  if (props.disabled || editing.value) return;
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
  if (props.disabled || editing.value || (event.key !== " " && event.key !== "Enter")) return;
  if (event.repeat) return;
  event.preventDefault();
  emit("update", { kind: "button", button, pressed });
}

function beginDpad(x: number, y: number, event: PointerEvent): void {
  if (props.disabled || editing.value) return;
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
  if (props.disabled || editing.value || (event.key !== " " && event.key !== "Enter")) return;
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
  if (props.disabled || editing.value || stickOwners.has(stick)) return;
  event.preventDefault();
  capture(event);
  stickOwners.set(stick, event.pointerId);
  activeSticks.value[stick] = true;
  const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
  const radius = Math.min(rect.width, rect.height) * .32;
  stickCenters.set(stick, { x: stickMode.value === 'fixed' ? rect.left + rect.width / 2 : event.clientX,
    y: stickMode.value === 'fixed' ? rect.top + rect.height / 2 : event.clientY, radius });
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
  stickCenters.delete(stick);
  activeSticks.value[stick] = false;
  stickVisual.value[stick] = {};
  const frame = stickFrames.get(stick);
  if (frame !== undefined) cancelAnimationFrame(frame);
  stickFrames.delete(stick);
  pendingStick.delete(stick);
  emit("update", { kind: "stick", stick, x: 0, y: 0 });
}

function updateStick(stick: StickName, event: PointerEvent): void {
  const element = event.currentTarget as HTMLElement;
  const rect = element.getBoundingClientRect();
  const center = stickCenters.get(stick);
  if (!center) return;
  const axes = stickAxes(center, event.clientX, event.clientY, stickMode.value === 'follow');
  stickVisual.value[stick] = {
    left: `${(center.x - rect.left) / rect.width * 100}%`, top: `${(center.y - rect.top) / rect.height * 100}%`,
    transform: `translate(-50%, -50%) translate(${axes.x / 32767 * 70}%, ${-axes.y / 32767 * 70}%)`,
  };
  pendingStick.set(stick, axes);
  if (stickFrames.has(stick)) return;
  stickFrames.set(stick, requestAnimationFrame(() => {
    stickFrames.delete(stick);
    const pending = pendingStick.get(stick);
    if (pending) emit("update", { kind: "stick", stick, ...pending });
  }));
}

function beginTrigger(trigger: TriggerName, event: PointerEvent): void {
  if (props.disabled || editing.value || triggerOwners.has(trigger)) return;
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
  if (triggerMode.value === 'button') {
    emit('update', { kind: 'trigger', trigger, value: 65535 });
    return;
  }
  const element = event.currentTarget as HTMLElement;
  const rect = element.getBoundingClientRect();
  const normalized = 1 - Math.max(0, Math.min(1, (event.clientY - rect.top) / rect.height));
  emit("update", { kind: "trigger", trigger, value: Math.round(normalized * 65535) });
}

function capture(event: PointerEvent): void {
  (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
}

function stickOffset(stick: StickName): Record<string, string> {
  if (activeSticks.value[stick]) return stickVisual.value[stick];
  const value = stick === "left" ? props.state.leftStick : props.state.rightStick;
  return {
    left: '50%', top: '50%', transform: `translate(-50%, -50%) translate(${(value.x / 32767) * 70}%, ${(-value.y / 32767) * 70}%)`,
  };
}

function isPressed(button: GamepadButton): boolean {
  return props.state.pressedButtons.includes(button);
}

function releaseInputs(): void {
  for (const frame of stickFrames.values()) cancelAnimationFrame(frame);
  stickFrames.clear();
  pendingStick.clear();
  buttonPointers.clear();
  dpadPointers.clear();
  stickOwners.clear();
  stickCenters.clear();
  activeSticks.value = { left: false, right: false };
  stickVisual.value = { left: {}, right: {} };
  triggerOwners.clear();
  if (!props.disabled) emit("update", { kind: "reset" });
}
function visibilityChanged(): void { if (document.hidden) releaseInputs(); }
onMounted(() => {
  void loadLayout();
  window.addEventListener('blur', releaseInputs);
  window.addEventListener('resize', surfaceResized);
  document.addEventListener('visibilitychange', visibilityChanged);
});
onUnmounted(() => {
  window.removeEventListener('blur', releaseInputs);
  window.removeEventListener('resize', surfaceResized);
  document.removeEventListener('visibilitychange', visibilityChanged);
  releaseInputs();
});
function surfaceResized(): void {
  drag = null;
  releaseInputs();
}
</script>

<template>
  <div class="gamepad-surface" :class="{ 'gamepad-surface--disabled': disabled && !fullscreen, 'gamepad-surface--fullscreen': fullscreen, 'gamepad-surface--editing': editing }" aria-label="触控手柄" @pointerdown.capture="beginLayout" @pointermove.capture="moveLayout" @pointerup.capture="endLayout" @pointercancel.capture="endLayout" @lostpointercapture="endLayout">
    <div v-if="fullscreen" class="layout-toolbar">
      <button type="button" @click="toggleLayout">{{ editing ? '保存布局，开始操作' : '调整布局' }}</button>
      <template v-if="editing">
        <span>拖动按键或摇杆调整位置（不会发送按键）</span>
        <select v-model="selectedGroup" aria-label="调整控件"><option v-for="(label, key) in layoutLabels" :key="key" :value="key">{{ label }}</option></select>
        <label>横向 <input v-model.number="layout[selectedGroup].x" aria-label="控件横向位置" type="range" min="-30" max="30" step="0.5" /></label>
        <label>纵向 <input v-model.number="layout[selectedGroup].y" aria-label="控件纵向位置" type="range" min="-30" max="30" step="0.5" /></label>
        <label>大小 <input v-model.number="layout[selectedGroup].scale" type="range" min="0.6" max="1.4" step="0.05" /></label>
        <label>摇杆 <select v-model="stickMode"><option value="fixed">固定中心</option><option value="floating">落指定位</option><option value="follow">跟随手指</option></select></label>
        <label>显示 <select v-model="stickVisibility"><option value="always">始终显示</option><option value="touch">触摸时显示</option><option value="hidden">隐藏摇杆</option></select></label>
        <label>扳机 <select v-model="triggerMode"><option value="button">按下全按</option><option value="analog">滑动控制力度</option></select></label>
        <button type="button" @click="layout = defaultLayout()">恢复默认</button>
      </template>
      <span role="status">{{ layoutMessage }}</span>
    </div>
    <div class="gamepad-shoulders" data-layout="shoulders" :style="placementStyle('shoulders')">
      <button data-layout="l1" :style="placementStyle('l1')" class="shoulder" :class="{ pressed: isPressed('left_shoulder') }" type="button" @pointerdown="beginButton('left_shoulder', $event)" @pointerup="endButton('left_shoulder', $event)" @pointercancel="endButton('left_shoulder', $event)" @lostpointercapture="endButton('left_shoulder', $event)" @keydown="keyboardButton('left_shoulder', true, $event)" @keyup="keyboardButton('left_shoulder', false, $event)">L1</button>
      <div data-layout="l2" :style="placementStyle('l2')" class="trigger" role="slider" aria-label="L2" aria-valuemin="0" aria-valuemax="65535" :aria-valuenow="state.leftTrigger" tabindex="0" @pointerdown="beginTrigger('left', $event)" @pointermove="moveTrigger('left', $event)" @pointerup="endTrigger('left', $event)" @pointercancel="endTrigger('left', $event)" @lostpointercapture="endTrigger('left', $event)"><i :style="{ height: `${state.leftTrigger / 655.35}%` }"></i><span>L2</span></div>
      <div class="controller-wordmark">CAPY PAD <small>{{ state.simulated ? "SIMULATED" : "LOCAL" }}</small></div>
      <div data-layout="r2" :style="placementStyle('r2')" class="trigger" role="slider" aria-label="R2" aria-valuemin="0" aria-valuemax="65535" :aria-valuenow="state.rightTrigger" tabindex="0" @pointerdown="beginTrigger('right', $event)" @pointermove="moveTrigger('right', $event)" @pointerup="endTrigger('right', $event)" @pointercancel="endTrigger('right', $event)" @lostpointercapture="endTrigger('right', $event)"><i :style="{ height: `${state.rightTrigger / 655.35}%` }"></i><span>R2</span></div>
      <button data-layout="r1" :style="placementStyle('r1')" class="shoulder" :class="{ pressed: isPressed('right_shoulder') }" type="button" @pointerdown="beginButton('right_shoulder', $event)" @pointerup="endButton('right_shoulder', $event)" @pointercancel="endButton('right_shoulder', $event)" @lostpointercapture="endButton('right_shoulder', $event)" @keydown="keyboardButton('right_shoulder', true, $event)" @keyup="keyboardButton('right_shoulder', false, $event)">R1</button>
    </div>

    <div class="gamepad-body">
      <div class="gamepad-side gamepad-side--left" data-layout="left" :style="placementStyle('left')">
        <div data-layout="dpad" :style="placementStyle('dpad')" class="dpad" aria-label="方向键">
          <button class="dpad__up" type="button" @pointerdown="beginDpad(0, 1, $event)" @pointerup="endDpad($event)" @pointercancel="endDpad($event)" @lostpointercapture="endDpad($event)" @keydown="keyboardDpad(0, 1, true, $event)" @keyup="keyboardDpad(0, 1, false, $event)">▲</button>
          <button class="dpad__left" type="button" @pointerdown="beginDpad(-1, 0, $event)" @pointerup="endDpad($event)" @pointercancel="endDpad($event)" @lostpointercapture="endDpad($event)" @keydown="keyboardDpad(-1, 0, true, $event)" @keyup="keyboardDpad(-1, 0, false, $event)">◀</button>
          <span></span>
          <button class="dpad__right" type="button" @pointerdown="beginDpad(1, 0, $event)" @pointerup="endDpad($event)" @pointercancel="endDpad($event)" @lostpointercapture="endDpad($event)" @keydown="keyboardDpad(1, 0, true, $event)" @keyup="keyboardDpad(1, 0, false, $event)">▶</button>
          <button class="dpad__down" type="button" @pointerdown="beginDpad(0, -1, $event)" @pointerup="endDpad($event)" @pointercancel="endDpad($event)" @lostpointercapture="endDpad($event)" @keydown="keyboardDpad(0, -1, true, $event)" @keyup="keyboardDpad(0, -1, false, $event)">▼</button>
        </div>
        <div data-layout="ls" :style="placementStyle('ls')" class="touch-stick" :class="{ 'stick-invisible': !editing && (stickVisibility === 'hidden' || (stickVisibility === 'touch' && !activeSticks.left)) }" role="slider" aria-label="左摇杆" tabindex="0" @pointerdown="beginStick('left', $event)" @pointermove="moveStick('left', $event)" @pointerup="endStick('left', $event)" @pointercancel="endStick('left', $event)" @lostpointercapture="endStick('left', $event)"><span :style="stickOffset('left')">L</span></div>
        <button data-layout="l3" :style="placementStyle('l3')" class="stick-click" :class="{ pressed: isPressed('left_stick') }" type="button" @pointerdown="beginButton('left_stick', $event)" @pointerup="endButton('left_stick', $event)" @pointercancel="endButton('left_stick', $event)" @lostpointercapture="endButton('left_stick', $event)" @keydown="keyboardButton('left_stick', true, $event)" @keyup="keyboardButton('left_stick', false, $event)">L3</button>
      </div>

      <div class="gamepad-center" data-layout="center" :style="placementStyle('center')">
        <button data-layout="select" :style="placementStyle('select')" type="button" :class="{ pressed: isPressed('select') }" @pointerdown="beginButton('select', $event)" @pointerup="endButton('select', $event)" @pointercancel="endButton('select', $event)" @lostpointercapture="endButton('select', $event)">SELECT</button>
        <button data-layout="guide" :style="placementStyle('guide')" class="guide" type="button" :class="{ pressed: isPressed('guide') }" @pointerdown="beginButton('guide', $event)" @pointerup="endButton('guide', $event)" @pointercancel="endButton('guide', $event)" @lostpointercapture="endButton('guide', $event)">IO</button>
        <button data-layout="start" :style="placementStyle('start')" type="button" :class="{ pressed: isPressed('start') }" @pointerdown="beginButton('start', $event)" @pointerup="endButton('start', $event)" @pointercancel="endButton('start', $event)" @lostpointercapture="endButton('start', $event)">START</button>
      </div>

      <div class="gamepad-side gamepad-side--right" data-layout="right" :style="placementStyle('right')">
        <div class="face-buttons" aria-label="功能按键">
          <button data-layout="north" :style="placementStyle('north')" class="face face--north" :class="{ pressed: isPressed('north') }" type="button" @pointerdown="beginButton('north', $event)" @pointerup="endButton('north', $event)" @pointercancel="endButton('north', $event)" @lostpointercapture="endButton('north', $event)">Y</button>
          <button data-layout="west" :style="placementStyle('west')" class="face face--west" :class="{ pressed: isPressed('west') }" type="button" @pointerdown="beginButton('west', $event)" @pointerup="endButton('west', $event)" @pointercancel="endButton('west', $event)" @lostpointercapture="endButton('west', $event)">X</button>
          <button data-layout="east" :style="placementStyle('east')" class="face face--east" :class="{ pressed: isPressed('east') }" type="button" @pointerdown="beginButton('east', $event)" @pointerup="endButton('east', $event)" @pointercancel="endButton('east', $event)" @lostpointercapture="endButton('east', $event)">B</button>
          <button data-layout="south" :style="placementStyle('south')" class="face face--south" :class="{ pressed: isPressed('south') }" type="button" @pointerdown="beginButton('south', $event)" @pointerup="endButton('south', $event)" @pointercancel="endButton('south', $event)" @lostpointercapture="endButton('south', $event)">A</button>
        </div>
        <div data-layout="rs" :style="placementStyle('rs')" class="touch-stick" :class="{ 'stick-invisible': !editing && (stickVisibility === 'hidden' || (stickVisibility === 'touch' && !activeSticks.right)) }" role="slider" aria-label="右摇杆" tabindex="0" @pointerdown="beginStick('right', $event)" @pointermove="moveStick('right', $event)" @pointerup="endStick('right', $event)" @pointercancel="endStick('right', $event)" @lostpointercapture="endStick('right', $event)"><span :style="stickOffset('right')">R</span></div>
        <button data-layout="r3" :style="placementStyle('r3')" class="stick-click" :class="{ pressed: isPressed('right_stick') }" type="button" @pointerdown="beginButton('right_stick', $event)" @pointerup="endButton('right_stick', $event)" @pointercancel="endButton('right_stick', $event)" @lostpointercapture="endButton('right_stick', $event)" @keydown="keyboardButton('right_stick', true, $event)" @keyup="keyboardButton('right_stick', false, $event)">R3</button>
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
.gamepad-surface--fullscreen { flex: 1; min-height: 0; display: flex; flex-direction: column; border: 0; border-radius: 0; box-shadow: none; }
.layout-toolbar { position: relative; z-index: 5; display: flex; flex-wrap: wrap; align-items: center; justify-content: center; gap: 8px; padding: 4px 12px; font-size: 12px; background: #11231c; }
.layout-toolbar button, .layout-toolbar select { min-height: 36px; border: 1px solid #577568; border-radius: 8px; background: #25483a; color: #eefbf5; padding: 4px 12px; }
.layout-toolbar label { display: flex; align-items: center; gap: 8px; }
.gamepad-surface--editing [data-layout] { cursor: move; }
.gamepad-surface--fullscreen .gamepad-body { flex: 1; min-height: 0; padding: 8px 4vw; gap: 2vw; }
.gamepad-surface--fullscreen .gamepad-side { gap: 2vh; }
.gamepad-surface--fullscreen .dpad, .gamepad-surface--fullscreen .face-buttons { --cell: min(8vh, 9vw); grid-template-columns: repeat(3, var(--cell)); grid-template-rows: repeat(3, var(--cell)); transform: none; margin: 0; }
.gamepad-surface--fullscreen .face { width: 88%; height: 88%; }
.gamepad-surface--fullscreen .touch-stick { width: min(24vh, 26vw); }
.gamepad-surface--fullscreen .touch-stick span { width: 44%; }
.touch-stick { position: relative; }
.touch-stick span { position: absolute; }
.stick-invisible { background: transparent; border-color: transparent; }
.stick-invisible span { opacity: 0; }
.stick-click { min-width: 64px; min-height: 44px; border: 1px solid #577568; border-radius: 18px; background: #25483a; color: inherit; touch-action: none; }
.gamepad-surface--fullscreen .gamepad-shoulders { grid-template-columns: 1fr 1fr minmax(40px, .4fr) 1fr 1fr; gap: clamp(8px, 1.5vw, 24px); padding-inline: max(16px, env(safe-area-inset-left), 4vw); }
.gamepad-surface--fullscreen .shoulder, .gamepad-surface--fullscreen .trigger { min-height: clamp(48px, 11vh, 104px); font-size: clamp(16px, 2.8vh, 24px); }
.gamepad-surface--fullscreen .trigger span { font-size: inherit; }
@media (max-width: 820px) {
  .gamepad-surface--fullscreen .gamepad-shoulders { grid-template-columns: repeat(4, 1fr); }
}
@media (orientation: landscape) {
  .gamepad-surface--fullscreen .gamepad-body { grid-template-columns: 1fr minmax(140px, .5fr) 1fr; }
  .gamepad-surface--fullscreen .gamepad-center { grid-column: 2; grid-row: 1; }
  .gamepad-surface--fullscreen .gamepad-side { grid-row: 1; }
  .gamepad-surface--fullscreen .gamepad-side--right { grid-column: 3; }
}
</style>
