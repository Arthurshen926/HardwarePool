<script setup lang="ts">
import { computed } from "vue";

import type { QuickActionOperation, UiQuickAction } from "../lib/types";
import StatusPill from "./StatusPill.vue";

const props = defineProps<{ action: UiQuickAction; busy: boolean }>();
const emit = defineEmits<{ invoke: [action: UiQuickAction, operation: QuickActionOperation] }>();

const tone = computed(() => {
  if (props.action.status === "active") return "success" as const;
  if (["offline", "failed", "blocked"].includes(props.action.status)) return "danger" as const;
  if (["starting", "stopping"].includes(props.action.status)) return "warning" as const;
  return "neutral" as const;
});

function operationLabel(operation: QuickActionOperation): string {
  if (operation === "retry") return "重试";
  if (operation === "stop") return "停止";
  return "启动";
}
</script>

<template>
  <article class="capability-card quick-action-card" :aria-labelledby="`${action.id}-title`">
    <header class="capability-card__header">
      <div class="capability-card__identity">
        <div class="capability-card__icon" aria-hidden="true">AUD</div>
        <div><p class="eyebrow">Quick Action schema v{{ action.schemaVersion }}</p><h3 :id="`${action.id}-title`">{{ action.title }}</h3></div>
      </div>
      <StatusPill :tone="tone" :label="action.status" />
    </header>
    <p>{{ action.summary }}</p>
    <dl class="capability-card__facts">
      <div><dt>Runtime</dt><dd>{{ action.routeState ?? "not installed" }}<template v-if="action.routeEpoch !== null"> · epoch {{ action.routeEpoch }}</template></dd></div>
      <div><dt>Evidence</dt><dd>{{ action.evidenceLevel }}</dd></div>
    </dl>
    <p v-if="action.problem" class="error-banner" role="status"><strong v-if="action.problemCode">{{ action.problemCode }} · </strong>{{ action.problem }}</p>
    <div class="capability-card__footer">
      <span class="capability-card__qos-label">{{ action.simulated ? "模拟投影" : "真实宿主边界" }}</span>
      <div class="quick-action-card__buttons">
        <button v-for="operation in action.availableOperations" :key="operation" class="primary-button" :class="{ 'primary-button--stop': operation === 'stop' }" type="button" :disabled="busy" @click="emit('invoke', action, operation)">
          {{ busy ? "处理中…" : operationLabel(operation) }}
        </button>
      </div>
    </div>
  </article>
</template>
