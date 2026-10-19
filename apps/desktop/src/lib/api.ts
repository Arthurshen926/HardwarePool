import { invoke } from "@tauri-apps/api/core";

import { BrowserMockCapyIOApi } from "./mock";
import { parseDisplaySenderState } from "./display-control";
import { parseAudioPeerSettings } from "./audio-peer-settings";
import { parseDiscoveryInterfaces } from "./discovery-interfaces";
import { parseDiscoveryCandidates } from "./discovery-candidates";
import { parsePairedPeers } from "./peer-connection";
import { parseResourceRequests } from "./resource-consent";
import type { CapyIOApi, GamepadControlUpdate, QuickActionOperation, TouchpadControlUpdate, UiAndroidAudioState, UiAndroidCameraSettings, UiAudioEndpointCatalog, UiGamepadState, UiLiveImu, UiQuickAction, UiSnapshot, UiTouchpadState, WindowsControllerKind } from "./types";

declare global {
  interface Window {
    CapyIOAndroid?: {
      openNativeDiagnostics(): boolean;
      getDiagnosticsJson?(): string;
      getSnapshotJson(): string;
      getResourceRequestsJson?(): string;
      getPairedPeersJson?(): string;
      getRemoteCatalogJson?(): string;
      getEnrollmentStatusJson?(): string;
      getEnrollmentAddressesJson?(): string;
      enrollmentActionJson?(request: string): string;
      connectPairedPeerAt?(host: string, port: number): boolean;
      getControlConnectionFailure?(): string;
      resolveResourceRequest?(routeId: string, allow: boolean): boolean;
      resolveResourceRequestRemembered?(routeId: string, allow: boolean, remember: boolean): boolean;
      clearRememberedResourcePermissions?(): boolean;
      getQuickActionsJson(): string;
      getGamepadStateJson(): string;
      getControllerUseJson?(): string;
      getControllerPeersJson?(): string;
      getAudioPeersJson?(): string;
      getAudioUsesJson?(): string;
      getCameraPeersJson?(): string;
      getCameraUseJson?(): string;
      startCameraUse?(peerNodeId: string): boolean;
      stopCameraUse?(): boolean;
      startAudioUse?(peerNodeId: string, direction: string): boolean;
      stopAudioUse?(direction: string): boolean;
      startControllerUse?(peerNodeId: string, mode: string): boolean;
      stopControllerUse?(): boolean;
      getTouchpadStateJson(): string;
      getAudioStateJson(): string;
      getAudioPeerSettingsJson?(direction: string): string;
      getDisplayStateJson?(): string;
      getDisplayPeersJson?(): string;
      getDisplayUseJson?(): string;
      startDisplayUse?(peerNodeId: string, width: number, height: number, refreshMillihertz: number, bitrateKbps: number, automatic: boolean): boolean;
      stopDisplayUse?(): boolean;
      importDisplayProvisioning?(): boolean;
      importPairingInvitation?(): boolean;
      openDisplayReceiver?(): boolean;
      configureDisplayViewing?(fullscreen: boolean, keepAwake: boolean, showStatistics: boolean): boolean;
      configureAudioPeer?(direction: string, host: string, localPort: number, peerPort: number, trustedLabConfirmed: boolean): boolean;
      getCameraSettingsJson(): string;
      setLocalCapabilityShared(capabilityId: string, selected: boolean): boolean;
      getBackgroundWaitingEnabled?(): boolean;
      setBackgroundWaitingEnabled?(enabled: boolean): boolean;
      invokeQuickAction(actionId: string, operation: QuickActionOperation): boolean;
      updateGamepad(kind: string, target: string, x: number, y: number, pressed: boolean): boolean;
      configureControllerRoute(host: string, port: number, token: string): boolean;
      updateTouchpad(encodedContacts: string): boolean;
      setTouchpadImmersive?(active: boolean): boolean;
      getGamepadLayout?(): string;
      getGamepadMotionSettings?(): string;
      setGamepadMotionEnabled?(enabled: boolean): boolean;
      setControllerMode?(mode: 'touch' | 'motion' | 'stopped'): boolean;
      getHapticsSettings?(): string;
      setHapticsStrength?(percent: number): boolean;
      saveGamepadLayout?(layout: string): boolean;
      performTouchpadHaptic?(effect: "tap" | "drag"): boolean;
      getTouchpadHapticStrength?(): number;
      setTouchpadHapticStrength?(strength: number): boolean;
      refreshCameraSources(): boolean;
      selectCameraSource(sourceKey: string): boolean;
      selectCameraQuality(quality: string): boolean;
    };
  }
}

