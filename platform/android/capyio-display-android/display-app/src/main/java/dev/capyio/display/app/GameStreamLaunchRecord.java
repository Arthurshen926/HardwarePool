package dev.capyio.display.app;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.util.Arrays;
import java.util.Objects;
import java.util.UUID;

/** Closed parser for the secret launch record received after mTLS Route binding. */
public final class GameStreamLaunchRecord implements AutoCloseable {
    public static final int RECORD_BYTES = 176;
    public static final int ACK_BYTES = 64;
    private static final byte[] MAGIC = {'C', 'P', 'Y', 'G', 'S', '0', '0', '1'};
    private static final int VERSION = 1;

    public record Expected(
            UUID adapterInstanceId,
            UUID displayCapabilityId,
            UUID sessionId,
            UUID routeId,
            UUID streamId,
            long streamEpoch,
            byte[] hostIpv4,
            int portBase,
            int width,
            int height,
            int refreshMillihertz,
            int codec,
            int dynamicRange,
            int bitrateKbps) {
        public Expected {
            Objects.requireNonNull(adapterInstanceId, "adapterInstanceId");
            Objects.requireNonNull(displayCapabilityId, "displayCapabilityId");
            Objects.requireNonNull(sessionId, "sessionId");
            Objects.requireNonNull(routeId, "routeId");
            Objects.requireNonNull(streamId, "streamId");
            Objects.requireNonNull(hostIpv4, "hostIpv4");
            hostIpv4 = Arrays.copyOf(hostIpv4, hostIpv4.length);
            if (isNil(adapterInstanceId) || isNil(displayCapabilityId) || isNil(sessionId)
                    || isNil(routeId) || isNil(streamId) || streamEpoch <= 0
                    || hostIpv4.length != 4 || allZero(hostIpv4)
                    || portBase < 1024 || portBase > 65514 || portBase == 47989
                    || width <= 0 || width > 8192 || height <= 0 || height > 8192
                    || refreshMillihertz <= 0 || refreshMillihertz > 500000
                    || codec < 1 || codec > 3 || dynamicRange < 1 || dynamicRange > 2
                    || (codec == 1 && dynamicRange == 2)
                    || bitrateKbps <= 0 || bitrateKbps > 200000) {
                throw new IllegalArgumentException("expected GameStream launch binding is invalid");
            }
        }

        @Override
        public byte[] hostIpv4() {
            return Arrays.copyOf(hostIpv4, hostIpv4.length);
        }
    }

    private final int launchSessionId;
    private final byte[] key;
    private final int keyId;
    private final byte[] pingPayload;
    private final byte[] acknowledgement;
    private final int controlConnectData;
    private boolean closed;

    private GameStreamLaunchRecord(
            int launchSessionId,
            byte[] key,
            int keyId,
            byte[] pingPayload,
            int controlConnectData,
            byte[] acknowledgement) {
        this.launchSessionId = launchSessionId;
        this.key = key;
        this.keyId = keyId;
        this.pingPayload = pingPayload;
        this.controlConnectData = controlConnectData;
        this.acknowledgement = acknowledgement;
    }

