import type { UiAndroidCameraSettings, UiAndroidCameraSource } from "./types";

const required = ["schemaVersion", "state", "generation", "permissionReady", "selectedSourceKey",
  "quality", "captureProfile", "finitePreflight", "problemCode", "sources"];
const counters = ["capturedFrames", "encodedAccessUnits", "sentAccessUnits", "exportDroppedAccessUnits"];
const optional = ["transportStatus", ...counters];
const invalid = (): never => { throw new Error("Android camera bridge returned incompatible data"); };
const record = (value: unknown): Record<string, unknown> => {
  if (!value || typeof value !== "object" || Array.isArray(value)) return invalid();
  return value as Record<string, unknown>;
};
const member = (value: unknown, allowed: readonly string[]): value is string =>
  typeof value === "string" && allowed.includes(value);
const integer = (value: unknown, max = Number.MAX_SAFE_INTEGER): value is number =>
  typeof value === "number" && Number.isSafeInteger(value) && value >= 0 && value <= max;
const text = (value: unknown, max: number): value is string =>
  typeof value === "string" && value.length > 0 && value.length <= max && !/[\u0000-\u001f\u007f-\u009f]/u.test(value);

/** Closed, bounded bridge metadata only. It cannot convey a Route, grant or codec payload. */
export function parseAndroidCameraSettings(value: unknown): UiAndroidCameraSettings {
  const dto = record(value);
  if (required.some(key => !Object.hasOwn(dto, key))
    || Object.keys(dto).some(key => !required.includes(key) && !optional.includes(key))
    || dto.schemaVersion !== 1 || !member(dto.state, ["stopped", "starting", "active", "failed"])
    || !integer(dto.generation) || typeof dto.permissionReady !== "boolean"
    || !member(dto.quality, ["economy", "balanced", "clear"])
    || !member(dto.captureProfile, ["legacy", "finite_720p30_v1"])
    || !member(dto.finitePreflight, ["not_requested", "eligible", "unsupported"])
    || (dto.problemCode !== null && (!text(dto.problemCode, 96) || !/^[A-Z0-9._]+$/.test(dto.problemCode)))
    || (dto.selectedSourceKey !== null && !text(dto.selectedSourceKey, 96))
    || (Object.hasOwn(dto, "transportStatus") && !text(dto.transportStatus, 128))
    || counters.some(key => Object.hasOwn(dto, key) && !integer(dto[key]))
    || !Array.isArray(dto.sources) || dto.sources.length > 128) return invalid();
  const seen = new Set<string>();
  const sources: UiAndroidCameraSource[] = dto.sources.map(value => {
    const source = record(value);
    if (Object.keys(source).sort().join() !== "cameraId,facing,key,zoomRatioMilli"
      || !text(source.key, 96) || !text(source.cameraId, 64)
      || !member(source.facing, ["front", "back", "external", "unknown"])
      || (source.zoomRatioMilli !== null && (!integer(source.zoomRatioMilli, 1_000_000) || source.zoomRatioMilli === 0))
      || seen.has(source.key)) return invalid();
    const key = source.zoomRatioMilli === null ? `${source.cameraId}@auto`
      : `${source.cameraId}@${((source.zoomRatioMilli as number) / 1000).toFixed(3)}x`;
    if (source.key !== key) return invalid();
    seen.add(source.key);
    return { key, cameraId: source.cameraId, facing: source.facing as UiAndroidCameraSource["facing"],
      zoomRatioMilli: source.zoomRatioMilli as number | null };
  });
  if (dto.selectedSourceKey !== null && !seen.has(dto.selectedSourceKey as string)) return invalid();
  if ((dto.captureProfile === "legacy") !== (dto.finitePreflight === "not_requested")
    || (dto.finitePreflight === "eligible" && (dto.selectedSourceKey === null || dto.quality !== "balanced"))) return invalid();
  // Copy only admitted fields rather than returning an unchecked bridge object.
  const result: UiAndroidCameraSettings = {
    schemaVersion: 1, state: dto.state as UiAndroidCameraSettings["state"], generation: dto.generation,
    permissionReady: dto.permissionReady, selectedSourceKey: dto.selectedSourceKey as string | null,
    quality: dto.quality as UiAndroidCameraSettings["quality"],
    captureProfile: dto.captureProfile as UiAndroidCameraSettings["captureProfile"],
    finitePreflight: dto.finitePreflight as UiAndroidCameraSettings["finitePreflight"],
    problemCode: dto.problemCode as string | null, sources,
  };
  if (Object.hasOwn(dto, "transportStatus")) result.transportStatus = dto.transportStatus as string;
  for (const key of counters as (keyof Pick<UiAndroidCameraSettings,
    "capturedFrames" | "encodedAccessUnits" | "sentAccessUnits" | "exportDroppedAccessUnits">)[]) {
    if (Object.hasOwn(dto, key)) result[key] = dto[key] as number;
  }
  return result;
}