class AndroidNodeCapyIOApi implements CapyIOApi {
  async getCameraPeers(): ReturnType<CapyIOApi['getCameraPeers']> { return JSON.parse(window.CapyIOAndroid?.getCameraPeersJson?.() ?? '[]'); }
  async getCameraUse(): ReturnType<CapyIOApi['getCameraUse']> { return JSON.parse(window.CapyIOAndroid?.getCameraUseJson?.() ?? 'null'); }
  async startCameraUse(peerNodeId: string): Promise<void> {
    if (window.CapyIOAndroid?.startCameraUse?.(peerNodeId) !== true) throw new Error('相机申请未被接受，请检查节点连接和相机资源。');
  }
  async stopCameraUse(): Promise<void> {
    if (window.CapyIOAndroid?.stopCameraUse?.() !== true) throw new Error('未能提交相机停止请求。');
  }
  async getAudioPeers(): ReturnType<CapyIOApi['getAudioPeers']> { return JSON.parse(window.CapyIOAndroid?.getAudioPeersJson?.() ?? '[]'); }
  async getAudioUses(): ReturnType<CapyIOApi['getAudioUses']> { return JSON.parse(window.CapyIOAndroid?.getAudioUsesJson?.() ?? '[]'); }
  async startAudioUse(peerNodeId: string, direction: import('./types').AudioUseDirection): Promise<void> {
    if (window.CapyIOAndroid?.startAudioUse?.(peerNodeId, direction) !== true) throw new Error('音频申请未被接受，请检查节点连接、资源发布和应用版本。');
  }
  async stopAudioUse(direction: import('./types').AudioUseDirection): Promise<void> {
    if (window.CapyIOAndroid?.stopAudioUse?.(direction) !== true) throw new Error('未能提交音频停止请求。');
  }
  async getControllerPeers(): ReturnType<CapyIOApi['getControllerPeers']> { return JSON.parse(window.CapyIOAndroid?.getControllerPeersJson?.() ?? '[]'); }
  async getControllerUse(): ReturnType<CapyIOApi['getControllerUse']> { return JSON.parse(window.CapyIOAndroid?.getControllerUseJson?.() ?? 'null'); }
  async startControllerUse(peerNodeId: string, mode: import('./types').ControllerUseMode): Promise<void> {
    if (window.CapyIOAndroid?.startControllerUse?.(peerNodeId, mode) !== true) throw new Error('手柄启动请求未被接受，请检查节点连接和应用版本。');
  }
  async stopControllerUse(): Promise<void> {
    if (window.CapyIOAndroid?.stopControllerUse?.() !== true) throw new Error('手柄停止请求未被接受。');
  }
  async getRemoteControllerSettings(_nodeId: string) { return null; }
  async setRemoteControllerSettings(_nodeId: string, _expectedRevision: number, _motionEnabled: boolean, _hapticsStrength: number): Promise<void> { throw new Error('当前节点拥有本地手柄设置，请使用本地设置'); }
  async getHapticsSettings() {
    const value = JSON.parse(window.CapyIOAndroid?.getHapticsSettings?.() ?? 'null');
    if (value === null) return null;
    if (!Number.isInteger(value.strength) || value.strength < 0 || value.strength > 100 || typeof value.available !== 'boolean') throw new Error('振动设置无效');
    return { strength: value.strength as number, available: value.available as boolean };
  }
  async setHapticsStrength(percent: number): Promise<void> {
    if (window.CapyIOAndroid?.setHapticsStrength?.(percent) !== true) throw new Error('无法保存振动强度');
  }
  async getGamepadMotionSettings() {
    const value = JSON.parse(window.CapyIOAndroid?.getGamepadMotionSettings?.() ?? 'null');
    if (value === null) return null;
    if (typeof value.enabled !== 'boolean' || typeof value.available !== 'boolean' || typeof value.independentActive !== 'boolean') throw new Error('手柄体感状态无效');
    return { enabled: value.enabled as boolean, available: value.available as boolean, independentActive: value.independentActive as boolean };
  }
  async setGamepadMotionEnabled(enabled: boolean): Promise<void> {
    if (window.CapyIOAndroid?.setGamepadMotionEnabled?.(enabled) !== true) throw new Error('无法启用体感，请检查加速度计和陀螺仪是否可用。');
  }
  async setAndroidControllerMode(mode: 'touch' | 'motion' | 'stopped'): Promise<void> {
    if (window.CapyIOAndroid?.setControllerMode?.(mode) !== true) throw new Error('手柄模式切换未被 Android 接受，请检查应用版本和运行状态。');
  }
  async getGamepadLayout(): Promise<string | null> { return window.CapyIOAndroid?.getGamepadLayout?.() ?? null; }
  async saveGamepadLayout(layout: string): Promise<void> {
    if (window.CapyIOAndroid?.saveGamepadLayout?.(layout) !== true) throw new Error('无法保存手柄布局');
  }
  async setGamepadImmersive(active: boolean): Promise<void> {
    // Reuse the host's input-surface system-bars/keep-awake presentation only.
    window.CapyIOAndroid?.setTouchpadImmersive?.(active);
  }
  async getBackgroundWaitingEnabled(): Promise<boolean> {
    return window.CapyIOAndroid?.getBackgroundWaitingEnabled?.() === true;
  }
  async setBackgroundWaitingEnabled(enabled: boolean): Promise<void> {
    if (window.CapyIOAndroid?.setBackgroundWaitingEnabled?.(enabled) !== true) {
      throw new Error("节点等待服务未就绪。请先完成配对并保持应用在前台，然后重试。");
    }
  }
  async getResourceRequests() {
    const encoded = window.CapyIOAndroid?.getResourceRequestsJson?.() ?? "[]";
    if (encoded.length > 32768) throw new Error("资源授权列表过大");
    return parseResourceRequests(JSON.parse(encoded));
  }
  async resolveResourceRequest(routeId: string, allow: boolean, remember = false) {
    const bridge = window.CapyIOAndroid;
    if (remember && !bridge?.resolveResourceRequestRemembered) throw new Error("请更新 Android App 后使用记住授权。");
    const accepted = bridge?.resolveResourceRequestRemembered
      ? bridge.resolveResourceRequestRemembered(routeId, allow, remember)
      : bridge?.resolveResourceRequest?.(routeId, allow);
    if (!accepted) throw new Error("请求已过期、已处理或连接已断开，请刷新。");
  }
  async clearRememberedResourcePermissions() {
    if (!window.CapyIOAndroid?.clearRememberedResourcePermissions?.()) throw new Error("节点服务未就绪或 App 需要更新。");
  }
  async getPairedPeers() {
    const encoded = window.CapyIOAndroid?.getPairedPeersJson?.() ?? "[]";
    if (encoded.length > 16384) throw new Error("已配对设备列表过大");
    return parsePairedPeers(JSON.parse(encoded));
  }
  checkPairedCredential(): Promise<void> { return this.readOnly(); }
  async getDiscoveryCandidates() { return []; }
  async getDiscoveryInterfaces() { return []; }
  connectDiscoveredPeer(): Promise<void> { return this.readOnly(); }
  async connectPairedPeerAt(nodeId: string, host: string, port: number): Promise<void> {
    const peers = await this.getPairedPeers();
    if (peers.filter(peer => !peer.revoked).length !== 1
      || !peers.some(peer => peer.nodeId === nodeId && !peer.revoked)) {
      throw new Error("Android 当前只支持向唯一已配对设备发起连接。");
    }
    if (!window.CapyIOAndroid?.connectPairedPeerAt?.(host, port)) {
      throw new Error("无法排队连接：检查节点服务、配对凭据、地址或现有连接。");
    }
  }
  async getControlConnectionFailure(): Promise<string | null> {
    return window.CapyIOAndroid?.getControlConnectionFailure?.() ?? null;
  }
  startPairedListener(): Promise<number> { return this.readOnly(); }
  stopPairedListener(): Promise<void> { return this.readOnly(); }
  async getPairedListenerStatus(): Promise<null> { return null; }
  disconnectPeer(): Promise<void> { return this.readOnly(); }
  createPairingInvitation(): Promise<string> { return Promise.reject(new Error("请在 Windows 端生成配对邀请。")); }
  async getEnrollmentStatus(): Promise<import('./types').UiEnrollmentStatus> {
    return JSON.parse(window.CapyIOAndroid?.getEnrollmentStatusJson?.() ?? '{"phase":"unavailable","message":"请更新配套 Android APK"}');
  }
  async getEnrollmentAddresses(): Promise<string[]> { return JSON.parse(window.CapyIOAndroid?.getEnrollmentAddressesJson?.() ?? '[]'); }
  async enrollmentAction(request: import('./types').UiEnrollmentRequest): Promise<import('./types').UiEnrollmentStatus> {
    const result = window.CapyIOAndroid?.enrollmentActionJson?.(JSON.stringify(request));
    if (!result) throw new Error('请更新配套 Android APK');
    return JSON.parse(result);
  }
  async importPairingInvitation(): Promise<void> {
    if (!window.CapyIOAndroid?.importPairingInvitation?.()) {
      throw new Error("当前 APK 不支持导入配对邀请。");
    }
  }
  async getDisplayPeers(): Promise<import("./types").UiDisplayPeer[]> {
    return JSON.parse(window.CapyIOAndroid?.getDisplayPeersJson?.() ?? '[]');
  }
  async getDisplayUse(): Promise<import("./types").UiDisplayUse | null> {
    return JSON.parse(window.CapyIOAndroid?.getDisplayUseJson?.() ?? 'null');
  }
  async startDisplayUse(peerNodeId: string, selection: import("./types").UiDisplaySelection): Promise<void> {
    if (!window.CapyIOAndroid?.startDisplayUse?.(peerNodeId, selection.width, selection.height, selection.refreshMillihertz, selection.bitrateKbps, selection.automatic ?? false)) {
      throw new Error('无法发起显示共享，请确认节点已连接且两端均为配套新版。');
    }
  }
  async stopDisplayUse(): Promise<void> {
    if (!window.CapyIOAndroid?.stopDisplayUse?.()) throw new Error('无法提交停止显示共享，请检查节点运行状态。');
  }
  async getDisplaySenderState(): Promise<import("./types").UiDisplaySenderState | null> { return null; }
  async startDisplaySender(): Promise<import("./types").UiDisplaySenderState | null> { throw new Error("本机不提供 Windows 显示发送控制。"); }
  async stopDisplaySender(): Promise<import("./types").UiDisplaySenderState | null> { throw new Error("本机不提供 Windows 显示发送控制。"); }
  async getDisplayReceiverState(): Promise<import("./types").UiDisplayReceiverState | null> {
    const encoded = window.CapyIOAndroid?.getDisplayStateJson?.();
    if (!encoded) return null;
    if (encoded.length > 2048) throw new Error("Display status exceeds its bound");
    const state = JSON.parse(encoded) as import("./types").UiDisplayReceiverState;
    if (state.schemaVersion !== 1 || !["unconfigured", "cpd1", "gamestream"].includes(state.transport)
        || typeof state.configurationPresent !== "boolean" || typeof state.nativeClientAvailable !== "boolean"
        || typeof state.canOpen !== "boolean" || typeof state.problemCode !== "string") {
      throw new Error("Invalid display receiver status");
    }
    return state;
  }
  async openDisplayReceiver(options?: import("./types").UiDisplayViewingOptions): Promise<void> {
    if (options && !window.CapyIOAndroid?.configureDisplayViewing?.(options.fullscreen, options.keepAwake, options.showStatistics)) {
      throw new Error("请更新配套 APK 后使用显示接收设置。");
    }
    if (!window.CapyIOAndroid?.openDisplayReceiver?.()) throw new Error("显示接收配置或原生组件尚未就绪，请刷新检查。");
  }
  async importDisplayProvisioning(): Promise<void> {
    if (!window.CapyIOAndroid?.importDisplayProvisioning?.()) {
      throw new Error("当前 APK 不支持从系统文件选择器导入显示连接包。");
    }
  }
  async getDiagnostics() {
    const encoded = window.CapyIOAndroid?.getDiagnosticsJson?.();
    if (!encoded) return { backend: "Android Node", version: "旧版 APK", revision: "未提供", builtAt: "未提供", checks: [{ label: "版本检查", detail: "请更新 APK 以读取原生构建标识。" }] };
    if (encoded.length > 16384) throw new Error("Android diagnostics response exceeds the size limit");
    const value = JSON.parse(encoded) as import("./types").UiDiagnostics;
    if (value.backend !== "Android Node" || typeof value.version !== "string"
      || typeof value.revision !== "string" || typeof value.builtAt !== "string"
      || !Array.isArray(value.checks) || value.checks.length > 16
      || !value.checks.every(check => check && typeof check.label === "string" && typeof check.detail === "string")) {
      throw new Error("Android diagnostics returned an incompatible response");
    }
    return value;
  }
  async getSnapshot(): Promise<UiSnapshot> {
    const encoded = window.CapyIOAndroid?.getSnapshotJson();
    if (!encoded) throw new Error("Android observer bridge is unavailable");
    const snapshot = JSON.parse(encoded) as UiSnapshot;
    if (snapshot.schemaVersion !== 5
      || snapshot.backendMode !== "android_node"
      || snapshot.controlPlane?.schemaVersion !== 1
      || typeof snapshot.controlPlane.simulated !== "boolean"
      || !["not_started", "listening", "simulated"].includes(snapshot.controlPlane.discoveryState)
      || !["not_configured", "ready", "failed", "simulated"].includes(snapshot.controlPlane.pairingState)
      || !["not_connected", "connected", "simulated"].includes(snapshot.controlPlane.transportState)
      || !["not_connected", "active", "simulated"].includes(snapshot.controlPlane.catalogExchangeState)
      || typeof snapshot.controlPlane.discoveredCandidateCount !== "number"
      || typeof snapshot.controlPlane.pairedPeerCount !== "number"
      || typeof snapshot.controlPlane.authenticatedConnectionCount !== "number"
      || (snapshot.controlPlane.listenPort !== null && (!Number.isInteger(snapshot.controlPlane.listenPort) || snapshot.controlPlane.listenPort < 1 || snapshot.controlPlane.listenPort > 65535))
      || !Array.isArray(snapshot.nodes)
      || !Array.isArray(snapshot.capabilities)
      || !Array.isArray(snapshot.routes)
      || !Array.isArray(snapshot.adapters)
      || !Array.isArray(snapshot.events)
      || !Array.isArray(snapshot.warnings)) {
      throw new Error("Android observer returned an incompatible snapshot");
    }
    const remoteJson = window.CapyIOAndroid?.getRemoteCatalogJson?.();
    if (remoteJson) {
      const remote = JSON.parse(remoteJson) as Pick<UiSnapshot, 'nodes' | 'capabilities'>;
      snapshot.nodes.push(...remote.nodes);
      snapshot.capabilities.push(...remote.capabilities);
    }
    return snapshot;
  }

