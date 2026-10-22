<script setup lang="ts">
import ResourceIcon from './ResourceIcon.vue';
import { computed, onMounted, onUnmounted, ref } from 'vue';
import type { AudioUseDirection, CapyIOApi, UiAudioPeer, UiAudioUse, UiAndroidMicrophoneSettings } from '../lib/types';
const props = defineProps<{ api: CapyIOApi; direction?: AudioUseDirection; preferredPeer?: string }>();
const peers = ref<UiAudioPeer[]>([]);
const uses = ref<UiAudioUse[]>([]);
const selected = ref<Record<AudioUseDirection, string>>({ microphone: '', 'microphone-secure': '', speaker: '' });
const errors = ref<Record<AudioUseDirection, string>>({ microphone: '', 'microphone-secure': '', speaker: '' });
const busy = ref<AudioUseDirection | null>(null);
const readError = ref('');
const loaded = ref(false);
const microphoneSettings = ref<UiAndroidMicrophoneSettings | null>(null);
const profileBusy = ref(false);
const profileError = ref('');
const microphoneDirection = computed<AudioUseDirection>(() => microphoneSettings.value?.captureProfile === 'secure_mono_tls13_v1' ? 'microphone-secure' : 'microphone');
let polling = false;
let readGeneration = 0;
let disposed = false;
let timer: ReturnType<typeof setInterval> | undefined;
const directions = computed<AudioUseDirection[]>(() => props.direction ? [props.direction !== 'speaker' ? microphoneDirection.value : props.direction] : [microphoneDirection.value, 'speaker']);
const statusLabels = { idle: '未使用', awaiting_consent: '等待设备允许', starting: '正在准备音频', active: '音频使用中', stopping: '正在停止', failed: '音频连接失败' };
function state(direction: AudioUseDirection) { return uses.value.find(use => use.direction === direction); }
function statusLabel(direction: AudioUseDirection) {
  const use = state(direction);
  if (!use) return loaded.value ? '未使用' : '尚未读取';
  if (use.message) return use.message;
  if (use.waitingMedia && use.status === 'starting') return '音频已准备，等待声音数据；尚未确认播放';
  return statusLabels[use.status];
}
function running(direction: AudioUseDirection) { const use = state(direction); return !!use && !['idle', 'failed'].includes(use.status); }
function candidates(direction: AudioUseDirection) { return peers.value.filter(peer => peer.directions.includes(direction)); }
function unavailable(direction: AudioUseDirection) {
  return peers.value.flatMap(peer => (peer.unavailableDirections ?? [])
    .filter(item => item.direction === direction)
    .map(item => ({ nodeId: peer.nodeId, displayName: peer.displayName, reason: item.reason })));
}
async function refresh(force = false) {
  if (disposed || (!force && (polling || busy.value || profileBusy.value))) return;
  const generation = ++readGeneration;
  polling = true;
  try {
    const [nextPeers, nextUses, nextSettings] = await Promise.all([props.api.getAudioPeers(), props.api.getAudioUses(), props.api.getAndroidMicrophoneSettings()]);
    if (generation !== readGeneration) return;
    peers.value = nextPeers; uses.value = nextUses; microphoneSettings.value = nextSettings; readError.value = ''; loaded.value = true;
    for (const direction of directions.value) {
      const use = state(direction);
      const choices = candidates(direction);
      if (running(direction) && use) selected.value[direction] = use.peerNodeId;
      else if (!choices.some(peer => peer.nodeId === selected.value[direction])) selected.value[direction] = props.preferredPeer ? (choices.find(peer => peer.nodeId === props.preferredPeer)?.nodeId ?? '') : (choices.length === 1 ? choices[0].nodeId : '');
    }
  } catch (error) { if (generation === readGeneration) { peers.value = []; readError.value = String(error); } }
  finally { if (generation === readGeneration) polling = false; }
}
async function change(direction: AudioUseDirection) {
  if (disposed || busy.value || profileBusy.value) return;
  readGeneration++;
  polling = false;
  busy.value = direction; errors.value[direction] = '';
  try {
    if (running(direction)) await props.api.stopAudioUse(direction);
    else await props.api.startAudioUse(selected.value[direction], direction);
    await refresh(true);
  } catch (error) { if (!disposed) errors.value[direction] = String(error); }
  finally { if (!disposed) busy.value = null; }
}
async function selectProfile(event: Event) {
  const settings = microphoneSettings.value;
  if (!settings || disposed || busy.value || profileBusy.value) return;
  const value = (event.target as HTMLSelectElement).value;
  if (value !== 'legacy' && value !== 'secure_mono_tls13_v1') return;
  readGeneration++; polling = false; profileBusy.value = true; profileError.value = '';
  try {
    const next = await props.api.selectAndroidMicrophoneProfile(value, settings.revision);
    if (disposed) return;
    microphoneSettings.value = next;
    selected.value.microphone = ''; selected.value['microphone-secure'] = '';
    await refresh(true);
  } catch (error) { if (!disposed) { profileError.value = String(error); await refresh(true); } }
  finally { if (!disposed) profileBusy.value = false; }
}
function secureStartBlocked(direction: AudioUseDirection) {
  return direction === 'microphone-secure' && (!microphoneSettings.value?.permissionReady
    || microphoneSettings.value.eligibility !== 'eligible');
}
function resumed() { if (!document.hidden) void refresh(); }
onMounted(() => { void refresh(); timer = setInterval(resumed, 1500); document.addEventListener('visibilitychange', resumed); });
onUnmounted(() => { disposed = true; readGeneration++; if (timer) clearInterval(timer); document.removeEventListener('visibilitychange', resumed); });
</script>

