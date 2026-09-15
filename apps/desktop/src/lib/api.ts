import { invoke } from "@tauri-apps/api/core";

import { BrowserMockCapyIOApi } from "./mock";
import type { CapyIOApi, GamepadControlUpdate, QuickActionOperation, TouchpadControlUpdate, UiAndroidAudioState, UiAndroidCameraSettings, UiAudioEndpointCatalog, UiGamepadState, UiLiveImu, UiQuickAction, UiSnapshot, UiTouchpadState, WindowsControllerKind } from "./types";

declare global {
  interface Window {
    CapyIOAndroid?: {
      getSnapshotJson(): string;
      getQuickActionsJson(): string;
      getGamepadStateJson(): string;
      getTouchpadStateJson(): string;
      getAudioStateJson(): string;
      getCameraSettingsJson(): string;
      setLocalCapabilityShared(capabilityId: string, selected: boolean): boolean;
      invokeQuickAction(actionId: string, operation: QuickActionOperation): boolean;
      updateGamepad(kind: string, target: string, x: number, y: number, pressed: boolean): boolean;
      updateTouchpad(encodedContacts: string): boolean;
      refreshCameraSources(): boolean;
      selectCameraSource(sourceKey: string): boolean;
      selectCameraQuality(quality: string): boolean;
    };
  }
}

class AndroidNodeCapyIOApi implements CapyIOApi {
  async getSnapshot(): Promise<UiSnapshot> {
    const encoded = window.CapyIOAndroid?.getSnapshotJson();
    if (!encoded) throw new Error("Android observer bridge is unavailable");
    const snapshot = JSON.parse(encoded) as UiSnapshot;
    if (snapshot.schemaVersion !== 4
      || snapshot.backendMode !== "android_node"
      || !Array.isArray(snapshot.nodes)
      || !Array.isArray(snapshot.capabilities)
      || !Array.isArray(snapshot.routes)
      || !Array.isArray(snapshot.adapters)
      || !Array.isArray(snapshot.events)
      || !Array.isArray(snapshot.warnings)) {
      throw new Error("Android observer returned an incompatible snapshot");
    }
    return snapshot;
  }

  setRoute(): Promise<UiSnapshot> { return this.readOnly(); }
  async setLocalCapabilityShared(capabilityId: string, selected: boolean): Promise<UiSnapshot> {
    if (!window.CapyIOAndroid?.setLocalCapabilityShared(capabilityId, selected)) {
      throw new Error("Android rejected the sharing selection");
    }
    return this.getSnapshot();
  }
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
  getSnapshot(): Promise<UiSnapshot> {
    return invoke<UiSnapshot>("get_snapshot");
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

  startWindowsGamepadProjection(enableXinputCompanion: boolean): Promise<UiGamepadState> {
    return invoke<UiGamepadState>("start_windows_gamepad_projection", { request: { enableXinputCompanion } });
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
