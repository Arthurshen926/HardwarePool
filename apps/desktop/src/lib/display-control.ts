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
