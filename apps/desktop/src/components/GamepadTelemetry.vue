<script setup lang="ts">
import { computed } from "vue";

import type { GamepadButton, UiGamepadState } from "../lib/types";

const props = defineProps<{ state: UiGamepadState }>();
const pressed = computed(() => new Set<GamepadButton>(props.state.pressedButtons));
const trackedButtons: GamepadButton[] = ["left_shoulder", "right_shoulder", "left_stick", "right_stick", "select", "start", "guide"];

function axisPercent(value: number): number { return 50 + (value / 32767) * 42; }
function triggerPercent(value: number): number { return (value / 65535) * 100; }
function fixed(value: number): string { return Number.isFinite(value) ? value.toFixed(3) : "—"; }
</script>

<template>
  <div class="telemetry" aria-label="完整手柄状态可视化">
    <div class="telemetry__meta">
      <div><span>Source</span><strong>{{ state.source }}</strong></div>
      <div><span>Epoch / Seq</span><strong>{{ state.streamEpoch }} / {{ state.sequence ?? "—" }}</strong></div>
      <div><span>Last update</span><strong>{{ state.lastUpdate }}</strong></div>
    </div>
    <div class="telemetry__body">
      <div class="trigger-meter"><span>L2</span><i><b :style="{ height: `${triggerPercent(state.leftTrigger)}%` }"></b></i><strong>{{ state.leftTrigger }}</strong></div>
      <div class="stick-meter"><span>LS</span><i><b :style="{ left: `${axisPercent(state.leftStick.x)}%`, top: `${axisPercent(-state.leftStick.y)}%` }"></b></i><small>{{ state.leftStick.x }}, {{ state.leftStick.y }}</small></div>
      <div class="button-map">
        <span class="button-map__north" :class="{ on: pressed.has('north') }">Y</span>
        <span class="button-map__west" :class="{ on: pressed.has('west') }">X</span>
        <span class="button-map__east" :class="{ on: pressed.has('east') }">B</span>
        <span class="button-map__south" :class="{ on: pressed.has('south') }">A</span>
        <small>D-pad {{ state.dpad.x }}, {{ state.dpad.y }}</small>
      </div>
      <div class="stick-meter"><span>RS</span><i><b :style="{ left: `${axisPercent(state.rightStick.x)}%`, top: `${axisPercent(-state.rightStick.y)}%` }"></b></i><small>{{ state.rightStick.x }}, {{ state.rightStick.y }}</small></div>
      <div class="trigger-meter"><span>R2</span><i><b :style="{ height: `${triggerPercent(state.rightTrigger)}%` }"></b></i><strong>{{ state.rightTrigger }}</strong></div>
    </div>
    <div class="button-strip">
      <span v-for="button in trackedButtons" :key="button" :class="{ on: pressed.has(button) }">{{ button.replaceAll('_', ' ') }}</span>
    </div>
    <div class="motion-grid" aria-label="IMU live values">
      <div><span>Acceleration · m/s²</span><strong>X {{ fixed(state.motion.acceleration[0]) }}</strong><strong>Y {{ fixed(state.motion.acceleration[1]) }}</strong><strong>Z {{ fixed(state.motion.acceleration[2]) }}</strong></div>
      <div><span>Angular velocity · rad/s</span><strong>X {{ fixed(state.motion.angularVelocity[0]) }}</strong><strong>Y {{ fixed(state.motion.angularVelocity[1]) }}</strong><strong>Z {{ fixed(state.motion.angularVelocity[2]) }}</strong></div>
    </div>
  </div>
</template>

<style scoped>
.telemetry { display: grid; gap: 18px; height: 100%; padding: 22px; border: 1px solid rgba(15,59,42,.12); border-radius: 24px; background: linear-gradient(145deg, rgba(249,253,250,.92), rgba(231,242,235,.86)); }
.telemetry__meta { display: grid; grid-template-columns: repeat(3,minmax(0,1fr)); gap: 10px; min-width: 0; }
.telemetry__meta div { min-width: 0; padding: 11px; border-radius: 12px; background: rgba(255,255,255,.72); }
.telemetry__meta span, .telemetry__meta strong { display: block; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.telemetry__meta span { color: #708078; font-size: 9px; font-weight: 800; letter-spacing: .08em; text-transform: uppercase; }
.telemetry__meta strong { margin-top: 5px; font-size: 11px; }
.telemetry__body { display: grid; grid-template-columns: 44px 1fr 1fr 1fr 44px; gap: 12px; align-items: center; }
.stick-meter, .trigger-meter { display: grid; gap: 7px; justify-items: center; color: #52655c; font-size: 10px; font-weight: 800; }
.stick-meter i { position: relative; width: 90px; aspect-ratio: 1; border: 1px solid rgba(18,66,47,.16); border-radius: 50%; background: linear-gradient(90deg, transparent 49%, rgba(18,66,47,.11) 50% 51%, transparent 52%), linear-gradient(transparent 49%, rgba(18,66,47,.11) 50% 51%, transparent 52%); }
.stick-meter b { position: absolute; width: 13px; height: 13px; border-radius: 50%; background: #0b7650; box-shadow: 0 0 0 5px rgba(11,118,80,.14); transform: translate(-50%,-50%); }
.stick-meter small { font-variant-numeric: tabular-nums; }
.trigger-meter i { position: relative; width: 14px; height: 100px; overflow: hidden; border-radius: 7px; background: rgba(18,66,47,.11); }
.trigger-meter b { position: absolute; right: 0; bottom: 0; left: 0; background: #0b7650; }
.trigger-meter strong { font-size: 9px; writing-mode: vertical-rl; }
.button-map { display: grid; grid-template-columns: repeat(3,38px); grid-template-rows: repeat(3,38px) auto; place-content: center; }
.button-map > span { display: grid; place-items: center; align-self: center; justify-self: center; width: 32px; height: 32px; border-radius: 50%; color: #829189; background: #dce7e1; font-size: 11px; font-weight: 900; }
.button-map__north { grid-area: 1/2; }.button-map__west { grid-area: 2/1; }.button-map__east { grid-area: 2/3; }.button-map__south { grid-area: 3/2; }
.button-map small { grid-column: 1/-1; text-align: center; color: #52655c; font-size: 9px; }
.button-map > span.on, .button-strip span.on { color: white; background: #0b7650; box-shadow: 0 0 0 4px rgba(11,118,80,.12); }
.button-strip { display: flex; flex-wrap: wrap; gap: 7px; }
.button-strip span { padding: 6px 8px; border-radius: 8px; color: #73837b; background: rgba(18,66,47,.07); font-size: 8px; font-weight: 850; text-transform: uppercase; }
.motion-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 9px; }
.motion-grid div { display: grid; grid-template-columns: 1fr repeat(3,auto); gap: 9px; padding: 10px 12px; border-radius: 12px; background: rgba(255,255,255,.68); font-size: 9px; font-variant-numeric: tabular-nums; }
.motion-grid span { color: #708078; font-weight: 800; text-transform: uppercase; }
.motion-grid strong { color: #244438; }
@media (max-width: 680px) { .telemetry__body { grid-template-columns: 34px 1fr 1fr 34px; }.button-map { grid-column: 2/4; grid-row: 2; }.stick-meter i { width: 78px; }.telemetry__meta { grid-template-columns: 1fr; } }
</style>
