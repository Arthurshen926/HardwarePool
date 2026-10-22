<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";

import ResourceIcon from "./components/ResourceIcon.vue";
import RouteCard from "./components/RouteCard.vue";
import AndroidAudioConnection from "./components/AndroidAudioConnection.vue";
import AudioUsePanel from './components/AudioUsePanel.vue';
import { androidResourceEntry, controllerResourceMode } from './lib/android-resource-entry';
import CameraUsePanel from './components/CameraUsePanel.vue';
import DisplayReceiverPanel from "./components/DisplayReceiverPanel.vue";
import CapabilityCatalog from "./components/CapabilityCatalog.vue";
import PairedPeerDiagnostics from "./components/PairedPeerDiagnostics.vue";
import PeerEnrollmentPanel from "./components/PeerEnrollmentPanel.vue";
import GamepadSurface from "./components/GamepadSurface.vue";
import GamepadTelemetry from "./components/GamepadTelemetry.vue";
import ControllerSettingsPanel from './components/ControllerSettingsPanel.vue';
import ControllerUsePanel from './components/ControllerUsePanel.vue';
import QuickActionCard from "./components/QuickActionCard.vue";
import StatusPill from "./components/StatusPill.vue";
import TouchpadSurface from "./components/TouchpadSurface.vue";
import { enqueueTouchpadUpdate } from "./lib/touchpadQueue";
import { createCapyIOApi } from "./lib/api";
import { ownedGamepadFeedback } from './lib/gamepad-feedback';
import { selectAutomaticPeerTarget } from "./lib/automatic-peer-target";
import { selectDiscoveryInterface } from "./lib/discovery-interfaces";
import { encodeTrustedEndpointHints, parseTrustedEndpointHints, rememberAuthenticatedEndpoint,
  retainActivelyPairedEndpoints, type TrustedEndpointHint } from "./lib/trusted-endpoints";
import { controlConnectionError, requiresConnectionAttention } from "./lib/peer-connection";
import { automaticConnectionStatus } from "./lib/automatic-connection-status";
import { hasIntegratedRemoteDataBinding, knownRouteUsage } from "./lib/remote-resource-support";
import { clearConnectionProfile, readConnectionProfile, saveConnectionProfile } from "./lib/connection-profile";
import { actionPresentation, backResourceNavigation, createResourceNavigation, initialResourceView, isHomeView, localResourceBindings, moveResourceNavigation, peerResourceGroups, primaryHardwareResources, resourceActions, type ResourceView } from "./lib/resource-navigation";
import type { DsuProjectionMode, GamepadControlUpdate, QuickActionOperation, TouchpadControlUpdate, UiAndroidAudioCapability, UiAndroidAudioState, UiAndroidCameraSettings, UiAudioEndpointCatalog, UiCapability, UiGamepadState, UiLiveImu, UiQuickAction, UiRoute, UiSnapshot, UiTouchpadState, WindowsControllerKind } from "./lib/types";

const api = createCapyIOApi();
let initialPreferredPeerHost = "";
let initialConnectionProfileMessage = "";
try { initialPreferredPeerHost = readConnectionProfile(window.localStorage)?.preferredPeerIpv4 ?? ""; }
catch { initialConnectionProfileMessage = "已保存的首选对端地址无效，请重新填写。"; }
const AUTO_CONNECT_STORAGE_KEY = "capyio.auto-connect-paired.v1";
const DISCOVERY_INTERFACE_STORAGE_KEY = "capyio.discovery-interface.v1";
const TRUSTED_ENDPOINT_STORAGE_KEY = "capyio.authenticated-endpoints.v1";
let initialAutoConnectPaired = true;
let initialDiscoveryInterfaceIpv4 = "";
let initialTrustedEndpointHints: TrustedEndpointHint[] = [];
try { initialAutoConnectPaired = window.localStorage.getItem(AUTO_CONNECT_STORAGE_KEY) !== "false"; }
catch { /* Automatic trusted-peer connection remains enabled for this session. */ }
try {
  const storedInterface = window.localStorage.getItem(DISCOVERY_INTERFACE_STORAGE_KEY) ?? "";
  if (/^(?:[0-9]{1,3}\.){3}[0-9]{1,3}$/.test(storedInterface)) initialDiscoveryInterfaceIpv4 = storedInterface;
} catch { /* Native inventory validation remains authoritative. */ }
try { initialTrustedEndpointHints = parseTrustedEndpointHints(window.localStorage.getItem(TRUSTED_ENDPOINT_STORAGE_KEY)); }
catch { /* Corrupt routing hints are ignored; native pairing remains authoritative. */ }
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
const initializationProblems = ref<{ label: string; detail: string }[]>([]);
type PrimaryView = ResourceView;
const requestedView = new URLSearchParams(window.location.search).get("view");
let savedView: string | null = null;
try { savedView = window.localStorage.getItem("capyio.home-view"); } catch { /* Some native hosts disable web storage. */ }
const initialView = initialResourceView(requestedView, savedView);
const navigation = ref(createResourceNavigation(initialView));
const view = computed(() => navigation.value.current.view);
const parentView = computed(() => navigation.value.home);
const selectedActionId = computed(() => navigation.value.current.actionId);
const backLabel = computed(() => {
  const previous = navigation.value.history.at(-1);
  if (previous?.view === "quick") {
    const action = quickActions.value.find(item => item.id === previous.actionId);
    return action ? actionPresentation(action).title : "资源详情";
  }
  return { local: "本机共享", remote: "远程资源", settings: "设置", controller: "手柄与体感", touchpad: "触控板", display: "显示共享", imu: "兼容 IMU 接入", workspace: "开发诊断", quick: "资源详情", capability: "资源详情", "audio-settings": "音频连接设置" }[previous?.view ?? parentView.value];
});
const pageHeading = ref<HTMLElement | null>(null);
const selectedAction = computed(() => quickActions.value.find((action) => action.id === selectedActionId.value));
const selectedLocalAudioDirection = computed(() => {
  if (!androidNode.value) return undefined;
  if (selectedActionId.value === 'capyio.quick-action.android-speaker') return 'speaker';
  if (selectedActionId.value === 'capyio.quick-action.android-microphone') return 'microphone';
  return undefined;
});
const selectedCatalogCapability = computed(() => (snapshot.value?.capabilities ?? []).find(capability => capability.id === selectedActionId.value && !capability.simulated));
const selectedCatalogNode = computed(() => (snapshot.value?.nodes ?? []).find(node => node.id === selectedCatalogCapability.value?.nodeId));
const selectedCatalogRoutes = computed(() => (snapshot.value?.routes ?? []).filter(route => route.sourceCapabilityId === selectedCatalogCapability.value?.id || route.sinkCapabilityId === selectedCatalogCapability.value?.id));
const selectedRemoteCapabilityInUse = computed(() => !selectedCatalogCapability.value?.local && selectedCatalogRoutes.value.some(route => ['prepared', 'starting', 'active', 'stopping'].includes(route.state)));
const selectedRemoteCapabilityPending = computed(() => !selectedCatalogCapability.value?.local && selectedCatalogRoutes.value.some(route => route.state === "draft" && route.authorization === "pending"));
// Navigation only: this does not bind the selected peer or authorize a Route.
const selectedCompatibilityAction = computed(() => {
  const capability = selectedCatalogCapability.value;
  if (!capability || capability.local || selectedCatalogNode.value?.platform !== "android") return undefined;
  const actionId = ({ camera: "capyio.quick-action.android-system-camera", speaker: "capyio.quick-action.remote-speaker", microphone: "capyio.quick-action.remote-microphone" } as Record<string, string>)[capability.capabilityClass];
  return quickActions.value.find(action => !action.simulated && action.id === actionId);
});
const selectedCatalogLeases = computed(() => (snapshot.value?.portLeases ?? []).filter(lease => lease.capabilityId === selectedCatalogCapability.value?.id));
const pageTitle = computed(() => ({ local: "本机共享", remote: "远程资源", settings: "设置", quick: selectedAction.value ? actionPresentation(selectedAction.value).title : "资源操作", capability: selectedCatalogCapability.value?.displayName ?? "资源详情", workspace: "开发诊断", controller: "手柄与体感", touchpad: "触控板", display: "显示共享", imu: "兼容 IMU 接入", "audio-settings": "音频连接设置" })[view.value]);
const isPrimaryView = computed(() => isHomeView(view.value));
const workflowPeer = ref<string>();
const workflowControllerMode = ref<import('./lib/types').ControllerUseMode>();
async function navigate(next: PrimaryView, actionId: string | null = selectedActionId.value, peer?: string): Promise<void> {
  workflowControllerMode.value = next === 'controller' && peer ? controllerResourceMode(selectedCatalogCapability.value ?? null) : undefined;
  workflowPeer.value = peer;
  navigation.value = moveResourceNavigation(navigation.value, next, actionId);
  if (isHomeView(next)) {
    try { window.localStorage.setItem("capyio.home-view", next); } catch { /* Navigation remains available without storage. */ }
  }
  await focusPage();
}
async function focusPage(): Promise<void> {
  await nextTick();
  pageHeading.value?.focus({ preventScroll: true });
  pageHeading.value?.scrollIntoView({ block: 'start', behavior: 'instant' });
  document.scrollingElement?.scrollTo({ top: 0, behavior: 'instant' });
  window.scrollTo({ top: 0, behavior: 'instant' });
}
async function goBack(): Promise<void> {
  navigation.value = backResourceNavigation(navigation.value);
  await focusPage();
}
function openAction(action: UiQuickAction): void {
  const controllerAction = ['capyio.quick-action.android-gamepad', 'capyio.quick-action.android-imu', 'capyio.quick-action.android-haptics'].includes(action.id);
  void navigate(controllerAction ? 'controller' : 'quick', action.id);
}
function inspectCapability(capability: UiCapability): void {
  void navigate("capability", capability.id);
}
const liveImu = ref<UiLiveImu | null>(null);
const liveImuIp = ref("");
const liveImuPort = ref(8080);
const liveImuBusy = ref(false);
const gamepad = ref<UiGamepadState | null>(null);
const controllerUse = ref<import('./lib/types').UiControllerUse | null>(null);
const controllerUseBusy = ref(false);
const controllerUseError = ref('');
const controllerPeers = ref<import('./lib/types').UiControllerPeer[]>([]);
const controllerUseNodes = computed(() => {
  if (androidNode.value) return controllerPeers.value.map(peer => ({ id: peer.nodeId, displayName: peer.displayName, platform: 'windows', platformVersion: '', online: true, local: false, capabilityCount: 0 }));
  const catalogNodes = (snapshot.value?.nodes ?? []).filter(node => !node.local && node.online
    && activePairedPeers.value.some(peer => peer.nodeId === node.id)
    && node.platform.toLowerCase() === 'android');
  return catalogNodes;
});
let controllerUseReading = false;
let controllerFullscreenPending = false;
async function refreshControllerUse(): Promise<void> {
  if (controllerUseReading) return;
  controllerUseReading = true;
  try {
    if (androidNode.value) controllerPeers.value = await api.getControllerPeers();
    const previous = controllerUse.value;
    controllerUse.value = await api.getControllerUse();
    if (androidNode.value && controllerUse.value?.status === 'active' && controllerUse.value.mode !== 'dsu'
      && (previous?.status !== 'active' || previous.peerNodeId !== controllerUse.value.peerNodeId)) controllerFullscreenPending = true;
    if (controllerUse.value?.status !== 'active') controllerFullscreenPending = false;
    if (controllerFullscreenPending && !resourceRequests.value.length) {
      if (await enterGamepadFullscreen()) controllerFullscreenPending = false;
    }
    if (previous?.status === 'active' && controllerUse.value?.status !== 'active' && gamepadFullscreenRequested.value) exitGamepadFullscreen();
  } catch (error) { if (androidNode.value) controllerPeers.value = []; controllerUseError.value = normalizeError(error); }
  finally { controllerUseReading = false; }
}
async function startControllerUse(peerNodeId: string, mode: import('./lib/types').ControllerUseMode): Promise<void> {
  if (controllerUseBusy.value) return;
  controllerUseBusy.value = true; controllerUseError.value = '';
  try { await api.startControllerUse(peerNodeId, mode); await refreshControllerUse(); }
  catch (error) { controllerUseError.value = normalizeError(error); }
  finally { controllerUseBusy.value = false; }
}
async function stopControllerUse(): Promise<void> {
  if (controllerUseBusy.value) return;
  controllerUseBusy.value = true; controllerUseError.value = '';
  try { await api.stopControllerUse(); exitGamepadFullscreen(); await refreshControllerUse(); }
  catch (error) { controllerUseError.value = normalizeError(error); }
  finally { controllerUseBusy.value = false; }
}
const gamepadBusy = ref(false);
const gamepadMotion = ref<Awaited<ReturnType<typeof api.getGamepadMotionSettings>>>(null);
const gamepadMotionBusy = ref(false);
const androidControllerMode = ref<'touch' | 'motion'>('touch');
const androidControllerModeBusy = ref(false);
const androidControllerModeMessage = ref('');
const androidControllerActions = computed(() => quickActions.value.filter(action =>
  !action.simulated && ['capyio.quick-action.android-gamepad', 'capyio.quick-action.android-imu'].includes(action.id)));
const androidTouchControllerActive = computed(() => androidControllerActions.value.some(action =>
  action.id === 'capyio.quick-action.android-gamepad' && action.status === 'active'));
async function applyAndroidControllerMode(stop = false): Promise<void> {
  if (androidControllerModeBusy.value) return;
  androidControllerModeBusy.value = true;
  androidControllerModeMessage.value = '';
  try {
    exitGamepadFullscreen();
    await api.setAndroidControllerMode(stop ? 'stopped' : androidControllerMode.value);
    quickActions.value = await api.getQuickActions();
    await refreshGamepad();
    androidControllerModeMessage.value = '命令已提交，实际采集状态见下方。';
  } catch (error) { androidControllerModeMessage.value = normalizeError(error); }
  finally { androidControllerModeBusy.value = false; }
}
const hapticsSettings = ref<Awaited<ReturnType<typeof api.getHapticsSettings>>>(null);
const hapticsBusy = ref(false);
const gamepadFeedbackEnabled = ref(true);
const gamepadFeedbackTarget = ref('');
const gamepadFeedbackMessage = ref('');
const gamepadFeedbackBusy = ref(false);
const gamepadFeedbackSinks = computed(() => (snapshot.value?.capabilities ?? []).filter(c => !c.local && !c.simulated
  && c.capabilityClass === 'haptics' && c.ports.some(p => p.direction === 'sink' && p.profile === 'capyio.haptics.feedback/1')));
const controllerSettingsNodes = computed(() => (snapshot.value?.nodes ?? []).filter(node => !node.local && node.online
  && snapshot.value?.capabilities.some(capability => !capability.simulated && capability.nodeId === node.id
    && ['gamepad', 'imu', 'haptics'].includes(capability.capabilityClass))));
