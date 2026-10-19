<script setup lang="ts">
import { computed } from "vue";

import type { QuickActionOperation, UiAudioEndpointCatalog, UiQuickAction } from "../lib/types";
import StatusPill from "./StatusPill.vue";
import { actionPresentation } from "../lib/resource-navigation";

const props = defineProps<{ action: UiQuickAction; busy: boolean; audioEndpoints: UiAudioEndpointCatalog | null }>();
const emit = defineEmits<{
  invoke: [action: UiQuickAction, operation: QuickActionOperation];
  refreshEndpoints: [];
  selectEndpoint: [action: UiQuickAction, selectionToken: string];
}>();

const tone = computed(() => {
  if (props.action.status === "active") return "success" as const;
  if (["offline", "failed", "blocked"].includes(props.action.status)) return "danger" as const;
  if (["starting", "stopping"].includes(props.action.status)) return "warning" as const;
  return "neutral" as const;
});

const isMicrophoneAction = computed(
  () => props.action.id === "capyio.quick-action.remote-microphone",
);
const isSpeakerAction = computed(
  () => props.action.id === "capyio.quick-action.remote-speaker",
);
const evidenceLabel = computed(() => ({
  not_started: "尚无数据到达证明",
  process_and_route_state: "进程及连接状态已确认",
  stable_tcp_receiver_presence: "接收端连接已确认",
  stable_phone_tcp_presence: "手机连接已确认",
  native_receiver_process_ready_only: "原生接收进程已就绪",
})[props.action.evidenceLevel]);

const actionIcon = computed(() => {
  if (props.action.id.includes("microphone")) return "MIC";
  if (props.action.id.includes("imu")) return "IMU";
  if (props.action.id.includes("gamepad")) return "PAD";
  if (props.action.id.includes("camera")) return "CAM";
  if (props.action.id.includes("touchpad")) return "PAD";
  return "AUD";
});

function operationLabel(operation: QuickActionOperation): string {
  if (operation === "retry") return "重试";
  if (operation === "stop") return props.action.id === "capyio.quick-action.android-touchpad"
    && props.action.status === "failed" ? "清除错误" : "停止";
  return "启动";
}

function chooseEndpoint(event: Event): void {
  const selectionToken = (event.target as HTMLSelectElement).value;
  if (selectionToken) emit("selectEndpoint", props.action, selectionToken);
}
</script>

<template>
  <article class="capability-card quick-action-card" :aria-labelledby="`${action.id}-title`">
    <header class="capability-card__header">
      <div class="capability-card__identity">
        <div class="capability-card__icon" aria-hidden="true">{{ actionIcon }}</div>
        <div><p class="eyebrow">{{ action.kind === 'capability_control' ? '本机功能' : '连接功能' }}</p><h3 :id="`${action.id}-title`">{{ actionPresentation(action).title }}</h3></div>
      </div>
      <StatusPill :tone="tone" :label="action.status" />
    </header>
    <p>{{ actionPresentation(action).summary }}</p>
    <dl class="capability-card__facts">
      <div v-if="action.connectionHint"><dt>提示</dt><dd>{{ action.connectionHint }}</dd></div>
      <div v-if="isMicrophoneAction"><dt>Windows 端</dt><dd>在录音或会议应用中选择 CapyIO 麦克风输入；当前实验室驱动也可能显示为“麦克风 (CapyIO Speaker with Render Bridge)”。</dd></div>
    </dl>
    <details><summary>运行诊断</summary><p>状态 {{ action.routeState ?? action.status }}<span v-if="action.routeEpoch !== null"> · 运行代次 {{ action.routeEpoch }}</span> · {{ evidenceLabel }}</p><p>启动状态不代表远端已收到数据或系统应用已可用。</p></details>
    <div v-if="isSpeakerAction && audioEndpoints?.supported" class="audio-endpoint-picker">
      <label :for="`${action.id}-endpoint`">Windows 播放设备（本次运行）</label>
      <div class="audio-endpoint-picker__controls">
        <select :id="`${action.id}-endpoint`" :value="audioEndpoints.choices.find((choice) => choice.selected)?.selectionToken ?? ''" :disabled="busy || !audioEndpoints.canSelect || audioEndpoints.choices.length === 0" @change="chooseEndpoint">
          <option value="">请选择当前播放设备</option>
          <option v-for="choice in audioEndpoints.choices" :key="choice.selectionToken" :value="choice.selectionToken">{{ choice.displayName }}{{ choice.isDefault ? "（默认）" : "" }}</option>
        </select>
        <button class="ghost-button" type="button" :disabled="busy || !audioEndpoints.canSelect" @click="emit('refreshEndpoints')">重新扫描</button>
      </div>
      <small v-if="!audioEndpoints.canSelect && audioEndpoints.supported">请先停止音频镜像，再切换设备。</small>
      <small v-if="audioEndpoints.problem" class="audio-endpoint-picker__problem">{{ audioEndpoints.problem }}</small>
    </div>
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
