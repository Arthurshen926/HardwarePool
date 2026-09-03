package io.capyio.camera.contract;

import java.nio.ByteBuffer;
import java.util.Arrays;

/** Bounded AVC codec-specific data reported by MediaCodec output format. */
public final class AvcCodecConfig {
    public static final int MAX_PARAMETER_SET_BYTES = 64 * 1024;

    private final byte[] csd0;
    private final byte[] csd1;

    public AvcCodecConfig(byte[] csd0, byte[] csd1) {
        if (csd0 == null || csd0.length == 0 || csd0.length > MAX_PARAMETER_SET_BYTES) {
            throw new IllegalArgumentException("AVC csd-0 is missing or oversized");
        }
        if (csd1 == null || csd1.length > MAX_PARAMETER_SET_BYTES) {
            throw new IllegalArgumentException("AVC csd-1 is oversized");
        }
        this.csd0 = Arrays.copyOf(csd0, csd0.length);
        this.csd1 = Arrays.copyOf(csd1, csd1.length);
    }

    public ByteBuffer csd0View() {
        return ByteBuffer.wrap(csd0).asReadOnlyBuffer();
    }

    public ByteBuffer csd1View() {
        return ByteBuffer.wrap(csd1).asReadOnlyBuffer();
    }
}
