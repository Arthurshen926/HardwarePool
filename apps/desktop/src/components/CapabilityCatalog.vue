<script setup lang="ts">
import StatusPill from "./StatusPill.vue";
import type { UiCapability } from "../lib/types";

defineProps<{
  catalogId: string;
  eyebrow: string;
  title: string;
  description: string;
  capabilities: UiCapability[];
  busyId: string | null;
}>();

const emit = defineEmits<{
  toggle: [capability: UiCapability, selected: boolean];
}>();

function toggle(capability: UiCapability, event: Event): void {
  emit("toggle", capability, (event.target as HTMLInputElement).checked);
}

function usageLabel(capability: UiCapability): string {
  const directions = new Set(capability.ports.map((port) => port.direction));
  if (capability.local) {
    return directions.has("source") ? "可从本机共享" : "本机接收 / 投影目标";
  }
  if (directions.has("source") && directions.has("sink")) return "可使用，也可路由到此处";
  if (directions.has("source")) return "可从远端使用";
  if (directions.has("sink")) return "可将本机能力路由到此处";
  return "远端控制能力";
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
      <article v-for="capability in capabilities" :key="capability.id" class="hardware-card">
        <header>
          <div>
            <span class="hardware-card__class">{{ capability.capabilityClass }}</span>
            <h3>{{ capability.displayName }}</h3>
            <p>{{ capability.nodeName }} · {{ usageLabel(capability) }}</p>
          </div>
          <StatusPill :tone="tone(capability)" :label="capability.availability" />
        </header>

        <ul class="port-list" aria-label="Capability ports">
          <li v-for="port in capability.ports" :key="port.id">
            <span class="port-direction" :class="`port-direction--${port.direction}`">{{ port.direction }}</span>
            <div><strong>{{ port.displayName }}</strong><small>{{ port.profile }} · {{ port.formats.join(", ") }}</small></div>
          </li>
        </ul>

        <footer>
          <label v-if="capability.selectableForSharing" class="share-toggle">
            <input
              type="checkbox"
              :checked="capability.selectedForSharing"
              :disabled="busyId === capability.id"
              @change="toggle(capability, $event)"
            >
            <span>{{ capability.selectedForSharing ? "已加入共享清单" : "加入可发布能力清单" }}</span>
          </label>
          <span v-else class="hardware-card__note">{{ capability.local ? "方向由 Sink / Control Port 决定" : "目录只读；建立 Route 时再单独授权" }}</span>
          <span v-if="capability.permissionRequirement !== 'none'" class="hardware-card__permission">{{ capability.permissionRequirement }}</span>
          <span v-if="capability.simulated" class="hardware-card__fixture">fixture</span>
        </footer>
      </article>
    </div>
    <p v-else class="empty-state">尚未发现能力。节点上线并完成目录交换后会显示在这里。</p>
  </section>
</template>
