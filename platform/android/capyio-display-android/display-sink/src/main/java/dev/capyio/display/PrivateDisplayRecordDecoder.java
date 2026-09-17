package dev.capyio.display;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.util.Arrays;
import java.util.Objects;
import java.util.UUID;

/** Strict complete-record parser for the private CapyIO display framing v1. */
public final class PrivateDisplayRecordDecoder {
    public static final int RECORD_VERSION = 1;
    private static final byte[] MAGIC = {'C', 'P', 'D', '1'};
    private static final int KIND_DECODER_CONFIG = 1;
    private static final int KIND_ACCESS_UNIT = 2;
    private static final int COMMON_HEADER_BYTES = 12;
    private static final int DECODER_CONFIG_HEADER_BYTES = 68;
    private static final int ACCESS_UNIT_HEADER_BYTES = 60;
    private static final int FLAG_KEY_FRAME = 1 << 0;
    private static final int FLAG_DISCONTINUITY = 1 << 1;
    private static final int FLAG_END_OF_STREAM = 1 << 2;
    private static final int KNOWN_ACCESS_UNIT_FLAGS =
            FLAG_KEY_FRAME | FLAG_DISCONTINUITY | FLAG_END_OF_STREAM;
    private static final int MAX_CODEC_CONFIG_BODY_BYTES = 2 * 64 * 1024;

    private PrivateDisplayRecordDecoder() {}

    public static AvcDecoderConfiguration decodeConfiguration(byte[] record) {
        Prelude prelude = parsePrelude(record, KIND_DECODER_CONFIG, DECODER_CONFIG_HEADER_BYTES);
        if (prelude.flags != 0) {
            throw new IllegalArgumentException("decoder configuration flags must be zero");
        }
        if (prelude.bodyBytes > MAX_CODEC_CONFIG_BODY_BYTES) {
            throw new IllegalArgumentException("decoder configuration body exceeds its bound");
        }
        if (record[58] != 0 || record[59] != 0) {
            throw new IllegalArgumentException("decoder configuration reserved bytes are nonzero");
        }

        ByteBuffer buffer = ByteBuffer.wrap(record).order(ByteOrder.BIG_ENDIAN);
        int spsBytes = positiveLength(buffer.getInt(60), "SPS");
        int ppsBytes = positiveLength(buffer.getInt(64), "PPS");
        if ((long) spsBytes + ppsBytes != prelude.bodyBytes) {
            throw new IllegalArgumentException("SPS and PPS lengths do not match the record body");
        }
        int spsEnd = Math.addExact(prelude.headerBytes, spsBytes);
        byte[] sps = Arrays.copyOfRange(record, prelude.headerBytes, spsEnd);
        byte[] pps = Arrays.copyOfRange(record, spsEnd, record.length);

        return new AvcDecoderConfiguration(
                readUuid(buffer, 12),
                buffer.getLong(28),
                buffer.getInt(36),
                buffer.getInt(40),
                buffer.getInt(44),
                buffer.getInt(48),
                buffer.getInt(52),
                decodeProfile(Byte.toUnsignedInt(record[56])),
                decodeLevel(Byte.toUnsignedInt(record[57])),
                sps,
                pps);
    }

    public static EncodedAccessUnit decodeAccessUnit(byte[] record, int selectedMaximumBytes) {
        Prelude prelude = parsePrelude(record, KIND_ACCESS_UNIT, ACCESS_UNIT_HEADER_BYTES);
        if ((prelude.flags & ~KNOWN_ACCESS_UNIT_FLAGS) != 0) {
            throw new IllegalArgumentException("access-unit flags contain reserved bits");
        }
        ByteBuffer buffer = ByteBuffer.wrap(record).order(ByteOrder.BIG_ENDIAN);
        byte[] payload = Arrays.copyOfRange(record, prelude.headerBytes, record.length);
        return new EncodedAccessUnit(
                readUuid(buffer, 12),
                buffer.getLong(28),
                buffer.getLong(36),
                buffer.getLong(44),
                buffer.getLong(52),
                (prelude.flags & FLAG_KEY_FRAME) != 0,
                (prelude.flags & FLAG_DISCONTINUITY) != 0,
                (prelude.flags & FLAG_END_OF_STREAM) != 0,
                payload,
                selectedMaximumBytes);
    }

    private static Prelude parsePrelude(byte[] record, int expectedKind, int expectedHeaderBytes) {
        Objects.requireNonNull(record, "record");
        if (record.length < COMMON_HEADER_BYTES) {
            throw new IllegalArgumentException("record is shorter than the common header");
        }
        for (int index = 0; index < MAGIC.length; index++) {
            if (record[index] != MAGIC[index]) {
                throw new IllegalArgumentException("record magic is invalid");
            }
        }
        if (Byte.toUnsignedInt(record[4]) != RECORD_VERSION) {
            throw new IllegalArgumentException("record version is unsupported");
        }
        if (Byte.toUnsignedInt(record[5]) != expectedKind) {
            throw new IllegalArgumentException("record kind does not match the requested DTO");
        }
        int headerBytes = Byte.toUnsignedInt(record[7]);
        if (headerBytes != expectedHeaderBytes) {
            throw new IllegalArgumentException("record header length is invalid");
        }
        int bodyBytes = ByteBuffer.wrap(record, 8, 4).order(ByteOrder.BIG_ENDIAN).getInt();
        if (bodyBytes < 0 || bodyBytes > AvcDecoderConfiguration.MAX_ACCESS_UNIT_BYTES) {
            throw new IllegalArgumentException("record body exceeds the absolute bound");
        }
        if ((long) headerBytes + bodyBytes != record.length) {
            throw new IllegalArgumentException("record length does not match header plus body");
        }
        return new Prelude(Byte.toUnsignedInt(record[6]), headerBytes, bodyBytes);
    }

    private static UUID readUuid(ByteBuffer buffer, int offset) {
        return new UUID(buffer.getLong(offset), buffer.getLong(offset + Long.BYTES));
    }

    private static int positiveLength(int value, String label) {
        if (value <= 0 || value > AvcDecoderConfiguration.MAX_CODEC_SPECIFIC_DATA_BYTES) {
            throw new IllegalArgumentException(label + " length is outside its bound");
        }
        return value;
    }

    private static AvcDecoderConfiguration.Profile decodeProfile(int value) {
        return switch (value) {
            case 1 -> AvcDecoderConfiguration.Profile.BASELINE;
            case 2 -> AvcDecoderConfiguration.Profile.MAIN;
            case 3 -> AvcDecoderConfiguration.Profile.HIGH;
            default -> throw new IllegalArgumentException("AVC profile value is unknown");
        };
    }

    private static AvcDecoderConfiguration.Level decodeLevel(int value) {
        return switch (value) {
            case 1 -> AvcDecoderConfiguration.Level.L3_1;
            case 2 -> AvcDecoderConfiguration.Level.L4;
            case 3 -> AvcDecoderConfiguration.Level.L4_1;
            case 4 -> AvcDecoderConfiguration.Level.L4_2;
            case 5 -> AvcDecoderConfiguration.Level.L5;
            case 6 -> AvcDecoderConfiguration.Level.L5_1;
            case 7 -> AvcDecoderConfiguration.Level.L5_2;
            default -> throw new IllegalArgumentException("AVC level value is unknown");
        };
    }

    private static final class Prelude {
        final int flags;
        final int headerBytes;
        final int bodyBytes;

        Prelude(int flags, int headerBytes, int bodyBytes) {
            this.flags = flags;
            this.headerBytes = headerBytes;
            this.bodyBytes = bodyBytes;
        }
    }
}