  startDiscovery(): Promise<UiSnapshot> { return this.readOnly(); }
  stopDiscovery(): Promise<UiSnapshot> { return this.readOnly(); }

  setRoute(): Promise<UiSnapshot> { return this.readOnly(); }
  async setLocalCapabilityShared(capabilityId: string, selected: boolean): Promise<UiSnapshot> {
    if (!window.CapyIOAndroid?.setLocalCapabilityShared(capabilityId, selected)) {
      throw new Error("Android rejected the sharing selection");
    }
    return this.getSnapshot();
  }
  setRemoteCapabilityUse(): Promise<UiSnapshot> { return this.readOnly(); }
  resetDemo(): Promise<UiSnapshot> { return this.readOnly(); }
  getLiveImu(): Promise<UiLiveImu | null> { return Promise.resolve(null); }
  startLiveImu(): Promise<UiLiveImu> { return this.readOnly(); }
  stopLiveImu(): Promise<UiLiveImu> { return this.readOnly(); }
  async getGamepadState(): Promise<UiGamepadState | null> {
    const encoded = window.CapyIOAndroid?.getGamepadStateJson();
    if (!encoded) return null;
    const state = JSON.parse(encoded) as UiGamepadState;
    if (state.schemaVersion !== 1
      || state.source !== "android_touch"
      || state.profile !== "capyio.input.gamepad-state/1"
      || !Array.isArray(state.pressedButtons)) {
      throw new Error("Android gamepad bridge returned incompatible data");
    }
    return state;
  }
  refreshWindowsGamepadPreflight(): Promise<UiGamepadState> { return this.readOnly(); }
  startWindowsGamepadProjection(): Promise<UiGamepadState> { return this.readOnly(); }
  startControllerHost(): Promise<UiGamepadState> { return this.readOnly(); }
  stopControllerHost(): Promise<UiGamepadState> { return this.readOnly(); }
  stopWindowsGamepadProjection(): Promise<UiGamepadState> { return this.readOnly(); }
  async updateGamepadState(update: GamepadControlUpdate): Promise<UiGamepadState> {
    const bridge = window.CapyIOAndroid;
    if (!bridge) throw new Error("Android gamepad bridge is unavailable");
    let accepted = false;
    switch (update.kind) {
      case "button":
        accepted = bridge.updateGamepad("button", update.button, 0, 0, update.pressed);
        break;
      case "dpad":
        accepted = bridge.updateGamepad("dpad", "", update.x, update.y, false);
        break;
      case "stick":
        accepted = bridge.updateGamepad("stick", update.stick, update.x, update.y, false);
        break;
      case "trigger":
        accepted = bridge.updateGamepad("trigger", update.trigger, update.value, 0, false);
        break;
      case "reset":
        accepted = bridge.updateGamepad("reset", "", 0, 0, false);
        break;
    }
    if (!accepted) throw new Error("Android rejected the gamepad update");
    const state = await this.getGamepadState();
    if (!state) throw new Error("Android gamepad state became unavailable");
    return state;
  }
  startGamepadDsu(): Promise<UiGamepadState> { return this.readOnly(); }
  stopGamepadDsu(): Promise<UiGamepadState> { return this.readOnly(); }
  startAndroidGamepad(): Promise<UiGamepadState> { return this.readOnly(); }
  stopAndroidGamepad(): Promise<UiGamepadState> { return this.readOnly(); }
  async configureAndroidController(
    host: string, port: number, token: string,
  ): Promise<UiGamepadState> {
    if (!window.CapyIOAndroid?.configureControllerRoute(host, port, token)) {
      throw new Error("Android rejected the controller endpoint or token");
    }
    const state = await this.getGamepadState();
    if (!state) throw new Error("Android gamepad state became unavailable");
    return state;
  }
  async getTouchpadState(): Promise<UiTouchpadState | null> {
    const encoded = window.CapyIOAndroid?.getTouchpadStateJson();
    if (!encoded) return null;
    const state = JSON.parse(encoded) as UiTouchpadState;
    if (state.schemaVersion !== 1
      || state.source !== "android_shared_ui"
      || state.profile !== "capyio.input.touchpad-frames/1"
      || !Array.isArray(state.contacts)) {
      throw new Error("Android touchpad bridge returned incompatible data");
    }
    return state;
  }
  async getAndroidAudioState(): Promise<UiAndroidAudioState | null> {
    const encoded = window.CapyIOAndroid?.getAudioStateJson();
    if (!encoded) return null;
    const state = JSON.parse(encoded) as UiAndroidAudioState;
    if (state.schemaVersion !== 1 || !state.nodeId
      || !state.microphone || !state.speaker) {
      throw new Error("Android audio bridge returned incompatible data");
    }
    return state;
  }
  async configureAndroidAudioPeer(direction: "microphone" | "speaker", host: string, localPort: number, peerPort: number, trustedLabConfirmed: boolean): Promise<void> {
    if (!window.CapyIOAndroid?.configureAudioPeer?.(direction, host, localPort, peerPort, trustedLabConfirmed)) {
      throw new Error("保存失败：请停止该音频功能，检查私有 IPv4 与端口，并确认可信网络限制。旧版 APK 需要更新。");
    }
  }
  async getAndroidAudioPeerSettings(direction: "microphone" | "speaker") {
    const read = window.CapyIOAndroid?.getAudioPeerSettingsJson;
    if (!read) return null;
    return parseAudioPeerSettings(window.CapyIOAndroid!.getAudioPeerSettingsJson!(direction), direction);
  }
  async getAndroidCameraSettings(): Promise<UiAndroidCameraSettings | null> {
    const encoded = window.CapyIOAndroid?.getCameraSettingsJson();
    if (!encoded) return null;
    const state = JSON.parse(encoded) as UiAndroidCameraSettings;
    if (state.schemaVersion !== 1 || !Array.isArray(state.sources)
      || !["economy", "balanced", "clear"].includes(state.quality)) {
      throw new Error("Android camera bridge returned incompatible data");
    }
    return state;
  }
  async refreshAndroidCameraSources(): Promise<UiAndroidCameraSettings> {
    if (!window.CapyIOAndroid?.refreshCameraSources()) {
      throw new Error("Android rejected camera inventory refresh");
    }
    const state = await this.getAndroidCameraSettings();
    if (!state) throw new Error("Android camera settings became unavailable");
    return state;
  }
  async selectAndroidCameraSource(sourceKey: string): Promise<UiAndroidCameraSettings> {
    if (!window.CapyIOAndroid?.selectCameraSource(sourceKey)) {
      throw new Error("Android rejected camera source selection");
    }
    const state = await this.getAndroidCameraSettings();
    if (!state) throw new Error("Android camera settings became unavailable");
    return state;
  }
  async selectAndroidCameraQuality(quality: UiAndroidCameraSettings["quality"]): Promise<UiAndroidCameraSettings> {
    if (!window.CapyIOAndroid?.selectCameraQuality(quality)) {
      throw new Error("Android rejected camera quality selection");
    }
    const state = await this.getAndroidCameraSettings();
    if (!state) throw new Error("Android camera settings became unavailable");
    return state;
  }
  async updateTouchpadState(update: TouchpadControlUpdate): Promise<UiTouchpadState> {
    const bridge = window.CapyIOAndroid;
    if (!bridge) throw new Error("Android touchpad bridge is unavailable");
    const encoded = [...update.contacts]
      .sort((left, right) => left.contactId - right.contactId)
      .map((contact) => `${contact.contactId},${contact.x},${contact.y},${contact.pressure}`)
      .join(";");
    if (!bridge.updateTouchpad(encoded)) {
      throw new Error("Android rejected the touchpad update");
    }
    const state = await this.getTouchpadState();
    if (!state) throw new Error("Android touchpad state became unavailable");
    return state;
  }
  async getQuickActions(): Promise<UiQuickAction[]> {
    const encoded = window.CapyIOAndroid?.getQuickActionsJson();
    if (!encoded) throw new Error("Android Quick Action bridge is unavailable");
    const actions = JSON.parse(encoded) as UiQuickAction[];
    if (!Array.isArray(actions)
      || actions.some((action) => action.schemaVersion !== 2
        || action.kind !== "capability_control")) {
      throw new Error("Android Quick Action bridge returned incompatible data");
    }
    return actions;
  }

