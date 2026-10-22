package dev.capyio.android.contract;

import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.function.IntUnaryOperator;
import java.util.function.Consumer;

/** Synthetic only: no Android framework, microphone, permission UI, or sockets. */
public final class SecureMonoCaptureSessionContractTest {
    private static int assertions;
    private static final long GENERATION = 19;

    private SecureMonoCaptureSessionContractTest() {}

    public static void main(String[] args) throws Exception {
        waitsForNativeReadyWithoutOpeningMicrophone();
        boundsReadinessWithoutResettingDeadline();
        rejectsPermissionDenialAndRevocation();
        rejectsActualFormatAndSourceMismatch();
        assemblesEvenPartialReadsWithoutConversion();
        rejectsOddNegativeAndOversizeReads();
        boundsZeroAndPartialReads();
        pinsActualSourceBeforePublication();
        rejectsSourceAndRouteChanges();
        queueAdmissionIsNotAuthenticatedDelivery();
        rejectsQueueFailureAndStaleGeneration();
        rejectsNativeStateRegression();
        stopDoesNotReleasePendingInitialization();
        stopDoesNotReleasePendingReadOrBuffer();
        stopBeforeStartIsTerminal();
        cleansUpOnceDespiteExceptions();
        processReservationSurvivesOwnerRecreation();
        processReservationWaitsForCompleteCleanInvalidation();
        exposesNoUdpOrEndpointBoundary();
        System.out.println("Secure mono capture contract: PASS (" + assertions + " assertions)");
    }

    private static void waitsForNativeReadyWithoutOpeningMicrophone() throws Exception {
        Fixture f = new Fixture();
        f.sender.state = SecureMonoCaptureSession.NativeState.WAITING;
        Gate pending = new Gate();
        f.platform.onPoll = pending::hold;
        check(f.session.start());
        pending.entered();
        check(f.platform.opens == 0);
        check(f.session.snapshot().state() == SecureMonoCaptureSession.State.WAITING_READY);
        f.session.stop();
        f.sender.state = SecureMonoCaptureSession.NativeState.READY; // Late Ready is stale.
        pending.release();
        done(f.session);
        check(f.platform.opens == 0 && f.sender.calls == 0);
        check(f.sender.stops == 1);
        check(!f.session.start());
        check(f.session.snapshot().state() == SecureMonoCaptureSession.State.STOPPED);
    }

    private static void boundsReadinessWithoutResettingDeadline() throws Exception {
        Fixture f = new Fixture();
        f.sender.state = SecureMonoCaptureSession.NativeState.WAITING;
        run(f);
        problem(f, SecureMonoCaptureSession.PROBLEM_READY_TIMEOUT);
        check(f.platform.polls == SecureMonoCaptureSession.MAX_READY_POLLS);
        check(f.platform.opens == 0);
        Fixture frozenClock = new Fixture();
        frozenClock.platform.advanceClock = false;
        frozenClock.sender.state = SecureMonoCaptureSession.NativeState.WAITING;
        run(frozenClock);
        problem(frozenClock, SecureMonoCaptureSession.PROBLEM_READY_TIMEOUT);
        check(frozenClock.platform.polls == SecureMonoCaptureSession.MAX_READY_POLLS);
    }

    private static void rejectsPermissionDenialAndRevocation() throws Exception {
        Fixture denied = new Fixture();
        denied.platform.permission = false;
        run(denied);
        problem(denied, SecureMonoCaptureSession.PROBLEM_PERMISSION);
        check(denied.platform.opens == 0 && denied.sender.calls == 0);

        Fixture duringOpen = new Fixture();
        duringOpen.platform.onOpen = () -> duringOpen.platform.permission = false;
        run(duringOpen);
        problem(duringOpen, SecureMonoCaptureSession.PROBLEM_PERMISSION);
        check(duringOpen.recorder.starts == 0 && duringOpen.recorder.releases == 1);

        Fixture duringRead = new Fixture();
        duringRead.recorder.onRead = () -> duringRead.platform.permission = false;
        run(duringRead);
        problem(duringRead, SecureMonoCaptureSession.PROBLEM_PERMISSION);
        check(duringRead.sender.calls == 0);
        released(duringRead);

        Fixture rejectedByPlatform = new Fixture();
        rejectedByPlatform.platform.onOpen = () -> { throw new SecurityException("synthetic"); };
        run(rejectedByPlatform);
        problem(rejectedByPlatform, SecureMonoCaptureSession.PROBLEM_PERMISSION);
        check(rejectedByPlatform.sender.calls == 0);
    }

