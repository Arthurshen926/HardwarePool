package dev.capyio.android.lan;

import java.net.DatagramPacket;
import java.net.DatagramSocket;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Arrays;
import java.util.UUID;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicLong;
import java.util.concurrent.atomic.AtomicReference;

public final class NativeLanContractTest {
    private static int assertions;

    private NativeLanContractTest() {}

    public static void main(String[] args) throws Exception {
        codecPreservesDirectionNeutralMetadata();
        goldenWireMatchesRustFixture();
        unsignedWireCountersPreserveAllBits();
        fragmentedStereoIsBoundedAndReconstructable();
        boundedQueueRejectsPressureAndWrongBinding();
        pcmPacketizerMarksDropsAsDiscontinuities();
        malformedWireFailsClosed();
        udpLoopbackUsesOneExplicitPeer();
        workerPipelineMovesPcmAndStopsBoundedly();
        pcmSinkHandlesPartialWritesAndTimelineGaps();
        pcmSinkRejectsMalformedPacketsAndTerminalRestart();
        speakerSessionConfigurationIsClosedAndLiteral();
        microphoneSessionConfigurationIsClosedAndLiteral();
        configurationIsLiteralAndBounded();
        System.out.println("Android native LAN contract: PASS (" + assertions + " assertions)");
    }

    private static void goldenWireMatchesRustFixture() throws Exception {
        NativeLanPacketCodec.Binding binding = binding();
        byte[] payload = new byte[16];
        for (int index = 0; index < payload.length; index++) {
            payload[index] = (byte) index;
        }
        NativeLanPacketCodec.Packet packet = new NativeLanPacketCodec.Packet(
                binding.streamId,
                binding.streamEpoch,
                0x0102_0304_0506_0708L,
                0x1112_1314_1516_1718L,
                0x2122_2324_2526_2728L,
                480,
                true,
                payload);
        byte[] datagram = new byte[NativeLanPacketCodec.MAX_DATAGRAM_BYTES];
        int bytes = NativeLanPacketCodec.encodeFragment(binding, packet, 0, datagram);
        byte[] golden = parseHex(Files.readString(Path.of(
                System.getProperty("capyio.nativeLanFixture"))));

        check(Arrays.equals(Arrays.copyOf(datagram, bytes), golden));
    }

    private static void unsignedWireCountersPreserveAllBits() {
        NativeLanPacketCodec.Binding binding = binding();
        NativeLanPacketCodec.Packet packet = new NativeLanPacketCodec.Packet(
                binding.streamId,
                binding.streamEpoch,
                -1L,
                Long.MIN_VALUE,
                0xFEDC_BA98_7654_3210L,
                1,
                false,
                new byte[] {42});
        byte[] datagram = new byte[NativeLanPacketCodec.MAX_DATAGRAM_BYTES];
        int bytes = NativeLanPacketCodec.encodeFragment(binding, packet, 0, datagram);
        NativeLanPacketCodec.Fragment fragment =
                NativeLanPacketCodec.decodeFragment(datagram, bytes);

        check(fragment.sequence == -1L);
        check(fragment.sourceTimestampMicros == Long.MIN_VALUE);
        check(fragment.firstSampleIndex == 0xFEDC_BA98_7654_3210L);
    }

    private static void codecPreservesDirectionNeutralMetadata() {
        NativeLanPacketCodec.Binding binding = binding();
        byte[] payload = new byte[16];
        for (int index = 0; index < payload.length; index++) {
            payload[index] = (byte) index;
        }
        NativeLanPacketCodec.Packet packet = new NativeLanPacketCodec.Packet(
                binding.streamId,
                binding.streamEpoch,
                0x0102_0304_0506_0708L,
                0x1112_1314_1516_1718L,
                0x2122_2324_2526_2728L,
                480,
                true,
                payload);
        byte[] datagram = new byte[NativeLanPacketCodec.MAX_DATAGRAM_BYTES];
        int bytes = NativeLanPacketCodec.encodeFragment(binding, packet, 0, datagram);
        NativeLanPacketCodec.Fragment fragment =
                NativeLanPacketCodec.decodeFragment(datagram, bytes);

        check(fragment.matches(binding));
        check(fragment.sequence == packet.sequence);
        check(fragment.sourceTimestampMicros == packet.sourceTimestampMicros);
        check(fragment.firstSampleIndex == packet.firstSampleIndex);
        check(fragment.sampleCount == packet.sampleCount);
        check(fragment.discontinuity);
        check(fragment.fragmentIndex == 0 && fragment.fragmentCount == 1);
        check(Arrays.equals(fragment.payloadCopy(), payload));
    }

