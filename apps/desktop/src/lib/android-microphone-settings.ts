import type { UiAndroidMicrophoneSettings } from './types';

/** Closed metadata only: no endpoint, credential, grant, session or media field. */
export function parseAndroidMicrophoneSettings(value: unknown): UiAndroidMicrophoneSettings {
  const invalid = (): never => { throw new Error('Android microphone bridge returned incompatible data'); };
  if (!value || typeof value !== 'object' || Array.isArray(value)) return invalid();
  const dto = value as Record<string, unknown>;
  if (Object.keys(dto).sort().join() !== 'captureProfile,eligibility,permissionReady,revision,schemaVersion,source'
    || dto.schemaVersion !== 1 || typeof dto.revision !== 'number' || !Number.isSafeInteger(dto.revision) || dto.revision < 0
    || !['legacy', 'secure_mono_tls13_v1'].includes(dto.captureProfile as string)
    || !['not_requested', 'eligible', 'unavailable'].includes(dto.eligibility as string)
    || (dto.captureProfile === 'legacy') !== (dto.eligibility === 'not_requested')
    || dto.source !== 'system_default' || typeof dto.permissionReady !== 'boolean') return invalid();
  return { schemaVersion: 1, revision: dto.revision,
    captureProfile: dto.captureProfile as UiAndroidMicrophoneSettings['captureProfile'],
    source: 'system_default', eligibility: dto.eligibility as UiAndroidMicrophoneSettings['eligibility'],
    permissionReady: dto.permissionReady };
}
