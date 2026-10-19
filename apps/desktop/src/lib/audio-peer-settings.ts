import type { UiAndroidAudioPeerSettings } from "./types";

function isPrivateOrOverlayIpv4(host: string): boolean {
  if (host.length > 15) return false;
  const parts = host.split(".");
  if (parts.length !== 4 || parts.some(part => !/^(0|[1-9][0-9]{0,2})$/.test(part) || Number(part) > 255)) return false;
  const [a, b] = parts.map(Number);
  return a === 10 || (a === 172 && b >= 16 && b <= 31) || (a === 192 && b === 168) || (a === 100 && b >= 64 && b <= 127);
}

export function parseAudioPeerSettings(encoded: string, direction: "microphone" | "speaker"): UiAndroidAudioPeerSettings {
  const invalid = () => { throw new Error("音频连接配置读取失败：宿主返回的数据不兼容。"); };
  if (!encoded || encoded.length > 512) return invalid();
  let value;
  try { value = JSON.parse(encoded); } catch { return invalid(); }
  if (!value || value.schemaVersion !== 1 || value.direction !== direction
    || Object.keys(value).sort().join() !== "direction,savedPeer,schemaVersion") return invalid();
  if (value.savedPeer !== null) {
    const peer = value.savedPeer;
    if (!peer || Object.keys(peer).sort().join() !== "host,localPort,peerPort"
      || typeof peer.host !== "string" || peer.host.length > 15) return invalid();
    if (!isPrivateOrOverlayIpv4(peer.host)) return invalid();
    if ([peer.localPort, peer.peerPort].some(port => !Number.isInteger(port) || port < 1 || port > 65535)) return invalid();
  }
  return value as UiAndroidAudioPeerSettings;
}