    private static void rejectsActualFormatAndSourceMismatch() throws Exception {
        for (ActualAudioFormat actual : List.of(
                new ActualAudioFormat(44_100, 1, "pcm_s16le", 480),
                new ActualAudioFormat(48_000, 2, "pcm_s16le", 480),
                new ActualAudioFormat(48_000, 1, "pcm_float", 480))) {
            Fixture f = new Fixture();
            f.recorder.format = actual;
            run(f);
            problem(f, SecureMonoCaptureSession.PROBLEM_FORMAT);
            check(f.recorder.starts == 0 && f.recorder.reads == 0);
            check(f.recorder.releases == 1 && f.sender.calls == 0);
        }
        Fixture source = new Fixture();
        source.recorder.source = 7;
        run(source);
        problem(source, SecureMonoCaptureSession.PROBLEM_FORMAT);
        check(source.recorder.releases == 1 && source.sender.calls == 0);
    }

    private static void assemblesEvenPartialReadsWithoutConversion() throws Exception {
        Fixture f = new Fixture();
        int[] reads = { 120, 2, 838 };
        f.recorder.readSize = ignored -> reads[f.recorder.reads - 1];
        f.sender.failAfter = 1;
        run(f);
        check(f.recorder.reads == 3);
        check(f.sender.accepted.size() == 1);
        check(f.session.snapshot().acceptedFrames() == 1);
        byte[] actual = f.sender.accepted.get(0);
        check(actual.length == 960);
        for (int i = 0; i < actual.length; i++) check(actual[i] == sample(i));
        check(f.recorder.seenBuffers.size() == 1);
        check(f.recorder.lastBuffer.length == SecureMonoCaptureSession.RECORD_BYTES);
        check(allZero(f.recorder.lastBuffer)); // Cleared only after read exited.
        released(f);
    }

    private static void rejectsOddNegativeAndOversizeReads() throws Exception {
        for (int invalid : new int[] { 1, -3, 962 }) {
            Fixture f = new Fixture();
            f.recorder.readSize = remaining -> invalid;
            run(f);
            problem(f, SecureMonoCaptureSession.PROBLEM_READ);
            check(f.sender.calls == 0 && f.recorder.reads == 1);
            released(f);
        }
        Fixture partialOdd = new Fixture();
        partialOdd.recorder.readSize = remaining -> partialOdd.recorder.reads == 1 ? 2 : 3;
        run(partialOdd);
        problem(partialOdd, SecureMonoCaptureSession.PROBLEM_READ);
        check(partialOdd.sender.calls == 0 && partialOdd.recorder.reads == 2);
    }

    private static void boundsZeroAndPartialReads() throws Exception {
        Fixture zero = new Fixture();
        zero.recorder.readSize = remaining -> 0;
        run(zero);
        problem(zero, SecureMonoCaptureSession.PROBLEM_STALL);
        check(zero.recorder.reads == 125 && zero.sender.calls == 0);

        Fixture frozenClock = new Fixture();
        frozenClock.platform.advanceClock = false;
        frozenClock.recorder.readSize = remaining -> 0;
        run(frozenClock);
        problem(frozenClock, SecureMonoCaptureSession.PROBLEM_STALL);
        check(frozenClock.recorder.reads == SecureMonoCaptureSession.MAX_FRAME_READS);

        Fixture partial = new Fixture();
        partial.recorder.readSize = remaining -> partial.recorder.reads == 61 ? 2 : 0;
        run(partial);
        problem(partial, SecureMonoCaptureSession.PROBLEM_STALL);
        check(partial.recorder.reads == 126); // Partial read did not reset the zero deadline.
        check(partial.sender.calls == 0);

        Fixture tiny = new Fixture();
        tiny.recorder.readSize = remaining -> 2;
        run(tiny);
        problem(tiny, SecureMonoCaptureSession.PROBLEM_STALL);
        check(tiny.recorder.reads == SecureMonoCaptureSession.MAX_FRAME_READS);
        check(tiny.sender.calls == 0);

        Fixture slowRead = new Fixture();
        slowRead.recorder.onRead = () -> slowRead.platform.now += 300_000_000L;
        run(slowRead);
        problem(slowRead, SecureMonoCaptureSession.PROBLEM_STALL);
        check(slowRead.sender.calls == 0);

        Fixture nextFrame = new Fixture();
        nextFrame.recorder.readSize = remaining -> nextFrame.recorder.reads == 2 ? 0 : remaining;
        nextFrame.sender.failAfter = 2;
        run(nextFrame);
        check(nextFrame.sender.accepted.size() == 2); // New whole frame gets its own budget.
    }

