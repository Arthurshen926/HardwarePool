import { invoke } from "@tauri-apps/api/core";

import { BrowserMockCapyIOApi } from "./mock";
import type { CapyIOApi, UiSnapshot } from "./types";

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
}

export function createCapyIOApi(): CapyIOApi {
  return window.__TAURI_INTERNALS__
    ? new TauriCapyIOApi()
    : new BrowserMockCapyIOApi();
}
