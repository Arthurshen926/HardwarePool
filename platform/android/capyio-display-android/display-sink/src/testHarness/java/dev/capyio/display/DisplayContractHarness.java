package dev.capyio.display;

import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Arrays;
import java.util.UUID;

public final class DisplayContractHarness {
    private static final UUID STREAM_ID = UUID.fromString("00000000-0000-4000-8000-00000000d201");

    private DisplayContractHarness() {}

    public static void main(String[] args) throws IOException {
        configurationIsBoundedAndDefensivelyCopied();
        accessUnitIsBoundedAndDefensivelyCopied();
        inputGateRequiresKeyFramesAndResynchronizesAfterGap();
        inputGateRejectsWrongIdentityAndNonAdvancingValues();
        reconfigurationRequiresStopAndNewEpoch();
        crossLanguagePrivateRecordsAreStrict(args);
        realAvcPrivateRecordsAreBounded(args);
        System.out.println("Android display contract harness: PASS (7 cases)");
    }

    private static void configurationIsBoundedAndDefensivelyCopied() {
        byte[] sps = sps();
        byte[] pps = pps();
        AvcDecoderConfiguration configuration = configuration(1, sps, pps);
        sps[4] = 0;
        pps[4] = 0;
        check((configuration.sequenceParameterSetCopy()[4] & 0x1f) == 7, "SPS copy changed");
        check((configuration.pictureParameterSetCopy()[4] & 0x1f) == 8, "PPS copy changed");
        expectFailure(() -> configuration(1, new byte[] {0, 0, 0, 1, 0x68}, pps()));
        expectFailure(() -> configuration(0, sps(), pps()));
    }

    private static void accessUnitIsBoundedAndDefensivelyCopied() {
        byte[] payload = {0, 0, 0, 1, 0x65, 1, 2};
        EncodedAccessUnit unit = unit(1, 0, true, false, false, payload);
        payload[4] = 0;
        ByteBuffer copied = ByteBuffer.allocate(unit.payloadSize());
        unit.copyPayloadTo(copied);
        check(copied.array()[4] == 0x65, "access-unit copy changed");
        expectFailure(() -> unit(1, 0, true, false, false, new byte[] {1, 2, 3}));
        expectFailure(() -> unit(1, 0, true, false, true, new byte[] {0, 0, 0, 1, 0x65}));
    }

    private static void inputGateRequiresKeyFramesAndResynchronizesAfterGap() {
        DecoderInputGate gate = new DecoderInputGate();
        AvcDecoderConfiguration configuration = configuration(1, sps(), pps());
        gate.ensureCanConfigure(configuration);
        gate.configured(configuration);

        EncodedAccessUnit dependent = unit(1, 0, false, false, false, pFrame());
        check(gate.plan(dependent) == DecoderInputGate.Plan.DROP_AWAITING_KEY_FRAME,
                "startup dependent frame was not dropped");
        gate.dropped(dependent);

        EncodedAccessUnit key = unit(1, 1, true, false, false, idr());
        check(gate.plan(key) == DecoderInputGate.Plan.QUEUE, "startup key frame was not queued");
        check(!gate.queued(key), "startup key frame was mislabeled as resynchronization");

        EncodedAccessUnit afterGap = unit(1, 3, false, false, false, pFrame());
        check(gate.plan(afterGap) == DecoderInputGate.Plan.FLUSH_DROP_AWAITING_KEY_FRAME,
                "sequence gap did not flush and drop dependent input");
        gate.flushed();
        gate.dropped(afterGap);

        EncodedAccessUnit resync = unit(1, 4, true, false, false, idr());
        check(gate.plan(resync) == DecoderInputGate.Plan.QUEUE, "resync key frame was not queued");
        check(gate.queued(resync), "resync key frame was not marked");
    }

    private static void inputGateRejectsWrongIdentityAndNonAdvancingValues() {
        DecoderInputGate gate = new DecoderInputGate();
        AvcDecoderConfiguration configuration = configuration(2, sps(), pps());
        gate.configured(configuration);
        EncodedAccessUnit first = unit(2, 0, true, false, false, idr());
        gate.queued(first);

        expectFailure(() -> gate.plan(new EncodedAccessUnit(
                UUID.randomUUID(), 2, 1, 1_033_333, 33_333_333,
                false, false, false, pFrame(), 1024)));
        expectFailure(() -> gate.plan(unit(3, 1, false, false, false, pFrame())));
        expectFailure(() -> gate.plan(unit(2, 0, true, false, false, idr())));

        EncodedAccessUnit staleTimestamp = new EncodedAccessUnit(
                STREAM_ID, 2, 1, 1_000_000, 33_333_333,
                false, false, false, pFrame(), 1024);
        expectFailure(() -> gate.plan(staleTimestamp));
    }