    private static void pinsActualSourceBeforePublication() throws Exception {
        Fixture deferred = new Fixture();
        deferred.recorder.device = 0;
        deferred.platform.onPoll = () -> {
            check(deferred.recorder.reads == 0 && deferred.sender.calls == 0);
            deferred.recorder.device = 23;
            deferred.recorder.revision++;
        };
        deferred.sender.failAfter = 1;
        run(deferred);
        check(deferred.session.snapshot().routedDeviceId() == 23);
        check(deferred.sender.accepted.size() == 1);

        Fixture missing = new Fixture();
        missing.recorder.device = 0;
        run(missing);
        problem(missing, SecureMonoCaptureSession.PROBLEM_SOURCE);
        check(missing.recorder.reads == 0 && missing.sender.calls == 0);
        check(missing.platform.polls <= SecureMonoCaptureSession.MAX_FRAME_READS);
    }

    private static void rejectsSourceAndRouteChanges() throws Exception {
        for (int change = 0; change < 4; change++) {
            Fixture f = new Fixture();
            final int selected = change;
            f.recorder.onRead = () -> {
                switch (selected) {
                    case 0 -> f.recorder.device = 12;
                    case 1 -> f.recorder.revision++; // Switch out and back to the same ID.
                    case 2 -> f.recorder.source = 7;
                    case 3 -> f.recorder.device = 0;
                    default -> throw new AssertionError();
                }
            };
            run(f);
            problem(f, SecureMonoCaptureSession.PROBLEM_SOURCE);
            check(f.sender.calls == 0);
            released(f);
        }
    }

    private static void queueAdmissionIsNotAuthenticatedDelivery() throws Exception {
        Fixture f = new Fixture();
        Gate secondRead = new Gate();
        Gate secondRecord = new Gate();
        f.recorder.onRead = () -> { if (f.recorder.reads == 2) secondRead.hold(); };
        f.sender.onRecord = () -> { if (f.sender.calls == 2) secondRecord.hold(); };
        check(f.session.start());
        secondRead.entered();
        check(f.session.snapshot().acceptedFrames() == 1);
        check(f.session.snapshot().state() == SecureMonoCaptureSession.State.CAPTURING);
        f.sender.state = SecureMonoCaptureSession.NativeState.ACTIVE;
        secondRead.release();
        secondRecord.entered();
        check(f.session.snapshot().state() == SecureMonoCaptureSession.State.ACTIVE);
        f.session.stop();
        secondRecord.release();
        done(f.session);
        check(f.sender.accepted.size() == 1); // In-flight callback rejected after native Stop.
        check(f.session.snapshot().state() == SecureMonoCaptureSession.State.STOPPED);
        check(f.sender.stops == 1);
        released(f);
    }

