package dev.capyio.android.lan;

import java.util.UUID;

/**
 * Direction-neutral, finite control-to-lab configuration value. This is not an
 * authorization token or a network wire format. The caller must supply current
 * authenticated Route/Session state, owner consent and leases before starting a
 * worker, and stop it on expiry, revocation or disconnect. These factories only
 * support the existing 48 kHz S16LE / 10 ms mono and stereo lab baselines; they
 * do not negotiate, convert, authenticate or encrypt media.
 */
public final class NativeLanRouteBinding {
    public static final long MAX_SAFE_TIMESTAMP_MS = 9_007_199_254_740_991L;

    public final NativeLanPacketCodec.Binding binding;
    public final long expiresAtMs;

    public NativeLanRouteBinding(
            UUID sessionId, UUID routeId, UUID streamId,
            long streamEpoch, long expiresAtMs, long nowMs) {
        if (sessionId == null || routeId == null || streamId == null) {
            throw new IllegalArgumentException("route binding IDs must be present");
        }
        if (streamEpoch < 1 || streamEpoch > Integer.MAX_VALUE) {
            throw new IllegalArgumentException("route epoch is outside the supported Java range");
        }
        binding = new NativeLanPacketCodec.Binding(sessionId, routeId, streamId, (int) streamEpoch);
        if (expiresAtMs < 1 || expiresAtMs > MAX_SAFE_TIMESTAMP_MS) {
            throw new IllegalArgumentException("route expiry must be a finite safe timestamp");
        }
        this.expiresAtMs = expiresAtMs;
        requireUnexpired(nowMs);
    }

    /** Timestamp validation only, not a replacement for current Runtime authorization. */
    public void requireUnexpired(long nowMs) {
        if (nowMs < 0 || nowMs > MAX_SAFE_TIMESTAMP_MS || nowMs >= expiresAtMs) {
            throw new IllegalArgumentException("route binding is expired or the clock is invalid");
        }
    }

    /** Peer address must come from the authenticated control owner, never discovery alone. */
    public NativeLanMicrophoneSessionConfig microphoneBaselineConfig(
            String authenticatedPeerIpv4, int localPort, int peerPort, long nowMs) {
        requireUnexpired(nowMs);
        return new NativeLanMicrophoneSessionConfig(
                authenticatedPeerIpv4, localPort, peerPort,
                binding.sessionId, binding.routeId, binding.streamId, binding.streamEpoch);
    }

    /** Peer address must come from the authenticated control owner, never discovery alone. */
    public NativeLanSpeakerSessionConfig speakerBaselineConfig(
            String authenticatedPeerIpv4, int localPort, int peerPort, long nowMs) {
        requireUnexpired(nowMs);
        return new NativeLanSpeakerSessionConfig(
                authenticatedPeerIpv4, localPort, peerPort,
                binding.sessionId, binding.routeId, binding.streamId, binding.streamEpoch);
    }
}
