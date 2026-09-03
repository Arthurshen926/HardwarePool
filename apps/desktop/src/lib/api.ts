import { invoke } from "@tauri-apps/api/core";

import { BrowserMockCapyIOApi } from "./mock";
import type { CapyIOApi, GamepadControlUpdate, QuickActionOperation, UiAudioEndpointCatalog, UiGamepadState, UiLiveImu, UiQuickAction, UiSnapshot, WindowsControllerKind } from "./types";

declare global {
  interface Window {
    CapyIOAndroid?: {
      getSnapshotJson(): string;
      getQuickActionsJson(): string;
      invokeQuickAction(actionId: string, operation: QuickActionOperation): boolean;
    };
  }
}

class AndroidObserverCapyIOApi implements CapyIOApi {
  async getSnapshot(): Promise<UiSnapshot> {
    const encoded = window.CapyIOAndroid?.getSnapshotJson();
    if (!encoded) throw new Error("Android observer bridge is unavailable");
    const snapshot = JSON.parse(encoded) as UiSnapshot;
    if (snapshot.schemaVersion !== 4
      || snapshot.backendMode !== "android_observer"
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
  setLocalCapabilityShared(): Promise<UiSnapshot> { return this.readOnly(); }
  resetDemo(): Promise<UiSnapshot> { return this.readOnly(); }
  getLiveImu(): Promise<UiLiveImu | null> { return Promise.resolve(null); }
  startLiveImu(): Promise<UiLiveImu> { return this.readOnly(); }
  stopLiveImu(): Promise<UiLiveImu> { return this.readOnly(); }
  getGamepadState(): Promise<UiGamepadState | null> { return Promise.resolve(null); }
  refreshWindowsGamepadPreflight(): Promise<UiGamepadState> { return this.readOnly(); }
  startWindowsGamepadProjection(): Promise<UiGamepadState> { return this.readOnly(); }
  stopWindowsGamepadProjection(): Promise<UiGamepadState> { return this.readOnly(); }
  updateGamepadState(): Promise<UiGamepadState> { return this.readOnly(); }
  startGamepadDsu(): Promise<UiGamepadState> { return this.readOnly(); }
  stopGamepadDsu(): Promise<UiGamepadState> { return this.readOnly(); }
  startAndroidGamepad(): Promise<UiGamepadState> { return this.readOnly(); }
  stopAndroidGamepad(): Promise<UiGamepadState> { return this.readOnly(); }
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
    return Promise.reject(new Error("Android shared UI is read-only in A01C2"));
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

  selectAudioEndpoint(actionId: string, selectionToken: string): Promise<UiQuickAction> {
    return invoke<UiQuickAction>("select_audio_endpoint", {
      request: { actionId, selectionToken },
    });
  }
}

export function createCapyIOApi(): CapyIOApi {
  if (window.__TAURI_INTERNALS__) return new TauriCapyIOApi();
  if (window.CapyIOAndroid) return new AndroidObserverCapyIOApi();
  return new BrowserMockCapyIOApi();
}
