import type {
  CapyIOApi,
  RouteState,
  UiEvent,
  UiAudioEndpointCatalog,
  GamepadButton,
  GamepadControlUpdate,
  UiGamepadState,
  UiLiveImu,
  QuickActionOperation,
  UiPort,
  UiQuickAction,
  UiRoute,
  UiSnapshot,
} from "./types";

const WINDOWS = "HP OmniBook Ultra Flip 14";
const PHONE = "vivo X200 Pro mini";

function port(
  nodeName: string,
  capabilityName: string,
  capabilityClass: string,
  portName: string,
  direction: "source" | "sink",
): UiPort {
  return { nodeName, capabilityName, capabilityClass, portName, direction };
}

function route(
  id: string,
  source: UiPort,
  sink: UiPort,
  profile: string,
  formatSummary: string,
  backend: string,
  projectionNote: string,
): UiRoute {
  return {
    id,
    title: `${source.capabilityName} → ${sink.capabilityName}`,
    summary: `${source.nodeName} → ${sink.nodeName}`,
    profile,
    backend,
    state: "draft",
    active: false,
    source,
    sink,
    formatSummary,
    qosModes: [profile.includes("imu") ? "measurement" : profile.includes("video") ? "basic" : "interactive"],
    projectionNote,
    metrics: {
      estimatedLatencyMs: null,
      packetLossPercent: null,
      bufferFillMs: null,
      simulated: true,
    },
  };
}

function initialSnapshot(): UiSnapshot {
  return {
    backendMode: "browser_mock",
    schemaVersion: 3,
    projectVersion: "0.1.0",
    nodes: [
      { id: "00000000-0000-4000-8000-000000000001", displayName: WINDOWS, platform: "windows", platformVersion: "Windows fixture", online: true, local: true, capabilityCount: 6 },
      { id: "00000000-0000-4000-8000-000000000002", displayName: PHONE, platform: "android", platformVersion: "Android fixture", online: true, local: false, capabilityCount: 6 },
    ],
    routes: [
      route("00000000-0000-4000-8000-000000000911", port(PHONE, "Phone Microphone", "microphone", "Phone Microphone Source", "source"), port(WINDOWS, "Windows Virtual Microphone", "microphone", "Virtual Microphone Sink", "sink"), "capyio.audio.frames/1", "pcm-s16le-48000-mono", "capydataplane", "Windows 系统端点投影（驱动尚未实现）"),
      route("00000000-0000-4000-8000-000000000912", port(WINDOWS, "System Mix", "system_audio_capture", "System Mix Source", "source"), port(PHONE, "Phone Speaker", "speaker", "Phone Speaker Sink", "sink"), "capyio.audio.frames/1", "pcm-s16le-48000-stereo", "capydataplane", "Android 应用内播放端（模拟）"),
      route("00000000-0000-4000-8000-000000000913", port(PHONE, "Phone IMU", "imu", "IMU Sample Source", "source"), port(WINDOWS, "Windows Gamepad Projection", "gamepad", "Gamepad Projection Sink", "sink"), "capyio.motion.imu-samples/1", "imu-si-f32-le", "localpipeline", "本地游戏手柄投影（best-effort，模拟）"),
      route("00000000-0000-4000-8000-000000000914", port(PHONE, "Back Camera", "camera", "Back Camera Source", "source"), port(WINDOWS, "Camera Preview Panel", "panel", "Camera Preview Sink", "sink"), "capyio.video.frames/1", "bgra8-1280x720-30", "capydataplane", "CapyIO 应用内 Panel（模拟）"),
    ],
    adapters: [
      { id: "adapter-windows-audio", nodeName: WINDOWS, displayName: "Windows Audio Adapter", adapterType: "capyio.windows.audio", deploymentMode: "sidecar", state: "ready", health: "healthy", capabilityCount: 4 },
      { id: "adapter-windows-projection", nodeName: WINDOWS, displayName: "Windows Projection Adapter", adapterType: "capyio.windows.projection", deploymentMode: "sidecar", state: "ready", health: "healthy", capabilityCount: 2 },
      { id: "adapter-android-hardware", nodeName: PHONE, displayName: "Android Integrated Hardware Adapter", adapterType: "capyio.android.integrated-hardware", deploymentMode: "in_process", state: "ready", health: "healthy", capabilityCount: 6 },
    ],
    events: [
      { sequence: 1, summary: `Registered ${PHONE}` },
      { sequence: 2, summary: "Created four independent deterministic Routes" },
    ],
    warnings: [
      "Browser Mock：状态、授权和指标是确定性演示数据，不代表真实硬件访问。",
      "当前没有真实网络、Android 节点、Windows 虚拟设备或驱动。",
    ],
    imuFixture: {
      mode: "deterministic_fixture",
      simulated: true,
      profile: "capyio.motion.imu-samples/1",
      sequence: 5,
      sourceTimestampNanos: 1_050_000_000,
      clockDomainId: "android.sensor.elapsed_realtime",
      acceleration: { x: 0.1, y: 0.03, z: 9.75 },
      angularVelocity: { x: 0.003, y: 0.001, z: 0 },
      panelReceived: 6,
      panelMissingSequences: 0,
      recorderRecords: 6,
      panelRouteState: "active",
      recorderRouteState: "active",
    },
  };
}

