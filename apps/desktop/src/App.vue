<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";

import RouteCard from "./components/RouteCard.vue";
import AndroidAudioConnection from "./components/AndroidAudioConnection.vue";
import DisplayReceiverPanel from "./components/DisplayReceiverPanel.vue";
import CapabilityCatalog from "./components/CapabilityCatalog.vue";
import GamepadSurface from "./components/GamepadSurface.vue";
import GamepadTelemetry from "./components/GamepadTelemetry.vue";
import QuickActionCard from "./components/QuickActionCard.vue";
import StatusPill from "./components/StatusPill.vue";
import TouchpadSurface from "./components/TouchpadSurface.vue";
import { createCapyIOApi } from "./lib/api";
import type { DsuProjectionMode, GamepadControlUpdate, QuickActionOperation, TouchpadControlUpdate, UiAndroidAudioCapability, UiAndroidAudioState, UiAndroidCameraSettings, UiAudioEndpointCatalog, UiCapability, UiGamepadState, UiLiveImu, UiQuickAction, UiRoute, UiSnapshot, UiTouchpadState, WindowsControllerKind } from "./lib/types";

const api = createCapyIOApi();
const diagnostics = ref<import("./lib/types").UiDiagnostics | null>(null);
const diagnosticsError = ref("");
async function refreshDiagnostics() {
  diagnosticsError.value = "";
  try { diagnostics.value = await api.getDiagnostics(); }
  catch (error) { diagnosticsError.value = String(error); }
}
const snapshot = ref<UiSnapshot | null>(null);
const busyRouteId = ref<string | null>(null);
const loading = ref(true);
const errorMessage = ref("");
type PrimaryView = "quick" | "workspace" | "controller" | "touchpad";
const requestedView = new URLSearchParams(window.location.search).get("view");
const initialView: PrimaryView = requestedView === "workspace" || requestedView === "controller" || requestedView === "touchpad" ? requestedView : "quick";
const view = ref<PrimaryView>(initialView);
const liveImu = ref<UiLiveImu | null>(null);
const liveImuIp = ref("");
const liveImuPort = ref(8080);
const liveImuBusy = ref(false);
const gamepad = ref<UiGamepadState | null>(null);
const gamepadBusy = ref(false);
const touchpad = ref<UiTouchpadState | null>(null);
const touchpadBusy = ref(false);
const androidAudio = ref<UiAndroidAudioState | null>(null);
const androidCamera = ref<UiAndroidCameraSettings | null>(null);
const cameraSettingsBusy = ref(false);
const gamepadDsuPort = ref(26760);
const gamepadDsuMode = ref<DsuProjectionMode>("motion_only");
const gamepadDsuBusy = ref(false);
const gamepadAndroidPort = ref(31580);
const gamepadAndroidHost = ref("");
const gamepadAndroidToken = ref("");
const gamepadAndroidBusy = ref(false);
const gamepadWindowsBusy = ref(false);
const gamepadWindowsKind = ref<WindowsControllerKind>("dualshock4");
const gamepadXinputCompanion = ref(false);
const quickActions = ref<UiQuickAction[]>([]);
const audioEndpoints = ref<UiAudioEndpointCatalog | null>(null);
const busyActionId = ref<string | null>(null);
const busyCapabilityId = ref<string | null>(null);
let livePoll: ReturnType<typeof setInterval> | null = null;

const localNode = computed(() => snapshot.value?.nodes.find((node) => node.local));
const remoteNode = computed(() => snapshot.value?.nodes.find((node) => !node.local));
const activeCount = computed(() => snapshot.value?.routes.filter((route) => route.active).length ?? 0);
const recentEvents = computed(() => [...(snapshot.value?.events ?? [])].reverse());
const liveImuRunning = computed(() => liveImu.value?.status === "connecting" || liveImu.value?.status === "active");
const liveImuTone = computed(() => liveImu.value?.status === "active" ? "success" : ["failed", "offline"].includes(liveImu.value?.status ?? "") ? "danger" : "warning");
const quickActionRouteIds = computed(() => new Set(quickActions.value.flatMap((action) => action.routeId ? [action.routeId] : [])));
const ordinaryQuickRoutes = computed(() => (snapshot.value?.routes ?? []).filter((route) => !quickActionRouteIds.value.has(route.id)));
const localCapabilities = computed(() => (snapshot.value?.capabilities ?? []).filter((capability) => capability.local));
const peerCapabilities = computed(() => (snapshot.value?.capabilities ?? []).filter((capability) => !capability.local));
const androidNode = computed(() => snapshot.value?.backendMode === "android_node");

function openAndroidNativeDiagnostics(): void {
  if (!window.CapyIOAndroid?.openNativeDiagnostics()) {
    errorMessage.value = "Android 原生诊断界面不可用";
  }
}

async function load(): Promise<void> {
  loading.value = true;
  errorMessage.value = "";
  try {
    const [nextSnapshot, nextLiveImu, nextGamepad, nextTouchpad, nextAndroidAudio, nextAndroidCamera, nextQuickActions, nextAudioEndpoints] = await Promise.all([api.getSnapshot(), api.getLiveImu(), api.getGamepadState(), api.getTouchpadState(), api.getAndroidAudioState(), api.getAndroidCameraSettings(), api.getQuickActions(), api.getAudioEndpoints()]);
    snapshot.value = nextSnapshot;
    liveImu.value = nextLiveImu;
    gamepad.value = nextGamepad;
    touchpad.value = nextTouchpad;
    androidAudio.value = nextAndroidAudio;
    androidCamera.value = nextAndroidCamera;
    quickActions.value = nextQuickActions;
    audioEndpoints.value = nextAudioEndpoints;
  }
  catch (error) { errorMessage.value = normalizeError(error); }
  finally { loading.value = false; }
}

async function refreshAudioEndpoints(): Promise<void> {
  errorMessage.value = "";
  try { audioEndpoints.value = await api.getAudioEndpoints(); }
  catch (error) { errorMessage.value = normalizeError(error); }
}

async function selectAudioEndpoint(action: UiQuickAction, selectionToken: string): Promise<void> {
  busyActionId.value = action.id;
  errorMessage.value = "";
  try {
    const updated = await api.selectAudioEndpoint(action.id, selectionToken);
    quickActions.value = quickActions.value.map((item) => item.id === updated.id ? updated : item);
    await refreshAudioEndpoints();
  }
  catch (error) { errorMessage.value = normalizeError(error); }
  finally { busyActionId.value = null; }
}

