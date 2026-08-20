export type BackendMode = "browser_mock" | "tauri_demo";

export type CapabilityKind =
  | "audio_capture"
  | "audio_render"
  | "audio_duplex_bundle";

export type BindingState =
  | "not_mapped"
  | "requested"
  | "authorized"
  | "negotiated"
  | "starting"
  | "active"
  | "suspended"
  | "stopping"
  | "stopped"
  | "rejected"
  | "offline"
  | "failed";

export interface UiMetricSet {
  estimatedLatencyMs: number | null;
  packetLossPercent: number | null;
  bufferFillMs: number | null;
  underruns: number;
  overruns: number;
  simulated: boolean;
}

export interface UiCapability {
  id: string;
  displayName: string;
  kind: CapabilityKind;
  profile: string;
  permissionRequirement: string;
  availability: string;
  projectionKind: string | null;
  bindingState: BindingState;
  active: boolean;
  formatSummary: string | null;
  qosModes: string[];
  metrics: UiMetricSet;
}

export interface UiPeer {
  id: string;
  displayName: string;
  platform: string;
  platformVersion: string;
  online: boolean;
}

export interface UiEvent {
  sequence: number;
  summary: string;
}

export interface UiSnapshot {
  backendMode: BackendMode;
  schemaVersion: 1;
  projectVersion: string;
  localNodeName: string;
  peers: UiPeer[];
  capabilities: UiCapability[];
  events: UiEvent[];
  warnings: string[];
}

export interface HardwarePoolApi {
  getSnapshot(): Promise<UiSnapshot>;
  setProjection(capabilityId: string, active: boolean): Promise<UiSnapshot>;
  resetDemo(): Promise<UiSnapshot>;
}
