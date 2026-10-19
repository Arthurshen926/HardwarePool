<script setup lang="ts">
const statusLabels: Record<string, string> = {
  active: "运行中", idle: "未启动", stopped: "已停止", starting: "启动中", stopping: "停止中",
  failed: "故障", blocked: "暂不可用", offline: "离线", available: "可用", busy: "使用中",
  permission_required: "需要授权", connected: "已连接", connecting: "连接中", listening: "等待连接",
  awaiting_consent: "等待允许", ready: "已就绪", disconnected: "未连接", running: "运行中",
  simulated: "模拟", "deterministic fixture": "模拟数据",
  unsupported: "不支持", unavailable: "不可用", online: "在线",
};
defineProps<{
  tone: "success" | "warning" | "neutral" | "danger";
  label: string;
}>();
</script>

<template>
  <span class="status-pill" :class="`status-pill--${tone}`">{{ statusLabels[label] ?? label }}</span>
</template>
