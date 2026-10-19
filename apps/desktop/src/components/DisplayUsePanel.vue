<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue';
import type { CapyIOApi, UiDisplayPeer, UiDisplayUse } from '../lib/types';
import { displayProblemHint } from '../lib/display-control';
const props = defineProps<{ api: CapyIOApi; preferredPeer?: string }>();
const peers = ref<UiDisplayPeer[]>([]);
const state = ref<UiDisplayUse | null>(null);
const peer = ref('');
const automatic = ref(true);
const mode = ref('1920x1080');
const refreshMillihertz = ref(60000);
const bitrateKbps = ref(15000);
const busy = ref(false);
const error = ref('');
const pollError = ref('');
const loaded = ref(false);
let polling = false;
let readGeneration = 0;
let disposed = false;
let timer: ReturnType<typeof setInterval> | undefined;
const running = computed(() => !!state.value && state.value.status !== 'failed');
const labels = { awaiting_consent: '等待资源授权', starting: '正在准备两端与接收画面', active: '接收端已呈现画面', stopping: '正在停止并恢复桌面', failed: '显示共享未完成' };
async function refresh(force = false) {
  if (disposed || (!force && (polling || busy.value))) return;
  const generation = ++readGeneration;
  polling = true;
  try {
    const [nextPeers, nextState] = await Promise.all([props.api.getDisplayPeers(), props.api.getDisplayUse()]);
    if (disposed || generation !== readGeneration) return;
    peers.value = nextPeers;
    state.value = nextState;
    pollError.value = ''; loaded.value = true;
    if (nextState && nextState.status !== 'failed') {
      peer.value = nextState.peerNodeId;
      automatic.value = nextState.automatic ?? false;
      mode.value = `${nextState.width}x${nextState.height}`;
      refreshMillihertz.value = nextState.refreshMillihertz;
      bitrateKbps.value = nextState.bitrateKbps;
    } else if (!nextPeers.some(node => node.nodeId === peer.value)) {
      peer.value = props.preferredPeer ? (nextPeers.find(node => node.nodeId === props.preferredPeer)?.nodeId ?? '')
        : (nextPeers.length === 1 ? nextPeers[0].nodeId : '');
    }
  } catch (failure) { if (!disposed && generation === readGeneration) pollError.value = String(failure); }
  finally { if (generation === readGeneration) polling = false; }
}
async function start() {
  if (disposed || busy.value || running.value || pollError.value) return;
  readGeneration++;
  polling = false;
  busy.value = true;
  error.value = '';
  try {
    const [width, height] = mode.value.split('x').map(Number);
    await props.api.startDisplayUse(peer.value, { automatic: automatic.value, width, height, refreshMillihertz: refreshMillihertz.value, bitrateKbps: bitrateKbps.value });
    await refresh(true);
  } catch (failure) { if (!disposed) error.value = String(failure); }
  finally { if (!disposed) busy.value = false; }
}
async function stop() {
  if (disposed || busy.value) return;
  readGeneration++;
  polling = false;
  busy.value = true;
  error.value = '';
  try { await props.api.stopDisplayUse(); await refresh(true); }
  catch (failure) { if (!disposed) error.value = String(failure); }
  finally { if (!disposed) busy.value = false; }
}
function resumed() { if (!document.hidden) void refresh(); }
onMounted(() => { void refresh(); timer = setInterval(resumed, 1000); document.addEventListener('visibilitychange', resumed); });
onUnmounted(() => { disposed = true; clearInterval(timer); document.removeEventListener('visibilitychange', resumed); });
</script>

