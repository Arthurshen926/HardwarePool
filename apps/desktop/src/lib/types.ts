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

export type GamepadButton =
  | "south"
  | "east"
  | "west"
  | "north"
  | "left_shoulder"
  | "right_shoulder"
  | "left_stick"
  | "right_stick"
  | "select"
  | "start"
  | "guide"
  | "touchpad"
  | "paddle1"
  | "paddle2"
  | "paddle3"
  | "paddle4";

export interface UiGamepadAxis2 {
  x: number;
  y: number;
}

export interface UiGamepadState {
  schemaVersion: 1;
  source: "desktop_simulator" | "android_touch";
  simulated: boolean;
  profile: "capyio.input.gamepad-state/1";
  streamEpoch: number;
  sequence: number | null;
  sourceTimestampNanos: number | null;
  pressedButtons: GamepadButton[];
  dpad: UiGamepadAxis2;
  leftStick: UiGamepadAxis2;
  rightStick: UiGamepadAxis2;
  leftTrigger: number;
  rightTrigger: number;
  lastUpdate: string;
  dsuProjection: UiDsuProjection;
  windowsProjection: UiWindowsGamepadProjection;
  androidInput: UiAndroidInput;
  motion: UiGamepadMotion;
}

export interface UiGamepadMotion {
  source: "stationary_fixture" | "android_sensors";
  sourceTimestampNanos: number | null;
  acceleration: [number, number, number];
  angularVelocity: [number, number, number];
}

export interface UiAndroidInput {
  supported: boolean;
  status: "unsupported" | "idle" | "listening" | "connected" | "stopped" | "failed";
  endpoint: string | null;
  lanHostHint: string | null;
  pairingToken: string | null;
  peerConnected: boolean;
  acceptedPackets: number;
  rejectedPackets: number;
  replayedPackets: number;
  peerTimeouts: number;
  projectionQueueFull: number;
  packetAgeMillis: number | null;
  remoteSequence: number | null;
  lastEvent: string;
}

export interface UiDsuProjection {
  supported: boolean;
  status: "unsupported" | "idle" | "active" | "stopped" | "failed";
  endpoint: string | null;
  mode: DsuProjectionMode;
  lastSubmit: string;
  controlsSubmitted: number;
  controlsAccepted: number;
  controlsQueueFull: number;
  controlsNeutralResets: number;
  activeSubscribers: number;
  padPacketsSent: number;
  packetSendErrors: number;
}

export type DsuProjectionMode = "motion_only" | "motion_and_controls";

export type WindowsControllerKind = "xbox360" | "dualshock4";

export interface UiWindowsGamepadProjection {
  supported: boolean;
  status: "unsupported" | "host_gate_required" | "starting" | "export_ready" | "active" | "stopped" | "offline" | "failed";
  controllerKind: WindowsControllerKind;
  deviceIdentity: string;
  viiperEndpoint: string | null;
  usbipEndpoint: string | null;
  viiperReady: boolean;
  usbipReady: boolean;
  xinputAvailable: boolean;
  xinputReady: boolean;
  exportCount: number;
  busId: string | null;
  ownedUsbipPort: number | null;
  inputPackets: number;
  nonNeutralPackets: number;
  ds4RejectedPackets: number;
  xinputPackets: number;
  inputOfflineEvents: number;
  lastRemoteSequence: number | null;
  lastEvent: string;
  problemCode: string | null;
  problem: string | null;
}

export type GamepadControlUpdate =
  | { kind: "button"; button: GamepadButton; pressed: boolean }
  | { kind: "dpad"; x: number; y: number }
  | { kind: "stick"; stick: "left" | "right"; x: number; y: number }
  | { kind: "trigger"; trigger: "left" | "right"; value: number }
  | { kind: "reset" };

export type QuickActionOperation = "start" | "retry" | "stop";

export interface UiQuickAction {
  schemaVersion: 1;
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
  evidenceLevel: "not_started" | "process_and_route_state" | "stable_tcp_receiver_presence";
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
  getGamepadState(): Promise<UiGamepadState>;
  refreshWindowsGamepadPreflight(controllerKind: WindowsControllerKind): Promise<UiGamepadState>;
  startWindowsGamepadProjection(enableXinputCompanion: boolean): Promise<UiGamepadState>;
  stopWindowsGamepadProjection(): Promise<UiGamepadState>;
  updateGamepadState(update: GamepadControlUpdate): Promise<UiGamepadState>;
  startGamepadDsu(port: number, mode: DsuProjectionMode): Promise<UiGamepadState>;
  stopGamepadDsu(): Promise<UiGamepadState>;
  startAndroidGamepad(port: number): Promise<UiGamepadState>;
  stopAndroidGamepad(): Promise<UiGamepadState>;
  getQuickActions(): Promise<UiQuickAction[]>;
  invokeQuickAction(actionId: string, operation: QuickActionOperation): Promise<UiQuickAction>;
  getAudioEndpoints(): Promise<UiAudioEndpointCatalog>;
  selectAudioEndpoint(actionId: string, selectionToken: string): Promise<UiQuickAction>;
}
