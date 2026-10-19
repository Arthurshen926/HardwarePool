<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import DisplayUsePanel from './DisplayUsePanel.vue';
import type { CapyIOApi, UiDisplayReceiverState, UiDisplaySenderState } from "../lib/types";
import { displaySenderCanStart, displaySenderCanStop, displayProblemHint } from "../lib/display-control";
const props = defineProps<{ api: CapyIOApi; preferredPeer?: string }>();
const state = ref<UiDisplayReceiverState | null>(null);
const sender = ref<UiDisplaySenderState | null>(null);
const senderLabels = { unavailable: "服务未连接", stopped: "已停止", starting: "正在启动", running: "后台已就绪（尚不代表出画）", stopping: "正在停止并清理", failed: "运行失败" };
let refreshTimer: ReturnType<typeof setInterval> | undefined;
const busy = ref(false);
const error = ref("");
const receiverReadError = ref("");
const senderReadError = ref("");
const viewingDirty = ref(false);
let disposed = false;
const viewing = ref({ fullscreen: true, keepAwake: true, showStatistics: false });
async function refresh() {
  if (disposed || busy.value) return;
  busy.value = true;
  try {
    const [receiverResult, senderResult] = await Promise.allSettled([props.api.getDisplayReceiverState(), props.api.getDisplaySenderState()]);
    if (disposed) return;
    if (receiverResult.status === 'fulfilled') {
      state.value = receiverResult.value; receiverReadError.value = '';
      if (!viewingDirty.value && state.value?.viewingOptions) viewing.value = { ...state.value.viewingOptions };
    } else { receiverReadError.value = String(receiverResult.reason); }
    if (senderResult.status === 'fulfilled') {
      sender.value = senderResult.value; senderReadError.value = '';
    } else { senderReadError.value = String(senderResult.reason); }
  } finally { if (!disposed) busy.value = false; }
}
async function open() {
  if (disposed || busy.value || receiverReadError.value) return;
  busy.value = true;
  error.value = "";
  try { await props.api.openDisplayReceiver({ ...viewing.value }); if (!disposed) viewingDirty.value = false; }
  catch (failure) { if (!disposed) error.value = String(failure); }
  finally { if (!disposed) busy.value = false; }
}
async function importProvisioning() {
  if (disposed || busy.value) return;
  busy.value = true;
  error.value = "";
  try { await props.api.importDisplayProvisioning(); }
  catch (failure) { if (!disposed) error.value = String(failure); }
  finally { if (!disposed) busy.value = false; }
}
async function controlSender(start: boolean) {
  if (disposed || busy.value || (start && senderReadError.value)) return;
  busy.value = true;
  error.value = "";
  try { const next = await (start ? props.api.startDisplaySender() : props.api.stopDisplaySender()); if (!disposed) { sender.value = next; senderReadError.value = ''; } }
  catch (failure) { if (!disposed) error.value = String(failure); }
  finally { if (!disposed) busy.value = false; }
}
onMounted(() => {
  void refresh();
  window.addEventListener("capyio-native-state", refresh);
  refreshTimer = setInterval(() => {
    if (sender.value && ["starting", "running", "stopping"].includes(sender.value.state)) void refresh();
  }, 1000);
});
onUnmounted(() => {
  disposed = true;
  clearInterval(refreshTimer);
  window.removeEventListener("capyio-native-state", refresh);
});
</script>

