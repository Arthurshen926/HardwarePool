<script setup lang="ts">
import ResourceIcon from "./ResourceIcon.vue";
import StatusPill from "./StatusPill.vue";
import type { UiCapability } from "../lib/types";
import type { UiQuickAction } from "../lib/types";
import { actionPresentation } from "../lib/resource-navigation";
import { remoteResourceUseHint } from "../lib/remote-resource-support";
import alphaScope from "../../../../release/alpha-scope.json";

withDefaults(defineProps<{
  catalogId: string;
  eyebrow: string;
  title: string;
  description: string;
  capabilities: UiCapability[];
  busyId: string | null;
  actionBindings?: Record<string, UiQuickAction>;
}>(), { actionBindings: () => ({}) });

const emit = defineEmits<{
  toggle: [capability: UiCapability, selected: boolean];
  open: [action: UiQuickAction];
  inspect: [capability: UiCapability];
}>();

function toggle(capability: UiCapability, event: Event): void {
  emit("toggle", capability, (event.target as HTMLInputElement).checked);
}

function usageLabel(capability: UiCapability): string {
  const directions = new Set(capability.ports.map((port) => port.direction));
  if (capability.local) {
    if (capability.capabilityClass === 'speaker') return '可共享的本机播放设备';
    return directions.has("source") ? "可从本机共享" : "本机接收 / 投影目标";
  }
  return remoteResourceUseHint(capability)!.label;
}

function actionLabel(action: UiQuickAction): string {
  return ['active', 'starting', 'stopping'].includes(action.status) ? '管理使用' : '打开使用';
}

function inspectLabel(capability: UiCapability): string {
  return capability.local ? '查看与设置' : '选择使用';
}

function tone(capability: UiCapability): "success" | "warning" | "danger" {
  if (capability.availability === "offline") return "danger";
  if (capability.availability === "available") return "success";
  return "warning";
}
</script>

<template>
  <section class="capability-catalog" :aria-labelledby="`${catalogId}-title`">
    <header class="capability-catalog__header">
      <div>
        <p class="eyebrow">{{ eyebrow }}</p>
        <h2 :id="`${catalogId}-title`">{{ title }}</h2>
      </div>
      <p>{{ description }}</p>
    </header>

    <div v-if="capabilities.length" class="hardware-grid">
      <article v-for="capability in capabilities" :key="capability.id" class="hardware-card" :class="{ 'hardware-card--shared': capability.selectedForSharing }">
        <header>
          <span class="hardware-card__icon"><ResourceIcon :kind="capability.capabilityClass" /></span>
          <div>
            <h3>{{ actionBindings[capability.id] ? actionPresentation(actionBindings[capability.id]!).title : capability.displayName }}</h3>
            <span v-if="alphaScope.experimentalCapabilities.includes(capability.capabilityClass)" class="hardware-card__note">实验功能</span>
            <p>{{ capability.nodeName }} · {{ usageLabel(capability) }}</p>
          </div>
          <StatusPill :tone="tone(capability)" :label="capability.availability" />
        </header>

        <details v-if="!capability.local" class="hardware-card__help"><summary>使用说明</summary><p class="hardware-card__note">{{ remoteResourceUseHint(capability)!.detail }}</p></details>

        <template v-if="actionBindings[capability.id]">
          <p>{{ actionPresentation(actionBindings[capability.id]!).summary }}</p>
          <p>运行状态 <StatusPill :tone="actionBindings[capability.id]!.status === 'active' ? 'success' : 'neutral'" :label="actionBindings[capability.id]!.status" /></p>
          <button class="primary-button" type="button" :aria-label="`${actionPresentation(actionBindings[capability.id]!).title}：${actionLabel(actionBindings[capability.id]!)}`" @click="emit('open', actionBindings[capability.id]!)">{{ actionLabel(actionBindings[capability.id]!) }} →</button>
        </template>
        <button v-else class="ghost-button" type="button" :aria-label="`${capability.displayName}：${inspectLabel(capability)}`" @click="emit('inspect', capability)">{{ inspectLabel(capability) }} →</button>

        <details><summary>技术信息</summary>
        <ul class="port-list" aria-label="Capability ports">
          <li v-for="port in capability.ports" :key="port.id">
            <span class="port-direction" :class="`port-direction--${port.direction}`">{{ port.direction }}</span>
            <div><strong>{{ port.displayName }}</strong><small>{{ port.profile }} · {{ port.formats.join(", ") }}</small></div>
          </li>
        </ul>
        </details>

        <footer>
          <label v-if="capability.selectableForSharing" class="share-toggle">
            <input
              type="checkbox"
              class="share-toggle__input"
              :aria-label="`${capability.displayName}：${capability.selectedForSharing ? '已选为共享资源' : '选择共享此资源'}`"
              :checked="capability.selectedForSharing"
              :disabled="busyId === capability.id"
              @change="toggle(capability, $event)"
            >
            <span class="share-toggle__track" aria-hidden="true"></span>
            <span>{{ capability.selectedForSharing ? "已选为共享资源" : "选择共享此资源" }}</span>
          </label>
          <span v-else class="hardware-card__note">{{ capability.local ? '此资源当前不可选择共享；请检查设备是否可用及应用版本。' : '远程资源的使用需要单独授权' }}</span>
          <span v-if="capability.permissionRequirement !== 'none'" class="hardware-card__permission">{{ { user_confirmation: '启动时需用户授权', foreground_service: '使用时保留后台通知', privileged: '需要系统权限' }[capability.permissionRequirement] }}</span>
          <span v-if="capability.simulated" class="hardware-card__fixture">演示数据</span>
        </footer>
      </article>
    </div>
    <p v-else class="empty-state">当前没有可展示的真实资源。</p>
  </section>
</template>