    private static void rejectsQueueFailureAndStaleGeneration() throws Exception {
        Fixture full = new Fixture();
        full.sender.reject = true;
        run(full);
        problem(full, SecureMonoCaptureSession.PROBLEM_NATIVE);
        check(full.sender.calls == 1 && full.session.snapshot().acceptedFrames() == 0);
        check(full.sender.accepted.isEmpty());

        Fixture nativeEnds = new Fixture();
        nativeEnds.recorder.onRead = () -> nativeEnds.sender.state =
                SecureMonoCaptureSession.NativeState.STOPPED;
        run(nativeEnds);
        problem(nativeEnds, SecureMonoCaptureSession.PROBLEM_NATIVE);
        check(nativeEnds.sender.calls == 0);

        Fixture stale = new Fixture();
        stale.sender.expectedGeneration = GENERATION + 1;
        run(stale);
        problem(stale, SecureMonoCaptureSession.PROBLEM_NATIVE);
        check(stale.platform.opens == 0 && stale.sender.calls == 0);

        Fixture alreadyActive = new Fixture();
        alreadyActive.sender.state = SecureMonoCaptureSession.NativeState.ACTIVE;
        run(alreadyActive);
        problem(alreadyActive, SecureMonoCaptureSession.PROBLEM_NATIVE);
        check(alreadyActive.platform.opens == 0);
    }

    private static void stopDoesNotReleasePendingInitialization() throws Exception {
        Fixture f = new Fixture();
        Gate opening = new Gate();
        f.platform.onOpen = opening::hold;
        check(f.session.start());
        opening.entered();
        check(f.session.snapshot().state() == SecureMonoCaptureSession.State.PREPARED);
        f.session.stop();
        check(!f.session.awaitStopped(0));
        check(f.session.snapshot().reapTimedOut());
        check(!f.session.snapshot().cleanupComplete());
        check(f.recorder.releases == 0 && f.recorder.stops == 0);
        opening.release();
        done(f.session);
        check(f.recorder.releases == 1 && f.recorder.starts == 0);
        check(f.sender.calls == 0 && f.sender.stops == 1);
    }

    private static void rejectsNativeStateRegression() throws Exception {
        Fixture f = new Fixture();
        f.sender.onRecord = () -> f.sender.state = SecureMonoCaptureSession.NativeState.ACTIVE;
        f.recorder.onRead = () -> {
            if (f.recorder.reads == 2) f.sender.state = SecureMonoCaptureSession.NativeState.READY;
        };
        run(f);
        problem(f, SecureMonoCaptureSession.PROBLEM_NATIVE);
        check(f.sender.accepted.size() == 1 && f.recorder.reads == 2);
    }

    private static void stopDoesNotReleasePendingReadOrBuffer() throws Exception {
        Fixture f = new Fixture();
        Gate reading = new Gate();
        f.recorder.onRead = reading::hold;
        check(f.session.start());
        reading.entered();
        byte[] retained = f.recorder.lastBuffer;
        retained[0] = 99;
        f.session.stop();
        f.session.stop();
        check(!f.session.awaitStopped(0));
        check(f.recorder.releases == 0 && f.recorder.stops == 0);
        check(retained[0] == 99); // Stop never clears in-use memory.
        reading.release();
        done(f.session);
        check(f.sender.calls == 0 && f.sender.stops == 1);
        check(allZero(retained));
        released(f);
    }

    private static void stopBeforeStartIsTerminal() throws Exception {
        Fixture f = new Fixture();
        f.session.stop();
        f.session.stop();
        check(!f.session.start());
        check(f.session.awaitStopped(0));
        check(f.platform.opens == 0 && f.sender.stops == 1);
        check(f.session.generation() == GENERATION);
        check(f.session.snapshot().generation() == GENERATION);
        expectIllegal(() -> new SecureMonoCaptureSession(0, f.platform, f.sender));
        expectIllegal(() -> f.session.awaitStopped(251));
        expectIllegal(() -> f.session.awaitStopped(-1));
    }

