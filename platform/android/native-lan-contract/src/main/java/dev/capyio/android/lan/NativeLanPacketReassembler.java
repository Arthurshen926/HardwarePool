package dev.capyio.android.lan;

import java.util.Arrays;
import java.util.Objects;
import java.util.TreeMap;

/** Bounded worker-thread reassembly for one pre-bound native audio Route. */
public final class NativeLanPacketReassembler {
    public static final int MAX_INFLIGHT_PACKETS = 8;

    private final NativeLanPacketCodec.Binding binding;
    private final int inflightCapacity;
    private final TreeMap<Long, PartialPacket> partial =
            new TreeMap<>(Long::compareUnsigned);

    private long acceptedFragments;
    private long completedPackets;
    private long duplicateFragments;
    private long wrongBindingFragments;
    private long malformedFragments;
    private long partialEvictions;

    public NativeLanPacketReassembler(
            NativeLanPacketCodec.Binding binding,
            int inflightCapacity) {
        this.binding = Objects.requireNonNull(binding, "binding");
        if (inflightCapacity < 1 || inflightCapacity > MAX_INFLIGHT_PACKETS) {
            throw new IllegalArgumentException("in-flight capacity outside 1..=8");
        }
        this.inflightCapacity = inflightCapacity;
    }

    public synchronized Outcome accept(NativeLanPacketCodec.Fragment fragment) {
        Objects.requireNonNull(fragment, "fragment");
        if (!fragment.matches(binding)) {
            wrongBindingFragments = saturatingIncrement(wrongBindingFragments);
            return Outcome.of(Kind.WRONG_BINDING, null);
        }

        PartialPacket packet = partial.get(fragment.sequence);
        if (packet == null) {
            if (partial.size() == inflightCapacity) {
                partial.pollFirstEntry();
                partialEvictions = saturatingIncrement(partialEvictions);
            }
            packet = new PartialPacket(fragment);
            partial.put(fragment.sequence, packet);
        } else if (!packet.metadataMatches(fragment)) {
            partial.remove(fragment.sequence);
            malformedFragments = saturatingIncrement(malformedFragments);
            return Outcome.of(Kind.MALFORMED, null);
        }

        long bit = 1L << fragment.fragmentIndex;
        byte[] fragmentPayload = fragment.payloadCopy();
        if ((packet.receivedFragments & bit) != 0) {
            byte[] prior = Arrays.copyOfRange(
                    packet.payload,
                    fragment.fragmentOffset,
                    fragment.fragmentOffset + fragmentPayload.length);
            if (!Arrays.equals(prior, fragmentPayload)) {
                partial.remove(fragment.sequence);
                malformedFragments = saturatingIncrement(malformedFragments);
                return Outcome.of(Kind.MALFORMED, null);
            }
            duplicateFragments = saturatingIncrement(duplicateFragments);
            return Outcome.of(Kind.DUPLICATE, null);
        }

        System.arraycopy(
                fragmentPayload,
                0,
                packet.payload,
                fragment.fragmentOffset,
                fragmentPayload.length);
        packet.receivedFragments |= bit;
        acceptedFragments = saturatingIncrement(acceptedFragments);
        if (!packet.complete()) {
            return Outcome.of(Kind.PENDING, null);
        }

        partial.remove(fragment.sequence);
        NativeLanPacketCodec.Packet complete = new NativeLanPacketCodec.Packet(
                binding.streamId,
                binding.streamEpoch,
                fragment.sequence,
                packet.sourceTimestampMicros,
                packet.firstSampleIndex,
                packet.sampleCount,
                packet.discontinuity,
                packet.payload);
        completedPackets = saturatingIncrement(completedPackets);
        return Outcome.of(Kind.COMPLETE, complete);
    }

    public synchronized int inflightPackets() {
        return partial.size();
    }

    public NativeLanPacketCodec.Binding binding() {
        return binding;
    }

    public synchronized Stats stats() {
        return new Stats(
                acceptedFragments,
                completedPackets,
                duplicateFragments,
                wrongBindingFragments,
                malformedFragments,
                partialEvictions);
    }

    private static long saturatingIncrement(long value) {
        return value == Long.MAX_VALUE ? value : value + 1;
    }

    public enum Kind {
        PENDING,
        COMPLETE,
        DUPLICATE,
        WRONG_BINDING,
        MALFORMED
    }

    public static final class Outcome {
        public final Kind kind;
        public final NativeLanPacketCodec.Packet packet;

        private Outcome(Kind kind, NativeLanPacketCodec.Packet packet) {
            this.kind = kind;
            this.packet = packet;
        }

        private static Outcome of(Kind kind, NativeLanPacketCodec.Packet packet) {
            return new Outcome(kind, packet);
        }
    }

    public static final class Stats {
        public final long acceptedFragments;
        public final long completedPackets;
        public final long duplicateFragments;
        public final long wrongBindingFragments;
        public final long malformedFragments;
        public final long partialEvictions;

        private Stats(
                long acceptedFragments,
                long completedPackets,
                long duplicateFragments,
                long wrongBindingFragments,
                long malformedFragments,
                long partialEvictions) {
            this.acceptedFragments = acceptedFragments;
            this.completedPackets = completedPackets;
            this.duplicateFragments = duplicateFragments;
            this.wrongBindingFragments = wrongBindingFragments;
            this.malformedFragments = malformedFragments;
            this.partialEvictions = partialEvictions;
        }
    }

    private static final class PartialPacket {
        private final long sourceTimestampMicros;
        private final long firstSampleIndex;
        private final int sampleCount;
        private final boolean discontinuity;
        private final int fragmentCount;
        private final byte[] payload;
        private long receivedFragments;

        private PartialPacket(NativeLanPacketCodec.Fragment fragment) {
            sourceTimestampMicros = fragment.sourceTimestampMicros;
            firstSampleIndex = fragment.firstSampleIndex;
            sampleCount = fragment.sampleCount;
            discontinuity = fragment.discontinuity;
            fragmentCount = fragment.fragmentCount;
            payload = new byte[fragment.totalPayloadBytes];
        }

        private boolean metadataMatches(NativeLanPacketCodec.Fragment fragment) {
            return sourceTimestampMicros == fragment.sourceTimestampMicros
                    && firstSampleIndex == fragment.firstSampleIndex
                    && sampleCount == fragment.sampleCount
                    && discontinuity == fragment.discontinuity
                    && fragmentCount == fragment.fragmentCount
                    && payload.length == fragment.totalPayloadBytes;
        }

        private boolean complete() {
            long expected = fragmentCount == 64 ? -1L : (1L << fragmentCount) - 1;
            return receivedFragments == expected;
        }
    }
}
