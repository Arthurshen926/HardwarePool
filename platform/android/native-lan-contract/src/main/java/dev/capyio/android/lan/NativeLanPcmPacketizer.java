package dev.capyio.android.lan;

import java.util.Objects;

/** Converts bounded PCM worker reads into exact common media packets. */
public final class NativeLanPcmPacketizer {
    public static final int MAX_PUSH_BYTES = 256 * 1024;

    private final NativeLanPacketCodec.Binding binding;
    private final NativeLanPacketQueue output;
    private final int sampleRate;
    private final int bytesPerFrame;
    private final int samplesPerPacket;
    private final int packetBytes;
    private final byte[] partial;

    private int partialBytes;
    private long nextSequence;
    private long nextSampleIndex;
    private long nextTimestampMicros;
    private long timestampRemainder;
    private boolean pendingDiscontinuity;
    private long emittedPackets;
    private long droppedPackets;
    private long consumedFrames;

    public NativeLanPcmPacketizer(
            NativeLanPacketCodec.Binding binding,
            NativeLanPacketQueue output,
            int sampleRate,
            int channels,
            int bytesPerSample,
            int frameDurationMicros,
            long initialSequence,
            long initialSampleIndex,
            long initialTimestampMicros) {
        this.binding = Objects.requireNonNull(binding, "binding");
        this.output = Objects.requireNonNull(output, "output");
        if (!binding.matches(output.binding())) {
            throw new IllegalArgumentException("packetizer and output queue bindings differ");
        }
        if (sampleRate < 8_000 || sampleRate > 384_000) {
            throw new IllegalArgumentException("sample rate outside 8000..=384000");
        }
        if (channels < 1 || channels > 32) {
            throw new IllegalArgumentException("channel count outside 1..=32");
        }
        if (bytesPerSample < 2 || bytesPerSample > 4) {
            throw new IllegalArgumentException("sample width outside 2..=4 bytes");
        }
        if (frameDurationMicros < 2_500 || frameDurationMicros > 60_000) {
            throw new IllegalArgumentException("frame duration outside 2500..=60000 us");
        }
        long sampleNumerator = (long) sampleRate * frameDurationMicros;
        if (sampleNumerator % 1_000_000L != 0) {
            throw new IllegalArgumentException("frame duration has a fractional sample count");
        }
        long computedSamples = sampleNumerator / 1_000_000L;
        long computedBytesPerFrame = (long) channels * bytesPerSample;
        long computedPacketBytes = computedSamples * computedBytesPerFrame;
        if (computedSamples < 1
                || computedSamples > Integer.MAX_VALUE
                || computedBytesPerFrame > Integer.MAX_VALUE
                || computedPacketBytes < 1
                || computedPacketBytes > NativeLanPacketCodec.MAX_PACKET_PAYLOAD_BYTES) {
            throw new IllegalArgumentException("PCM packet exceeds native LAN bounds");
        }
        this.sampleRate = sampleRate;
        bytesPerFrame = (int) computedBytesPerFrame;
        samplesPerPacket = (int) computedSamples;
        packetBytes = (int) computedPacketBytes;
        partial = new byte[packetBytes];
        nextSequence = initialSequence;
        nextSampleIndex = initialSampleIndex;
        nextTimestampMicros = initialTimestampMicros;
    }

    public synchronized PushResult push(byte[] input, int offset, int length) {
        Objects.requireNonNull(input, "input");
        if (offset < 0
                || length < 0
                || offset > input.length - length
                || length > MAX_PUSH_BYTES
                || length % bytesPerFrame != 0) {
            throw new IllegalArgumentException("PCM read is outside bounded frame alignment");
        }

        int cursor = offset;
        int remaining = length;
        int emitted = 0;
        int dropped = 0;
        while (remaining > 0) {
            int copied = Math.min(remaining, packetBytes - partialBytes);
            System.arraycopy(input, cursor, partial, partialBytes, copied);
            cursor += copied;
            remaining -= copied;
            partialBytes += copied;
            if (partialBytes == packetBytes) {
                NativeLanPacketCodec.Packet packet = new NativeLanPacketCodec.Packet(
                        binding.streamId,
                        binding.streamEpoch,
                        nextSequence,
                        nextTimestampMicros,
                        nextSampleIndex,
                        samplesPerPacket,
                        pendingDiscontinuity,
                        partial);
                NativeLanPacketQueue.OfferOutcome outcome = output.offer(packet);
                if (outcome == NativeLanPacketQueue.OfferOutcome.ACCEPTED) {
                    emitted++;
                    emittedPackets = saturatingIncrement(emittedPackets);
                    pendingDiscontinuity = false;
                } else if (outcome == NativeLanPacketQueue.OfferOutcome.WRONG_BINDING) {
                    throw new IllegalStateException("validated packetizer binding changed");
                } else {
                    dropped++;
                    droppedPackets = saturatingIncrement(droppedPackets);
                    pendingDiscontinuity = true;
                }
                nextSequence++;
                advanceTimeline(samplesPerPacket);
                partialBytes = 0;
            }
        }
        long frames = Integer.toUnsignedLong(length / bytesPerFrame);
        consumedFrames = saturatingAdd(consumedFrames, frames);
        return new PushResult(frames, emitted, dropped, partialBytes);
    }

    public synchronized void markDiscontinuity() {
        advanceTimeline(partialBytes / bytesPerFrame);
        partialBytes = 0;
        pendingDiscontinuity = true;
    }

    public synchronized Stats stats() {
        return new Stats(emittedPackets, droppedPackets, consumedFrames, partialBytes);
    }

    private static long saturatingIncrement(long value) {
        return value == Long.MAX_VALUE ? value : value + 1;
    }

    private void advanceTimeline(int frames) {
        nextSampleIndex += Integer.toUnsignedLong(frames);
        long numerator = Integer.toUnsignedLong(frames) * 1_000_000L + timestampRemainder;
        nextTimestampMicros += numerator / sampleRate;
        timestampRemainder = numerator % sampleRate;
    }

    private static long saturatingAdd(long value, long increment) {
        return Long.MAX_VALUE - value < increment ? Long.MAX_VALUE : value + increment;
    }

    public static final class PushResult {
        public final long consumedFrames;
        public final int emittedPackets;
        public final int droppedPackets;
        public final int bufferedBytes;

        private PushResult(
                long consumedFrames,
                int emittedPackets,
                int droppedPackets,
                int bufferedBytes) {
            this.consumedFrames = consumedFrames;
            this.emittedPackets = emittedPackets;
            this.droppedPackets = droppedPackets;
            this.bufferedBytes = bufferedBytes;
        }
    }

    public static final class Stats {
        public final long emittedPackets;
        public final long droppedPackets;
        public final long consumedFrames;
        public final int bufferedBytes;

        private Stats(
                long emittedPackets,
                long droppedPackets,
                long consumedFrames,
                int bufferedBytes) {
            this.emittedPackets = emittedPackets;
            this.droppedPackets = droppedPackets;
            this.consumedFrames = consumedFrames;
            this.bufferedBytes = bufferedBytes;
        }
    }
}
