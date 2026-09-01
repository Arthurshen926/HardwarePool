package dev.capyio.android.lan;

import java.io.IOException;
import java.net.SocketTimeoutException;
import java.util.Objects;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.function.Consumer;

/** Owns UDP, reassembly and complete-packet queueing on one background worker. */
public final class NativeLanReceiverWorker implements AutoCloseable {
    public static final String PROBLEM_IO = "CAPY.ANDROID.LAN_RECEIVE_IO";

    private final NativeLanUdpEndpoint endpoint;
    private final NativeLanPacketReassembler reassembler;
    private final NativeLanPacketQueue output;
    private final Consumer<String> failureListener;
    private final AtomicBoolean running = new AtomicBoolean();
    private Thread worker;
    private boolean started;
    private boolean stopped;

    public NativeLanReceiverWorker(
            NativeLanUdpEndpoint endpoint,
            NativeLanPacketReassembler reassembler,
            NativeLanPacketQueue output,
            Consumer<String> failureListener) {
        this.endpoint = Objects.requireNonNull(endpoint, "endpoint");
        this.reassembler = Objects.requireNonNull(reassembler, "reassembler");
        this.output = Objects.requireNonNull(output, "output");
        this.failureListener = Objects.requireNonNull(failureListener, "failureListener");
        if (!endpoint.binding().matches(reassembler.binding())
                || !endpoint.binding().matches(output.binding())) {
            throw new IllegalArgumentException("receiver endpoint, reassembler and queue differ");
        }
    }

    public synchronized void start() {
        if (started || stopped) {
            throw new IllegalStateException("native LAN receiver is one-shot");
        }
        started = true;
        running.set(true);
        Thread next = new Thread(this::run, "capyio-native-lan-receiver");
        next.setDaemon(true);
        worker = next;
        next.start();
    }

    public void stop() {
        Thread current;
        synchronized (this) {
            current = worker;
            stopped = true;
            running.set(false);
        }
        endpoint.close();
        if (current != null && current != Thread.currentThread()) {
            current.interrupt();
            joinBounded(current);
        }
    }

    public boolean isRunning() {
        return running.get();
    }

    public Stats stats() {
        NativeLanUdpEndpoint.Metrics endpointMetrics = endpoint.metrics();
        NativeLanPacketReassembler.Stats reassemblyMetrics = reassembler.stats();
        NativeLanPacketQueue.Stats queueMetrics = output.stats();
        return new Stats(
                endpointMetrics.datagramsReceived,
                endpointMetrics.wrongPeerDatagrams,
                endpointMetrics.malformedDatagrams,
                reassemblyMetrics.completedPackets,
                reassemblyMetrics.partialEvictions,
                queueMetrics.fullPacketDrops,
                queueMetrics.fullByteDrops);
    }

    @Override
    public void close() {
        stop();
    }

    private void run() {
        try {
            while (running.get()) {
                NativeLanUdpEndpoint.ReceiveOutcome received;
                try {
                    received = endpoint.receive();
                } catch (SocketTimeoutException timeout) {
                    continue;
                }
                if (received.kind != NativeLanUdpEndpoint.ReceiveOutcome.Kind.FRAGMENT) {
                    continue;
                }
                NativeLanPacketReassembler.Outcome reassembled =
                        reassembler.accept(received.fragment);
                if (reassembled.kind == NativeLanPacketReassembler.Kind.COMPLETE) {
                    output.offer(reassembled.packet);
                }
            }
        } catch (IOException failure) {
            if (running.getAndSet(false)) {
                failureListener.accept(PROBLEM_IO);
            }
        } finally {
            running.set(false);
            endpoint.close();
            synchronized (this) {
                worker = null;
            }
        }
    }

    private static void joinBounded(Thread thread) {
        try {
            thread.join(2_000);
        } catch (InterruptedException interrupted) {
            Thread.currentThread().interrupt();
        }
        if (thread.isAlive()) {
            throw new IllegalStateException("native LAN receiver did not stop within 2 seconds");
        }
    }

    public static final class Stats {
        public final long datagramsReceived;
        public final long wrongPeerDatagrams;
        public final long malformedDatagrams;
        public final long completedPackets;
        public final long partialEvictions;
        public final long fullPacketDrops;
        public final long fullByteDrops;

        private Stats(
                long datagramsReceived,
                long wrongPeerDatagrams,
                long malformedDatagrams,
                long completedPackets,
                long partialEvictions,
                long fullPacketDrops,
                long fullByteDrops) {
            this.datagramsReceived = datagramsReceived;
            this.wrongPeerDatagrams = wrongPeerDatagrams;
            this.malformedDatagrams = malformedDatagrams;
            this.completedPackets = completedPackets;
            this.partialEvictions = partialEvictions;
            this.fullPacketDrops = fullPacketDrops;
            this.fullByteDrops = fullByteDrops;
        }
    }
}