const selectedGamepadFeedback = computed(() => {
  if (gamepadFeedbackTarget.value) return gamepadFeedbackSinks.value.find(c => c.id === gamepadFeedbackTarget.value);
  const activeSinks = new Set(ownedGamepadFeedback(snapshot.value?.routes ?? []).map(route => route.sinkCapabilityId));
  if (activeSinks.size === 1) return gamepadFeedbackSinks.value.find(c => activeSinks.has(c.id));
  return gamepadFeedbackSinks.value.length === 1 ? gamepadFeedbackSinks.value[0] : undefined;
});
function feedbackRoutes(capabilityId: string) {
  return (snapshot.value?.routes ?? []).filter(r => r.sinkCapabilityId === capabilityId
    && r.profile === 'capyio.haptics.feedback/1' && (['starting', 'active'].includes(r.state) || (r.state === 'draft' && r.authorization === 'pending')));
}
const gamepadFeedbackRoutes = computed(() => selectedGamepadFeedback.value ? feedbackRoutes(selectedGamepadFeedback.value.id) : []);
async function setGamepadFeedback(active: boolean): Promise<void> {
  const capability = selectedGamepadFeedback.value;
  if (!capability || gamepadFeedbackBusy.value) {
    gamepadFeedbackMessage.value = '请选择本次手柄对应设备的振动器；没有资源时请先连接设备。';
    return;
  }
  gamepadFeedbackBusy.value = true;
  try {
    snapshot.value = await api.setRemoteCapabilityUse(capability.id, active);
    gamepadFeedbackMessage.value = active ? '已提交振动请求；首次需在设备上允许，状态以下方实际 Route 为准。' : '振动已停止或申请已取消；按键输入不受影响。';
  } catch (error) { gamepadFeedbackMessage.value = `振动未完成：${normalizeError(error)}。手柄输入可继续使用。`; }
  finally { gamepadFeedbackBusy.value = false; }
}
async function stopBundledFeedback(): Promise<void> {
  snapshot.value = await api.getSnapshot();
  const capabilities = new Set(ownedGamepadFeedback(snapshot.value.routes).map(route => route.sinkCapabilityId));
  for (const capability of capabilities) {
    if (capability) snapshot.value = await api.setRemoteCapabilityUse(capability, false);
  }
}
async function changeHapticsStrength(event: Event): Promise<void> {
  hapticsBusy.value = true;
  try {
    await api.setHapticsStrength(Number((event.target as HTMLInputElement).value));
    hapticsSettings.value = await api.getHapticsSettings();
  } catch (error) { errorMessage.value = normalizeError(error); }
  finally {
    (event.target as HTMLInputElement).value = String(hapticsSettings.value?.strength ?? 70);
    hapticsBusy.value = false;
  }
}
async function changeGamepadMotion(event: Event): Promise<void> {
  gamepadMotionBusy.value = true;
  try {
    await api.setGamepadMotionEnabled((event.target as HTMLInputElement).checked);
    gamepadMotion.value = await api.getGamepadMotionSettings();
  } catch (error) { errorMessage.value = normalizeError(error); }
  finally {
    (event.target as HTMLInputElement).checked = gamepadMotion.value?.enabled ?? false;
    gamepadMotionBusy.value = false;
  }
}
const gamepadFullscreenRequested = ref(false);
async function enterGamepadFullscreen(): Promise<boolean> {
  try {
    quickActions.value = await api.getQuickActions();
    gamepad.value = await api.getGamepadState();
    if (!androidTouchControllerActive.value || gamepad.value?.androidInput.status !== 'connected') {
      androidControllerModeMessage.value = '请先启动触控手柄并连接 Windows 接收器；仅体感模式不需要触控界面。';
      await navigate('controller');
      return false;
    }
    gamepadFullscreenRequested.value = true;
    await navigate('controller');
    return true;
  } catch (error) { errorMessage.value = normalizeError(error); return false; }
}
function exitGamepadFullscreen(): void {
  updateGamepad({ kind: 'reset' });
  gamepadFullscreenRequested.value = false;
}
const touchpad = ref<UiTouchpadState | null>(null);
const touchpadBusy = ref(false);
const touchpadHapticStrength = ref(50);
const touchpadHapticMessage = ref("");
function saveTouchpadHapticStrength(): void {
  const strength = touchpadHapticStrength.value;
  if (!Number.isInteger(strength) || strength < 0 || strength > 100
    || !window.CapyIOAndroid?.setTouchpadHapticStrength?.(strength)) {
    touchpadHapticMessage.value = "未能保存触控板震动强度，请重试。";
    return;
  }
  touchpadHapticMessage.value = strength === 0 ? "触控板震动已关闭。" : "触控板震动强度已保存。";
}
watch(() => touchpad.value?.status, (status, previous) => {
  if (androidNode.value && status === "active" && previous !== "active") void navigate("touchpad");
});
const androidAudio = ref<UiAndroidAudioState | null>(null);
const androidCamera = ref<UiAndroidCameraSettings | null>(null);
const cameraSettingsBusy = ref(false);
const gamepadDsuPort = ref(26760);
const gamepadDsuMode = ref<DsuProjectionMode>("motion_only");
const gamepadDsuBusy = ref(false);
const gamepadAndroidPort = ref(31580);
const preferredPeerHost = ref(initialPreferredPeerHost);
const lastSavedPreferredPeerHost = ref(initialPreferredPeerHost);
const connectionProfileMessage = ref(initialConnectionProfileMessage);
const discoveryInterfaceIpv4 = ref(initialDiscoveryInterfaceIpv4);
const discoveryInterfaces = ref<import("./lib/types").UiDiscoveryInterface[]>([]);
const interfaceBusy = ref(false);
const interfaceMessage = ref("");
const discoveryBusy = ref(false);
const discoveryMessage = ref("");
const discoveryCandidates = ref<import("./lib/types").UiDiscoveryCandidate[]>([]);
const discoveryCandidatesError = ref("");
const peerConnectionBusy = ref<string | null>(null);
const autoConnectPaired = ref(initialAutoConnectPaired);
const autoConnectionMessage = ref("");
const connectionAttention = ref(new Map<string, string>());
const pairingTargetNodeId = ref("");
const pairingTargetName = ref("");
const pairingBusy = ref(false);
const pairingMessage = ref("");
const renewExistingPair = ref(false);
const renewPairAcknowledged = ref(false);
const backgroundWaitingEnabled = ref(false);
const backgroundWaitingBusy = ref(false);
const backgroundWaitingMessage = ref("");
const manualControlNodeId = ref("");
const manualControlHost = ref(initialPreferredPeerHost);
const manualControlPort = ref(0);
const manualControlBusy = ref(false);
const manualControlMessage = ref("");
const pairedListenerHost = ref(initialDiscoveryInterfaceIpv4);
const pairedListenerPort = ref(43193);
const pairedListenerBusy = ref(false);
const pairedListenerActive = ref(false);
const pairedListenerNodeId = ref("");
const pairedListenerMessage = ref("");
const androidDialPending = ref(false);
const pairedPeers = ref<import("./lib/types").UiPairedPeer[]>([]);
const pairedPeersError = ref("");
const credentialCheckBusy = ref(false);
const credentialCheckMessage = ref("");
const trustedEndpointHints = ref(initialTrustedEndpointHints);
const activePairedPeers = computed(() => pairedPeers.value.filter(peer => !peer.revoked));
const automaticConnectionSummary = computed(() => automaticConnectionStatus({
  enabled: autoConnectPaired.value,
  listening: pairedListenerActive.value,
  connected: (snapshot.value?.controlPlane?.authenticatedConnectionCount ?? 0) > 0,
  paired: activePairedPeers.value.length > 0,
  hasEndpoint: trustedEndpointHints.value.some(hint => activePairedPeers.value.some(peer => peer.nodeId === hint.nodeId)),
  hasInterface: Boolean(discoveryInterfaceIpv4.value),
}));
const manuallyDisconnectedPeerIds = new Set<string>();
const automaticAttemptAt = new Map<string, number>();
const gamepadAndroidHost = ref(initialPreferredPeerHost);
const gamepadAndroidToken = ref("");
const gamepadAndroidBusy = ref(false);
const gamepadWindowsBusy = ref(false);
const gamepadWindowsKind = ref<WindowsControllerKind>("dualshock4");
const quickActions = ref<UiQuickAction[]>([]);
const audioEndpoints = ref<UiAudioEndpointCatalog | null>(null);
const busyActionId = ref<string | null>(null);
const busyCapabilityId = ref<string | null>(null);
const remoteUseBusy = ref(false);
const resourceRequests = ref<import("./lib/types").UiResourceRequest[]>([]);
const resourceRequestPanel = ref<HTMLElement | null>(null);
const resourceRequestBusy = ref(false);
const resourceRequestError = ref("");
const rememberResourcePermission = ref(true);
const rememberedPermissionMessage = ref("");
let resourceRequestPolling = false;
async function refreshResourceRequests(): Promise<void> {
  if (resourceRequestPolling || resourceRequestBusy.value) return;
  resourceRequestPolling = true;
  try {
    const previous = new Set(resourceRequests.value.map(request => request.routeId));
    resourceRequests.value = await api.getResourceRequests();
    const newlyArrived = resourceRequests.value.some(request => !previous.has(request.routeId));
    if (gamepadFullscreenRequested.value && newlyArrived) exitGamepadFullscreen();
    if (newlyArrived) {
      await nextTick();
      resourceRequestPanel.value?.focus({ preventScroll: true });
      resourceRequestPanel.value?.scrollIntoView({ block: 'start', behavior: 'instant' });
    }
  }
  catch (error) { resourceRequests.value = []; resourceRequestError.value = normalizeError(error); }
  finally { resourceRequestPolling = false; }
}
async function resolveResourceRequest(routeId: string, allow: boolean): Promise<void> {
  if (resourceRequestBusy.value) return;
  resourceRequestBusy.value = true;
  resourceRequestError.value = "";
  try { await api.resolveResourceRequest(routeId, allow, allow && rememberResourcePermission.value); }
  catch (error) { resourceRequestError.value = normalizeError(error); }
  finally { resourceRequestBusy.value = false; await refreshResourceRequests(); }
}
async function clearRememberedResourcePermissions(): Promise<void> {
  try {
    if (!api.clearRememberedResourcePermissions) throw new Error("当前节点尚不支持此设置。");
    await api.clearRememberedResourcePermissions();
    rememberedPermissionMessage.value = "已清除触控板的自动允许记录。下次使用需要确认；当前正在使用的连接不受影响，可用“停止使用”结束。";
  } catch (error) { rememberedPermissionMessage.value = normalizeError(error); }
}
const catalogRefreshing = ref(false);
const catalogUpdatedAt = ref<Date | null>(null);
let livePoll: ReturnType<typeof setInterval> | null = null;
let catalogPoll: ReturnType<typeof setInterval> | null = null;
let autoReconnectPoll: ReturnType<typeof setInterval> | null = null;
let controllerPoll: ReturnType<typeof setInterval> | null = null;
let lastControllerRefresh = 0;
function refreshControllerLifecycle(force = false): void {
  const usingController = controllerUse.value !== null && !['idle', 'failed'].includes(controllerUse.value.status);
  if (document.hidden && !usingController && !force) return;
  const now = Date.now();
  if (!force && now - lastControllerRefresh < (document.hidden ? 2000 : 1000)) return;
  lastControllerRefresh = now;
  // Runtime lifetime continues while the WebView is occluded. Keep its compact
  // status separate from unrelated panels and refresh immediately on return.
  void refreshControllerUse();
  if (view.value === 'controller' || usingController) void refreshGamepad();
}
function controllerWindowResumed(event: Event): void {
  if (event.type === 'focus' || !document.hidden) refreshControllerLifecycle(true);
}

const localNode = computed(() => snapshot.value?.nodes.find((node) => node.local));
const recentEvents = computed(() => [...(snapshot.value?.events ?? [])].reverse());
const liveImuRunning = computed(() => liveImu.value?.status === "connecting" || liveImu.value?.status === "active");
const liveImuTone = computed(() => liveImu.value?.status === "active" ? "success" : ["failed", "offline"].includes(liveImu.value?.status ?? "") ? "danger" : "warning");
const quickActionRouteIds = computed(() => new Set(quickActions.value.flatMap((action) => action.routeId ? [action.routeId] : [])));
const ordinaryQuickRoutes = computed(() => (snapshot.value?.routes ?? []).filter((route) => !quickActionRouteIds.value.has(route.id)));
const localCapabilities = computed(() => (snapshot.value?.capabilities ?? []).filter((capability) => capability.local));
const peerCapabilities = computed(() => (snapshot.value?.capabilities ?? []).filter((capability) => !capability.local));
const androidNode = computed(() => snapshot.value?.backendMode === "android_node");
const gamepadImmersive = computed(() => androidNode.value && view.value === 'controller'
  && gamepadFullscreenRequested.value && gamepad.value !== null);
watch(gamepadImmersive, active => { void api.setGamepadImmersive(active); });
const touchpadImmersive = computed(() => androidNode.value
  && view.value === "touchpad"
  && touchpad.value?.status === "active");
watch(touchpadImmersive, (active) => {
  window.CapyIOAndroid?.setTouchpadImmersive?.(active);
}, { immediate: true });
const realLocalCapabilities = computed(() => localCapabilities.value.filter((capability) => !capability.simulated));
const primaryLocalCapabilities = computed(() => primaryHardwareResources(realLocalCapabilities.value));
const controllerLocalOptions = computed(() => realLocalCapabilities.value.filter(c =>
  ['imu', 'haptics'].includes(c.capabilityClass) && c.selectableForSharing));
const realPeerGroups = computed(() => peerResourceGroups(snapshot.value?.nodes ?? [], snapshot.value?.capabilities ?? []));
const selectedAndroidEntry = computed(() => androidNode.value ? androidResourceEntry(selectedCatalogCapability.value ?? null) : null);
const controlPlane = computed(() => snapshot.value?.controlPlane ?? null);
const discoveryStatus = computed(() => controlPlane.value?.discoveryState === "listening" && androidNode.value
  ? "正在广播本机节点"
  : ({ not_started: "未启动监听", listening: "正在监听", simulated: "模拟" })[controlPlane.value?.discoveryState ?? "not_started"]);
const pairingStatus = computed(() => ({ not_configured: "未配置凭据存储", ready: "已就绪", failed: "存储不可用", simulated: "模拟" })[controlPlane.value?.pairingState ?? "not_configured"]);
const transportStatus = computed(() => ({ not_connected: "没有认证连接", connected: "已有认证连接", simulated: "模拟" })[controlPlane.value?.transportState ?? "not_connected"]);
const catalogExchangeStatus = computed(() => ({ not_connected: "未交换目录", active: "正在交换目录", simulated: "模拟" })[controlPlane.value?.catalogExchangeState ?? "not_connected"]);
const connectedPeerIds = computed(() => new Set((snapshot.value?.nodes ?? []).filter(node => !node.local && node.online).map(node => node.id)));
const activeRemoteCapabilities = computed(() => {
  const activeIds = new Set<string>();
  for (const route of snapshot.value?.routes ?? []) {
    if (route.active && route.sourceCapabilityId) activeIds.add(route.sourceCapabilityId);
    if (route.active && route.sinkCapabilityId) activeIds.add(route.sinkCapabilityId);
  }
  return (snapshot.value?.capabilities ?? []).filter(capability => !capability.local && activeIds.has(capability.id));
});
const localBindings = computed(() => localResourceBindings(localNode.value?.id ?? null, realLocalCapabilities.value, quickActions.value));
const visibleActions = computed(() => {
  const boundIds = new Set(Object.values(localBindings.value).map(action => action.id));
  return resourceActions(quickActions.value, isHomeView(view.value) ? view.value : "settings")
    .filter(action => view.value !== "local" || !boundIds.has(action.id));
});
const selectedCapability = computed(() => realLocalCapabilities.value.find(capability => localBindings.value[capability.id]?.id === selectedActionId.value));
const panelActions = computed(() => quickActions.value.filter(action => !action.simulated && action.kind === "capability_control"
  && view.value === "touchpad" && action.id === "capyio.quick-action.android-touchpad"));

function openAndroidNativeDiagnostics(): void {
  if (!window.CapyIOAndroid?.openNativeDiagnostics()) {
    errorMessage.value = "Android 原生诊断界面不可用";
  }
}

async function createPairingInvitation(): Promise<void> {
  if (pairingBusy.value) return;
  if (renewExistingPair.value && !renewPairAcknowledged.value) {
    pairingMessage.value = "更新现有配对前，请先确认中断说明。";
    return;
  }
  pairingBusy.value = true;
  pairingMessage.value = "";
  try {
    const path = await api.createPairingInvitation(pairingTargetNodeId.value.trim(), pairingTargetName.value.trim(), renewExistingPair.value);
    pairingMessage.value = `${renewExistingPair.value ? '旧配对已更新；两端控制连接会暂时中断。' : ''}邀请已保存到 ${path}。把该文件发送到目标 Android 设备并在 CapyIO 设置中导入；导入后删除文件。`;
    await refreshPairedPeers();
    await refreshSnapshot();
  } catch (error) { pairingMessage.value = normalizeError(error); }
  finally { pairingBusy.value = false; }
}

async function importPairingInvitation(): Promise<void> {
  pairingMessage.value = "";
  try {
    await api.importPairingInvitation();
    pairingMessage.value = "请选择 Windows 生成的 .cpi 邀请文件。";
  } catch (error) { pairingMessage.value = normalizeError(error); }
}

async function connectManualControlEndpoint(): Promise<void> {
  if (manualControlBusy.value || peerConnectionBusy.value) return;
  manualControlBusy.value = true;
  manualControlMessage.value = "正在验证设备身份并交换资源目录…";
  try {
    await api.connectPairedPeerAt(
      manualControlNodeId.value.trim(),
      manualControlHost.value.trim(),
      manualControlPort.value,
    );
    if (androidNode.value) {
      androidDialPending.value = true;
      manualControlMessage.value = "已提交连接请求，正在后台校验配对证书并同步资源；请等待连接状态更新。";
      return;
    }
    saveAuthenticatedEndpointHint({ nodeId: manualControlNodeId.value.trim(), host: manualControlHost.value.trim(), port: manualControlPort.value });
    manuallyDisconnectedPeerIds.delete(manualControlNodeId.value.trim());
    connectionAttention.value.delete(manualControlNodeId.value.trim());
    snapshot.value = await api.getSnapshot();
    manualControlMessage.value = "已认证连接并同步资源目录。可打开远程资源查看；这不代表触控板数据通道已启动。";
  } catch (error) { manualControlMessage.value = controlConnectionError(error); }
  finally { manualControlBusy.value = false; }
}

let enrollmentNeedsHandoff = true;
let enrollmentHandoffBusy = false;
const handledEnrollments = new Set<string>();
function enrollmentProgress(status: import("./lib/types").UiEnrollmentStatus): void {
  enrollmentNeedsHandoff = !['idle', 'failed', 'cancelled', 'unavailable', 'recovery_pending'].includes(status.phase);
}
async function refreshEnrollmentHandoff(): Promise<void> {
  if (!enrollmentNeedsHandoff || enrollmentHandoffBusy || document.hidden
    || touchpadImmersive.value || gamepadImmersive.value) return;
  try {
    const status = await api.getEnrollmentStatus();
    enrollmentProgress(status);
    if (status.phase === 'complete' || status.phase === 'recovered') await completeFilelessPairing(status);
  } catch { /* Pairing errors remain visible in the settings panel. */ }
}
async function completeFilelessPairing(status: import("./lib/types").UiEnrollmentStatus): Promise<void> {
  if (!['complete', 'recovered'].includes(status.phase)) return;
  const key = `${status.phase}:${status.peerNodeId}:${status.peerControlHost}:${status.peerControlPort}`;
  if (handledEnrollments.has(key)) { enrollmentNeedsHandoff = false; return; }
  // Keep the completion pending while another connection action owns the form.
  if (enrollmentHandoffBusy || manualControlBusy.value || peerConnectionBusy.value || pairedListenerBusy.value) return;
  enrollmentHandoffBusy = true;
  try {
  await refreshPairedPeers();
  handledEnrollments.add(key);
  enrollmentNeedsHandoff = false;
  if (status.phase !== "complete" || !status.peerNodeId || !status.peerControlHost || !status.peerControlPort) return;
  if (connectedPeerIds.value.has(status.peerNodeId)) return;
  manualControlNodeId.value = status.peerNodeId;
  manualControlHost.value = status.peerControlHost;
  manualControlPort.value = status.peerControlPort;
  manualControlMessage.value = "配对已完成，已填入对方实际控制地址。";
  if (autoConnectPaired.value && !pairedListenerActive.value && !manuallyDisconnectedPeerIds.has(status.peerNodeId)) {
    await connectManualControlEndpoint();
  }
  } finally { enrollmentHandoffBusy = false; }
}

