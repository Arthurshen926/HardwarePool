package dev.capyio.android.lan;

import java.util.Arrays;
import java.util.Objects;
import java.util.UUID;

/** Fixed, bounded worker-thread wire codec for the insecure LAN lab backend. */
public final class NativeLanPacketCodec {
    public static final String BACKEND_ID = "dev.capyio.audio.lan-lab/1";
    public static final int WIRE_VERSION = 1;
    public static final int MAX_DATAGRAM_BYTES = 1_200;
    public static final int HEADER_BYTES = 104;
    public static final int MAX_FRAGMENT_PAYLOAD_BYTES = MAX_DATAGRAM_BYTES - HEADER_BYTES;
    public static final int MAX_FRAGMENTS = 64;
    public static final int MAX_PACKET_PAYLOAD_BYTES =
            MAX_FRAGMENT_PAYLOAD_BYTES * MAX_FRAGMENTS;

    private static final byte[] MAGIC = {'C', 'P', 'Y', 'A'};
    private static final int FLAG_DISCONTINUITY = 0x01;

    private NativeLanPacketCodec() {}

    public static int fragmentCount(int payloadBytes) {
        if (payloadBytes <= 0 || payloadBytes > MAX_PACKET_PAYLOAD_BYTES) {
            throw new IllegalArgumentException("payload length outside native LAN bound");
        }
        return (payloadBytes + MAX_FRAGMENT_PAYLOAD_BYTES - 1)
                / MAX_FRAGMENT_PAYLOAD_BYTES;
    }

    public static int encodeFragment(
            Binding binding,
            Packet packet,
            int fragmentIndex,
            byte[] output) {
        Objects.requireNonNull(binding, "binding");
        Objects.requireNonNull(packet, "packet");
        Objects.requireNonNull(output, "output");
        if (!packet.streamId.equals(binding.streamId) || packet.streamEpoch != binding.streamEpoch) {
            throw new IllegalArgumentException("packet does not match binding");
        }
        int fragmentCount = fragmentCount(packet.payload.length);
        if (fragmentIndex < 0 || fragmentIndex >= fragmentCount) {
            throw new IllegalArgumentException("fragment index outside packet");
        }
        int fragmentOffset = fragmentIndex * MAX_FRAGMENT_PAYLOAD_BYTES;
        int fragmentBytes = Math.min(
                packet.payload.length - fragmentOffset,
                MAX_FRAGMENT_PAYLOAD_BYTES);
        int datagramBytes = HEADER_BYTES + fragmentBytes;
        if (output.length < datagramBytes) {
            throw new IllegalArgumentException("datagram output buffer is too small");
        }

        Arrays.fill(output, 0, datagramBytes, (byte) 0);
        System.arraycopy(MAGIC, 0, output, 0, MAGIC.length);
        output[4] = (byte) WIRE_VERSION;
        output[5] = packet.discontinuity ? (byte) FLAG_DISCONTINUITY : 0;
        writeU16(output, 6, HEADER_BYTES);
        writeUuid(output, 8, binding.sessionId);
        writeUuid(output, 24, binding.routeId);
        writeUuid(output, 40, binding.streamId);
        writeU32(output, 56, binding.streamEpoch);
        writeU64(output, 60, packet.sequence);
        writeU64(output, 68, packet.sourceTimestampMicros);
        writeU64(output, 76, packet.firstSampleIndex);
        writeU32(output, 84, packet.sampleCount);
        writeU32(output, 88, packet.payload.length);
        writeU32(output, 92, fragmentOffset);
        writeU16(output, 96, fragmentIndex);
        writeU16(output, 98, fragmentCount);
        writeU16(output, 100, fragmentBytes);
        System.arraycopy(packet.payload, fragmentOffset, output, HEADER_BYTES, fragmentBytes);
        return datagramBytes;
    }