async function refreshQuickActions(): Promise<void> {
  try { quickActions.value = await api.getQuickActions(); }
  catch (error) { errorMessage.value = normalizeError(error); }
}

async function invokeQuickAction(action: UiQuickAction, operation: QuickActionOperation): Promise<void> {
  busyActionId.value = action.id;
  errorMessage.value = "";
  try {
    const updated = await api.invokeQuickAction(action.id, operation);
    quickActions.value = quickActions.value.map((item) => item.id === updated.id ? updated : item);
    snapshot.value = await api.getSnapshot();
  }
  catch (error) { errorMessage.value = normalizeError(error); }
  finally { busyActionId.value = null; }
}

async function refreshLiveImu(): Promise<void> {
  try { liveImu.value = await api.getLiveImu(); }
  catch (error) { errorMessage.value = normalizeError(error); }
}

async function refreshGamepad(): Promise<void> {
  try { gamepad.value = await api.getGamepadState(); }
  catch (error) { errorMessage.value = normalizeError(error); }
}

async function refreshTouchpad(): Promise<void> {
  try { touchpad.value = await api.getTouchpadState(); }
  catch (error) { errorMessage.value = normalizeError(error); }
}

async function refreshAndroidAudio(): Promise<void> {
  try { androidAudio.value = await api.getAndroidAudioState(); }
  catch (error) { errorMessage.value = normalizeError(error); }
}

async function refreshAndroidCamera(): Promise<void> {
  try { androidCamera.value = await api.getAndroidCameraSettings(); }
  catch (error) { errorMessage.value = normalizeError(error); }
}

async function refreshCameraSources(): Promise<void> {
  cameraSettingsBusy.value = true;
  errorMessage.value = "";
  try { androidCamera.value = await api.refreshAndroidCameraSources(); }
  catch (error) { errorMessage.value = normalizeError(error); }
  finally { cameraSettingsBusy.value = false; }
}

async function selectCameraSource(event: Event): Promise<void> {
  cameraSettingsBusy.value = true;
  errorMessage.value = "";
  try { androidCamera.value = await api.selectAndroidCameraSource((event.target as HTMLSelectElement).value); }
  catch (error) { errorMessage.value = normalizeError(error); }
  finally { cameraSettingsBusy.value = false; }
}

async function selectCameraQuality(event: Event): Promise<void> {
  cameraSettingsBusy.value = true;
  errorMessage.value = "";
  try { androidCamera.value = await api.selectAndroidCameraQuality((event.target as HTMLSelectElement).value as UiAndroidCameraSettings["quality"]); }
  catch (error) { errorMessage.value = normalizeError(error); }
  finally { cameraSettingsBusy.value = false; }
}

function cameraSourceLabel(source: UiAndroidCameraSettings["sources"][number]): string {
  const zoom = source.zoomRatioMilli === null ? "auto" : `${(source.zoomRatioMilli / 1000).toFixed(3)}x`;
  return `Camera2 ${source.cameraId} · ${source.facing} · ${zoom}`;
}

function describeAudio(capability: UiAndroidAudioCapability): string {
  const format = capability.actualFormat;
  if (!format) return "等待平台协商";
  return `${format.sampleRateHz} Hz · ${format.channelCount} ch · ${format.encoding} · ${format.framesPerBuffer} frames/buffer`;
}

async function startLiveImu(): Promise<void> {
  liveImuBusy.value = true;
  errorMessage.value = "";
  try { liveImu.value = await api.startLiveImu(liveImuIp.value.trim(), liveImuPort.value); }
  catch (error) { errorMessage.value = normalizeError(error); }
  finally { liveImuBusy.value = false; }
}

async function stopLiveImu(): Promise<void> {
  liveImuBusy.value = true;
  errorMessage.value = "";
  try { liveImu.value = await api.stopLiveImu(); }
  catch (error) { errorMessage.value = normalizeError(error); }
  finally { liveImuBusy.value = false; }
}

async function toggleRoute(route: UiRoute): Promise<void> {
  busyRouteId.value = route.id;
  errorMessage.value = "";
  try { snapshot.value = await api.setRoute(route.id, !route.active); }
  catch (error) { errorMessage.value = normalizeError(error); }
  finally { busyRouteId.value = null; }
}

async function toggleCapability(capability: UiCapability, selected: boolean): Promise<void> {
  busyCapabilityId.value = capability.id;
  errorMessage.value = "";
  try { snapshot.value = await api.setLocalCapabilityShared(capability.id, selected); }
  catch (error) { errorMessage.value = normalizeError(error); }
  finally { busyCapabilityId.value = null; }
}

async function resetDemo(): Promise<void> {
  loading.value = true;
  errorMessage.value = "";
  try {
    snapshot.value = await api.resetDemo();
    gamepad.value = await api.getGamepadState();
  }
  catch (error) { errorMessage.value = normalizeError(error); }
  finally { loading.value = false; }
}

let gamepadUpdates = Promise.resolve();
function updateGamepad(update: GamepadControlUpdate): void {
  gamepadBusy.value = true;
  gamepadUpdates = gamepadUpdates
    .then(async () => { gamepad.value = await api.updateGamepadState(update); })
    .catch((error: unknown) => { errorMessage.value = normalizeError(error); })
    .finally(() => { gamepadBusy.value = false; });
}

let touchpadUpdates = Promise.resolve();
function updateTouchpad(update: TouchpadControlUpdate): void {
  touchpadBusy.value = true;
  touchpadUpdates = touchpadUpdates
    .then(async () => { touchpad.value = await api.updateTouchpadState(update); })
    .catch((error: unknown) => { errorMessage.value = normalizeError(error); })
    .finally(() => { touchpadBusy.value = false; });
}

async function startGamepadDsu(): Promise<void> {
  gamepadDsuBusy.value = true;
  errorMessage.value = "";
  try { gamepad.value = await api.startGamepadDsu(gamepadDsuPort.value, gamepadDsuMode.value); }
  catch (error) { errorMessage.value = normalizeError(error); }
  finally { gamepadDsuBusy.value = false; }
}

async function stopGamepadDsu(): Promise<void> {
  gamepadDsuBusy.value = true;
  errorMessage.value = "";
  try { gamepad.value = await api.stopGamepadDsu(); }
  catch (error) { errorMessage.value = normalizeError(error); }
  finally { gamepadDsuBusy.value = false; }
}

