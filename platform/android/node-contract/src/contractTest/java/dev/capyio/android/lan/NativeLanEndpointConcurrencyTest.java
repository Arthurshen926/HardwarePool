package dev.capyio.android.lan;

import java.io.IOException;
import java.net.DatagramPacket;
import java.net.DatagramSocket;
import java.net.DatagramSocketImpl;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.NetworkInterface;
import java.net.SocketAddress;
import java.net.SocketException;
import java.util.UUID;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;

/** No real sockets: deterministically holds receive while observing metrics. */
public final class NativeLanEndpointConcurrencyTest {
    private NativeLanEndpointConcurrencyTest() {}

    public static void main(String[] args) throws Exception {
        blockedIoDoesNotBlockMetrics(false);
        blockedIoDoesNotBlockMetrics(true);
        completedIoRetainsExactCounters();
        System.out.println("Android native LAN endpoint concurrency: PASS");
    }

    private static void blockedIoDoesNotBlockMetrics(boolean sending) throws Exception {
        BlockingSocket socket = new BlockingSocket();
        NativeLanPacketCodec.Binding binding = new NativeLanPacketCodec.Binding(
                UUID.randomUUID(), UUID.randomUUID(), UUID.randomUUID(), 1);
        NativeLanUdpEndpoint endpoint = new NativeLanUdpEndpoint(socket,
                new InetSocketAddress(InetAddress.getLoopbackAddress(), 40211), binding, 50);
        AtomicReference<Throwable> failure = new AtomicReference<>();
        CountDownLatch metricsStarted = new CountDownLatch(1);
        CountDownLatch metricsReturned = new CountDownLatch(1);
        Thread receiver = new Thread(() -> {
            try {
                if (sending) endpoint.send(new NativeLanPacketCodec.Packet(binding.streamId,
                        binding.streamEpoch, 0, 0, 0, 480, false, new byte[960]));
                else endpoint.receive();
                failure.compareAndSet(null, new AssertionError("fake I/O must end by close"));
            } catch (SocketException expected) {
                // Closing the endpoint must unblock in-flight receive.
            } catch (Throwable error) {
                failure.compareAndSet(null, error);
            }
        }, "contract-blocked-io");
        Thread observer = new Thread(() -> {
            metricsStarted.countDown();
            try {
                NativeLanUdpEndpoint.Metrics metrics = endpoint.metrics();
                if (metrics.datagramsReceived != 0 || metrics.packetsSent != 0) {
                    throw new AssertionError("no completed I/O may be counted");
                }
            } catch (Throwable error) {
                failure.compareAndSet(null, error);
            } finally {
                metricsReturned.countDown();
            }
        }, "contract-metrics-observer");
        receiver.setDaemon(true);
        observer.setDaemon(true);
        boolean returnedWhileReceiveBlocked = false;
        try {
            receiver.start();
            require(socket.entered.await(2, TimeUnit.SECONDS), "I/O did not enter fake socket");
            observer.start();
            require(metricsStarted.await(2, TimeUnit.SECONDS), "metrics observer did not start");
            returnedWhileReceiveBlocked = metricsReturned.await(2, TimeUnit.SECONDS);
        } finally {
            endpoint.close();
            receiver.join(2_000);
            observer.join(2_000);
        }
        require(returnedWhileReceiveBlocked, "metrics blocked behind socket I/O");
        require(socket.closed.getCount() == 0, "endpoint close did not reach socket");
        require(!receiver.isAlive() && !observer.isAlive(), "endpoint threads did not stop");
        if (failure.get() != null) throw new AssertionError("endpoint thread failed", failure.get());
    }

    private static void require(boolean condition, String message) {
        if (!condition) throw new AssertionError(message);
    }

    private static final class BlockingSocket extends DatagramSocket {
        final CountDownLatch entered = new CountDownLatch(1);
        final CountDownLatch closed = new CountDownLatch(1);
        BlockingSocket() { super(new NoIoSocketImpl()); }
        @Override public void receive(DatagramPacket packet) throws IOException { blockUntilClosed(); }
        @Override public void send(DatagramPacket packet) throws IOException { blockUntilClosed(); }
        private void blockUntilClosed() throws IOException {
            entered.countDown();
            try {
                if (!closed.await(10, TimeUnit.SECONDS)) throw new IOException("fake close deadline");
            } catch (InterruptedException interrupted) {
                Thread.currentThread().interrupt();
                throw new IOException(interrupted);
            }
            throw new SocketException("fake socket closed");
        }
        @Override public void close() { closed.countDown(); }
    }