  async invokeQuickAction(actionId: string, operation: QuickActionOperation): Promise<UiQuickAction> {
    if (!window.CapyIOAndroid?.invokeQuickAction(actionId, operation)) {
      throw new Error("Android rejected the Capability operation");
    }
    const action = (await this.getQuickActions()).find((candidate) => candidate.id === actionId);
    if (!action) throw new Error("Android Capability action disappeared");
    return {
      ...action,
      status: operation === "stop" ? "stopping" : "starting",
      availableOperations: [],
    };
  }
  getAudioEndpoints(): Promise<UiAudioEndpointCatalog | null> { return Promise.resolve(null); }
  selectAudioEndpoint(): Promise<UiQuickAction> { return this.readOnly(); }

  private readOnly<T>(): Promise<T> {
    return Promise.reject(new Error("This operation is unavailable on the Android Node backend"));
  }
}

class TauriCapyIOApi implements CapyIOApi {
  getRemoteControllerSettings(nodeId: string): ReturnType<CapyIOApi['getRemoteControllerSettings']> { return invoke('get_remote_controller_settings', { nodeId }); }
  setRemoteControllerSettings(nodeId: string, expectedRevision: number, motionEnabled: boolean, hapticsStrength: number): Promise<void> { return invoke('set_remote_controller_settings', { nodeId, expectedRevision, motionEnabled, hapticsStrength }); }
  async getGamepadMotionSettings() { return null; }
  async getHapticsSettings() { return null; }
  async setHapticsStrength(_percent: number): Promise<void> { throw new Error('请在振动设备所在节点设置强度'); }
  async setGamepadMotionEnabled(_enabled: boolean): Promise<void> { throw new Error('请在手柄所在节点设置体感'); }
  async setAndroidControllerMode(_mode: 'touch' | 'motion' | 'stopped'): Promise<void> { throw new Error('请在 Android 手柄页选择发送模式'); }
  async getGamepadLayout(): Promise<string | null> { return localStorage.getItem('capyio.gamepad-layout.v1'); }
  async saveGamepadLayout(layout: string): Promise<void> { localStorage.setItem('capyio.gamepad-layout.v1', layout); }
  async setGamepadImmersive(_active: boolean): Promise<void> {}
  async getControlConnectionFailure(): Promise<string | null> { return null; }
  startPairedListener(nodeId: string, bindHost: string, port: number): Promise<number> {
    return invoke<number>("start_paired_listener", { request: { nodeId, bindHost, port } });
  }
  stopPairedListener(): Promise<void> { return invoke("stop_paired_listener"); }
  getPairedListenerStatus(): Promise<{nodeId: string; bindHost: string; port: number} | null> {
    return invoke("get_paired_listener_status");
  }
  async getBackgroundWaitingEnabled(): Promise<boolean> { return false; }
  async setBackgroundWaitingEnabled(): Promise<void> { throw new Error("Windows 后台节点等待尚未开放。"); }
  getControllerUse(): ReturnType<CapyIOApi['getControllerUse']> { return invoke('get_controller_use'); }
  async getCameraPeers(): ReturnType<CapyIOApi['getCameraPeers']> { return []; }
  async getCameraUse(): ReturnType<CapyIOApi['getCameraUse']> { return null; }
  async startCameraUse(): Promise<void> { throw new Error('Windows 请在远程资源中选择具体相机。'); }
  async stopCameraUse(): Promise<void> { throw new Error('Windows 请在远程资源中停止具体相机。'); }
  async getAudioPeers(): ReturnType<CapyIOApi['getAudioPeers']> { return []; }
  async getAudioUses(): ReturnType<CapyIOApi['getAudioUses']> { return []; }
  async startAudioUse(): Promise<void> { throw new Error('Windows 请在远程资源中选择具体麦克风或扬声器。'); }
  async stopAudioUse(): Promise<void> { throw new Error('Windows 请在远程资源中停止具体音频资源。'); }
  async getControllerPeers(): ReturnType<CapyIOApi['getControllerPeers']> { return []; } // Windows uses its complete Runtime snapshot.
  startControllerUse(peerNodeId: string, mode: import('./types').ControllerUseMode): Promise<void> { return invoke('start_controller_use', { peerNodeId, mode }); }
  stopControllerUse(): Promise<void> { return invoke('stop_controller_use'); }
  async getResourceRequests() { return parseResourceRequests(await invoke('get_resource_requests')); }
  async resolveResourceRequest(routeId: string, allow: boolean, remember = false): Promise<void> { return invoke('resolve_resource_request', { routeId, allow, remember }); }
  async getPairedPeers() { return parsePairedPeers(await invoke("get_paired_peers")); }
  getEnrollmentStatus(): Promise<import('./types').UiEnrollmentStatus> { return invoke('enrollment_status'); }
  async getEnrollmentAddresses(): Promise<string[]> { return (await this.getDiscoveryInterfaces()).map(item => item.ipv4); }
  enrollmentAction(request: import('./types').UiEnrollmentRequest): Promise<import('./types').UiEnrollmentStatus> { return invoke('enrollment_action', { request }); }
  checkPairedCredential(nodeId: string): Promise<void> {
    return invoke("check_paired_credential", { request: { nodeId } });
  }
  async getDiscoveryCandidates() { return parseDiscoveryCandidates(await invoke("get_discovery_candidates")); }
  async getDiscoveryInterfaces() { return parseDiscoveryInterfaces(await invoke("get_discovery_interfaces")); }
  connectDiscoveredPeer(nodeId: string): Promise<void> { return invoke("connect_discovered_peer", { request: { nodeId } }); }
  connectPairedPeerAt(nodeId: string, host: string, port: number): Promise<void> {
    return invoke("connect_paired_peer_at", { request: { nodeId, host, port } });
  }
  createPairingInvitation(nodeId: string, displayName: string, renewExisting = false): Promise<string> {
    return invoke<string>("create_pairing_invitation", { request: { nodeId, displayName, renewExisting } });
  }
  importPairingInvitation(): Promise<void> { return Promise.reject(new Error("请在 Android 端导入配对邀请。")); }
  disconnectPeer(nodeId: string): Promise<void> { return invoke("disconnect_peer", { request: { nodeId } }); }
  getDisplayPeers(): Promise<import("./types").UiDisplayPeer[]> { return invoke('get_display_peers'); }
  getDisplayUse(): Promise<import("./types").UiDisplayUse | null> { return invoke('get_display_use_state'); }
  startDisplayUse(peerNodeId: string, selection: import("./types").UiDisplaySelection): Promise<void> {
    return invoke('start_display_use', { peerNodeId, ...selection });
  }
  stopDisplayUse(): Promise<void> { return invoke('stop_display_use'); }
  async getDisplaySenderState(): Promise<import("./types").UiDisplaySenderState | null> { return parseDisplaySenderState(await invoke("get_display_sender_state")); }
  async startDisplaySender(): Promise<import("./types").UiDisplaySenderState | null> { return parseDisplaySenderState(await invoke("start_display_sender")); }
  async stopDisplaySender(): Promise<import("./types").UiDisplaySenderState | null> { return parseDisplaySenderState(await invoke("stop_display_sender")); }
  getDiagnostics() { return invoke<import("./types").UiDiagnostics>("get_diagnostics"); }
  getSnapshot(): Promise<UiSnapshot> {
    return invoke<UiSnapshot>("get_snapshot");
  }