<template>
  <section class="info-panel display-receiver" aria-labelledby="display-receiver-heading">
    <h2 id="display-receiver-heading">显示共享</h2>
    <div v-if="(state && !state.nativeClientAvailable) || (sender && !sender.available)" class="resource-detail__notice" role="status">
      <p v-if="state && !state.nativeClientAvailable">本机尚未加载显示接收组件。Core 包不包含实验显示引擎，请先确认使用的是含匹配组件的实验版本；重复配对不会补齐组件。</p>
      <p v-if="sender && !sender.available">本机显示服务暂不可用：可能尚未配置、未运行，或当前账号没有访问权限。请展开下方诊断查看状态；此页面不会安装组件或自动提权。</p>
    </div>
    <DisplayUsePanel :api="api" :preferred-peer="preferredPeer" />
    <details :open="!!receiverReadError || !!senderReadError || !!error || !!sender?.problemCode || (!!sender && ['starting', 'running', 'stopping'].includes(sender.state))">
    <summary>旧版显示连接与诊断（不用于节点一键共享）</summary>
    <template v-if="sender">
      <h3>共享到远端屏幕</h3>
      <p>Windows 后台服务负责显示运行，关闭本界面不会停止共享。此控制仅影响显示，不影响音频。</p>
      <p role="status">{{ senderLabels[sender.state] }}</p>
      <p v-if="!sender.available">显示服务尚未配置、未运行，或当前 Windows 用户没有访问权限。需要配置新版服务的显示连接和允许控制的用户；刷新不会自动安装或提权。</p>
      <p v-else>当前显示的是后台运行状态。请在接收设备确认画面；连接认证、画面计数与实际出画验证仍需分别检查。</p>
      <div class="display-actions">
        <button type="button" :disabled="busy || !!senderReadError || !displaySenderCanStart(sender)" @click="controlSender(true)">启动显示共享</button>
        <button type="button" :disabled="busy || !displaySenderCanStop(sender)" @click="controlSender(false)">停止显示共享</button>
      </div>
      <p v-if="sender.problemCode" role="alert">{{ displayProblemHint(sender.problemCode) }}<br>{{ sender.problemCode }}</p>
    </template>
    <template v-if="state">
      <h3>在本机接收画面</h3>
      <p>使用本机屏幕接收远端画面。接收页面属于同一个 CapyIO 应用，不需要再启动独立 APK。</p>
      <dl>
        <dt>传输配置</dt><dd>{{ state.transport === "unconfigured" ? "尚未配置" : state.transport }}</dd>
        <dt>连接文件</dt><dd>{{ state.configurationPresent ? "已存在（连接时仍需验证）" : "缺少有效的应用私有连接文件" }}</dd>
        <dt>原生接收组件</dt><dd>{{ state.nativeClientAvailable ? "已加载" : "尚未打包或 ABI 不兼容" }}</dd>
      </dl>
      <p v-if="!state.canOpen">请先完成显示连接配置，并确保所选传输的组件已就绪。本状态仅检查启动前提，不代表已经连接或成功呈现画面。</p>
      <fieldset :disabled="busy" @change="viewingDirty = true">
        <legend>本机观看设置（下次打开生效）</legend>
        <label><input v-model="viewing.fullscreen" type="checkbox"> 全屏显示，滑动屏幕边缘可唤出系统导航</label>
        <label><input v-model="viewing.keepAwake" type="checkbox"> 接收画面时保持屏幕亮起</label>
        <label><input v-model="viewing.showStatistics" type="checkbox"> 显示分辨率、呈现帧率及累计帧数</label>
      </fieldset>
      <p>画面保持原始比例。以上为本机观看选项，不改变远端分辨率、帧率或码率。退出接收页不会停止 Windows 扩展屏，请在发送端停止共享以恢复桌面。</p>
      <div class="display-actions">
        <button type="button" :disabled="busy" @click="importProvisioning">导入显示连接包</button>
        <button type="button" :disabled="busy || !!receiverReadError || !state.canOpen" @click="open">打开显示画面</button>
      </div>
      <p class="gamepad-lab__note">连接包由可信 Windows 节点生成，通过系统文件选择器导入；证书和私钥不会进入网页表单。</p>
      <p v-if="state.problemCode" class="gamepad-lab__note">{{ displayProblemHint(state.problemCode) }}<br>{{ state.problemCode }}</p>
    </template>
    <p v-else-if="!sender && !busy && !error && !receiverReadError && !senderReadError">此后端未提供真实显示控制；模拟界面不会启动硬件共享。</p>
    <p v-if="receiverReadError || senderReadError" role="alert">部分显示状态暂时无法更新；仍显示的状态可能是上次读取结果。可刷新重试，或停止现有共享。</p>
    <details v-if="receiverReadError || senderReadError"><summary>查看读取失败原因</summary><p v-if="receiverReadError">本机接收：{{ receiverReadError }}</p><p v-if="senderReadError">电脑发送：{{ senderReadError }}</p></details>
    <button type="button" class="ghost-button" :disabled="busy" @click="refresh">刷新显示状态</button>
    <p v-if="error" role="alert">{{ error }}</p>
    </details>
  </section>
</template>

<style scoped>
.display-receiver { display: grid; gap: 0.6rem; }
dl { display: grid; grid-template-columns: auto minmax(0, 1fr); gap: 0.4rem 1rem; }
dd { margin: 0; overflow-wrap: anywhere; }
button { justify-self: start; }
.display-actions { display: flex; flex-wrap: wrap; gap: 0.6rem; }
fieldset { display: grid; gap: 0.75rem; min-width: 0; }
label { display: flex; align-items: start; gap: 0.5rem; }
</style>
