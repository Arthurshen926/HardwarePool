import type { UiCapability } from './types';

export function controllerResourceMode(capability: UiCapability | null): 'dualshock4' | 'xbox360' | 'dsu' | undefined {
  if (!capability || capability.local || capability.simulated || !capability.ports.some(p =>
    p.direction === 'sink' && p.profile === 'capyio.input.gamepad-state/1')) return undefined;
  const modes: Record<string, 'dualshock4' | 'xbox360' | 'dsu'> = {
    'd41251b3-63ed-4e83-807e-4c4ab9b20c21': 'dualshock4',
    'd41251b3-63ed-4e83-807e-4c4ab9b20c22': 'xbox360',
    'd41251b3-63ed-4e83-807e-4c4ab9b20c23': 'dsu',
  };
  return modes[capability.id];
}

// Exact contracts already admitted by the Android native workflow, not name matching.
export function androidResourceEntry(capability: UiCapability | null): 'microphone' | 'speaker' | 'camera' | null {
  if (!capability || capability.local || capability.simulated) return null;
  const bindings = [
    ['d41251b3-63ed-4e83-807e-4c4ab9b20d21', 'sink', 'capyio.audio.frames/1', 'microphone'],
    ['d41251b3-63ed-4e83-807e-4c4ab9b20d22', 'source', 'capyio.audio.frames/1', 'speaker'],
    ['d41251b3-63ed-4e83-807e-4c4ab9b20e21', 'sink', 'capyio.video.frames/1', 'camera'],
  ] as const;
  return bindings.find(([id, direction, profile]) => capability.id === id
    && capability.ports.some(p => p.direction === direction && p.profile === profile))?.[3] ?? null;
}
