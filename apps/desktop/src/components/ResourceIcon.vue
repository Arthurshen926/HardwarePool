<script setup lang="ts">
defineProps<{ kind?: string }>();
const paths: Record<string, string> = {
  microphone: 'M12 3a3 3 0 0 0-3 3v6a3 3 0 0 0 6 0V6a3 3 0 0 0-3-3ZM5 11v1a7 7 0 0 0 14 0v-1M12 19v3M8 22h8',
  speaker: 'M11 5 6 9H3v6h3l5 4V5ZM15 8a6 6 0 0 1 0 8M18 5a10 10 0 0 1 0 14',
  camera: 'M8 5 6 8H3v12h18V8h-3l-2-3H8ZM16 14a4 4 0 1 1-8 0 4 4 0 0 1 8 0Z',
  touchpad: 'M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2ZM3 16h18M12 16v5',
  display: 'M3 4h18v13H3V4ZM12 17v4M8 21h8',
  gamepad: 'M7 7h10c2 0 3 2 4 5l1 5c0 3-3 3-5 0H7c-2 3-5 3-5 0l1-5c1-3 2-5 4-5ZM7 10v5M4.5 12.5h5M16 11h.01M19 14h.01',
  device: 'M7 2h10v20H7V2ZM10 18h4',
};
</script>
<template><svg class="resource-icon" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path :d="paths[kind ?? 'device'] ?? paths.device" /></svg></template>