async function startAndroidGamepad(): Promise<void> {
  gamepadAndroidBusy.value = true;
  errorMessage.value = "";
  try { gamepad.value = await api.startAndroidGamepad(gamepadAndroidPort.value); }
  catch (error) { errorMessage.value = normalizeError(error); }
  finally { gamepadAndroidBusy.value = false; }
}

async function stopAndroidGamepad(): Promise<void> {
  gamepadAndroidBusy.value = true;
  errorMessage.value = "";
  try { gamepad.value = await api.stopAndroidGamepad(); }
  catch (error) { errorMessage.value = normalizeError(error); }
  finally { gamepadAndroidBusy.value = false; }
}

async function configureAndroidController(): Promise<void> {
  gamepadAndroidBusy.value = true;
  errorMessage.value = "";
  try {
    gamepad.value = await api.configureAndroidController(
      gamepadAndroidHost.value.trim(),
      gamepadAndroidPort.value,
      gamepadAndroidToken.value.trim(),
    );
  }
  catch (error) { errorMessage.value = normalizeError(error); }
  finally { gamepadAndroidBusy.value = false; }
}

async function refreshWindowsGamepadPreflight(): Promise<void> {
  gamepadWindowsBusy.value = true;
  errorMessage.value = "";
  try { gamepad.value = await api.refreshWindowsGamepadPreflight(gamepadWindowsKind.value); }
  catch (error) { errorMessage.value = normalizeError(error); }
  finally { gamepadWindowsBusy.value = false; }
}

async function startWindowsGamepadProjection(): Promise<void> {
  gamepadWindowsBusy.value = true;
  errorMessage.value = "";
  try { gamepad.value = await api.startWindowsGamepadProjection(gamepadXinputCompanion.value); }
  catch (error) { errorMessage.value = normalizeError(error); }
  finally { gamepadWindowsBusy.value = false; }
}

async function stopWindowsGamepadProjection(): Promise<void> {
  gamepadWindowsBusy.value = true;
  errorMessage.value = "";
  try { gamepad.value = await api.stopWindowsGamepadProjection(); }
  catch (error) { errorMessage.value = normalizeError(error); }
  finally { gamepadWindowsBusy.value = false; }
}