  startDiscovery(interfaceIpv4: string): Promise<UiSnapshot> {
    return invoke<UiSnapshot>("start_discovery", { request: { interfaceIpv4 } });
  }

  stopDiscovery(): Promise<UiSnapshot> {
    return invoke<UiSnapshot>("stop_discovery");
  }

  setRoute(routeId: string, active: boolean): Promise<UiSnapshot> {
    return invoke<UiSnapshot>("set_route", {
      request: { routeId, active },
    });
  }

  setLocalCapabilityShared(capabilityId: string, selected: boolean): Promise<UiSnapshot> {
    return invoke<UiSnapshot>("set_local_capability_shared", {
      request: { capabilityId, selected },
    });
  }

  setRemoteCapabilityUse(capabilityId: string, active: boolean): Promise<UiSnapshot> {
    return invoke<UiSnapshot>("set_remote_capability_use", {
      request: { capabilityId, active },
    });
  }

  resetDemo(): Promise<UiSnapshot> {
    return invoke<UiSnapshot>("reset_demo");
  }

  getLiveImu(): Promise<UiLiveImu> {
    return invoke<UiLiveImu>("get_live_imu");
  }

  startLiveImu(ip: string, port: number): Promise<UiLiveImu> {
    return invoke<UiLiveImu>("start_live_imu", { request: { ip, port } });
  }

