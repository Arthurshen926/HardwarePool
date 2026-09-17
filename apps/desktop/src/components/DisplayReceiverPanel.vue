<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import type { CapyIOApi, UiDisplayReceiverState, UiDisplaySenderState } from "../lib/types";
import { displaySenderCanStart, displaySenderCanStop } from "../lib/display-control";
const props = defineProps<{ api: CapyIOApi }>();
const state = ref<UiDisplayReceiverState | null>(null);
const sender = ref<UiDisplaySenderState | null>(null);
const senderLabels = { unavailable: "服务未连接", stopped: "已停止", starting: "正在启动", running: "后台已就绪（尚不代表出画）", stopping: "正在停止并清理", failed: "运行失败" };
let refreshTimer: ReturnType<typeof setInterval> | undefined;
const busy = ref(false);
const error = ref("");
async function refresh() {
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  try { [state.value, sender.value] = await Promise.all([props.api.getDisplayReceiverState(), props.api.getDisplaySenderState()]); }
  catch (failure) { state.value = null; sender.value = null; error.value = String(failure); }
  finally { busy.value = false; }
}
async function open() {
  busy.value = true;
  error.value = "";
  try { await props.api.openDisplayReceiver(); }
  catch (failure) { error.value = String(failure); }
  finally { busy.value = false; }
}
async function importProvisioning() {
  busy.value = true;
  error.value = "";
  try { await props.api.importDisplayProvisioning(); }
  catch (failure) { error.value = String(failure); }
  finally { busy.value = false; }
}
async function controlSender(start: boolean) {
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  try { sender.value = await (start ? props.api.startDisplaySender() : props.api.stopDisplaySender()); }
  catch (failure) { error.value = String(failure); }
  finally { busy.value = false; }
}
onMounted(() => {
  void refresh();
  window.addEventListener("capyio-native-state", refresh);
  refreshTimer = setInterval(() => {
    if (sender.value && ["starting", "running", "stopping"].includes(sender.value.state)) void refresh();
  }, 1000);
});
onUnmounted(() => {
  clearInterval(refreshTimer);
  window.removeEventListener("capyio-native-state", refresh);
});
</script>

<template>
  <section class="info-panel display-receiver" aria-labelledby="display-receiver-heading">
    <h2 id="display-receiver-heading">显示共享</h2>
    <template v-if="sender">
      <h3>共享到远端屏幕</h3>
      <p>Windows 后台服务负责显示运行，关闭本界面不会停止共享。此控制仅影响显示，不影响音频。</p>
      <p role="status">{{ senderLabels[sender.state] }}</p>
      <p v-if="!sender.available">显示服务尚未配置、未运行，或当前 Windows 用户没有访问权限。需要配置新版服务的显示连接和允许控制的用户；刷新不会自动安装或提权。</p>
      <p v-else>当前显示的是后台运行状态。请在接收设备确认画面；连接认证、画面计数与实际出画验证仍需分别检查。</p>
      <div class="display-actions">
        <button type="button" :disabled="busy || !displaySenderCanStart(sender)" @click="controlSender(true)">启动显示共享</button>
        <button type="button" :disabled="busy || !displaySenderCanStop(sender)" @click="controlSender(false)">停止显示共享</button>
      </div>
      <p v-if="sender.problemCode">{{ sender.problemCode }}</p>
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
      <div class="display-actions">
        <button type="button" :disabled="busy" @click="importProvisioning">导入显示连接包</button>
        <button type="button" :disabled="busy || !state.canOpen" @click="open">打开显示画面</button>
      </div>
      <p class="gamepad-lab__note">连接包由可信 Windows 节点生成，通过系统文件选择器导入；证书和私钥不会进入网页表单。</p>
      <p v-if="state.problemCode" class="gamepad-lab__note">{{ state.problemCode }}</p>
    </template>
    <p v-else-if="!sender && !busy && !error">此后端未提供真实显示控制；模拟界面不会启动硬件共享。</p>
    <button type="button" class="ghost-button" :disabled="busy" @click="refresh">刷新显示状态</button>
    <p v-if="error" role="alert">{{ error }}</p>
  </section>
</template>

<style scoped>
.display-receiver { display: grid; gap: 0.6rem; }
dl { display: grid; grid-template-columns: auto minmax(0, 1fr); gap: 0.4rem 1rem; }
dd { margin: 0; overflow-wrap: anywhere; }
button { justify-self: start; }
.display-actions { display: flex; flex-wrap: wrap; gap: 0.6rem; }
</style>