    private static void fragmentedStereoIsBoundedAndReconstructable() {
        NativeLanPacketCodec.Binding binding = binding();
        byte[] payload = stereoPayload();
        NativeLanPacketCodec.Packet packet = new NativeLanPacketCodec.Packet(
                binding.streamId,
                binding.streamEpoch,
                9,
                90_000,
                4_320,
                480,
                true,
                payload);
        check(NativeLanPacketCodec.fragmentCount(payload.length) == 2);

        byte[] firstDatagram = new byte[NativeLanPacketCodec.MAX_DATAGRAM_BYTES];
        byte[] secondDatagram = new byte[NativeLanPacketCodec.MAX_DATAGRAM_BYTES];
        int firstBytes = NativeLanPacketCodec.encodeFragment(binding, packet, 0, firstDatagram);
        int secondBytes = NativeLanPacketCodec.encodeFragment(binding, packet, 1, secondDatagram);
        check(firstBytes == NativeLanPacketCodec.MAX_DATAGRAM_BYTES);
        check(secondBytes < NativeLanPacketCodec.MAX_DATAGRAM_BYTES);

        NativeLanPacketCodec.Fragment second =
                NativeLanPacketCodec.decodeFragment(secondDatagram, secondBytes);
        NativeLanPacketCodec.Fragment first =
                NativeLanPacketCodec.decodeFragment(firstDatagram, firstBytes);
        byte[] restored = new byte[payload.length];
        System.arraycopy(
                second.payloadCopy(),
                0,
                restored,
                second.fragmentOffset,
                second.payloadCopy().length);
        System.arraycopy(
                first.payloadCopy(),
                0,
                restored,
                first.fragmentOffset,
                first.payloadCopy().length);
        check(Arrays.equals(restored, payload));

        NativeLanPacketReassembler reassembler =
                new NativeLanPacketReassembler(binding, 2);
        check(reassembler.accept(second).kind == NativeLanPacketReassembler.Kind.PENDING);
        check(reassembler.accept(second).kind == NativeLanPacketReassembler.Kind.DUPLICATE);
        NativeLanPacketReassembler.Outcome complete = reassembler.accept(first);
        check(complete.kind == NativeLanPacketReassembler.Kind.COMPLETE);
        check(Arrays.equals(complete.packet.payloadCopy(), payload));
        check(reassembler.stats().duplicateFragments == 1);
        check(reassembler.stats().completedPackets == 1);

        byte[] conflictingDatagram = firstDatagram.clone();
        conflictingDatagram[NativeLanPacketCodec.HEADER_BYTES] ^= 0x01;
        NativeLanPacketCodec.Fragment conflicting =
                NativeLanPacketCodec.decodeFragment(conflictingDatagram, firstBytes);
        NativeLanPacketReassembler conflictReassembler =
                new NativeLanPacketReassembler(binding, 2);
        check(conflictReassembler.accept(first).kind
                == NativeLanPacketReassembler.Kind.PENDING);
        check(conflictReassembler.accept(conflicting).kind
                == NativeLanPacketReassembler.Kind.MALFORMED);
        check(conflictReassembler.inflightPackets() == 0);

        NativeLanPacketCodec.Packet laterPacket = new NativeLanPacketCodec.Packet(
                binding.streamId,
                binding.streamEpoch,
                10,
                100_000,
                4_800,
                480,
                false,
                payload);
        byte[] laterDatagram = new byte[NativeLanPacketCodec.MAX_DATAGRAM_BYTES];
        int laterBytes = NativeLanPacketCodec.encodeFragment(
                binding, laterPacket, 0, laterDatagram);
        NativeLanPacketReassembler capacityReassembler =
                new NativeLanPacketReassembler(binding, 1);
        check(capacityReassembler.accept(first).kind
                == NativeLanPacketReassembler.Kind.PENDING);
        check(capacityReassembler.accept(
                NativeLanPacketCodec.decodeFragment(laterDatagram, laterBytes)).kind
                == NativeLanPacketReassembler.Kind.PENDING);
        check(capacityReassembler.inflightPackets() == 1);
        check(capacityReassembler.stats().partialEvictions == 1);
    }