  stopLiveImu(): Promise<UiLiveImu> {
    return invoke<UiLiveImu>("stop_live_imu");
  }

  getGamepadState(): Promise<UiGamepadState> {
    return invoke<UiGamepadState>("get_gamepad_state");
  }

  refreshWindowsGamepadPreflight(controllerKind: WindowsControllerKind): Promise<UiGamepadState> {
    return invoke<UiGamepadState>("refresh_windows_gamepad_preflight", { request: { controllerKind } });
  }

  startWindowsGamepadProjection(controllerKind: WindowsControllerKind): Promise<UiGamepadState> {
    return invoke<UiGamepadState>("start_windows_gamepad_projection", { request: { controllerKind } });
  }

  startControllerHost(controllerKind: WindowsControllerKind, port: number): Promise<UiGamepadState> {
    return invoke<UiGamepadState>("start_controller_host", { request: { controllerKind, port } });
  }

  stopControllerHost(): Promise<UiGamepadState> {
    return invoke<UiGamepadState>("stop_controller_host");
  }

  stopWindowsGamepadProjection(): Promise<UiGamepadState> {
    return invoke<UiGamepadState>("stop_windows_gamepad_projection");
  }

  updateGamepadState(update: GamepadControlUpdate): Promise<UiGamepadState> {
    return invoke<UiGamepadState>("update_gamepad_state", { request: update });
  }

