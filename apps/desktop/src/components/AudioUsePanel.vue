<script setup lang="ts">
import ResourceIcon from './ResourceIcon.vue';
import { computed, onMounted, onUnmounted, ref } from 'vue';
import type { AudioUseDirection, CapyIOApi, UiAudioPeer, UiAudioUse } from '../lib/types';
const props = defineProps<{ api: CapyIOApi; direction?: AudioUseDirection; preferredPeer?: string }>();
const peers = ref<UiAudioPeer[]>([]);
const uses = ref<UiAudioUse[]>([]);
const selected = ref<Record<AudioUseDirection, string>>({ microphone: '', speaker: '' });
const errors = ref<Record<AudioUseDirection, string>>({ microphone: '', speaker: '' });
const busy = ref<AudioUseDirection | null>(null);
const readError = ref('');
const loaded = ref(false);
let polling = false;
let readGeneration = 0;
let disposed = false;
let timer: ReturnType<typeof setInterval> | undefined;
const directions = computed<AudioUseDirection[]>(() => props.direction ? [props.direction] : ['microphone', 'speaker']);
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
async function refresh(force = false) {
  if (disposed || (!force && (polling || busy.value))) return;
  const generation = ++readGeneration;
  polling = true;
  try {
    const [nextPeers, nextUses] = await Promise.all([props.api.getAudioPeers(), props.api.getAudioUses()]);
    if (generation !== readGeneration) return;
    peers.value = nextPeers; uses.value = nextUses; readError.value = ''; loaded.value = true;
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
  if (disposed || busy.value) return;
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
function resumed() { if (!document.hidden) void refresh(); }
onMounted(() => { void refresh(); timer = setInterval(resumed, 1500); document.addEventListener('visibilitychange', resumed); });
onUnmounted(() => { disposed = true; readGeneration++; if (timer) clearInterval(timer); document.removeEventListener('visibilitychange', resumed); });
</script>

<template>
  <section class="info-panel use-panel" aria-label="跨设备音频">
    <header class="use-panel__heading"><span class="use-panel__icon"><ResourceIcon kind="speaker" /></span><div><p class="eyebrow">声音，在设备间流动</p><h2>跨设备音频</h2></div></header>
    <p>选择已连接节点并开始，自动准备两端；无需在另一端再次启动。麦克风与扬声器分别授权、分别停止。</p>
    <p v-if="!direction || direction === 'microphone'">开始使用麦克风会同时开启本机麦克风资源共享。</p>
    <p v-if="!loaded && !readError" role="status">正在读取设备与使用状态…</p>
    <div v-if="readError" class="use-panel__empty">
      <p role="alert">暂时无法更新设备与使用状态，不能据此判断设备是否离线。{{ loaded ? '下方保留上次读取的使用状态。' : '' }}</p>
      <button type="button" class="ghost-button" :disabled="!!busy" @click="refresh()">重新读取状态</button>
      <details><summary>查看技术原因</summary><p>{{ readError }}</p></details>
    </div>
    <div class="use-panel__resources">
    <article v-for="direction in directions" :key="direction" class="audio-use-row use-panel__resource" :class="{ 'use-panel__resource--active': state(direction)?.status === 'active' }">
      <h3>{{ direction === 'microphone' ? '把本机麦克风提供给电脑' : '用本机扬声器播放电脑声音' }}</h3>
      <div class="use-panel__controls"><label>选择设备 <select v-model="selected[direction]" :disabled="!!busy || running(direction)"><option value="">请选择已连接设备</option><option v-for="peer in candidates(direction)" :key="peer.nodeId" :value="peer.nodeId">{{ peer.displayName }}</option></select></label>
      <button :class="running(direction) ? 'danger-button' : 'primary-button'" :disabled="!!busy || state(direction)?.status === 'stopping' || (!running(direction) && !candidates(direction).some(peer => peer.nodeId === selected[direction]))" @click="change(direction)">{{ busy === direction ? '正在提交…' : running(direction) ? state(direction)?.status === 'awaiting_consent' ? '取消申请' : '停止使用' : '开始使用' }}</button></div>
      <p class="use-panel__status" :data-state="state(direction)?.status ?? 'idle'" role="status">{{ statusLabel(direction) }}<span v-if="errors[direction] || state(direction)?.error"> · {{ errors[direction] || state(direction)?.error }}</span></p>
      <p v-if="loaded && !readError && !candidates(direction).length && !running(direction)" class="use-panel__empty">还没有可用的设备。请先连接电脑，并确认它支持此项音频功能。</p>
      <p>{{ direction === 'microphone' ? '首次录音仍需系统麦克风权限；电脑应用中选择对应虚拟麦克风。' : '在电脑应用中选择 CapyIO Speaker 输出；接收器准备完成不代表已经收到可听见的声音。' }}</p>
    </article>
    </div>

    <p class="use-panel__footnote">当前媒体使用既有可信局域网／Tailscale UDP 通道，不是应用层加密音频。准备中不代表播放或录音已成功。</p>
  </section>
</template>
