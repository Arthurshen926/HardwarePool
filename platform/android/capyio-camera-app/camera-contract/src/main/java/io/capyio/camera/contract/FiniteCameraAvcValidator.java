package io.capyio.camera.contract;

import java.nio.ByteBuffer;
import java.util.Arrays;

/**
 * Fail-closed, transport-free eligibility check for one finite 720p30 encoder epoch.
 *
 * <p>This is deliberately a stricter subset of the Linux native AVC parser:
 * Baseline (implicit 8-bit 4:2:0), progressive uncropped 1280x720, explicit BT.709
 * limited-range SDR and fixed 30-fps SPS timing are mandatory. Parameter sets
 * cannot change, media starts with an IDR, and sequence/PTS cannot reset. The
 * caller must route codec-config buffers to {@link #acceptConfig}, without
 * consuming media sequence numbers, and terminate capture on any exception.</p>
 *
 * <p>This is NOT a decoder, authentication check or proof of captured pixels,
 * camera orientation, actual cadence or delivery. Like the native pre-parser,
 * it inspects only the first three slice-header Exp-Golomb fields and bounds
 * SEI/AUD rather than interpreting their contents. The receiver must retain its
 * complete decoder, decoded-frame, authority, rate and work-budget checks.</p>
 *
 * <p>Use on a single serialized codec callback thread. Every failure permanently
 * latches this instance; there is no reset, fallback, recovery or new epoch.</p>
 */
public final class FiniteCameraAvcValidator {
    public static final int MAX_CODEC_CONFIG_BYTES = 64 * 1024;
    private static final int MAX_NALS = 256;
    private static final int SLICE_PREFIX_BYTES = 128;

    private ParameterSets parameterSets;
    private long lastSequence;
    private long lastPresentationTimeUs;
    private boolean ended;
    private boolean failed;

    public FiniteCameraAvcValidator(AvcEncoderConfig config) {
        require(config != null && config.width() == 1280 && config.height() == 720
                        && config.framesPerSecond() == 30
                        && config.clockwiseRotationDegrees() == 0,
                "finite AVC requires exact 1280x720 at 30 fps with zero rotation");
    }

    /**
     * Accepts exactly one SPS followed by one linked PPS, split or combined.
     * Repeated identical NAL bytes are allowed, including different Annex-B
     * start-code lengths/container splits; changed parameter sets are not.
     */
    public void acceptConfig(AvcCodecConfig config) {
        ensureOpen();
        try {
            require(config != null, "finite AVC codec config is missing");
            ByteBuffer first = config.csd0View();
            ByteBuffer second = config.csd1View();
            require(first.remaining() <= MAX_CODEC_CONFIG_BYTES - second.remaining(),
                    "finite AVC combined codec config exceeds 64 KiB");
            byte[] combined = new byte[first.remaining() + second.remaining()];
            int split = first.remaining();
            first.get(combined, 0, split);
            second.get(combined, split, second.remaining());
            ParameterSets candidate = new ParameterSets();
            visitNals(ByteBuffer.wrap(combined), (bytes, start, length, type) -> {
                if (type == 7) {
                    require(candidate.sps == null && candidate.pps == null,
                            "finite AVC requires one ordered SPS/PPS pair");
                    candidate.spsId = parseSps(bytes, start, length);
                    candidate.sps = copy(bytes, start, length);
                } else if (type == 8) {
                    require(candidate.sps != null && candidate.pps == null,
                            "finite AVC PPS must follow its SPS exactly once");
                    candidate.ppsId = parsePps(bytes, start, length, candidate.spsId);
                    candidate.pps = copy(bytes, start, length);
                } else {
                    throw invalid("finite AVC config contains a non-parameter-set NAL");
                }
            });
            require(candidate.sps != null && candidate.pps != null,
                    "finite AVC config requires both SPS and PPS");
            if (parameterSets == null) {
                parameterSets = candidate;
            } else {
                require(Arrays.equals(parameterSets.sps, candidate.sps)
                                && Arrays.equals(parameterSets.pps, candidate.pps),
                        "finite AVC parameter sets changed inside the epoch");
            }
        } catch (RuntimeException error) {
            failed = true;
            throw error;
        }
    }

