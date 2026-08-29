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

export interface UiVector3 {
  x: number;
  y: number;
  z: number;
}

export interface UiImuFixture {
  mode: "deterministic_fixture";
  simulated: true;
  profile: "capyio.motion.imu-samples/1";
  sequence: number;
  sourceTimestampNanos: number;
  clockDomainId: string;
  acceleration: UiVector3;
  angularVelocity: UiVector3;
  panelReceived: number;
  panelMissingSequences: number;
  recorderRecords: number;
  panelRouteState: "active";
  recorderRouteState: "active";
}

export type LiveImuStatus = "idle" | "connecting" | "active" | "offline" | "stopped" | "failed" | "unsupported";

export interface UiLiveImu {
  status: LiveImuStatus;
  simulated: boolean;
  routeId: string;
  routeState: RouteState;
  endpoint: string | null;
  profile: "capyio.motion.imu-samples/1";
  streamEpoch: number;
  sequence: number | null;
  sourceTimestampNanos: number | null;
  clockDomainId: string | null;
  acceleration: UiVector3 | null;
  angularVelocity: UiVector3 | null;
  receivedSamples: number;
  problemCode: string | null;
  problem: string | null;
}

export type QuickActionOperation = "start" | "retry" | "stop";

export interface UiQuickAction {
  schemaVersion: 2;
  id: string;
  kind: "route_control";
  title: string;
  summary: string;
  status: "blocked" | "idle" | "starting" | "active" | "stopping" | "offline" | "failed";
  simulated: boolean;
  routeId: string | null;
  routeState: RouteState | null;
  routeEpoch: number | null;
  availableOperations: QuickActionOperation[];
  evidenceLevel: "not_started" | "process_and_route_state" | "stable_tcp_receiver_presence" | "stable_phone_tcp_presence";
  connectionHint: string | null;
  problemCode: string | null;
  problem: string | null;
}

export interface UiAudioEndpointChoice {
  selectionToken: string;
  displayName: string;
  isDefault: boolean;
  selected: boolean;
}

export interface UiAudioEndpointCatalog {
  schemaVersion: 1;
  actionId: string;
  supported: boolean;
  canSelect: boolean;
  choices: UiAudioEndpointChoice[];
  problem: string | null;
}

export interface UiSnapshot {
  backendMode: BackendMode;
  schemaVersion: 3;
  projectVersion: string;
  nodes: UiNode[];
  routes: UiRoute[];
  adapters: UiAdapter[];
  events: UiEvent[];
  warnings: string[];
  imuFixture: UiImuFixture;
}

export interface CapyIOApi {
  getSnapshot(): Promise<UiSnapshot>;
  setRoute(routeId: string, active: boolean): Promise<UiSnapshot>;
  resetDemo(): Promise<UiSnapshot>;
  getLiveImu(): Promise<UiLiveImu>;
  startLiveImu(ip: string, port: number): Promise<UiLiveImu>;
  stopLiveImu(): Promise<UiLiveImu>;
  getQuickActions(): Promise<UiQuickAction[]>;
  invokeQuickAction(actionId: string, operation: QuickActionOperation): Promise<UiQuickAction>;
  getAudioEndpoints(): Promise<UiAudioEndpointCatalog>;
  selectAudioEndpoint(actionId: string, selectionToken: string): Promise<UiQuickAction>;
}
