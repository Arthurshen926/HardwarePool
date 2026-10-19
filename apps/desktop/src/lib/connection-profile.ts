export interface LocalConnectionProfile {
  schemaVersion: 1;
  preferredPeerIpv4: string;
}

const STORAGE_KEY = "capyio.connection-profile.v1";

export function isPrivateOrOverlayIpv4(host: string): boolean {
  if (host.length > 15) return false;
  const parts = host.split(".");
  if (parts.length !== 4 || parts.some(part => !/^(0|[1-9][0-9]{0,2})$/.test(part) || Number(part) > 255)) return false;
  const [a, b] = parts.map(Number);
  return a === 10 || (a === 172 && b >= 16 && b <= 31) || (a === 192 && b === 168) || (a === 100 && b >= 64 && b <= 127);
}

export function parseConnectionProfile(encoded: string): LocalConnectionProfile {
  const invalid = () => { throw new Error("连接配置无效，请重新保存可信网络 IPv4。"); };
  if (!encoded || encoded.length > 128) return invalid();
  let value: unknown;
  try { value = JSON.parse(encoded); } catch { return invalid(); }
  if (!value || typeof value !== "object" || Array.isArray(value)
    || Object.keys(value).sort().join() !== "preferredPeerIpv4,schemaVersion") return invalid();
  const profile = value as Record<string, unknown>;
  if (profile.schemaVersion !== 1 || typeof profile.preferredPeerIpv4 !== "string"
    || !isPrivateOrOverlayIpv4(profile.preferredPeerIpv4)) return invalid();
  return value as LocalConnectionProfile;
}

export function readConnectionProfile(storage: Pick<Storage, "getItem">): LocalConnectionProfile | null {
  const encoded = storage.getItem(STORAGE_KEY);
  return encoded === null ? null : parseConnectionProfile(encoded);
}

export function saveConnectionProfile(storage: Pick<Storage, "setItem">, host: string): LocalConnectionProfile {
  const preferredPeerIpv4 = host.trim();
  if (!isPrivateOrOverlayIpv4(preferredPeerIpv4)) throw new Error("请输入规范的私有或覆盖网络 IPv4 地址。");
  const profile: LocalConnectionProfile = { schemaVersion: 1, preferredPeerIpv4 };
  storage.setItem(STORAGE_KEY, JSON.stringify(profile));
  return profile;
}

export function clearConnectionProfile(storage: Pick<Storage, "removeItem">): void {
  storage.removeItem(STORAGE_KEY);
}
