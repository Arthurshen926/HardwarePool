package io.capyio.camera.contract;

import java.io.ByteArrayOutputStream;
import java.nio.ByteBuffer;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HexFormat;
import java.util.List;
import java.util.function.Consumer;

/**
 * Pure Java, no Android/native decoder/network/capture. The committed native
 * decoder fixture is the positive byte oracle; synthetic bit writers exercise
 * parser branches only and do not pretend that slice-header vectors decode.
 */
public final class FiniteCameraAvcValidatorContractTest {
    private static final String FIXTURE_SHA256 =
            "921e9ed4a85717cf3e404722700d9d55ecdfa6a77da952164701a68082fbc9d7";

    public static void main(String[] args) throws Exception {
        Fixture fixture = loadFixture(args);
        canonicalNativeFixture(fixture);
        canonicalParameterSetRepresentations(fixture);
        exactEncoderConfiguration();
        spsEvidenceIsMandatory();
        ppsMustBeLinkedBaseline();
        nalAndSliceBounds();
        sequenceTimestampAndTerminalState(fixture);
        allocationBounds(fixture);
        failureIsPermanent(fixture);
        System.out.println("Finite AVC validator contract tests: PASS");
    }

    private static void canonicalNativeFixture(Fixture fixture) {
        FiniteCameraAvcValidator validator = validator(fixture.config());
        for (EncodedAvcAccessUnit unit : fixture.units()) validator.acceptUnit(unit);
        reject(() -> validator.acceptConfig(fixture.config()), "EOS forbids reconfiguration");
        reject(() -> validator.acceptUnit(fixture.units().get(0)), "EOS forbids restart");
    }

    private static void canonicalParameterSetRepresentations(Fixture fixture) {
        FiniteCameraAvcValidator validator = validator(fixture.config());
        validator.acceptConfig(fixture.config());
        validator.acceptConfig(new AvcCodecConfig(concat(fixture.csd0(), fixture.csd1()), new byte[0]));
        validator.acceptConfig(new AvcCodecConfig(threeBytePrefix(fixture.csd0()),
                threeBytePrefix(fixture.csd1())));
        validator.acceptUnit(fixture.units().get(0));
        validator.acceptConfig(fixture.config()); // Repeated format cannot reset media state.
        validator.acceptUnit(fixture.units().get(1));

        byte[] changed = fixture.csd0().clone();
        changed[6] ^= 0x40; // A different valid constraint flag, not a container change.
        reject(() -> validator.acceptConfig(new AvcCodecConfig(changed, fixture.csd1())),
                "changed parameter set is terminal even when otherwise valid");
        reject(() -> validator.acceptConfig(fixture.config()), "original CSD cannot recover");

        FiniteCameraAvcValidator inline = validator(fixture.config());
        inline.acceptUnit(unit(1, 0, true,
                concat(fixture.csd0(), fixture.csd1(), payload(fixture.units().get(0)))));
        reject(() -> inline.acceptUnit(unit(2, 33333, true,
                concat(changed, payload(fixture.units().get(1))))), "changed in-band SPS");
    }

    private static void exactEncoderConfiguration() {
        new FiniteCameraAvcValidator(AvcEncoderConfig.baseline720p30());
        reject(() -> new FiniteCameraAvcValidator(null), "missing config");
        for (AvcEncoderConfig config : List.of(
                new AvcEncoderConfig(640, 720, 30, 4_000_000, 0, 1, 2),
                new AvcEncoderConfig(1280, 704, 30, 4_000_000, 0, 1, 2),
                new AvcEncoderConfig(1280, 720, 60, 4_000_000, 0, 1, 2),
                new AvcEncoderConfig(1280, 720, 30, 4_000_000, 90, 1, 2))) {
            reject(() -> new FiniteCameraAvcValidator(config), "noncanonical encoder metadata");
        }
    }

