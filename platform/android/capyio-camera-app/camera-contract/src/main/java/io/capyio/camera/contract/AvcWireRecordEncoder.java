package io.capyio.camera.contract;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.util.Arrays;
import java.util.Objects;

/** Version-1 private AdapterManaged AVC record encoder shared with the Rust receiver. */
public final class AvcWireRecordEncoder {
    public static final int HEADER_BYTES = 56;
    public static final int VERSION_MAJOR = 1;
    public static final int VERSION_MINOR = 1;

    private static final byte[] MAGIC = new byte[] {'C', 'A', 'V', 'C'};
    private static final int CONFIG_PAYLOAD_HEADER_BYTES = 28;
    private static final int KIND_CONFIG = 1;
    private static final int KIND_ACCESS_UNIT = 2;
    private static final int FLAG_KEY_FRAME = 0x01;
    private static final int FLAG_END_OF_STREAM = 0x02;
    private static final int FLAG_DISCONTINUITY = 0x04;

    private AvcWireRecordEncoder() {}

    public enum Layout {
        ANNEX_B(1),
        LENGTH_PREFIXED_4(2),
        AVC_DECODER_CONFIGURATION_RECORD(3);

        private final int wireValue;

        Layout(int wireValue) {
            this.wireValue = wireValue;
        }
    }

    /** Immutable stream binding. A new route/reconnect uses a new positive epoch. */
    public static final class StreamKey {
        private final byte[] streamId;
        private final long epoch;

        public StreamKey(byte[] streamId, long epoch) {
            if (streamId == null || streamId.length != 16) {
                throw new IllegalArgumentException("AVC stream ID must contain exactly 16 bytes");
            }
            boolean nonZero = false;
            for (byte value : streamId) {
                nonZero |= value != 0;
            }
            if (!nonZero) {
                throw new IllegalArgumentException("AVC stream ID must not be all zero");
            }
            if (epoch <= 0) {
                throw new IllegalArgumentException("AVC stream epoch must be positive");
            }
            this.streamId = Arrays.copyOf(streamId, streamId.length);
            this.epoch = epoch;
        }

        public byte[] streamIdCopy() {
            return Arrays.copyOf(streamId, streamId.length);
        }

        public long epoch() {
            return epoch;
        }
    }

    public static byte[] encodeConfig(
            StreamKey stream,
            AvcEncoderConfig encoderConfig,
            Layout accessUnitLayout,
            Layout codecSpecificLayout,
            AvcCodecConfig codecConfig) {
        Objects.requireNonNull(stream, "stream");
        Objects.requireNonNull(encoderConfig, "encoderConfig");
        Objects.requireNonNull(accessUnitLayout, "accessUnitLayout");
        Objects.requireNonNull(codecSpecificLayout, "codecSpecificLayout");
        Objects.requireNonNull(codecConfig, "codecConfig");
        if (accessUnitLayout == Layout.AVC_DECODER_CONFIGURATION_RECORD) {
            throw new IllegalArgumentException(
                    "AVC access units cannot use a decoder configuration record");
        }

        byte[] csd0 = copy(codecConfig.csd0View());
        byte[] csd1 = copy(codecConfig.csd1View());
        int payloadLength = Math.addExact(
                CONFIG_PAYLOAD_HEADER_BYTES,
                Math.addExact(csd0.length, csd1.length));
        ByteBuffer record = newRecord(
                KIND_CONFIG,
                0,
                stream,
                0,
                0,
                payloadLength);
        record.putShort((short) encoderConfig.width());
        record.putShort((short) encoderConfig.height());
        record.putShort((short) encoderConfig.framesPerSecond());
        record.putShort((short) 0);
        record.putInt(encoderConfig.bitrateBitsPerSecond());
        record.put((byte) accessUnitLayout.wireValue);
        record.put((byte) codecSpecificLayout.wireValue);
        record.put(new byte[] {1, 1, 1}); // limited-range BT.709 SDR
        record.put((byte) rotationCode(encoderConfig.clockwiseRotationDegrees()));
        record.put(new byte[] {0, 0});
        record.putInt(csd0.length);
        record.putInt(csd1.length);
        record.put(csd0);
        record.put(csd1);
        return record.array();
    }

    private static int rotationCode(int clockwiseRotationDegrees) {
        return switch (clockwiseRotationDegrees) {
            case 0 -> 0;
            case 90 -> 1;
            case 180 -> 2;
            case 270 -> 3;
            default -> throw new IllegalArgumentException("unsupported AVC display rotation");
        };
    }

    public static byte[] encodeAccessUnit(
            StreamKey stream,
            EncodedAvcAccessUnit unit,
            boolean discontinuity) {
        Objects.requireNonNull(stream, "stream");
        Objects.requireNonNull(unit, "unit");
        if (unit.codecConfig()) {
            throw new IllegalArgumentException(
                    "codec-config buffers must use the dedicated config record");
        }
        if (unit.endOfStream() && unit.keyFrame()) {
            throw new IllegalArgumentException("end-of-stream cannot be a key frame");
        }

        int flags = 0;
        if (unit.keyFrame()) {
            flags |= FLAG_KEY_FRAME;
        }
        if (unit.endOfStream()) {
            flags |= FLAG_END_OF_STREAM;
        }
        if (discontinuity) {
            flags |= FLAG_DISCONTINUITY;
        }
        byte[] payload = copy(unit.payloadView());
        ByteBuffer record = newRecord(
                KIND_ACCESS_UNIT,
                flags,
                stream,
                unit.sequence(),
                unit.presentationTimeUs(),
                payload.length);
        record.put(payload);
        return record.array();
    }

    private static ByteBuffer newRecord(
            int kind,
            int flags,
            StreamKey stream,
            long sequence,
            long presentationTimeUs,
            int payloadLength) {
        if (payloadLength < 0 || payloadLength > EncodedAvcAccessUnit.MAX_PAYLOAD_BYTES) {
            throw new IllegalArgumentException("AVC wire payload exceeds the bootstrap bound");
        }
        ByteBuffer record = ByteBuffer.allocate(Math.addExact(HEADER_BYTES, payloadLength))
                .order(ByteOrder.BIG_ENDIAN);
        record.put(MAGIC);
        record.put((byte) VERSION_MAJOR);
        record.put((byte) VERSION_MINOR);
        record.put((byte) kind);
        record.put((byte) flags);
        record.putShort((short) HEADER_BYTES);
        record.putShort((short) 0);
        record.put(stream.streamId);
        record.putLong(stream.epoch);
        record.putLong(sequence);
        record.putLong(presentationTimeUs);
        record.putInt(payloadLength);
        return record;
    }

    private static byte[] copy(ByteBuffer source) {
        ByteBuffer view = source.duplicate();
        byte[] copy = new byte[view.remaining()];
        view.get(copy);
        return copy;
    }
}
