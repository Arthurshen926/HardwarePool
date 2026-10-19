<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import type { CapyIOApi, UiAndroidAudioState, UiAndroidAudioPeerSettings } from "../lib/types";

const props = withDefaults(defineProps<{ api: CapyIOApi; state: UiAndroidAudioState; initialDirection?: "microphone" | "speaker"; expanded?: boolean; suggestedHost?: string }>(), { suggestedHost: "" });
const direction = ref<"microphone" | "speaker">(props.initialDirection ?? "microphone");
const host = ref(props.suggestedHost);
const localPort = ref(direction.value === "microphone" ? 40010 : 40000);
const peerPort = ref(direction.value === "microphone" ? 40011 : 40001);
const confirmed = ref(false);
const busy = ref(false);
const message = ref("");
const reading = ref(false);
const loaded = ref(false);
const readError = ref("");
const savedSettings = ref<UiAndroidAudioPeerSettings | null>(null);
let readGeneration = 0;
const running = computed(() => !["stopped", "failed"].includes(props.state[direction.value].state));
async function readSaved(): Promise<boolean> {
  const generation = ++readGeneration;
  reading.value = true;
  loaded.value = false;
  readError.value = "";
  savedSettings.value = null;
  host.value = props.suggestedHost;
  localPort.value = direction.value === "microphone" ? 40010 : 40000;
  peerPort.value = direction.value === "microphone" ? 40011 : 40001;
  confirmed.value = false;
  try {
    const value = await props.api.getAndroidAudioPeerSettings(direction.value);
    if (generation !== readGeneration) return false;
    savedSettings.value = value;
    loaded.value = true;
    if (value?.savedPeer) {
      host.value = value.savedPeer.host;
      localPort.value = value.savedPeer.localPort;
      peerPort.value = value.savedPeer.peerPort;
    }
    return value !== null;
  } catch {
    if (generation === readGeneration) readError.value = "读取失败，请重试；当前表单不代表已保存配置。";
    return false;
  } finally {
    if (generation === readGeneration) reading.value = false;
  }
}
watch(direction, () => {
  message.value = "";
  void readSaved();
});
watch(() => props.suggestedHost, (next) => {
  if (!savedSettings.value?.savedPeer && !confirmed.value) host.value = next;
});
onMounted(() => { void readSaved(); });
onUnmounted(() => { readGeneration++; });
async function save() {
  if (busy.value || reading.value || running.value || !confirmed.value) return;
  busy.value = true;
  message.value = "";
  try {
    const expected = { host: host.value.trim(), localPort: localPort.value, peerPort: peerPort.value };
    await props.api.configureAndroidAudioPeer(direction.value, expected.host, expected.localPort, expected.peerPort, confirmed.value);
    const verified = await readSaved();
    const saved = savedSettings.value?.savedPeer;
    message.value = verified && saved && saved.host === expected.host && saved.localPort === expected.localPort && saved.peerPort === expected.peerPort
      ? "已保存并回读确认。请手动启动此功能；另一方向未改变，Windows 需使用匹配端口。"
      : "保存请求已成功，但尚未完成回读确认；请重新读取并核对后使用。";
  } catch (error) { message.value = error instanceof Error ? error.message : String(error); }
  finally { busy.value = false; }
}
</script>

<template>
  <details class="info-panel" :open="expanded">
    <summary>音频连接配置（可信网络实验）</summary>
    <p>新设备无需重编 APK。配置按方向保存，仅在本机使用；此操作不建立配对、不启动 Windows 服务，也不证明远端已连接。</p>
    <p v-if="reading" role="status">正在读取已保存配置…</p>
    <p v-else-if="readError" role="alert">{{ readError }}</p>
    <p v-else-if="loaded && savedSettings?.savedPeer">已读取此方向保存的配置。保存配置不代表网络已连通。</p>
    <p v-else-if="loaded && savedSettings">此方向尚未保存配置；可能仍使用构建时实验配置。下方端口是示例，请与 Windows 核对。</p>
    <p v-else-if="loaded">此后端不支持配置回读，请更新 APK。下方不是已保存配置。</p>
    <p v-else>尚未读取配置。</p>
    <button class="ghost-button" type="button" :disabled="busy || reading" @click="readSaved">重新读取（放弃未保存修改）</button>
    <form class="audio-peer-form" @submit.prevent="save">
      <label>配置方向
        <select v-model="direction" :disabled="busy">
          <option value="microphone">本机麦克风 → Windows</option>
          <option value="speaker">Windows → 本机扬声器</option>
        </select>
      </label>
      <label>Windows 私有或覆盖网络 IPv4
        <input v-model="host" required maxlength="15" inputmode="decimal" autocomplete="off" placeholder="例如 192.168.1.20" :disabled="busy || reading || running" />
      </label>
      <label>本机 UDP 端口<input v-model.number="localPort" type="number" min="1" max="65535" required :disabled="busy || reading || running" /></label>
      <label>Windows UDP 端口<input v-model.number="peerPort" type="number" min="1" max="65535" required :disabled="busy || reading || running" /></label>
      <label><input v-model="confirmed" type="checkbox" :disabled="busy" />我确认只在可信网络中测试；当前音频协议未实现相互认证与加密。</label>
      <p v-if="running">请先停止当前选择的音频功能，再修改连接。</p>
      <button type="submit" :disabled="busy || reading || running || !confirmed">{{ busy ? "保存中…" : "保存此方向配置" }}</button>
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
