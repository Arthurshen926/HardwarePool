package io.capyio.camera.contract;

import java.nio.ByteBuffer;
import java.util.Arrays;

/** One bounded, owned MediaCodec output buffer on a private Adapter data plane. */
public final class EncodedAvcAccessUnit {
    public static final int MAX_PAYLOAD_BYTES = 4 * 1024 * 1024;

    private final long sequence;
    private final long presentationTimeUs;
    private final boolean codecConfig;
    private final boolean keyFrame;
    private final boolean endOfStream;
    private final byte[] payload;

    public EncodedAvcAccessUnit(
            long sequence,
            long presentationTimeUs,
            boolean codecConfig,
            boolean keyFrame,
            boolean endOfStream,
            byte[] payload) {
        if (sequence <= 0) {
            throw new IllegalArgumentException("access-unit sequence must be positive");
        }
        if (presentationTimeUs < 0) {
            throw new IllegalArgumentException("access-unit presentation time must be non-negative");
        }
        if (payload == null || payload.length > MAX_PAYLOAD_BYTES) {
            throw new IllegalArgumentException("access-unit payload exceeds the bootstrap bound");
        }
        if (!endOfStream && payload.length == 0) {
            throw new IllegalArgumentException("non-terminal access unit must carry bytes");
        }
        if (codecConfig && endOfStream) {
            throw new IllegalArgumentException("codec-config and end-of-stream flags cannot be combined");
        }
        this.sequence = sequence;
        this.presentationTimeUs = presentationTimeUs;
        this.codecConfig = codecConfig;
        this.keyFrame = keyFrame;
        this.endOfStream = endOfStream;
        this.payload = Arrays.copyOf(payload, payload.length);
    }

    public long sequence() {
        return sequence;
    }

    public long presentationTimeUs() {
        return presentationTimeUs;
    }

    public boolean codecConfig() {
        return codecConfig;
    }

    public boolean keyFrame() {
        return keyFrame;
    }

    public boolean endOfStream() {
        return endOfStream;
    }

    public int payloadLength() {
        return payload.length;
    }

    /** Returns a read-only view whose backing array is owned by this object. */
    public ByteBuffer payloadView() {
        return ByteBuffer.wrap(payload).asReadOnlyBuffer();
    }
}
