<script setup lang="ts">
defineProps<{
  tone: "success" | "warning" | "neutral" | "danger";
  label: string;
}>();
</script>

<template>
  <span class="status-pill" :class="`status-pill--${tone}`">{{ label }}</span>
</template>
