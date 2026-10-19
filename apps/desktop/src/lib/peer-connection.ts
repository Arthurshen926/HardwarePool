import type { UiPairedPeer } from "./types";

const uuid = /^[0-9a-f]{8}(-[0-9a-f]{4}){3}-[0-9a-f]{12}$/;

/** These failures require user action, not another identical automatic dial. */
export function requiresConnectionAttention(error: unknown): boolean {
  const detail = error instanceof Error ? error.message : String(error);
  return /invalid peer certificate|BadSignature|certificate.*mismatch|DPAPI|credential could not be loaded|pairing directory refresh failed|\bunpaired\b|\brevoked\b/i.test(detail);
}
export function parsePairedPeers(value: unknown): UiPairedPeer[] {
  if (!Array.isArray(value) || value.length > 64) throw new Error("已配对设备列表无效。");
  const seen = new Set<string>();
  return value.map(item => {
    const fields = item ? Object.keys(item).sort().join() : "";
    const hasPairingTime = fields === "displayName,nodeId,pairedAtMs,revoked";
    if (!item || (!hasPairingTime && fields !== "displayName,nodeId,revoked")
      || typeof item.nodeId !== "string" || !uuid.test(item.nodeId)
      || item.nodeId === "00000000-0000-0000-0000-000000000000" || seen.has(item.nodeId)
      || typeof item.displayName !== "string" || !item.displayName.trim() || item.displayName.length > 128
      || /[\u0000-\u001f\u007f]/.test(item.displayName) || typeof item.revoked !== "boolean"
      || (hasPairingTime && (!Number.isSafeInteger(item.pairedAtMs)
        || item.pairedAtMs < 0 || item.pairedAtMs > 8_640_000_000_000_000))) {
      throw new Error("已配对设备信息无效。");
    }
    seen.add(item.nodeId);
    return { nodeId: item.nodeId, displayName: item.displayName, revoked: item.revoked,
      ...(hasPairingTime ? { pairedAtMs: item.pairedAtMs } : {}) };
  });
}

export function controlConnectionError(error: unknown): string {
  const detail = error instanceof Error ? error.message : String(error);
  const hint = /DPAPI|credential could not be loaded/.test(detail)
    ? "无法读取当前 Windows 用户的配对凭据；请使用完成配对的同一用户启动软件。"
    : /unpaired|revoked/i.test(detail)
      ? "设备尚未完成配对或已被吊销。首次添加请使用「连接一台新设备」，任一端显示 PIN，双方确认信任；旧版设备可使用文件邀请。已有配对冲突需先处理，重复输入 PIN 不会覆盖旧证书。"
      : /already has an authenticated/.test(detail)
        ? "该设备已经连接，请在远程资源页查看。"
        : /numeric IP|endpoint is invalid/.test(detail)
          ? "请填写设备 IP 和 CapyIO 设置页的本机控制端口，不要填写无线调试端口。"
          : /invalid peer certificate|BadSignature|certificate.*mismatch/i.test(detail)
            ? "已到达 TLS 服务，但证书与当前窗口使用的配对记录不一致。先确认平板 CapyIO 的本机控制端口（不是 ADB 端口），再对照两端显示的配对时间。可查看下方“Windows 配对身份诊断”；账号和路径相同也不能排除不同进程读取了不同文件。若批次不同，请从你正常启动的 Windows 窗口更新配对并在平板导入同一邀请；不要复制 Windows 私钥。不会自动信任或覆盖证书。"
          : /pairing directory refresh failed/.test(detail)
            ? "无法重新读取本机配对记录，已停止连接，未使用旧凭据重试。"
          : /connection refused|actively refused|os error 10061|拒绝连接/i.test(detail)
            ? "目标端口拒绝连接。请打开对方 CapyIO，确认其本机控制端口（不是 ADB 端口）正在监听；若地址或端口已变化，请更新后重试。这通常不是配对错误，无需先重新配对。"
          : /timed out|timeout|os error 10060|连接超时/i.test(detail)
            ? "连接超时，尚不能确定是否已到达对方 CapyIO。请确认双方网络在线；使用 Tailscale 时检查两端 Tailscale 均已连接，再核对地址、本机控制端口及防火墙。先排查网络，不要仅凭超时更新配对。"
          : /authenticated control connection failed|catalog receive failed/.test(detail)
            ? "连接未完成。请确认平板 CapyIO 正在运行，并核对其本机控制端口（不是 ADB 端口）；检查两端网络及防火墙。若证书不匹配，请核对配对设备。"
            : "连接未完成，请根据下方原因检查。";
  return `${hint}\n技术原因：${detail}`;
}
