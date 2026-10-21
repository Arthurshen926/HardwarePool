package io.capyio.camera.contract;

import java.nio.ByteBuffer;
import java.util.List;
import java.util.concurrent.CopyOnWriteArrayList;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicInteger;

/** Fake native boundary, real AVC wire encoder, no Android, network or hardware. */
public final class SecureAvcSenderContractTest {
    public static void main(String[] args) throws Exception {
        SecureCameraCaptureSessionContractTest.run();
        completeRecordsReachExactGeneration();
        reservationOverflowFailsBeforePayloadCopy();
        stopBeforeReadyCannotStartOrCopy();
        staleGenerationAndRejectedRecordAreTerminal();
        inFlightUnitRetainsCapacity();
        expiryStopsWithoutRetry();
        System.out.println("Secure AVC sender contract tests: PASS");
    }

    private static SecureAvcSender sender(FakeTransport nativeBoundary, List<String> status) {
        byte[] id = new byte[16];
        id[0] = 1;
        return new SecureAvcSender(nativeBoundary, 7,
                new AvcWireRecordEncoder.StreamKey(id, 9), AvcEncoderConfig.baseline720p30(),
                System.currentTimeMillis() + 10_000, (text, sent, dropped) -> status.add(text));
    }

    private static EncodedAvcAccessUnit unit(long sequence) {
        return new EncodedAvcAccessUnit(sequence, sequence * 1000, false, true, false,
                new byte[] {0, 0, 0, 1, 0x65, 1});
    }

    private static void configure(SecureAvcSender sender) {
        sender.setCodecConfig(new AvcCodecConfig(new byte[] {0, 0, 1, 0x67},
                new byte[] {0, 0, 1, 0x68}));
    }

    private static void completeRecordsReachExactGeneration() throws Exception {
        FakeTransport nativeBoundary = new FakeTransport();
        List<String> status = new CopyOnWriteArrayList<>();
        SecureAvcSender sender = sender(nativeBoundary, status);
        sender.start();
        configure(sender);
        check(sender.reserveOutput(6), "capacity must be reserved before fake capture copies");
        sender.offerReserved(unit(1));
        await(() -> nativeBoundary.records.size() == 2);
        byte[] config = nativeBoundary.records.get(0);
        byte[] access = nativeBoundary.records.get(1);
        check(config[6] == 1 && access[6] == 2, "complete config then AU records");
        for (byte[] record : nativeBoundary.records) {
            check(record[0] == 'C' && record[3] == 'C', "real CAVC encoder");
            check(ByteBuffer.wrap(record).getLong(28) == 9, "epoch preserved");
            check(ByteBuffer.wrap(record).getInt(52) == record.length - 56, "one complete record");
        }
        check(status.stream().noneMatch(s -> s.equalsIgnoreCase("active")), "no fabricated Active");
        check(status.stream().anyMatch(s -> s.contains("awaiting authenticated receiver")), "truthful waiting state");
        sender.close();
        check(!sender.reserveOutput(6), "late capture rejected");
        check(nativeBoundary.starts.get() == 0, "sender never starts or retries native generation");
    }

    private static void reservationOverflowFailsBeforePayloadCopy() {
        FakeTransport nativeBoundary = new FakeTransport();
        SecureAvcSender sender = sender(nativeBoundary, new CopyOnWriteArrayList<>());
        sender.start(); // No format yet: queued units cannot leave.
        sender.offer(unit(1));
        sender.offer(unit(2));
        check(!sender.reserveOutput(EncodedAvcAccessUnit.MAX_PAYLOAD_BYTES), "third copy denied");
        check(nativeBoundary.stops.get() == 1, "overflow terminates instead of dropping oldest");
        check(nativeBoundary.records.isEmpty(), "overflow cannot send unconfigured records");
        sender.close();
        check(nativeBoundary.stops.get() == 1, "terminal stop is idempotent");
    }