    private static void completedIoRetainsExactCounters() throws Exception {
        InetSocketAddress peer = new InetSocketAddress(InetAddress.getLoopbackAddress(), 40211);
        NativeLanPacketCodec.Binding binding = new NativeLanPacketCodec.Binding(
                UUID.randomUUID(), UUID.randomUUID(), UUID.randomUUID(), 1);
        CompletingSocket socket = new CompletingSocket();
        try (NativeLanUdpEndpoint endpoint = new NativeLanUdpEndpoint(socket, peer, binding, 50)) {
            endpoint.send(new NativeLanPacketCodec.Packet(binding.streamId,
                    binding.streamEpoch, 0, 0, 0, 480, false, new byte[1920]));
            require(socket.sent.size() == 2, "stereo packet must fragment into two datagrams");
            socket.next(socket.sent.get(0), peer);
            require(endpoint.receive().kind == NativeLanUdpEndpoint.ReceiveOutcome.Kind.FRAGMENT,
                    "valid receive must retain binding");
            socket.next(socket.sent.get(1), new InetSocketAddress(peer.getAddress(), 40212));
            require(endpoint.receive().kind == NativeLanUdpEndpoint.ReceiveOutcome.Kind.WRONG_PEER,
                    "wrong peer must remain rejected");
            socket.next(new byte[] {1}, peer);
            require(endpoint.receive().kind == NativeLanUdpEndpoint.ReceiveOutcome.Kind.MALFORMED,
                    "malformed datagram must remain rejected");
            NativeLanUdpEndpoint.Metrics metrics = endpoint.metrics();
            require(metrics.packetsSent == 1 && metrics.datagramsSent == 2,
                    "completed send counters must remain exact");
            require(metrics.datagramsReceived == 3 && metrics.wrongPeerDatagrams == 1
                    && metrics.malformedDatagrams == 1,
                    "completed receive counters must remain exact");
        }
    }

    private static final class CompletingSocket extends DatagramSocket {
        final List<byte[]> sent = new ArrayList<>();
        private byte[] input;
        private SocketAddress source;
        CompletingSocket() { super(new NoIoSocketImpl()); }
        void next(byte[] bytes, SocketAddress address) { input = bytes; source = address; }
        @Override public void send(DatagramPacket packet) {
            sent.add(Arrays.copyOfRange(packet.getData(), packet.getOffset(),
                    packet.getOffset() + packet.getLength()));
        }
        @Override public void receive(DatagramPacket packet) {
            require(input != null, "fake input must be explicit");
            System.arraycopy(input, 0, packet.getData(), packet.getOffset(), input.length);
            packet.setLength(input.length);
            packet.setSocketAddress(source);
            input = null;
        }
        @Override public void close() {}
    }

    /** DatagramSocket's protected constructor uses this stub, never native I/O. */
    private static final class NoIoSocketImpl extends DatagramSocketImpl {
        @Override protected void create() {}
        @Override protected void bind(int port, InetAddress address) {}
        @Override protected void send(DatagramPacket packet) { throw new AssertionError("no send"); }
        @Override protected int peek(InetAddress address) { throw new AssertionError("no peek"); }
        @Override protected int peekData(DatagramPacket packet) { throw new AssertionError("no peek"); }
        @Override protected void receive(DatagramPacket packet) { throw new AssertionError("overridden receive required"); }
        @Override protected void setTTL(byte ttl) {}
        @Override protected byte getTTL() { return 0; }
        @Override protected void setTimeToLive(int ttl) {}
        @Override protected int getTimeToLive() { return 0; }
        @Override protected void join(InetAddress address) { throw new AssertionError("no multicast"); }
        @Override protected void leave(InetAddress address) { throw new AssertionError("no multicast"); }
        @Override protected void joinGroup(SocketAddress address, NetworkInterface network) { throw new AssertionError("no multicast"); }
        @Override protected void leaveGroup(SocketAddress address, NetworkInterface network) { throw new AssertionError("no multicast"); }
        @Override protected void close() {}
        @Override public void setOption(int option, Object value) {}
        @Override public Object getOption(int option) { return 0; }
    }
}
