import { invoke } from "@tauri-apps/api/core";

import { BrowserMockCapyIOApi } from "./mock";
import type { CapyIOApi, GamepadControlUpdate, QuickActionOperation, UiAudioEndpointCatalog, UiGamepadState, UiLiveImu, UiQuickAction, UiSnapshot, WindowsControllerKind } from "./types";

class TauriCapyIOApi implements CapyIOApi {
  getSnapshot(): Promise<UiSnapshot> {
    return invoke<UiSnapshot>("get_snapshot");
  }

  setRoute(routeId: string, active: boolean): Promise<UiSnapshot> {
    return invoke<UiSnapshot>("set_route", {
      request: { routeId, active },
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
  return window.__TAURI_INTERNALS__
    ? new TauriCapyIOApi()
    : new BrowserMockCapyIOApi();
}