    /** Validates one contiguous media AU or empty terminal EOS before forwarding. */
    public void acceptUnit(EncodedAvcAccessUnit unit) {
        ensureOpen();
        try {
            require(unit != null && parameterSets != null,
                    "finite AVC media requires validated codec config");
            require(!unit.codecConfig(), "route codec-config buffers to acceptConfig");
            require(lastSequence != Long.MAX_VALUE && unit.sequence() == lastSequence + 1,
                    "finite AVC media sequence must start at one and remain contiguous");
            require(unit.presentationTimeUs() <= Long.MAX_VALUE / 1000L,
                    "finite AVC timestamp exceeds receiver nanosecond bound");
            require(unit.presentationTimeUs() >= lastPresentationTimeUs
                            && (lastSequence == 0 || unit.endOfStream()
                                    || unit.presentationTimeUs() > lastPresentationTimeUs),
                    "finite AVC media presentation time must advance");
            require(unit.payloadLength() <= EncodedAvcAccessUnit.MAX_PAYLOAD_BYTES,
                    "finite AVC access unit exceeds 4 MiB");
            if (unit.endOfStream()) {
                require(lastSequence != 0 && unit.payloadLength() == 0 && !unit.keyFrame(),
                        "finite AVC EOS must be empty and follow media");
                ended = true;
            } else {
                require(lastSequence != 0 || unit.keyFrame(),
                        "finite AVC first media access unit must be an IDR");
                AccessUnitState state = new AccessUnitState();
                visitNals(unit.payloadView(), (bytes, start, length, type) -> {
                    if (type == 7 || type == 8) {
                        require(equal(bytes, start, length,
                                        type == 7 ? parameterSets.sps : parameterSets.pps),
                                "finite AVC in-band parameter set changed");
                    } else if (type == 1 || type == 5) {
                        parseSlice(bytes, start, length, type, parameterSets.ppsId, state);
                    } else {
                        require(type == 6 || type == 9,
                                "finite AVC NAL type is outside the supported subset");
                    }
                });
                require(state.slices > 0 && state.idr == unit.keyFrame(),
                        "finite AVC key-frame flag must match the actual IDR slices");
            }
            lastSequence = unit.sequence();
            lastPresentationTimeUs = unit.presentationTimeUs();
        } catch (RuntimeException error) {
            failed = true;
            throw error;
        }
    }

    private void ensureOpen() {
        if (failed || ended) {
            throw new IllegalStateException("finite AVC validator is terminal");
        }
    }

    private static int parseSps(ByteBuffer bytes, int start, int length) {
        require(length >= 4 && length <= MAX_CODEC_CONFIG_BYTES, "invalid finite AVC SPS size");
        Bits bits = rbsp(bytes, start + 1, length - 1);
        require(bits.read(8) == 66, "finite AVC requires Baseline profile (8-bit 4:2:0)");
        require((bits.read(8) & 3) == 0, "finite AVC SPS has reserved constraint bits");
        long level = bits.read(8);
        // Match the immutable finite MediaFormat request, not the broader receiver subset.
        require(level == 31, "finite AVC requires exact level 3.1");
        int id = bits.ue();
        require(id <= 31, "finite AVC SPS id is out of range");
        require(bits.ue() <= 12, "finite AVC frame-number width is out of range");
        int poc = bits.ue();
        if (poc == 0) {
            require(bits.ue() <= 12, "finite AVC picture-order width is out of range");
        } else if (poc == 1) {
            bits.read(1);
            bits.se();
            bits.se();
            int count = bits.ue();
            require(count <= 16, "finite AVC picture-order cycle is too large");
            for (int i = 0; i < count; i++) bits.se();
        } else {
            require(poc == 2, "finite AVC picture-order type is unsupported");
        }
        require(bits.ue() <= 4, "finite AVC has too many reference frames");
        bits.read(1);
        require(bits.ue() == 79 && bits.ue() == 44 && bits.read(1) == 1,
                "finite AVC SPS must be progressive exact 1280x720");
        bits.read(1);
        require(bits.read(1) == 0, "finite AVC SPS cropping is unsupported");
        require(bits.read(1) == 1, "finite AVC SPS must explicitly describe color and timing");
        parseVui(bits);
        bits.trailing();
        return id;
    }