    private static void cleansUpOnceDespiteExceptions() throws Exception {
        Fixture open = new Fixture();
        open.platform.onOpen = () -> { throw new IllegalStateException("synthetic"); };
        run(open);
        problem(open, SecureMonoCaptureSession.PROBLEM_PLATFORM);
        check(open.recorder.releases == 0 && open.sender.stops == 1);

        Fixture start = new Fixture();
        start.recorder.throwStart = true;
        run(start);
        problem(start, SecureMonoCaptureSession.PROBLEM_PLATFORM);
        released(start);

        Fixture read = new Fixture();
        read.recorder.onRead = () -> { throw new IllegalStateException("synthetic"); };
        run(read);
        problem(read, SecureMonoCaptureSession.PROBLEM_PLATFORM);
        released(read);

        Fixture cleanup = new Fixture();
        cleanup.recorder.throwStop = true;
        cleanup.recorder.throwRelease = true;
        cleanup.sender.reject = true;
        cleanup.sender.throwStop = true;
        run(cleanup);
        problem(cleanup, SecureMonoCaptureSession.PROBLEM_NATIVE);
        check(cleanup.session.snapshot().cleanupFailed());
        check(cleanup.session.snapshot().cleanupComplete());
        released(cleanup);
        cleanup.session.close();
        check(cleanup.sender.stops == 1 && cleanup.recorder.releases == 1);
    }

    private static void exposesNoUdpOrEndpointBoundary() throws Exception {
        Method record = SecureMonoCaptureSession.NativeSender.class.getMethod(
                "record", long.class, byte[].class);
        check(record.getReturnType() == boolean.class);
        check(SecureMonoCaptureSession.NativeSender.class.getDeclaredMethods().length == 3);
        // Native gets only a generation and bounded PCM, never host, port,
        // credentials, media counters, or a trusted-lab endpoint object.
        for (Method method : SecureMonoCaptureSession.NativeSender.class.getDeclaredMethods()) {
            for (Class<?> parameter : method.getParameterTypes()) {
                check(parameter == long.class || parameter == byte[].class);
            }
        }
    }

    private static void processReservationSurvivesOwnerRecreation() throws Exception {
        SecureMonoCaptureReservation gate = new SecureMonoCaptureReservation();
        SecureMonoCaptureReservation.Lease first = gate.tryAcquire();
        check(first != null && gate.isHeld());
        check(gate.tryAcquire() == null);
        first.completeCleanup(true);
        check(!gate.isHeld());
        SecureMonoCaptureReservation.Lease second = gate.tryAcquire();
        check(second != null);
        first.completeCleanup(true); // Stale cleanup cannot release a new owner.
        check(gate.isHeld() && gate.tryAcquire() == null);
        second.completeCleanup(false);
        second.completeCleanup(true); // Failure cannot later be silently relabeled clean.
        check(gate.isHeld() && gate.tryAcquire() == null);

        SecureMonoCaptureReservation pending = new SecureMonoCaptureReservation();
        SecureMonoCaptureReservation.Lease owned = pending.tryAcquire();
        Fixture f = new Fixture();
        f.platform.onCleanup = owned::completeCleanup;
        Gate opening = new Gate();
        f.platform.onOpen = opening::hold;
        check(f.session.start());
        opening.entered();
        f.session.stop();
        check(pending.isHeld() && pending.tryAcquire() == null);
        check(f.platform.cleanups == 0);
        opening.release();
        done(f.session);
        check(!pending.isHeld());
        check(f.platform.cleanups == 1 && f.platform.cleanCompletion);

        SecureMonoCaptureReservation failed = new SecureMonoCaptureReservation();
        SecureMonoCaptureReservation.Lease uncertain = failed.tryAcquire();
        Fixture bad = new Fixture();
        bad.platform.onCleanup = uncertain::completeCleanup;
        bad.recorder.throwRelease = true;
        bad.sender.reject = true;
        run(bad);
        check(bad.platform.cleanups == 1 && !bad.platform.cleanCompletion);
        check(failed.isHeld() && failed.tryAcquire() == null);
    }

