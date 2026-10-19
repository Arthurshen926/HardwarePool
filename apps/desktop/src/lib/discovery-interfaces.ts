import type { UiDiscoveryInterface } from "./types";

export function parseDiscoveryInterfaces(value: unknown): UiDiscoveryInterface[] {
  if (!Array.isArray(value) || value.length > 64) throw new Error("网卡列表无效，请刷新后重试。");
  const seen = new Set<string>();
  return value.map(item => {
    if (!item || typeof item !== "object" || Object.keys(item).sort().join() !== "interfaceIndex,ipv4"
      || !Number.isInteger(item.interfaceIndex) || item.interfaceIndex < 1 || item.interfaceIndex > 0xffffffff
      || typeof item.ipv4 !== "string" || item.ipv4.length > 15) throw new Error("网卡信息无效。");
    const parts = item.ipv4.split(".");
    if (parts.length !== 4 || parts.some((part: string) => !/^(0|[1-9][0-9]{0,2})$/.test(part) || Number(part) > 255)
      || Number(parts[0]) === 0 || Number(parts[0]) === 127 || Number(parts[0]) >= 224) throw new Error("网卡地址无效。");
    const key = `${item.interfaceIndex}:${item.ipv4}`;
    if (seen.has(key)) throw new Error("网卡列表包含重复记录。");
    seen.add(key);
    return { ipv4: item.ipv4, interfaceIndex: item.interfaceIndex };
  });
}

/** Keep a user's choice while present; do not guess between multiple networks. */
export function selectDiscoveryInterface(items: UiDiscoveryInterface[], current: string): string {
  if (items.some(item => item.ipv4 === current)) return current;
  const addresses = [...new Set(items.map(item => item.ipv4))];
  return addresses.length === 1 ? addresses[0]! : "";
}