  getTouchpadState(): Promise<UiTouchpadState | null> {
    return Promise.resolve(null);
  }

  updateTouchpadState(): Promise<UiTouchpadState> {
    return Promise.reject(new Error("Touchpad surface is available in the Android shared UI"));
  }

  startGamepadDsu(port: number, mode: import("./types").DsuProjectionMode): Promise<UiGamepadState> {
    return invoke<UiGamepadState>("start_gamepad_dsu", { request: { port, mode } });
  }

  stopGamepadDsu(): Promise<UiGamepadState> {
    return invoke<UiGamepadState>("stop_gamepad_dsu");
  }

  startAndroidGamepad(port: number): Promise<UiGamepadState> {
    return invoke<UiGamepadState>("start_android_gamepad", { request: { port } });
  }

  stopAndroidGamepad(): Promise<UiGamepadState> {
    return invoke<UiGamepadState>("stop_android_gamepad");
  }

  configureAndroidController(): Promise<UiGamepadState> {
    return Promise.reject(new Error("Controller sender configuration requires the Android backend"));
  }

  getQuickActions(): Promise<UiQuickAction[]> {
    return invoke<UiQuickAction[]>("get_quick_actions");
  }

  invokeQuickAction(actionId: string, operation: QuickActionOperation): Promise<UiQuickAction> {
    return invoke<UiQuickAction>("invoke_quick_action", {
      request: { actionId, operation },
    });
  }

