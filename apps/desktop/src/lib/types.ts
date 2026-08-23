export type BackendMode = "browser_mock" | "tauri_demo";

export type RouteState =
  | "draft"
  | "prepared"
  | "starting"
  | "active"
  | "stopping"
  | "stopped"
  | "failed"
  | "offline";

export interface UiMetricSet {
  estimatedLatencyMs: number | null;
  packetLossPercent: number | null;
  bufferFillMs: number | null;
  simulated: boolean;
}

export interface UiPort {
  nodeName: string;
  capabilityName: string;
  capabilityClass: string;
  portName: string;
  direction: "source" | "sink" | "control";
}

export interface UiRoute {
  id: string;
  title: string;
  summary: string;
  profile: string;
  backend: string;
  state: RouteState;
  active: boolean;
  source: UiPort;
  sink: UiPort;
  formatSummary: string | null;
  qosModes: string[];
  projectionNote: string;
  metrics: UiMetricSet;
}

export interface UiNode {
  id: string;
  displayName: string;
  platform: string;
  platformVersion: string;
  online: boolean;
  local: boolean;
  capabilityCount: number;
}

export interface UiAdapter {
  id: string;
  nodeName: string;
  displayName: string;
  adapterType: string;
  deploymentMode: string;
  state: string;
  health: string;
  capabilityCount: number;
}

export interface UiEvent {
  sequence: number;
  summary: string;
}

export interface UiSnapshot {
  backendMode: BackendMode;
  schemaVersion: 2;
  projectVersion: string;
  nodes: UiNode[];
  routes: UiRoute[];
  adapters: UiAdapter[];
  events: UiEvent[];
  warnings: string[];
}

export interface CapyIOApi {
  getSnapshot(): Promise<UiSnapshot>;
  setRoute(routeId: string, active: boolean): Promise<UiSnapshot>;
  resetDemo(): Promise<UiSnapshot>;
}