    private static void spsEvidenceIsMandatory() {
        validator(syntheticConfig(new Sps(), new Pps()));
        rejectSps("Main profile", s -> s.profile = 77);
        rejectSps("High profile / format not implicitly 8-bit 420", s -> s.profile = 100);
        rejectSps("reserved SPS constraint bits", s -> s.constraints = 1);
        rejectSps("level below canonical 720p30", s -> s.level = 30);
        rejectSps("level differs from requested actual MediaFormat", s -> s.level = 41);
        rejectSps("level above native ceiling", s -> s.level = 42);
        rejectSps("SPS id exceeds bound", s -> s.id = 32);
        rejectSps("wrong coded width", s -> s.widthMinusOne = 78);
        rejectSps("wrong coded height", s -> s.heightMinusOne = 45);
        rejectSps("interlaced field coding", s -> s.progressive = false);
        rejectSps("cropping flag, even zero offsets", s -> s.crop = true);
        rejectSps("too many references", s -> s.references = 5);
        rejectSps("absent VUI", s -> s.vui = false);
        rejectSps("absent video-signal information", s -> s.videoSignal = false);
        rejectSps("full-range rather than limited", s -> s.fullRange = true);
        rejectSps("absent color description", s -> s.colorDescription = false);
        rejectSps("unspecified primaries are not positive BT709 evidence", s -> s.primaries = 2);
        rejectSps("BT601 matrix", s -> s.matrix = 6);
        rejectSps("BT2020 primaries", s -> s.primaries = 9);
        rejectSps("PQ transfer is not SDR", s -> s.transfer = 16);
        rejectSps("HLG transfer is not SDR", s -> s.transfer = 18);
        rejectSps("missing frame timing", s -> s.timing = false);
        rejectSps("zero timing units", s -> s.units = 0);
        rejectSps("wrong frame rate", s -> s.scale = 50);
        rejectSps("variable-rate timing", s -> s.fixedTiming = false);
        rejectSps("frame reordering", s -> s.reorder = 1);
        rejectSps("excess decoded buffering", s -> s.buffering = 5);

        Sps largeTiming = new Sps();
        largeTiming.units = 71_582_788L;
        largeTiming.scale = 4_294_967_280L; // Unsigned 32-bit timing, no Java int overflow.
        validator(syntheticConfig(largeTiming, new Pps()));
        rejectSps("unsigned timing arithmetic cannot wrap", s -> {
            s.units = 0xffff_ffffL;
            s.scale = 0xffff_ffc4L;
        });

        byte[] sps = sps(new Sps());
        byte[] garbage = concat(sps, new byte[] {0x40});
        rejectConfig(new AvcCodecConfig(garbage, pps(new Pps())), "non-trailing extra SPS bits");
        rejectConfig(new AvcCodecConfig(Arrays.copyOf(sps, sps.length - 1), pps(new Pps())),
                "truncated SPS");
        rejectConfig(new AvcCodecConfig(new byte[] {0, 0, 1, 0x67, 66, 0, 31, 0, 0, 3},
                pps(new Pps())), "truncated emulation prevention");
        rejectConfig(new AvcCodecConfig(new byte[] {0, 0, 1, 0x67, 66, 0, 31, 0, 0, 3, 4},
                pps(new Pps())), "noncanonical emulation prevention");
        BitsWriter overflow = new BitsWriter();
        overflow.bits(66, 8).bits(0, 8).bits(31, 8).bits(0, 31).bits(1, 1);
        rejectConfig(new AvcCodecConfig(nal(0x67, overflow.finish()), pps(new Pps())),
                "Exp-Golomb over 30 leading zeros");
    }