    private static void boundedQueueRejectsPressureAndWrongBinding() throws Exception {
        NativeLanPacketCodec.Binding binding = binding();
        NativeLanPacketQueue queue = new NativeLanPacketQueue(binding, 1, 64);
        NativeLanPacketCodec.Packet first = packet(binding, 1, new byte[16]);
        NativeLanPacketCodec.Packet second = packet(binding, 2, new byte[16]);
        check(queue.offer(first) == NativeLanPacketQueue.OfferOutcome.ACCEPTED);
        check(queue.offer(second) == NativeLanPacketQueue.OfferOutcome.FULL_PACKETS);
        check(queue.size() == 1 && queue.queuedBytes() == 16);
        check(queue.poll(0) == first);
        check(queue.size() == 0 && queue.queuedBytes() == 0);

        NativeLanPacketQueue byteBound = new NativeLanPacketQueue(binding, 2, 8);
        check(byteBound.offer(first) == NativeLanPacketQueue.OfferOutcome.FULL_BYTES);
        NativeLanPacketCodec.Packet foreign = new NativeLanPacketCodec.Packet(
                UUID.fromString("aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee"),
                binding.streamEpoch,
                1,
                10_000,
                0,
                480,
                false,
                new byte[] {1, 2});
        check(byteBound.offer(foreign) == NativeLanPacketQueue.OfferOutcome.WRONG_BINDING);
        check(byteBound.stats().fullByteDrops == 1);
        check(byteBound.stats().wrongBindingDrops == 1);
        check(new NativeLanPacketQueue(binding, 128, 512 * 1024).size() == 0);
        expectFailure(() -> new NativeLanPacketQueue(binding, 129, 512 * 1024));
    }

    private static void pcmPacketizerMarksDropsAsDiscontinuities() throws Exception {
        NativeLanPacketCodec.Binding binding = binding();
        NativeLanPacketQueue queue = new NativeLanPacketQueue(binding, 1, 2_000);
        NativeLanPcmPacketizer packetizer = new NativeLanPcmPacketizer(
                binding,
                queue,
                48_000,
                1,
                2,
                10_000,
                100,
                0,
                1_000_000);
        byte[] threePackets = new byte[3 * 960];
        NativeLanPcmPacketizer.PushResult pressure =
                packetizer.push(threePackets, 0, threePackets.length);
        check(pressure.consumedFrames == 1_440);
        check(pressure.emittedPackets == 1 && pressure.droppedPackets == 2);
        NativeLanPacketCodec.Packet first = queue.poll(0);
        check(first.sequence == 100 && !first.discontinuity);

        NativeLanPcmPacketizer.PushResult recovered =
                packetizer.push(new byte[960], 0, 960);
        check(recovered.emittedPackets == 1 && recovered.droppedPackets == 0);
        NativeLanPacketCodec.Packet afterDrop = queue.poll(0);
        check(afterDrop.sequence == 103 && afterDrop.discontinuity);
        check(afterDrop.firstSampleIndex == 1_440);
        check(afterDrop.sourceTimestampMicros == 1_030_000);
        check(packetizer.stats().droppedPackets == 2);

        NativeLanPacketQueue partialQueue = new NativeLanPacketQueue(binding, 2, 2_000);
        NativeLanPcmPacketizer partialPacketizer = new NativeLanPcmPacketizer(
                binding, partialQueue, 48_000, 1, 2, 10_000, 0, 0, 0);
        check(partialPacketizer.push(new byte[480], 0, 480).bufferedBytes == 480);
        partialPacketizer.markDiscontinuity();
        partialPacketizer.push(new byte[960], 0, 960);
        NativeLanPacketCodec.Packet afterPartialDrop = partialQueue.poll(0);
        check(afterPartialDrop.discontinuity);
        check(afterPartialDrop.firstSampleIndex == 240);
        check(afterPartialDrop.sourceTimestampMicros == 5_000);
    }