<template>
  <section class="display-use" aria-label="跨设备扩展屏">
    <h3>跨设备扩展屏</h3>
    <p>使用 Android 屏幕显示 Windows 扩展桌面。任意一端发起，沿用节点配对；首次使用仍需资源授权，不需要单独导入显示连接包。</p>
    <p v-if="!loaded && !pollError" role="status">正在读取设备与使用状态…</p>
    <div v-if="pollError" class="use-panel__empty">
      <p role="alert">暂时无法更新设备与使用状态，不能据此判断设备是否离线。{{ loaded ? '下方保留上次读取的使用状态。' : '' }}</p>
      <button type="button" class="ghost-button" :disabled="!!busy" @click="refresh()">重新读取状态</button>
      <details><summary>查看技术原因</summary><p>{{ pollError }}</p></details>
    </div>
    <fieldset :disabled="busy || running">
      <legend>本次共享画质</legend>
      <label>画质模式<select v-model="automatic"><option :value="true">自动 · 适配接收端屏幕</option><option :value="false">手动 · 自选参数</option></select></label>
      <label>已连接节点<select v-model="peer"><option value="">请选择节点</option><option v-for="node in peers" :key="node.nodeId" :value="node.nodeId">{{ node.displayName }}{{ node.displayAvailable === false ? ' · 未发布显示资源' : '' }}</option></select></label>
      <label v-if="!automatic">分辨率<select v-model="mode"><option v-if="!['1280x720', '1920x1080', '2560x1440'].includes(mode)" :value="mode">{{ mode.replace('x', ' × ') }} · 上次协商</option><option value="1280x720">1280 × 720 · 流畅</option><option value="1920x1080">1920 × 1080 · 均衡</option><option value="2560x1440">2560 × 1440 · 清晰</option></select></label>
      <label v-if="!automatic">帧率<select v-model.number="refreshMillihertz"><option v-if="![30000, 60000].includes(refreshMillihertz)" :value="refreshMillihertz">{{ refreshMillihertz / 1000 }} FPS · 上次协商</option><option :value="30000">30 FPS</option><option :value="60000">60 FPS</option></select></label>
      <label v-if="!automatic">码率<select v-model.number="bitrateKbps"><option v-if="![5000, 10000, 15000, 25000, 40000].includes(bitrateKbps)" :value="bitrateKbps">{{ bitrateKbps / 1000 }} Mbps · 上次协商</option><option :value="5000">5 Mbps</option><option :value="10000">10 Mbps</option><option :value="15000">15 Mbps</option><option :value="25000">25 Mbps</option><option :value="40000">40 Mbps</option></select></label>
    </fieldset>
    <p v-if="automatic" class="gamepad-lab__note">启动时按接收端屏幕比例、刷新率与解码能力选择，最高 60 FPS，并推荐码率。不随网络实时调码率；旋转或更换画质后需停止再启动。</p>
    <p v-else class="gamepad-lab__note">H.264 / SDR。两端确认所选参数；不支持时提示，不会默默降低画质。调整参数前请停止当前共享。</p>
    <div class="display-use-actions">
      <button v-if="!running" class="primary-button" :disabled="busy || !!pollError || !peers.some(node => node.nodeId === peer && node.displayAvailable !== false)" @click="start">开始共享</button>
      <button v-else class="danger-button" :disabled="busy || state?.status === 'stopping'" @click="stop">{{ state?.status === 'awaiting_consent' ? '取消申请' : '停止共享并恢复桌面' }}</button>
    </div>
    <p role="status">{{ busy ? '正在提交操作…' : state ? labels[state.status] : loaded ? '未共享' : '尚未读取' }}</p>
    <p v-if="state?.status === 'active'">实际画质：{{ state.width }} × {{ state.height }} · {{ state.refreshMillihertz / 1000 }} FPS · {{ state.bitrateKbps / 1000 }} Mbps · H.264 / SDR</p>
    <p v-if="loaded && !pollError && !peers.length">没有可用的已连接显示节点。请先连接配套新版，并在本机共享中开启显示资源。</p>
    <p v-if="peers.some(node => node.nodeId === peer && node.displayAvailable === false)">节点已连接，但未发布兼容的显示资源。请在 Windows「本机共享」中勾选「Windows 扩展屏幕」，即可从平板发起；不需要重新配对。</p>
    <p class="gamepad-lab__note">共享期间不要在 Windows 系统设置中更改此扩展屏的分辨率或刷新率。请先停止共享，再在 CapyIO 选择画质并重新启动；当前界面不支持 120 FPS。</p>
    <p v-if="error || state?.error" role="alert">{{ error || state?.error }}</p>
    <p v-if="state?.error?.startsWith('CAPY.DISPLAY.')">{{ displayProblemHint(state.error) }}</p>
  </section>
</template>

<style scoped>
.display-use { display: grid; gap: .6rem; }
fieldset { display: flex; flex-wrap: wrap; gap: 1rem; min-width: 0; }
label { display: flex; flex-direction: column; gap: .4rem; flex: 1 1 10rem; }
select { min-height: 2.75rem; max-width: 100%; }
.display-use-actions { display: flex; flex-wrap: wrap; gap: .6rem; }
</style>