    public static Fragment decodeFragment(byte[] datagram, int length) {
        Objects.requireNonNull(datagram, "datagram");
        if (length < HEADER_BYTES || length > MAX_DATAGRAM_BYTES || length > datagram.length) {
            throw new IllegalArgumentException("datagram length outside native LAN bound");
        }
        for (int index = 0; index < MAGIC.length; index++) {
            if (datagram[index] != MAGIC[index]) {
                throw new IllegalArgumentException("unknown native LAN magic");
            }
        }
        if (Byte.toUnsignedInt(datagram[4]) != WIRE_VERSION) {
            throw new IllegalArgumentException("unsupported native LAN version");
        }
        int flags = Byte.toUnsignedInt(datagram[5]);
        if ((flags & ~FLAG_DISCONTINUITY) != 0) {
            throw new IllegalArgumentException("unknown native LAN flags");
        }
        if (readU16(datagram, 6) != HEADER_BYTES || readU16(datagram, 102) != 0) {
            throw new IllegalArgumentException("non-canonical native LAN header");
        }

        int totalPayloadBytes = readPositiveInt(datagram, 88, "payload length");
        int fragmentOffset = readNonNegativeInt(datagram, 92, "fragment offset");
        int fragmentIndex = readU16(datagram, 96);
        int fragmentCount = readU16(datagram, 98);
        int fragmentBytes = readU16(datagram, 100);
        int expectedCount = fragmentCount(totalPayloadBytes);
        if (fragmentCount != expectedCount || fragmentIndex >= fragmentCount) {
            throw new IllegalArgumentException("non-canonical fragment index/count");
        }
        int expectedOffset = fragmentIndex * MAX_FRAGMENT_PAYLOAD_BYTES;
        int expectedBytes = Math.min(
                totalPayloadBytes - expectedOffset,
                MAX_FRAGMENT_PAYLOAD_BYTES);
        if (fragmentOffset != expectedOffset
                || fragmentBytes != expectedBytes
                || length != HEADER_BYTES + fragmentBytes) {
            throw new IllegalArgumentException("inconsistent native LAN fragment bounds");
        }

        return new Fragment(
                new Binding(
                        readUuid(datagram, 8),
                        readUuid(datagram, 24),
                        readUuid(datagram, 40),
                        readPositiveInt(datagram, 56, "stream epoch")),
                readRawU64(datagram, 60),
                readRawU64(datagram, 68),
                readRawU64(datagram, 76),
                readPositiveInt(datagram, 84, "sample count"),
                (flags & FLAG_DISCONTINUITY) != 0,
                totalPayloadBytes,
                fragmentOffset,
                fragmentIndex,
                fragmentCount,
                Arrays.copyOfRange(datagram, HEADER_BYTES, length));
    }

    public static final class Binding {
        public final UUID sessionId;
        public final UUID routeId;
        public final UUID streamId;
        public final int streamEpoch;

        public Binding(UUID sessionId, UUID routeId, UUID streamId, int streamEpoch) {
            this.sessionId = requireNonNil(sessionId, "sessionId");
            this.routeId = requireNonNil(routeId, "routeId");
            this.streamId = requireNonNil(streamId, "streamId");
            if (streamEpoch <= 0) {
                throw new IllegalArgumentException("stream epoch must be positive");
            }
            this.streamEpoch = streamEpoch;
        }

        public boolean matches(Binding expected) {
            return expected != null
                    && sessionId.equals(expected.sessionId)
                    && routeId.equals(expected.routeId)
                    && streamId.equals(expected.streamId)
                    && streamEpoch == expected.streamEpoch;
        }

        private static UUID requireNonNil(UUID value, String name) {
            Objects.requireNonNull(value, name);
            if (value.getMostSignificantBits() == 0 && value.getLeastSignificantBits() == 0) {
                throw new IllegalArgumentException(name + " must not be nil");
            }
            return value;
        }
    }

    public static final class Packet {
        public final UUID streamId;
        public final int streamEpoch;
        public final long sequence;
        public final long sourceTimestampMicros;
        public final long firstSampleIndex;
        public final int sampleCount;
        public final boolean discontinuity;
        private final byte[] payload;

        public Packet(
                UUID streamId,
                int streamEpoch,
                long sequence,
                long sourceTimestampMicros,
                long firstSampleIndex,
                int sampleCount,
                boolean discontinuity,
                byte[] payload) {
            this.streamId = Objects.requireNonNull(streamId, "streamId");
            if (streamEpoch <= 0 || sampleCount <= 0) {
                throw new IllegalArgumentException("epoch and sample count must be positive");
            }
            this.streamEpoch = streamEpoch;
            this.sequence = sequence;
            this.sourceTimestampMicros = sourceTimestampMicros;
            this.firstSampleIndex = firstSampleIndex;
            this.sampleCount = sampleCount;
            this.discontinuity = discontinuity;
            this.payload = Objects.requireNonNull(payload, "payload").clone();
            fragmentCount(this.payload.length);
        }

