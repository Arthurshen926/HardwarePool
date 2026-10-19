import type { UiDisplaySenderState } from "./types";

const states = ["unavailable", "stopped", "starting", "running", "stopping", "failed"];
export function parseDisplaySenderState(value: unknown): UiDisplaySenderState | null {
  if (value === null) return null;
  if (!value || typeof value !== "object") throw new Error("Invalid display sender state");
  const state = value as UiDisplaySenderState;
  if (state.schemaVersion !== 1 || typeof state.available !== "boolean"
    || !states.includes(state.state) || !Number.isSafeInteger(state.generation) || state.generation < 0
    || state.available === (state.state === "unavailable")
    || !(state.problemCode === null || (typeof state.problemCode === "string"
      && state.problemCode.length <= 128 && /^CAPY\.DISPLAY\.[A-Z_]+$/.test(state.problemCode)))) {
    throw new Error("Invalid display sender state");
  }
  return state;
}
export function displaySenderCanStart(state: UiDisplaySenderState | null): boolean {
  return !!state?.available && (state.state === "stopped" || state.state === "failed");
}
export function displaySenderCanStop(state: UiDisplaySenderState | null): boolean {
  return !!state?.available && (state.state === "starting" || state.state === "running");
}

export function displayProblemHint(code: string | null | undefined): string {
  switch (code) {
    case "CAPY.DISPLAY.DISPLAY_MODE_CHANGED":
      return "Windows 扩展屏的分辨率或刷新率已被修改，本次共享已中止。请等待桌面恢复，再在 CapyIO 选择画质并重新启动；当前不支持共享期间直接修改系统显示模式。";
    case "CAPY.DISPLAY.ENGINE_WORKER_FAILED":
      return "显示工作进程已退出。请保留错误码，并检查显示组件和扩展屏状态；此提示本身不能确定是否为远控软件冲突。";
    case "CAPY.DISPLAY.ENGINE_LOAD_FAILED":
    case "CAPY.DISPLAY.ENGINE_START_FAILED":
      return "显示引擎未能启动。请修复完整显示组件，包括 DLL、assets、include 和 tools 运行目录；只替换 EXE 可能导致黑屏，不需要重新配对。";
    case "CAPY.DISPLAY.MONITOR_CREATE_FAILED":
      return "无法创建 CapyIO 扩展屏。请检查虚拟显示组件及 Windows 显示设置，然后停止并重试。";
    case "CAPY.DISPLAY.OUTPUT_BIND_FAILED":
      return "Windows 未能绑定本次 CapyIO 扩展屏。若正在使用微软远程桌面（RDP），请停止共享，切回本地交互会话或非 RDP 远控后重试；否则检查虚拟显示组件及显示设置。此错误不代表节点配对失败。";
    case "CAPY.DISPLAY.STREAMING_FAILED":
      return "显示传输已中断。请确认扩展屏仍启用及两端网络正常，然后重新启动共享。";
    case "CAPY.DISPLAY.CONTROL_UNAVAILABLE":
      return "无法访问显示服务。请确认服务已运行，且允许控制的 Windows 账号与当前账号一致；不需要重新配对设备。";
    case "CAPY.DISPLAY.CONFIGURATION_INVALID":
      return "显示连接配置缺失或与当前组件不匹配，请修复显示安装配置后重试。";
    case "CAPY.DISPLAY.NATIVE_CLIENT_MISSING":
      return "此安装包缺少显示接收组件，请安装包含显示组件的完整版本。重新配对不能修复缺失组件。";
    default:
      return code ? "显示共享未完成。请停止后重试，并保留下面的错误码。" : "";
  }
}
