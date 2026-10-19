import type { UiCapability, UiRoute } from "./types";

export function knownRouteUsage(routes: Pick<UiRoute, 'state'>[], leaseCount: number): string {
  const labels: Record<string, string> = { draft: '申请中', prepared: '已准备', starting: '启动中／等待媒体', active: '使用中', stopping: '停止中', stopped: '已停止', offline: '离线', failed: '失败' };
  const counts = new Map<string, number>();
  for (const route of routes) counts.set(route.state, (counts.get(route.state) ?? 0) + 1);
  const states = [...counts].map(([state, count]) => `${count} 条${labels[state] ?? state} Route`).join('；');
  return `${states || '没有已知 Route'}；${leaseCount ? `${leaseCount} 个所有者权威租约` : '没有可见租约'}；不代表全局空闲`;
}

// Presentation of data bindings implemented by this build, not peer authority.
// Add another binding only when its host negotiates and owns a real data worker.
const integratedRemoteBindings = [
  { capabilityClass: "touchpad", profile: "capyio.input.touchpad-frames/1", direction: "source" },
  { capabilityClass: "haptics", profile: "capyio.haptics.feedback/1", direction: "sink" },
  { capabilityClass: "microphone", profile: "capyio.audio.frames/1", direction: "source" },
  { capabilityClass: "speaker", profile: "capyio.audio.frames/1", direction: "sink" },
  { capabilityClass: "camera", profile: "capyio.video.frames/1", direction: "source" },
] as const;

export function hasIntegratedRemoteDataBinding(capability: UiCapability): boolean {
  return !capability.local && !capability.simulated && integratedRemoteBindings.some(binding =>
    capability.capabilityClass === binding.capabilityClass
    && capability.ports.some(port => port.direction === binding.direction && port.profile === binding.profile));
}

export function remoteResourceUseHint(capability: UiCapability): { label: string; detail: string } | null {
  if (capability.local) return null;
  if (capability.simulated) return { label: "演示资源", detail: "演示目录不代表可使用的真实硬件。" };
  if (capability.capabilityClass === 'display' && capability.ports.some(port => port.formats.includes('gamestream-h264-sdr-v1'))) return {
    label: '通过显示共享使用',
    detail: '打开显示共享，选择节点和画质后开始；沿用节点信任，首次仍需资源授权。任一端停止会清理扩展屏。',
  };
  if (capability.capabilityClass === 'gamepad') return {
    label: '通过手柄与体感使用',
    detail: '打开手柄与体感，选择已认证节点及 DS4、Xbox 或仅体感模式，一键申请和停止；此卡片不直接启动手柄。',
  };
  if (hasIntegratedRemoteDataBinding(capability)) {
    if (capability.capabilityClass === 'camera') return {
      label: capability.availability === 'offline' ? '相机当前离线' : '相机可申请',
      detail: '申请后自动准备两端；系统相机权限仍由设备确认。Windows 应用中选择 CapyIO 虚拟相机并检查实际画面。',
    };
    if (['microphone', 'speaker'].includes(capability.capabilityClass)) return {
      label: capability.availability === 'offline' ? '音频设备当前离线' : '音频可申请',
      detail: capability.capabilityClass === 'microphone' ? '申请后自动准备两端麦克风连接；首次仍需设备授权及系统录音权限。' : '申请后自动准备两端扬声器连接；在 Windows 应用中选择 CapyIO Speaker 输出。',
    };
    if (capability.capabilityClass === "haptics") return {
      label: capability.availability === "offline" ? "振动器当前离线" : "游戏振动可申请",
      detail: "先启动 DS4 或 Xbox，再申请平板振动反馈；首次需要平板单独允许。",
    };
    return {
      label: capability.availability === "offline" ? "触控板当前离线" : "触控板可申请",
      detail: "进入资源详情申请使用；对端允许且本机接收器就绪后才能开始控制。",
    };
  }
  return {
    label: "需通过实验功能入口配置",
    detail: "此资源尚未接入统一连接的一键使用。已有兼容功能可从相应实验入口配置；查看目录不会启动硬件。",
  };
}
