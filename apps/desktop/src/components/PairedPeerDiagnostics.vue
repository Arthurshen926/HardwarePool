<script setup lang="ts">
import { computed } from "vue";
import type { UiPairedPeer } from "../lib/types";

const props = defineProps<{ peer?: UiPairedPeer }>();
const pairingTime = computed(() => props.peer?.pairedAtMs === undefined
  ? null : new Date(props.peer.pairedAtMs).toISOString());
</script>

<template>
  <div v-if="peer" class="paired-peer-diagnostics">
    <p>配对时间：<time v-if="pairingTime" :datetime="pairingTime">{{ pairingTime.replace("T", " ").replace("Z", " UTC") }}</time><span v-else>旧版未提供，请更新应用后查看</span></p>
    <p>请对照两端此设备的配对时间。若不同，请更新配对并在对端导入同一份新邀请；时间相同仍需通过证书校验。</p>
  </div>
</template>
