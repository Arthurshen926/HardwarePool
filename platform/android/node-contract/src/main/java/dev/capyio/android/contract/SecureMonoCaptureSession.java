package dev.capyio.android.contract;

import java.util.Arrays;
import java.util.Objects;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicReference;

/**
 * One-shot, generation-bound microphone boundary. Owns one 960-byte assembly
 * buffer; it has no transport, sockets, retry, conversion, or permission UI.
 * All recorder calls (including release) belong to the capture worker. Stop
 * invalidates publication immediately, but never releases an in-use recorder.
 */
public final class SecureMonoCaptureSession implements AutoCloseable {
    public static final int SAMPLE_RATE_HZ = 48_000;
    public static final int CHANNEL_COUNT = 1;
    public static final int FRAMES_PER_RECORD = 480;
    public static final int RECORD_BYTES = 960;
    public static final int MICROPHONE_SOURCE = 1;
    public static final int MAX_FRAME_READS = 128;
    public static final int MAX_READY_POLLS = 500;
    public static final long FRAME_DEADLINE_NANOS = 250_000_000L;
    public static final long READY_DEADLINE_NANOS = 5_000_000_000L;
    public static final long MAX_REAP_MILLIS = 250;
    private static final long READ_POLL_NANOS = 2_000_000L;
    private static final long READY_POLL_NANOS = 10_000_000L;

    public static final String PROBLEM_PERMISSION = "CAPY.ANDROID.SECURE_MIC_PERMISSION";
    public static final String PROBLEM_FORMAT = "CAPY.ANDROID.SECURE_MIC_FORMAT";
    public static final String PROBLEM_SOURCE = "CAPY.ANDROID.SECURE_MIC_SOURCE_CHANGED";
    public static final String PROBLEM_READ = "CAPY.ANDROID.SECURE_MIC_INVALID_READ";
    public static final String PROBLEM_STALL = "CAPY.ANDROID.SECURE_MIC_STALLED";
    public static final String PROBLEM_NATIVE = "CAPY.ANDROID.SECURE_MIC_NATIVE_UNAVAILABLE";
    public static final String PROBLEM_READY_TIMEOUT = "CAPY.ANDROID.SECURE_MIC_READY_TIMEOUT";
    public static final String PROBLEM_PLATFORM = "CAPY.ANDROID.SECURE_MIC_PLATFORM_FAILED";
    public static final String PROBLEM_CLEANUP = "CAPY.ANDROID.SECURE_MIC_CLEANUP_FAILED";

    public enum NativeState { WAITING, READY, ACTIVE, STOPPED, FAILED }
    public enum State { NEW, WAITING_READY, PREPARED, CAPTURING, ACTIVE, STOPPED, FAILED }

    @FunctionalInterface
    public interface Cancellation {
        boolean cancelled();
    }

    public interface Platform {
        /** Current RECORD_AUDIO permission AND user-visible consent/lifecycle. */
        boolean permissionGranted();
        /** Worker-only. Recheck cancellation before/after any platform allocation. */
        Recorder openRecorder(Cancellation cancellation) throws Exception;
        long nanoTime();
        /** Bounded wait on the worker, interruptible by Stop. */
        void awaitPoll(long nanos) throws InterruptedException;
        /**
         * Exactly once after recorder cleanup and native invalidation finish.
         * For an atomic process reservation only: must not wait, touch UI,
         * acquire a Service monitor, or reopen any resource. False means a
         * cleanup operation threw; retain the reservation in that case.
         */
        default void cleanupComplete(boolean clean) {}
    }

    public interface Recorder {
        ActualAudioFormat actualFormat();
        int audioSource();
        void start();
        /** Positive actual routed-device ID, or zero when not yet routed. */
        int routedDeviceId();
        /** Monotonic routing notification counter, including transient changes. */
        long routingGeneration();
        /** Nonblocking. Must not retain the supplied buffer or return > length. */
        int read(byte[] target, int offset, int length);
        void stop();
        void release();
    }

