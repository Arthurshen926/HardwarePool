package dev.capyio.android.lan;

import java.util.ArrayDeque;
import java.util.Objects;

/**
 * Non-blocking producer / bounded worker queue for complete native-LAN packets.
 *
 * <p>The queue may be used by Android's blocking audio worker threads, but not
 * by a real-time callback. It never evicts an accepted packet silently.</p>
 */
public final class NativeLanPacketQueue {
    public static final int MAX_PACKET_CAPACITY = 128;
    public static final int MAX_AGGREGATE_BYTES = 4 * 1024 * 1024;

    private final NativeLanPacketCodec.Binding binding;
    private final int packetCapacity;
    private final int byteCapacity;
    private final ArrayDeque<NativeLanPacketCodec.Packet> packets = new ArrayDeque<>();

    private int queuedBytes;
    private long acceptedPackets;
    private long polledPackets;
    private long fullPacketDrops;
    private long fullByteDrops;
    private long wrongBindingDrops;

    public NativeLanPacketQueue(
            NativeLanPacketCodec.Binding binding,
            int packetCapacity,
            int byteCapacity) {
        this.binding = Objects.requireNonNull(binding, "binding");
        if (packetCapacity < 1 || packetCapacity > MAX_PACKET_CAPACITY) {
            throw new IllegalArgumentException("packet capacity outside 1..=128");
        }
        if (byteCapacity < 1 || byteCapacity > MAX_AGGREGATE_BYTES) {
            throw new IllegalArgumentException("byte capacity outside 1..=4194304");
        }
        this.packetCapacity = packetCapacity;
        this.byteCapacity = byteCapacity;
    }

    public synchronized OfferOutcome offer(NativeLanPacketCodec.Packet packet) {
        Objects.requireNonNull(packet, "packet");
        if (!packet.streamId.equals(binding.streamId)
                || packet.streamEpoch != binding.streamEpoch) {
            wrongBindingDrops = saturatingIncrement(wrongBindingDrops);
            return OfferOutcome.WRONG_BINDING;
        }
        if (packets.size() == packetCapacity) {
            fullPacketDrops = saturatingIncrement(fullPacketDrops);
            return OfferOutcome.FULL_PACKETS;
        }
        if (packet.payloadLength() > byteCapacity - queuedBytes) {
            fullByteDrops = saturatingIncrement(fullByteDrops);
            return OfferOutcome.FULL_BYTES;
        }
        packets.addLast(packet);
        queuedBytes += packet.payloadLength();
        acceptedPackets = saturatingIncrement(acceptedPackets);
        notifyAll();
        return OfferOutcome.ACCEPTED;
    }

    /** Returns null on a bounded empty-queue timeout. */
    public synchronized NativeLanPacketCodec.Packet poll(long timeoutMillis)
            throws InterruptedException {
        if (timeoutMillis < 0 || timeoutMillis > 2_000) {
            throw new IllegalArgumentException("queue timeout outside 0..=2000 ms");
        }
        long remainingNanos = timeoutMillis * 1_000_000L;
        long deadline = System.nanoTime() + remainingNanos;
        while (packets.isEmpty() && remainingNanos > 0) {
            long waitMillis = remainingNanos / 1_000_000L;
            int waitNanos = (int) (remainingNanos % 1_000_000L);
            wait(waitMillis, waitNanos);
            remainingNanos = deadline - System.nanoTime();
        }
        NativeLanPacketCodec.Packet packet = packets.pollFirst();
        if (packet != null) {
            queuedBytes -= packet.payloadLength();
            polledPackets = saturatingIncrement(polledPackets);
        }
        return packet;
    }

    public synchronized void clear() {
        packets.clear();
        queuedBytes = 0;
        notifyAll();
    }

    public synchronized int size() {
        return packets.size();
    }

    public synchronized int queuedBytes() {
        return queuedBytes;
    }

    public NativeLanPacketCodec.Binding binding() {
        return binding;
    }

    public synchronized Stats stats() {
        return new Stats(
                acceptedPackets,
                polledPackets,
                fullPacketDrops,
                fullByteDrops,
                wrongBindingDrops);
    }

    private static long saturatingIncrement(long value) {
        return value == Long.MAX_VALUE ? value : value + 1;
    }

    public enum OfferOutcome {
        ACCEPTED,
        FULL_PACKETS,
        FULL_BYTES,
        WRONG_BINDING
    }

    public static final class Stats {
        public final long acceptedPackets;
        public final long polledPackets;
        public final long fullPacketDrops;
        public final long fullByteDrops;
        public final long wrongBindingDrops;

        private Stats(
                long acceptedPackets,
                long polledPackets,
                long fullPacketDrops,
                long fullByteDrops,
                long wrongBindingDrops) {
            this.acceptedPackets = acceptedPackets;
            this.polledPackets = polledPackets;
            this.fullPacketDrops = fullPacketDrops;
            this.fullByteDrops = fullByteDrops;
            this.wrongBindingDrops = wrongBindingDrops;
        }
    }
}
