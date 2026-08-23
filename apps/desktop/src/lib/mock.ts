import type {
  CapyIOApi,
  RouteState,
  UiEvent,
  UiPort,
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
    return structuredClone(this.snapshot);
  }

  private pushEvent(summary: string): void {
    const event: UiEvent = { sequence: this.nextSequence, summary };
    this.nextSequence += 1;
    this.snapshot.events.push(event);
    this.snapshot.events = this.snapshot.events.slice(-20);
  }
}