export class BrowserMockCapyIOApi implements CapyIOApi {
  private snapshot = initialSnapshot();
  private nextSequence = 3;
  private gamepad = initialGamepadState();

  async getSnapshot(): Promise<UiSnapshot> {
    return structuredClone(this.snapshot);
  }

  async setRoute(routeId: string, active: boolean): Promise<UiSnapshot> {
    const target = this.snapshot.routes.find((item) => item.id === routeId);
    if (!target) throw new Error(`Unknown Route: ${routeId}`);

    const state: RouteState = active ? "active" : "stopped";
    target.active = active;
    target.state = state;
    const video = target.profile.includes("video");
    const motion = target.profile.includes("imu");
    target.metrics = active
      ? {
          estimatedLatencyMs: video ? 81.2 : motion ? 18.6 : 47.3,
          packetLossPercent: video ? 0.07 : motion ? 0.01 : 0.03,
          bufferFillMs: video ? 66 : motion ? 12 : 30,
          simulated: true,
        }
      : { estimatedLatencyMs: null, packetLossPercent: null, bufferFillMs: null, simulated: true };
    this.pushEvent(`${target.title}: Route ${active ? "started" : "stopped"}`);
    return structuredClone(this.snapshot);
  }

  async resetDemo(): Promise<UiSnapshot> {
    this.snapshot = initialSnapshot();
    this.nextSequence = 3;
    this.gamepad = initialGamepadState();
    return structuredClone(this.snapshot);
  }

  async getLiveImu(): Promise<UiLiveImu> {
    return unsupportedLiveImu();
  }

  async startLiveImu(): Promise<UiLiveImu> {
    throw new Error("Live IMU requires the Tauri desktop backend.");
  }

  async stopLiveImu(): Promise<UiLiveImu> {
    return unsupportedLiveImu();
  }

  async getGamepadState(): Promise<UiGamepadState> {
    return structuredClone(this.gamepad);
  }

  async refreshWindowsGamepadPreflight(controllerKind: import("./types").WindowsControllerKind): Promise<UiGamepadState> {
    this.gamepad.windowsProjection.controllerKind = controllerKind;
    this.gamepad.windowsProjection.deviceIdentity = controllerKind === "dualshock4"
      ? "DualShock 4 · 054c:09cc · native motion"
      : "Xbox 360 Controller · 045e:028e";
    return structuredClone(this.gamepad);
  }

