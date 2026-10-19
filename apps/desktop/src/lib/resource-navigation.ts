import type { UiCapability, UiNode, UiQuickAction } from "./types";

export type HomeView = "local" | "remote" | "settings";
export type ResourceView = HomeView | "quick" | "capability" | "workspace" | "controller" | "touchpad" | "display" | "imu" | "audio-settings";

export interface ResourceLocation { view: ResourceView; actionId: string | null }
export interface ResourceNavigation { current: ResourceLocation; home: HomeView; history: ResourceLocation[] }

export function createResourceNavigation(view: ResourceView): ResourceNavigation {
  const home = isHomeView(view) ? view : view === "workspace" || view === "imu" ? "settings" : view === "touchpad" ? "local" : "remote";
  return { current: { view, actionId: null }, home, history: [] };
}

export function moveResourceNavigation(state: ResourceNavigation, view: ResourceView, actionId: string | null = state.current.actionId): ResourceNavigation {
  if (isHomeView(view)) return { current: { view, actionId: null }, home: view, history: [] };
  if (view === state.current.view && actionId === state.current.actionId) return state;
  return { current: { view, actionId }, home: state.home, history: [...state.history, state.current].slice(-8) };
}

export function backResourceNavigation(state: ResourceNavigation): ResourceNavigation {
  const previous = state.history.at(-1);
  return previous ? { current: previous, home: state.home, history: state.history.slice(0, -1) }
    : createResourceNavigation(state.home);
}

export function isHomeView(value: unknown): value is HomeView {
  return value === "local" || value === "remote" || value === "settings";
}

export function initialResourceView(requested: string | null, saved: string | null): ResourceView {
  if (isHomeView(requested)) return requested;
  // Android's existing launcher URL is an alias, so a saved home can take precedence.
  if (requested === "workspace" || requested === "controller" || requested === "touchpad" || requested === "display" || requested === "imu") return requested;
  if (isHomeView(saved)) return saved;
  return requested === "quick" ? "local" : "remote";
}

export function resourceActions(actions: UiQuickAction[], view: HomeView): UiQuickAction[] {
  if (view === "settings") return [];
  return actions.filter((action) => {
    // Compatibility actions remain in Workspace; keep a running one reachable for Stop.
    if (['capyio.quick-action.remote-speaker', 'capyio.quick-action.remote-microphone'].includes(action.id)
      && ['idle', 'stopped', 'blocked', 'failed'].includes(action.status)) return false;
    // Keep an already running diagnostic reachable for Stop; ordinary use belongs
    // to the system-camera action. The full action remains in the diagnostic view.
    if (action.id === 'capyio.quick-action.android-camera-receiver'
      && ['idle', 'stopped', 'blocked'].includes(action.status)) return false;
    if (actions.some(item => item.id === 'capyio.quick-action.android-gamepad' && !item.simulated)
      && ['capyio.quick-action.android-imu', 'capyio.quick-action.android-haptics'].includes(action.id)) return false;
    const remote = action.kind === "route_control" || [
      "capyio.quick-action.android-system-camera",
      "capyio.quick-action.android-camera-receiver",
    ].includes(action.id);
    return !action.simulated && (view === "remote" ? remote : !remote);
  });
}

// Presentation grouping only: underlying capabilities, grants and routes remain intact.
export function primaryHardwareResources(capabilities: UiCapability[]): UiCapability[] {
  const gamepadNodes = new Set(capabilities.filter(c => c.capabilityClass === 'gamepad' && !c.simulated).map(c => c.nodeId));
  return capabilities.filter(c => !gamepadNodes.has(c.nodeId) || !(
    (c.capabilityClass === 'imu' && c.ports.some(p => p.profile === 'capyio.motion.imu-samples/1'))
    || (c.capabilityClass === 'haptics' && c.ports.some(p => p.direction === 'sink' && p.profile === 'capyio.haptics.feedback/1'))));
}

export function peerResourceGroups(nodes: UiNode[], capabilities: UiCapability[]) {
  return nodes.filter((node) => !node.local).map((node) => ({
    node,
    capabilities: capabilities.filter((capability) => !capability.local && !capability.simulated && capability.nodeId === node.id),
  })).filter((group) => group.capabilities.length > 0);
}

// Compatibility presentation binding for the existing Android schema-4 catalog.
// Exact host-authored keys only: class/name similarity never authorizes an action.
const androidResources = [
  ["microphone", "麦克风", "采集本机声音，供电脑使用"],
  ["speaker", "扬声器", "在本机播放电脑发送的声音"],
  ["camera", "摄像头", "选择摄像头和画质，向电脑提供视频"],
  ["imu", "运动传感器（独立 IMU）", "单独共享加速度和陀螺仪；不需要打开触控手柄"],
  ["gamepad", "手柄与体感", "触控手柄、仅体感模式和游戏振动设置"],
  ["touchpad", "触控板", "将本机触摸屏作为多点触控板"],
  ["haptics", "振动反馈", "管理本机振动并执行短时测试"],
] as const;

export function localResourceBindings(localNodeId: string | null, capabilities: UiCapability[], actions: UiQuickAction[]): Record<string, UiQuickAction> {
  const bindings: Record<string, UiQuickAction> = {};
  if (!localNodeId) return bindings;
  for (const [key] of androidResources) {
    const matchingCapabilities = capabilities.filter(capability => capability.local && !capability.simulated
      && capability.inventorySource === "android_node_catalog" && capability.nodeId === localNodeId
      && capability.id === `${localNodeId}:android.${key}` && capability.capabilityClass === key);
    const matchingActions = actions.filter(action => action.id === `capyio.quick-action.android-${key}`
      && action.kind === "capability_control" && !action.simulated);
    if (matchingCapabilities.length === 1 && matchingActions.length === 1) bindings[matchingCapabilities[0]!.id] = matchingActions[0]!;
  }
  return bindings;
}

export function actionPresentation(action: UiQuickAction): { title: string; summary: string } {
  if (action.id === 'capyio.quick-action.android-camera-receiver') {
    return { title: '摄像头传输诊断（不提供系统摄像头）', summary: '仅接收和解码手机视频，用于排障；Windows 相机、会议软件请使用“手机作为系统摄像头”。不要同时启动两个入口。' };
  }
  const entry = androidResources.find(([key]) => action.id === `capyio.quick-action.android-${key}`);
  return entry ? { title: entry[1], summary: entry[2] } : { title: action.title, summary: action.summary };
}