    private static void stopBeforeReadyCannotStartOrCopy() {
        FakeTransport nativeBoundary = new FakeTransport();
        nativeBoundary.state = SecureCameraTransport.PENDING;
        SecureAvcSender sender = sender(nativeBoundary, new CopyOnWriteArrayList<>());
        sender.close();
        nativeBoundary.state = SecureCameraTransport.READY; // Late native completion.
        boolean rejected = false;
        try { sender.start(); } catch (IllegalStateException expected) { rejected = true; }
        check(rejected && !sender.reserveOutput(6), "late Ready cannot resurrect stopped sender");
        check(nativeBoundary.records.isEmpty(), "no records before Ready");
    }

    private static void staleGenerationAndRejectedRecordAreTerminal() throws Exception {
        FakeTransport stale = new FakeTransport();
        stale.currentGeneration = 8;
        SecureAvcSender sender = sender(stale, new CopyOnWriteArrayList<>());
        sender.start();
        check(!sender.reserveOutput(6), "stale generation cannot capture");
        FakeTransport rejected = new FakeTransport();
        rejected.reject = true;
        sender = sender(rejected, new CopyOnWriteArrayList<>());
        sender.start();
        configure(sender);
        sender.offer(unit(1));
        await(() -> rejected.stops.get() == 1);
        check(rejected.records.isEmpty() && rejected.calls.get() == 1, "enqueue rejection never retries");
    }

    private static void inFlightUnitRetainsCapacity() throws Exception {
        FakeTransport nativeBoundary = new FakeTransport();
        nativeBoundary.block = new CountDownLatch(1);
        SecureAvcSender sender = sender(nativeBoundary, new CopyOnWriteArrayList<>());
        sender.start();
        configure(sender);
        sender.offer(unit(1));
        check(nativeBoundary.entered.await(2, TimeUnit.SECONDS), "worker entered fake enqueue");
        sender.offer(unit(2));
        check(!sender.reserveOutput(6), "in-flight and queued units consume two slots");
        // Even a deliberately blocked fake transport cannot make close join the worker.
        sender.close();
        nativeBoundary.block.countDown();
        await(() -> nativeBoundary.stops.get() == 1);
    }

    private static void expiryStopsWithoutRetry() throws Exception {
        FakeTransport nativeBoundary = new FakeTransport();
        byte[] id = new byte[16]; id[0] = 1;
        SecureAvcSender sender = new SecureAvcSender(nativeBoundary, 7,
                new AvcWireRecordEncoder.StreamKey(id, 9), AvcEncoderConfig.baseline720p30(),
                System.currentTimeMillis() - 1, (s, n, d) -> {});
        sender.start();
        check(!sender.reserveOutput(6) && nativeBoundary.stops.get() == 1, "expired sender terminal");
    }

    private interface Condition { boolean met(); }
    private static void await(Condition condition) throws Exception {
        long deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(2);
        while (!condition.met() && System.nanoTime() < deadline) Thread.sleep(1);
        check(condition.met(), "timed out waiting for fake transport");
    }
    private static void check(boolean value, String reason) {
        if (!value) throw new AssertionError(reason);
    }

    private static final class FakeTransport implements SecureCameraTransport {
        final List<byte[]> records = new CopyOnWriteArrayList<>();
        final AtomicInteger starts = new AtomicInteger();
        final AtomicInteger stops = new AtomicInteger();
        final AtomicInteger calls = new AtomicInteger();
        final CountDownLatch entered = new CountDownLatch(1);
        volatile int state = READY;
        volatile long currentGeneration = 7;
        volatile boolean reject;
        volatile CountDownLatch block;
        public long start(String routeId) { starts.incrementAndGet(); return currentGeneration; }
        public int state(long generation) { return generation == currentGeneration ? state : STOPPED; }
        public boolean record(long generation, byte[] record) {
            calls.incrementAndGet();
            entered.countDown();
            if (block != null) {
                try { block.await(2, TimeUnit.SECONDS); }
                catch (InterruptedException interrupted) { Thread.currentThread().interrupt(); }
            }
            if (reject || state(generation) != READY) return false;
            records.add(record.clone());
            return true;
        }
        public void stop(long generation) { stops.incrementAndGet(); if (generation == currentGeneration) state = STOPPED; }
    }
}