    private static void reconfigurationRequiresStopAndNewEpoch() {
        DecoderInputGate gate = new DecoderInputGate();
        AvcDecoderConfiguration epochFour = configuration(4, sps(), pps());
        gate.configured(epochFour);
        expectFailure(() -> gate.ensureCanConfigure(configuration(5, sps(), pps())));
        gate.stopped();
        expectFailure(() -> gate.ensureCanConfigure(configuration(4, sps(), pps())));
        gate.ensureCanConfigure(configuration(5, sps(), pps()));
    }

    private static void crossLanguagePrivateRecordsAreStrict(String[] args) throws IOException {
        check(args.length == 4, "expected synthetic and real AVC fixture paths");
        byte[] configurationBytes = readHexFixture(Path.of(args[0]));
        byte[] accessUnitBytes = readHexFixture(Path.of(args[1]));

        AvcDecoderConfiguration configuration =
                PrivateDisplayRecordDecoder.decodeConfiguration(configurationBytes);
        check(configuration.streamId().equals(STREAM_ID), "fixture stream ID changed");
        check(configuration.streamEpoch() == 1, "fixture stream epoch changed");
        check(configuration.width() == 1280 && configuration.height() == 720,
                "fixture dimensions changed");
        check(configuration.frameRateNumerator() == 30
                        && configuration.frameRateDenominator() == 1,
                "fixture frame rate changed");
        check(configuration.maximumAccessUnitBytes() == 2 * 1024 * 1024,
                "fixture access-unit bound changed");
        check(configuration.profile() == AvcDecoderConfiguration.Profile.MAIN,
                "fixture AVC profile changed");
        check(configuration.level() == AvcDecoderConfiguration.Level.L3_1,
                "fixture AVC level changed");

        EncodedAccessUnit unit = PrivateDisplayRecordDecoder.decodeAccessUnit(
                accessUnitBytes, configuration.maximumAccessUnitBytes());
        check(unit.streamId().equals(STREAM_ID), "fixture access-unit stream changed");
        check(unit.streamEpoch() == 1 && unit.sequence() == 0,
                "fixture access-unit ordering changed");
        check(unit.keyFrame() && !unit.discontinuity() && !unit.endOfStream(),
                "fixture access-unit flags changed");
        ByteBuffer payload = ByteBuffer.allocate(unit.payloadSize());
        unit.copyPayloadTo(payload);
        check(Arrays.equals(payload.array(), new byte[] {0, 0, 0, 1, 0x65, (byte) 0x88, (byte) 0x84}),
                "fixture access-unit payload changed");

        expectFailure(() -> PrivateDisplayRecordDecoder.decodeConfiguration(
                mutate(configurationBytes, 4, (byte) 2)));
        expectFailure(() -> PrivateDisplayRecordDecoder.decodeConfiguration(
                mutate(configurationBytes, 58, (byte) 1)));
        expectFailure(() -> PrivateDisplayRecordDecoder.decodeAccessUnit(
                mutate(accessUnitBytes, 6, (byte) 0x81),
                configuration.maximumAccessUnitBytes()));
        expectFailure(() -> PrivateDisplayRecordDecoder.decodeAccessUnit(
                Arrays.copyOf(accessUnitBytes, accessUnitBytes.length + 1),
                configuration.maximumAccessUnitBytes()));
    }