<template>
  <section class="info-panel use-panel" aria-label="跨设备音频">
    <header class="use-panel__heading"><span class="use-panel__icon"><ResourceIcon kind="speaker" /></span><div><p class="eyebrow">声音，在设备间流动</p><h2>跨设备音频</h2></div></header>
    <p>选择已连接节点并开始，自动准备两端；无需在另一端再次启动。麦克风与扬声器分别授权、分别停止。</p>
    <p v-if="!direction || direction === 'microphone'">开始使用麦克风会同时开启本机麦克风资源共享。</p>
    <div v-if="microphoneSettings && (!direction || direction !== 'speaker')" class="use-panel__resource">
      <label>麦克风传输配置
        <select :value="microphoneSettings.captureProfile" :disabled="!!busy || profileBusy" @change="selectProfile">
          <option value="legacy">Legacy：可信实验网络 UDP（明文）</option>
          <option value="secure_mono_tls13_v1">实验性安全单声道：固定 TLS 1.3</option>
        </select>
      </label>
      <p>切换会停止当前麦克风会话并取消共享，必须重新开始和授权。来源固定为系统默认麦克风；采集中来源变化会终止会话。</p>
      <p v-if="microphoneSettings.captureProfile === 'secure_mono_tls13_v1'">实验性 48 kHz / S16LE / 单声道，单次最多 60 秒并受原始授权期限限制。必须保持应用可见。失败不会退回 UDP；不支持安全扬声器或安全双工。这里只表示具备尝试条件，不保证真机录音、低延迟或远端播放。Linux 生产启动器的端到端接入尚未完成。</p>
      <p v-if="microphoneSettings.captureProfile === 'secure_mono_tls13_v1' && !microphoneSettings.permissionReady" role="status">系统录音或通知权限尚未授予。请先在 Android 应用权限中允许录音和通知；此配置不会自动弹出权限请求。</p>
      <p v-if="profileError" role="alert">{{ profileError }}</p>
    </div>
    <p v-if="!loaded && !readError" role="status">正在读取设备与使用状态…</p>
    <div v-if="readError" class="use-panel__empty">
      <p role="alert">暂时无法更新设备与使用状态，不能据此判断设备是否离线。{{ loaded ? '下方保留上次读取的使用状态。' : '' }}</p>
      <button type="button" class="ghost-button" :disabled="!!busy" @click="refresh()">重新读取状态</button>
      <details><summary>查看技术原因</summary><p>{{ readError }}</p></details>
    </div>
    <div class="use-panel__resources">
    <article v-for="direction in directions" :key="direction" class="audio-use-row use-panel__resource" :class="{ 'use-panel__resource--active': state(direction)?.status === 'active' }">
      <h3>{{ direction !== 'speaker' ? '把本机麦克风提供给电脑' : '用本机扬声器播放电脑声音' }}</h3>
      <div class="use-panel__controls"><label>选择设备 <select v-model="selected[direction]" :disabled="!!busy || profileBusy || running(direction)"><option value="">请选择已连接设备</option><option v-for="peer in candidates(direction)" :key="peer.nodeId" :value="peer.nodeId">{{ peer.displayName }}</option></select></label>
      <button :class="running(direction) ? 'danger-button' : 'primary-button'" :disabled="!!busy || profileBusy || state(direction)?.status === 'stopping' || (!running(direction) && (secureStartBlocked(direction) || !candidates(direction).some(peer => peer.nodeId === selected[direction])))" @click="change(direction)">{{ busy === direction ? '正在提交…' : running(direction) ? state(direction)?.status === 'awaiting_consent' ? '取消申请' : '停止使用' : '开始使用' }}</button></div>
      <p class="use-panel__status" :data-state="state(direction)?.status ?? 'idle'" role="status">{{ statusLabel(direction) }}<span v-if="errors[direction] || state(direction)?.error"> · {{ errors[direction] || state(direction)?.error }}</span></p>
      <p v-if="loaded && !readError && !candidates(direction).length && !running(direction)" class="use-panel__empty">还没有可用的设备。请先连接电脑，并确认它支持此项音频功能。</p>
      <p v-for="peer in unavailable(direction)" :key="peer.nodeId" class="use-panel__empty">{{ peer.displayName }}：{{ peer.reason }}</p>
      <p v-if="direction === 'microphone-secure'">接收端必须明确支持此安全配置并独立授权。当前不提供 Windows 安全音频或 Linux 生产启动器的完整连接流程，不提供虚拟麦克风。</p>
      <p v-else>{{ direction !== 'speaker' ? '首次录音仍需系统麦克风权限，接收端独立批准。Windows 可使用对应虚拟麦克风；Linux 当前仅支持指定目标播放，不提供虚拟麦克风。' : '在电脑应用中选择 CapyIO Speaker 输出；接收器准备完成不代表已经收到可听见的声音。' }}</p>
    </article>
    </div>

    <p class="use-panel__footnote">Legacy 音频使用可信实验局域网／Tailscale UDP，不提供应用层加密。安全配置不会复用这个 UDP 通道。准备中或成功发送均不证明接收端播放或听见。</p>
  </section>
</template>
