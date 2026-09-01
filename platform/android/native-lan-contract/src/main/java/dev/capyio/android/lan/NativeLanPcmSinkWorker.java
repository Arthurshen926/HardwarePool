package dev.capyio.android.lan;

import java.util.Objects;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.function.Consumer;
import java.util.function.LongConsumer;

/**
 * Drains complete PCM packets into one non-blocking platform sink.
 *
 * <p>The sink callback must return promptly and must never perform network or
 * UI work. Android uses {@code AudioTrack.WRITE_NON_BLOCKING}; this worker is
 * not an audio callback.</p>
 */
public final class NativeLanPcmSinkWorker implements AutoCloseable {
    public static final String PROBLEM_PACKET = "CAPY.ANDROID.SPEAKER_PACKET_INVALID";
    public static final String PROBLEM_WRITE = "CAPY.ANDROID.SPEAKER_WRITE_FAILED";
    public static final String PROBLEM_STALLED = "CAPY.ANDROID.SPEAKER_WRITE_STALLED";

    private static final int MAX_ZERO_WRITES = 1_000;
    private static final long ZERO_WRITE_BACKOFF_MILLIS = 2;

    private final NativeLanPacketQueue input;
    private final int bytesPerFrame;
    private final PcmSink sink;
    private final Runnable threadInitializer;
    private final LongConsumer framesListener;
    private final Consumer<String> failureListener;
    private final AtomicBoolean running = new AtomicBoolean();

    private Thread worker;
    private boolean started;
    private boolean stopped;
    private long packetsWritten;
    private long framesWritten;
    private long discontinuities;
    private long zeroWrites;

    public NativeLanPcmSinkWorker(
            NativeLanPacketQueue input,
            int bytesPerFrame,
            PcmSink sink,
            LongConsumer framesListener,
            Consumer<String> failureListener) {
        this(input, bytesPerFrame, sink, () -> {}, framesListener, failureListener);
    }

    public NativeLanPcmSinkWorker(
            NativeLanPacketQueue input,
            int bytesPerFrame,
            PcmSink sink,
            Runnable threadInitializer,
            LongConsumer framesListener,
            Consumer<String> failureListener) {
        this.input = Objects.requireNonNull(input, "input");
        if (bytesPerFrame < 1 || bytesPerFrame > 128) {
            throw new IllegalArgumentException("bytes per frame outside 1..=128");
        }
        this.bytesPerFrame = bytesPerFrame;
        this.sink = Objects.requireNonNull(sink, "sink");
        this.threadInitializer = Objects.requireNonNull(threadInitializer, "threadInitializer");
        this.framesListener = Objects.requireNonNull(framesListener, "framesListener");
        this.failureListener = Objects.requireNonNull(failureListener, "failureListener");
    }

    public synchronized void start() {
        if (started || stopped) {
            throw new IllegalStateException("native LAN PCM sink is one-shot");
        }
        started = true;
        running.set(true);
        Thread next = new Thread(this::run, "capyio-native-lan-pcm-sink");
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
        if (current != null && current != Thread.currentThread()) {
            current.interrupt();
            joinBounded(current);
        }
    }

    public boolean isRunning() {
        return running.get();
    }

    public synchronized Stats stats() {
        return new Stats(packetsWritten, framesWritten, discontinuities, zeroWrites);
    }

    @Override
    public void close() {
        stop();
    }

    private void run() {
        boolean haveTimeline = false;
        long expectedSequence = 0;
        long expectedSampleIndex = 0;
        try {
            threadInitializer.run();
            while (running.get()) {
                NativeLanPacketCodec.Packet packet = input.poll(50);
                if (packet == null) {
                    continue;
                }
                long expectedBytes = (long) packet.sampleCount * bytesPerFrame;
                if (expectedBytes != packet.payloadLength()) {
                    fail(PROBLEM_PACKET);
                    break;
                }

                boolean gap = haveTimeline
                        && (packet.sequence != expectedSequence
                                || packet.firstSampleIndex != expectedSampleIndex);
                if (packet.discontinuity || gap) {
                    sink.reset();
                    incrementDiscontinuities();
                }

                byte[] payload = packet.payloadCopy();
                int offset = 0;
                int consecutiveZeroWrites = 0;
                while (running.get() && offset < payload.length) {
                    int written = sink.write(payload, offset, payload.length - offset);
                    if (written < 0 || written > payload.length - offset) {
                        fail(PROBLEM_WRITE);
                        break;
                    }
                    if (written == 0) {
                        consecutiveZeroWrites++;
                        incrementZeroWrites();
                        if (consecutiveZeroWrites > MAX_ZERO_WRITES) {
                            fail(PROBLEM_STALLED);
                            break;
                        }
                        Thread.sleep(ZERO_WRITE_BACKOFF_MILLIS);
                        continue;
                    }
                    consecutiveZeroWrites = 0;
                    offset += written;
                }
                if (!running.get() || offset != payload.length) {
                    break;
                }

                recordWritten(packet.sampleCount);
                framesListener.accept(packet.sampleCount);
                expectedSequence = packet.sequence + 1;
                expectedSampleIndex = packet.firstSampleIndex
                        + Integer.toUnsignedLong(packet.sampleCount);
                haveTimeline = true;
            }
        } catch (InterruptedException interrupted) {
            Thread.currentThread().interrupt();
        } catch (RuntimeException failure) {
            fail(PROBLEM_WRITE);
        } finally {
            running.set(false);
            synchronized (this) {
                worker = null;
            }
        }
    }

    private void fail(String problemCode) {
        if (running.getAndSet(false)) {
            failureListener.accept(problemCode);
        }
    }

    private synchronized void recordWritten(int frames) {
        packetsWritten = saturatingIncrement(packetsWritten);
        framesWritten = saturatingAdd(framesWritten, Integer.toUnsignedLong(frames));
    }

    private synchronized void incrementDiscontinuities() {
        discontinuities = saturatingIncrement(discontinuities);
    }

    private synchronized void incrementZeroWrites() {
        zeroWrites = saturatingIncrement(zeroWrites);
    }

    private static long saturatingIncrement(long value) {
        return value == Long.MAX_VALUE ? value : value + 1;
    }

    private static long saturatingAdd(long value, long increment) {
        return Long.MAX_VALUE - value < increment ? Long.MAX_VALUE : value + increment;
    }

    private static void joinBounded(Thread thread) {
        try {
            thread.join(2_000);
        } catch (InterruptedException interrupted) {
            Thread.currentThread().interrupt();
        }
        if (thread.isAlive()) {
            throw new IllegalStateException("native LAN PCM sink did not stop within 2 seconds");
        }
    }

    public interface PcmSink {
        int write(byte[] payload, int offset, int length);

        void reset();
    }

    public static final class Stats {
        public final long packetsWritten;
        public final long framesWritten;
        public final long discontinuities;
        public final long zeroWrites;

        private Stats(
                long packetsWritten,
                long framesWritten,
                long discontinuities,
                long zeroWrites) {
            this.packetsWritten = packetsWritten;
            this.framesWritten = framesWritten;
            this.discontinuities = discontinuities;
            this.zeroWrites = zeroWrites;
        }
    }
}