    private static void ppsMustBeLinkedBaseline() {
        rejectPps("unlinked PPS", p -> p.spsId = 1);
        rejectPps("PPS id exceeds bound", p -> p.id = 256);
        rejectPps("CABAC cannot be Baseline", p -> p.cabac = true);
        rejectPps("field picture order", p -> p.fields = true);
        rejectPps("FMO slice groups", p -> p.groups = 1);
        rejectPps("excess reference count", p -> p.references = 4);
        rejectPps("weighted prediction outside Baseline", p -> p.weighted = true);
        rejectPps("weighted B prediction", p -> p.weightedB = 1);
        rejectPps("bad quantizer", p -> p.qp = 26);
        rejectPps("bad signed chroma offset", p -> p.chroma = -13);
        rejectPps("redundant pictures", p -> p.redundant = true);
        rejectPps("8x8 transform outside Baseline", p -> {
            p.extension = true;
            p.transform8x8 = true;
        });
        rejectPps("scaling matrix", p -> {
            p.extension = true;
            p.scaling = true;
        });
        Pps extension = new Pps();
        extension.extension = true;
        validator(syntheticConfig(new Sps(), extension));
        rejectConfig(new AvcCodecConfig(pps(new Pps()), sps(new Sps())), "PPS before SPS");
        rejectConfig(new AvcCodecConfig(sps(new Sps()), new byte[0]), "missing PPS");
        rejectConfig(new AvcCodecConfig(concat(sps(new Sps()), sps(new Sps())), pps(new Pps())),
                "duplicate SPS in config");
        rejectConfig(new AvcCodecConfig(sps(new Sps()), concat(pps(new Pps()), pps(new Pps()))),
                "duplicate PPS in config");
    }

    private static void nalAndSliceBounds() {
        byte[] idr = slice(5, 0, 2, 0);
        FiniteCameraAvcValidator progressive = validator(syntheticConfig(new Sps(), new Pps()));
        progressive.acceptUnit(unit(1, 0, true, idr));
        progressive.acceptUnit(unit(2, 33333, false, slice(1, 0, 0, 0)));
        progressive.acceptUnit(unit(3, 66666, false, slice(1, 0, 2, 0)));
        for (int kind : new int[] {1, 3, 4, 6, 8, 9}) {
            rejectAfterIdr(slice(1, 0, kind, 0), false, "B/SP/SI slice kind " + kind);
        }
        rejectFirst(slice(5, 0, 0, 0), true, "IDR with P slice");
        rejectFirst(slice(5, 0, 2, 1), true, "slice references other PPS");
        rejectFirst(slice(5, 1, 2, 0), true, "first slice not at macroblock zero");
        rejectFirst(concat(idr, slice(5, 0, 2, 0)), true, "two pictures / duplicate first macroblock");
        rejectFirst(concat(idr, slice(5, 50, 2, 0), slice(5, 20, 2, 0)), true,
                "macroblock order regressed");
        rejectFirst(concat(idr, slice(5, 3600, 2, 0)), true, "macroblock outside frame");
        rejectFirst(concat(idr, slice(1, 1, 2, 0)), true, "mixed IDR/non-IDR");
        rejectFirst(idr, false, "unflagged IDR");
        rejectFirst(slice(1, 0, 2, 0), true, "key-frame flag does not make non-IDR an IDR");
        rejectFirst(slice(1, 0, 0, 0), false, "first picture not IDR");
        rejectAfterIdr(slice(1, 0, 0, 0), true, "later key-frame flag mismatch");
        rejectFirst(new byte[] {0, 0, 0, 2, 0x65, (byte) 0xb8}, true, "length-prefix not Annex B");
        rejectFirst(new byte[] {1, 66, 0, 31, (byte) 0xff}, true, "avcC not Annex B");
        rejectFirst(new byte[] {0, 0, 1}, true, "empty NAL");
        rejectFirst(concat(new byte[] {0, 0, 1}, idr), true, "empty interposed NAL");
        rejectFirst(new byte[] {0, 0, 1, (byte) 0xe5, (byte) 0xb8}, true, "forbidden header bit");
        rejectFirst(new byte[] {0, 0, 1, 5, (byte) 0xb8}, true, "IDR without reference priority");
        rejectFirst(concat(new byte[] {0, 0, 1, 12, (byte) 0x80}, idr), true, "unsupported NAL");

        byte[] aud = new byte[] {0, 0, 1, 9, (byte) 0xf0};
        ByteArrayOutputStream maximum = new ByteArrayOutputStream();
        for (int i = 0; i < 255; i++) maximum.writeBytes(aud);
        maximum.writeBytes(idr);
        validator(syntheticConfig(new Sps(), new Pps())).acceptUnit(unit(1, 0, true, maximum.toByteArray()));
        rejectFirst(concat(aud, maximum.toByteArray()), true, "257 NALs exceeds fixed bound");

        byte[] inlinePps = pps(new Pps());
        inlinePps[inlinePps.length - 1] ^= 0x10;
        rejectFirst(concat(inlinePps, idr), true, "in-band PPS mutation");
    }

