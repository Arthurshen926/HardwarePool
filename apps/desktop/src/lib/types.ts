export type BackendMode = "android_node" | "browser_mock" | "tauri_demo";
export interface UiEnrollmentStatus {
  discoveryState?: string;
  discoveryMessage?: string;
  candidates?: Array<{ nodeId: string; name: string; host: string; port: number }>;
  phase: string;
  pin?: string | null;
  endpoint?: string | null;
  peerNodeId?: string | null;
  peerName?: string | null;
  peerControlHost?: string | null;
  peerControlPort?: number | null;
  message: string;
}
export interface UiEnrollmentRequest {
  action: 'welcome' | 'join' | 'approve' | 'cancel' | 'recover' | 'discover';
  host?: string;
  port?: number;
  pin?: string;
}
export interface UiAndroidAudioPeerSettings {
  schemaVersion: 1;
  direction: "microphone" | "speaker";
  savedPeer: { host: string; localPort: number; peerPort: number } | null;
}

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
  sourceCapabilityId: string | null;
  sinkCapabilityId: string | null;
  title: string;
  summary: string;
  profile: string;
  backend: string;
  state: RouteState;
  active: boolean;
  authorization: "pending" | "authorized" | "denied" | "revoked" | "unknown";
  authorizationExpiresAtMs: number | null;
  source: UiPort;
  sink: UiPort;
  formatSummary: string | null;
  qosModes: string[];
  projectionNote: string;
  error?: string | null;
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

export type CapabilityAvailability = "available" | "busy" | "permission_required" | "offline";
export type PermissionRequirement = "none" | "user_confirmation" | "foreground_service" | "privileged";

export interface UiCapabilityPort {
  id: string;
  displayName: string;
  direction: "source" | "sink" | "control";
  profile: string;
  formats: string[];
}

