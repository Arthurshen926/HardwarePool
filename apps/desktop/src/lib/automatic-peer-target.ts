import type { UiDiscoveryCandidate, UiPairedPeer } from './types';
import type { TrustedEndpointHint } from './trusted-endpoints';

/** Fair scheduling across discovery and remembered addresses; never creates trust. */
export function selectAutomaticPeerTarget(
  candidates: UiDiscoveryCandidate[], hints: TrustedEndpointHint[], peers: UiPairedPeer[],
  connected: ReadonlySet<string>, suppressed: ReadonlySet<string>,
  attempts: ReadonlyMap<string, number>, nowMs: number,
): { nodeId: string; candidate?: UiDiscoveryCandidate; cached?: TrustedEndpointHint } | null {
  const active = new Set(peers.filter(peer => !peer.revoked).map(peer => peer.nodeId));
  const targets = new Map<string, { nodeId: string; candidate?: UiDiscoveryCandidate; cached?: TrustedEndpointHint }>();
  for (const candidate of candidates) {
    if (candidate.pairingState === 'paired' && active.has(candidate.nodeId)) {
      targets.set(candidate.nodeId, { nodeId: candidate.nodeId, candidate });
    }
  }
  for (const cached of hints) {
    if (active.has(cached.nodeId)) targets.set(cached.nodeId, { ...targets.get(cached.nodeId), nodeId: cached.nodeId, cached });
  }
  return [...targets.values()]
    .filter(target => !connected.has(target.nodeId) && !suppressed.has(target.nodeId)
      && (!attempts.has(target.nodeId) || nowMs - attempts.get(target.nodeId)! >= 10_000))
    .sort((a, b) => (attempts.get(a.nodeId) ?? -Infinity) - (attempts.get(b.nodeId) ?? -Infinity))[0] ?? null;
}