    private static void sequenceTimestampAndTerminalState(Fixture fixture) {
        EncodedAvcAccessUnit first = fixture.units().get(0);
        reject(() -> new FiniteCameraAvcValidator(AvcEncoderConfig.baseline720p30()).acceptUnit(first),
                "AU before config");
        reject(() -> validator(fixture.config()).acceptUnit(null), "null unit");
        reject(() -> validator(fixture.config()).acceptUnit(unit(2, 0, true, payload(first))),
                "sequence must start at one");
        reject(() -> validator(fixture.config()).acceptUnit(new EncodedAvcAccessUnit(1, 0, true,
                false, false, concat(fixture.csd0(), fixture.csd1()))), "config is not a media AU");
        reject(() -> validator(fixture.config()).acceptUnit(eos(1, 0)), "EOS before media");
        for (long sequence : new long[] {1, 3, Long.MAX_VALUE}) {
            FiniteCameraAvcValidator validator = validator(fixture.config());
            validator.acceptUnit(first);
            reject(() -> validator.acceptUnit(unit(sequence, 33333, true, payload(first))),
                    "duplicate, gap or overflow sequence");
        }
        for (long pts : new long[] {0, Long.MAX_VALUE / 1000L + 1, Long.MAX_VALUE}) {
            FiniteCameraAvcValidator validator = validator(fixture.config());
            validator.acceptUnit(first);
            reject(() -> validator.acceptUnit(unit(2, pts, true, payload(first))),
                    "duplicate/overflow media PTS");
        }
        FiniteCameraAvcValidator regression = validator(fixture.config());
        regression.acceptUnit(unit(1, 2, true, payload(first)));
        reject(() -> regression.acceptUnit(unit(2, 1, true, payload(first))), "regressing media PTS");
        FiniteCameraAvcValidator eosRegression = validator(fixture.config());
        eosRegression.acceptUnit(unit(1, 2, true, payload(first)));
        reject(() -> eosRegression.acceptUnit(eos(2, 1)), "regressing EOS PTS");
        FiniteCameraAvcValidator validEos = validator(fixture.config());
        validEos.acceptUnit(first);
        validEos.acceptUnit(eos(2, first.presentationTimeUs()));
        reject(() -> validEos.acceptUnit(eos(3, 1)), "repeated EOS");
        for (EncodedAvcAccessUnit invalid : List.of(
                new EncodedAvcAccessUnit(2, 0, false, true, true, new byte[0]),
                new EncodedAvcAccessUnit(2, 0, false, false, true, payload(first)))) {
            FiniteCameraAvcValidator validator = validator(fixture.config());
            validator.acceptUnit(first);
            reject(() -> validator.acceptUnit(invalid), "EOS must be empty and non-key");
        }
    }

