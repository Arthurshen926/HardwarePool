<script setup lang="ts">
import ResourceIcon from './ResourceIcon.vue';
import { computed, onMounted, onUnmounted, ref } from 'vue';
import type { AudioUseDirection, CapyIOApi, UiAudioPeer, UiAudioUse, UiAndroidMicrophoneSettings, UiAndroidSpeakerSettings } from '../lib/types';
const props = defineProps<{ api: CapyIOApi; direction?: AudioUseDirection; preferredPeer?: string }>();
const peers = ref<UiAudioPeer[]>([]);
const uses = ref<UiAudioUse[]>([]);
const selected = ref<Record<AudioUseDirection, string>>({ microphone: '', 'microphone-secure': '', speaker: '', 'speaker-secure': '' });
const errors = ref<Record<AudioUseDirection, string>>({ microphone: '', 'microphone-secure': '', speaker: '', 'speaker-secure': '' });
const busy = ref<AudioUseDirection | null>(null);
const readError = ref('');
const loaded = ref(false);
const microphoneSettings = ref<UiAndroidMicrophoneSettings | null>(null);
const speakerSettings = ref<UiAndroidSpeakerSettings | null>(null);
const speakerProfileError = ref('');
const secureSpeakerSelected = computed(() => speakerSettings.value?.playbackProfile === 'secure_stereo_tls13_v1');
const speakerDirection = computed<AudioUseDirection>(() => secureSpeakerSelected.value ? 'speaker-secure' : 'speaker');
const profileBusy = ref(false);
const profileError = ref('');
const secureMicrophoneSelected = computed(() => microphoneSettings.value?.captureProfile === 'secure_mono_tls13_v1'
  || microphoneSettings.value?.captureProfile === 'secure_mono_tls13_300s_v1');