    private static void malformedWireFailsClosed() {
        NativeLanPacketCodec.Binding binding = binding();
        NativeLanPacketCodec.Packet packet = new NativeLanPacketCodec.Packet(
                binding.streamId,
                binding.streamEpoch,
                1,
                10_000,
                480,
                480,
                false,
                new byte[] {1, 2, 3, 4});
        byte[] datagram = new byte[NativeLanPacketCodec.MAX_DATAGRAM_BYTES];
        int bytes = NativeLanPacketCodec.encodeFragment(binding, packet, 0, datagram);

        byte[] wrongVersion = datagram.clone();
        wrongVersion[4] = 2;
        expectFailure(() -> NativeLanPacketCodec.decodeFragment(wrongVersion, bytes));

        byte[] wrongFlags = datagram.clone();
        wrongFlags[5] = (byte) 0x80;
        expectFailure(() -> NativeLanPacketCodec.decodeFragment(wrongFlags, bytes));

        byte[] reserved = datagram.clone();
        reserved[102] = 1;
        expectFailure(() -> NativeLanPacketCodec.decodeFragment(reserved, bytes));

        expectFailure(() -> NativeLanPacketCodec.fragmentCount(0));
        check(NativeLanPacketCodec.fragmentCount(
                NativeLanPacketCodec.MAX_PACKET_PAYLOAD_BYTES) == 64);
        expectFailure(() -> NativeLanPacketCodec.fragmentCount(
                NativeLanPacketCodec.MAX_PACKET_PAYLOAD_BYTES + 1));
        expectFailure(() -> new NativeLanPacketCodec.Binding(
                new UUID(0, 0), binding.routeId, binding.streamId, 1));
    }

    private static void udpLoopbackUsesOneExplicitPeer() throws Exception {
        NativeLanPacketCodec.Binding binding = binding();
        InetAddress loopback = InetAddress.getLoopbackAddress();
        try (DatagramSocket senderSocket = new DatagramSocket(
                        new InetSocketAddress(loopback, 0));
                DatagramSocket receiverSocket = new DatagramSocket(
                        new InetSocketAddress(loopback, 0));
                NativeLanUdpEndpoint sender = new NativeLanUdpEndpoint(
                        senderSocket,
                        new InetSocketAddress(loopback, receiverSocket.getLocalPort()),
                        binding,
                        250);
                NativeLanUdpEndpoint receiver = new NativeLanUdpEndpoint(
                        receiverSocket,
                        new InetSocketAddress(loopback, senderSocket.getLocalPort()),
                        binding,
                        250)) {
            byte[] payload = stereoPayload();
            NativeLanPacketCodec.Packet packet = new NativeLanPacketCodec.Packet(
                    binding.streamId,
                    binding.streamEpoch,
                    3,
                    30_000,
                    1_440,
                    480,
                    false,
                    payload);
            sender.send(packet);
            NativeLanUdpEndpoint.ReceiveOutcome first = receiver.receive();
            NativeLanUdpEndpoint.ReceiveOutcome second = receiver.receive();
            check(first.kind == NativeLanUdpEndpoint.ReceiveOutcome.Kind.FRAGMENT);
            check(second.kind == NativeLanUdpEndpoint.ReceiveOutcome.Kind.FRAGMENT);
            check(first.fragment.fragmentIndex == 0);
            check(second.fragment.fragmentIndex == 1);
            check(sender.metrics().packetsSent == 1);
            check(sender.metrics().datagramsSent == 2);
            check(receiver.metrics().datagramsReceived == 2);

            try (DatagramSocket spoof = new DatagramSocket(
                    new InetSocketAddress(loopback, 0))) {
                byte[] datagram = new byte[NativeLanPacketCodec.MAX_DATAGRAM_BYTES];
                int bytes = NativeLanPacketCodec.encodeFragment(binding, packet, 0, datagram);
                spoof.send(new DatagramPacket(
                        datagram,
                        bytes,
                        new InetSocketAddress(loopback, receiverSocket.getLocalPort())));
                check(receiver.receive().kind
                        == NativeLanUdpEndpoint.ReceiveOutcome.Kind.WRONG_PEER);
                check(receiver.metrics().wrongPeerDatagrams == 1);
            }
        }
    }