    public interface NativeSender {
        /** Nonblocking generation lookup, never connection establishment. */
        NativeState state(long generation);
        /**
         * Bounded nonblocking COPY into the native two-slot sender job. The
         * buffer is reused immediately and MUST NOT be retained. No TLS, socket
         * I/O, waiting, or unbounded allocation may run on this caller thread.
         * Acceptance is queue admission, NOT proof of authenticated delivery.
         */
        boolean record(long generation, byte[] pcm960);
        /**
         * Lifecycle invalidation, not a media callback. May briefly synchronize
         * on the native job registry, but never on control/network operations
         * or a worker join. After its linearization point, concurrent or stale
         * record calls must reject this generation.
         */
        void stop(long generation);
    }

    public record Snapshot(long generation, State state, String problemCode,
            long acceptedFrames, ActualAudioFormat actualFormat, int routedDeviceId,
            boolean cleanupComplete, boolean reapTimedOut, boolean cleanupFailed) {}

    private record Status(State state, String problemCode) {
        boolean terminal() { return state == State.STOPPED || state == State.FAILED; }
    }

    private final long generation;
    private final Platform platform;
    private final NativeSender sender;
    private final AtomicReference<Status> status =
            new AtomicReference<>(new Status(State.NEW, ""));
    private final AtomicBoolean nativeStopped = new AtomicBoolean();
    private final AtomicBoolean nativeStopCompleted = new AtomicBoolean();
    private final AtomicBoolean captureCleaned = new AtomicBoolean();
    private final AtomicBoolean completionClaimed = new AtomicBoolean();
    private final CountDownLatch completed = new CountDownLatch(1);
    private final Thread worker;
    private final Cancellation cancellation = () -> status.get().terminal();
    private volatile long acceptedFrames;
    private volatile ActualAudioFormat actualFormat;
    private volatile int routedDeviceId;
    private long pinnedRoutingGeneration;
    private volatile boolean reapTimedOut;
    private volatile boolean cleanupFailed;

    public SecureMonoCaptureSession(long generation, Platform platform, NativeSender sender) {
        if (generation <= 0) throw new IllegalArgumentException("positive generation required");
        this.generation = generation;
        this.platform = Objects.requireNonNull(platform, "platform");
        this.sender = Objects.requireNonNull(sender, "sender");
        worker = new Thread(this::capture, "CapyIO-Secure-Mono-Capture");
        worker.setDaemon(true);
    }

    public long generation() { return generation; }

    /** Starts only once. Does not open a microphone on the caller thread. */
    public boolean start() {
        Status initial = status.get();
        if (initial.state() != State.NEW || !status.compareAndSet(initial,
                new Status(State.WAITING_READY, ""))) return false;
        try {
            worker.start();
            return true;
        } catch (RuntimeException failed) {
            fail(PROBLEM_PLATFORM);
            finishCapture();
            return false;
        }
    }

    /** Idempotent, never joins. Safe while initialization/read is pending. */
    public void stop() {
        Status previous;
        do {
            previous = status.get();
            if (previous.terminal()) break;
        } while (!status.compareAndSet(previous, new Status(State.STOPPED, "")));
        stopNative();
        worker.interrupt();
        if (previous.state() == State.NEW) finishCapture();
    }

    @Override public void close() { stop(); }

    public Snapshot snapshot() {
        Status current = status.get();
        return new Snapshot(generation, current.state(), current.problemCode(), acceptedFrames,
                actualFormat, routedDeviceId, completed.getCount() == 0,
                reapTimedOut, cleanupFailed);
    }

