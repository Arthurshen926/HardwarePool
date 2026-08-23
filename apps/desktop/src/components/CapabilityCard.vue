<script setup lang="ts">
import { computed } from "vue";

import type { UiCapability } from "../lib/types";
import MetricTile from "./MetricTile.vue";
import StatusPill from "./StatusPill.vue";

const props = defineProps<{
  capability: UiCapability;
  busy: boolean;
}>();

const emit = defineEmits<{
  toggle: [capability: UiCapability];
}>();

const title = computed(() =>
  props.capability.kind === "audio_capture"
    ? "手机麦克风 → Windows"
    : "Windows → 手机扬声器",
);

const icon = computed(() =>
  props.capability.kind === "audio_capture" ? "MIC" : "SPK",
);

const statusTone = computed(() => {
  if (props.capability.bindingState === "failed") return "danger" as const;
  if (props.capability.bindingState === "offline") return "warning" as const;
  if (props.capability.active) return "success" as const;
  return "neutral" as const;
});

const latency = computed(() => {
  const value = props.capability.metrics.estimatedLatencyMs;
  return value === null ? "—" : `${value.toFixed(1)} ms`;
});

const loss = computed(() => {
  const value = props.capability.metrics.packetLossPercent;
  return value === null ? "—" : `${value.toFixed(2)}%`;
});

const buffer = computed(() => {
  const value = props.capability.metrics.bufferFillMs;
  return value === null ? "—" : `${value.toFixed(0)} ms`;
});
</script>

<template>
  <article class="capability-card" :class="{ 'capability-card--active': capability.active }">
    <header class="capability-card__header">
      <div class="capability-card__identity">
        <div class="capability-card__icon" aria-hidden="true">{{ icon }}</div>
        <div>
          <p class="eyebrow">{{ title }}</p>
          <h3>{{ capability.displayName }}</h3>
        </div>
      </div>
      <StatusPill :tone="statusTone" :label="capability.bindingState" />
    </header>

    <dl class="capability-card__facts">
      <div>
        <dt>Profile</dt>
        <dd>{{ capability.profile }}</dd>
      </div>
      <div>
        <dt>Projection</dt>
        <dd>{{ capability.projectionKind ?? "not applicable" }}</dd>
      </div>
      <div>
        <dt>Permission</dt>
        <dd>{{ capability.permissionRequirement }}</dd>
      </div>
      <div>
        <dt>Format</dt>
        <dd>{{ capability.formatSummary ?? "not negotiated" }}</dd>
      </div>
    </dl>

    <div class="metric-grid">
      <MetricTile label="Estimated latency" :value="latency" detail="demo metric" />
      <MetricTile label="Packet loss" :value="loss" detail="demo metric" />
      <MetricTile label="Buffer" :value="buffer" detail="target fill" />
    </div>

    <div class="capability-card__footer">
      <div>
        <span class="capability-card__qos-label">QoS</span>
        <div class="tag-list">
          <span v-for="mode in capability.qosModes" :key="mode" class="tag">{{ mode }}</span>
        </div>
      </div>
      <button
        class="primary-button"
        :class="{ 'primary-button--stop': capability.active }"
        type="button"
        :disabled="busy"
        @click="emit('toggle', capability)"
      >
        {{ busy ? "处理中…" : capability.active ? "停止映射" : "启动映射" }}
      </button>
    </div>
  </article>
</template>
