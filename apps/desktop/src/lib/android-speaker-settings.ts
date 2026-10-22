import type { UiAndroidSpeakerSettings } from './types';

/** Closed metadata only; transport, endpoints and authority cannot be supplied by this DTO. */
export function parseAndroidSpeakerSettings(value: unknown): UiAndroidSpeakerSettings {
  const invalid = (): never => { throw new Error('Android speaker bridge returned incompatible data'); };
  if (!value || typeof value !== 'object' || Array.isArray(value)) return invalid();
  const dto = value as Record<string, unknown>;
  if (Object.keys(dto).sort().join() !== 'eligibility,permissionReady,playbackProfile,revision,schemaVersion,source'
    || dto.schemaVersion !== 1 || typeof dto.revision !== 'string'
    || !/^(0|[1-9][0-9]{0,15})$/.test(dto.revision) || BigInt(dto.revision) > 9007199254740991n
    || !['legacy', 'secure_stereo_tls13_v1'].includes(dto.playbackProfile as string)
    || !['not_requested', 'eligible', 'unavailable'].includes(dto.eligibility as string)
    || (dto.playbackProfile === 'legacy') !== (dto.eligibility === 'not_requested')
    || dto.source !== 'system_default' || typeof dto.permissionReady !== 'boolean') return invalid();
  return { schemaVersion: 1, revision: dto.revision,
    playbackProfile: dto.playbackProfile as UiAndroidSpeakerSettings['playbackProfile'],
    source: 'system_default', eligibility: dto.eligibility as UiAndroidSpeakerSettings['eligibility'],
    permissionReady: dto.permissionReady };
}