  async startWindowsGamepadProjection(_enableXinputCompanion: boolean): Promise<UiGamepadState> {
    throw new Error("Windows DS4 projection requires the Tauri desktop backend.");
  }

  async stopWindowsGamepadProjection(): Promise<UiGamepadState> {
    this.gamepad.windowsProjection.status = "stopped";
    this.gamepad.windowsProjection.busId = null;
    this.gamepad.windowsProjection.ownedUsbipPort = null;
    return structuredClone(this.gamepad);
  }

  async updateGamepadState(update: GamepadControlUpdate): Promise<UiGamepadState> {
    const next = structuredClone(this.gamepad);
    switch (update.kind) {
      case "button": {
        const pressed = new Set<GamepadButton>(next.pressedButtons);
        if (update.pressed) pressed.add(update.button);
        else pressed.delete(update.button);
        next.pressedButtons = [...pressed];
        break;
      }
      case "dpad":
        if (!isDpadValue(update.x) || !isDpadValue(update.y)) throw new Error("D-pad axes must be -1, 0, or 1.");
        next.dpad = { x: update.x, y: update.y };
        break;
      case "stick":
        if (!isAxisValue(update.x) || !isAxisValue(update.y)) throw new Error("Stick axes must be -32767..=32767.");
        if (update.stick === "left") next.leftStick = { x: update.x, y: update.y };
        else next.rightStick = { x: update.x, y: update.y };
        break;
      case "trigger":
        if (!Number.isInteger(update.value) || update.value < 0 || update.value > 65535) throw new Error("Trigger must be 0..=65535.");
        if (update.trigger === "left") next.leftTrigger = update.value;
        else next.rightTrigger = update.value;
        break;
      case "reset":
        next.pressedButtons = [];
        next.dpad = { x: 0, y: 0 };
        next.leftStick = { x: 0, y: 0 };
        next.rightStick = { x: 0, y: 0 };
        next.leftTrigger = 0;
        next.rightTrigger = 0;
        break;
    }
    next.sequence = (next.sequence ?? -1) + 1;
    next.sourceTimestampNanos = Math.max(1, Math.round(performance.now() * 1_000_000));
    next.lastUpdate = update.kind === "reset" ? "reset.neutral" : "control.update";
    this.gamepad = next;
    return structuredClone(next);
  }

  async startGamepadDsu(_port: number, _mode: import("./types").DsuProjectionMode): Promise<UiGamepadState> {
    throw new Error("DSU loopback projection requires the Tauri desktop backend.");
  }

  async stopGamepadDsu(): Promise<UiGamepadState> {
    return structuredClone(this.gamepad);
  }

  async startAndroidGamepad(_port: number): Promise<UiGamepadState> {
    throw new Error("Android controller UDP input requires the Tauri desktop backend.");
  }

  async stopAndroidGamepad(): Promise<UiGamepadState> {
    return structuredClone(this.gamepad);
  }

  async getQuickActions(): Promise<UiQuickAction[]> {
    return [unsupportedAudioQuickAction()];
  }

  async invokeQuickAction(_actionId: string, _operation: QuickActionOperation): Promise<UiQuickAction> {
    throw new Error("Physical Quick Actions require the Tauri desktop backend.");
  }

  async getAudioEndpoints(): Promise<UiAudioEndpointCatalog> {
    return {
      schemaVersion: 1,
      actionId: "capyio.quick-action.remote-speaker",
      supported: false,
      canSelect: false,
      choices: [],
      problem: "Windows 播放端点枚举需要 Tauri 桌面后端。",
    };
  }

  async selectAudioEndpoint(_actionId: string, _selectionToken: string): Promise<UiQuickAction> {
    throw new Error("Windows playback endpoint selection requires the Tauri desktop backend.");
  }

  private pushEvent(summary: string): void {
    const event: UiEvent = { sequence: this.nextSequence, summary };
    this.nextSequence += 1;
    this.snapshot.events.push(event);
    this.snapshot.events = this.snapshot.events.slice(-20);
  }
}