    private static void allocationBounds(Fixture fixture) {
        byte[] exact = Arrays.copyOf(concat(fixture.csd0(), fixture.csd1()),
                FiniteCameraAvcValidator.MAX_CODEC_CONFIG_BYTES);
        validator(new AvcCodecConfig(exact, new byte[0]));
        rejectConfig(new AvcCodecConfig(exact, new byte[] {1}), "combined CSD limit before parsing");
        byte[] exactAu = Arrays.copyOf(payload(fixture.units().get(0)), EncodedAvcAccessUnit.MAX_PAYLOAD_BYTES);
        validator(fixture.config()).acceptUnit(unit(1, 0, true, exactAu));
        reject(() -> unit(1, 0, true, new byte[EncodedAvcAccessUnit.MAX_PAYLOAD_BYTES + 1]),
                "DTO rejects an AU above allocation ceiling");
    }

    private static void failureIsPermanent(Fixture fixture) {
        FiniteCameraAvcValidator badConfig = new FiniteCameraAvcValidator(AvcEncoderConfig.baseline720p30());
        reject(() -> badConfig.acceptConfig(null), "missing config latches failure");
        reject(() -> badConfig.acceptConfig(fixture.config()), "config failure cannot be repaired");
        reject(() -> badConfig.acceptUnit(fixture.units().get(0)), "unit cannot bypass config failure");
        FiniteCameraAvcValidator badUnit = validator(fixture.config());
        reject(() -> badUnit.acceptUnit(unit(1, 0, true, slice(5, 0, 1, 0))), "malformed first unit");
        reject(() -> badUnit.acceptUnit(fixture.units().get(0)), "unit failure cannot reset sequence");
        reject(() -> badUnit.acceptConfig(fixture.config()), "repeated config cannot reset failure");
    }