        public byte[] payloadCopy() {
            return payload.clone();
        }

        public int payloadLength() {
            return payload.length;
        }
    }

    public static final class Fragment {
        public final Binding binding;
        public final long sequence;
        public final long sourceTimestampMicros;
        public final long firstSampleIndex;
        public final int sampleCount;
        public final boolean discontinuity;
        public final int totalPayloadBytes;
        public final int fragmentOffset;
        public final int fragmentIndex;
        public final int fragmentCount;
        private final byte[] payload;

        private Fragment(
                Binding binding,
                long sequence,
                long sourceTimestampMicros,
                long firstSampleIndex,
                int sampleCount,
                boolean discontinuity,
                int totalPayloadBytes,
                int fragmentOffset,
                int fragmentIndex,
                int fragmentCount,
                byte[] payload) {
            this.binding = binding;
            this.sequence = sequence;
            this.sourceTimestampMicros = sourceTimestampMicros;
            this.firstSampleIndex = firstSampleIndex;
            this.sampleCount = sampleCount;
            this.discontinuity = discontinuity;
            this.totalPayloadBytes = totalPayloadBytes;
            this.fragmentOffset = fragmentOffset;
            this.fragmentIndex = fragmentIndex;
            this.fragmentCount = fragmentCount;
            this.payload = payload;
        }

        public boolean matches(Binding expected) {
            return binding.matches(expected);
        }

        public byte[] payloadCopy() {
            return payload.clone();
        }
    }

    private static void writeUuid(byte[] output, int offset, UUID value) {
        writeU64(output, offset, value.getMostSignificantBits());
        writeU64(output, offset + 8, value.getLeastSignificantBits());
    }

    private static UUID readUuid(byte[] input, int offset) {
        return new UUID(readRawU64(input, offset), readRawU64(input, offset + 8));
    }

    private static void writeU16(byte[] output, int offset, int value) {
        output[offset] = (byte) (value >>> 8);
        output[offset + 1] = (byte) value;
    }

    private static void writeU32(byte[] output, int offset, int value) {
        output[offset] = (byte) (value >>> 24);
        output[offset + 1] = (byte) (value >>> 16);
        output[offset + 2] = (byte) (value >>> 8);
        output[offset + 3] = (byte) value;
    }

    private static void writeU64(byte[] output, int offset, long value) {
        for (int index = 7; index >= 0; index--) {
            output[offset + index] = (byte) value;
            value >>>= 8;
        }
    }

    private static int readU16(byte[] input, int offset) {
        return (Byte.toUnsignedInt(input[offset]) << 8)
                | Byte.toUnsignedInt(input[offset + 1]);
    }

    private static int readPositiveInt(byte[] input, int offset, String name) {
        int value = readNonNegativeInt(input, offset, name);
        if (value == 0) {
            throw new IllegalArgumentException(name + " must be positive");
        }
        return value;
    }

    private static int readNonNegativeInt(byte[] input, int offset, String name) {
        long value = readU32(input, offset);
        if (value > Integer.MAX_VALUE) {
            throw new IllegalArgumentException(name + " exceeds Java worker bound");
        }
        return (int) value;
    }

    private static long readU32(byte[] input, int offset) {
        return ((long) Byte.toUnsignedInt(input[offset]) << 24)
                | ((long) Byte.toUnsignedInt(input[offset + 1]) << 16)
                | ((long) Byte.toUnsignedInt(input[offset + 2]) << 8)
                | Byte.toUnsignedInt(input[offset + 3]);
    }

    // Java has no unsigned long storage type. Counter fields retain the exact unsigned
    // 64-bit wire pattern in a signed long; callers must use unsigned comparison/formatting.
    private static long readRawU64(byte[] input, int offset) {
        long value = 0;
        for (int index = 0; index < 8; index++) {
            value = (value << 8) | Byte.toUnsignedLong(input[offset + index]);
        }
        return value;
    }
}