  getAudioEndpoints(): Promise<UiAudioEndpointCatalog> {
    return invoke<UiAudioEndpointCatalog>("get_audio_endpoints");
  }

  getAndroidAudioState(): Promise<UiAndroidAudioState | null> {
    return Promise.resolve(null);
  }
  getDisplayReceiverState(): Promise<import("./types").UiDisplayReceiverState | null> { return Promise.resolve(null); }
  importDisplayProvisioning(): Promise<void> { return Promise.reject(new Error("请在 Android 设备上导入显示连接包。")); }
  openDisplayReceiver(): Promise<void> { return Promise.reject(new Error("当前平台没有 Android 显示接收页。")); }
  async getAndroidAudioPeerSettings() { return null; }
  configureAndroidAudioPeer(): Promise<void> {
    return Promise.reject(new Error("请在 Android 设备上配置本机音频连接。"));
  }

  getAndroidCameraSettings(): Promise<UiAndroidCameraSettings | null> { return Promise.resolve(null); }
  refreshAndroidCameraSources(): Promise<UiAndroidCameraSettings> { return Promise.reject(new Error("Android camera settings require the Android backend")); }
  selectAndroidCameraSource(): Promise<UiAndroidCameraSettings> { return Promise.reject(new Error("Android camera settings require the Android backend")); }
  selectAndroidCameraQuality(): Promise<UiAndroidCameraSettings> { return Promise.reject(new Error("Android camera settings require the Android backend")); }

  selectAudioEndpoint(actionId: string, selectionToken: string): Promise<UiQuickAction> {
    return invoke<UiQuickAction>("select_audio_endpoint", {
      request: { actionId, selectionToken },
    });
  }
}

export function createCapyIOApi(): CapyIOApi {
  if (window.__TAURI_INTERNALS__) return new TauriCapyIOApi();
  if (window.CapyIOAndroid) return new AndroidNodeCapyIOApi();
  return new BrowserMockCapyIOApi();
}
