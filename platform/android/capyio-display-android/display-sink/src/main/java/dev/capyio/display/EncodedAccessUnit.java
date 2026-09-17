package dev.capyio.display;

import java.nio.BufferOverflowException;
import java.nio.ByteBuffer;
import java.util.Arrays;
import java.util.Objects;
import java.util.UUID;

/** One complete bounded AVC Annex-B access unit. */
public final class EncodedAccessUnit {
    public static final int SCHEMA_VERSION = 1;
    private static final byte[] ANNEX_B_START_CODE = {0, 0, 0, 1};

    private final UUID streamId;
    private final long streamEpoch;
    private final long sequence;
    private final long presentationTimeMicros;
    private final long durationNanos;
    private final boolean keyFrame;
    private final boolean discontinuity;
    private final boolean endOfStream;
    private final byte[] payload;

    public EncodedAccessUnit(
            UUID streamId,
            long streamEpoch,
            long sequence,
            long presentationTimeMicros,
            long durationNanos,
            boolean keyFrame,
            boolean discontinuity,
            boolean endOfStream,
            byte[] payload,
            int selectedMaximumBytes) {
        this.streamId = Objects.requireNonNull(streamId, "streamId");
        this.streamEpoch = streamEpoch;
        this.sequence = sequence;
        this.presentationTimeMicros = presentationTimeMicros;
        this.durationNanos = durationNanos;
        this.keyFrame = keyFrame;
        this.discontinuity = discontinuity;
        this.endOfStream = endOfStream;
        this.payload = Arrays.copyOf(Objects.requireNonNull(payload, "payload"), payload.length);
        validate(selectedMaximumBytes);
    }

    private void validate(int selectedMaximumBytes) {
        if (streamEpoch <= 0 || sequence < 0 || presentationTimeMicros < 0 || durationNanos <= 0) {
            throw new IllegalArgumentException("access-unit metadata is outside its valid range");
        }
        if (selectedMaximumBytes <= 0
                || selectedMaximumBytes > AvcDecoderConfiguration.MAX_ACCESS_UNIT_BYTES) {
            throw new IllegalArgumentException("selected access-unit bound is invalid");
        }
        if (endOfStream) {
            if (payload.length != 0 || keyFrame || discontinuity) {
                throw new IllegalArgumentException("end-of-stream must be a distinct empty unit");
            }
            return;
        }
        if (payload.length <= ANNEX_B_START_CODE.length || payload.length > selectedMaximumBytes) {
            throw new IllegalArgumentException("access-unit payload is empty or oversized");
        }
        for (int index = 0; index < ANNEX_B_START_CODE.length; index++) {
            if (payload[index] != ANNEX_B_START_CODE[index]) {
                throw new IllegalArgumentException("access unit lacks the Annex-B start code");
            }
        }
    }

    public void copyPayloadTo(ByteBuffer destination) {
        Objects.requireNonNull(destination, "destination");
        if (destination.remaining() < payload.length) {
            throw new BufferOverflowException();
        }
        destination.put(payload);
    }

    public UUID streamId() {
        return streamId;
    }

    public long streamEpoch() {
        return streamEpoch;
    }

    public long sequence() {
        return sequence;
    }

    public long presentationTimeMicros() {
        return presentationTimeMicros;
    }

    public long durationNanos() {
        return durationNanos;
    }

    public boolean keyFrame() {
        return keyFrame;
    }

    public boolean discontinuity() {
        return discontinuity;
    }

    public boolean endOfStream() {
        return endOfStream;
    }

    public int payloadSize() {
        return payload.length;
    }
}
