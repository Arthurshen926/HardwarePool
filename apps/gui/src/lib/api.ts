import { invoke } from "@tauri-apps/api/core";

import { BrowserMockHardwarePoolApi } from "./mock";
import type { HardwarePoolApi, UiSnapshot } from "./types";

class TauriHardwarePoolApi implements HardwarePoolApi {
  getSnapshot(): Promise<UiSnapshot> {
    return invoke<UiSnapshot>("get_snapshot");
  }

  setProjection(
    capabilityId: string,
    active: boolean,
  ): Promise<UiSnapshot> {
    return invoke<UiSnapshot>("set_projection", {
      request: { capabilityId, active },
    });
  }

  resetDemo(): Promise<UiSnapshot> {
    return invoke<UiSnapshot>("reset_demo");
  }
}

export function createHardwarePoolApi(): HardwarePoolApi {
  return window.__TAURI_INTERNALS__
    ? new TauriHardwarePoolApi()
    : new BrowserMockHardwarePoolApi();
}
