import type { UiPairedPeer } from "./types";

/** Routing hints only. The native control dial still selects and verifies paired credentials. */
export interface TrustedEndpointHint {
  nodeId: string;
  host: string;
  port: number;
}

const NODE_ID = /^[0-9a-f]{8}(-[0-9a-f]{4}){3}-[0-9a-f]{12}$/;
const NIL_NODE_ID = "00000000-0000-0000-0000-000000000000";

function canonicalIp(value: unknown): value is string {
  if (typeof value !== "string" || value.length > 45 || !/^[0-9a-f:.]+$/.test(value)) return false;
  if (!value.includes(":") && !/^\d+\.\d+\.\d+\.\d+$/.test(value)) return false;
  const host = value.includes(":") ? `[${value}]` : value;
  try { return new URL(`http://${host}/`).hostname === host; }
  catch { return false; }
}

function validHint(value: unknown): value is TrustedEndpointHint {
  if (!value || typeof value !== "object" || Object.keys(value).sort().join() !== "host,nodeId,port") return false;
  const item = value as Record<string, unknown>;
  return typeof item.nodeId === "string" && NODE_ID.test(item.nodeId) && item.nodeId !== NIL_NODE_ID
    && canonicalIp(item.host) && Number.isInteger(item.port) && (item.port as number) >= 1 && (item.port as number) <= 65535;
}

export function parseTrustedEndpointHints(raw: string | null): TrustedEndpointHint[] {
  if (raw === null) return [];
  if (raw.length > 8192) throw new Error("已保存的设备地址列表过大");
  const value: unknown = JSON.parse(raw);
  if (!value || typeof value !== "object" || Object.keys(value).sort().join() !== "endpoints,version") throw new Error("已保存的设备地址格式无效");
  const record = value as Record<string, unknown>;
  if (record.version !== 1 || !Array.isArray(record.endpoints) || record.endpoints.length > 64) throw new Error("已保存的设备地址版本无效");
  const seen = new Set<string>();
  return record.endpoints.map(item => {
    if (!validHint(item) || seen.has(item.nodeId)) throw new Error("已保存的设备地址内容无效");
    seen.add(item.nodeId);
    return { nodeId: item.nodeId, host: item.host, port: item.port };
  });
}

export function encodeTrustedEndpointHints(hints: TrustedEndpointHint[]): string {
  if (hints.length > 64 || new Set(hints.map(item => item.nodeId)).size !== hints.length || hints.some(item => !validHint(item))) {
    throw new Error("不能保存无效的设备地址");
  }
  return JSON.stringify({ version: 1, endpoints: hints });
}

export function rememberAuthenticatedEndpoint(hints: TrustedEndpointHint[], next: TrustedEndpointHint): TrustedEndpointHint[] {
  if (!validHint(next)) throw new Error("认证连接的地址无效");
  return [...hints.filter(item => item.nodeId !== next.nodeId), next].slice(-64);
}

export function retainActivelyPairedEndpoints(hints: TrustedEndpointHint[], peers: UiPairedPeer[]): TrustedEndpointHint[] {
  const active = new Set(peers.filter(peer => !peer.revoked).map(peer => peer.nodeId));
  return hints.filter(item => active.has(item.nodeId));
}

export function selectAutomaticTrustedEndpoint(
  hints: TrustedEndpointHint[], peers: UiPairedPeer[], connected: ReadonlySet<string>,
  suppressed: ReadonlySet<string>, attempts: ReadonlyMap<string, number>, nowMs: number, retryAfterMs = 10_000,
): TrustedEndpointHint | null {
  const active = new Set(peers.filter(peer => !peer.revoked).map(peer => peer.nodeId));
  return hints.find(item => active.has(item.nodeId) && !connected.has(item.nodeId)
    && !suppressed.has(item.nodeId) && nowMs - (attempts.get(item.nodeId) ?? 0) >= retryAfterMs) ?? null;
}