    public static GameStreamLaunchRecord parse(byte[] record, Expected expected) {
        Objects.requireNonNull(record, "record");
        Objects.requireNonNull(expected, "expected");
        if (record.length != RECORD_BYTES || !Arrays.equals(MAGIC, Arrays.copyOf(record, 8))) {
            throw new SecurityException("GameStream launch record framing is invalid");
        }
        ByteBuffer bytes = ByteBuffer.wrap(record).order(ByteOrder.BIG_ENDIAN);
        if (Short.toUnsignedInt(bytes.getShort(8)) != VERSION
                || Short.toUnsignedInt(bytes.getShort(10)) != RECORD_BYTES
                || bytes.getInt(12) != RECORD_BYTES
                || bytes.getShort(110) != 0 || bytes.getInt(172) != 0) {
            throw new SecurityException("GameStream launch record version or reserved fields are invalid");
        }
        requireUuid(bytes, 16, expected.adapterInstanceId());
        requireUuid(bytes, 32, expected.displayCapabilityId());
        requireUuid(bytes, 48, expected.sessionId());
        requireUuid(bytes, 64, expected.routeId());
        requireUuid(bytes, 80, expected.streamId());
        if (bytes.getLong(96) != expected.streamEpoch()
                || !Arrays.equals(Arrays.copyOfRange(record, 104, 108), expected.hostIpv4())
                || Short.toUnsignedInt(bytes.getShort(108)) != expected.portBase()
                || bytes.getInt(112) != expected.width()
                || bytes.getInt(116) != expected.height()
                || bytes.getInt(120) != expected.refreshMillihertz()
                || bytes.getInt(124) != expected.codec()
                || bytes.getInt(128) != expected.dynamicRange()
                || bytes.getInt(132) != expected.bitrateKbps()) {
            throw new SecurityException("GameStream launch record differs from the authorized selection");
        }
        int launchSessionId = bytes.getInt(136);
        byte[] key = Arrays.copyOfRange(record, 140, 156);
        if (launchSessionId == 0 || allZero(key)) {
            Arrays.fill(key, (byte) 0);
            throw new SecurityException("GameStream launch secrets are invalid");
        }
        return new GameStreamLaunchRecord(
                launchSessionId,
                key,
                bytes.getInt(156),
                Arrays.copyOfRange(record, 160, 168),
                bytes.getInt(168),
                encodeAcknowledgement(expected, launchSessionId));
    }

    public int launchSessionId() {
        requireOpen();
        return launchSessionId;
    }

    public byte[] keyCopy() {
        requireOpen();
        return Arrays.copyOf(key, key.length);
    }

    public int keyId() {
        requireOpen();
        return keyId;
    }

    public byte[] pingPayloadCopy() {
        requireOpen();
        return Arrays.copyOf(pingPayload, pingPayload.length);
    }

    public int controlConnectData() {
        requireOpen();
        return controlConnectData;
    }

    /** Returned only after the full launch record has passed authorization. */
    public byte[] acknowledgement() {
        requireOpen();
        return Arrays.copyOf(acknowledgement, acknowledgement.length);
    }

    @Override
    public void close() {
        Arrays.fill(key, (byte) 0);
        Arrays.fill(pingPayload, (byte) 0);
        Arrays.fill(acknowledgement, (byte) 0);
        closed = true;
    }

    private void requireOpen() {
        if (closed) {
            throw new IllegalStateException("GameStream launch record is closed");
        }
    }

    private static void requireUuid(ByteBuffer bytes, int offset, UUID expected) {
        UUID actual = new UUID(bytes.getLong(offset), bytes.getLong(offset + 8));
        if (!actual.equals(expected)) {
            throw new SecurityException("GameStream launch identity is unauthorized");
        }
    }

    private static boolean isNil(UUID value) {
        return value.getMostSignificantBits() == 0 && value.getLeastSignificantBits() == 0;
    }

    private static boolean allZero(byte[] value) {
        for (byte item : value) {
            if (item != 0) {
                return false;
            }
        }
        return true;
    }

    private static byte[] encodeAcknowledgement(Expected expected, int launchSessionId) {
        ByteBuffer bytes = ByteBuffer.allocate(ACK_BYTES).order(ByteOrder.BIG_ENDIAN);
        bytes.put(new byte[] {'C', 'P', 'Y', 'G', 'S', 'A', '0', '1'});
        bytes.putShort((short) VERSION).putShort((short) ACK_BYTES);
        bytes.putInt(launchSessionId);
        putUuid(bytes, expected.routeId());
        putUuid(bytes, expected.streamId());
        bytes.putLong(expected.streamEpoch());
        bytes.putInt(0).putInt(0);
        return bytes.array();
    }

    private static void putUuid(ByteBuffer bytes, UUID value) {
        bytes.putLong(value.getMostSignificantBits());
        bytes.putLong(value.getLeastSignificantBits());
    }
}
