<script setup lang="ts">
import { computed } from "vue";

import type { UiRoute } from "../lib/types";
import MetricTile from "./MetricTile.vue";
import StatusPill from "./StatusPill.vue";

const props = defineProps<{ route: UiRoute; busy: boolean }>();
const emit = defineEmits<{ toggle: [route: UiRoute] }>();

const icon = computed(() => {
  if (props.route.profile.includes("audio")) return "AUD";
  if (props.route.profile.includes("video")) return "CAM";
  return "IMU";
});
const statusTone = computed(() => {
  if (props.route.state === "failed") return "danger" as const;
  if (props.route.state === "offline") return "warning" as const;
  if (props.route.active) return "success" as const;
  return "neutral" as const;
});
const latency = computed(() => metric(props.route.metrics.estimatedLatencyMs, "ms", 1));
const loss = computed(() => metric(props.route.metrics.packetLossPercent, "%", 2));
const buffer = computed(() => metric(props.route.metrics.bufferFillMs, "ms", 0));

function metric(value: number | null, unit: string, digits: number): string {
  return value === null ? "—" : `${value.toFixed(digits)} ${unit}`;
}
</script>

<template>
  <article class="capability-card" :class="{ 'capability-card--active': route.active }">
    <header class="capability-card__header">
      <div class="capability-card__identity">
        <div class="capability-card__icon" aria-hidden="true">{{ icon }}</div>
        <div>
          <p class="eyebrow">{{ route.summary }}</p>
          <h3>{{ route.title }}</h3>
        </div>
      </div>
      <StatusPill :tone="statusTone" :label="route.state" />
    </header>

    <div class="route-flow" aria-label="Source to Sink">
      <div>
        <span>Source</span>
        <strong>{{ route.source.portName }}</strong>
      </div>
      <i aria-hidden="true">→</i>
      <div>
        <span>Sink</span>
        <strong>{{ route.sink.portName }}</strong>
      </div>
    </div>

    <dl class="capability-card__facts">
      <div><dt>Profile</dt><dd>{{ route.profile }}</dd></div>
      <div><dt>Backend</dt><dd>{{ route.backend }}</dd></div>
      <div><dt>Format</dt><dd>{{ route.formatSummary ?? "not negotiated" }}</dd></div>
      <div><dt>Projection</dt><dd>{{ route.projectionNote }}</dd></div>
    </dl>

    <div class="metric-grid">
      <MetricTile label="Estimated latency" :value="latency" detail="simulated" />
      <MetricTile label="Packet loss" :value="loss" detail="simulated" />
      <MetricTile label="Buffer" :value="buffer" detail="simulated" />
    </div>

    <div class="capability-card__footer">
      <div>
        <span class="capability-card__qos-label">QoS</span>
        <div class="tag-list">
          <span v-for="mode in route.qosModes" :key="mode" class="tag">{{ mode }}</span>
        </div>
      </div>
      <button
        class="primary-button"
        :class="{ 'primary-button--stop': route.active }"
        type="button"
        :disabled="busy"
        @click="emit('toggle', route)"
      >
        {{ busy ? "处理中…" : route.active ? "停止 Route" : "启动 Route" }}
      </button>
    </div>
  </article>
</template>