    private static void processReservationWaitsForCompleteCleanInvalidation() throws Exception {
        for (boolean nativeStopFails : new boolean[] { false, true }) {
            SecureMonoCaptureReservation gate = new SecureMonoCaptureReservation();
            SecureMonoCaptureReservation.Lease owned = gate.tryAcquire();
            Fixture f = new Fixture();
            f.platform.onCleanup = owned::completeCleanup;
            Gate reading = new Gate();
            Gate stopping = new Gate();
            CountDownLatch released = new CountDownLatch(1);
            f.recorder.onRead = reading::hold;
            f.recorder.onRelease = released::countDown;
            f.sender.onStop = stopping::hold;
            f.sender.throwStop = nativeStopFails;
            check(f.session.start());
            reading.entered();
            Thread stopOwner = new Thread(f.session::stop, "synthetic-stop-owner");
            stopOwner.start();
            stopping.entered();
            reading.release();
            check(released.await(5, TimeUnit.SECONDS));
            check(!f.session.awaitStopped(0));
            check(f.platform.cleanups == 0 && gate.isHeld());
            stopping.release();
            stopOwner.join(5_000);
            check(!stopOwner.isAlive());
            done(f.session);
            check(f.platform.cleanups == 1);
            check(gate.isHeld() == nativeStopFails);
            check(f.platform.cleanCompletion != nativeStopFails);
            released(f);
        }
    }

    private static void run(Fixture f) throws Exception {
        check(f.session.start());
        done(f.session);
    }

    private static void done(SecureMonoCaptureSession session) throws Exception {
        // Individual production reap calls stay bounded. The test harness has
        // a separate finite scheduling tolerance for loaded hosted runners.
        for (int i = 0; i < 40; i++) {
            if (session.awaitStopped(250)) return;
        }
        throw new AssertionError("capture worker did not finish");
    }

    private static void problem(Fixture f, String expected) {
        check(f.session.snapshot().state() == SecureMonoCaptureSession.State.FAILED);
        check(expected.equals(f.session.snapshot().problemCode()));
        check(f.session.snapshot().cleanupComplete());
    }

    private static void released(Fixture f) {
        check(f.recorder.stops == 1);
        check(f.recorder.releases == 1);
        check(f.recorder.workerOnly.get());
    }

    private static boolean allZero(byte[] bytes) {
        for (byte value : bytes) if (value != 0) return false;
        return true;
    }

    private static byte sample(int index) { return (byte) (index * 37 + 11); }

    private static void check(boolean value) {
        assertions++;
        if (!value) throw new AssertionError("assertion " + assertions + " failed");
    }

    @FunctionalInterface private interface Throwing { void run() throws Exception; }

    private static void expectIllegal(Throwing action) throws Exception {
        try { action.run(); }
        catch (IllegalArgumentException expected) { check(true); return; }
        throw new AssertionError("expected IllegalArgumentException");
    }

    private static final class Gate {
        private final CountDownLatch inside = new CountDownLatch(1);
        private final CountDownLatch allowed = new CountDownLatch(1);
        void hold() {
            inside.countDown();
            boolean interrupted = false;
            for (;;) {
                try {
                    if (!allowed.await(5, TimeUnit.SECONDS)) throw new AssertionError("gate timeout");
                    break;
                } catch (InterruptedException stopped) { interrupted = true; }
            }
            if (interrupted) Thread.currentThread().interrupt();
        }
        void entered() throws Exception {
            if (!inside.await(5, TimeUnit.SECONDS)) throw new AssertionError("worker did not enter gate");
        }
        void release() { allowed.countDown(); }
    }

    private static final class Fixture {
        final FakeRecorder recorder = new FakeRecorder();
        final FakePlatform platform = new FakePlatform(recorder);
        final FakeSender sender = new FakeSender();
        final SecureMonoCaptureSession session = new SecureMonoCaptureSession(GENERATION, platform, sender);
    }

    private static final class FakePlatform implements SecureMonoCaptureSession.Platform {
        final FakeRecorder recorder;
        volatile boolean permission = true;
        volatile long now;
        boolean advanceClock = true;
        int opens;
        int polls;
        Runnable onOpen = () -> {};
        Runnable onPoll = () -> {};
        Consumer<Boolean> onCleanup = clean -> {};
        volatile int cleanups;
        volatile boolean cleanCompletion;
        FakePlatform(FakeRecorder recorder) { this.recorder = recorder; }
        @Override public boolean permissionGranted() { return permission; }
        @Override public SecureMonoCaptureSession.Recorder openRecorder(
                SecureMonoCaptureSession.Cancellation cancellation) {
            opens++;
            recorder.owner = Thread.currentThread();
            onOpen.run();
            return recorder; // Deliberately returns late to test caller ownership.
        }
        @Override public long nanoTime() { return now; }
        @Override public void awaitPoll(long nanos) throws InterruptedException {
            if (Thread.interrupted()) throw new InterruptedException();
            polls++;
            if (advanceClock) now += nanos;
            onPoll.run();
        }
        @Override public void cleanupComplete(boolean clean) {
            cleanups++;
            cleanCompletion = clean;
            onCleanup.accept(clean);
        }
    }