function initialGamepadState(): UiGamepadState {
  return {
    schemaVersion: 1,
    source: "desktop_simulator",
    simulated: true,
    profile: "capyio.input.gamepad-state/1",
    streamEpoch: 1,
    sequence: null,
    sourceTimestampNanos: null,
    pressedButtons: [],
    dpad: { x: 0, y: 0 },
    leftStick: { x: 0, y: 0 },
    rightStick: { x: 0, y: 0 },
    leftTrigger: 0,
    rightTrigger: 0,
    lastUpdate: "neutral.initial",
    dsuProjection: {
      supported: false,
      status: "unsupported",
      endpoint: null,
      mode: "motion_only",
      lastSubmit: "not_started",
      controlsSubmitted: 0,
      controlsAccepted: 0,
      controlsQueueFull: 0,
      controlsNeutralResets: 0,
      activeSubscribers: 0,
      padPacketsSent: 0,
      packetSendErrors: 0,
    },
    windowsProjection: {
      supported: false,
      status: "unsupported",
      controllerKind: "dualshock4",
      deviceIdentity: "DualShock 4 · 054c:09cc · native motion",
      viiperEndpoint: null,
      usbipEndpoint: null,
      viiperReady: false,
      usbipReady: false,
      xinputAvailable: false,
      xinputReady: false,
      exportCount: 0,
      busId: null,
      ownedUsbipPort: null,
      inputPackets: 0,
      nonNeutralPackets: 0,
      ds4RejectedPackets: 0,
      xinputPackets: 0,
      inputOfflineEvents: 0,
      lastRemoteSequence: null,
      lastEvent: "browser_mock_has_no_platform_access",
      problemCode: null,
      problem: null,
    },
    androidInput: {
      supported: false,
      status: "unsupported",
      endpoint: null,
      lanHostHint: null,
      pairingToken: null,
      peerConnected: false,
      acceptedPackets: 0,
      rejectedPackets: 0,
      replayedPackets: 0,
      peerTimeouts: 0,
      projectionQueueFull: 0,
      packetAgeMillis: null,
      remoteSequence: null,
      lastEvent: "not_started",
    },
    motion: {
      source: "stationary_fixture",
      sourceTimestampNanos: null,
      acceleration: [0, 0, 9.80665],
      angularVelocity: [0, 0, 0],
    },
  };
}

function isDpadValue(value: number): boolean {
  return Number.isInteger(value) && value >= -1 && value <= 1;
}

function isAxisValue(value: number): boolean {
  return Number.isInteger(value) && value >= -32767 && value <= 32767;
}

function unsupportedAudioQuickAction(): UiQuickAction {
  return {
    schemaVersion: 1,
    id: "capyio.quick-action.remote-speaker",
    kind: "route_control",
    title: "将电脑声音镜像到手机",
    summary: "系统音频镜像 · 非虚拟扬声器 · Browser Mock 不启动外部进程",
    status: "blocked",
    simulated: true,
    routeId: null,
    routeState: null,
    routeEpoch: null,
    availableOperations: [],
    evidenceLevel: "not_started",
    problemCode: "CAPY.UI.BROWSER_MOCK",
    problem: "请使用配置了 Audio Share 的 Tauri 桌面宿主。",
  };
}

function unsupportedLiveImu(): UiLiveImu {
  return {
    status: "unsupported",
    simulated: true,
    routeId: "00000000-0000-4000-8000-000000000000",
    routeState: "draft",
    endpoint: null,
    profile: "capyio.motion.imu-samples/1",
    streamEpoch: 0,
    sequence: null,
    sourceTimestampNanos: null,
    clockDomainId: null,
    acceleration: null,
    angularVelocity: null,
    receivedSamples: 0,
    problemCode: null,
    problem: "Browser Mock 不会访问真实网络或传感器。",
  };
}