function normalizeError(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

onMounted(() => {
  void load();
  livePoll = setInterval(() => { void refreshLiveImu(); void refreshGamepad(); void refreshTouchpad(); void refreshAndroidAudio(); void refreshAndroidCamera(); void refreshQuickActions(); }, 500);
  window.addEventListener("capyio-native-state", refreshSnapshot);
});
onUnmounted(() => {
  if (livePoll) clearInterval(livePoll);
  window.removeEventListener("capyio-native-state", refreshSnapshot);
});

function refreshSnapshot(): void {
  void Promise.all([api.getSnapshot(), api.getGamepadState(), api.getTouchpadState(), api.getAndroidAudioState(), api.getAndroidCameraSettings(), api.getQuickActions()])
    .then(([nextSnapshot, nextGamepad, nextTouchpad, nextAndroidAudio, nextAndroidCamera, nextQuickActions]) => {
      snapshot.value = nextSnapshot;
      gamepad.value = nextGamepad;
      touchpad.value = nextTouchpad;
      androidAudio.value = nextAndroidAudio;
      androidCamera.value = nextAndroidCamera;
      quickActions.value = nextQuickActions;
    })
    .catch((error: unknown) => { errorMessage.value = normalizeError(error); });
}
</script>

<template>
  <main class="app-shell">
    <header class="topbar">
      <a class="brand" href="#overview" aria-label="CapyIO home">
        <span class="brand__mark" aria-hidden="true">IO</span>
        <span><strong>CapyIO</strong><small>Distributed capability fabric</small></span>
      </a>
      <nav class="view-switch" aria-label="Primary views">
        <button type="button" :class="{ active: view === 'quick' }" @click="view = 'quick'">Quick Actions</button>
        <button type="button" :class="{ active: view === 'workspace' }" @click="view = 'workspace'">设备与能力</button>
        <button v-if="!androidNode || gamepad" type="button" :class="{ active: view === 'controller' }" @click="view = 'controller'">Controller</button>
        <button v-if="touchpad" type="button" :class="{ active: view === 'touchpad' }" @click="view = 'touchpad'">Touchpad</button>
      </nav>
      <div class="topbar__actions">
        <StatusPill v-if="snapshot" :tone="snapshot.backendMode === 'tauri_demo' ? 'success' : 'warning'" :label="snapshot.backendMode" />
        <button v-if="androidNode" class="ghost-button" type="button" @click="openAndroidNativeDiagnostics">原生诊断</button>
        <button v-if="!androidNode" class="ghost-button" type="button" :disabled="loading" @click="resetDemo">Reset demo</button>
      </div>
    </header>

    <section v-if="view !== 'controller' && view !== 'touchpad'" id="overview" class="hero-panel hero-panel--compact">
      <div class="hero-panel__copy">
        <p class="eyebrow">Integrated non-display capability labs</p>
        <h1>连接能力，不绑定设备角色。</h1>
        <p>两个对等 Node 通过有方向、强类型的 Route 组合音频、摄像头、IMU、手柄与触控板能力。每条 Route 独立启动和停止。</p>
        <div class="hero-panel__summary">
          <div><span>Local Node</span><strong>{{ localNode?.displayName ?? "Loading…" }}</strong></div>
          <div><span>Peer Node</span><strong>{{ remoteNode?.displayName ?? "No peer" }}</strong></div>
          <div><span>Active Routes</span><strong>{{ activeCount }} / {{ snapshot?.routes.length ?? 0 }}</strong></div>
        </div>
      </div>
      <div class="hero-panel__diagram" aria-label="Symmetric Node model">
        <div class="node-box node-box--phone"><span>Node · {{ localNode?.platform ?? "local" }}</span><strong>{{ localNode?.displayName ?? "Loading…" }}</strong><small>local capability catalog</small></div>
        <div class="flow-line"><span>typed Routes</span><i></i><span>both directions</span></div>
        <div class="node-box node-box--pc"><span>Node · {{ remoteNode?.platform ?? "peer" }}</span><strong>{{ remoteNode?.displayName ?? "No connected peer" }}</strong><small>remote capability catalog</small></div>
      </div>
    </section>

    <section v-if="view !== 'controller' && view !== 'touchpad' && snapshot?.warnings.length" class="warning-stack" aria-label="Warnings">
      <p v-for="warning in snapshot.warnings" :key="warning">{{ warning }}</p>
    </section>
    <details class="info-panel">
      <summary>版本与运行检查</summary>
      <button class="ghost-button" type="button" @click="refreshDiagnostics">刷新检查</button>
      <p v-if="diagnosticsError" role="alert">{{ diagnosticsError }}</p>
      <template v-if="diagnostics">
        <p>{{ diagnostics.backend }} · {{ diagnostics.version }}</p>
        <p>源码：{{ diagnostics.revision }} · 构建时间：{{ diagnostics.builtAt }}</p>
        <dl><template v-for="check in diagnostics.checks" :key="check.label"><dt>{{ check.label }}</dt><dd>{{ check.detail }}</dd></template></dl>
      </template>
    </details>
    <p v-if="errorMessage" class="error-banner" role="alert">{{ errorMessage }}</p>
    <section v-if="loading && !snapshot" class="loading-panel">Loading Runtime snapshot…</section>

    <section v-if="view === 'quick' && liveImu" class="info-panel imu-lab imu-live" aria-labelledby="imu-live-title">
      <header class="imu-lab__header">
        <div>
          <p class="eyebrow">Physical IMU lab · trusted LAN only</p>
          <h2 id="imu-live-title">Android 实时加速度与角速度</h2>
          <p>{{ liveImu.profile }} · Runtime {{ liveImu.routeState }} · epoch {{ liveImu.streamEpoch }}<template v-if="liveImu.sequence !== null"> · sequence {{ liveImu.sequence }}</template></p>
        </div>
        <StatusPill :tone="liveImuTone" :label="liveImu.status" />
      </header>
      <form class="imu-live__controls" @submit.prevent="startLiveImu">
        <label>手机 IP<input v-model="liveImuIp" type="text" inputmode="decimal" autocomplete="off" placeholder="例如 192.168.1.20" :disabled="liveImuRunning || liveImu.status === 'unsupported'" /></label>
        <label>端口<input v-model.number="liveImuPort" type="number" min="1" max="65535" :disabled="liveImuRunning || liveImu.status === 'unsupported'" /></label>
        <button class="primary-button" type="submit" :disabled="liveImuBusy || liveImuRunning || liveImu.status === 'unsupported' || !liveImuIp.trim()">连接</button>
        <button class="ghost-button" type="button" :disabled="liveImuBusy || !liveImuRunning" @click="stopLiveImu">停止</button>
      </form>
      <p v-if="liveImu.problem" class="error-banner" role="status"><strong v-if="liveImu.problemCode">{{ liveImu.problemCode }} · </strong>{{ liveImu.problem }}</p>
      <div class="imu-lab__grid">
        <div>
          <h3>Live Numeric Panel</h3>
          <div class="metric-grid">
            <div class="metric-tile"><span class="metric-tile__label">Acceleration X</span><strong class="metric-tile__value">{{ liveImu.acceleration?.x.toFixed(3) ?? "—" }}</strong><span class="metric-tile__detail">m/s²</span></div>
            <div class="metric-tile"><span class="metric-tile__label">Acceleration Y</span><strong class="metric-tile__value">{{ liveImu.acceleration?.y.toFixed(3) ?? "—" }}</strong><span class="metric-tile__detail">m/s²</span></div>
            <div class="metric-tile"><span class="metric-tile__label">Acceleration Z</span><strong class="metric-tile__value">{{ liveImu.acceleration?.z.toFixed(3) ?? "—" }}</strong><span class="metric-tile__detail">m/s²</span></div>
          </div>
          <p class="imu-lab__detail">Angular velocity: {{ liveImu.angularVelocity ? `${liveImu.angularVelocity.x.toFixed(3)}, ${liveImu.angularVelocity.y.toFixed(3)}, ${liveImu.angularVelocity.z.toFixed(3)}` : "—" }} rad/s</p>
        </div>
        <dl class="imu-lab__sinks">
          <div><dt>Runtime Route</dt><dd><StatusPill :tone="liveImuTone" :label="liveImu.routeState" /><span>{{ liveImu.routeId }}</span></dd></div>
          <div><dt>Connection</dt><dd><StatusPill :tone="liveImuTone" :label="liveImu.status" /><span>{{ liveImu.endpoint ?? "未配置" }}</span></dd></div>
          <div><dt>Live samples</dt><dd><span>{{ liveImu.receivedSamples }} received · {{ liveImu.clockDomainId ?? "no clock yet" }}</span></dd></div>
          <div><dt>Evidence boundary</dt><dd><span>真实手机数据；明文 ws:// 仅限可信局域网实验，不代表生产安全。</span></dd></div>
        </dl>
      </div>
    </section>

    <section v-if="view === 'quick' && androidAudio" class="info-panel imu-lab" aria-labelledby="android-audio-title">
      <header class="imu-lab__header">
        <div>
          <p class="eyebrow">Android Node · platform-owned audio</p>
          <h2 id="android-audio-title">本机音频状态</h2>
          <p>{{ androidAudio.nodeId }} · 麦克风与扬声器生命周期相互独立</p>
        </div>
      </header>
      <div class="imu-lab__grid">
        <article>
          <h3>Phone microphone · Source</h3>
          <p><StatusPill :tone="androidAudio.microphone.state === 'active' ? 'success' : androidAudio.microphone.state === 'failed' ? 'danger' : 'warning'" :label="androidAudio.microphone.state" /></p>
          <p class="imu-lab__detail">{{ describeAudio(androidAudio.microphone) }}</p>
          <p class="imu-lab__detail">{{ androidAudio.microphone.framesProcessed }} frames · generation {{ androidAudio.microphone.generation }} · permission {{ androidAudio.microphone.permissionReady ? 'ready' : 'required on Start' }}</p>
          <p v-if="androidAudio.microphone.problemCode" class="error-banner">{{ androidAudio.microphone.problemCode }}</p>
        </article>
        <article>
          <h3>Phone speaker · Sink</h3>
          <p><StatusPill :tone="androidAudio.speaker.state === 'active' ? 'success' : androidAudio.speaker.state === 'failed' ? 'danger' : 'warning'" :label="androidAudio.speaker.state" /></p>
          <p class="imu-lab__detail">{{ describeAudio(androidAudio.speaker) }}</p>
          <p class="imu-lab__detail">{{ androidAudio.speaker.framesProcessed }} frames · generation {{ androidAudio.speaker.generation }} · notification {{ androidAudio.speaker.permissionReady ? 'ready' : 'required on Start' }}</p>
          <p v-if="androidAudio.speaker.problemCode" class="error-banner">{{ androidAudio.speaker.problemCode }}</p>
        </article>
      </div>
    </section>

    <section v-if="view === 'quick' && androidCamera" class="info-panel imu-lab" aria-labelledby="android-camera-title">
      <header class="imu-lab__header">
        <div>
          <p class="eyebrow">Android Node · Camera2 Source</p>
          <h2 id="android-camera-title">本机摄像头设置</h2>
          <p>选择只在停止状态生效；权限、采集生命周期、共享勾选与 Route 保持分离。</p>
        </div>
        <StatusPill :tone="androidCamera.state === 'active' ? 'success' : androidCamera.state === 'failed' ? 'danger' : 'warning'" :label="androidCamera.state" />
      </header>
      <form class="imu-live__controls" @submit.prevent="refreshCameraSources">
        <label>具体 Camera2 来源
          <select :value="androidCamera.selectedSourceKey ?? ''" :disabled="cameraSettingsBusy || ['starting', 'active'].includes(androidCamera.state) || !androidCamera.permissionReady" @change="selectCameraSource">
            <option value="">自动（优先后置）</option>
            <option v-for="source in androidCamera.sources" :key="source.key" :value="source.key">{{ cameraSourceLabel(source) }}</option>
          </select>
        </label>
        <label>AVC 质量
          <select :value="androidCamera.quality" :disabled="cameraSettingsBusy || ['starting', 'active'].includes(androidCamera.state)" @change="selectCameraQuality">
            <option value="economy">Economy · 2 Mbit/s 基准</option>
            <option value="balanced">Balanced · 4 Mbit/s 基准</option>
            <option value="clear">Clear · 6 Mbit/s 基准</option>
          </select>
        </label>
        <button class="ghost-button" type="submit" :disabled="cameraSettingsBusy || ['starting', 'active'].includes(androidCamera.state) || !androidCamera.permissionReady">刷新硬件来源</button>
      </form>
      <p v-if="!androidCamera.permissionReady" class="gamepad-lab__note">首次在 Quick Actions 启动 Camera 后由 Android 请求权限；停止后即可刷新并选择具体 Camera2 ID/厂商 Zoom 目标。</p>
      <p v-if="androidCamera.problemCode" class="error-banner" role="status">{{ androidCamera.problemCode }}</p>
    </section>

    <AndroidAudioConnection v-if="view === 'quick' && androidAudio" :api="api" :state="androidAudio" />
    <DisplayReceiverPanel v-if="view === 'quick'" :api="api" />

    <section v-if="view === 'controller' && gamepad" class="info-panel gamepad-lab gamepad-lab--primary" aria-labelledby="gamepad-lab-title">
      <header class="imu-lab__header">
        <div>
          <p class="eyebrow">CapyIO Controller · touch + motion composition</p>
          <h1 id="gamepad-lab-title">触控体感手柄</h1>
          <p>{{ gamepad.profile }} · epoch {{ gamepad.streamEpoch }} · sequence {{ gamepad.sequence ?? "尚未输入" }}</p>
        </div>
        <div class="gamepad-lab__actions">
          <StatusPill :tone="gamepad.simulated ? 'warning' : 'success'" :label="gamepad.simulated ? 'simulated source' : 'android live source'" />
          <button class="ghost-button" type="button" :disabled="gamepadBusy || (androidNode ? gamepad.androidInput.status !== 'connected' : ['listening', 'connected'].includes(gamepad.androidInput.status))" @click="updateGamepad({ kind: 'reset' })">全部归零</button>
        </div>
      </header>
      <template v-if="androidNode">
      <p class="gamepad-lab__note">先在 Windows 的同一界面启动“监听手机”，再把显示的 IPv4、UDP 端口和 token 填到这里。配置完成后，从 Quick Actions 启动 Touch gamepad、IMU 或两者；统一 Service 会把完整控制状态与独立传感器采样送到 Windows。</p>
      <form class="dsu-lab-bar android-lab-bar" @submit.prevent="configureAndroidController">
        <label>Windows IPv4<input v-model="gamepadAndroidHost" type="text" inputmode="decimal" placeholder="100.66.231.100" :disabled="gamepadAndroidBusy || gamepad.androidInput.status === 'connected'" /></label>
        <label>UDP 端口<input v-model.number="gamepadAndroidPort" type="number" min="1" max="65535" :disabled="gamepadAndroidBusy || gamepad.androidInput.status === 'connected'" /></label>
        <label>桌面 token<input v-model="gamepadAndroidToken" type="text" autocomplete="off" placeholder="8–64 位十六进制" :disabled="gamepadAndroidBusy || gamepad.androidInput.status === 'connected'" /></label>
        <button class="primary-button" type="submit" :disabled="gamepadAndroidBusy || gamepad.androidInput.status === 'connected'">保存发送目标</button>
        <StatusPill :tone="gamepad.androidInput.status === 'connected' ? 'success' : gamepad.androidInput.status === 'failed' ? 'danger' : 'warning'" :label="gamepad.androidInput.status" />
      </form>
      <p class="gamepad-lab__note">当前目标：{{ gamepad.androidInput.endpoint ?? '尚未配置' }}</p>
      </template>
      <template v-else>
      <p class="gamepad-lab__note">桌面模拟器和 Android 真机共用完整 <code>GamepadState</code> 检查器。Android 每个 UDP 数据报携带按键、摇杆、扳机与 IMU 完整快照；序列回退会被拒绝，超过 350ms 未收到数据会强制手柄归零。</p>
      <div class="dsu-lab-bar android-lab-bar">
        <div><span>Android controller input</span><strong>{{ gamepad.androidInput.lanHostHint ?? '本机局域网 IPv4' }}:{{ gamepadAndroidPort }} · token {{ gamepad.androidInput.pairingToken ?? '启动后生成' }}</strong></div>
        <label>UDP 端口<input v-model.number="gamepadAndroidPort" type="number" min="1" max="65535" :disabled="gamepadAndroidBusy || gamepad.androidInput.status === 'listening' || gamepad.androidInput.status === 'connected'" /></label>
        <button class="primary-button" type="button" :disabled="gamepadAndroidBusy || gamepad.androidInput.status === 'listening' || gamepad.androidInput.status === 'connected' || gamepad.dsuProjection.status === 'active'" @click="startAndroidGamepad">监听手机</button>
        <button class="ghost-button" type="button" :disabled="gamepadAndroidBusy || !['listening', 'connected'].includes(gamepad.androidInput.status) || gamepad.dsuProjection.status === 'active'" @click="stopAndroidGamepad">停止监听</button>
        <StatusPill :tone="gamepad.androidInput.status === 'connected' ? 'success' : gamepad.androidInput.status === 'failed' ? 'danger' : 'warning'" :label="gamepad.androidInput.status" />
      </div>
      <div class="dsu-stats" aria-label="Android controller input counters">
        <span>Accepted <strong>{{ gamepad.androidInput.acceptedPackets }}</strong></span>
        <span>Rejected <strong>{{ gamepad.androidInput.rejectedPackets }}</strong></span>
        <span>Replay <strong>{{ gamepad.androidInput.replayedPackets }}</strong></span>
        <span>Timeout <strong>{{ gamepad.androidInput.peerTimeouts }}</strong></span>
        <span>Age <strong>{{ gamepad.androidInput.packetAgeMillis ?? '—' }} ms</strong></span>
        <span>Remote seq <strong>{{ gamepad.androidInput.remoteSequence ?? '—' }}</strong></span>
        <span>Last <strong>{{ gamepad.androidInput.lastEvent }}</strong></span>
      </div>
      <div class="dsu-lab-bar">
        <div><span>DSU loopback projection</span><strong>{{ gamepad.dsuProjection.endpoint ?? "127.0.0.1 · 未启动" }}</strong></div>
        <label>端口<input v-model.number="gamepadDsuPort" type="number" min="1" max="65535" :disabled="gamepadDsuBusy || gamepad.dsuProjection.status === 'active' || !gamepad.dsuProjection.supported" /></label>
        <label>数据<select v-model="gamepadDsuMode" :disabled="gamepadDsuBusy || gamepad.dsuProjection.status === 'active' || !gamepad.dsuProjection.supported"><option value="motion_only">仅手机体感</option><option value="motion_and_controls">手机体感 + 触控按键</option></select></label>
        <button class="primary-button" type="button" :disabled="gamepadDsuBusy || gamepad.dsuProjection.status === 'active' || !gamepad.dsuProjection.supported" @click="startGamepadDsu">启动 DSU</button>
        <button class="ghost-button" type="button" :disabled="gamepadDsuBusy || gamepad.dsuProjection.status !== 'active'" @click="stopGamepadDsu">停止</button>
        <StatusPill :tone="gamepad.dsuProjection.status === 'active' ? 'success' : gamepad.dsuProjection.status === 'failed' ? 'danger' : 'warning'" :label="gamepad.dsuProjection.status" />
      </div>
      <div class="dsu-stats" aria-label="DSU projection counters">
        <span>Controls <strong>{{ gamepad.dsuProjection.controlsAccepted }} / {{ gamepad.dsuProjection.controlsSubmitted }}</strong></span>
        <span>Mode <strong>{{ gamepad.dsuProjection.mode }}</strong></span>
        <span>Subscribers <strong>{{ gamepad.dsuProjection.activeSubscribers }}</strong></span>
        <span>Packets <strong>{{ gamepad.dsuProjection.padPacketsSent }}</strong></span>
        <span>Queue full <strong>{{ gamepad.dsuProjection.controlsQueueFull }}</strong></span>
        <span>Send errors <strong>{{ gamepad.dsuProjection.packetSendErrors }}</strong></span>
        <span>Last <strong>{{ gamepad.dsuProjection.lastSubmit }}</strong></span>
      </div>
      <div class="dsu-lab-bar windows-gamepad-bar" aria-label="Windows virtual controller projection status">
        <div>
          <span>Windows virtual controller projection</span>
          <strong>{{ gamepad.windowsProjection.deviceIdentity }} · {{ gamepad.windowsProjection.busId ?? '尚未创建 bus' }}</strong>
        </div>
        <label>设备<select v-model="gamepadWindowsKind" :disabled="gamepadWindowsBusy || gamepad.windowsProjection.status === 'active'"><option value="dualshock4">DualShock 4（原生体感）</option><option value="xbox360">Xbox 360（无原生体感）</option></select></label>
        <div>
          <span>Loopback boundaries</span>
          <strong>VIIPER {{ gamepad.windowsProjection.viiperEndpoint ?? '—' }} · USB/IP {{ gamepad.windowsProjection.usbipEndpoint ?? '—' }}</strong>
        </div>
        <div>
          <span>Owned USB/IP port</span>
          <strong>{{ gamepad.windowsProjection.ownedUsbipPort ?? '—' }}</strong>
        </div>
        <div>
          <span>Read-only preflight</span>
          <strong>VIIPER {{ gamepad.windowsProjection.viiperReady ? 'ready' : '—' }} · USB/IP {{ gamepad.windowsProjection.usbipReady ? 'ready' : '—' }} · XInput runtime {{ gamepad.windowsProjection.xinputAvailable ? 'available' : '—' }} · active {{ gamepad.windowsProjection.xinputReady ? 'yes' : 'no' }} · exports {{ gamepad.windowsProjection.exportCount }}</strong>
        </div>
        <div>
          <span>Complete-state ingress</span>
          <strong>packets {{ gamepad.windowsProjection.inputPackets }} · non-neutral {{ gamepad.windowsProjection.nonNeutralPackets }} · DS4 rejected {{ gamepad.windowsProjection.ds4RejectedPackets }} · XInput {{ gamepad.windowsProjection.xinputPackets }} · offline {{ gamepad.windowsProjection.inputOfflineEvents }} · remote seq {{ gamepad.windowsProjection.lastRemoteSequence ?? '—' }}</strong>
        </div>
        <button class="ghost-button" type="button" :disabled="gamepadWindowsBusy || !gamepad.windowsProjection.supported || gamepad.windowsProjection.status === 'active'" @click="refreshWindowsGamepadPreflight">{{ gamepadWindowsBusy ? '检查中…' : '只读预检' }}</button>
        <label><input v-model="gamepadXinputCompanion" type="checkbox" :disabled="gamepadWindowsBusy || gamepad.windowsProjection.status === 'active' || !gamepad.windowsProjection.xinputAvailable" /> Direct XInput 兼容设备</label>
        <button v-if="gamepad.windowsProjection.status !== 'active'" class="primary-button" type="button" :disabled="gamepadWindowsBusy || gamepadWindowsKind !== 'dualshock4' || !gamepad.androidInput.endpoint" @click="startWindowsGamepadProjection">启动 DS4</button>
        <button v-else class="danger-button" type="button" :disabled="gamepadWindowsBusy" @click="stopWindowsGamepadProjection">停止 DS4</button>
        <StatusPill :tone="gamepad.windowsProjection.status === 'active' ? 'success' : gamepad.windowsProjection.status === 'failed' || gamepad.windowsProjection.status === 'offline' ? 'danger' : 'warning'" :label="gamepad.windowsProjection.status" />
      </div>
      <p v-if="gamepad.windowsProjection.problem" class="error-banner" role="status"><strong v-if="gamepad.windowsProjection.problemCode">{{ gamepad.windowsProjection.problemCode }} · </strong>{{ gamepad.windowsProjection.problem }}</p>
      <p class="gamepad-lab__note">默认只发布完整 DS4：其按键、摇杆与 IMU 已通过 Windows Raw HID/WGI 动态报告验证，适合 Steam 的 PlayStation 支持与 DS4-aware 应用。Direct XInput 兼容设备仅供直接调用 XInput 的旧游戏显式启用；本机验证显示虚拟 Xbox 在 WGI 中可枚举但不会更新，因此不建议为 Steam 或浏览器启用。停止会先发送安全中立态，再精确卸载本次拥有的设备；此入口不安装或卸载驱动，也不触发重启。</p>
      </template>
      <div class="gamepad-lab__grid">
        <div><h3>{{ androidNode ? 'Android local touch source' : gamepad.simulated ? 'Desktop virtual input source' : 'Android input mirror' }}</h3><GamepadSurface :state="gamepad" :disabled="androidNode ? gamepad.androidInput.status !== 'connected' : !gamepad.simulated" @update="updateGamepad" /></div>
        <div><h3>{{ androidNode ? 'Local state inspector' : 'PC live inspector' }}</h3><GamepadTelemetry :state="gamepad" /></div>
      </div>
    </section>

    <section v-if="view === 'touchpad' && touchpad" class="info-panel gamepad-lab gamepad-lab--primary" aria-labelledby="touchpad-title">
      <header class="imu-lab__header">
        <div>
          <p class="eyebrow">CapyIO Touchpad · Android local source</p>
          <h1 id="touchpad-title">多点触控板</h1>
          <p>{{ touchpad.profile }} · epoch {{ touchpad.streamEpoch }} · sequence {{ touchpad.sequence }}</p>
        </div>
        <div class="gamepad-lab__actions">
          <StatusPill :tone="touchpad.status === 'active' ? 'success' : touchpad.status === 'failed' ? 'danger' : 'warning'" :label="touchpad.status" />
          <button class="ghost-button" type="button" :disabled="touchpadBusy || touchpad.status !== 'active'" @click="updateTouchpad({ contacts: [] })">释放全部触点</button>
        </div>
      </header>
      <p class="gamepad-lab__note">请先在 Quick Actions 启动 Touchpad surface。页面每帧提交最多 5 个规范化触点的完整状态；移动事件按浏览器帧合并，取消、离开页面和 Stop 都释放全部触点。当前是本机真实触摸输入边界，尚未宣称已通过 Route 到达 Windows Precision Touchpad。</p>
      <div class="gamepad-lab__grid">
        <div><h3>Android local touch source</h3><TouchpadSurface :state="touchpad" :disabled="touchpad.status !== 'active'" @update="updateTouchpad" /></div>
        <div>
          <h3>Complete state inspector</h3>
          <dl class="imu-lab__sinks">
            <div><dt>Lifecycle</dt><dd><StatusPill :tone="touchpad.status === 'active' ? 'success' : 'warning'" :label="touchpad.status" /><span>generation {{ touchpad.streamEpoch }}</span></dd></div>
            <div><dt>Sequence</dt><dd><span>{{ touchpad.sequence }}</span></dd></div>
            <div><dt>Contacts</dt><dd><span>{{ touchpad.contacts.length }} / 5 active</span></dd></div>
            <div v-for="contact in touchpad.contacts" :key="contact.contactId"><dt>Contact {{ contact.contactId + 1 }}</dt><dd><span>x {{ contact.x }} · y {{ contact.y }} · pressure {{ contact.pressure }}</span></dd></div>
          </dl>
        </div>
      </div>
    </section>

    <section v-if="view === 'quick' && snapshot?.imuFixture" class="info-panel imu-lab" aria-labelledby="imu-lab-title">
      <header class="imu-lab__header">
        <div>
          <p class="eyebrow">IMU StandardPort lab</p>
          <h2 id="imu-lab-title">同一份有界数据，独立送往 Panel 与 Recorder</h2>
          <p>{{ snapshot.imuFixture.profile }} · sequence {{ snapshot.imuFixture.sequence }} · {{ snapshot.imuFixture.clockDomainId }}</p>
        </div>
        <StatusPill tone="warning" label="deterministic fixture" />
      </header>
      <div class="imu-lab__grid">
        <div>
          <h3>Numeric Panel</h3>
          <div class="metric-grid">
            <div class="metric-tile"><span class="metric-tile__label">Acceleration X</span><strong class="metric-tile__value">{{ snapshot.imuFixture.acceleration.x.toFixed(3) }}</strong><span class="metric-tile__detail">m/s²</span></div>
            <div class="metric-tile"><span class="metric-tile__label">Acceleration Y</span><strong class="metric-tile__value">{{ snapshot.imuFixture.acceleration.y.toFixed(3) }}</strong><span class="metric-tile__detail">m/s²</span></div>
            <div class="metric-tile"><span class="metric-tile__label">Acceleration Z</span><strong class="metric-tile__value">{{ snapshot.imuFixture.acceleration.z.toFixed(3) }}</strong><span class="metric-tile__detail">m/s²</span></div>
          </div>
          <p class="imu-lab__detail">Angular velocity: {{ snapshot.imuFixture.angularVelocity.x.toFixed(3) }}, {{ snapshot.imuFixture.angularVelocity.y.toFixed(3) }}, {{ snapshot.imuFixture.angularVelocity.z.toFixed(3) }} rad/s</p>
        </div>
        <dl class="imu-lab__sinks">
          <div><dt>Panel Route</dt><dd><StatusPill tone="success" :label="snapshot.imuFixture.panelRouteState" /><span>{{ snapshot.imuFixture.panelReceived }} samples · {{ snapshot.imuFixture.panelMissingSequences }} missing</span></dd></div>
          <div><dt>Recorder Route</dt><dd><StatusPill tone="success" :label="snapshot.imuFixture.recorderRouteState" /><span>{{ snapshot.imuFixture.recorderRecords }} bounded JSONL records</span></dd></div>
          <div><dt>Evidence boundary</dt><dd><span>编译内确定性 fixture；不是手机实时传感器数据。</span></dd></div>
        </dl>
      </div>
    </section>

    <template v-if="view === 'quick'">
      <section class="section-heading">
        <div><p class="eyebrow">Quick Actions</p><h2>按需启动互不耦合的 Route</h2></div>
        <p>这里隐藏 Adapter 与 Port 细节，只呈现用户要完成的硬件组合。</p>
      </section>
      <section class="capability-grid">
        <QuickActionCard v-for="action in quickActions" :key="action.id" :action="action" :busy="busyActionId === action.id" :audio-endpoints="audioEndpoints?.actionId === action.id ? audioEndpoints : null" @invoke="invokeQuickAction" @refresh-endpoints="refreshAudioEndpoints" @select-endpoint="selectAudioEndpoint" />
        <RouteCard v-for="route in ordinaryQuickRoutes" :key="route.id" :route="route" :busy="busyRouteId === route.id" @toggle="toggleRoute" />
      </section>
    </template>

    <template v-else-if="view === 'workspace'">
      <section class="section-heading">
        <div><p class="eyebrow">Devices & capabilities</p><h2>共享本机硬件，使用远端能力</h2></div>
        <p>界面按任务分区，但不会把整台 Node 固定成发送端或接收端；实际方向始终属于每个 Port。</p>
      </section>

      <section class="catalog-pair">
        <CapabilityCatalog
          catalog-id="local-capabilities"
          eyebrow="Local capability catalog"
          title="本机能力"
          description="Runtime 从本机 Adapter 目录汇总能力。Source Port 可以逐项勾选；Sink 与 Control Port 仍作为接收、投影或控制目标显示。"
          :capabilities="localCapabilities"
          :busy-id="busyCapabilityId"
          @toggle="toggleCapability"
        />
        <CapabilityCatalog
          catalog-id="peer-capabilities"
          eyebrow="Peer capability catalog"
          title="远端节点公开的能力"
          description="目录来自已连接节点并保持只读。Source 表示可从远端使用，Sink 表示可将本机能力路由到远端；授权在建立具体 Route 时单独完成。"
          :capabilities="peerCapabilities"
          :busy-id="busyCapabilityId"
          @toggle="toggleCapability"
        />
      </section>

      <section class="workspace-grid">
        <article class="info-panel">
          <p class="eyebrow">Nodes</p><h2>对等节点目录</h2>
          <ul class="workspace-list">
            <li v-for="node in snapshot?.nodes ?? []" :key="node.id">
              <div><strong>{{ node.displayName }}</strong><span>{{ node.platform }} · {{ node.local ? "local" : "peer" }}</span></div>
              <StatusPill :tone="node.online ? 'success' : 'warning'" :label="node.online ? 'online' : 'offline'" />
            </li>
          </ul>
        </article>
        <article class="info-panel">
          <p class="eyebrow">Adapters</p><h2>显式部署边界</h2>
          <ul class="workspace-list">
            <li v-for="adapter in snapshot?.adapters ?? []" :key="adapter.id">
              <div><strong>{{ adapter.displayName }}</strong><span>{{ adapter.nodeName }} · {{ adapter.deploymentMode }} · {{ adapter.capabilityCount }} capabilities</span></div>
              <StatusPill :tone="adapter.health === 'healthy' ? 'success' : 'warning'" :label="adapter.state" />
            </li>
          </ul>
        </article>
      </section>

      <article class="info-panel route-table-panel">
        <p class="eyebrow">Route graph</p><h2>强类型连接</h2>
        <div class="route-table-wrap">
          <table class="route-table">
            <thead><tr><th>Source Port</th><th>Profile</th><th>Sink Port</th><th>Backend</th><th>State</th></tr></thead>
            <tbody><tr v-for="route in snapshot?.routes ?? []" :key="route.id"><td>{{ route.source.nodeName }}<small>{{ route.source.portName }}</small></td><td>{{ route.profile }}</td><td>{{ route.sink.nodeName }}<small>{{ route.sink.portName }}</small></td><td>{{ route.backend }}</td><td>{{ route.state }}</td></tr></tbody>
          </table>
        </div>
      </article>
    </template>

    <section v-if="view !== 'controller' && view !== 'touchpad'" class="lower-grid">
      <article class="info-panel">
        <p class="eyebrow">Architecture contract</p><h2>共享语义，隔离平台实现</h2>
        <ol class="contract-list">
          <li><strong>Core</strong><span>Node、Capability、Port、Route、Session、Problem</span></li>
          <li><strong>Adapter</strong><span>硬件枚举、权限、数据面和系统投影</span></li>
          <li><strong>Profile</strong><span>格式、QoS、时钟与互操作语义</span></li>
          <li><strong>Panel</strong><span>当系统投影不可用时的应用内 Sink</span></li>
        </ol>
      </article>
      <article class="event-panel">
        <header><div><p class="eyebrow">Runtime events</p><h2>可追踪状态变化</h2></div><span>{{ snapshot?.events.length ?? 0 }} retained</span></header>
        <ol class="event-list"><li v-for="event in recentEvents" :key="event.sequence"><span>#{{ event.sequence.toString().padStart(3, "0") }}</span><p>{{ event.summary }}</p></li></ol>
      </article>
    </section>

    <footer v-if="view !== 'controller' && view !== 'touchpad'" class="app-footer"><span>CapyIO pre-alpha</span><span>Protocol 1.0 · IMU + input + bidirectional audio labs</span><span>当前明文实验传输仅限可信网络</span></footer>
  </main>
</template>
