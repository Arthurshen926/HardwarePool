package dev.capyio.android.contract;

import java.util.Arrays;
import java.util.Objects;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;

/** One finite, generation-bound playback worker. Acceptance is not audibility. */
public final class SecureStereoPlaybackSession implements AutoCloseable {
    public static final int SAMPLE_RATE_HZ = 48_000;
    public static final int RECORD_BYTES = 1_920;
    public static final int MAX_BUFFER_BYTES = 19_200;
    public static final long OPERATION_DEADLINE_NANOS = 250_000_000L;
    public static final long MAX_REAP_MILLIS = 250;
    private static final long POLL_NANOS = 1_000_000;
    private static final int MAX_ATTEMPTS = 256;
    public enum State { NEW, WAITING_BOUND, PREPARED, PLAYING, STOPPING, STOPPED, FAILED }
    @FunctionalInterface public interface Cancellation { boolean cancelled(); }
    public interface Platform {
        /** Independent current visible playback consent, not RECORD_AUDIO permission. */
        boolean permitted();
        /** Worker-owned, including cleanup of any allocation before return. */
        Track openTrack(Cancellation cancellation) throws Exception;
        long nanoTime();
        void awaitPoll(long nanos) throws InterruptedException;
        default void cleanupComplete(boolean clean) {}
    }
    public interface Track {
        ActualAudioFormat actualFormat();
        boolean initialized();
        int capacityFrames();
        void play();
        boolean playing();
        int routedDeviceId();
        long routingGeneration();
        /** WRITE_NON_BLOCKING; never retain this array. Offset is always zero. */
        int write(byte[] pcm, int offset, int length);
        void pause();
        void flush();
        void release();
    }
    public interface NativeReceiver {
        /** 0 pending/busy, 1 bound, 2 failed, 3 stopped, 4 enabled. */
        int state(long generation, long[] counters);
        boolean enable(long generation);
        /** Remaining bytes copied to index zero; metadata token/logical offset/length. */
        int read(long generation, byte[] pcm, long[] metadata);
        /**
         * 1 enqueues the exact ACK for authoritative worker revalidation; it
         * is not confirmation that the worker has counted native acceptance.
         * 0 retries the SAME ACK, never repeats the platform write.
         */
        int ack(long generation, long token, int offset, int acceptedBytes);
        /** Synchronously revoke admission before independent resource cleanup. */
        void stop(long generation);
    }
    public static final class CleanupFailure extends RuntimeException {
        public CleanupFailure(Throwable cause) { super(cause); }
    }
    /**
     * acceptedBytes/acceptedRecords count actual platform writes with an
     * enqueued ACK only. They are diagnostic handoff progress, NOT native
     * worker-confirmed acceptance, Route Active, rendering or audibility.
     * Publish native state counters separately for confirmed acceptance.
     */
    public record Snapshot(long generation, State state, String problemCode,
            long acceptedBytes, long acceptedRecords, ActualAudioFormat actualFormat,
            int routedDeviceId, boolean cleanupComplete, boolean reapTimedOut,
            boolean cleanupFailed) {}

    private final long generation;
    private final Platform platform;
    private final NativeReceiver receiver;
    private final Thread worker;
    private final AtomicBoolean cancelled = new AtomicBoolean();
    private final CountDownLatch completed = new CountDownLatch(1);
    private final Object nativeStopLock = new Object();
    private boolean nativeStopped;
    private volatile State state = State.NEW;
    private volatile String problem = "";
    private volatile boolean cleanupFailed;
    private volatile boolean reapTimedOut;
    private volatile long acceptedBytes;
    private volatile long acceptedRecords;
    private volatile ActualAudioFormat actualFormat;
    private volatile int routedDeviceId;
    private long routeRevision;

