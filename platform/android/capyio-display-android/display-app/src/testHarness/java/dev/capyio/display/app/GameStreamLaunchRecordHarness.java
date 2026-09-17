package dev.capyio.display.app;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.util.Arrays;
import java.util.UUID;

public final class GameStreamLaunchRecordHarness {
    private GameStreamLaunchRecordHarness() {}

    public static void main(String[] args) {
        GameStreamLaunchRecord.Expected expected = expected();
        byte[] record = fixture();
        GameStreamLaunchRecord launch = GameStreamLaunchRecord.parse(record, expected);
        try (launch) {
            check(launch.launchSessionId() == 7, "launch ID mismatch");
            check(Arrays.equals(launch.keyCopy(), filled(0x42, 16)), "key mismatch");
            check(launch.keyId() == 0x12345678, "key ID mismatch");
            check(launch.controlConnectData() == 0x90abcdef, "control nonce mismatch");
            check(Arrays.equals(launch.acknowledgement(), acknowledgement()),
                    "launch acknowledgement mismatch");
        }
        expectClosed(launch::acknowledgement);

        ByteArrayOutputStream output = new ByteArrayOutputStream();
        GameStreamLaunchRecord exchanged;
        try {
            exchanged = GameStreamLaunchExchange.receiveAndAcknowledge(
                    new ByteArrayInputStream(fixture()), output, expected);
        } catch (Exception error) {
            throw new AssertionError("launch exchange failed", error);
        }
        try (exchanged) {
            check(Arrays.equals(output.toByteArray(), acknowledgement()),
                    "launch exchange acknowledgement mismatch");
        }

        byte[] changedRefresh = fixture();
        changedRefresh[120 + 3] ^= 1;
        expectFailure(() -> GameStreamLaunchRecord.parse(changedRefresh, expected));
        byte[] crossRoute = fixture();
        crossRoute[64] ^= 1;
        expectFailure(() -> GameStreamLaunchRecord.parse(crossRoute, expected));
        byte[] zeroKey = fixture();
        Arrays.fill(zeroKey, 140, 156, (byte) 0);
        expectFailure(() -> GameStreamLaunchRecord.parse(zeroKey, expected));
        byte[] reserved = fixture();
        reserved[175] = 1;
        expectFailure(() -> GameStreamLaunchRecord.parse(reserved, expected));
    }

    private static GameStreamLaunchRecord.Expected expected() {
        return new GameStreamLaunchRecord.Expected(
                repeatedUuid(1), repeatedUuid(2), repeatedUuid(3), repeatedUuid(4), repeatedUuid(5),
                9, new byte[] {(byte) 192, 0, 2, 10}, 50000,
                2640, 1216, 59940, 2, 2, 45000);
    }

    private static byte[] fixture() {
        ByteBuffer bytes = ByteBuffer.allocate(GameStreamLaunchRecord.RECORD_BYTES)
                .order(ByteOrder.BIG_ENDIAN);
        bytes.put(new byte[] {'C', 'P', 'Y', 'G', 'S', '0', '0', '1'});
        bytes.putShort((short) 1).putShort((short) 176).putInt(176);
        bytes.put(filled(1, 16)).put(filled(2, 16)).put(filled(3, 16));
        bytes.put(filled(4, 16)).put(filled(5, 16)).putLong(9);
        bytes.put(new byte[] {(byte) 192, 0, 2, 10}).putShort((short) 50000).putShort((short) 0);
        bytes.putInt(2640).putInt(1216).putInt(59940).putInt(2).putInt(2).putInt(45000);
        bytes.putInt(7).put(filled(0x42, 16)).putInt(0x12345678);
        bytes.put(filled(0x10, 8)).putInt(0x90abcdef).putInt(0);
        return bytes.array();
    }

    private static byte[] acknowledgement() {
        ByteBuffer bytes = ByteBuffer.allocate(GameStreamLaunchRecord.ACK_BYTES)
                .order(ByteOrder.BIG_ENDIAN);
        bytes.put(new byte[] {'C', 'P', 'Y', 'G', 'S', 'A', '0', '1'});
        bytes.putShort((short) 1).putShort((short) 64).putInt(7);
        bytes.put(filled(4, 16)).put(filled(5, 16)).putLong(9).putInt(0).putInt(0);
        return bytes.array();
    }

    private static UUID repeatedUuid(int value) {
        byte[] bytes = filled(value, 16);
        ByteBuffer buffer = ByteBuffer.wrap(bytes).order(ByteOrder.BIG_ENDIAN);
        return new UUID(buffer.getLong(), buffer.getLong());
    }

    private static byte[] filled(int value, int count) {
        byte[] bytes = new byte[count];
        Arrays.fill(bytes, (byte) value);
        return bytes;
    }

    private static void expectFailure(Runnable operation) {
        try {
            operation.run();
            throw new AssertionError("expected failure");
        } catch (IllegalArgumentException | SecurityException expected) {
            // Expected fail-closed validation.
        }
    }

    private static void expectClosed(Runnable operation) {
        try {
            operation.run();
            throw new AssertionError("expected closed launch record");
        } catch (IllegalStateException expected) {
            // Secret-bearing record is unusable after close.
        }
    }

    private static void check(boolean condition, String message) {
        if (!condition) {
            throw new AssertionError(message);
        }
    }
}
