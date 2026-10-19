/** Presentation only: never grants trust or changes the connection preference. */
export function automaticConnectionStatus(state: {
  enabled: boolean;
  listening: boolean;
  connected: boolean;
  paired: boolean;
  hasEndpoint: boolean;
  hasInterface: boolean;
}): string {
  if (!state.enabled) return "自动连接已关闭；重启后需要手动连接。现有连接不会因此断开。";
  if (state.listening) return "正在等待对端主动连接；本机暂不自动拨号，避免双端抢占连接。";
  if (state.connected) return "已建立认证连接；自动连接保持启用，硬件仍需单独申请使用。";
  if (!state.paired) return "尚无有效配对设备；自动发现不会自动信任陌生节点。";
  if (state.hasEndpoint) return "将尝试上次认证成功的地址；不依赖组播发现，仍校验配对证书。";
  if (state.hasInterface) return "等待发现已配对设备；若网络不支持发现，请手动成功连接一次以保存地址。";
  return "请先选择本机发现网络，或手动成功连接一次；目前没有可用于自动连接的地址。";
}
