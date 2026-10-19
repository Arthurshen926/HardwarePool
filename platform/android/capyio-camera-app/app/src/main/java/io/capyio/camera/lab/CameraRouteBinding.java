package io.capyio.camera.lab;

import android.os.SystemClock;
import io.capyio.camera.contract.AvcWireRecordEncoder;
import java.nio.ByteBuffer;
import java.util.UUID;

/** Immutable media identity supplied only by the authorized Node lifecycle. */
record CameraRouteBinding(UUID streamId, long epoch, long expiresAtMs, long monotonicDeadline) {
    CameraRouteBinding(String stream, long epoch, long expiry) {
        this(UUID.fromString(stream), epoch, expiry,
                SystemClock.elapsedRealtime() + Math.max(0, expiry-System.currentTimeMillis()));
        if (epoch <= 0 || epoch > Integer.MAX_VALUE || !isLive())
            throw new IllegalArgumentException("camera Route is expired or invalid");
    }
    boolean isLive() {
        return System.currentTimeMillis()<expiresAtMs && SystemClock.elapsedRealtime()<monotonicDeadline;
    }
    AvcWireRecordEncoder.StreamKey streamKey() {
        return new AvcWireRecordEncoder.StreamKey(ByteBuffer.allocate(16)
                .putLong(streamId.getMostSignificantBits()).putLong(streamId.getLeastSignificantBits()).array(),epoch);
    }
}