    /**
     * Optional bounded reap outside any service/UI monitor. A timeout reports
     * pending cleanup; it never frees memory still used by initialization/read.
     */
    public boolean awaitStopped(long timeoutMillis) throws InterruptedException {
        if (timeoutMillis < 0 || timeoutMillis > MAX_REAP_MILLIS) {
            throw new IllegalArgumentException("reap must be between 0 and 250 ms");
        }
        boolean stopped = completed.await(timeoutMillis, TimeUnit.MILLISECONDS);
        if (!stopped) reapTimedOut = true;
        return stopped;
    }

    private boolean live() { return !status.get().terminal(); }

    private boolean permitted() {
        if (!live()) return false;
        if (!platform.permissionGranted()) {
            fail(PROBLEM_PERMISSION);
            return false;
        }
        return live();
    }

    private void publishState(State next) {
        Status old;
        do {
            old = status.get();
            if (old.terminal() || old.state() == next) return;
        } while (!status.compareAndSet(old, new Status(next, "")));
    }

    private void fail(String problem) {
        Status old;
        do {
            old = status.get();
            if (old.terminal()) return;
        } while (!status.compareAndSet(old, new Status(State.FAILED, problem)));
        stopNative();
    }

    private void stopNative() {
        if (nativeStopped.compareAndSet(false, true)) {
            try {
                sender.stop(generation);
            } catch (RuntimeException | LinkageError failed) {
                cleanupFailed = true;
            } finally {
                nativeStopCompleted.set(true);
                finishIfComplete();
            }
        }
    }

    private void finishCapture() {
        captureCleaned.set(true);
        finishIfComplete();
    }

    private void finishIfComplete() {
        if (captureCleaned.get() && nativeStopCompleted.get()
                && completionClaimed.compareAndSet(false, true)) {
            try { platform.cleanupComplete(!cleanupFailed); }
            catch (RuntimeException | LinkageError failed) { cleanupFailed = true; }
            finally { completed.countDown(); }
        }
    }

    private boolean awaitReady() throws InterruptedException {
        long began = platform.nanoTime();
        for (int polls = 0; polls < MAX_READY_POLLS && permitted(); polls++) {
            if (platform.nanoTime() - began >= READY_DEADLINE_NANOS) break;
            NativeState current = sender.state(generation);
            if (!live()) return false;
            if (current == NativeState.READY) {
                publishState(State.PREPARED);
                return true;
            }
            if (current != NativeState.WAITING) {
                fail(PROBLEM_NATIVE);
                return false;
            }
            platform.awaitPoll(READY_POLL_NANOS);
        }
        if (live()) fail(PROBLEM_READY_TIMEOUT);
        return false;
    }

    private boolean nativeLive() {
        if (!live()) return false;
        NativeState current = sender.state(generation);
        if (!live()) return false;
        if (current == NativeState.ACTIVE && acceptedFrames > 0) {
            publishState(State.ACTIVE);
            return true;
        }
        if (current == NativeState.READY && status.get().state() != State.ACTIVE) return live();
        fail(PROBLEM_NATIVE);
        return false;
    }

    private boolean pinRoute(Recorder recorder) throws InterruptedException {
        long began = platform.nanoTime();
        for (int attempt = 0; attempt < MAX_FRAME_READS && permitted() && nativeLive(); attempt++) {
            if (platform.nanoTime() - began >= FRAME_DEADLINE_NANOS) break;
            long revision = recorder.routingGeneration();
            int actual = recorder.routedDeviceId();
            if (actual > 0) {
                if (recorder.routingGeneration() != revision) {
                    fail(PROBLEM_SOURCE);
                    return false;
                }
                routedDeviceId = actual;
                pinnedRoutingGeneration = revision;
                return true;
            }
            if (actual < 0) break;
            platform.awaitPoll(READ_POLL_NANOS);
        }
        if (live()) fail(PROBLEM_SOURCE);
        return false;
    }

    private boolean sameSource(Recorder recorder, long routingGeneration) {
        if (recorder.audioSource() != MICROPHONE_SOURCE
                || recorder.routedDeviceId() != routedDeviceId
                || recorder.routingGeneration() != routingGeneration) {
            fail(PROBLEM_SOURCE);
            return false;
        }
        return live();
    }