    private static void workerPipelineMovesPcmAndStopsBoundedly() throws Exception {
        NativeLanPacketCodec.Binding binding = binding();
        InetAddress loopback = InetAddress.getLoopbackAddress();
        try (DatagramSocket senderSocket = new DatagramSocket(
                        new InetSocketAddress(loopback, 0));
                DatagramSocket receiverSocket = new DatagramSocket(
                        new InetSocketAddress(loopback, 0))) {
            NativeLanUdpEndpoint senderEndpoint = new NativeLanUdpEndpoint(
                    senderSocket,
                    new InetSocketAddress(loopback, receiverSocket.getLocalPort()),
                    binding,
                    50);
            NativeLanUdpEndpoint receiverEndpoint = new NativeLanUdpEndpoint(
                    receiverSocket,
                    new InetSocketAddress(loopback, senderSocket.getLocalPort()),
                    binding,
                    50);
            NativeLanPacketQueue sendQueue = new NativeLanPacketQueue(binding, 4, 8_000);
            NativeLanPacketQueue receiveQueue = new NativeLanPacketQueue(binding, 4, 8_000);
            AtomicReference<String> failure = new AtomicReference<>();
            NativeLanSenderWorker sender =
                    new NativeLanSenderWorker(senderEndpoint, sendQueue, failure::set);
            NativeLanReceiverWorker receiver = new NativeLanReceiverWorker(
                    receiverEndpoint,
                    new NativeLanPacketReassembler(binding, 4),
                    receiveQueue,
                    failure::set);
            receiver.start();
            sender.start();
            NativeLanPcmPacketizer packetizer = new NativeLanPcmPacketizer(
                    binding,
                    sendQueue,
                    48_000,
                    2,
                    2,
                    10_000,
                    10,
                    4_800,
                    500_000);
            byte[] twoPackets = new byte[3_840];
            for (int index = 0; index < twoPackets.length; index++) {
                twoPackets[index] = (byte) (index % 251);
            }
            NativeLanPcmPacketizer.PushResult pushed =
                    packetizer.push(twoPackets, 0, twoPackets.length);
            check(pushed.emittedPackets == 2 && pushed.droppedPackets == 0);

            NativeLanPacketCodec.Packet first = receiveQueue.poll(1_000);
            NativeLanPacketCodec.Packet second = receiveQueue.poll(1_000);
            check(first != null && second != null);
            check(first.sequence == 10 && second.sequence == 11);
            check(first.firstSampleIndex == 4_800 && second.firstSampleIndex == 5_280);
            check(first.sourceTimestampMicros == 500_000);
            check(second.sourceTimestampMicros == 510_000);
            check(Arrays.equals(
                    first.payloadCopy(),
                    Arrays.copyOfRange(twoPackets, 0, 1_920)));
            check(Arrays.equals(
                    second.payloadCopy(),
                    Arrays.copyOfRange(twoPackets, 1_920, 3_840)));
            check(failure.get() == null);
            NativeLanReceiverWorker.Stats receiverStats = receiver.stats();
            check(receiverStats.datagramsReceived == 4);
            check(receiverStats.wrongPeerDatagrams == 0);
            check(receiverStats.malformedDatagrams == 0);
            check(receiverStats.completedPackets == 2);
            check(receiverStats.partialEvictions == 0);
            check(receiverStats.fullPacketDrops == 0);
            check(receiverStats.fullByteDrops == 0);

            sender.stop();
            receiver.stop();
            check(!sender.isRunning() && !receiver.isRunning());
            boolean restartRejected = false;
            try {
                sender.start();
            } catch (IllegalStateException expected) {
                restartRejected = true;
            }
            check(restartRejected);
        }
    }

