import type { UiResourceRequest } from "./types";
const uuid = /^[0-9a-f]{8}(-[0-9a-f]{4}){3}-[0-9a-f]{12}$/;
export function parseResourceRequests(value: unknown): UiResourceRequest[] {
  if (!Array.isArray(value) || value.length > 16) throw new Error("资源授权列表无效");
  const seen = new Set<string>();
  return value.map(item => {
    const fields = item && Object.keys(item).sort().join();
    const scoped = fields === "capabilityId,capabilityName,expiresAtMs,peerName,peerNodeId,resourceKind,routeId";
    if (!item || (!scoped && fields !== "capabilityName,expiresAtMs,peerName,peerNodeId,routeId")
      || (scoped && (typeof item.capabilityId !== "string" || !uuid.test(item.capabilityId)
        || item.capabilityId === "00000000-0000-0000-0000-000000000000"
        || !["touchpad", "haptics", "gamepad", "microphone", "speaker", "camera", "display"].includes(item.resourceKind)))
      || ![item.routeId, item.peerNodeId].every(id => typeof id === "string" && uuid.test(id) && id !== "00000000-0000-0000-0000-000000000000")
      || ![item.peerName, item.capabilityName].every(name => typeof name === "string" && name.length > 0 && name.length <= 256)
      || !Number.isSafeInteger(item.expiresAtMs) || item.expiresAtMs <= 0 || seen.has(item.routeId)) {
      throw new Error("资源授权请求无效");
    }
    seen.add(item.routeId);
    return { routeId: item.routeId, peerNodeId: item.peerNodeId, peerName: item.peerName,
      capabilityName: item.capabilityName, expiresAtMs: item.expiresAtMs,
      ...(scoped ? { capabilityId: item.capabilityId, resourceKind: item.resourceKind } : {}) };
  });
}