    static Fixture loadFixture(String... args) throws Exception {
        Path path;
        if (args.length == 1) {
            path = Path.of(args[0]);
        } else {
            check(args.length == 0, "expected optional canonical fixture path");
            path = null;
            for (Path root = Path.of("").toAbsolutePath(); root != null; root = root.getParent()) {
                Path candidate = root.resolve("fixtures/linux-avc/solid-720p30.cavc");
                if (Files.isRegularFile(candidate)) {
                    path = candidate;
                    break;
                }
            }
            check(path != null, "run from repository/module or supply the canonical fixture path");
        }
        check(Files.size(path) == 9095, "canonical fixture length changed");
        byte[] source = Files.readAllBytes(path);
        check(HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(source))
                .equals(FIXTURE_SHA256), "canonical fixture bytes changed; review native/Java oracle together");
        ByteBuffer data = ByteBuffer.wrap(source);
        byte[] csd0 = null, csd1 = null;
        List<EncodedAvcAccessUnit> units = new ArrayList<>();
        while (data.hasRemaining()) {
            check(data.remaining() >= 56, "truncated fixture header");
            int recordStart = data.position();
            check(data.getInt() == 0x43415643 && data.get() == 1 && data.get() == 1, "fixture CAVC version");
            int kind = data.get() & 255, flags = data.get() & 255;
            check(data.getShort() == 56 && data.getShort() == 0, "fixture header length/reserved");
            data.position(recordStart + 28);
            check(data.getLong() == 1, "fixture epoch");
            long sequence = data.getLong(), pts = data.getLong();
            int length = data.getInt();
            check(length >= 0 && length <= data.remaining(), "fixture record size");
            byte[] payload = new byte[length];
            data.get(payload);
            if (kind == 1) {
                check(csd0 == null && units.isEmpty() && flags == 0 && sequence == 0 && pts == 0,
                        "one initial fixture config");
                ByteBuffer config = ByteBuffer.wrap(payload);
                check(config.getShort() == 1280 && config.getShort() == 720
                        && config.getShort() == 30 && config.getShort() == 0,
                        "fixture declared dimensions/rate");
                config.position(20);
                int c0 = config.getInt(), c1 = config.getInt();
                check(c0 > 0 && c1 > 0 && c0 + c1 == config.remaining(), "fixture CSD lengths");
                csd0 = new byte[c0];
                csd1 = new byte[c1];
                config.get(csd0).get(csd1);
            } else {
                check(kind == 2 && (flags == 1 || flags == 2), "fixture media flags");
                units.add(new EncodedAvcAccessUnit(sequence, pts, false,
                        flags == 1, flags == 2, payload));
            }
        }
        check(csd0 != null && csd1 != null && units.size() == 4 && units.get(3).endOfStream(),
                "canonical fixture must carry config/3 AUs/EOS");
        return new Fixture(csd0, csd1, List.copyOf(units));
    }

    static record Fixture(byte[] csd0, byte[] csd1, List<EncodedAvcAccessUnit> units) {
        AvcCodecConfig config() { return new AvcCodecConfig(csd0, csd1); }
    }

    private static FiniteCameraAvcValidator validator(AvcCodecConfig config) {
        FiniteCameraAvcValidator validator = new FiniteCameraAvcValidator(AvcEncoderConfig.baseline720p30());
        validator.acceptConfig(config);
        return validator;
    }

    private static EncodedAvcAccessUnit unit(long sequence, long pts, boolean key, byte[] bytes) {
        return new EncodedAvcAccessUnit(sequence, pts, false, key, false, bytes);
    }

    private static EncodedAvcAccessUnit eos(long sequence, long pts) {
        return new EncodedAvcAccessUnit(sequence, pts, false, false, true, new byte[0]);
    }

    static byte[] payload(EncodedAvcAccessUnit unit) {
        byte[] payload = new byte[unit.payloadLength()];
        unit.payloadView().get(payload);
        return payload;
    }

    private static void rejectConfig(AvcCodecConfig config, String reason) {
        reject(() -> validator(config), reason);
    }

    private static void rejectSps(String reason, Consumer<Sps> mutation) {
        Sps options = new Sps();
        mutation.accept(options);
        rejectConfig(syntheticConfig(options, new Pps()), reason);
    }

    private static void rejectPps(String reason, Consumer<Pps> mutation) {
        Pps options = new Pps();
        mutation.accept(options);
        rejectConfig(syntheticConfig(new Sps(), options), reason);
    }

    private static void rejectFirst(byte[] payload, boolean key, String reason) {
        reject(() -> validator(syntheticConfig(new Sps(), new Pps()))
                .acceptUnit(unit(1, 0, key, payload)), reason);
    }

    private static void rejectAfterIdr(byte[] payload, boolean key, String reason) {
        FiniteCameraAvcValidator validator = validator(syntheticConfig(new Sps(), new Pps()));
        validator.acceptUnit(unit(1, 0, true, slice(5, 0, 2, 0)));
        reject(() -> validator.acceptUnit(unit(2, 33333, key, payload)), reason);
    }

    private static AvcCodecConfig syntheticConfig(Sps sps, Pps pps) {
        return new AvcCodecConfig(sps(sps), pps(pps));
    }

    private static final class Sps {
        int profile = 66, constraints, level = 31, id, references = 1;
        int widthMinusOne = 79, heightMinusOne = 44;
        boolean progressive = true, crop, vui = true, videoSignal = true;
        boolean fullRange, colorDescription = true, timing = true, fixedTiming = true;
        int primaries = 1, transfer = 1, matrix = 1, reorder, buffering = 1;
        long units = 1, scale = 60;
    }

    private static byte[] sps(Sps s) {
        BitsWriter b = new BitsWriter();
        b.bits(s.profile, 8).bits(s.constraints, 8).bits(s.level, 8).ue(s.id);
        b.ue(0).ue(2).ue(s.references).bit(false).ue(s.widthMinusOne).ue(s.heightMinusOne);
        b.bit(s.progressive);
        if (!s.progressive) b.bit(false);
        b.bit(true).bit(s.crop);
        if (s.crop) b.ue(0).ue(0).ue(0).ue(0);
        b.bit(s.vui);
        if (s.vui) {
            b.bit(false).bit(false).bit(s.videoSignal);
            if (s.videoSignal) {
                b.bits(5, 3).bit(s.fullRange).bit(s.colorDescription);
                if (s.colorDescription) b.bits(s.primaries, 8).bits(s.transfer, 8).bits(s.matrix, 8);
            }
            b.bit(false).bit(s.timing);
            if (s.timing) b.bits(s.units, 32).bits(s.scale, 32).bit(s.fixedTiming);
            b.bit(false).bit(false).bit(false).bit(true).bit(true);
            b.ue(0).ue(0).ue(0).ue(0).ue(s.reorder).ue(s.buffering);
        }
        return nal(0x67, b.finish());
    }

    private static final class Pps {
        int id, spsId, groups, references, weightedB, qp, chroma;
        boolean cabac, fields, weighted, redundant, extension, transform8x8, scaling;
    }

    private static byte[] pps(Pps p) {
        BitsWriter b = new BitsWriter();
        b.ue(p.id).ue(p.spsId).bit(p.cabac).bit(p.fields).ue(p.groups);
        b.ue(p.references).ue(0).bit(p.weighted).bits(p.weightedB, 2);
        b.se(p.qp).se(0).se(p.chroma).bit(true).bit(false).bit(p.redundant);
        if (p.extension) b.bit(p.transform8x8).bit(p.scaling).se(0);
        return nal(0x68, b.finish());
    }

    private static byte[] slice(int type, int macroblock, int sliceType, int ppsId) {
        return nal(0x60 | type, new BitsWriter().ue(macroblock).ue(sliceType).ue(ppsId).finish());
    }

    private static byte[] nal(int header, byte[] raw) {
        ByteArrayOutputStream bytes = new ByteArrayOutputStream();
        bytes.writeBytes(new byte[] {0, 0, 0, 1, (byte) header});
        int zeros = 0;
        for (byte value : raw) {
            int next = value & 255;
            if (zeros >= 2 && next <= 3) {
                bytes.write(3);
                zeros = 0;
            }
            bytes.write(next);
            zeros = next == 0 ? zeros + 1 : 0;
        }
        return bytes.toByteArray();
    }

    private static byte[] threeBytePrefix(byte[] original) {
        check(original[0] == 0 && original[1] == 0 && original[2] == 0 && original[3] == 1,
                "fixture has four-byte prefix");
        return Arrays.copyOfRange(original, 1, original.length);
    }

    static byte[] concat(byte[]... parts) {
        ByteArrayOutputStream bytes = new ByteArrayOutputStream();
        for (byte[] part : parts) bytes.writeBytes(part);
        return bytes.toByteArray();
    }

    private static final class BitsWriter {
        private final ByteArrayOutputStream bytes = new ByteArrayOutputStream();
        private int current, count;

        BitsWriter bits(long value, int length) {
            for (int shift = length - 1; shift >= 0; shift--) {
                current = (current << 1) | (int) ((value >>> shift) & 1);
                if (++count == 8) {
                    bytes.write(current);
                    current = count = 0;
                }
            }
            return this;
        }

        BitsWriter bit(boolean value) { return bits(value ? 1 : 0, 1); }

        BitsWriter ue(int value) {
            long code = (long) value + 1;
            int width = 63 - Long.numberOfLeadingZeros(code);
            return bits(0, width).bits(code, width + 1);
        }

        BitsWriter se(int value) { return ue(value <= 0 ? -2 * value : 2 * value - 1); }

        byte[] finish() {
            bit(true);
            while (count != 0) bit(false);
            return bytes.toByteArray();
        }
    }

    static void reject(Runnable action, String reason) {
        try {
            action.run();
        } catch (IllegalArgumentException | IllegalStateException expected) {
            return;
        }
        throw new AssertionError("accepted invalid finite AVC: " + reason);
    }

    private static void check(boolean value, String reason) {
        if (!value) throw new AssertionError(reason);
    }
}