    private void capture() {
        Recorder recorder = null;
        byte[] frame = null;
        boolean startAttempted = false;
        try {
            if (!awaitReady() || !permitted() || !nativeLive()) return;
            recorder = platform.openRecorder(cancellation);
            if (!live()) return;
            if (recorder == null) {
                fail(PROBLEM_PLATFORM);
                return;
            }
            if (!permitted() || !nativeLive()) return;
            actualFormat = recorder.actualFormat();
            if (actualFormat == null || actualFormat.sampleRateHz() != SAMPLE_RATE_HZ
                    || actualFormat.channelCount() != CHANNEL_COUNT
                    || !"pcm_s16le".equals(actualFormat.encoding())
                    || recorder.audioSource() != MICROPHONE_SOURCE) {
                fail(PROBLEM_FORMAT);
                return;
            }
            if (!permitted() || !nativeLive()) return;
            startAttempted = true;
            recorder.start();
            if (!permitted() || !nativeLive() || !pinRoute(recorder)) return;
            long routingGeneration = pinnedRoutingGeneration;
            if (!sameSource(recorder, routingGeneration)) return;
            frame = new byte[RECORD_BYTES];
            publishState(State.CAPTURING);
            while (permitted() && nativeLive()) {
                // One immutable deadline/attempt budget per frame. Neither
                // partial progress nor a zero read extends this budget.
                long began = platform.nanoTime();
                int used = 0;
                int reads = 0;
                while (used < RECORD_BYTES && permitted() && nativeLive()) {
                    if (++reads > MAX_FRAME_READS
                            || platform.nanoTime() - began >= FRAME_DEADLINE_NANOS) {
                        fail(PROBLEM_STALL);
                        return;
                    }
                    if (!sameSource(recorder, routingGeneration)) return;
                    int remaining = RECORD_BYTES - used;
                    int count = recorder.read(frame, used, remaining);
                    if (!permitted() || !nativeLive()
                            || !sameSource(recorder, routingGeneration)) return;
                    if (platform.nanoTime() - began >= FRAME_DEADLINE_NANOS) {
                        fail(PROBLEM_STALL);
                        return;
                    }
                    if (count < 0 || count > remaining || (count & 1) != 0) {
                        fail(PROBLEM_READ);
                        return;
                    }
                    if (count == 0) platform.awaitPoll(READ_POLL_NANOS);
                    else used += count;
                }
                if (used != RECORD_BYTES || !permitted() || !nativeLive()
                        || !sameSource(recorder, routingGeneration)) return;
                if (!sender.record(generation, frame)) {
                    fail(PROBLEM_NATIVE);
                    return;
                }
                acceptedFrames++;
            }
        } catch (InterruptedException interrupted) {
            Thread.currentThread().interrupt();
            if (live()) fail(PROBLEM_PLATFORM);
        } catch (SecurityException denied) {
            if (live()) fail(PROBLEM_PERMISSION);
        } catch (Exception | LinkageError failed) {
            if (live()) fail(PROBLEM_PLATFORM);
        } finally {
            // Even an unexpected VM/platform error cannot leave a completed
            // worker looking usable to a polling service owner.
            if (live()) fail(PROBLEM_PLATFORM);
            stopNative();
            if (recorder != null) {
                if (startAttempted) {
                    try { recorder.stop(); }
                    catch (RuntimeException | LinkageError failed) { cleanupFailed = true; }
                }
                try { recorder.release(); }
                catch (RuntimeException | LinkageError failed) { cleanupFailed = true; }
            }
            if (frame != null) Arrays.fill(frame, (byte) 0);
            if (cleanupFailed && live()) fail(PROBLEM_CLEANUP);
            finishCapture();
        }
    }
}
