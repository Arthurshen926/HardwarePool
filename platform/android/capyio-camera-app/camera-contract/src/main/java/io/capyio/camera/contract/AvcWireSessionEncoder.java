package io.capyio.camera.contract;

import java.nio.ByteBuffer;
import java.util.List;
import java.util.Objects;

/**
 * Stateful, transport-free encoder for one private CAVC stream epoch.
 *
 * <p>The caller may drop access units before they reach this object. A gap is
 * recovered only at a key frame and is marked as a discontinuity. Codec-config
 * buffers are consumed as observations because the dedicated config record is
 * built from MediaCodec's output format.</p>
 */
public final class AvcWireSessionEncoder {
    private final AvcWireRecordEncoder.StreamKey stream;
    private final AvcEncoderConfig encoderConfig;

    private AvcCodecConfig codecConfig;
    private AvcWireRecordEncoder.Layout codecSpecificLayout;
    private AvcWireRecordEncoder.Layout accessUnitLayout;
    private long lastObservedSequence;
    private long lastSentSequence;
    private long lastSentPresentationTimeUs;
    private boolean configSent;
    private boolean ended;

    public AvcWireSessionEncoder(
            AvcWireRecordEncoder.StreamKey stream,
            AvcEncoderConfig encoderConfig) {
        this.stream = Objects.requireNonNull(stream, "stream");
        this.encoderConfig = Objects.requireNonNull(encoderConfig, "encoderConfig");
    }

    public void setCodecConfig(AvcCodecConfig config) {
        Objects.requireNonNull(config, "config");
        if (configSent) {
            throw new IllegalArgumentException("codec config cannot change inside one stream epoch");
        }
        AvcWireRecordEncoder.Layout detected = detectCodecSpecificLayout(config.csd0View());
        if (codecSpecificLayout != null && codecSpecificLayout != detected) {
            throw new IllegalArgumentException("codec-specific layout changed before stream start");
        }
        codecConfig = config;
        codecSpecificLayout = detected;
    }

    /**
     * Returns zero records while awaiting config/key-frame recovery, two for
     * the first accepted unit (config + unit), and one thereafter.
     */
    public List<byte[]> encode(EncodedAvcAccessUnit unit) {
        Objects.requireNonNull(unit, "unit");
        if (ended) {
            throw new IllegalArgumentException("stream epoch already ended");
        }
        if (unit.sequence() <= lastObservedSequence) {
            throw new IllegalArgumentException("access-unit sequence did not advance");
        }
        lastObservedSequence = unit.sequence();

        if (unit.codecConfig()) {
            return List.of();
        }
        if (codecConfig == null) {
            return List.of();
        }
        if (unit.endOfStream()) {
            if (!configSent
                    || unit.sequence() != lastSentSequence + 1
                    || unit.presentationTimeUs() < lastSentPresentationTimeUs) {
                return List.of();
            }
            byte[] terminal = AvcWireRecordEncoder.encodeAccessUnit(stream, unit, false);
            lastSentSequence = unit.sequence();
            lastSentPresentationTimeUs = unit.presentationTimeUs();
            ended = true;
            return List.of(terminal);
        }

        AvcWireRecordEncoder.Layout detected = detectAccessUnitLayout(unit.payloadView());
        if (accessUnitLayout == null) {
            accessUnitLayout = detected;
        } else if (accessUnitLayout != detected) {
            throw new IllegalArgumentException("access-unit layout changed inside one stream epoch");
        }

        boolean discontinuity = lastSentSequence == 0
                ? unit.sequence() != 1
                : unit.sequence() != lastSentSequence + 1;
        if ((lastSentSequence == 0 || discontinuity) && !unit.keyFrame()) {
            return List.of();
        }
        if (lastSentSequence != 0
                && unit.presentationTimeUs() < lastSentPresentationTimeUs) {
            throw new IllegalArgumentException("access-unit presentation time regressed");
        }

        byte[] access = AvcWireRecordEncoder.encodeAccessUnit(stream, unit, discontinuity);
        lastSentSequence = unit.sequence();
        lastSentPresentationTimeUs = unit.presentationTimeUs();
        if (configSent) {
            return List.of(access);
        }

        byte[] config = AvcWireRecordEncoder.encodeConfig(
                stream,
                encoderConfig,
                accessUnitLayout,
                codecSpecificLayout,
                codecConfig);
        configSent = true;
        return List.of(config, access);
    }

    public long lastSentSequence() {
        return lastSentSequence;
    }

    public boolean configSent() {
        return configSent;
    }

    private static AvcWireRecordEncoder.Layout detectCodecSpecificLayout(ByteBuffer bytes) {
        if (startsWithAnnexB(bytes)) {
            return AvcWireRecordEncoder.Layout.ANNEX_B;
        }
        if (bytes.remaining() >= 7 && unsigned(bytes.get(bytes.position())) == 1) {
            return AvcWireRecordEncoder.Layout.AVC_DECODER_CONFIGURATION_RECORD;
        }
        if (isLengthPrefixed4(bytes)) {
            return AvcWireRecordEncoder.Layout.LENGTH_PREFIXED_4;
        }
        throw new IllegalArgumentException("unsupported AVC codec-specific layout");
    }

    private static AvcWireRecordEncoder.Layout detectAccessUnitLayout(ByteBuffer bytes) {
        if (startsWithAnnexB(bytes)) {
            return AvcWireRecordEncoder.Layout.ANNEX_B;
        }
        if (isLengthPrefixed4(bytes)) {
            return AvcWireRecordEncoder.Layout.LENGTH_PREFIXED_4;
        }
        throw new IllegalArgumentException("unsupported AVC access-unit layout");
    }

    private static boolean startsWithAnnexB(ByteBuffer source) {
        ByteBuffer bytes = source.duplicate();
        int remaining = bytes.remaining();
        int offset = bytes.position();
        return remaining >= 3
                && unsigned(bytes.get(offset)) == 0
                && unsigned(bytes.get(offset + 1)) == 0
                && (unsigned(bytes.get(offset + 2)) == 1
                        || (remaining >= 4
                                && unsigned(bytes.get(offset + 2)) == 0
                                && unsigned(bytes.get(offset + 3)) == 1));
    }

    private static boolean isLengthPrefixed4(ByteBuffer source) {
        ByteBuffer bytes = source.duplicate();
        while (bytes.hasRemaining()) {
            if (bytes.remaining() < 5) {
                return false;
            }
            long length = Integer.toUnsignedLong(bytes.getInt());
            if (length == 0 || length > bytes.remaining()) {
                return false;
            }
            bytes.position(bytes.position() + (int) length);
        }
        return true;
    }

    private static int unsigned(byte value) {
        return value & 0xff;
    }
}
