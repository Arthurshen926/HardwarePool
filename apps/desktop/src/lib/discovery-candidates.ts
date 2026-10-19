import type { UiDiscoveryCandidate } from "./types";

function canonicalIp(value: unknown): value is string {
  if (typeof value !== "string" || value.length > 45 || !/^[0-9a-f:.]+$/.test(value)) return false;
  if (!value.includes(":") && !/^\d+\.\d+\.\d+\.\d+$/.test(value)) return false;
  const host = value.includes(":") ? `[${value}]` : value;
  try { return new URL(`http://${host}/`).hostname === host; }
  catch { return false; }
}

export function parseDiscoveryCandidates(value: unknown): UiDiscoveryCandidate[] {
  if (!Array.isArray(value) || value.length > 64) throw new Error("发现设备列表无效，请刷新。");
  const seen = new Set<string>();
  return value.map(item => {
    if (!item || typeof item !== "object" || Object.keys(item).sort().join() !== "controlPort,host,lastSeenAgeMs,nodeId,pairingState"
      || typeof item.nodeId !== "string" || !/^[0-9a-f]{8}(-[0-9a-f]{4}){3}-[0-9a-f]{12}$/.test(item.nodeId)
      || item.nodeId === "00000000-0000-0000-0000-000000000000" || seen.has(item.nodeId)
      || !canonicalIp(item.host)
      || !Number.isInteger(item.controlPort) || item.controlPort < 1 || item.controlPort > 65535
      || !Number.isInteger(item.lastSeenAgeMs) || item.lastSeenAgeMs < 0 || item.lastSeenAgeMs > 15000
      || !["unpaired", "paired", "revoked"].includes(item.pairingState)) {
      throw new Error("发现设备信息无效。");
    }
    seen.add(item.nodeId);
    return { nodeId: item.nodeId, host: item.host, controlPort: item.controlPort, lastSeenAgeMs: item.lastSeenAgeMs, pairingState: item.pairingState } as UiDiscoveryCandidate;
  });
}

export function selectAutomaticPairedCandidate(
  candidates: UiDiscoveryCandidate[],
  connectedPeerIds: ReadonlySet<string>,
  suppressedPeerIds: ReadonlySet<string>,
  attemptAt: ReadonlyMap<string, number>,
  nowMs: number,
  retryAfterMs = 10_000,
): UiDiscoveryCandidate | null {
  return candidates.find(candidate => candidate.pairingState === "paired"
    && !connectedPeerIds.has(candidate.nodeId)
    && !suppressedPeerIds.has(candidate.nodeId)
    && nowMs - (attemptAt.get(candidate.nodeId) ?? 0) >= retryAfterMs) ?? null;
}