async function startPairedListener(): Promise<void> {
  if (pairedListenerBusy.value || manualControlBusy.value || peerConnectionBusy.value || !manualControlNodeId.value) return;
  if (connectedPeerIds.value.has(manualControlNodeId.value)) {
    pairedListenerMessage.value = "该设备已有认证连接。请先断开现有连接，再让它从平板端主动连接。";
    return;
  }
  pairedListenerBusy.value = true;
  pairedListenerMessage.value = "正在为指定已配对设备开启认证监听…";
  try {
    const port = await api.startPairedListener(manualControlNodeId.value, pairedListenerHost.value.trim(), pairedListenerPort.value);
    pairedListenerActive.value = true;
    pairedListenerNodeId.value = manualControlNodeId.value;
    pairedListenerMessage.value = `正在 ${pairedListenerHost.value}:${port} 等待 ${manualControlNodeId.value}；平板可用此地址主动连接。证书与节点身份仍会校验。`;
  } catch (error) { pairedListenerMessage.value = normalizeError(error); }
  finally { pairedListenerBusy.value = false; }
}

async function stopPairedListener(): Promise<void> {
  if (pairedListenerBusy.value) return;
  pairedListenerBusy.value = true;
  try {
    await api.stopPairedListener();
    pairedListenerActive.value = false;
    pairedListenerNodeId.value = "";
    pairedListenerMessage.value = "已停止入站认证监听；现有连接不会被强制断开。";
  } catch (error) { pairedListenerMessage.value = normalizeError(error); }
  finally { pairedListenerBusy.value = false; }
}

async function refreshPairedListenerStatus(): Promise<void> {
  if (snapshot.value?.backendMode !== "tauri_demo") return;
  try {
    const status = await api.getPairedListenerStatus();
    pairedListenerActive.value = status !== null;
    pairedListenerNodeId.value = status?.nodeId ?? "";
    if (status) {
      pairedListenerHost.value = status.bindHost;
      pairedListenerPort.value = status.port;
      if (!pairedListenerMessage.value) pairedListenerMessage.value = `正在 ${status.bindHost}:${status.port} 等待已配对设备 ${status.nodeId}。`;
    }
  } catch (error) { pairedListenerMessage.value = normalizeError(error); }
}

function persistTrustedEndpointHints(): void {
  try { window.localStorage.setItem(TRUSTED_ENDPOINT_STORAGE_KEY, encodeTrustedEndpointHints(trustedEndpointHints.value)); }
  catch { /* The authenticated Session remains usable even if hints cannot be persisted. */ }
}

function saveAuthenticatedEndpointHint(endpoint: TrustedEndpointHint): void {
  try {
    trustedEndpointHints.value = rememberAuthenticatedEndpoint(trustedEndpointHints.value, endpoint);
    persistTrustedEndpointHints();
  } catch { /* A rejected routing hint never turns an authenticated connection into a failure. */ }
}

async function refreshPairedPeers(): Promise<void> {
  try {
    pairedPeers.value = await api.getPairedPeers();
    pairedPeersError.value = "";
    const retained = retainActivelyPairedEndpoints(trustedEndpointHints.value, pairedPeers.value);
    if (retained.length !== trustedEndpointHints.value.length) {
      trustedEndpointHints.value = retained;
      persistTrustedEndpointHints();
    }
    if (!activePairedPeers.value.some(peer => peer.nodeId === manualControlNodeId.value)) {
      manualControlNodeId.value = activePairedPeers.value.length === 1 ? activePairedPeers.value[0]!.nodeId : "";
      useDiscoveredEndpoint();
    }
  } catch (error) {
    pairedPeers.value = [];
    manualControlNodeId.value = "";
    pairedPeersError.value = normalizeError(error);
  }
}

async function checkCurrentPairedCredential(): Promise<void> {
  if (!manualControlNodeId.value || credentialCheckBusy.value) return;
  const checkedNodeId = manualControlNodeId.value;
  credentialCheckBusy.value = true;
  credentialCheckMessage.value = "正在检查当前 Windows 账号是否能读取该配对凭据…";
  try {
    await api.checkPairedCredential(checkedNodeId);
    if (manualControlNodeId.value === checkedNodeId) credentialCheckMessage.value = "当前进程可以读取该设备的配对凭据；连接仍需对端证书匹配。";
  } catch (error) {
    if (manualControlNodeId.value === checkedNodeId) credentialCheckMessage.value = normalizeError(error);
  }
  finally { credentialCheckBusy.value = false; }
}

function preparePairRenewal(): void {
  const peer = activePairedPeers.value.find(item => item.nodeId === manualControlNodeId.value);
  if (!peer) return;
  pairingTargetNodeId.value = peer.nodeId;
  pairingTargetName.value = peer.displayName;
  renewExistingPair.value = true;
  renewPairAcknowledged.value = false;
  pairingMessage.value = "已填入所选设备；确认中断说明后才能生成更新邀请。";
  document.getElementById("pairing-title")?.scrollIntoView({ behavior: "smooth" });
}

async function prepareDiscoveredPairing(nodeId: string): Promise<void> {
  if (pairingBusy.value) return;
  const candidate = discoveryCandidates.value.find(item => item.nodeId === nodeId && item.pairingState === "unpaired");
  if (!candidate) return;
  pairingTargetNodeId.value = candidate.nodeId;
  pairingTargetName.value = "";
  renewExistingPair.value = false;
  renewPairAcknowledged.value = false;
  pairingMessage.value = "已填入发现的节点 ID，尚未建立信任。请先与目标 Android 设置页核对 ID，填写备注名，再生成邀请；此操作不会自动更新已有配对。";
  await navigate("settings");
  document.getElementById("pairing-target-node")?.focus();
}

async function refreshBackgroundWaiting(): Promise<void> {
  if (!androidNode.value) return;
  try { backgroundWaitingEnabled.value = await api.getBackgroundWaitingEnabled(); }
  catch (error) { backgroundWaitingMessage.value = normalizeError(error); }
}

async function toggleBackgroundWaiting(): Promise<void> {
  if (backgroundWaitingBusy.value) return;
  const requested = backgroundWaitingEnabled.value;
  backgroundWaitingBusy.value = true;
  backgroundWaitingMessage.value = "";
  try {
    await api.setBackgroundWaitingEnabled(requested);
    backgroundWaitingMessage.value = requested
      ? "正在保持节点在线；通知和连接状态将在服务就绪后更新。"
      : "已关闭后台等待；正在使用的其他资源仍按各自开关运行。";
  } catch (error) {
    backgroundWaitingEnabled.value = !requested;
    backgroundWaitingMessage.value = normalizeError(error);
  } finally {
    backgroundWaitingBusy.value = false;
    window.setTimeout(() => { void refreshBackgroundWaiting(); }, 500);
  }
}

function useDiscoveredEndpoint(): void {
  credentialCheckMessage.value = "";
  manualControlMessage.value = "";
  const candidate = discoveryCandidates.value.find(item => item.nodeId === manualControlNodeId.value && item.pairingState === "paired");
  const cached = trustedEndpointHints.value.find(item => item.nodeId === manualControlNodeId.value);
  if (candidate || cached) {
    manualControlHost.value = (candidate ?? cached)!.host;
    manualControlPort.value = candidate?.controlPort ?? cached!.port;
  } else {
    manualControlHost.value = "";
    manualControlPort.value = 0;
  }
}

async function inspectAttentionPeer(nodeId: string): Promise<void> {
  if (!activePairedPeers.value.some(peer => peer.nodeId === nodeId)) {
    autoConnectionMessage.value = "该设备已不在有效配对列表中，请先刷新已配对设备。";
    return;
  }
  manualControlNodeId.value = nodeId;
  useDiscoveredEndpoint();
  await navigate("settings");
  document.getElementById("paired-control-host")?.focus();
}

async function startDiscoveryListener(): Promise<void> {
  if (discoveryBusy.value || interfaceBusy.value) return;
  discoveryBusy.value = true;
  discoveryMessage.value = "";
  try {
    snapshot.value = await api.startDiscovery(discoveryInterfaceIpv4.value.trim());
    discoveryMessage.value = "发现监听已启动；候选节点仍需完成配对和认证。";
  } catch (error) { discoveryMessage.value = normalizeError(error); }
  finally { discoveryBusy.value = false; }
}

async function stopDiscoveryListener(): Promise<void> {
  discoveryBusy.value = true;
  discoveryMessage.value = "";
  try {
    snapshot.value = await api.stopDiscovery();
    discoveryMessage.value = "发现监听已停止。";
  } catch (error) { discoveryMessage.value = normalizeError(error); }
  finally { discoveryBusy.value = false; }
}

async function refreshDiscoveryInterfaces(): Promise<void> {
  if (interfaceBusy.value || discoveryBusy.value || controlPlane.value?.discoveryState === "listening") return;
  interfaceBusy.value = true;
  interfaceMessage.value = "正在读取本机网卡…";
  try {
    discoveryInterfaces.value = await api.getDiscoveryInterfaces();
    discoveryInterfaceIpv4.value = selectDiscoveryInterface(discoveryInterfaces.value, discoveryInterfaceIpv4.value);
    if (!pairedListenerHost.value && discoveryInterfaceIpv4.value) pairedListenerHost.value = discoveryInterfaceIpv4.value;
    saveDiscoveryInterfacePreference();
    interfaceMessage.value = discoveryInterfaces.value.length === 0 ? "未找到可用 IPv4 网卡，请连接网络后刷新。"
      : discoveryInterfaceIpv4.value ? "已选本机地址；启动发现不会自动配对或开启硬件。"
        : "检测到多个本机地址，请选择与另一台设备相通的网络。";
  } catch (error) {
    discoveryInterfaces.value = [];
    discoveryInterfaceIpv4.value = "";
    interfaceMessage.value = normalizeError(error);
  } finally { interfaceBusy.value = false; }
}

function saveDiscoveryInterfacePreference(): void {
  try {
    if (discoveryInterfaceIpv4.value) window.localStorage.setItem(DISCOVERY_INTERFACE_STORAGE_KEY, discoveryInterfaceIpv4.value);
    else window.localStorage.removeItem(DISCOVERY_INTERFACE_STORAGE_KEY);
  } catch { /* The selected native inventory item still applies for this session. */ }
}

function chooseDiscoveryInterface(): void {
  saveDiscoveryInterfacePreference();
  if (autoConnectPaired.value && discoveryInterfaceIpv4.value && controlPlane.value?.discoveryState !== "listening") {
    void startDiscoveryListener();
  }
}

watch([view, () => snapshot.value?.backendMode], () => {
  if (view.value === "settings" && snapshot.value?.backendMode === "tauri_demo") {
    void refreshDiscoveryInterfaces();
    void refreshPairedPeers();
  }
});

async function load(): Promise<void> {
  if (loading.value && snapshot.value) return;
  loading.value = true;
  errorMessage.value = "";
  initializationProblems.value = [];
  try {
    snapshot.value = await api.getSnapshot();
    catalogUpdatedAt.value = new Date();
    // Optional capability queries must not prevent the device catalog from opening.
    const requests = [
      { label: '体感状态', read: async () => { liveImu.value = await api.getLiveImu(); } },
      { label: '手柄状态', read: async () => { gamepad.value = await api.getGamepadState(); } },
      { label: '触控板状态', read: async () => { touchpad.value = await api.getTouchpadState(); } },
      { label: '本机音频状态', read: async () => { androidAudio.value = await api.getAndroidAudioState(); } },
      { label: '本机相机设置', read: async () => { androidCamera.value = await api.getAndroidCameraSettings(); } },
      { label: '资源操作入口', read: async () => { quickActions.value = await api.getQuickActions(); } },
      { label: '音频设备列表', read: async () => { audioEndpoints.value = await api.getAudioEndpoints(); } },
    ];
    const results = await Promise.allSettled(requests.map(request => request.read()));
    initializationProblems.value = results.flatMap((result, index) => result.status === 'rejected'
      ? [{ label: requests[index].label, detail: normalizeError(result.reason) }] : []);
  }
  catch (error) { errorMessage.value = normalizeError(error); }
  finally { loading.value = false; }
}

async function refreshCatalog(): Promise<void> {
  if (catalogRefreshing.value) return;
  catalogRefreshing.value = true;
  errorMessage.value = "";
  try {
    await refreshPairedPeers();
    await refreshDiscoveryCandidates();
    snapshot.value = await api.getSnapshot();
    await refreshPairedListenerStatus();
    if (androidNode.value && androidDialPending.value) {
      if ((snapshot.value.controlPlane?.authenticatedConnectionCount ?? 0) > 0) {
        androidDialPending.value = false;
        manualControlMessage.value = "已认证连接并同步资源目录。";
      } else {
        const failure = await api.getControlConnectionFailure();
        if (failure) {
          androidDialPending.value = false;
          manualControlMessage.value = `连接失败：${failure}`;
        }
      }
    }
    catalogUpdatedAt.value = new Date();
    await refreshBackgroundWaiting();
    await maybeAutoConnectPairedCandidate();
  } catch (error) { errorMessage.value = normalizeError(error); }
  finally { catalogRefreshing.value = false; }
}

async function maybeAutoConnectPairedCandidate(): Promise<void> {
  if (!autoConnectPaired.value || pairedListenerActive.value || snapshot.value?.backendMode !== "tauri_demo" || peerConnectionBusy.value || manualControlBusy.value) return;
  const now = Date.now();
  const target = selectAutomaticPeerTarget(discoveryCandidates.value, trustedEndpointHints.value, activePairedPeers.value,
    connectedPeerIds.value, new Set([...manuallyDisconnectedPeerIds, ...connectionAttention.value.keys()]), automaticAttemptAt, now);
  if (!target) return;
  const { nodeId: targetNodeId, candidate, cached } = target;
  automaticAttemptAt.set(targetNodeId, now);
  peerConnectionBusy.value = targetNodeId;
  autoConnectionMessage.value = "正在认证已配对设备并同步资源目录…";
  try {
    if (candidate) {
      try {
        await api.connectDiscoveredPeer(candidate.nodeId);
        saveAuthenticatedEndpointHint({ nodeId: candidate.nodeId, host: candidate.host, port: candidate.controlPort });
      } catch (discoveryError) {
        if (!cached || cached.nodeId !== candidate.nodeId || (cached.host === candidate.host && cached.port === candidate.controlPort)) throw discoveryError;
        await api.connectPairedPeerAt(cached.nodeId, cached.host, cached.port);
      }
    } else if (cached) {
      await api.connectPairedPeerAt(cached.nodeId, cached.host, cached.port);
    }
    snapshot.value = await api.getSnapshot();
    catalogUpdatedAt.value = new Date();
    autoConnectionMessage.value = "已自动连接已配对设备并同步资源目录。";
  } catch (error) {
    if (requiresConnectionAttention(error)) {
      connectionAttention.value.set(targetNodeId, controlConnectionError(error));
      autoConnectionMessage.value = "某个设备的自动重试已暂停，请处理下方连接问题；其他设备仍可自动连接。";
    } else {
      autoConnectionMessage.value = `自动连接暂未成功：${normalizeError(error)}；将在稍后重试。`;
    }
  } finally {
    peerConnectionBusy.value = null;
  }
}

function retryAttentionPeer(nodeId: string): void {
  connectionAttention.value.delete(nodeId);
  automaticAttemptAt.delete(nodeId);
  autoConnectionMessage.value = autoConnectPaired.value
    ? "已恢复该设备的自动重试，仍会校验原有配对凭据。"
    : "暂停已解除；自动连接未开启，可手动连接设备。";
  void maybeAutoConnectPairedCandidate();
}

async function initializeAutomaticControlConnection(): Promise<void> {
  if (!autoConnectPaired.value || snapshot.value?.backendMode !== "tauri_demo") return;
  await refreshDiscoveryInterfaces();
  if (discoveryInterfaceIpv4.value && controlPlane.value?.discoveryState !== "listening") {
    await startDiscoveryListener();
  }
  await refreshCatalog();
}

function updateAutoConnectPreference(): void {
  try { window.localStorage.setItem(AUTO_CONNECT_STORAGE_KEY, String(autoConnectPaired.value)); }
  catch { /* The in-memory preference still applies. */ }
  autoConnectionMessage.value = autoConnectPaired.value
    ? "已启用：应用会在可用网络上发现并连接已配对设备。"
    : "已停用自动连接；发现监听可继续手动使用。";
  if (autoConnectPaired.value) void initializeAutomaticControlConnection();
}

async function refreshDiscoveryCandidates(): Promise<void> {
  try {
    discoveryCandidates.value = await api.getDiscoveryCandidates();
    discoveryCandidatesError.value = "";
  } catch (error) {
    discoveryCandidates.value = [];
    discoveryCandidatesError.value = normalizeError(error);
  }
}

async function connectCandidate(nodeId: string): Promise<void> {
  if (peerConnectionBusy.value || manualControlBusy.value) return;
  peerConnectionBusy.value = nodeId;
  errorMessage.value = "";
  manuallyDisconnectedPeerIds.delete(nodeId);
  try {
    await api.connectDiscoveredPeer(nodeId);
    connectionAttention.value.delete(nodeId);
    await refreshCatalog();
  } catch (error) { errorMessage.value = controlConnectionError(error); }
  finally { peerConnectionBusy.value = null; }
}

async function disconnectCandidate(nodeId: string): Promise<void> {
  if (peerConnectionBusy.value) return;
  peerConnectionBusy.value = nodeId;
  manuallyDisconnectedPeerIds.add(nodeId);
  errorMessage.value = "";
  try {
    await api.disconnectPeer(nodeId);
    await refreshCatalog();
  } catch (error) { errorMessage.value = normalizeError(error); }
  finally { peerConnectionBusy.value = null; }
}

