package io.capyio.camera.contract;

import java.util.List;
import java.util.Objects;
import java.util.concurrent.ConcurrentLinkedQueue;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicReference;
import java.util.concurrent.locks.LockSupport;

/**
 * One-shot secure CAVC exporter. No sockets, retries, reconnect or drop-oldest behavior.
 * Reserve on the codec callback BEFORE copying its output; offerReserved/cancelReservation
 * must consume that reservation on the same callback thread. Permits include copying,
 * queued and in-flight AUs, so the producer never outruns the two-AU bound.
 *
 * Payload live-set budget (not a total VM/GC heap bound): two 4 MiB owned AUs,
 * two conservatively counted 4 MiB callback scratch copies, worker payload clone
 * 4 MiB + record 4 MiB, <= 1 MiB config copies, native budget 20 MiB = < 45 MiB.
 * A conservative 64 MiB application-owned payload budget leaves headroom for headers
 * and bounded bookkeeping. Uncollected garbage and OS codec/driver buffers are NOT
 * bounded by this queue and are not included in that application live-set claim.
 */
public final class SecureAvcSender implements AutoCloseable {
    public interface Listener {
        void onStatus(String state, long sentAccessUnits, long droppedAccessUnits);
    }

    public static final int CAPACITY = 2;
    private final SecureCameraTransport transport;
    private final long generation;
    private final long expiresAtMs;
    private final long createdAtNanos;
    private final long lifetimeNanos;
    private final Listener listener;
    private final AvcWireSessionEncoder encoder;
    private final AtomicInteger lifecycle = new AtomicInteger(); // 0 new, 1 running, 2 terminal
    private final AtomicInteger reservations = new AtomicInteger();
    private final ConcurrentLinkedQueue<EncodedAvcAccessUnit> queue = new ConcurrentLinkedQueue<>();
    private final AtomicReference<AvcCodecConfig> codecConfig = new AtomicReference<>();
    private final ThreadLocal<Integer> copyingBytes = new ThreadLocal<>();
    private volatile Thread worker;
    private volatile String terminalStatus = "secure export stopped";
    private long sent;

    public SecureAvcSender(SecureCameraTransport transport, long generation,
            AvcWireRecordEncoder.StreamKey stream, AvcEncoderConfig encoderConfig,
            long expiresAtMs, Listener listener) {
        if (generation <= 0 || expiresAtMs <= 0) throw new IllegalArgumentException("invalid secure generation");
        this.transport = Objects.requireNonNull(transport, "transport");
        this.generation = generation;
        this.expiresAtMs = expiresAtMs;
        createdAtNanos = System.nanoTime();
        lifetimeNanos = TimeUnit.MILLISECONDS.toNanos(
                Math.max(0, expiresAtMs - System.currentTimeMillis()));
        this.listener = Objects.requireNonNull(listener, "listener");
        encoder = new AvcWireSessionEncoder(stream, encoderConfig);
    }

    public void start() {
        if (!lifecycle.compareAndSet(0, 1)) throw new IllegalStateException("secure sender is one-shot");
        if (!live()) {
            terminate("secure transport is not ready");
            return;
        }
        Thread created = new Thread(this::runWorker, "CapyIO-Secure-AVC");
        worker = created;
        created.start();
    }

    public void setCodecConfig(AvcCodecConfig config) {
        if (lifecycle.get() == 1) codecConfig.compareAndSet(null, Objects.requireNonNull(config, "config"));
    }

    /** No payload allocation or copying is allowed until this returns true. */
    public boolean reserveOutput(int payloadBytes) {
        if (copyingBytes.get() != null) throw new IllegalStateException("unconsumed codec reservation");
        if (payloadBytes < 0 || payloadBytes > EncodedAvcAccessUnit.MAX_PAYLOAD_BYTES) {
            terminate("secure output exceeds bound");
            return false;
        }
        if (!live()) {
            terminate("secure generation unavailable or expired");
            return false;
        }
        // One attempt: callbacks never wait or spin on contention.
        int current = reservations.get();
        if (current >= CAPACITY || !reservations.compareAndSet(current, current + 1)) {
            terminate("secure output queue overflow");
            return false;
        }
        copyingBytes.set(payloadBytes);
        if (lifecycle.get() != 1) {
            cancelReservation();
            return false;
        }
        return true;
    }

    public void offerReserved(EncodedAvcAccessUnit unit) {
        Integer bytes = copyingBytes.get();
        if (bytes == null) throw new IllegalStateException("reserve before copying codec output");
        if (unit == null || bytes != unit.payloadLength()) {
            cancelReservation();
            terminate("secure output reservation mismatch");
            return;
        }
        copyingBytes.remove();
        if (lifecycle.get() != 1) {
            reservations.decrementAndGet();
            return;
        }
        queue.offer(unit);
        if (lifecycle.get() != 1) drainQueue();
        LockSupport.unpark(worker);
    }

    public void cancelReservation() {
        if (copyingBytes.get() != null) {
            copyingBytes.remove();
            reservations.decrementAndGet();
        }
    }

    /** For already bounded non-codec producers only; real MediaCodec uses reserveOutput first. */
    public void offer(EncodedAvcAccessUnit unit) {
        Objects.requireNonNull(unit, "unit");
        if (reserveOutput(unit.payloadLength())) offerReserved(unit);
    }

    private boolean live() {
        return lifecycle.get() == 1 && System.currentTimeMillis() < expiresAtMs
                && System.nanoTime() - createdAtNanos < lifetimeNanos
                && transport.state(generation) == SecureCameraTransport.READY;
    }

    private void runWorker() {
        try {
            listener.onStatus("secure transport ready; awaiting authenticated receiver", 0, 0);
            AvcCodecConfig applied = null;
            while (live()) {
                AvcCodecConfig config = codecConfig.get();
                if (config != null && applied == null) {
                    encoder.setCodecConfig(config);
                    applied = config;
                }
                // Preserve early output until the format callback supplies codec configuration.
                if (applied == null) {
                    LockSupport.parkNanos(1_000_000L);
                    continue;
                }
                EncodedAvcAccessUnit unit = queue.poll();
                if (unit == null) {
                    LockSupport.parkNanos(1_000_000L);
                    continue;
                }
                try {
                    List<byte[]> records = encoder.encode(unit); // at most config + AU
                    for (byte[] record : records) {
                        if (!live() || !transport.record(generation, record)) {
                            terminate("secure record rejected");
                            return;
                        }
                    }
                    if (!records.isEmpty() && !unit.codecConfig() && !unit.endOfStream()) {
                        sent++;
                        listener.onStatus("secure records queued; awaiting authenticated receiver", sent, 0);
                    }
                } finally {
                    reservations.decrementAndGet();
                }
            }
            terminate("secure generation unavailable or expired");
        } catch (RuntimeException failed) {
            terminate("secure encoder failed");
        } finally {
            drainQueue();
            listener.onStatus(terminalStatus, sent, 0);
        }
    }

    private void drainQueue() {
        while (queue.poll() != null) reservations.decrementAndGet();
    }

    private void terminate(String status) {
        if (lifecycle.getAndSet(2) != 2) {
            terminalStatus = status;
            transport.stop(generation);
            LockSupport.unpark(worker);
            drainQueue();
        }
    }

    /** Nonblocking: cancellation never joins a worker while a service monitor is held. */
    @Override public void close() {
        terminate("secure export stopped");
    }
}
