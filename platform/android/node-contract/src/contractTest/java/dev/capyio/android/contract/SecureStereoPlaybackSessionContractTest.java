package dev.capyio.android.contract;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;

/** Deterministic fake-clock/native/track evidence only: no Android or sockets. */
public final class SecureStereoPlaybackSessionContractTest {
    public static void main(String[] args) throws Exception {
        partialWritesAndBusyAck();
        zeroWriteConsumesPermit();
        invalidWritesFailClosed();
        pendingAndStaleGeneration();
        boundBeforeAllocation();
        stopBeforeStart();
        stopDuringInitialization();
        stopDuringWrite();
        stopDuringAck();
        rejectedAckDoesNotCount();
        stalledAckNeverRewrites();
        originalBlockBudgetDoesNotRefresh();
        invalidPlatformAndInitFailure();
        consentRevocation();
        routeChange();
        cleanupQuarantine();
        malformedPermit();
        enableRejection();
        routeReadyBeforeEnable();
        System.out.println("SecureStereoPlaybackSessionContractTest passed");
    }
    private static void partialWritesAndBusyAck() throws Exception {
        Fixture f = new Fixture(480, 1440);
        f.nativeBridge.busyAcks = 2;
        f.run();
        check(f.track.writes == 2, "busy ACK repeated platform write");
        check(f.track.offsets.equals(List.of(0, 0)), "array offset must be zero");
        check(f.nativeBridge.offsets.equals(List.of(0, 0, 0, 480)), "ACK uses logical offset");
        check(f.nativeBridge.tokens.get(0).equals(f.nativeBridge.tokens.get(2)), "busy ACK token changed");
        check(f.track.firstBytes.equals(List.of((byte) 0, (byte) 120)), "remaining slice copy incorrect");
        check(f.session.snapshot().acceptedBytes() == 1920 && f.session.snapshot().acceptedRecords() == 1, "acceptance mismatch");
        check(f.session.snapshot().state() == SecureStereoPlaybackSession.State.STOPPED, "normal completion");
    }
    private static void zeroWriteConsumesPermit() throws Exception {
        Fixture f = new Fixture(0, 1920); f.run();
        check(f.nativeBridge.ackCounts.equals(List.of(0, 1920)), "zero progress must ACK");
        check(f.nativeBridge.tokens.get(0) < f.nativeBridge.tokens.get(1), "new zero-write permit expected");
    }
    private static void invalidWritesFailClosed() throws Exception {
        for (int value : new int[] {-1, -6, 1, 2, 3, 1924}) {
            Fixture f = new Fixture(value); f.run();
            check(f.nativeBridge.ackCounts.isEmpty(), "invalid write acknowledged");
            check(f.session.snapshot().acceptedBytes() == 0, "invalid write counted");
            check(f.session.snapshot().state() == SecureStereoPlaybackSession.State.FAILED, "invalid write not failed");
        }
    }
    private static void pendingAndStaleGeneration() throws Exception {
        Fixture f = new Fixture(1920); f.nativeBridge.initialState = 0; f.run();
        check(f.platform.opens == 0 && f.nativeBridge.stops == 1, "pending timeout opens track");
        Fixture stale = new Fixture(1920); stale.nativeBridge.initialState = 3; stale.run();
        check(stale.platform.opens == 0, "stale generation opens track");
    }
    private static void boundBeforeAllocation() throws Exception {
        Fixture f = new Fixture(1920); f.run();
        check(f.events.indexOf("bound") < f.events.indexOf("open"), "allocation before Bound");
        check(f.events.indexOf("open") < f.events.indexOf("enable"), "enable before track validation");
        check(f.events.indexOf("native-stop") < f.events.indexOf("release"), "release before native revoke");
    }
    private static void stopBeforeStart() throws Exception {
        Fixture f = new Fixture(1920); f.session.stop(); f.await();
        check(!f.session.start() && f.platform.opens == 0, "stopped session revived");
    }
    private static void stopDuringInitialization() throws Exception {
        Fixture f = new Fixture(1920); f.platform.openEntered = new CountDownLatch(1); f.platform.openContinue = new CountDownLatch(1);
        check(f.session.start(), "start"); entered(f.platform.openEntered);
        f.session.stop();
        check(f.nativeBridge.stops == 1 && !f.session.snapshot().cleanupComplete(), "premature stopped claim");
        check(f.session.snapshot().state() == SecureStereoPlaybackSession.State.STOPPING, "initialization not stopping");
        check(!f.session.awaitStopped(0), "premature reap");
        f.platform.openContinue.countDown(); f.await();
        check(f.track.releases == 1 && f.track.writes == 0 && !f.nativeBridge.enabled, "late init used track");
    }
    private static void stopDuringWrite() throws Exception {
        Fixture f = new Fixture(1920); f.track.entered = new CountDownLatch(1); f.track.proceed = new CountDownLatch(1);
        f.session.start(); entered(f.track.entered); f.session.stop();
        check(f.track.releases == 0, "release while write in flight");
        f.track.proceed.countDown(); f.await();
        check(f.nativeBridge.ackCounts.isEmpty() && f.track.releases == 1, "late write acknowledged");
    }
    private static void stopDuringAck() throws Exception {
        Fixture f = new Fixture(1920); f.nativeBridge.ackEntered = new CountDownLatch(1); f.nativeBridge.ackContinue = new CountDownLatch(1);
        f.session.start(); entered(f.nativeBridge.ackEntered); f.session.stop();
        check(f.track.releases == 0, "release during ACK");
        f.nativeBridge.ackContinue.countDown(); f.await();
        check(f.session.snapshot().acceptedBytes() == 0, "late ACK published acceptance");
    }
    private static void rejectedAckDoesNotCount() throws Exception {
        Fixture f = new Fixture(1920); f.nativeBridge.rejectAck = true; f.run();
        check(f.session.snapshot().acceptedBytes() == 0 && f.track.writes == 1, "rejected ACK counted/replayed");
    }
    private static void stalledAckNeverRewrites() throws Exception {
        Fixture f = new Fixture(1920); f.nativeBridge.busyAcks = 1000; f.run();
        check(f.track.writes == 1 && f.session.snapshot().acceptedBytes() == 0, "stalled ACK rewrites");
        check(f.platform.now <= 250_000_000L, "unbounded ACK budget");
    }
    private static void originalBlockBudgetDoesNotRefresh() throws Exception {
        Fixture f = new Fixture(4, 4, 1912); f.track.writeAdvance = 125_000_000L; f.run();
        check(f.track.writes == 2 && f.nativeBridge.ackCounts.size() == 1, "partial progress refreshed record deadline");
    }
    private static void invalidPlatformAndInitFailure() throws Exception {
        Fixture f = new Fixture(1920); f.track.format = new ActualAudioFormat(44100, 2, "pcm_s16le", 480); f.run();
        check(!f.nativeBridge.enabled && f.track.writes == 0, "wrong actual rate accepted");
        Fixture capacity = new Fixture(1920); capacity.track.capacity = 4801; capacity.run();
        check(!capacity.nativeBridge.enabled, "unbounded capacity accepted");
        Fixture init = new Fixture(1920); init.platform.failOpen = true; init.run();
        check(init.nativeBridge.stops == 1 && init.session.snapshot().cleanupComplete(), "init failure cleanup");
        Fixture play = new Fixture(1920); play.track.failPlay = true; play.run();
        check(play.track.releases == 1 && play.track.writes == 0, "play failure ownership");
    }
    private static void consentRevocation() throws Exception {
        Fixture f = new Fixture(1920); f.platform.permitted = false; f.run();
        check(f.platform.opens == 0, "no visible playback consent");
        Fixture revoked = new Fixture(1920); revoked.track.afterWrite = () -> revoked.platform.permitted = false; revoked.run();
        check(revoked.nativeBridge.ackCounts.isEmpty(), "revoked write ACKed");
    }
    private static void routeChange() throws Exception {
        Fixture f = new Fixture(480, 1440); f.track.afterWrite = () -> f.track.routeRevision++; f.run();
        check(f.track.writes == 1, "changed route reused");
    }
    private static void malformedPermit() throws Exception {
        for (int corruption = 1; corruption <= 3; corruption++) {
            Fixture f = new Fixture(1920); f.nativeBridge.corruption = corruption; f.run();
            check(f.track.writes == 0, "malformed native permit used");
        }
    }
    private static void routeReadyBeforeEnable() throws Exception {
        Fixture delayed = new Fixture(1920); delayed.track.routeReadyAt = 3_000_000L; delayed.run();
        check(delayed.events.indexOf("play") < delayed.events.indexOf("route-ready"), "route checked before play");
        check(delayed.events.indexOf("route-ready") < delayed.events.indexOf("enable"), "enable before route readiness");
        Fixture unavailable = new Fixture(1920); unavailable.track.routeReadyAt = Long.MAX_VALUE; unavailable.run();
        check(!unavailable.nativeBridge.enabled && unavailable.track.writes == 0, "missing route enabled or probed");
        Fixture changed = new Fixture(1920);
        changed.nativeBridge.afterEnable = () -> changed.track.routeRevision++;
        changed.run();
        check(changed.track.writes == 0, "route changed during enable used for PCM");
        Fixture revoked = new Fixture(1920);
        revoked.nativeBridge.afterEnable = () -> revoked.platform.permitted = false;
        revoked.run();
        check(revoked.track.writes == 0, "consent revoked during enable used for PCM");
    }
    private static void enableRejection() throws Exception {
        Fixture f = new Fixture(1920); f.nativeBridge.rejectEnable = true; f.run();
        check(!f.track.playing && f.track.writes == 0 && f.track.releases == 1, "rejected enable opens playback");
    }
    private static void cleanupQuarantine() throws Exception {
        SecureStereoPlaybackReservation reservation = new SecureStereoPlaybackReservation();
        SecureStereoPlaybackReservation.Lease lease = reservation.tryAcquire();
        check(lease != null && reservation.tryAcquire() == null, "overlapping process owner");
        Fixture f = new Fixture(1920); f.track.failRelease = true; f.platform.cleanup = lease::completeCleanup; f.run();
        check(f.session.snapshot().cleanupFailed() && f.session.snapshot().state() == SecureStereoPlaybackSession.State.FAILED, "false stopped cleanup");
        lease.completeCleanup(true);
        check(reservation.tryAcquire() == null, "quarantine cleared after uncertain release");
        Fixture nativeFailure = new Fixture(1920); nativeFailure.nativeBridge.failStop = true; nativeFailure.run();
        check(nativeFailure.session.snapshot().cleanupFailed(), "native stop failure hidden");
        SecureStereoPlaybackReservation clean = new SecureStereoPlaybackReservation();
        clean.tryAcquire().completeCleanup(true);
        check(clean.tryAcquire() != null, "clean owner not reusable");
    }
    private static void entered(CountDownLatch latch) throws Exception { check(latch.await(2, TimeUnit.SECONDS), "worker did not reach test barrier"); }
    private static void hold(CountDownLatch latch) {
        if (latch == null) return;
        boolean interrupted = false;
        while (true) {
            try { latch.await(); break; } catch (InterruptedException ignored) { interrupted = true; }
        }
        if (interrupted) Thread.currentThread().interrupt();
    }
    private static void check(boolean value, String message) { if (!value) throw new AssertionError(message); }
    private static final class Fixture {
        final List<String> events = new ArrayList<>();
        final FakePlatform platform = new FakePlatform(this);
        final FakeTrack track;
        final FakeNative nativeBridge = new FakeNative(this);
        final SecureStereoPlaybackSession session;
        Fixture(int... writes) {
            track = new FakeTrack(this, writes);
            session = new SecureStereoPlaybackSession(7, platform, nativeBridge);
        }
        void run() throws Exception { check(session.start(), "session start"); await(); }
        void await() throws Exception {
            check(session.awaitStopped(250), "worker did not finish");
            check(nativeBridge.stops == 1, "native stop not exactly once");
        }
    }
    private static final class FakePlatform implements SecureStereoPlaybackSession.Platform {
        final Fixture f;
        long now;
        volatile boolean permitted = true;
        boolean failOpen;
        int opens;
        CountDownLatch openEntered, openContinue;
        java.util.function.Consumer<Boolean> cleanup = clean -> {};
        FakePlatform(Fixture f) { this.f = f; }
        public boolean permitted() { return permitted; }
        public SecureStereoPlaybackSession.Track openTrack(SecureStereoPlaybackSession.Cancellation cancellation) {
            opens++; f.events.add("open");
            if (openEntered != null) openEntered.countDown();
            hold(openContinue);
            if (failOpen) throw new IllegalStateException("synthetic init failure");
            return f.track;
        }
        public long nanoTime() { return now; }
        public void awaitPoll(long nanos) { now += nanos; }
        public void cleanupComplete(boolean clean) { cleanup.accept(clean); }
    }
    private static final class FakeTrack implements SecureStereoPlaybackSession.Track {
        final Fixture f;
        final int[] returns;
        final List<Integer> offsets = new ArrayList<>();
        final List<Byte> firstBytes = new ArrayList<>();
        ActualAudioFormat format = new ActualAudioFormat(48000, 2, "pcm_s16le", 1920);
        int capacity = 1920, writes, releases;
        long routeRevision, writeAdvance, routeReadyAt;
        boolean playing, failPlay, failRelease;
        Runnable afterWrite = () -> {};
        CountDownLatch entered, proceed;
        FakeTrack(Fixture f, int[] returns) { this.f = f; this.returns = returns; }
        public ActualAudioFormat actualFormat() { return format; }
        public boolean initialized() { return true; }
        public int capacityFrames() { return capacity; }
        public void play() { if (failPlay) throw new IllegalStateException("play failed"); playing = true; f.events.add("play"); }
        public boolean playing() { return playing; }
        public int routedDeviceId() {
            if (!playing || f.platform.now < routeReadyAt) return 0;
            f.events.add("route-ready");
            return 23;
        }
        public long routingGeneration() { return routeRevision; }
        public int write(byte[] pcm, int offset, int length) {
            check(pcm.length == 1920 && offset == 0, "fixed buffer/slice offset");
            offsets.add(offset); firstBytes.add(pcm[0]);
            if (entered != null) entered.countDown(); hold(proceed);
            f.platform.now += writeAdvance;
            int result = returns[Math.min(writes++, returns.length - 1)];
            afterWrite.run(); return result;
        }
        public void pause() { playing = false; }
        public void flush() { check(f.nativeBridge.stops == 1, "flush before revoke"); }
        public void release() {
            f.events.add("release"); releases++;
            if (failRelease) throw new IllegalStateException("release uncertain");
        }
    }
    private static final class FakeNative implements SecureStereoPlaybackSession.NativeReceiver {
        final Fixture f;
        final List<Integer> offsets = new ArrayList<>(), ackCounts = new ArrayList<>();
        final List<Long> tokens = new ArrayList<>();
        int initialState = 1, busyAcks, position, records;
        volatile int stops;
        long token;
        boolean enabled, rejectAck, failStop, rejectEnable;
        int corruption;
        Runnable afterEnable = () -> {};
        CountDownLatch ackEntered, ackContinue;
        FakeNative(Fixture f) { this.f = f; }
        private void generation(long value) { check(value == 7, "stale generation forwarded"); }
        public int state(long generation, long[] counters) {
            generation(generation); check(counters.length == 3, "bounded counters");
            if (stops > 0 || records == 1) return 3;
            if (enabled) return 4;
            if (initialState == 1) f.events.add("bound");
            return initialState;
        }
        public boolean enable(long generation) {
            generation(generation); check(f.platform.opens == 1, "enable before open");
            check(f.track.playing && f.track.routedDeviceId() > 0, "enable before playing route ready");
            f.events.add("enable"); enabled = !rejectEnable; afterEnable.run(); return enabled;
        }
        public int read(long generation, byte[] pcm, long[] metadata) {
            generation(generation); check(enabled && stops == 0, "read without admission");
            for (int i = 0; i < 1920 - position; i++) pcm[i] = (byte) ((position + i) / 4);
            metadata[0] = ++token; metadata[1] = position; metadata[2] = 1920 - position;
            if (corruption == 1) metadata[0] = 0;
            if (corruption == 2) metadata[1] += 4;
            if (corruption == 3) metadata[2] -= 1;
            return 1;
        }
        public int ack(long generation, long permit, int offset, int count) {
            generation(generation); offsets.add(offset); ackCounts.add(count); tokens.add(permit);
            check(permit == token && offset == position, "incorrect ACK identity");
            if (ackEntered != null) ackEntered.countDown(); hold(ackContinue);
            if (stops > 0 || rejectAck) return -1;
            if (busyAcks-- > 0) return 0;
            position += count;
            if (position == 1920) records++;
            return 1;
        }
        public void stop(long generation) {
            generation(generation); stops++; f.events.add("native-stop");
            if (failStop) throw new IllegalStateException("stop uncertain");
        }
    }
}
