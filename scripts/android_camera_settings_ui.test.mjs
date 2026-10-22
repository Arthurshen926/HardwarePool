import { test } from 'node:test';
import assert from 'node:assert/strict';
import { parseAndroidCameraSettings } from '../apps/desktop/src/lib/android-camera-settings.ts';

const source = { key: '0@auto', cameraId: '0', facing: 'back', zoomRatioMilli: null };
const base = { schemaVersion: 1, state: 'stopped', generation: 0, permissionReady: true,
  selectedSourceKey: null, quality: 'balanced', captureProfile: 'legacy',
  finitePreflight: 'not_requested', problemCode: null, sources: [source] };

test('legacy remains explicit and finite metadata eligibility is distinct from verified output', () => {
  assert.deepEqual(parseAndroidCameraSettings(base), base);
  const eligible = { ...base, selectedSourceKey: source.key, captureProfile: 'finite_720p30_v1', finitePreflight: 'eligible' };
  assert.deepEqual(parseAndroidCameraSettings(eligible), eligible);
  assert.deepEqual(parseAndroidCameraSettings({ ...eligible, finitePreflight: 'unsupported', selectedSourceKey: null }),
    { ...eligible, finitePreflight: 'unsupported', selectedSourceKey: null });
  assert.notEqual(parseAndroidCameraSettings(eligible).finitePreflight, 'verified');
});

test('closed settings reject unknown profiles, wire config, stale selection and incompatible eligibility', () => {
  for (const value of [null, [], {}, { ...base, schemaVersion: 2 }, { ...base, secureConfig: {} },
    { ...base, captureProfile: 'finite' }, { ...base, finitePreflight: 'verified' },
    { ...base, finitePreflight: 'eligible' }, { ...base, captureProfile: 'finite_720p30_v1' },
    { ...base, selectedSourceKey: 'missing@auto' }, { ...base, generation: -1 },
    { ...base, generation: 1.5 }, { ...base, generation: Number.MAX_SAFE_INTEGER + 1 },
    { ...base, state: 'streaming' }, { ...base, permissionReady: 'true' },
    { ...base, problemCode: 'secret payload' }, { ...base, capturedFrames: NaN },
    { ...base, transportStatus: '' }, { ...base, captureProfile: 'finite_720p30_v1', finitePreflight: 'eligible' },
    { ...base, captureProfile: 'finite_720p30_v1', finitePreflight: 'eligible', selectedSourceKey: source.key, quality: 'clear' }]) {
    assert.throws(() => parseAndroidCameraSettings(value));
  }
  for (const key of Object.keys(base)) {
    const missing = { ...base }; delete missing[key];
    assert.throws(() => parseAndroidCameraSettings(missing));
  }
});

test('source inventory and optional counters are bounded and reconstructed', () => {
  for (const sources of [[null], [source, source], Array(129).fill(source),
    [{ ...source, secret: 'x' }], [{ ...source, key: 'bad' }], [{ ...source, cameraId: '' }],
    [{ ...source, zoomRatioMilli: 0 }], [{ ...source, zoomRatioMilli: 1_000_001 }],
    [{ ...source, facing: 'rear' }], [{ ...source, cameraId: '0\n' }]]) {
    assert.throws(() => parseAndroidCameraSettings({ ...base, sources }));
  }
  const zoom = { ...source, key: '0@1.000x', zoomRatioMilli: 1000 };
  const dto = { ...base, sources: [zoom], selectedSourceKey: zoom.key,
    transportStatus: 'pending', capturedFrames: 0, encodedAccessUnits: 1, sentAccessUnits: 0, exportDroppedAccessUnits: 0 };
  const parsed = parseAndroidCameraSettings(dto);
  assert.deepEqual(parsed, dto);
  assert.notEqual(parsed, dto);
  assert.notEqual(parsed.sources, dto.sources);
  assert.notEqual(parsed.sources[0], dto.sources[0]);
});