    private static void pcmSinkHandlesPartialWritesAndTimelineGaps() throws Exception {
        NativeLanPacketCodec.Binding binding = binding();
        NativeLanPacketQueue queue = new NativeLanPacketQueue(binding, 4, 8_000);
        byte[] firstPayload = stereoPayload();
        byte[] secondPayload = stereoPayload();
        secondPayload[0] = 99;
        check(queue.offer(new NativeLanPacketCodec.Packet(
                        binding.streamId,
                        binding.streamEpoch,
                        40,
                        400_000,
                        19_200,
                        480,
                        false,
                        firstPayload))
                == NativeLanPacketQueue.OfferOutcome.ACCEPTED);
        check(queue.offer(new NativeLanPacketCodec.Packet(
                        binding.streamId,
                        binding.streamEpoch,
                        42,
                        420_000,
                        20_160,
                        480,
                        false,
                        secondPayload))
                == NativeLanPacketQueue.OfferOutcome.ACCEPTED);

        byte[] rendered = new byte[firstPayload.length + secondPayload.length];
        AtomicLong renderedBytes = new AtomicLong();
        AtomicLong renderedFrames = new AtomicLong();
        AtomicLong resets = new AtomicLong();
        AtomicBoolean threadInitialized = new AtomicBoolean();
        AtomicReference<String> failure = new AtomicReference<>();
        CountDownLatch complete = new CountDownLatch(2);
        NativeLanPcmSinkWorker worker = new NativeLanPcmSinkWorker(
                queue,
                4,
                new NativeLanPcmSinkWorker.PcmSink() {
                    @Override
                    public int write(byte[] payload, int offset, int length) {
                        int accepted = Math.min(length, 257);
                        int destination = Math.toIntExact(renderedBytes.get());
                        System.arraycopy(payload, offset, rendered, destination, accepted);
                        renderedBytes.addAndGet(accepted);
                        return accepted;
                    }

                    @Override
                    public void reset() {
                        resets.incrementAndGet();
                    }
                },
                () -> threadInitialized.set(true),
                frames -> {
                    renderedFrames.addAndGet(frames);
                    complete.countDown();
                },
                failure::set);
        worker.start();
        check(complete.await(1, TimeUnit.SECONDS));
        worker.stop();
        check(failure.get() == null);
        check(threadInitialized.get());
        check(renderedFrames.get() == 960);
        check(resets.get() == 1);
        check(Arrays.equals(
                rendered,
                concat(firstPayload, secondPayload)));
        check(worker.stats().packetsWritten == 2);
        check(worker.stats().discontinuities == 1);
    }

    private static void pcmSinkRejectsMalformedPacketsAndTerminalRestart() throws Exception {
        NativeLanPacketCodec.Binding binding = binding();
        NativeLanPacketQueue queue = new NativeLanPacketQueue(binding, 1, 64);
        check(queue.offer(packet(binding, 1, new byte[] {1, 2}))
                == NativeLanPacketQueue.OfferOutcome.ACCEPTED);
        AtomicReference<String> failure = new AtomicReference<>();
        CountDownLatch failed = new CountDownLatch(1);
        NativeLanPcmSinkWorker worker = new NativeLanPcmSinkWorker(
                queue,
                4,
                new NativeLanPcmSinkWorker.PcmSink() {
                    @Override
                    public int write(byte[] payload, int offset, int length) {
                        return length;
                    }

                    @Override
                    public void reset() {}
                },
                ignored -> {},
                problem -> {
                    failure.set(problem);
                    failed.countDown();
                });
        worker.start();
        check(failed.await(1, TimeUnit.SECONDS));
        check(NativeLanPcmSinkWorker.PROBLEM_PACKET.equals(failure.get()));
        worker.stop();
        boolean restartRejected = false;
        try {
            worker.start();
        } catch (IllegalStateException expected) {
            restartRejected = true;
        }
        check(restartRejected);
    }