    private static final class FakeRecorder implements SecureMonoCaptureSession.Recorder {
        ActualAudioFormat format = new ActualAudioFormat(48_000, 1, "pcm_s16le", 1_920);
        volatile int source = SecureMonoCaptureSession.MICROPHONE_SOURCE;
        volatile int device = 11;
        volatile long revision;
        int starts;
        int reads;
        int stops;
        int releases;
        boolean throwStart;
        boolean throwStop;
        boolean throwRelease;
        volatile byte[] lastBuffer;
        Thread owner;
        final AtomicBoolean workerOnly = new AtomicBoolean(true);
        final List<byte[]> seenBuffers = new ArrayList<>();
        IntUnaryOperator readSize = remaining -> remaining;
        Runnable onRead = () -> {};
        Runnable onRelease = () -> {};
        private void owner() {
            if (Thread.currentThread() != owner) workerOnly.set(false);
        }
        @Override public ActualAudioFormat actualFormat() { owner(); return format; }
        @Override public int audioSource() { owner(); return source; }
        @Override public void start() {
            owner(); starts++;
            if (throwStart) throw new IllegalStateException("synthetic");
        }
        @Override public int routedDeviceId() { owner(); return device; }
        @Override public long routingGeneration() { owner(); return revision; }
        @Override public int read(byte[] target, int offset, int length) {
            owner(); reads++;
            check(target.length == SecureMonoCaptureSession.RECORD_BYTES);
            check(offset >= 0 && (offset & 1) == 0 && length > 0 && (length & 1) == 0);
            check(offset + length == SecureMonoCaptureSession.RECORD_BYTES);
            lastBuffer = target;
            if (!seenBuffers.contains(target)) seenBuffers.add(target);
            onRead.run();
            int count = readSize.applyAsInt(length);
            if (count > 0 && count <= length) {
                for (int i = offset; i < offset + count; i++) target[i] = sample(i);
            }
            return count;
        }
        @Override public void stop() {
            owner(); stops++;
            if (throwStop) throw new IllegalStateException("synthetic");
        }
        @Override public void release() {
            owner(); releases++;
            onRelease.run();
            if (throwRelease) throw new IllegalStateException("synthetic");
        }
    }

    private static final class FakeSender implements SecureMonoCaptureSession.NativeSender {
        volatile SecureMonoCaptureSession.NativeState state = SecureMonoCaptureSession.NativeState.READY;
        volatile boolean stopped;
        long expectedGeneration = GENERATION;
        int calls;
        int stops;
        int failAfter = Integer.MAX_VALUE;
        boolean reject;
        boolean throwStop;
        Runnable onRecord = () -> {};
        Runnable onStop = () -> {};
        final List<byte[]> accepted = new ArrayList<>();
        @Override public SecureMonoCaptureSession.NativeState state(long generation) {
            if (generation != expectedGeneration || stopped) return SecureMonoCaptureSession.NativeState.STOPPED;
            if (accepted.size() >= failAfter) return SecureMonoCaptureSession.NativeState.FAILED;
            return state;
        }
        @Override public boolean record(long generation, byte[] pcm960) {
            calls++;
            onRecord.run();
            if (reject || stopped || generation != expectedGeneration || pcm960.length != 960) return false;
            accepted.add(pcm960.clone());
            return true;
        }
        @Override public void stop(long generation) {
            stops++;
            stopped = true;
            onStop.run();
            if (throwStop) throw new IllegalStateException("synthetic");
        }
    }
}
