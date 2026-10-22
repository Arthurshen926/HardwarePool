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
  { capabilityClass: "camera", profile: "capyio.video.frames/1", direction: "source" },
] as const;

// Exact descriptor allowlist from remote_resource_contract.rs / native_audio_route.rs. This shared
// desktop action owns only legacy UDP audio; the Linux secure launcher is separate.
export function hasLegacyRemoteAudioBinding(capability: UiCapability): boolean {
  if (capability.local || capability.simulated) return false;
  const expected = capability.capabilityClass === 'microphone'
    ? { direction: 'source', format: 'pcm-s16le-48000-mono' }
    : capability.capabilityClass === 'speaker'
      ? { direction: 'sink', format: 'pcm-s16le-48000-stereo' } : null;
  if (!expected) return false;
  const ports = capability.ports.filter(port => port.direction === expected.direction
    && port.profile === 'capyio.audio.frames/1');
  // Do not silently choose UDP out of an unknown or mixed secure/legacy catalog.
  return ports.length > 0 && ports.every(port => port.formats?.length === 1
    && port.formats[0] === expected.format);
}

export function hasIntegratedRemoteDataBinding(capability: UiCapability): boolean {
  if (['microphone', 'speaker'].includes(capability.capabilityClass)) return hasLegacyRemoteAudioBinding(capability);
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
  if (['microphone', 'speaker'].includes(capability.capabilityClass)
    && !hasLegacyRemoteAudioBinding(capability)
    && capability.ports.some(port => port.profile === 'capyio.audio.frames/1')) {
    const ports = capability.ports.filter(port => port.profile === 'capyio.audio.frames/1');
    const format = ports[0]?.formats?.[0];
    const secureMaximum = format === 'pcm-s16le-48000-mono-tls13-v1' ? '最多 60 秒'
      : format === 'pcm-s16le-48000-mono-tls13-300s-v1' ? '最多 5 分钟（300 秒）' : null;
    if (capability.capabilityClass === 'microphone' && secureMaximum && ports.every(port =>
      port.direction === 'source' && port.formats?.length === 1 && port.formats[0] === format)) return {
      label: capability.availability === 'offline' ? '安全音频当前离线' : '安全音频需匹配接收端',
      // A catalog format is a requirement, not evidence of an active TLS session.
      detail: `此目录声明实验性单声道配置，${secureMaximum}，要求独立的固定 TLS 1.3 通道。须从支持同一精确配置的 Linux 有限会话入口申请，原始授权或租约可能更短；不支持 Windows 一键使用，不会退回 UDP。目录不表示已经授权、加密传输或播放；合成验证不保证真机兼容。`,
    };
    return { label: '音频配置不受支持', detail: '此音频目录缺少当前入口支持的精确配置，或混用了不同配置；不能推断时长、安全传输或自动退回 Legacy。' };
  }
  if (hasIntegratedRemoteDataBinding(capability)) {
    if (capability.capabilityClass === 'camera') return {
      label: capability.availability === 'offline' ? '相机当前离线' : '相机可申请',
      detail: '申请后自动准备两端；系统相机权限仍由设备确认。Windows 应用中选择 CapyIO 虚拟相机并检查实际画面。',
    };
    if (['microphone', 'speaker'].includes(capability.capabilityClass)) return {
      label: capability.availability === 'offline' ? '音频设备当前离线' : '音频可申请',
      detail: capability.capabilityClass === 'microphone' ? '此入口使用可信实验网络 UDP 明文音频。申请后自动准备两端麦克风连接；首次仍需设备授权及系统录音权限。' : '此入口使用可信实验网络 UDP 明文音频。申请后自动准备两端扬声器连接；在 Windows 应用中选择 CapyIO Speaker 输出。',
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