    public SecureStereoPlaybackSession(long generation, Platform platform, NativeReceiver receiver) {
        if (generation <= 0) throw new IllegalArgumentException("positive generation required");
        this.generation = generation;
        this.platform = Objects.requireNonNull(platform);
        this.receiver = Objects.requireNonNull(receiver);
        worker = new Thread(this::run, "CapyIO-Secure-Stereo-Playback");
        worker.setDaemon(true);
    }
    public synchronized boolean start() {
        if (state != State.NEW || cancelled.get()) return false;
        state = State.WAITING_BOUND;
        try { worker.start(); return true; }
        catch (RuntimeException failure) {
            fail("PLATFORM");
            finish();
            return false;
        }
    }
    public void stop() {
        boolean neverStarted;
        synchronized (this) {
            neverStarted = state == State.NEW;
            cancelled.set(true);
            if (state != State.STOPPED && state != State.FAILED) state = State.STOPPING;
        }
        stopNative();
        worker.interrupt();
        if (neverStarted) finish();
    }
    @Override public void close() { stop(); }
    public Snapshot snapshot() {
        return new Snapshot(generation, state, problem, acceptedBytes, acceptedRecords,
                actualFormat, routedDeviceId, completed.getCount() == 0, reapTimedOut, cleanupFailed);
    }
    public boolean awaitStopped(long milliseconds) throws InterruptedException {
        if (milliseconds < 0 || milliseconds > MAX_REAP_MILLIS) throw new IllegalArgumentException("bounded reap required");
        boolean done = completed.await(milliseconds, TimeUnit.MILLISECONDS);
        if (!done) reapTimedOut = true;
        return done;
    }
    private synchronized void publish(State next) { if (!cancelled.get()) state = next; }
    private void fail(String code) {
        synchronized (this) {
            if (!cancelled.get()) {
                problem = "CAPY.ANDROID.SECURE_SPEAKER_" + code;
                state = State.FAILED;
                cancelled.set(true);
            }
        }
        stopNative();
    }
    private void stopNative() {
        synchronized (nativeStopLock) {
            if (nativeStopped) return;
            nativeStopped = true;
            try { receiver.stop(generation); }
            catch (RuntimeException | LinkageError failure) { cleanupFailed = true; }
        }
    }
    private void finish() {
        if (cleanupFailed) {
            problem = "CAPY.ANDROID.SECURE_SPEAKER_CLEANUP_FAILED";
            state = State.FAILED;
        } else if (state != State.FAILED) state = State.STOPPED;
        try { platform.cleanupComplete(!cleanupFailed); }
        catch (RuntimeException | LinkageError failure) {
            cleanupFailed = true;
            problem = "CAPY.ANDROID.SECURE_SPEAKER_CLEANUP_FAILED";
            state = State.FAILED;
        } finally { completed.countDown(); }
    }
    private boolean permitted() {
        if (cancelled.get()) return false;
        if (!platform.permitted()) { fail("CONSENT_REVOKED"); return false; }
        return !cancelled.get();
    }
    private boolean within(long start) {
        long elapsed = platform.nanoTime() - start;
        return elapsed >= 0 && elapsed < OPERATION_DEADLINE_NANOS;
    }
    private boolean awaitBound(long[] counters) throws InterruptedException {
        long began = platform.nanoTime();
        for (int attempt = 0; attempt < MAX_ATTEMPTS && permitted() && within(began); attempt++) {
            int nativeState = receiver.state(generation, counters);
            if (!permitted()) return false;
            if (nativeState == 1) return true;
            if (nativeState != 0) { fail("NATIVE_UNAVAILABLE"); return false; }
            platform.awaitPoll(POLL_NANOS);
        }
        if (!cancelled.get()) fail("BOUND_TIMEOUT");
        return false;
    }
    private boolean validTrack(Track track) {
        ActualAudioFormat format = track.actualFormat();
        int capacity = track.capacityFrames();
        if (!track.initialized() || format.sampleRateHz() != SAMPLE_RATE_HZ
                || format.channelCount() != 2 || !format.encoding().equals("pcm_s16le")
                || format.framesPerBuffer() > MAX_BUFFER_BYTES / 4
                || capacity < format.framesPerBuffer() || capacity > MAX_BUFFER_BYTES / 4) {
            fail("FORMAT_MISMATCH"); return false;
        }
        if (actualFormat == null) actualFormat = format;
        else if (!actualFormat.equals(format)) { fail("FORMAT_CHANGED"); return false; }
        return true;
    }
    private boolean route(Track track) {
        long revision = track.routingGeneration();
        int device = track.routedDeviceId();
        if (device <= 0 || track.routingGeneration() != revision) return false;
        if (routedDeviceId == 0) { routedDeviceId = device; routeRevision = revision; }
        return device == routedDeviceId && revision == routeRevision;
    }
    private void run() {
        Track track = null;
        byte[] pcm = new byte[RECORD_BYTES];
        long[] metadata = new long[3];
        long[] counters = new long[3];
        try {
            if (!awaitBound(counters) || !permitted()) return;
            publish(State.PREPARED);
            track = platform.openTrack(cancelled::get);
            if (!permitted()) return;
            if (track == null || !validTrack(track)) { fail("PLATFORM"); return; }
            if (!permitted()) return;
            track.play();
            if (!permitted() || !track.playing()) { fail("PLATFORM"); return; }
            long routeBegan = platform.nanoTime();
            int routeAttempts = 0;
            while (permitted()) {
                if (++routeAttempts > MAX_ATTEMPTS || !within(routeBegan)) { fail("ROUTE_UNAVAILABLE"); return; }
                // Bound remains authoritative throughout readiness. Pending
                // may mean registry contention; it is not new permission.
                int nativeState = receiver.state(generation, counters);
                if (!permitted()) return;
                if (nativeState != 0 && nativeState != 1) { fail("NATIVE_UNAVAILABLE"); return; }
                if (!validTrack(track) || !track.playing()) { fail("PLATFORM"); return; }
                if (route(track)) break;
                platform.awaitPoll(POLL_NANOS);
            }
            // No PCM probe and no deadline extension: only a fully verified
            // playing route may enable native delivery. Enable revalidates the
            // original native consent/claims/deadline, not a new setup budget.
            if (!permitted() || !validTrack(track) || !track.playing() || !route(track)) { fail("ROUTE_CHANGED"); return; }
            if (!receiver.enable(generation)) { fail("NATIVE_UNAVAILABLE"); return; }
            if (!permitted() || !validTrack(track) || !track.playing() || !route(track)) { fail("ROUTE_CHANGED"); return; }
            publish(State.PLAYING);
            int expectedOffset = 0;
            long blockBegan = platform.nanoTime();
            int attempts = 0;
            while (permitted()) {
                if (++attempts > MAX_ATTEMPTS || !within(blockBegan)) { fail("STALLED"); break; }
                int nativeState = receiver.state(generation, counters);
                if (!permitted()) break;
                if (nativeState == 3) { stop(); break; }
                if (nativeState != 0 && nativeState != 4) { fail("NATIVE_UNAVAILABLE"); break; }
                if (!validTrack(track) || !track.playing() || !route(track)) { fail("ROUTE_CHANGED"); break; }
                int read = receiver.read(generation, pcm, metadata);
                if (!permitted()) break;
                if (read == 0) { platform.awaitPoll(POLL_NANOS); continue; }
                if (read != 1 || metadata[0] <= 0 || metadata[1] != expectedOffset
                        || metadata[2] != RECORD_BYTES - expectedOffset) { fail("INVALID_PERMIT"); break; }
                int length = (int) metadata[2];
                if (!within(blockBegan)) { fail("STALLED"); break; }
                if (!permitted() || !validTrack(track) || !track.playing() || !route(track)) { fail("ROUTE_CHANGED"); break; }
                // Native Read copied the REMAINING slice at array index zero.
                // logical offset belongs exclusively to ACK, not this write.
                int written = track.write(pcm, 0, length);
                if (!permitted()) break;
                if (written < 0 || written > length || (written & 3) != 0) { fail("INVALID_WRITE"); break; }
                int ack;
                do {
                    if (!permitted()) return;
                    if (!validTrack(track) || !track.playing() || !route(track)) { fail("ROUTE_CHANGED"); return; }
                    if (++attempts > MAX_ATTEMPTS || !within(blockBegan)) { fail("STALLED"); return; }
                    ack = receiver.ack(generation, metadata[0], expectedOffset, written);
                    if (!permitted()) return;
                    if (ack == 0) platform.awaitPoll(POLL_NANOS);
                } while (ack == 0);
                if (ack != 1) { fail("ACK_REJECTED"); break; }
                // ACK enqueue succeeded; native worker validation/counters
                // may still lag or reject after concurrent revocation.
                acceptedBytes += written;
                expectedOffset += written;
                if (expectedOffset == RECORD_BYTES) {
                    acceptedRecords++;
                    expectedOffset = 0;
                    blockBegan = platform.nanoTime();
                    attempts = 0;
                } else if (written == 0) platform.awaitPoll(POLL_NANOS);
            }
        } catch (InterruptedException interrupted) {
            if (!cancelled.get()) fail("INTERRUPTED");
            Thread.currentThread().interrupt();
        } catch (CleanupFailure failure) {
            cleanupFailed = true;
            fail("CLEANUP_FAILED");
        } catch (Exception | LinkageError failure) {
            fail("PLATFORM");
        } finally {
            cancelled.set(true);
            stopNative(); // Serialized with caller Stop; admission is closed before cleanup.
            if (track != null) {
                try { track.pause(); } catch (RuntimeException | LinkageError failure) { cleanupFailed = true; }
                try { track.flush(); } catch (RuntimeException | LinkageError failure) { cleanupFailed = true; }
                try { track.release(); } catch (RuntimeException | LinkageError failure) { cleanupFailed = true; }
            }
            Arrays.fill(pcm, (byte) 0);
            Arrays.fill(metadata, 0);
            finish();
        }
    }
}