    private static void parseVui(Bits bits) {
        if (bits.flag()) {
            long aspect = bits.read(8);
            if (aspect == 255) {
                long width = bits.read(16), height = bits.read(16);
                require(width != 0 && width == height, "finite AVC requires square samples");
            } else {
                require(aspect == 0 || aspect == 1, "finite AVC requires square samples");
            }
        }
        if (bits.flag()) bits.read(1); // overscan_appropriate_flag
        require(bits.flag(), "finite AVC VUI video signal description is missing");
        require(bits.read(3) <= 5, "finite AVC VUI video format is reserved");
        require(!bits.flag() && bits.flag(), "finite AVC requires explicit limited-range color");
        require(bits.read(8) == 1 && bits.read(8) == 1 && bits.read(8) == 1,
                "finite AVC requires explicit BT.709 primaries, transfer and matrix");
        if (bits.flag()) {
            require(bits.ue() <= 5 && bits.ue() <= 5, "finite AVC chroma sample location is invalid");
        }
        require(bits.flag(), "finite AVC SPS timing is missing");
        long units = bits.read(32), scale = bits.read(32);
        require(units != 0 && units * 60L == scale && bits.flag(),
                "finite AVC SPS timing must be fixed 30 fps");
        boolean nalHrd = bits.flag();
        if (nalHrd) parseHrd(bits);
        boolean vclHrd = bits.flag();
        if (vclHrd) parseHrd(bits);
        if (nalHrd || vclHrd) bits.read(1);
        bits.read(1); // pic_struct_present_flag; decoded output is independently checked.
        if (bits.flag()) {
            bits.read(1);
            require(bits.ue() <= 16 && bits.ue() <= 16
                            && bits.ue() <= 16 && bits.ue() <= 16,
                    "finite AVC VUI motion/denominator restriction is out of range");
            require(bits.ue() == 0 && bits.ue() <= 4,
                    "finite AVC VUI reordering or buffering exceeds the subset");
        }
    }

    private static void parseHrd(Bits bits) {
        int count = bits.ue();
        require(count <= 31, "finite AVC HRD entry count is too large");
        bits.read(4);
        bits.read(4);
        for (int i = 0; i <= count; i++) {
            bits.ue();
            bits.ue();
            bits.read(1);
        }
        for (int i = 0; i < 4; i++) bits.read(5);
    }

    private static int parsePps(ByteBuffer bytes, int start, int length, int spsId) {
        require(length >= 2 && length <= MAX_CODEC_CONFIG_BYTES, "invalid finite AVC PPS size");
        Bits bits = rbsp(bytes, start + 1, length - 1);
        int id = bits.ue();
        require(id <= 255 && bits.ue() == spsId, "finite AVC PPS must reference its pinned SPS");
        require(!bits.flag(), "finite Baseline AVC cannot use CABAC");
        require(!bits.flag() && bits.ue() == 0, "finite AVC fields and FMO are unsupported");
        require(bits.ue() <= 3 && bits.ue() <= 3, "finite AVC PPS reference count is too large");
        require(!bits.flag() && bits.read(2) == 0,
                "finite Baseline AVC cannot use weighted prediction");
        int qp = bits.se(), qs = bits.se(), chroma = bits.se();
        require(qp >= -26 && qp <= 25 && qs >= -26 && qs <= 25
                        && chroma >= -12 && chroma <= 12,
                "finite AVC PPS quantizer offsets are out of range");
        bits.read(1);
        bits.read(1);
        require(!bits.flag(), "finite AVC redundant pictures are unsupported");
        if (!bits.atTrailing()) {
            require(!bits.flag() && !bits.flag(),
                    "finite Baseline AVC cannot use 8x8 transforms or scaling matrices");
            chroma = bits.se();
            require(chroma >= -12 && chroma <= 12, "finite AVC second chroma offset is invalid");
        }
        bits.trailing();
        return id;
    }

    private static void parseSlice(ByteBuffer bytes, int start, int length, int type,
            int ppsId, AccessUnitState state) {
        int take = Math.min(length - 1, SLICE_PREFIX_BYTES);
        // Keep an emulation-prevention escape plus its next byte together.
        if (length - 1 > take && take >= 3 && unsigned(bytes.get(start + take)) == 3
                && bytes.get(start + take - 1) == 0 && bytes.get(start + take - 2) == 0) {
            take++;
        }
        Bits bits = rbsp(bytes, start + 1, take);
        int macroblock = bits.ue(), slice = bits.ue(), id = bits.ue();
        require(slice <= 9 && (slice % 5 == 0 || slice % 5 == 2)
                        && id == ppsId && macroblock < 3600,
                "finite AVC slice type, PPS or macroblock is unsupported");
        boolean idr = type == 5;
        require(!idr || slice % 5 == 2, "finite AVC IDR must carry I slices");
        require(state.slices == 0 ? macroblock == 0 : macroblock > state.lastMacroblock,
                "finite AVC slices must start at zero and strictly advance");
        require(state.slices == 0 || state.idr == idr, "finite AVC cannot mix IDR/non-IDR slices");
        state.slices++;
        state.lastMacroblock = macroblock;
        state.idr = idr;
    }

    private interface NalVisitor {
        void accept(ByteBuffer bytes, int start, int length, int type);
    }