    private static void speakerSessionConfigurationIsClosedAndLiteral() {
        NativeLanPacketCodec.Binding binding = binding();
        NativeLanSpeakerSessionConfig config = new NativeLanSpeakerSessionConfig(
                "100.66.157.119",
                46_000,
                46_001,
                binding.sessionId,
                binding.routeId,
                binding.streamId,
                binding.streamEpoch);
        check(config.localAddress.getPort() == 46_000);
        check(config.peerAddress.getPort() == 46_001);
        check("100.66.157.119".equals(config.peerAddress.getAddress().getHostAddress()));
        check(config.binding.matches(binding));
        check(NativeLanSpeakerSessionConfig.SAMPLE_RATE == 48_000);
        check(NativeLanSpeakerSessionConfig.CHANNELS == 2);
        expectFailure(() -> new NativeLanSpeakerSessionConfig(
                "phone.example",
                46_000,
                46_001,
                binding.sessionId,
                binding.routeId,
                binding.streamId,
                binding.streamEpoch));
        expectFailure(() -> new NativeLanSpeakerSessionConfig(
                "255.255.255.255",
                46_000,
                46_001,
                binding.sessionId,
                binding.routeId,
                binding.streamId,
                binding.streamEpoch));
        expectFailure(() -> new NativeLanSpeakerSessionConfig(
                "100.66.157.119",
                0,
                46_001,
                binding.sessionId,
                binding.routeId,
                binding.streamId,
                binding.streamEpoch));
    }

    private static void microphoneSessionConfigurationIsClosedAndLiteral() {
        NativeLanPacketCodec.Binding binding = binding();
        NativeLanMicrophoneSessionConfig config = new NativeLanMicrophoneSessionConfig(
                "100.66.231.100",
                46_010,
                46_011,
                binding.sessionId,
                binding.routeId,
                binding.streamId,
                binding.streamEpoch);
        check(config.localAddress.getPort() == 46_010);
        check(config.peerAddress.getPort() == 46_011);
        check("100.66.231.100".equals(config.peerAddress.getAddress().getHostAddress()));
        check(config.binding.matches(binding));
        check(NativeLanMicrophoneSessionConfig.SAMPLE_RATE == 48_000);
        check(NativeLanMicrophoneSessionConfig.CHANNELS == 1);
        expectFailure(() -> new NativeLanMicrophoneSessionConfig(
                "windows.example",
                46_010,
                46_011,
                binding.sessionId,
                binding.routeId,
                binding.streamId,
                binding.streamEpoch));
        expectFailure(() -> new NativeLanMicrophoneSessionConfig(
                "255.255.255.255",
                46_010,
                46_011,
                binding.sessionId,
                binding.routeId,
                binding.streamId,
                binding.streamEpoch));
        expectFailure(() -> new NativeLanMicrophoneSessionConfig(
                "100.66.231.100",
                46_010,
                0,
                binding.sessionId,
                binding.routeId,
                binding.streamId,
                binding.streamEpoch));
    }