export interface UiCapability {
  id: string;
  nodeId: string;
  nodeName: string;
  displayName: string;
  capabilityClass: string;
  local: boolean;
  availability: CapabilityAvailability;
  permissionRequirement: PermissionRequirement;
  inventorySource: "android_node_catalog" | "runtime_catalog" | "authenticated_peer_catalog" | "host_adapter_catalog";
  simulated: boolean;
  selectableForSharing: boolean;
  selectedForSharing: boolean;
  ports: UiCapabilityPort[];
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
  source: "stationary_fixture" | "android_sensors" | "unavailable";
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
  feedbackPackets?: number;
  feedbackSmallMotor?: number;
  feedbackLargeMotor?: number;
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

export interface UiTouchpadContact {
  contactId: number;
  x: number;
  y: number;
  pressure: number;
}

export interface UiTouchpadState {
  schemaVersion: 1;
  source: "android_shared_ui";
  profile: "capyio.input.touchpad-frames/1";
  streamEpoch: number;
  sequence: number;
  status: "idle" | "starting" | "active" | "stopping" | "failed";
  contacts: UiTouchpadContact[];
}

export interface TouchpadControlUpdate {
  contacts: UiTouchpadContact[];
}

export type QuickActionOperation = "start" | "retry" | "stop";

export interface UiQuickAction {
  schemaVersion: 2;
  id: string;
  kind: "capability_control" | "route_control";
  title: string;
  summary: string;
  status: "blocked" | "idle" | "starting" | "active" | "stopping" | "offline" | "failed";
  simulated: boolean;
  routeId: string | null;
  routeState: RouteState | null;
  routeEpoch: number | null;
  availableOperations: QuickActionOperation[];
  evidenceLevel: "not_started" | "process_and_route_state" | "stable_tcp_receiver_presence" | "stable_phone_tcp_presence" | "native_receiver_process_ready_only";
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

export interface UiAndroidAudioFormat {
  sampleRateHz: number;
  channelCount: number;
  encoding: string;
  framesPerBuffer: number;
}

export interface UiAndroidAudioCapability {
  state: "stopped" | "starting" | "active" | "stopping" | "failed";
  generation: number;
  framesProcessed: number;
  permissionReady: boolean;
  actualFormat: UiAndroidAudioFormat | null;
  problemCode: string | null;
}

export interface UiAndroidAudioState {
  schemaVersion: 1;
  nodeId: string;
  microphone: UiAndroidAudioCapability;
  speaker: UiAndroidAudioCapability;
}

export interface UiAndroidCameraSource {
  key: string;
  cameraId: string;
  facing: "front" | "back" | "external" | "unknown";
  zoomRatioMilli: number | null;
}

export interface UiAndroidCameraSettings {
  schemaVersion: 1;
  state: "stopped" | "starting" | "active" | "failed";
  generation: number;
  transportStatus?: string;
  capturedFrames?: number;
  encodedAccessUnits?: number;
  sentAccessUnits?: number;
  exportDroppedAccessUnits?: number;
  permissionReady: boolean;
  selectedSourceKey: string | null;
  quality: "economy" | "balanced" | "clear";
  captureProfile: "legacy" | "finite_720p30_v1";
  finitePreflight: "not_requested" | "eligible" | "unsupported";
  problemCode: string | null;
  sources: UiAndroidCameraSource[];
}

export interface UiControlPlaneStatus {
  schemaVersion: 1;
  simulated: boolean;
  discoveryState: "not_started" | "listening" | "simulated";
  pairingState: "not_configured" | "ready" | "failed" | "simulated";
  transportState: "not_connected" | "connected" | "simulated";
  catalogExchangeState: "not_connected" | "active" | "simulated";
  discoveredCandidateCount: number;
  pairedPeerCount: number;
  authenticatedConnectionCount: number;
  listenPort: number | null;
  pairingStorePath?: string | null;
  processUserSid?: string | null;
}

export interface UiSnapshot {
  backendMode: BackendMode;
  schemaVersion: 5;
  projectVersion: string;
  controlPlane: UiControlPlaneStatus;
  nodes: UiNode[];
  capabilities: UiCapability[];
  routes: UiRoute[];
  portLeases?: UiPortLease[];
  adapters: UiAdapter[];
  events: UiEvent[];
  warnings: string[];
  imuFixture: UiImuFixture | null;
}

export interface UiPortLease {
  nodeId: string;
  capabilityId: string;
  portId: string;
  routeId: string;
  sessionId: string;
  expiresAtMs: number;
}

export interface UiDiagnostics {
  backend: string;
  version: string;
  revision: string;
  builtAt: string;
  checks: { label: string; detail: string }[];
}

export interface UiDisplayViewingOptions {
  fullscreen: boolean;
  keepAwake: boolean;
  showStatistics: boolean;
}

export interface UiDisplayReceiverState {
  schemaVersion: 1;
  viewingOptions?: UiDisplayViewingOptions;
  transport: "unconfigured" | "cpd1" | "gamestream";
  configurationPresent: boolean;
  nativeClientAvailable: boolean;
  canOpen: boolean;
  problemCode: string;
}

export interface UiDisplaySelection {
  automatic?: boolean;
  width: number;
  height: number;
  refreshMillihertz: number;
  bitrateKbps: number;
}
export interface UiDisplayUse extends UiDisplaySelection {
  peerNodeId: string;
  status: 'awaiting_consent' | 'starting' | 'active' | 'stopping' | 'failed';
  error?: string | null;
  presentedFrames?: number;
}
export interface UiDisplayPeer { nodeId: string; displayName: string; displayAvailable?: boolean; }

export interface UiDisplaySenderState {
  schemaVersion: 1;
  available: boolean;
  state: "unavailable" | "stopped" | "starting" | "running" | "stopping" | "failed";
  generation: number;
  problemCode: string | null;
}

export type ControllerUseMode = 'dualshock4' | 'xbox360' | 'dsu';
export interface UiControllerPeer { nodeId: string; displayName: string; }
export interface UiCameraUse {
  peerNodeId: string;
  status: 'idle' | 'awaiting_consent' | 'starting' | 'active' | 'stopping' | 'failed';
  error?: string | null;
}
export interface UiAndroidMicrophoneSettings {
  schemaVersion: 1;
  revision: number;
  captureProfile: 'legacy' | 'secure_mono_tls13_v1';
  source: 'system_default';
  eligibility: 'not_requested' | 'eligible' | 'unavailable';
  permissionReady: boolean;
}
export type AudioUseDirection = 'microphone' | 'microphone-secure' | 'speaker';
export interface UiAudioPeer { nodeId: string; displayName: string; directions: AudioUseDirection[]; unavailableDirections?: { direction: AudioUseDirection; reason: string }[]; }
export interface UiAudioUse {
  peerNodeId: string;
  direction: AudioUseDirection;
  status: 'idle' | 'awaiting_consent' | 'starting' | 'active' | 'stopping' | 'failed';
  waitingMedia?: boolean;
  message?: string;
  error?: string | null;
}
export interface UiControllerUse {
  peerNodeId: string;
  mode: ControllerUseMode;
  status: 'idle' | 'awaiting_consent' | 'starting' | 'active' | 'stopping' | 'failed';
  error?: string | null;
}

export interface CapyIOApi {
  getCameraPeers(): Promise<UiControllerPeer[]>;
  getCameraUse(): Promise<UiCameraUse | null>;
  startCameraUse(peerNodeId: string): Promise<void>;
  stopCameraUse(): Promise<void>;
  getAudioPeers(): Promise<UiAudioPeer[]>;
  getAudioUses(): Promise<UiAudioUse[]>;
  getAndroidMicrophoneSettings(): Promise<UiAndroidMicrophoneSettings | null>;
  selectAndroidMicrophoneProfile(profile: UiAndroidMicrophoneSettings['captureProfile'], expectedRevision: number): Promise<UiAndroidMicrophoneSettings>;
  startAudioUse(peerNodeId: string, direction: AudioUseDirection): Promise<void>;
  stopAudioUse(direction: AudioUseDirection): Promise<void>;
  getControllerPeers(): Promise<UiControllerPeer[]>;
  getControllerUse(): Promise<UiControllerUse | null>;
  startControllerUse(peerNodeId: string, mode: ControllerUseMode): Promise<void>;
  stopControllerUse(): Promise<void>;
  getGamepadMotionSettings(): Promise<{ enabled: boolean; available: boolean; independentActive: boolean } | null>;
  setGamepadMotionEnabled(enabled: boolean): Promise<void>;
  setAndroidControllerMode(mode: 'touch' | 'motion' | 'stopped'): Promise<void>;
  getRemoteControllerSettings(nodeId: string): Promise<{nodeId: string; revision: number; motionEnabled: boolean; hapticsStrength: number; pending: boolean; error?: string | null} | null>;
  setRemoteControllerSettings(nodeId: string, expectedRevision: number, motionEnabled: boolean, hapticsStrength: number): Promise<void>;
  getHapticsSettings(): Promise<{ strength: number; available: boolean; executedFrames?: number; networkActive?: boolean } | null>;
  setHapticsStrength(percent: number): Promise<void>;
  setGamepadImmersive(active: boolean): Promise<void>;
  getGamepadLayout(): Promise<string | null>;
  saveGamepadLayout(layout: string): Promise<void>;
  getResourceRequests(): Promise<UiResourceRequest[]>;
  resolveResourceRequest(routeId: string, allow: boolean, remember?: boolean): Promise<void>;
  clearRememberedResourcePermissions?(): Promise<void>;
  getPairedPeers(): Promise<UiPairedPeer[]>;
  getEnrollmentStatus(): Promise<UiEnrollmentStatus>;
  getEnrollmentAddresses(): Promise<string[]>;
  enrollmentAction(request: UiEnrollmentRequest): Promise<UiEnrollmentStatus>;
  checkPairedCredential(nodeId: string): Promise<void>;
  getDiscoveryCandidates(): Promise<UiDiscoveryCandidate[]>;
  getDiscoveryInterfaces(): Promise<UiDiscoveryInterface[]>;
  connectDiscoveredPeer(nodeId: string): Promise<void>;
  connectPairedPeerAt(nodeId: string, host: string, port: number): Promise<void>;
  getControlConnectionFailure(): Promise<string | null>;
  startPairedListener(nodeId: string, bindHost: string, port: number): Promise<number>;
  stopPairedListener(): Promise<void>;
  getPairedListenerStatus(): Promise<{nodeId: string; bindHost: string; port: number} | null>;
  disconnectPeer(nodeId: string): Promise<void>;
  createPairingInvitation(nodeId: string, displayName: string, renewExisting?: boolean): Promise<string>;
  importPairingInvitation(): Promise<void>;
  getDisplayPeers(): Promise<UiDisplayPeer[]>;
  getDisplayUse(): Promise<UiDisplayUse | null>;
  startDisplayUse(peerNodeId: string, selection: UiDisplaySelection): Promise<void>;
  stopDisplayUse(): Promise<void>;
  getDisplaySenderState(): Promise<UiDisplaySenderState | null>;
  startDisplaySender(): Promise<UiDisplaySenderState | null>;
  stopDisplaySender(): Promise<UiDisplaySenderState | null>;
  getDisplayReceiverState(): Promise<UiDisplayReceiverState | null>;
  importDisplayProvisioning(): Promise<void>;
  openDisplayReceiver(options?: UiDisplayViewingOptions): Promise<void>;
  getDiagnostics(): Promise<UiDiagnostics>;
  getSnapshot(): Promise<UiSnapshot>;
  startDiscovery(interfaceIpv4: string): Promise<UiSnapshot>;
  stopDiscovery(): Promise<UiSnapshot>;
  setRoute(routeId: string, active: boolean): Promise<UiSnapshot>;
  setLocalCapabilityShared(capabilityId: string, selected: boolean): Promise<UiSnapshot>;
  getBackgroundWaitingEnabled(): Promise<boolean>;
  setBackgroundWaitingEnabled(enabled: boolean): Promise<void>;
  setRemoteCapabilityUse(capabilityId: string, active: boolean): Promise<UiSnapshot>;
  resetDemo(): Promise<UiSnapshot>;
  getLiveImu(): Promise<UiLiveImu | null>;
  startLiveImu(ip: string, port: number): Promise<UiLiveImu>;
  stopLiveImu(): Promise<UiLiveImu>;
  getGamepadState(): Promise<UiGamepadState | null>;
  refreshWindowsGamepadPreflight(controllerKind: WindowsControllerKind): Promise<UiGamepadState>;
  startWindowsGamepadProjection(controllerKind: WindowsControllerKind): Promise<UiGamepadState>;
  startControllerHost(controllerKind: WindowsControllerKind, port: number): Promise<UiGamepadState>;
  stopControllerHost(): Promise<UiGamepadState>;
  stopWindowsGamepadProjection(): Promise<UiGamepadState>;
  updateGamepadState(update: GamepadControlUpdate): Promise<UiGamepadState>;
  getTouchpadState(): Promise<UiTouchpadState | null>;
  updateTouchpadState(update: TouchpadControlUpdate): Promise<UiTouchpadState>;
  startGamepadDsu(port: number, mode: DsuProjectionMode): Promise<UiGamepadState>;
  stopGamepadDsu(): Promise<UiGamepadState>;
  startAndroidGamepad(port: number): Promise<UiGamepadState>;
  stopAndroidGamepad(): Promise<UiGamepadState>;
  configureAndroidController(host: string, port: number, token: string): Promise<UiGamepadState>;
  getQuickActions(): Promise<UiQuickAction[]>;
  invokeQuickAction(actionId: string, operation: QuickActionOperation): Promise<UiQuickAction>;
  getAudioEndpoints(): Promise<UiAudioEndpointCatalog | null>;
  getAndroidAudioState(): Promise<UiAndroidAudioState | null>;
  configureAndroidAudioPeer(direction: "microphone" | "speaker", host: string, localPort: number, peerPort: number, trustedLabConfirmed: boolean): Promise<void>;
  getAndroidAudioPeerSettings(direction: "microphone" | "speaker"): Promise<UiAndroidAudioPeerSettings | null>;
  getAndroidCameraSettings(): Promise<UiAndroidCameraSettings | null>;
  refreshAndroidCameraSources(): Promise<UiAndroidCameraSettings>;
  selectAndroidCameraSource(sourceKey: string): Promise<UiAndroidCameraSettings>;
  selectAndroidCameraQuality(quality: UiAndroidCameraSettings["quality"]): Promise<UiAndroidCameraSettings>;
  selectAndroidCameraProfile(profile: UiAndroidCameraSettings["captureProfile"]): Promise<UiAndroidCameraSettings>;
  selectAudioEndpoint(actionId: string, selectionToken: string): Promise<UiQuickAction>;
}

export interface UiDiscoveryInterface {
  ipv4: string;
  interfaceIndex: number;
}

/** Non-secret pairing metadata, not proof of a live authenticated connection. */
export interface UiPairedPeer {
  nodeId: string;
  displayName: string;
  revoked: boolean;
  /** Invitation issuance time; absent on older hosts, never a credential. */
  pairedAtMs?: number;
}

export interface UiResourceRequest {
  capabilityId?: string;
  resourceKind?: "touchpad" | "haptics" | "gamepad" | "microphone" | "speaker" | "camera" | "display";
  routeId: string;
  peerNodeId: string;
  peerName: string;
  capabilityName: string;
  expiresAtMs: number;
}

export interface UiDiscoveryCandidate {
  nodeId: string;
  host: string;
  controlPort: number;
  lastSeenAgeMs: number;
  pairingState: "unpaired" | "paired" | "revoked";
}