async function disconnectSelectedPairedPeer(): Promise<void> {
  const nodeId = manualControlNodeId.value;
  if (!nodeId || manualControlBusy.value || peerConnectionBusy.value || !connectedPeerIds.value.has(nodeId)) return;
  manualControlBusy.value = true;
  manuallyDisconnectedPeerIds.add(nodeId);
  try {
    await api.disconnectPeer(nodeId);
    await refreshCatalog();
    manualControlMessage.value = "已断开认证连接。现在可以启动入站监听，让该设备主动连接本机。";
  } catch (error) {
    manualControlMessage.value = normalizeError(error);
  } finally {
    manualControlBusy.value = false;
  }
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

let gamepadReading = false;
async function refreshGamepad(): Promise<void> {
  if (gamepadReading) return;
  gamepadReading = true;
  try {
    if (androidNode.value) gamepadMotion.value = await api.getGamepadMotionSettings();
    if (androidNode.value && !hapticsBusy.value) hapticsSettings.value = await api.getHapticsSettings();
    gamepad.value = await api.getGamepadState();
  }
  catch (error) { errorMessage.value = normalizeError(error); }
  finally { gamepadReading = false; }
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

async function selectCameraProfile(event: Event): Promise<void> {
  cameraSettingsBusy.value = true;
  errorMessage.value = "";
  try { androidCamera.value = await api.selectAndroidCameraProfile((event.target as HTMLSelectElement).value as UiAndroidCameraSettings["captureProfile"]); }
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

async function toggleRemoteCapabilityUse(): Promise<void> {
  const capability = selectedCatalogCapability.value;
  if (!capability || capability.local || remoteUseBusy.value) return;
  remoteUseBusy.value = true;
  errorMessage.value = "";
  try {
    snapshot.value = await api.setRemoteCapabilityUse(capability.id, !selectedRemoteCapabilityInUse.value && !selectedRemoteCapabilityPending.value);
  } catch (error) { errorMessage.value = normalizeError(error); }
  finally { remoteUseBusy.value = false; }
}

async function stopRemoteCapabilityUse(capabilityId: string): Promise<void> {
  if (remoteUseBusy.value) return;
  remoteUseBusy.value = true;
  errorMessage.value = "";
  try { snapshot.value = await api.setRemoteCapabilityUse(capabilityId, false); }
  catch (error) { errorMessage.value = normalizeError(error); }
  finally { remoteUseBusy.value = false; }
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
    .catch(async (error: unknown) => {
      // A surface unmount releases controls, but native Stop may already have
      // completed. Do not present that expected final reset as an input failure.
      if (androidNode.value && update.kind === 'reset') {
        try {
          quickActions.value = await api.getQuickActions();
          if (!androidTouchControllerActive.value) return;
        } catch { /* Preserve the actual update failure when state is unknown. */ }
      }
      errorMessage.value = normalizeError(error);
    })
    .finally(() => { gamepadBusy.value = false; });
}

const pendingTouchpadUpdates: TouchpadControlUpdate[] = [];
let touchpadUpdateInFlight = false;

function updateTouchpad(update: TouchpadControlUpdate): void {
  enqueueTouchpadUpdate(pendingTouchpadUpdates, update);
  touchpadBusy.value = true;
  void drainTouchpadUpdates();
}

async function drainTouchpadUpdates(): Promise<void> {
  if (touchpadUpdateInFlight) return;
  const next = pendingTouchpadUpdates.shift();
  if (!next) {
    touchpadBusy.value = false;
    return;
  }
  touchpadUpdateInFlight = true;
  try {
    touchpad.value = await api.updateTouchpadState(next);
  } catch (error) {
    pendingTouchpadUpdates.splice(0);
    errorMessage.value = normalizeError(error);
  } finally {
    touchpadUpdateInFlight = false;
    if (pendingTouchpadUpdates.length > 0) void drainTouchpadUpdates();
    else touchpadBusy.value = false;
  }
}

async function exitImmersiveTouchpad(): Promise<void> {
  if (!touchpadImmersive.value) return;
  updateTouchpad({ contacts: [] });
  try {
    const updated = await api.invokeQuickAction("capyio.quick-action.android-touchpad", "stop");
    quickActions.value = quickActions.value.map(action => action.id === updated.id ? updated : action);
    touchpad.value = await api.getTouchpadState();
  } catch (error) {
    errorMessage.value = normalizeError(error);
  }
  await navigate("local");
}

async function startGamepadDsu(): Promise<void> {
  if (gamepadDsuBusy.value || gamepadWindowsBusy.value) return;
  gamepadDsuBusy.value = true;
  errorMessage.value = "";
  try {
    await stopBundledFeedback();
    if (gamepad.value?.windowsProjection.status === 'active') gamepad.value = await api.stopWindowsGamepadProjection();
    gamepad.value = await api.startGamepadDsu(gamepadDsuPort.value, gamepadDsuMode.value);
  }
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

function savePreferredPeer(): void {
  connectionProfileMessage.value = "";
  try {
    const previous = lastSavedPreferredPeerHost.value;
    const profile = saveConnectionProfile(window.localStorage, preferredPeerHost.value);
    preferredPeerHost.value = profile.preferredPeerIpv4;
    if (!gamepadAndroidHost.value || gamepadAndroidHost.value === previous) gamepadAndroidHost.value = profile.preferredPeerIpv4;
    lastSavedPreferredPeerHost.value = profile.preferredPeerIpv4;
    connectionProfileMessage.value = "首选地址已保存在本机，并用于尚未单独保存地址的功能表单。";
  } catch (error) { connectionProfileMessage.value = normalizeError(error); }
}

function clearPreferredPeer(): void {
  connectionProfileMessage.value = "";
  try {
    clearConnectionProfile(window.localStorage);
    preferredPeerHost.value = "";
    lastSavedPreferredPeerHost.value = "";
    connectionProfileMessage.value = "已清除本机首选地址；各功能原先保存的配置没有改变。";
  } catch (error) { connectionProfileMessage.value = normalizeError(error); }
}

async function refreshWindowsGamepadPreflight(): Promise<void> {
  gamepadWindowsBusy.value = true;
  errorMessage.value = "";
  try { gamepad.value = await api.refreshWindowsGamepadPreflight(gamepadWindowsKind.value); }
  catch (error) { errorMessage.value = normalizeError(error); }
  finally { gamepadWindowsBusy.value = false; }
}

async function startWindowsGamepadProjection(): Promise<void> {
  if (gamepadDsuBusy.value || gamepadWindowsBusy.value) return;
  gamepadWindowsBusy.value = true;
  errorMessage.value = "";
  try {
    await stopBundledFeedback();
    if (gamepad.value?.dsuProjection.status === 'active') gamepad.value = await api.stopGamepadDsu();
    gamepad.value = await api.startControllerHost(gamepadWindowsKind.value, gamepadAndroidPort.value);
    if (gamepad.value.windowsProjection.status === 'active' && gamepad.value.androidInput.status === 'connected' && gamepadFeedbackEnabled.value && !gamepadFeedbackRoutes.value.length) {
      await setGamepadFeedback(true);
    }
  }
  catch (error) { errorMessage.value = normalizeError(error); }
  finally { gamepadWindowsBusy.value = false; }
}

async function stopWindowsGamepadProjection(): Promise<void> {
  gamepadWindowsBusy.value = true;
  errorMessage.value = "";
  try {
    try { await stopBundledFeedback(); }
    finally { gamepad.value = await api.stopControllerHost(); }
  }
  catch (error) { errorMessage.value = normalizeError(error); }
  finally { gamepadWindowsBusy.value = false; }
}

function normalizeError(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

onMounted(() => {
  const savedHapticStrength = window.CapyIOAndroid?.getTouchpadHapticStrength?.();
  if (typeof savedHapticStrength === "number" && Number.isInteger(savedHapticStrength)
    && savedHapticStrength >= 0 && savedHapticStrength <= 100) {
    touchpadHapticStrength.value = savedHapticStrength;
  }
  void load().then(refreshPairedPeers).then(refreshPairedListenerStatus).then(refreshBackgroundWaiting).then(initializeAutomaticControlConnection);
  catalogPoll = setInterval(() => {
    void refreshEnrollmentHandoff();
    void refreshResourceRequests();
    if (!loading.value && !discoveryBusy.value && (view.value === 'settings' || view.value === 'remote' || view.value === 'capability')) void refreshCatalog();
  }, 2000);
  autoReconnectPoll = setInterval(() => {
    if (!loading.value && autoConnectPaired.value && snapshot.value?.backendMode === 'tauri_demo'
      && view.value !== 'settings' && view.value !== 'remote') void refreshCatalog();
  }, 10_000);
  controllerPoll = setInterval(() => refreshControllerLifecycle(), 1000);
  window.addEventListener('focus', controllerWindowResumed);
  document.addEventListener('visibilitychange', controllerWindowResumed);
  livePoll = setInterval(() => {
    if (document.hidden || liveRefreshRunning) return;
    // The full-screen input surface needs only its own lifecycle poll. Reading
    // unrelated hardware state every 500 ms competes with the WebView input
    // thread on Android; resume ordinary panel refresh as soon as it exits.
    if (touchpadImmersive.value) {
      liveRefreshRunning = true;
      void refreshTouchpad().finally(() => { liveRefreshRunning = false; });
      return;
    }
    if (gamepadImmersive.value) {
      liveRefreshRunning = true;
      void Promise.all([refreshGamepad(), refreshControllerUse()]).finally(() => { liveRefreshRunning = false; });
      return;
    }
    liveRefreshRunning = true;
    void refreshVisibleState().finally(() => { liveRefreshRunning = false; });
  }, 500);
  window.addEventListener("capyio-native-state", refreshSnapshot);
});
onUnmounted(() => {
  if (livePoll) clearInterval(livePoll);
  if (catalogPoll) clearInterval(catalogPoll);
  if (autoReconnectPoll) clearInterval(autoReconnectPoll);
  if (controllerPoll) clearInterval(controllerPoll);
  window.removeEventListener('focus', controllerWindowResumed);
  document.removeEventListener('visibilitychange', controllerWindowResumed);
  window.removeEventListener("capyio-native-state", refreshSnapshot);
});

let liveRefreshRunning = false;
let snapshotDirty = false;
let lastSummaryRefresh = 0;
function refreshSnapshot(): void { snapshotDirty = true; }

async function refreshVisibleState(): Promise<void> {
  const jobs: Promise<void>[] = [];
  if (Date.now() - lastSummaryRefresh >= 1000) {
    lastSummaryRefresh = Date.now();
    jobs.push(refreshQuickActions());
    if (snapshotDirty) {
      snapshotDirty = false;
      jobs.push(api.getSnapshot().then(value => { snapshot.value = value; }).catch(error => { errorMessage.value = normalizeError(error); }));
    }
  }
  if (view.value === "controller") jobs.push(refreshGamepad());
  if (view.value === "imu" || view.value === "controller") jobs.push(refreshLiveImu());
  if (view.value === "touchpad") jobs.push(refreshTouchpad());
  if (view.value === "quick") {
    if (selectedActionId.value?.includes("camera")) jobs.push(refreshAndroidCamera());
    if (/microphone|speaker/.test(selectedActionId.value ?? "")) jobs.push(refreshAndroidAudio());
  }
  await Promise.all(jobs);
}
</script>

<template>
  <main class="app-shell" :class="{ 'app-shell--touchpad': touchpadImmersive }">
    <TouchpadSurface
      v-if="touchpadImmersive && touchpad"
      :state="touchpad"
      :disabled="false"
      immersive
      @update="updateTouchpad"
      @exit="exitImmersiveTouchpad"
    />
    <section v-else-if="gamepadImmersive && gamepad" class="gamepad-fullscreen" aria-label="全屏手柄">
      <header class="gamepad-fullscreen__bar">
        <span>{{ gamepad.androidInput.status === 'connected' ? '触控手柄' : '手柄未连接 · 返回设置检查连接' }}</span>
        <span v-if="gamepadMotion">体感：{{ !gamepadMotion.enabled ? '关闭' : gamepadMotion.available ? '实时采集中' : '等待有效采样' }}</span>
        <button class="ghost-button" type="button" @click="exitGamepadFullscreen">返回设置</button>
      </header>
      <p v-if="errorMessage" role="alert">{{ errorMessage }}</p>
      <GamepadSurface :state="gamepad" :disabled="gamepad.androidInput.status !== 'connected'" fullscreen @update="updateGamepad" />
    </section>
    <template v-else>
    <header class="topbar">
      <a class="brand" href="#overview" aria-label="CapyIO 远程资源" @click.prevent="navigate('remote')">
        <span class="brand__mark" aria-hidden="true">IO</span>
        <span><strong>CapyIO</strong><small>跨设备硬件共享</small></span>
      </a>
      <nav class="view-switch" aria-label="主导航">
        <button v-for="item in ([['remote', '远程资源'], ['local', '本机共享'], ['settings', '设置']] as const)" :key="item[0]" type="button" :class="{ active: (isPrimaryView ? view : parentView) === item[0] }" :aria-current="(isPrimaryView ? view : parentView) === item[0] ? 'page' : undefined" @click="navigate(item[0])">{{ item[1] }}</button>
      </nav>
      <div class="topbar__actions">
        <StatusPill v-if="snapshot" :tone="snapshot.backendMode === 'browser_mock' ? 'warning' : 'neutral'" :label="androidNode ? 'Android' : snapshot.backendMode === 'browser_mock' ? '浏览器演示' : 'Windows 测试版'" />
      </div>
    </header>

    <section id="overview" class="page-heading">
      <button v-if="!isPrimaryView" type="button" class="ghost-button" @click="goBack">← 返回{{ backLabel }}</button>
      <h1 ref="pageHeading" tabindex="-1">{{ pageTitle }}</h1>
      <p v-if="view === 'local'">选择本机要共享的硬件，并独立管理各项功能。</p>
      <p v-else-if="view === 'remote'">查看其他设备的共享资源，选择要在本机使用的功能。</p>
      <p v-else-if="view === 'settings'">连接配置、应用信息与运行诊断。</p>
    </section>

    <section v-if="resourceRequests.length || resourceRequestError" ref="resourceRequestPanel" tabindex="-1" class="info-panel" aria-label="待确认资源请求" aria-live="polite" style="scroll-margin-top: 1rem">
      <h2>有设备申请使用本机资源</h2>
      <p>首次使用每项资源由你确认。选择记住后，只自动允许这台设备再次使用当前资源；触控板许可不会同时开放振动器或其他硬件。</p>
      <article v-for="request in resourceRequests" :key="request.routeId" class="pairing-panel">
        <h3>{{ request.peerName }} → {{ request.capabilityName }}</h3>
        <p>已认证节点：<code>{{ request.peerNodeId }}</code></p>
        <p v-if="request.resourceKind === 'haptics'">此请求仅授权对端向本机振动器发送游戏反馈，不授权其他硬件。授权不代表振动数据通道已启动；本机强度设置仍然有效。</p>
        <p v-else-if="request.resourceKind === 'gamepad'">{{ androidNode ? '此请求允许向该设备发送触控手柄输入及已开启的体感。' : '此请求允许对方在本机启动所选手柄输出并发送输入。' }}不包含振动器授权。允许请求不代表数据通道已经就绪；启动结果会单独显示。</p>
        <p v-else-if="request.resourceKind === 'microphone'">允许此麦克风音频连接。录音仍需要 Android 系统麦克风权限；此许可不开放扬声器、摄像头或其他硬件。</p>
        <p v-else-if="request.resourceKind === 'speaker'">允许此扬声器音频连接。仅开放本项播放资源，不同时授权麦克风采集或其他硬件。</p>
        <p v-else-if="request.resourceKind === 'camera'">允许此相机视频连接；Android 系统相机权限仍需确认。不包含麦克风录音或其他硬件授权。接收器启动后还需在应用内检查实际图像。</p>
        <p v-else-if="request.resourceKind === 'display'">{{ androidNode ? '允许在本机屏幕接收对方的扩展桌面。' : '允许创建本机扩展屏幕并把其中的桌面画面发送给对方。' }}仅授权本次显示共享，不同时开放音频、摄像头或远程输入；任一端都可以停止。</p>
        <p v-else>允许后，申请设备可在本次授权有效期内建立加密触控连接。成功启动后将自动打开触控板面板；停止共享或断线会释放触点。</p>
        <label class="checkbox-row"><input v-model="rememberResourcePermission" type="checkbox" :disabled="resourceRequestBusy"> 记住此设备对此项资源的许可</label>
        <button class="primary-button" type="button" :disabled="resourceRequestBusy" @click="resolveResourceRequest(request.routeId, true)">{{ rememberResourcePermission ? '允许并记住' : '仅允许本次' }}</button>
        <button class="ghost-button" type="button" :disabled="resourceRequestBusy" @click="resolveResourceRequest(request.routeId, false)">拒绝</button>
      </article>
      <p v-if="resourceRequestError" role="alert">{{ resourceRequestError }}</p>
    </section>
    <template v-if="view === 'local'">
      <CapabilityCatalog catalog-id="local-resources" eyebrow="本机硬件" title="硬件资源" description="打开共享开关，让已连接设备看到这项资源。共享不会自动开始录音或采集，首次使用仍需你允许。" :capabilities="primaryLocalCapabilities" :busy-id="busyCapabilityId" :action-bindings="localBindings" @toggle="toggleCapability" @open="openAction" @inspect="inspectCapability" />
      <p v-if="!androidNode && !realLocalCapabilities.length" class="info-panel">当前未探测到可发布的 Windows 宿主适配器资源。兼容 fixture 位于设置中的开发诊断，不会作为真实硬件显示。</p>
    </template>
    <template v-if="view === 'remote'">
      <section class="start-guide" :class="{ 'start-guide--connected': realPeerGroups.length }" aria-labelledby="start-guide-title">
        <div class="start-guide__copy">
          <p class="eyebrow">YOUR DEVICES. WORKING TOGETHER.</p>
          <h2 id="start-guide-title">{{ realPeerGroups.length ? '设备已就位，选一个开始。' : '让身边的设备，\n成为你的新搭档。' }}</h2>
          <p>{{ realPeerGroups.length ? '选择下方资源申请使用。每项共享独立控制，用完随时停止。' : '把平板变成麦克风、摄像头或触控板。连接一次，让设备之间的协作更简单。' }}</p>
          <button class="primary-button" type="button" @click="navigate('settings')">{{ activePairedPeers.length ? '管理设备连接' : '连接我的设备' }} <span aria-hidden="true">↗</span></button>
          <span class="start-guide__privacy">首次使用由你确认 · 随时停止共享</span>
        </div>
        <div v-if="!realPeerGroups.length" class="device-scene" aria-hidden="true">
          <div class="device-scene__orbit"></div>
          <div class="device-scene__desktop"><div class="device-scene__screen"><span class="device-scene__logo">IO</span><span>CapyIO</span><i></i></div><div class="device-scene__stand"></div></div>
          <div class="device-scene__tablet"><span class="device-scene__camera"></span><ResourceIcon kind="microphone" /><span>一起，做更多</span><div class="device-scene__wave"><i></i><i></i><i></i><i></i><i></i><i></i><i></i></div></div>
          <span class="device-scene__chip device-scene__chip--camera"><ResourceIcon kind="camera" /></span>
          <span class="device-scene__chip device-scene__chip--touch"><ResourceIcon kind="touchpad" /></span>
        </div>
      </section>
      <ol v-if="!realPeerGroups.length" class="onboarding-steps" aria-label="使用步骤">
        <li><span>01</span><div><strong>连接设备</strong><small>两端打开 CapyIO，用 PIN 配对</small></div></li>
        <li><span>02</span><div><strong>选择资源</strong><small>在对方设备开启你需要的共享</small></div></li>
        <li><span>03</span><div><strong>开始协作</strong><small>申请使用，对方确认后开始</small></div></li>
      </ol>
      <section v-if="connectionAttention.size" class="info-panel" aria-labelledby="remote-connection-attention-title">
        <h2 id="remote-connection-attention-title">有设备需要处理连接问题</h2>
        <p>以下设备因配对或凭据问题暂停自动重试；这不表示资源被占用。其他已连接设备仍可正常使用。</p>
        <div v-for="[nodeId] in connectionAttention" :key="nodeId">
          <span>{{ activePairedPeers.find(peer => peer.nodeId === nodeId)?.displayName ?? nodeId }}</span>
          <button class="ghost-button" type="button" :disabled="peerConnectionBusy !== null || manualControlBusy" @click="inspectAttentionPeer(nodeId)">查看原因与处理</button>
        </div>
      </section>
      <section v-if="activeRemoteCapabilities.length" class="info-panel" aria-labelledby="active-remote-use-title">
        <h2 id="active-remote-use-title">正在使用的远程资源</h2>
        <p>用完后直接停止，不影响其他正在使用的资源。</p>
        <ul class="discovery-candidate-list">
          <li v-for="capability in activeRemoteCapabilities" :key="capability.id">
            <strong>{{ capability.displayName }}</strong>
            <small>{{ capability.nodeName }}</small>
            <button class="ghost-button" type="button" :disabled="remoteUseBusy" @click="stopRemoteCapabilityUse(capability.id)">{{ remoteUseBusy ? '正在停止…' : '停止使用' }}</button>
          </li>
        </ul>
      </section>
      <details v-if="androidNode" class="info-panel" aria-labelledby="paired-android-peers-title">
        <summary id="paired-android-peers-title">已配对设备与连接状态</summary>
        <p>配对记录不是在线状态。当前控制连接：{{ transportStatus }}；从 Android 主动连接 Windows 请到设置页的已配对节点连接入口。</p>
        <p v-if="pairedPeersError" role="alert">{{ pairedPeersError }}</p>
        <p v-else-if="!activePairedPeers.length">尚无已配对节点。请到设置通过一次性 PIN 添加设备。</p>
        <ul v-else class="discovery-candidate-list">
          <li v-for="peer in activePairedPeers" :key="peer.nodeId">
            <strong>{{ peer.displayName }}</strong>
            <small>节点 {{ peer.nodeId }}</small>
            <small>{{ activePairedPeers.length === 1 && controlPlane?.transportState === 'connected' ? '当前控制连接已认证；不代表硬件 Route 已启动' : '已配对，当前未确认连接' }}</small>
          </li>
        </ul>
        <button class="ghost-button" type="button" @click="refreshPairedPeers">刷新节点</button>
      </details>
      <details v-if="snapshot?.backendMode === 'tauri_demo'" class="info-panel" aria-labelledby="discovered-devices-title">
        <summary id="discovered-devices-title">查找设备与连接诊断</summary>
        <p>以下是网络广播中的候选，不是已连接节点。配对状态来自本机目录；即使显示已配对，也必须完成认证连接后才能使用硬件。首次配对可在设置中生成并导入邀请。</p>
        <p v-if="discoveryCandidatesError" role="alert">{{ discoveryCandidatesError }}</p>
        <p v-else-if="!discoveryCandidates.length">{{ controlPlane?.discoveryState === 'listening' ? '暂未发现设备。请确认对端已完成配对、正在发布发现广播，且当前网络允许局域网组播。' : '发现监听未运行或候选已过期。请先到设置选择本机网络并启动发现。' }}</p>
        <ul v-else class="discovery-candidate-list">
          <li v-for="candidate in discoveryCandidates" :key="candidate.nodeId">
            <strong>{{ candidate.pairingState === 'paired' ? '已配对，等待认证' : candidate.pairingState === 'revoked' ? '已吊销设备' : '未配对设备' }}</strong>
            <span>{{ candidate.host }} · 控制端口 {{ candidate.controlPort }}</span>
            <small>节点 {{ candidate.nodeId }}</small>
            <small>最近 {{ Math.floor(candidate.lastSeenAgeMs / 1000) }} 秒内收到广播 · 不代表已认证或可用</small>
            <button v-if="candidate.pairingState === 'unpaired'" class="ghost-button" type="button" :disabled="pairingBusy" @click="prepareDiscoveredPairing(candidate.nodeId)">填入首次配对表单</button>
            <button v-if="candidate.pairingState === 'paired' && !connectedPeerIds.has(candidate.nodeId)" class="primary-button" type="button" :disabled="peerConnectionBusy !== null" @click="connectCandidate(candidate.nodeId)">{{ peerConnectionBusy === candidate.nodeId ? '认证并交换目录中…' : '认证连接' }}</button>
            <button v-else-if="connectedPeerIds.has(candidate.nodeId)" class="ghost-button" type="button" :disabled="peerConnectionBusy !== null" @click="disconnectCandidate(candidate.nodeId)">{{ peerConnectionBusy === candidate.nodeId ? '断开中…' : '断开连接' }}</button>
          </li>
        </ul>
        <button type="button" class="ghost-button" @click="navigate('settings')">发现与连接设置</button>
      </details>
      <section v-if="realPeerGroups.length || androidNode" class="catalog-toolbar" aria-label="远程资源目录状态">
        <div>
          <strong v-if="androidNode">{{ controlPlane?.authenticatedConnectionCount ?? 0 }} 条已认证设备连接</strong>
          <strong v-else>{{ realPeerGroups.length }} 个含资源的远程节点</strong>
          <span v-if="!androidNode">{{ (snapshot?.nodes ?? []).filter(node => !node.local && node.online).length }} 个在线 · {{ (snapshot?.nodes ?? []).filter(node => !node.local && !node.online).length }} 个离线</span>
          <details><summary>连接详情</summary><small>发现：{{ discoveryStatus }} · 配对：{{ pairingStatus }} · {{ transportStatus }}</small></details>
          <small>{{ catalogUpdatedAt ? `目录刷新于 ${catalogUpdatedAt.toLocaleTimeString()}` : '目录尚未刷新' }}</small>
        </div>
        <button class="ghost-button" type="button" :disabled="catalogRefreshing" @click="refreshCatalog">{{ catalogRefreshing ? '刷新中…' : '刷新资源' }}</button>
      </section>
      <section v-if="androidNode && controlPlane?.transportState === 'connected' && !realPeerGroups.length" class="info-panel">
        <h2>设备已连接，选择下方功能开始使用</h2>
        <p>各功能的节点列表来自已认证连接。此 Android 版本尚未将完整远程资源目录接入总览；没有资源卡片不代表连接失败。开始使用仍需相应资源授权，不会因连接成功自动开启硬件。</p>
      </section>
      <CapabilityCatalog v-for="group in realPeerGroups" :key="group.node.id" :catalog-id="`peer-${group.node.id}`" :eyebrow="group.node.online ? '远程设备 · 在线' : '远程设备 · 离线目录'" :title="group.node.displayName" :description="group.node.online ? '选择要使用的资源，首次使用需对方允许。' : '设备已离线。重新连接后可继续使用。'" :capabilities="primaryHardwareResources(group.capabilities)" :busy-id="busyCapabilityId" @toggle="toggleCapability" @inspect="inspectCapability" />
    </template>
    <section v-if="view === 'capability' && selectedCatalogCapability" class="info-panel resource-detail" aria-labelledby="capability-detail-title">
      <div class="resource-detail__heading">
        <div>
          <p class="eyebrow">{{ selectedCatalogCapability.local ? '本机资源' : '远程资源' }}</p>
          <h2 id="capability-detail-title">{{ selectedCatalogCapability.displayName }}</h2>
          <p>{{ selectedCatalogNode?.displayName ?? selectedCatalogCapability.nodeName }} · {{ selectedCatalogNode?.platform ?? '未知平台' }}</p>
        </div>
        <StatusPill :tone="selectedCatalogCapability.availability === 'available' ? 'success' : selectedCatalogCapability.availability === 'offline' ? 'danger' : 'warning'" :label="selectedCatalogCapability.availability" />
      </div>
      <dl class="detail-grid">
        <div><dt>节点状态</dt><dd>{{ selectedCatalogNode?.online ? '在线' : '离线或未确认' }}</dd></div>
        <div><dt>使用状态</dt><dd>{{ selectedCatalogCapability.availability === 'busy' ? '宿主报告忙碌' : selectedCatalogCapability.availability === 'available' ? '目录报告可用' : '当前不可直接使用' }}</dd></div>
        <div><dt>本节点已知使用状态</dt><dd>{{ knownRouteUsage(selectedCatalogRoutes, selectedCatalogLeases.length) }}</dd></div>
        <div><dt>远程授权</dt><dd>{{ selectedCatalogCapability.local ? '由本机启动流程确认' : selectedAndroidEntry ? '通过下方功能面板申请；授权与运行状态见面板' : selectedRemoteCapabilityPending ? '等待对端确认，允许后自动继续' : selectedCatalogRoutes.some(route => route.authorization === 'authorized') ? '已有已授权 Route；准备和媒体状态见下方' : selectedCatalogCapability.capabilityClass === 'gamepad' ? '在手柄与体感中选择节点并申请' : hasIntegratedRemoteDataBinding(selectedCatalogCapability) ? '可通过下方按钮申请' : '兼容入口独立启动；尚未接入统一 Route 授权' }}</dd></div>
      </dl>
      <template v-if="selectedCatalogRoutes.length">
        <h3>本节点已知 Route</h3>
        <ul class="route-usage-list">
          <li v-for="route in selectedCatalogRoutes" :key="route.id"><strong>{{ route.title }}</strong><span>{{ route.state }} · {{ route.authorization }}</span><p v-if="route.error" role="alert">{{ route.error }}</p></li>
        </ul>
      </template>
      <template v-if="selectedCatalogLeases.length">
        <h3>本机端口租约</h3>
        <ul class="route-usage-list">
          <li v-for="lease in selectedCatalogLeases" :key="`${lease.portId}:${lease.routeId}`"><strong>Route {{ lease.routeId.slice(0, 8) }}</strong><span>到期 {{ lease.expiresAtMs }} ms</span></li>
        </ul>
      </template>
      <h3>端口与方向</h3>
      <ul class="port-list" aria-label="资源端口">
        <li v-for="port in selectedCatalogCapability.ports" :key="port.id">
          <span class="port-direction" :class="`port-direction--${port.direction}`">{{ port.direction }}</span>
          <div><strong>{{ port.displayName }}</strong><small>{{ port.profile }} · {{ port.formats.join(', ') || '格式由适配器协商' }}</small></div>
        </li>
      </ul>
      <AudioUsePanel v-if="selectedAndroidEntry === 'microphone' || selectedAndroidEntry === 'speaker'" :key="selectedCatalogCapability.nodeId + selectedAndroidEntry" :api="api" :direction="selectedAndroidEntry" :preferred-peer="selectedCatalogCapability.nodeId" />
      <CameraUsePanel v-else-if="selectedAndroidEntry === 'camera'" :key="selectedCatalogCapability.nodeId" :api="api" :preferred-peer="selectedCatalogCapability.nodeId" />
      <div v-else-if="!selectedCatalogCapability.local" class="resource-detail-links" aria-label="远程资源控制">
        <button v-if="selectedCatalogCapability.capabilityClass === 'gamepad'" class="primary-button" type="button" @click="navigate('controller', null, selectedCatalogCapability.nodeId)">打开手柄与体感 · 使用此节点</button>
        <button v-else-if="selectedCatalogCapability.capabilityClass === 'display'" class="primary-button" type="button" @click="navigate('display', null, selectedCatalogCapability.nodeId)">打开显示共享 · 使用此节点</button>
        <button v-else-if="selectedCompatibilityAction && !hasIntegratedRemoteDataBinding(selectedCatalogCapability)" class="primary-button" type="button" @click="openAction(selectedCompatibilityAction)">打开{{ selectedCompatibilityAction.title }} · 配置与启停</button>
        <button v-else-if="!selectedCompatibilityAction || hasIntegratedRemoteDataBinding(selectedCatalogCapability)" class="primary-button" type="button" :disabled="remoteUseBusy || (!selectedRemoteCapabilityInUse && !selectedRemoteCapabilityPending && (!hasIntegratedRemoteDataBinding(selectedCatalogCapability) || !selectedCatalogNode?.online || selectedCatalogCapability.availability === 'offline'))" @click="toggleRemoteCapabilityUse">{{ remoteUseBusy ? '正在协商…' : selectedRemoteCapabilityInUse ? '停止使用' : selectedRemoteCapabilityPending ? '取消申请' : hasIntegratedRemoteDataBinding(selectedCatalogCapability) ? '申请并使用' : '尚未接入自动使用' }}</button>
      </div>
      <button v-if="selectedCatalogCapability.local && selectedCatalogCapability.capabilityClass === 'display'" class="primary-button" type="button" @click="navigate('display')">打开显示共享</button>
      <details v-if="selectedCompatibilityAction && hasIntegratedRemoteDataBinding(selectedCatalogCapability)"><summary>高级：兼容诊断入口</summary><button class="ghost-button" @click="openAction(selectedCompatibilityAction)">打开独立兼容连接设置</button><p>与上方自动连接分开使用；不需要同时启动。</p></details>
      <p v-if="selectedCompatibilityAction && !hasIntegratedRemoteDataBinding(selectedCatalogCapability)" class="resource-detail__notice">此入口打开本机兼容接收/投影配置，不会自动选择或启动当前节点。请核对入口内的目标地址；宿主忙碌不代表本机已收到数据。相机需两端分别启动。</p>
      <p v-if="!selectedCatalogCapability.local && ['microphone', 'speaker'].includes(selectedCatalogCapability.capabilityClass)" class="resource-detail__notice">一次申请自动准备两端，停止使用会清理对应方向；麦克风和扬声器可分别控制。当前音频数据沿既有可信局域网／Tailscale UDP 通道传输，并非应用层加密音频。Starting 表示正在准备，不能当作已收到声音；请同时检查实际录音或播放。</p>
      <p v-else-if="!selectedCatalogCapability.local && selectedCatalogCapability.capabilityClass === 'camera'" class="resource-detail__notice">一次申请自动准备相机发送和系统接收；在 Windows 应用中选择 CapyIO 虚拟相机检查实际画面，不需要再分别启动。停止或断线清理对应连接；相机许可不包含录音。当前使用既有可信局域网／Tailscale 视频通道。</p>
      <p v-else-if="!selectedCatalogCapability.local" class="resource-detail__notice">资源授权与配对独立；停止或断线会清理当前使用。触控板使用加密数据通道；其他未接入自动连接的资源保留独立兼容入口。</p>
    </section>
    <section v-else-if="view === 'capability'" class="info-panel empty-state">
      <h2>资源已不可用</h2>
      <p>目录刷新后找不到该资源。返回资源列表查看节点当前公开的项目。</p>
    </section>
    <template v-if="view === 'local' || view === 'remote'">
      <AudioUsePanel v-if="androidNode" :api="api" />
      <CameraUsePanel v-if="androidNode" :api="api" />
      <section class="section-heading"><div><h2>{{ view === 'local' ? '更多玩法' : '探索更多可能' }}</h2></div><p>选择一种方式，拓展设备的用途。</p></section>
      <section class="resource-grid">
        <button v-for="action in visibleActions" :key="action.id" type="button" class="resource-entry" @click="openAction(action)"><strong>{{ actionPresentation(action).title }}</strong><StatusPill :tone="action.status === 'active' ? 'success' : ['failed', 'blocked', 'offline'].includes(action.status) ? 'warning' : 'neutral'" :label="action.status" /><span>{{ actionPresentation(action).summary }}</span><small>配置与使用 →</small></button>
        <button v-if="gamepad && !androidNode && view === 'remote'" type="button" class="resource-entry" @click="navigate('controller')"><span class="resource-entry__icon"><ResourceIcon kind="gamepad" /></span><strong>手柄与体感</strong><span>实验功能 · 电脑需已配置对应手柄后端。</span><small>{{ gamepad.simulated ? '含明确标注的模拟输入' : '打开操作面板 →' }}</small></button>
        <button v-if="androidNode ? view === 'local' : view === 'remote'" type="button" class="resource-entry" @click="navigate('display')"><span class="resource-entry__icon"><ResourceIcon kind="display" /></span><strong>显示共享</strong><span>{{ androidNode ? '使用本机屏幕接收电脑画面' : '多一块屏幕，多一点工作空间。' }}</span><small>实验功能 · 需要额外显示组件 →</small></button>
      </section>
    </template>
    <section v-if="view === 'settings'" class="info-panel">
      <h2>设备连接</h2>
        <PeerEnrollmentPanel :api="api" :existing-single-peer="androidNode && activePairedPeers.length > 0" @progress="enrollmentProgress" @paired="completeFilelessPairing" />

      <div v-if="snapshot?.backendMode === 'tauri_demo'">
        <label class="checkbox-row"><input v-model="autoConnectPaired" type="checkbox" @change="updateAutoConnectPreference"> 自动连接已配对设备（包括重启后）</label>
        <p role="status" aria-live="polite">{{ automaticConnectionSummary }}</p>
        <p v-if="autoConnectionMessage" role="status" aria-live="polite">{{ autoConnectionMessage }}</p>
        <article v-for="[nodeId, reason] in connectionAttention" :key="nodeId" class="pairing-panel">
          <h3>{{ activePairedPeers.find(peer => peer.nodeId === nodeId)?.displayName ?? nodeId }}：自动重试已暂停</h3>
          <p>修正地址或配对后可重新尝试。暂停仅限当前窗口，不会断开其他设备或删除配对。</p>
          <details><summary>原因与处理建议</summary><p style="white-space: pre-line">{{ reason }}</p></details>
          <button class="ghost-button" type="button" :disabled="peerConnectionBusy !== null || manualControlBusy" @click="inspectAttentionPeer(nodeId)">检查此设备的地址与配对</button>
          <button class="ghost-button" type="button" :disabled="peerConnectionBusy !== null || manualControlBusy" @click="retryAttentionPeer(nodeId)">问题已处理，重新尝试</button>
        </article>
      </div>
      <details v-if="snapshot?.backendMode === 'tauri_demo'" class="pairing-panel" aria-labelledby="connect-peer-title" :open="!!manualControlMessage || !!credentialCheckMessage || connectionAttention.size > 0">
        <summary id="connect-peer-title">手动连接与配对检查</summary>
        <p>优先自动发现；若网络不支持发现，会重试上次成功认证的地址。首次连接或端口变化时才需要填写对方 CapyIO 的地址和“本机控制端口”，不是 Android 无线调试端口。</p>
        <p v-if="pairedPeersError" role="alert">{{ pairedPeersError }}</p>
        <button class="ghost-button" type="button" @click="refreshPairedPeers">刷新已配对设备</button>
        <button class="ghost-button" type="button" :disabled="!manualControlNodeId || credentialCheckBusy" @click="checkCurrentPairedCredential">检查当前账号配对凭据</button>
        <button class="ghost-button" type="button" :disabled="!manualControlNodeId || manualControlBusy" @click="preparePairRenewal">为所选设备更新配对</button>
        <p role="status" aria-live="polite">{{ credentialCheckMessage }}</p>
        <p v-if="!activePairedPeers.length">尚无可连接的配对设备。请使用「连接一台新设备」：任一端显示 PIN，另一端输入，双方确认后保存信任。文件邀请保留为兼容入口；保存地址不能代替配对。</p>
        <form v-else class="connection-profile-form" @submit.prevent="connectManualControlEndpoint">
          <label>要连接的已配对设备<select v-model="manualControlNodeId" required :disabled="manualControlBusy" @change="useDiscoveredEndpoint"><option disabled value="">请选择设备</option><option v-for="peer in activePairedPeers" :key="peer.nodeId" :value="peer.nodeId">{{ peer.displayName }} · {{ peer.nodeId }}</option></select></label>
          <PairedPeerDiagnostics :peer="activePairedPeers.find(peer => peer.nodeId === manualControlNodeId)" />
          <label>设备 IP 地址<input id="paired-control-host" v-model="manualControlHost" required maxlength="45" autocomplete="off" :disabled="manualControlBusy" placeholder="例如 192.168.1.20"></label>
          <label>CapyIO 本机控制端口<input v-model.number="manualControlPort" required type="number" min="1" max="65535" :disabled="manualControlBusy" aria-describedby="control-port-help"></label>
          <p id="control-port-help">到平板 CapyIO → 设置 → 设备连接状态查看。应用重启后端口可能变化。</p>
          <div><button class="primary-button" type="submit" :disabled="manualControlBusy || peerConnectionBusy !== null || !manualControlNodeId || connectedPeerIds.has(manualControlNodeId)">{{ connectedPeerIds.has(manualControlNodeId) ? '设备已连接' : manualControlBusy ? '认证并同步中…' : '连接设备' }}</button><button v-if="connectedPeerIds.has(manualControlNodeId)" class="ghost-button" type="button" :disabled="manualControlBusy || peerConnectionBusy !== null" @click="disconnectSelectedPairedPeer">断开当前连接</button><button class="ghost-button" type="button" :disabled="manualControlBusy" @click="useDiscoveredEndpoint">使用发现的地址</button><button class="ghost-button" type="button" @click="navigate('remote')">查看远程资源</button></div>
        </form>
        <p role="status" aria-live="polite" style="white-space: pre-line">{{ manualControlMessage }}</p>
        <p>连接仍校验配对证书和设备身份；不会因填写 IP 自动授权控制 Windows。</p>
      </details>
      <details v-if="snapshot?.backendMode === 'tauri_demo'" class="pairing-panel" aria-labelledby="paired-listener-title" :open="pairedListenerActive || !!pairedListenerMessage">
        <summary id="paired-listener-title">允许对方主动连接：手动设置</summary>
        <p>选择本机网段地址和已配对设备后启动监听。平板填写此地址与端口即可发起连接；首次配对请使用上方 PIN 入口；邀请文件仅用于旧版兼容或配对修复。</p>
        <form class="connection-profile-form" @submit.prevent="startPairedListener">
          <label>本机监听 IP 地址<select v-if="discoveryInterfaces.length" v-model="pairedListenerHost" required :disabled="pairedListenerBusy || pairedListenerActive"><option disabled value="">请选择网段</option><option v-for="item in discoveryInterfaces" :key="`listen:${item.interfaceIndex}:${item.ipv4}`" :value="item.ipv4">{{ item.ipv4 }} · 网卡 {{ item.interfaceIndex }}</option></select><input v-else v-model="pairedListenerHost" required maxlength="45" autocomplete="off" :disabled="pairedListenerBusy || pairedListenerActive" placeholder="本机局域网 IP"></label>
          <label>本机控制端口<input v-model.number="pairedListenerPort" required type="number" min="1" max="65535" :disabled="pairedListenerBusy || pairedListenerActive"></label>
          <p>目标已配对节点：{{ activePairedPeers.find(peer => peer.nodeId === (pairedListenerActive ? pairedListenerNodeId : manualControlNodeId))?.displayName ?? '请在上方选择设备' }}</p>
          <div><button class="primary-button" type="submit" :disabled="pairedListenerBusy || pairedListenerActive || manualControlBusy || peerConnectionBusy !== null || !manualControlNodeId || connectedPeerIds.has(manualControlNodeId)">{{ pairedListenerActive ? '正在监听' : '启动入站监听' }}</button><button class="ghost-button" type="button" :disabled="pairedListenerBusy || !pairedListenerActive" @click="stopPairedListener">停止监听</button></div>
        </form>
        <p role="status" aria-live="polite">{{ pairedListenerMessage }}</p>
      </details>
      <details v-if="androidNode" class="pairing-panel" aria-labelledby="android-connect-peer-title" :open="!!manualControlMessage || androidDialPending">
        <summary id="android-connect-peer-title">自动连接失败？手动填写地址</summary>
        <p>请先在电脑上打开 CapyIO，并开启“允许对方主动连接”。填写电脑 CapyIO 显示的地址和控制端口，不是无线调试端口。</p>
        <p v-if="pairedPeersError" role="alert">{{ pairedPeersError }}</p>
        <p v-if="activePairedPeers.length !== 1">当前 Android 仅支持一个有效配对设备。尚未配对时使用上方 PIN 入口；已有配对冲突请查看下方配对修复。</p>
        <form v-else class="connection-profile-form" @submit.prevent="connectManualControlEndpoint">
          <p>目标：{{ activePairedPeers[0]?.displayName }} · {{ activePairedPeers[0]?.nodeId }}</p>
          <PairedPeerDiagnostics :peer="activePairedPeers[0]" />
          <label>Windows IP 地址<input v-model="manualControlHost" required maxlength="45" autocomplete="off" :disabled="manualControlBusy || androidDialPending" placeholder="例如 192.168.1.20"></label>
          <label>Windows CapyIO 控制端口<input v-model.number="manualControlPort" required type="number" min="1" max="65535" :disabled="manualControlBusy || androidDialPending"></label>
          <div><button class="primary-button" type="submit" :disabled="manualControlBusy || androidDialPending || (controlPlane?.authenticatedConnectionCount ?? 0) > 0">{{ androidDialPending ? '正在认证…' : (controlPlane?.authenticatedConnectionCount ?? 0) > 0 ? '已连接' : '连接设备' }}</button></div>
        </form>
        <p role="status" aria-live="polite">{{ manualControlMessage }}</p>
      </details>
      <details>
      <summary>旧功能兼容地址（不是设备连接）</summary>
      <p>首选对端地址可复用于尚未单独保存地址的功能。端口、token 和显示凭据仍按功能隔离；保存地址不建立连接、配对或授权。</p>
      <form class="connection-profile-form" @submit.prevent="savePreferredPeer">
        <label>首选对端私有或覆盖网络 IPv4
          <input v-model="preferredPeerHost" required maxlength="15" inputmode="decimal" autocomplete="off" placeholder="例如 192.168.1.20">
        </label>
        <div><button class="primary-button" type="submit">保存首选地址</button><button class="ghost-button" type="button" @click="clearPreferredPeer">清除</button></div>
        <p role="status" aria-live="polite">{{ connectionProfileMessage }}</p>
      </form>
      <p>{{ androidAudio ? '本机音频连接在下方设置。' : 'Windows 音频网络参数目前由后台宿主配置，尚不能在此修改。' }}这些是旧版专项配置，不是统一节点的正常使用流程。统一触控板使用认证网络连接，不需要 ADB；其余硬件的自动连接仍在整合中。</p>
      </details>
      <section v-if="androidNode" class="pairing-panel" aria-labelledby="background-waiting-title">
        <h3 id="background-waiting-title">后台节点连接</h3>
        <label class="checkbox-row"><input v-model="backgroundWaitingEnabled" type="checkbox" :disabled="backgroundWaitingBusy" @change="toggleBackgroundWaiting"> 后台保持节点在线</label>
        <p>开启后，即使未选择共享硬件，平板也会显示运行通知并等待已配对设备连接。关闭仅撤销后台等待，不会自动启动或停止任何硬件。首次配对请使用上方 PIN 入口。</p>
        <p role="status" aria-live="polite">{{ backgroundWaitingMessage }}</p>
        <h3>已记住的资源权限</h3>
        <p>“允许并记住”按设备和具体资源保存。重新配对后需要重新允许；触控板许可不会自动开放振动器、麦克风或摄像头。</p>
        <button class="ghost-button" type="button" @click="clearRememberedResourcePermissions">清除触控板自动允许记录</button>
        <p role="status" aria-live="polite">{{ rememberedPermissionMessage }}</p>
      </section>
      <h3>设备连接状态</h3>
      <dl class="detail-grid">
        <div><dt>局域网发现</dt><dd>{{ discoveryStatus }} · {{ controlPlane?.discoveredCandidateCount ?? 0 }} 个候选</dd></div>
        <div><dt>配对目录</dt><dd>{{ pairingStatus }} · {{ controlPlane?.pairedPeerCount ?? 0 }} 个节点</dd></div>
        <div><dt>认证控制连接</dt><dd>{{ transportStatus }} · {{ controlPlane?.authenticatedConnectionCount ?? 0 }} 个连接</dd></div>
        <div><dt>本机控制端口</dt><dd>{{ controlPlane?.listenPort ?? (androidNode ? '尚未监听' : '由对端发现') }}</dd></div>
        <div><dt>能力目录交换</dt><dd>{{ catalogExchangeStatus }}</dd></div>
      </dl>
      <details v-if="snapshot?.backendMode === 'tauri_demo'">
        <summary>Windows 配对身份诊断</summary>
        <p>当前进程身份 SID：<code>{{ controlPlane?.processUserSid ?? '读取失败' }}</code></p>
        <p>当前进程配对目录：<code>{{ controlPlane?.pairingStorePath ?? '不可用' }}</code></p>
        <p>这里显示的是诊断信息，不表示已发现账号不匹配。若管理员窗口的 SID 或目录不同，原账号的 DPAPI 凭据不能直接迁移；若两项相同但仍报证书错误，应核对已选设备与双方的配对邀请。不会自动信任证书；切勿手工复制私钥文件。</p>
      </details>
      <details :open="!!pairingTargetNodeId || !!pairingMessage || renewExistingPair" class="pairing-panel" aria-labelledby="pairing-title">
        <summary id="pairing-title">高级：邀请文件与配对修复</summary>
        <template v-if="androidNode">
          <p>本机 Node ID：<code>{{ localNode?.id ?? '服务尚未就绪' }}</code></p>
          <p>在 Windows 设置中输入这个 ID 生成一次性邀请，再回到这里导入。邀请包含设备私钥材料，导入成功后应删除原文件。</p>
          <button class="primary-button" type="button" :disabled="!localNode" @click="importPairingInvitation">导入 .cpi 配对邀请</button>
        </template>
        <form v-else-if="snapshot?.backendMode === 'tauri_demo'" class="connection-profile-form" @submit.prevent="createPairingInvitation">
          <label>目标 Android Node ID
            <input id="pairing-target-node" v-model="pairingTargetNodeId" required maxlength="36" autocomplete="off" placeholder="在 Android 设置页复制 UUID">
          </label>
          <label>新设备备注名（不是上方已配对设备）
            <input v-model="pairingTargetName" required maxlength="128" autocomplete="off" placeholder="例如 Y700 平板">
          </label>
          <label class="checkbox-row"><input v-model="renewExistingPair" type="checkbox" :disabled="pairingBusy"> 更新同一 Node ID 的现有配对</label>
          <p v-if="renewExistingPair">仅在两端确实是同一设备时使用。Windows 当前账号必须能解密旧凭据；签发后旧连接会中断，必须立即在平板导入新邀请才能恢复。不会自动替换平板证书。</p>
          <label v-if="renewExistingPair" class="checkbox-row"><input v-model="renewPairAcknowledged" type="checkbox" :disabled="pairingBusy"> 我已确认旧连接会暂时中断，并会在平板导入新邀请</label>
          <button class="primary-button" type="submit" :disabled="pairingBusy || (renewExistingPair && !renewPairAcknowledged)">{{ pairingBusy ? '正在签发…' : renewExistingPair ? '更新配对并生成邀请' : '生成配对邀请' }}</button>
        </form>
        <p v-if="pairingMessage" role="status" aria-live="polite">{{ pairingMessage }}</p>
      </details>
      <form v-if="snapshot?.backendMode === 'tauri_demo'" class="connection-profile-form" @submit.prevent="startDiscoveryListener">
        <label>发现附近设备使用的本机网络
          <select v-model="discoveryInterfaceIpv4" required :disabled="controlPlane?.discoveryState === 'listening' || discoveryBusy || interfaceBusy" @change="chooseDiscoveryInterface">
            <option disabled value="">请选择本机网络</option>
            <option v-for="item in discoveryInterfaces" :key="`${item.interfaceIndex}:${item.ipv4}`" :value="item.ipv4">{{ item.ipv4 }} · 网卡 {{ item.interfaceIndex }}</option>
          </select>
        </label>
        <div>
          <button class="ghost-button" type="button" :disabled="controlPlane?.discoveryState === 'listening' || discoveryBusy || interfaceBusy" @click="refreshDiscoveryInterfaces">刷新网卡</button>
          <button class="primary-button" type="submit" :disabled="!discoveryInterfaceIpv4 || controlPlane?.discoveryState === 'listening' || discoveryBusy || interfaceBusy">启动发现</button>
          <button class="ghost-button" type="button" :disabled="controlPlane?.discoveryState !== 'listening' || discoveryBusy" @click="stopDiscoveryListener">停止发现</button>
        </div>
        <p role="status" aria-live="polite">{{ interfaceMessage }}</p>
        <p role="status" aria-live="polite">{{ discoveryMessage }}</p>
        <p>这里选择的是本机地址，不是手机的无线调试端口。自动发现仅覆盖支持组播的网络；VPN、访客 Wi-Fi 或网络隔离可能阻止发现。</p>
      </form>
      <p>配对后连接设备，即可查看对方共享的资源。连接不会自动启动硬件，首次使用每项资源仍需授权。</p>
      <button class="ghost-button" type="button" @click="navigate('controller')">手柄连接设置</button>
      <button class="ghost-button" type="button" @click="navigate('display')">显示连接设置</button>
    </section>

    <section v-if="view === 'settings' && androidNode" class="info-panel" aria-labelledby="touchpad-haptic-title">
      <template v-if="hapticsSettings">
        <h2>手柄振动器</h2>
        <label for="gamepad-haptic-strength">强度：{{ hapticsSettings.strength }}%</label>
        <input id="gamepad-haptic-strength" type="range" min="0" max="100" step="5" :value="hapticsSettings.strength" :disabled="hapticsBusy || !hapticsSettings.available" @change="changeHapticsStrength" />
        <p>0% 关闭；本机保存。Windows 启动手柄后，在远程资源申请本机“振动反馈”，首次需在这里允许。设备不支持振幅控制时可能无法体现强度差异。</p>
      </template>
      <h2 id="touchpad-haptic-title">触控板反馈</h2>
      <label for="touchpad-haptic-strength">震动强度：{{ touchpadHapticStrength === 0 ? '关闭' : `${touchpadHapticStrength}%` }}</label>
      <input id="touchpad-haptic-strength" v-model.number="touchpadHapticStrength" type="range" min="0" max="100" step="10" aria-describedby="touchpad-haptic-help" @change="saveTouchpadHapticStrength">
      <p id="touchpad-haptic-help">用于点击和双击拖动反馈，保存在本机。设备不支持振幅控制时，系统可能忽略强度差异。</p>
      <p role="status" aria-live="polite">{{ touchpadHapticMessage }}</p>
      <p>全屏触控板会尽量避开导航手势，但 Android 或设备厂商保留的三指系统手势无法由普通应用完全屏蔽。</p>
    </section>

    <section v-if="view === 'workspace' && snapshot?.warnings.length" class="warning-stack" aria-label="运行提示">
      <p v-for="warning in snapshot.warnings" :key="warning">{{ warning }}</p>
    </section>
    <details v-if="view === 'settings'" class="info-panel">
      <summary>功能支持与使用条件</summary>
      <p>首次使用请先连接设备，再选择资源。应用安装完成不代表系统投影组件已经安装；缺少组件时请按功能页提示处理，无需反复配对。</p>
      <ul>
        <li>音频与触控板：电脑需已配置对应驱动和服务。</li>
        <li>摄像头：电脑需已注册虚拟相机；普通权限使用还需配置相机共享服务。</li>
        <li>手柄与体感（实验）：电脑需相应输出后端。DS4、Xbox 360 和仅体感模式的游戏支持不同，收到数据不等于游戏已经使用。</li>
        <li>振动（实验）：需设备支持和单独授权；不同设备的强度与效果可能不同。</li>
        <li>显示共享（实验）：需额外显示组件，Core 包未包含显示引擎；已有短时验收，完整恢复验证仍待完成。</li>
      </ul>
      <p>实验功能已有部分实机验证，但尚未覆盖完整兼容与恢复场景。当前适用于可信局域网或 Tailscale。</p>
    </details>
    <details v-if="view === 'settings'" class="info-panel">
      <summary>版本与运行检查</summary>
      <button class="ghost-button" type="button" @click="refreshDiagnostics">刷新检查</button>
      <p v-if="diagnosticsError" role="alert">{{ diagnosticsError }}</p>
      <template v-if="diagnostics">
        <p>{{ diagnostics.backend }} · {{ diagnostics.version }}</p>
        <p>源码：{{ diagnostics.revision }} · 构建时间：{{ diagnostics.builtAt }}</p>
        <dl><template v-for="check in diagnostics.checks" :key="check.label"><dt>{{ check.label }}</dt><dd>{{ check.detail }}</dd></template></dl>
      </template>
    </details>
    <details v-if="view === 'settings'" class="info-panel">
      <summary>开发诊断与兼容接入</summary>
      <p>{{ localNode?.displayName }} · {{ snapshot?.backendMode }}。演示目录不代表实际设备或连接。</p>
      <button class="ghost-button" type="button" @click="navigate('workspace')">演示目录与运行事件</button>
      <button v-if="liveImu" class="ghost-button" type="button" @click="navigate('imu')">SensorServer 兼容接入</button>
      <button v-if="androidNode" class="ghost-button" type="button" @click="openAndroidNativeDiagnostics">原生诊断</button>
      <button v-else class="ghost-button" type="button" :disabled="loading" @click="resetDemo">重置演示数据</button>
    </details>
    <p v-if="errorMessage" class="error-banner" role="alert">{{ errorMessage }}</p>
    <section v-if="!snapshot" class="loading-panel" role="status">
      <p>{{ loading ? '正在连接本机节点…' : '暂时无法读取本机节点，请重试。' }}</p>
      <button v-if="!loading" type="button" class="ghost-button" @click="load">重新读取</button>
    </section>
    <section v-if="initializationProblems.length" class="info-panel" aria-label="部分状态未能读取">
      <p role="status">部分功能状态暂未读到，已读取的设备目录仍可使用。</p>
      <button type="button" class="ghost-button" :disabled="loading" @click="load">重新读取功能状态</button>
      <details><summary>查看受影响的功能</summary><ul><li v-for="problem in initializationProblems" :key="problem.label">{{ problem.label }}：{{ problem.detail }}</li></ul></details>
    </section>

    <template v-if="view === 'quick'">
      <AudioUsePanel v-if="selectedLocalAudioDirection" :api="api" :direction="selectedLocalAudioDirection" />
      <details v-if="selectedLocalAudioDirection && selectedAction" :open="['active', 'starting', 'stopping'].includes(selectedAction.status)">
        <summary>高级：本机音频诊断与停止</summary>
        <p>此处只控制旧本机音频 worker，不会连接 Windows，也不会创建跨设备 Route。若旧功能正在运行，请先在此停止，再使用上方跨设备入口。</p>
        <QuickActionCard :action="selectedAction" :busy="busyActionId === selectedAction.id" :audio-endpoints="null" @invoke="invokeQuickAction" @refresh-endpoints="refreshAudioEndpoints" @select-endpoint="selectAudioEndpoint" />
        <button class="ghost-button" type="button" @click="navigate('audio-settings')">旧音频连接诊断设置</button>
      </details>
      <CameraUsePanel v-if="androidNode && selectedActionId?.includes('camera')" :api="api" />
      <details v-if="androidNode && selectedActionId?.includes('camera') && selectedAction"><summary>高级：仅启动本机相机采集</summary><QuickActionCard :action="selectedAction" :busy="busyActionId === selectedAction.id" :audio-endpoints="null" @invoke="invokeQuickAction" @refresh-endpoints="refreshAudioEndpoints" @select-endpoint="selectAudioEndpoint" /></details>
      <section v-else-if="!selectedLocalAudioDirection" class="capability-grid">
        <QuickActionCard v-if="selectedAction" :action="selectedAction" :busy="busyActionId === selectedAction.id" :audio-endpoints="audioEndpoints?.actionId === selectedAction.id ? audioEndpoints : null" @invoke="invokeQuickAction" @refresh-endpoints="refreshAudioEndpoints" @select-endpoint="selectAudioEndpoint" />
      </section>
      <p v-if="!selectedAction" class="info-panel">此功能当前不可用，请返回资源列表刷新后重试。</p>
      <div class="resource-detail-links" aria-label="资源操作入口">
        <button v-if="androidNode && selectedActionId?.includes('gamepad')" class="primary-button" type="button" @click="enterGamepadFullscreen">进入全屏手柄</button>
        <button v-if="selectedActionId?.includes('gamepad') || selectedActionId?.includes('imu')" class="ghost-button" type="button" @click="navigate('controller')">手柄与体感设置</button>
        <button v-if="selectedActionId?.includes('touchpad') && touchpad" class="ghost-button" type="button" @click="navigate('touchpad')">打开触控板面板</button>
        <button v-if="androidAudio && !selectedLocalAudioDirection && (selectedActionId?.includes('microphone') || selectedActionId?.includes('speaker'))" class="ghost-button" type="button" @click="navigate('audio-settings')">音频连接设置</button>
      </div>
    </template>

    <section v-if="view === 'quick' && selectedCapability" class="info-panel">
      <p>所属设备：{{ selectedCapability.nodeName }}</p>
      <p>共享意向：{{ selectedCapability.selectableForSharing ? selectedCapability.selectedForSharing ? '已选择' : '未选择' : '当前不可选择共享' }} · 远程使用者：尚未同步</p>
      <p v-if="selectedCapability.ports.some(port => port.direction === 'source')">共享选择与运行相互独立。停止当前功能不会取消共享选择，取消共享选择也不会自动停止正在运行的功能。</p>
    </section>
    <section v-if="panelActions.length" class="capability-grid" aria-label="面板功能控制">
      <QuickActionCard v-for="action in panelActions" :key="action.id" :action="action" :busy="busyActionId === action.id" :audio-endpoints="null" @invoke="invokeQuickAction" @refresh-endpoints="refreshAudioEndpoints" @select-endpoint="selectAudioEndpoint" />
    </section>

    <section v-if="view === 'imu' && liveImu" class="info-panel imu-lab imu-live" aria-labelledby="imu-live-title">
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
          <h3>实时传感器数据</h3>
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
          <div><dt>数据说明</dt><dd><span>真实手机数据；明文 ws:// 仅限可信局域网实验，不代表生产安全。</span></dd></div>
        </dl>
      </div>
    </section>

    <section v-if="view === 'quick' && androidAudio && (selectedActionId?.includes('microphone') || selectedActionId?.includes('speaker'))" class="info-panel imu-lab" aria-labelledby="android-audio-title">
      <header class="imu-lab__header">
        <div>
          <p class="eyebrow">Android Node · platform-owned audio</p>
          <h2 id="android-audio-title">{{ selectedActionId?.includes('microphone') ? '麦克风状态' : '扬声器状态' }}</h2>
          <p>显示实际协商格式与处理帧数；接收效果仍需在另一台设备确认。</p>
        </div>
      </header>
      <div class="imu-lab__grid">
        <article v-if="selectedActionId?.includes('microphone')">
          <h3>本机声音采集</h3>
          <p><StatusPill :tone="androidAudio.microphone.state === 'active' ? 'success' : androidAudio.microphone.state === 'failed' ? 'danger' : 'warning'" :label="androidAudio.microphone.state" /></p>
          <p class="imu-lab__detail">{{ describeAudio(androidAudio.microphone) }}</p>
          <p class="imu-lab__detail">{{ androidAudio.microphone.framesProcessed }} frames · generation {{ androidAudio.microphone.generation }} · permission {{ androidAudio.microphone.permissionReady ? 'ready' : 'required on Start' }}</p>
          <p v-if="androidAudio.microphone.problemCode" class="error-banner">{{ androidAudio.microphone.problemCode }}</p>
        </article>
        <article v-if="selectedActionId?.includes('speaker')">
          <h3>本机声音播放</h3>
          <p><StatusPill :tone="androidAudio.speaker.state === 'active' ? 'success' : androidAudio.speaker.state === 'failed' ? 'danger' : 'warning'" :label="androidAudio.speaker.state" /></p>
          <p class="imu-lab__detail">{{ describeAudio(androidAudio.speaker) }}</p>
          <p class="imu-lab__detail">{{ androidAudio.speaker.framesProcessed }} frames · generation {{ androidAudio.speaker.generation }} · notification {{ androidAudio.speaker.permissionReady ? 'ready' : 'required on Start' }}</p>
          <p v-if="androidAudio.speaker.problemCode" class="error-banner">{{ androidAudio.speaker.problemCode }}</p>
        </article>
      </div>
    </section>

    <section v-if="view === 'quick' && androidCamera && selectedActionId?.includes('camera')" class="info-panel imu-lab" aria-labelledby="android-camera-title">
      <header class="imu-lab__header">
        <div>
          <p class="eyebrow">Android Node · Camera2 Source</p>
          <h2 id="android-camera-title">本机摄像头设置</h2>
          <p>停止后可修改或刷新；编辑会取消待处理申请和相机共享，之后必须重新点击 Start。不会延长授权。</p>
        </div>
        <StatusPill :tone="androidCamera.state === 'active' ? 'success' : androidCamera.state === 'failed' ? 'danger' : 'warning'" :label="androidCamera.state" />
      </header>
      <form class="imu-live__controls" @submit.prevent="refreshCameraSources">
        <label>采集模式（仅本次进程）
          <select :value="androidCamera.captureProfile" :disabled="cameraSettingsBusy || ['starting', 'active'].includes(androidCamera.state)" @change="selectCameraProfile">
            <option value="legacy">Legacy · 默认模式</option>
            <option value="finite_720p30_v1">实验 · 有限 720p30 · 必须选择具体相机</option>
          </select>
        </label>
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
      <p v-if="androidCamera.captureProfile === 'finite_720p30_v1'" class="gamepad-lab__note" role="status">
        实验有限模式：Android 13+，1280×720 / 30 fps / Balanced 4 Mbit/s。静态 0°/90°/180°/270° 传感器经像素校正；90°/270° 保留完整画面并加黑色侧边，不镜像、不跟随屏幕旋转。仍需硬件能力和实际输出检查，不代表所有手机兼容。必须明确选择相机，失败不会退回 Legacy。
        预检：{{ androidCamera.finitePreflight === 'eligible' ? '符合元数据条件（eligible），不代表已验证编码输出、设备兼容或接收端显示' : '不支持或未满足条件，未发布有限格式' }}。
        最长 60 秒且受帧数、字节和速率上限约束，不是连续相机支持声明。
      </p>
      <p v-if="!androidCamera.permissionReady" class="gamepad-lab__note">相机权限仍由 Android 在显式启动时请求；获得权限后才能刷新具体来源。模式选择不会授予权限。</p>
      <p v-if="androidCamera.transportStatus" class="imu-lab__detail">视频传输：{{ androidCamera.transportStatus }}</p>
      <p v-if="androidCamera.sentAccessUnits !== undefined" class="imu-lab__detail">采集 {{ androidCamera.capturedFrames }} 帧 · 编码 {{ androidCamera.encodedAccessUnits }} · 已发送 {{ androidCamera.sentAccessUnits }} · 发送队列丢弃 {{ androidCamera.exportDroppedAccessUnits }}。已发送不等于 Windows 相机应用已显示画面。</p>
      <p v-if="androidCamera.problemCode" class="error-banner" role="status">{{ androidCamera.problemCode }}</p>
    </section>

    <AndroidAudioConnection v-if="(view === 'settings' || view === 'audio-settings') && androidAudio" :key="view" :api="api" :state="androidAudio" :suggested-host="preferredPeerHost" :initial-direction="view === 'audio-settings' && selectedActionId === 'capyio.quick-action.android-speaker' ? 'speaker' : 'microphone'" :expanded="view === 'audio-settings'" />
    <DisplayReceiverPanel v-if="view === 'display'" :key="workflowPeer ?? 'default'" :api="api" :preferred-peer="workflowPeer" />

    <section v-if="view === 'controller' && gamepad" class="info-panel gamepad-lab gamepad-lab--primary" aria-labelledby="gamepad-lab-title">
      <header class="imu-lab__header">
        <div>
          <p class="eyebrow">CapyIO Controller · touch + motion composition</p>
          <h1 id="gamepad-lab-title">触控体感手柄</h1>
          <p>{{ gamepad.profile }} · epoch {{ gamepad.streamEpoch }} · sequence {{ gamepad.sequence ?? "尚未输入" }}</p>
        </div>
        <div class="gamepad-lab__actions">
          <button v-if="androidNode" class="primary-button" type="button" :disabled="!androidTouchControllerActive || gamepad.androidInput.status !== 'connected'" @click="enterGamepadFullscreen">进入全屏手柄</button>
          <StatusPill :tone="gamepad.simulated ? 'warning' : 'success'" :label="gamepad.simulated ? 'simulated source' : 'android live source'" />
          <button class="ghost-button" type="button" :disabled="gamepadBusy || (androidNode ? gamepad.androidInput.status !== 'connected' : ['listening', 'connected'].includes(gamepad.androidInput.status))" @click="updateGamepad({ kind: 'reset' })">全部归零</button>
        </div>
      </header>
      <ControllerUsePanel :key="`${workflowPeer ?? 'default'}:${workflowControllerMode ?? 'default'}`" :nodes="controllerUseNodes" :state="controllerUse" :busy="controllerUseBusy" :error="controllerUseError" :preferred-peer="workflowPeer" :preferred-mode="workflowControllerMode" @start="startControllerUse" @stop="stopControllerUse" />
      <template v-if="androidNode">
      <details><summary>高级：仅启动本机采集</summary>
      <div class="resource-detail__notice">
        <h3>使用模式</h3>
        <label>发送内容 <select v-model="androidControllerMode" :disabled="androidControllerModeBusy"><option value="touch">触控手柄（可含体感与游戏振动）</option><option value="motion">仅体感（配合实体手柄／DSU）</option></select></label>
        <button class="primary-button" type="button" :disabled="androidControllerModeBusy" @click="applyAndroidControllerMode(false)">启动／切换所选模式</button>
        <button class="ghost-button" type="button" :disabled="androidControllerModeBusy" @click="applyAndroidControllerMode(true)">停止手柄、体感和振动</button>
        <p>切换会先停止不再使用的采集。仅体感模式不发送触控按键，并停止本机游戏振动；实体手柄直接接入电脑，无需 CapyIO 转发。Windows 端仍需选择相应的 DSU 或虚拟手柄输出。</p>
        <p role="status">{{ androidControllerModeMessage }}</p>
        <p v-for="action in androidControllerActions" :key="action.id">{{ action.id.endsWith('android-imu') ? '仅体感采集' : '触控手柄采集' }}：{{ action.status }} <span v-if="action.status === 'failed'">· 可先停止后重试</span></p>
      </div>
      </details>
      <div class="resource-detail__notice">
        <label><input type="checkbox" :checked="gamepadMotion?.enabled ?? false" :disabled="!gamepadMotion || gamepadMotionBusy" @change="changeGamepadMotion" /> 启用手柄体感</label>
        <label v-if="hapticsSettings">平板振动强度：{{ hapticsSettings.strength }}%
          <input aria-label="平板振动强度" type="range" min="0" max="100" step="5" :value="hapticsSettings.strength" :disabled="hapticsBusy || !hapticsSettings.available" @change="changeHapticsStrength" />
        </label>
        <p v-if="hapticsSettings">{{ hapticsSettings.available ? '0% 关闭振动；强度保存在平板。Windows 启动手柄时可一起申请游戏振动，首次在这里确认。两路游戏马达合成为本机振动器强度。' : '设备未报告可用振动器。' }}</p>
        <label v-for="capability in controllerLocalOptions" :key="capability.id"><input type="checkbox" :checked="capability.selectedForSharing" :disabled="busyCapabilityId === capability.id" @change="toggleCapability(capability, ($event.target as HTMLInputElement).checked)" /> 向已连接节点公开{{ capability.capabilityClass === 'imu' ? '仅体感模式' : '游戏振动' }}</label>
        <p>启动触控手柄时自动采集加速度和陀螺仪，无需再启动独立 IMU。关闭后仅发送手柄操作；不会停止另行启动的运动传感器。</p>
        <p v-if="gamepadMotion">体感状态：{{ !gamepadMotion.enabled ? '关闭' : gamepadMotion.available ? '实时采集中' : '未采集或等待有效采样' }} · 独立 IMU：{{ gamepadMotion.independentActive ? '使用中' : '未启动' }}</p>
        <p>本版两项功能仍共用接收目标；DSU 遇到无效体感会停止导出，恢复采集后需重新启动 DSU。</p>
      </div>
      <p class="gamepad-lab__note">使用上方“开始使用”自动准备两端。接收地址和临时凭据通过认证连接同步；当前兼容数据通道用于可信网络或 Tailscale。</p>
      <p class="gamepad-lab__note">当前目标：{{ gamepad.androidInput.endpoint ?? '等待已连接的 Windows 开启监听' }}</p>
      <details>
      <summary>高级：手动配置兼容连接</summary>
      <form class="dsu-lab-bar android-lab-bar" @submit.prevent="configureAndroidController">
        <label>Windows IPv4<input v-model="gamepadAndroidHost" type="text" inputmode="decimal" placeholder="例如 192.168.1.20" :disabled="gamepadAndroidBusy || gamepad.androidInput.status === 'connected'" /></label>
        <label>UDP 端口<input v-model.number="gamepadAndroidPort" type="number" min="1" max="65535" :disabled="gamepadAndroidBusy || gamepad.androidInput.status === 'connected'" /></label>
        <label>桌面 token<input v-model="gamepadAndroidToken" type="text" autocomplete="off" placeholder="8–64 位十六进制" :disabled="gamepadAndroidBusy || gamepad.androidInput.status === 'connected'" /></label>
        <button class="primary-button" type="submit" :disabled="gamepadAndroidBusy || gamepad.androidInput.status === 'connected'">保存发送目标</button>
        <StatusPill :tone="gamepad.androidInput.status === 'connected' ? 'success' : gamepad.androidInput.status === 'failed' ? 'danger' : 'warning'" :label="gamepad.androidInput.status" />
      </form>
      </details>
      </template>
      <template v-else>
      <p class="gamepad-lab__note">桌面模拟器和 Android 真机共用完整 <code>GamepadState</code> 检查器。Android 每个 UDP 数据报携带按键、摇杆、扳机与 IMU 完整快照；序列回退会被拒绝，超过 350ms 未收到数据会强制手柄归零。</p>
      <ControllerSettingsPanel :api="api" :nodes="controllerSettingsNodes" />
      <details><summary>高级：手动输出与诊断（不用于跨端一键启动）</summary>
      <details class="gamepad-lab__note">
      <summary>高级：输入接收与网络诊断</summary>
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
      </details>
      <details class="gamepad-lab__note">
      <summary>仅体感模式：配合实体手柄（DSU）</summary>
      <p>DS4 本身携带体感，不需要开启 DSU。点击切换会先停止当前输出，再启动所选模式；两者不会同时使用。若新模式启动失败，旧模式保持停止，可重新选择启动。</p>
      <p>实体手柄通过有线、蓝牙或 USB/IP 接入电脑，由模拟器读取按键；此处选择“仅手机体感”补充陀螺仪，不创建虚拟手柄，也不转发实体手柄。模拟器需支持单独选择 DSU 运动输入。</p>
      <div class="dsu-lab-bar">
        <div><span>DSU loopback projection</span><strong>{{ gamepad.dsuProjection.endpoint ?? "127.0.0.1 · 未启动" }}</strong></div>
        <label>端口<input v-model.number="gamepadDsuPort" type="number" min="1" max="65535" :disabled="gamepadDsuBusy || gamepad.dsuProjection.status === 'active' || !gamepad.dsuProjection.supported" /></label>
        <label>数据<select v-model="gamepadDsuMode" :disabled="gamepadDsuBusy || gamepad.dsuProjection.status === 'active' || !gamepad.dsuProjection.supported"><option value="motion_only">仅手机体感</option><option value="motion_and_controls">手机体感 + 触控按键</option></select></label>
        <button class="primary-button" type="button" :disabled="gamepadDsuBusy || gamepadWindowsBusy || gamepad.dsuProjection.status === 'active' || !gamepad.dsuProjection.supported" @click="startGamepadDsu">{{ gamepad.windowsProjection.status === 'active' ? '停止虚拟手柄并切换为 DSU' : '启动 DSU' }}</button>
        <button class="ghost-button" type="button" :disabled="gamepadDsuBusy || gamepad.dsuProjection.status !== 'active'" @click="stopGamepadDsu">停止</button>
        <StatusPill :tone="gamepad.dsuProjection.status === 'active' ? 'success' : gamepad.dsuProjection.status === 'failed' ? 'danger' : 'warning'" :label="gamepad.dsuProjection.status" />
      </div>
      <div class="dsu-stats" aria-label="DSU projection counters">
        <span v-if="gamepad.androidInput.lastEvent === 'motion_unavailable_dsu_stopped'">体感样本失效，DSU 已停止；恢复采集后可重新启动。</span>
        <span>Controls <strong>{{ gamepad.dsuProjection.controlsAccepted }} / {{ gamepad.dsuProjection.controlsSubmitted }}</strong></span>
        <span>Mode <strong>{{ gamepad.dsuProjection.mode }}</strong></span>
        <span>Subscribers <strong>{{ gamepad.dsuProjection.activeSubscribers }}</strong></span>
        <span>Packets <strong>{{ gamepad.dsuProjection.padPacketsSent }}</strong></span>
        <span>Queue full <strong>{{ gamepad.dsuProjection.controlsQueueFull }}</strong></span>
        <span>Send errors <strong>{{ gamepad.dsuProjection.packetSendErrors }}</strong></span>
        <span>Last <strong>{{ gamepad.dsuProjection.lastSubmit }}</strong></span>
      </div>
      </details>
      <div class="dsu-lab-bar windows-gamepad-bar" aria-label="Windows virtual controller projection status">
        <div>
          <span>Windows virtual controller projection</span>
          <strong>{{ gamepad.windowsProjection.deviceIdentity }} · {{ gamepad.windowsProjection.busId ?? '尚未创建 bus' }}</strong>
        </div>
        <label>设备<select v-model="gamepadWindowsKind" :disabled="gamepadWindowsBusy"><option value="dualshock4">DualShock 4（原生体感）</option><option value="xbox360">Xbox 360（无原生体感）</option></select></label>
        <div v-if="gamepad.windowsProjection.controllerKind === 'dualshock4'">
          <span>Loopback boundaries</span>
          <strong>VIIPER {{ gamepad.windowsProjection.viiperEndpoint ?? '—' }} · USB/IP {{ gamepad.windowsProjection.usbipEndpoint ?? '—' }}</strong>
        </div>
        <div v-if="gamepad.windowsProjection.controllerKind === 'dualshock4'">
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
        <button v-if="gamepad.windowsProjection.status !== 'active' || gamepad.windowsProjection.controllerKind !== gamepadWindowsKind" class="primary-button" type="button" :disabled="gamepadWindowsBusy || gamepadDsuBusy || gamepadAndroidBusy" @click="startWindowsGamepadProjection">{{ gamepad.dsuProjection.status === 'active' ? '停止 DSU 并切换为' : gamepad.windowsProjection.status === 'active' ? '停止当前设备并切换为' : '启动 Windows 手柄：' }} {{ gamepadWindowsKind === 'dualshock4' ? 'DS4（含体感）' : 'Xbox 360' }}</button>
        <button v-if="gamepad.windowsProjection.status === 'active'" class="danger-button" type="button" :disabled="gamepadWindowsBusy || gamepadAndroidBusy" @click="stopWindowsGamepadProjection">停止手柄和接收</button>
        <StatusPill :tone="gamepad.windowsProjection.status === 'active' ? 'success' : gamepad.windowsProjection.status === 'failed' || gamepad.windowsProjection.status === 'offline' ? 'danger' : 'warning'" :label="gamepad.windowsProjection.status" />
      </div>
      <p v-if="gamepad.windowsProjection.problem" class="error-banner" role="status"><strong v-if="gamepad.windowsProjection.problemCode">{{ gamepad.windowsProjection.problemCode }} · </strong>{{ gamepad.windowsProjection.problem }}</p>
      <div class="gamepad-lab__note" aria-label="接收到的体感数据">
        <h3>游戏振动</h3>
        <label><input v-model="gamepadFeedbackEnabled" type="checkbox" /> 启动 DS4／Xbox 时同时申请游戏振动（首次仍需设备允许）</label>
        <label>振动设备
          <select :value="selectedGamepadFeedback?.id ?? ''" :disabled="gamepadFeedbackBusy || gamepadFeedbackRoutes.length > 0" @change="gamepadFeedbackTarget = ($event.target as HTMLSelectElement).value">
            <option value="">请选择本次手柄对应的设备</option>
            <option v-for="sink in gamepadFeedbackSinks" :key="sink.id" :value="sink.id">{{ sink.nodeName }} · {{ sink.displayName }}</option>
          </select>
        </label>
        <button class="ghost-button" type="button" :disabled="gamepadFeedbackBusy || !selectedGamepadFeedback || (!gamepadFeedbackRoutes.length && gamepad.windowsProjection.status !== 'active')" @click="setGamepadFeedback(!gamepadFeedbackRoutes.length)">{{ gamepadFeedbackRoutes.length ? '停止振动／取消申请' : '启用游戏振动' }}</button>
        <p role="status">{{ gamepadFeedbackMessage }}</p>
        <p>振动状态：{{ gamepadFeedbackRoutes.length ? gamepadFeedbackRoutes.map(route => route.state === 'active' ? '使用中' : route.authorization === 'pending' ? '等待设备允许' : '正在启动').join('、') : '未启用' }}。强度可在上方设备设置或平板端调整；仅 DSU 模式不提供游戏振动。</p>
        <p>游戏反馈：Windows 已接收 {{ gamepad.windowsProjection.feedbackPackets ?? 0 }} 帧；小马达 {{ gamepad.windowsProjection.feedbackSmallMotor ?? 0 }} / 大马达 {{ gamepad.windowsProjection.feedbackLargeMotor ?? 0 }}（0–255）。此计数不等于平板实际执行次数。</p>
        <strong>{{ gamepad.motion.source === 'android_sensors' ? '已收到平板体感' : '尚未收到实时体感' }}</strong>
        <p v-if="gamepad.motion.source === 'android_sensors'">角速度（rad/s）：{{ gamepad.motion.angularVelocity.map(value => value.toFixed(3)).join(' / ') }}；加速度（m/s²）：{{ gamepad.motion.acceleration.map(value => value.toFixed(3)).join(' / ') }}</p>
        <p>这里是网络接收值，不代表游戏已读取。原生 DS4 体感不会自动推动摇杆；普通 Gamepad API 网页也不提供原生陀螺仪字段。游戏需支持 DS4 体感，或显式设置体感到鼠标/摇杆的映射。</p>
      </div>
      <p class="gamepad-lab__note">DS4 包含按键、摇杆与启用后的体感，不需要 DSU；游戏仍需支持 DS4 体感或通过 Steam Input 映射。Xbox 360 使用独立 XInput 设备，不同时创建 DS4，也没有原生体感字段。切换会先停止旧设备；振动通过独立授权连接回传。此入口不会安装或卸载驱动。</p>
      </details>
      </template>
      <div class="gamepad-lab__grid">
        <div><h3>{{ androidNode ? '本机触控手柄' : gamepad.simulated ? '模拟手柄输入' : '平板输入状态' }}</h3><GamepadSurface :state="gamepad" :disabled="androidNode ? gamepad.androidInput.status !== 'connected' : !gamepad.simulated" @update="updateGamepad" /></div>
        <details><summary>按键与体感诊断</summary><GamepadTelemetry :state="gamepad" /></details>
      </div>
    </section>

    <section v-if="view === 'touchpad' && touchpad && !touchpadImmersive" class="info-panel gamepad-lab gamepad-lab--primary" aria-labelledby="touchpad-title">
      <header class="imu-lab__header">
        <div>
          <p class="eyebrow">CapyIO · 本机触控板</p>
          <h1 id="touchpad-title">多点触控板</h1>
          <p>{{ touchpad.profile }} · epoch {{ touchpad.streamEpoch }} · sequence {{ touchpad.sequence }}</p>
        </div>
        <div class="gamepad-lab__actions">
          <StatusPill :tone="touchpad.status === 'active' ? 'success' : touchpad.status === 'failed' ? 'danger' : 'warning'" :label="touchpad.status" />
          <button class="ghost-button" type="button" :disabled="touchpadBusy || touchpad.status !== 'active'" @click="updateTouchpad({ contacts: [] })">释放全部触点</button>
        </div>
      </header>
      <p class="gamepad-lab__note">先共享本机触控板，再由已连接的 Windows 节点申请使用并在本机确认。加密连接成功后会自动打开此面板。离开面板会释放全部触点，但不会停止后台共享；Windows 是否收到输入仍需实机确认。</p>
      <p class="gamepad-lab__note">若三、四指同时落下时触点立即消失，请检查平板系统的多指截图、分屏或切换手势。系统取消的触点必须释放，应用不能把它继续发送为按住状态。</p>
      <div class="gamepad-lab__grid">
        <div><h3>触控区域</h3><TouchpadSurface :state="touchpad" :disabled="touchpad.status !== 'active'" @update="updateTouchpad" /></div>
        <details>
          <summary>触点诊断</summary>
          <dl class="imu-lab__sinks">
            <div><dt>运行状态</dt><dd><StatusPill :tone="touchpad.status === 'active' ? 'success' : 'warning'" :label="touchpad.status" /><span>generation {{ touchpad.streamEpoch }}</span></dd></div>
            <div><dt>数据序号</dt><dd><span>{{ touchpad.sequence }}</span></dd></div>
            <div><dt>触点数量</dt><dd><span>{{ touchpad.contacts.length }} / 5 个</span></dd></div>
            <div v-for="contact in touchpad.contacts" :key="contact.contactId"><dt>触点 {{ contact.contactId + 1 }}</dt><dd><span>x {{ contact.x }} · y {{ contact.y }} · 压力 {{ contact.pressure }}</span></dd></div>
          </dl>
        </details>
      </div>
    </section>

    <section v-if="view === 'workspace' && snapshot?.imuFixture" class="info-panel imu-lab" aria-labelledby="imu-lab-title">
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
          <h3>传感器数据</h3>
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
          <div><dt>数据说明</dt><dd><span>编译内确定性 fixture；不是手机实时传感器数据。</span></dd></div>
        </dl>
      </div>
    </section>

    <template v-if="view === 'workspace'">
      <p class="warning-stack">开发诊断：以下目录可能包含演示数据，不能作为真实设备发现或占用情况的证据。</p>
      <details>
        <summary>兼容接入：Audio Share / MicYou（非默认依赖）</summary>
        <QuickActionCard v-for="action in quickActions.filter(item => ['capyio.quick-action.remote-speaker', 'capyio.quick-action.remote-microphone'].includes(item.id))" :key="action.id" :action="action" :busy="busyActionId === action.id" :audio-endpoints="audioEndpoints?.actionId === action.id ? audioEndpoints : null" @invoke="invokeQuickAction" @refresh-endpoints="refreshAudioEndpoints" @select-endpoint="selectAudioEndpoint" />
      </details>
      <section class="capability-grid">
        <QuickActionCard v-for="action in quickActions.filter(item => item.simulated)" :key="action.id" :action="action" :busy="busyActionId === action.id" :audio-endpoints="null" @invoke="invokeQuickAction" @refresh-endpoints="refreshAudioEndpoints" @select-endpoint="selectAudioEndpoint" />
        <RouteCard v-for="route in ordinaryQuickRoutes" :key="route.id" :route="route" :busy="busyRouteId === route.id" @toggle="toggleRoute" />
      </section>
      <section class="section-heading">
        <div><p class="eyebrow">设备与资源</p><h2>共享本机硬件，使用远端能力</h2></div>
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
          <p class="eyebrow">节点</p><h2>对等节点目录</h2>
          <ul class="workspace-list">
            <li v-for="node in snapshot?.nodes ?? []" :key="node.id">
              <div><strong>{{ node.displayName }}</strong><span>{{ node.platform }} · {{ node.local ? "local" : "peer" }}</span></div>
              <StatusPill :tone="node.online ? 'success' : 'warning'" :label="node.online ? 'online' : 'offline'" />
            </li>
          </ul>
        </article>
        <article class="info-panel">
          <p class="eyebrow">适配器</p><h2>显式部署边界</h2>
          <ul class="workspace-list">
            <li v-for="adapter in snapshot?.adapters ?? []" :key="adapter.id">
              <div><strong>{{ adapter.displayName }}</strong><span>{{ adapter.nodeName }} · {{ adapter.deploymentMode }} · {{ adapter.capabilityCount }} capabilities</span></div>
              <StatusPill :tone="adapter.health === 'healthy' ? 'success' : 'warning'" :label="adapter.state" />
            </li>
          </ul>
        </article>
      </section>

      <article class="info-panel route-table-panel">
        <p class="eyebrow">资源连接</p><h2>强类型连接</h2>
        <div class="route-table-wrap">
          <table class="route-table">
            <thead><tr><th>Source Port</th><th>Profile</th><th>Sink Port</th><th>Backend</th><th>State</th></tr></thead>
            <tbody><tr v-for="route in snapshot?.routes ?? []" :key="route.id"><td>{{ route.source.nodeName }}<small>{{ route.source.portName }}</small></td><td>{{ route.profile }}</td><td>{{ route.sink.nodeName }}<small>{{ route.sink.portName }}</small></td><td>{{ route.backend }}</td><td>{{ route.state }}</td></tr></tbody>
          </table>
        </div>
      </article>
    </template>

    <section v-if="view === 'workspace'" class="lower-grid">
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
        <header><div><p class="eyebrow">运行事件</p><h2>可追踪状态变化</h2></div><span>{{ snapshot?.events.length ?? 0 }} retained</span></header>
        <ol class="event-list"><li v-for="event in recentEvents" :key="event.sequence"><span>#{{ event.sequence.toString().padStart(3, "0") }}</span><p>{{ event.summary }}</p></li></ol>
      </article>
    </section>

    <footer v-if="view === 'settings'" class="app-footer"><span>CapyIO · 本地 Alpha</span><span>当前实验传输仅限可信网络</span></footer>
    </template>
  </main>
</template>