    private static void configurationIsLiteralAndBounded() throws Exception {
        NativeLanPacketCodec.Binding binding = binding();
        NativeLanPacketCodec.Binding foreignBinding = new NativeLanPacketCodec.Binding(
                binding.sessionId,
                UUID.fromString("12345678-1234-4234-8234-123456789abc"),
                binding.streamId,
                binding.streamEpoch);
        check(!binding.matches(foreignBinding));
        expectFailure(() -> new NativeLanPcmPacketizer(
                binding,
                new NativeLanPacketQueue(foreignBinding, 1, 2_000),
                48_000,
                1,
                2,
                10_000,
                0,
                0,
                0));
        try (DatagramSocket socket = new DatagramSocket(
                new InetSocketAddress(InetAddress.getLoopbackAddress(), 0))) {
            expectFailure(() -> {
                try {
                    new NativeLanUdpEndpoint(
                            socket,
                            InetSocketAddress.createUnresolved("example.invalid", 9000),
                            binding,
                            20);
                } catch (java.io.IOException failure) {
                    throw new IllegalStateException(failure);
                }
            });
            expectFailure(() -> {
                try {
                    new NativeLanUdpEndpoint(
                            socket,
                            new InetSocketAddress(InetAddress.getLoopbackAddress(), 9000),
                            binding,
                            0);
                } catch (java.io.IOException failure) {
                    throw new IllegalStateException(failure);
                }
            });
            expectFailure(() -> {
                try {
                    new NativeLanUdpEndpoint(
                            socket,
                            new InetSocketAddress(
                                    InetAddress.getByName("255.255.255.255"), 9000),
                            binding,
                            20);
                } catch (java.io.IOException failure) {
                    throw new IllegalStateException(failure);
                }
            });
        }

        InetAddress loopback = InetAddress.getLoopbackAddress();
        try (DatagramSocket senderSocket = new DatagramSocket(
                        new InetSocketAddress(loopback, 0));
                DatagramSocket receiverSocket = new DatagramSocket(
                        new InetSocketAddress(loopback, 0))) {
            NativeLanUdpEndpoint endpoint = new NativeLanUdpEndpoint(
                    senderSocket,
                    new InetSocketAddress(loopback, receiverSocket.getLocalPort()),
                    binding,
                    20);
            expectFailure(() -> new NativeLanSenderWorker(
                    endpoint,
                    new NativeLanPacketQueue(foreignBinding, 1, 2_000),
                    ignored -> {}));
            expectFailure(() -> new NativeLanReceiverWorker(
                    endpoint,
                    new NativeLanPacketReassembler(foreignBinding, 1),
                    new NativeLanPacketQueue(binding, 1, 2_000),
                    ignored -> {}));
            NativeLanSenderWorker stoppedBeforeStart = new NativeLanSenderWorker(
                    endpoint,
                    new NativeLanPacketQueue(binding, 1, 2_000),
                    ignored -> {});
            stoppedBeforeStart.stop();
            boolean terminalStopRejectedStart = false;
            try {
                stoppedBeforeStart.start();
            } catch (IllegalStateException expected) {
                terminalStopRejectedStart = true;
            }
            check(terminalStopRejectedStart);
        }
    }

    private static NativeLanPacketCodec.Binding binding() {
        return new NativeLanPacketCodec.Binding(
                UUID.fromString("11111111-2222-4333-8444-555555555555"),
                UUID.fromString("66666666-7777-4888-8999-aaaaaaaaaaaa"),
                UUID.fromString("bbbbbbbb-cccc-4ddd-8eee-ffffffffffff"),
                7);
    }

    private static byte[] stereoPayload() {
        byte[] payload = new byte[1_920];
        for (int index = 0; index < payload.length; index++) {
            payload[index] = (byte) (index % 251);
        }
        return payload;
    }

    private static NativeLanPacketCodec.Packet packet(
            NativeLanPacketCodec.Binding binding,
            long sequence,
            byte[] payload) {
        return new NativeLanPacketCodec.Packet(
                binding.streamId,
                binding.streamEpoch,
                sequence,
                sequence * 10_000,
                sequence * 480,
                480,
                false,
                payload);
    }

    private static byte[] parseHex(String value) {
        String[] encoded = value.trim().split("\\s+");
        byte[] decoded = new byte[encoded.length];
        for (int index = 0; index < encoded.length; index++) {
            decoded[index] = (byte) Integer.parseUnsignedInt(encoded[index], 16);
        }
        return decoded;
    }

    private static byte[] concat(byte[] first, byte[] second) {
        byte[] result = Arrays.copyOf(first, first.length + second.length);
        System.arraycopy(second, 0, result, first.length, second.length);
        return result;
    }

    private static void expectFailure(Runnable operation) {
        boolean failed = false;
        try {
            operation.run();
        } catch (IllegalArgumentException expected) {
            failed = true;
        }
        check(failed);
    }

    private static void check(boolean condition) {
        assertions++;
        if (!condition) {
            throw new AssertionError("native LAN assertion " + assertions + " failed");
        }
    }
}
