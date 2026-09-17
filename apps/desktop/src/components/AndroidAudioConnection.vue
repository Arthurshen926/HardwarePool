<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type { CapyIOApi, UiAndroidAudioState } from "../lib/types";

const props = defineProps<{ api: CapyIOApi; state: UiAndroidAudioState }>();
const direction = ref<"microphone" | "speaker">("microphone");
const host = ref("");
const localPort = ref(40010);
const peerPort = ref(40011);
const confirmed = ref(false);
const busy = ref(false);
const message = ref("");
const running = computed(() => !["stopped", "failed"].includes(props.state[direction.value].state));
watch(direction, value => {
  localPort.value = value === "microphone" ? 40010 : 40000;
  peerPort.value = value === "microphone" ? 40011 : 40001;
  message.value = "";
});
async function save() {
  if (busy.value || running.value || !confirmed.value) return;
  busy.value = true;
  message.value = "";
  try {
    await props.api.configureAndroidAudioPeer(direction.value, host.value.trim(), localPort.value, peerPort.value, confirmed.value);
    message.value = "已保存此方向的连接配置；请手动启动。另一方向未改变。Windows 接收/发送端必须使用匹配端口。";
  } catch (error) { message.value = error instanceof Error ? error.message : String(error); }
  finally { busy.value = false; }
}
</script>

<template>
  <details class="info-panel">
    <summary>音频连接配置（可信网络实验）</summary>
    <p>新设备无需重编 APK。配置按方向保存，仅在本机使用；此操作不建立配对、不启动 Windows 服务，也不证明远端已连接。</p>
    <form class="audio-peer-form" @submit.prevent="save">
      <label>配置方向
        <select v-model="direction" :disabled="busy">
          <option value="microphone">本机麦克风 → Windows</option>
          <option value="speaker">Windows → 本机扬声器</option>
        </select>
      </label>
      <label>Windows 私有或覆盖网络 IPv4
        <input v-model="host" required maxlength="15" inputmode="decimal" autocomplete="off" placeholder="例如 192.168.1.20" :disabled="busy || running" />
      </label>
      <label>本机 UDP 端口<input v-model.number="localPort" type="number" min="1" max="65535" required :disabled="busy || running" /></label>
      <label>Windows UDP 端口<input v-model.number="peerPort" type="number" min="1" max="65535" required :disabled="busy || running" /></label>
      <label><input v-model="confirmed" type="checkbox" :disabled="busy" />我确认只在可信网络中测试；当前音频协议未实现相互认证与加密。</label>
      <p v-if="running">请先停止当前选择的音频功能，再修改连接。</p>
      <button type="submit" :disabled="busy || running || !confirmed">{{ busy ? "保存中…" : "保存此方向配置" }}</button>
      <p role="status" aria-live="polite">{{ message }}</p>
    </form>
  </details>
</template>

<style scoped>
.audio-peer-form { display: grid; gap: 0.8rem; max-width: 42rem; margin-top: 1rem; }
label { display: grid; gap: 0.3rem; }
input, select, button { font: inherit; padding: 0.6rem; min-width: 0; }
input[type="checkbox"] { justify-self: start; }
</style>