const microphoneDirection = computed<AudioUseDirection>(() => secureMicrophoneSelected.value ? 'microphone-secure' : 'microphone');
let polling = false;
let readGeneration = 0;
let disposed = false;
let timer: ReturnType<typeof setInterval> | undefined;
const directions = computed<AudioUseDirection[]>(() => props.direction ? [props.direction.startsWith('speaker') ? speakerDirection.value : microphoneDirection.value] : [microphoneDirection.value, speakerDirection.value]);
const statusLabels = { idle: '未使用', awaiting_consent: '等待设备允许', starting: '正在准备音频', active: '音频使用中', stopping: '正在停止', failed: '音频连接失败' };
function state(direction: AudioUseDirection) { return uses.value.find(use => use.direction === direction); }
function statusLabel(direction: AudioUseDirection) {
  const use = state(direction);
  if (!use) return loaded.value ? '未使用' : '尚未读取';
  if (direction === 'speaker-secure' && ['starting', 'active'].includes(use.status)) return '实验性 TLS 扬声器准备中（Starting）；PCM 被接受也不证明可听见';
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
    const [nextPeers, nextUses, nextSettings, nextSpeakerSettings] = await Promise.all([props.api.getAudioPeers(), props.api.getAudioUses(), props.api.getAndroidMicrophoneSettings(), props.api.getAndroidSpeakerSettings?.() ?? null]);
    if (disposed || generation !== readGeneration) return;
    peers.value = nextPeers; uses.value = nextUses; microphoneSettings.value = nextSettings; speakerSettings.value = nextSpeakerSettings; readError.value = ''; loaded.value = true;
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
  if (direction === 'speaker-secure' && !running(direction)) return;
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
  if (value !== 'legacy' && value !== 'secure_mono_tls13_v1' && value !== 'secure_mono_tls13_300s_v1') return;
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
async function selectSpeakerProfile(event: Event) {
  const settings = speakerSettings.value;
  if (!settings || disposed || busy.value || profileBusy.value) return;
  const value = (event.target as HTMLSelectElement).value;
  if (value !== 'legacy' && value !== 'secure_stereo_tls13_v1') return;
  readGeneration++; polling = false; profileBusy.value = true; speakerProfileError.value = '';
  try {
    const next = await props.api.selectAndroidSpeakerProfile(value, settings.revision);
    if (disposed) return;
    speakerSettings.value = next;
    selected.value.speaker = ''; selected.value['speaker-secure'] = '';
    await refresh(true);
  } catch (error) { if (!disposed) { speakerProfileError.value = String(error); await refresh(true); } }
  finally { if (!disposed) profileBusy.value = false; }
}
function secureStartBlocked(direction: AudioUseDirection) {
  if (direction === 'speaker-secure') return true; // Inbound-only; no generic start or UDP fallback.
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
    <p>Legacy 音频与安全麦克风可选择已连接节点并开始。安全扬声器仅接受兼容 Source API 发起的请求。麦克风与扬声器分别授权、分别停止。</p>
    <p v-if="!direction || direction === 'microphone'">开始使用麦克风会同时开启本机麦克风资源共享。</p>
    <div v-if="microphoneSettings && (!direction || !direction.startsWith('speaker'))" class="use-panel__resource">
      <label>麦克风传输配置
        <select :value="microphoneSettings.captureProfile" :disabled="!!busy || profileBusy" @change="selectProfile">
          <option value="legacy">Legacy：可信实验网络 UDP（明文）</option>
          <option value="secure_mono_tls13_v1">实验性安全单声道：固定 TLS 1.3，最多 60 秒</option>
          <option value="secure_mono_tls13_300s_v1">实验性安全单声道：固定 TLS 1.3，最多 5 分钟（300 秒）</option>
        </select>
      </label>
      <p>默认仍为 Legacy；不会自动升级为较长配置。切换会停止当前麦克风会话并取消共享，必须重新开始和授权。来源固定为系统默认麦克风；采集中来源变化会终止会话。</p>
      <p v-if="microphoneSettings.captureProfile === 'secure_mono_tls13_v1'">此配置单次最多 60 秒。</p>
      <p v-if="microphoneSettings.captureProfile === 'secure_mono_tls13_300s_v1'">此配置单次最多 5 分钟（300 秒），不是保证可录满 5 分钟。</p>
      <p v-if="secureMicrophoneSelected">实验性 48 kHz / S16LE / 单声道。每次仍需独立的有限会话授权；原始授权或租约期限可能更短，协商也会占用时间。选择配置不表示正在传输，不会开始录音、延长授权或自动续期。必须保持应用可见。失败不会退回 UDP；此麦克风入口不支持安全扬声器或安全双工。这里只表示具备尝试条件；合成测试不保证真机录音、低延迟、录满上限或远端播放。</p>
      <p v-if="secureMicrophoneSelected && !microphoneSettings.permissionReady" role="status">系统录音或通知权限尚未授予。请先在 Android 应用权限中允许录音和通知；此配置不会自动弹出权限请求。</p>
      <p v-if="profileError" role="alert">{{ profileError }}</p>
    </div>
    <div v-if="speakerSettings && (!direction || direction.startsWith('speaker'))" class="use-panel__resource">
      <label>扬声器传输配置
        <select :value="speakerSettings.playbackProfile" :disabled="!!busy || profileBusy" @change="selectSpeakerProfile">
          <option value="legacy">Legacy：可信实验网络 UDP（明文）</option>
          <option value="secure_stereo_tls13_v1">实验性安全双声道：固定 TLS 1.3，最多 60 秒</option>
        </select>
      </label>
      <p>默认仍为 Legacy。扬声器配置独立于麦克风的 60／300 秒设置；切换会停止扬声器会话并取消共享，需要重新共享和逐次授权，不会自动启动、重连或续期。</p>
      <p v-if="secureSpeakerSelected">48 kHz / S16LE / 双声道，最多 60 秒，原始授权或租约可能更短。先共享本机扬声器，再由支持同一精确配置的 Source API 发起请求，在本机逐次允许。Linux 捕获启动器／终端目前仍是 Legacy，不是完整的手机／终端安全播放流程；Windows 一键使用不支持此配置。失败不会退回 UDP。必须保持应用可见。</p>
      <p v-if="secureSpeakerSelected">TLS 就绪和 AudioTrack 接受 PCM 均不证明实际播放或可听见；状态保持 Starting。合成测试不代表真机扬声器或低延迟验证。</p>
      <p v-if="secureSpeakerSelected && !speakerSettings.permissionReady" role="status">系统通知权限尚未授予。此设置不会申请录音权限，也不会自动弹出权限请求。</p>
      <p v-if="secureSpeakerSelected && speakerSettings.eligibility !== 'eligible'" role="status">当前设备尚不具备尝试此安全扬声器配置的条件。</p>
      <p v-if="speakerProfileError" role="alert">{{ speakerProfileError }}</p>
    </div>
    <p v-if="!loaded && !readError" role="status">正在读取设备与使用状态…</p>
    <div v-if="readError" class="use-panel__empty">
      <p role="alert">暂时无法更新设备与使用状态，不能据此判断设备是否离线。{{ loaded ? '下方保留上次读取的使用状态。' : '' }}</p>
      <button type="button" class="ghost-button" :disabled="!!busy" @click="refresh()">重新读取状态</button>
      <details><summary>查看技术原因</summary><p>{{ readError }}</p></details>
    </div>
    <div class="use-panel__resources">
    <article v-for="direction in directions" :key="direction" class="audio-use-row use-panel__resource" :class="{ 'use-panel__resource--active': direction !== 'speaker-secure' && state(direction)?.status === 'active' }">
      <h3>{{ !direction.startsWith('speaker') ? '把本机麦克风提供给电脑' : '用本机扬声器播放电脑声音' }}</h3>
      <div class="use-panel__controls"><label v-if="direction !== 'speaker-secure'">选择设备 <select v-model="selected[direction]" :disabled="!!busy || profileBusy || running(direction)"><option value="">请选择已连接设备</option><option v-for="peer in candidates(direction)" :key="peer.nodeId" :value="peer.nodeId">{{ peer.displayName }}</option></select></label>
      <button v-if="direction !== 'speaker-secure' || running(direction)" :class="running(direction) ? 'danger-button' : 'primary-button'" :disabled="!!busy || profileBusy || state(direction)?.status === 'stopping' || (!running(direction) && (secureStartBlocked(direction) || !candidates(direction).some(peer => peer.nodeId === selected[direction])))" @click="change(direction)">{{ busy === direction ? '正在提交…' : running(direction) ? state(direction)?.status === 'awaiting_consent' ? '取消申请' : '停止使用' : '开始使用' }}</button></div>
      <p class="use-panel__status" :data-state="state(direction)?.status ?? 'idle'" role="status">{{ statusLabel(direction) }}<span v-if="errors[direction] || state(direction)?.error"> · {{ errors[direction] || state(direction)?.error }}</span></p>
      <p v-if="direction !== 'speaker-secure' && loaded && !readError && !candidates(direction).length && !running(direction)" class="use-panel__empty">还没有可用的设备。请先连接电脑，并确认它支持此项音频功能。</p>
      <p v-for="peer in unavailable(direction)" :key="peer.nodeId" class="use-panel__empty">{{ peer.displayName }}：{{ peer.reason }}</p>
      <p v-if="direction === 'microphone-secure'">接收端必须明确支持所选的同一安全配置并独立授权。Linux 有限会话入口可选择指定目标播放或虚拟麦克风；当前不提供 Windows 安全音频。不支持的接收端不能退回较短配置或 UDP。</p>
      <p v-else-if="direction === 'speaker-secure'">等待兼容 Source API 的入站申请；此入口不发起安全扬声器连接。</p>
      <p v-else>{{ !direction.startsWith('speaker') ? '首次录音仍需系统麦克风权限，接收端独立批准。Windows 可使用对应虚拟麦克风；Linux 有限会话入口可选择指定目标播放或虚拟麦克风。' : '在电脑应用中选择 CapyIO Speaker 输出；接收器准备完成不代表已经收到可听见的声音。' }}</p>
    </article>
    </div>

    <p class="use-panel__footnote">Legacy 音频使用可信实验局域网／Tailscale UDP，不提供应用层加密。安全配置不会复用这个 UDP 通道。准备中或成功发送均不证明接收端播放或听见。</p>
  </section>
</template>
