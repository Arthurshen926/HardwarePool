import type {
  BindingState,
  HardwarePoolApi,
  UiCapability,
  UiEvent,
  UiSnapshot,
} from "./types";

const MICROPHONE_ID = "00000000-0000-4000-8000-000000000101";
const SPEAKER_ID = "00000000-0000-4000-8000-000000000102";

function capability(
  id: string,
  displayName: string,
  kind: "audio_capture" | "audio_render",
): UiCapability {
  const capture = kind === "audio_capture";
  return {
    id,
    displayName,
    kind,
    profile: capture
      ? "hardwarepool.audio.capture/1"
      : "hardwarepool.audio.render/1",
    permissionRequirement: capture ? "foreground_service" : "user_confirmation",
    availability: capture ? "permission_required" : "available",
    projectionKind: capture
      ? "system_capture_endpoint"
      : "system_render_endpoint",
    bindingState: "not_mapped",
    active: false,
    formatSummary: capture
      ? "48 kHz · PCM i16 LE · Mono · 10 ms"
      : "48 kHz · PCM i16 LE · Stereo · 10 ms",
    qosModes: capture
      ? ["voice_interactive", "raw_lan"]
      : ["media_playback", "voice_interactive"],
    metrics: {
      estimatedLatencyMs: null,
      packetLossPercent: null,
      bufferFillMs: null,
      underruns: 0,
      overruns: 0,
      simulated: true,
    },
  };
}

function initialSnapshot(): UiSnapshot {
  return {
    backendMode: "browser_mock",
    schemaVersion: 1,
    projectVersion: "0.1.0-bootstrap",
    localNodeName: "HP OmniBook Ultra Flip 14",
    peers: [
      {
        id: "00000000-0000-4000-8000-000000000002",
        displayName: "vivo X200 Pro mini",
        platform: "android",
        platformVersion: "Build not inventoried",
        online: true,
      },
    ],
    capabilities: [
      capability(MICROPHONE_ID, "Internal Microphone", "audio_capture"),
      capability(SPEAKER_ID, "Internal Speaker", "audio_render"),
    ],
    events: [
      { sequence: 1, summary: "Registered vivo X200 Pro mini" },
      { sequence: 2, summary: "Opened deterministic demo session" },
    ],
    warnings: [
      "Browser Mock 模式：状态与指标为确定性演示数据，不代表真实音频或设备访问。",
      "Windows 虚拟音频驱动、Android 音频 Adapter 和网络传输尚未实现。",
    ],
  };
}

export class BrowserMockHardwarePoolApi implements HardwarePoolApi {
  private snapshot = initialSnapshot();
  private nextSequence = 3;

  async getSnapshot(): Promise<UiSnapshot> {
    return structuredClone(this.snapshot);
  }

  async setProjection(
    capabilityId: string,
    active: boolean,
  ): Promise<UiSnapshot> {
    const target = this.snapshot.capabilities.find(
      (item) => item.id === capabilityId,
    );
    if (!target) {
      throw new Error(`Unknown capability: ${capabilityId}`);
    }

    const state: BindingState = active ? "active" : "stopped";
    target.active = active;
    target.bindingState = state;
    target.metrics = active
      ? {
          estimatedLatencyMs:
            target.kind === "audio_capture" ? 47.3 : 63.8,
          packetLossPercent: target.kind === "audio_capture" ? 0.03 : 0.01,
          bufferFillMs: target.kind === "audio_capture" ? 30 : 45,
          underruns: 0,
          overruns: 0,
          simulated: true,
        }
      : {
          estimatedLatencyMs: null,
          packetLossPercent: null,
          bufferFillMs: null,
          underruns: 0,
          overruns: 0,
          simulated: true,
        };

    this.pushEvent(
      `${target.displayName}: projection ${active ? "started" : "stopped"}`,
    );
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
    this.snapshot.events = this.snapshot.events.slice(-12);
  }
}