    /** Linear scan, at most 256 NALs; no length-prefix/avcC fallback. */
    private static void visitNals(ByteBuffer source, NalVisitor visitor) {
        ByteBuffer bytes = source.slice();
        int size = bytes.remaining(), offset = 0, count = 0;
        require(prefix(bytes, 0) != 0, "finite AVC requires Annex B from byte zero");
        while (offset < size) {
            int prefix = prefix(bytes, offset);
            require(prefix != 0 && ++count <= MAX_NALS, "finite AVC NAL count/framing is invalid");
            int start = offset + prefix, end = start;
            while (end < size && prefix(bytes, end) == 0) end++;
            int trimmed = end;
            while (trimmed > start && bytes.get(trimmed - 1) == 0) trimmed--;
            require(trimmed > start, "finite AVC contains an empty NAL");
            int header = unsigned(bytes.get(start)), type = header & 31;
            require((header & 128) == 0
                            && ((type != 5 && type != 7 && type != 8) || (header & 96) != 0),
                    "finite AVC NAL header is invalid");
            visitor.accept(bytes, start, trimmed - start, type);
            offset = end;
        }
    }

    private static int prefix(ByteBuffer bytes, int offset) {
        int left = bytes.limit() - offset;
        if (left >= 3 && bytes.get(offset) == 0 && bytes.get(offset + 1) == 0) {
            if (bytes.get(offset + 2) == 1) return 3;
            if (left >= 4 && bytes.get(offset + 2) == 0 && bytes.get(offset + 3) == 1) return 4;
        }
        return 0;
    }

    private static Bits rbsp(ByteBuffer bytes, int start, int length) {
        require(length > 0 && length <= MAX_CODEC_CONFIG_BYTES, "finite AVC RBSP is empty or oversized");
        byte[] raw = new byte[length];
        int written = 0, zeros = 0;
        for (int i = 0; i < length; i++) {
            int value = unsigned(bytes.get(start + i));
            if (zeros >= 2 && value == 3) {
                require(i + 1 < length && unsigned(bytes.get(start + i + 1)) <= 3,
                        "finite AVC emulation-prevention escape is invalid");
                zeros = 0;
                continue;
            }
            require(zeros < 2 || value >= 3, "finite AVC unescaped RBSP byte is invalid");
            raw[written++] = (byte) value;
            zeros = value == 0 ? zeros + 1 : 0;
        }
        require(written != 0, "finite AVC RBSP is empty");
        return new Bits(raw, written);
    }

    private static byte[] copy(ByteBuffer bytes, int start, int length) {
        byte[] result = new byte[length];
        ByteBuffer view = bytes.duplicate();
        view.position(start);
        view.get(result);
        return result;
    }

    private static boolean equal(ByteBuffer bytes, int start, int length, byte[] expected) {
        if (length != expected.length) return false;
        for (int i = 0; i < length; i++) if (bytes.get(start + i) != expected[i]) return false;
        return true;
    }

    private static int unsigned(byte value) { return value & 0xff; }

    private static void require(boolean accepted, String reason) {
        if (!accepted) throw invalid(reason);
    }

    private static IllegalArgumentException invalid(String reason) {
        return new IllegalArgumentException(reason);
    }

    private static final class ParameterSets {
        byte[] sps;
        byte[] pps;
        int spsId;
        int ppsId;
    }

    private static final class AccessUnitState {
        int slices;
        int lastMacroblock;
        boolean idr;
    }

    private static final class Bits {
        private final byte[] bytes;
        private final int limit;
        private int position;

        Bits(byte[] bytes, int length) {
            this.bytes = bytes;
            limit = length * 8; // Input is bounded to 64 KiB.
        }

        long read(int count) {
            require(count >= 0 && count <= 32 && count <= limit - position,
                    "finite AVC bit field is truncated");
            long value = 0;
            for (int i = 0; i < count; i++, position++) {
                value = (value << 1) | ((bytes[position / 8] >> (7 - position % 8)) & 1);
            }
            return value;
        }

        boolean flag() { return read(1) != 0; }

        int ue() {
            int zeros = 0;
            while (!flag()) {
                require(++zeros <= 30, "finite AVC Exp-Golomb field exceeds 30 leading zeros");
            }
            return (int) ((1L << zeros) - 1 + read(zeros));
        }

        int se() {
            int value = ue();
            return (value & 1) != 0 ? (value + 1) / 2 : -(value / 2);
        }

        boolean atTrailing() {
            int left = limit - position;
            if (left < 1 || left > 8) return false;
            for (int i = position; i < limit; i++) {
                int actual = (bytes[i / 8] >> (7 - i % 8)) & 1;
                if (actual != (i == position ? 1 : 0)) return false;
            }
            return true;
        }

        void trailing() {
            require(atTrailing(), "finite AVC parameter-set RBSP trailing bits are invalid");
            position = limit;
        }
    }
}
