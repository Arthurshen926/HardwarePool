<script setup lang="ts">
import ResourceIcon from './ResourceIcon.vue';
import { computed, onMounted, onUnmounted, ref } from 'vue';
import type { CapyIOApi, UiControllerPeer, UiCameraUse } from '../lib/types';
const props = defineProps<{ api: CapyIOApi; preferredPeer?: string }>();
const peers = ref<UiControllerPeer[]>([]);
const use = ref<UiCameraUse | null>(null);
const selected = ref('');
const busy = ref(false);
const error = ref('');
const readError = ref('');
const loaded = ref(false);
let reading = false;
let readGeneration = 0;
let disposed = false;
let timer: ReturnType<typeof setInterval> | undefined;
const running = computed(() => !!use.value && !['idle', 'failed'].includes(use.value.status));
const labels = { idle: '未使用', awaiting_consent: '等待设备允许', starting: '正在准备相机', active: '相机使用中', stopping: '正在停止', failed: '相机连接失败' };
async function refresh(force = false) {
  if (disposed || (!force && (reading || busy.value))) return;
  const generation = ++readGeneration;
  reading = true;
  try {
    const [nextPeers, nextUse] = await Promise.all([props.api.getCameraPeers(), props.api.getCameraUse()]);
    if (generation !== readGeneration) return;
    peers.value = nextPeers; use.value = nextUse; readError.value = ''; loaded.value = true;
    if (running.value && nextUse) selected.value = nextUse.peerNodeId;
    else if (!peers.value.some(peer => peer.nodeId === selected.value)) selected.value = props.preferredPeer ? (peers.value.find(peer => peer.nodeId === props.preferredPeer)?.nodeId ?? '') : (peers.value.length === 1 ? peers.value[0].nodeId : '');
  } catch (failure) { if (generation === readGeneration) { peers.value = []; readError.value = String(failure); } }
  finally { if (generation === readGeneration) reading = false; }
}
async function change() {
  if (disposed || busy.value) return;
  readGeneration++;
  reading = false;
  busy.value = true; error.value = '';
  try {
    if (running.value) await props.api.stopCameraUse();
    else await props.api.startCameraUse(selected.value);
    await refresh(true);
  } catch (failure) { if (!disposed) error.value = String(failure); }
  finally { if (!disposed) busy.value = false; }
}
function resumed() { if (!document.hidden) void refresh(); }
onMounted(() => { void refresh(); timer = setInterval(resumed, 1500); document.addEventListener('visibilitychange', resumed); });
onUnmounted(() => { disposed = true; readGeneration++; if (timer) clearInterval(timer); document.removeEventListener('visibilitychange', resumed); });
</script>

<template>
  <section class="info-panel use-panel" aria-label="跨设备相机">
    <header class="use-panel__heading"><span class="use-panel__icon"><ResourceIcon kind="camera" /></span><div><p class="eyebrow">让电脑用上这颗镜头</p><h2>把本机摄像头提供给电脑</h2></div></header>
    <p>选择已连接节点并开始，自动准备两端；不用在电脑上另外启动接收器。首次仍需资源授权及 Android 系统相机权限。</p>
    <p>开始使用会同时开启本机摄像头资源共享。</p>
    <p v-if="!loaded && !readError" role="status">正在读取设备与使用状态…</p>
    <div v-if="readError" class="use-panel__empty">
      <p role="alert">暂时无法更新设备与使用状态，不能据此判断设备是否离线。{{ loaded ? '下方保留上次读取的使用状态。' : '' }}</p>
      <button type="button" class="ghost-button" :disabled="!!busy" @click="refresh()">重新读取状态</button>
      <details><summary>查看技术原因</summary><p>{{ readError }}</p></details>
    </div>
    <div class="camera-use-controls use-panel__controls">
      <label>选择设备 <select v-model="selected" :disabled="busy || running"><option value="">请选择已连接设备</option><option v-for="peer in peers" :key="peer.nodeId" :value="peer.nodeId">{{ peer.displayName }}</option></select></label>
      <button :class="running ? 'danger-button' : 'primary-button'" :disabled="busy || use?.status === 'stopping' || (!running && !peers.some(peer => peer.nodeId === selected))" @click="change">{{ busy ? '正在提交…' : running ? use?.status === 'awaiting_consent' ? '取消申请' : '停止使用' : '开始使用' }}</button>
    </div>
    <p class="use-panel__status" :data-state="use?.status ?? 'idle'" role="status">{{ use ? labels[use.status] : loaded ? '未使用' : '尚未读取' }}<span v-if="error || use?.error"> · {{ error || use?.error }}</span></p>

    <p v-if="loaded && !readError && !peers.length && !running" class="use-panel__empty">还没有可用的设备。请先连接电脑，并确认它支持接收摄像头画面。</p>
    <p class="use-panel__footnote">在电脑相机/会议应用中选择 CapyIO 虚拟相机并检查实际画面。接收器就绪不代表应用已经显示图像。当前视频沿既有可信局域网／Tailscale 数据通道传输。</p>
  </section>
</template>
