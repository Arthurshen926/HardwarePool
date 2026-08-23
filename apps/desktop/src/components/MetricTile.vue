<script setup lang="ts">
defineProps<{
  label: string;
  value: string;
  detail?: string;
}>();
</script>

<template>
  <div class="metric-tile">
    <span class="metric-tile__label">{{ label }}</span>
    <strong class="metric-tile__value">{{ value }}</strong>
    <small v-if="detail" class="metric-tile__detail">{{ detail }}</small>
  </div>
</template>