    private static void realAvcPrivateRecordsAreBounded(String[] args) throws IOException {
        byte[] configurationBytes = readHexFixture(Path.of(args[2]));
        byte[] accessUnitBytes = readHexFixture(Path.of(args[3]));
        AvcDecoderConfiguration configuration =
                PrivateDisplayRecordDecoder.decodeConfiguration(configurationBytes);
        check(configuration.streamId().equals(STREAM_ID), "real fixture stream ID changed");
        check(configuration.streamEpoch() == 2, "real fixture epoch changed");
        check(configuration.width() == 1280 && configuration.height() == 720,
                "real fixture dimensions changed");
        check(configuration.sequenceParameterSetCopy().length == 28,
                "real fixture SPS length changed");
        check(configuration.pictureParameterSetCopy().length == 9,
                "real fixture PPS length changed");

        EncodedAccessUnit unit = PrivateDisplayRecordDecoder.decodeAccessUnit(
                accessUnitBytes, configuration.maximumAccessUnitBytes());
        check(unit.streamId().equals(STREAM_ID) && unit.streamEpoch() == 2,
                "real fixture identity changed");
        check(unit.sequence() == 0 && unit.keyFrame() && unit.payloadSize() == 32_447,
                "real fixture key access unit changed");
        ByteBuffer payload = ByteBuffer.allocate(unit.payloadSize());
        unit.copyPayloadTo(payload);
        check((payload.array()[4] & 0x1f) == 5, "real fixture is not an IDR NAL unit");

        DecoderInputGate gate = new DecoderInputGate();
        gate.configured(configuration);
        check(gate.plan(unit) == DecoderInputGate.Plan.QUEUE,
                "real fixture was not accepted by the Android decoder gate");
    }

    private static byte[] readHexFixture(Path path) throws IOException {
        String text = Files.readString(path);
        if (text.length() > 2 * (AvcDecoderConfiguration.MAX_ACCESS_UNIT_BYTES + 128)) {
            throw new IllegalArgumentException("hex fixture exceeds the absolute record bound");
        }
        StringBuilder digits = new StringBuilder(text.length());
        for (int index = 0; index < text.length(); index++) {
            char value = text.charAt(index);
            if (!Character.isWhitespace(value)) {
                digits.append(value);
            }
        }
        if (digits.length() == 0 || digits.length() % 2 != 0) {
            throw new IllegalArgumentException("hex fixture must contain complete bytes");
        }
        byte[] decoded = new byte[digits.length() / 2];
        for (int index = 0; index < decoded.length; index++) {
            int high = Character.digit(digits.charAt(index * 2), 16);
            int low = Character.digit(digits.charAt(index * 2 + 1), 16);
            if (high < 0 || low < 0) {
                throw new IllegalArgumentException("hex fixture contains a non-hex character");
            }
            decoded[index] = (byte) ((high << 4) | low);
        }
        return decoded;
    }

    private static byte[] mutate(byte[] source, int offset, byte value) {
        byte[] copy = Arrays.copyOf(source, source.length);
        copy[offset] = value;
        return copy;
    }

    private static AvcDecoderConfiguration configuration(long epoch, byte[] sps, byte[] pps) {
        return new AvcDecoderConfiguration(
                STREAM_ID,
                epoch,
                1280,
                720,
                30,
                1,
                1024,
                AvcDecoderConfiguration.Profile.MAIN,
                AvcDecoderConfiguration.Level.L3_1,
                sps,
                pps);
    }

    private static EncodedAccessUnit unit(
            long epoch,
            long sequence,
            boolean keyFrame,
            boolean discontinuity,
            boolean endOfStream,
            byte[] payload) {
        return new EncodedAccessUnit(
                STREAM_ID,
                epoch,
                sequence,
                1_000_000 + sequence * 33_333,
                33_333_333,
                keyFrame,
                discontinuity,
                endOfStream,
                payload,
                1024);
    }

    private static byte[] sps() {
        return new byte[] {0, 0, 0, 1, 0x67, 0x64, 0, 31};
    }

    private static byte[] pps() {
        return new byte[] {0, 0, 0, 1, 0x68, (byte) 0xeb, (byte) 0xec, (byte) 0xb2};
    }

    private static byte[] idr() {
        return new byte[] {0, 0, 0, 1, 0x65, 1};
    }

    private static byte[] pFrame() {
        return new byte[] {0, 0, 0, 1, 0x41, 1};
    }

    private static void check(boolean condition, String message) {
        if (!condition) {
            throw new AssertionError(message);
        }
    }

    private static void expectFailure(Runnable action) {
        try {
            action.run();
        } catch (IllegalArgumentException | IllegalStateException expected) {
            return;
        }
        throw new AssertionError("expected validation failure");
    }
}
