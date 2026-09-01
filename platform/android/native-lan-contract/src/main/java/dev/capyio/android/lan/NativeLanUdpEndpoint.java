package dev.capyio.android.lan;

import java.io.Closeable;
import java.io.IOException;
import java.net.DatagramPacket;
import java.net.DatagramSocket;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.SocketAddress;
import java.util.Objects;

/**
 * Explicit-peer UDP worker boundary for the LAN lab backend.
 *
 * <p>This class must run on a media worker. Android audio callbacks never call
 * socket methods.</p>
 */
public final class NativeLanUdpEndpoint implements Closeable {
    private final DatagramSocket socket;
    private final InetSocketAddress peer;
    private final NativeLanPacketCodec.Binding binding;
    private final byte[] sendBuffer = new byte[NativeLanPacketCodec.MAX_DATAGRAM_BYTES];
    private final byte[] receiveBuffer = new byte[NativeLanPacketCodec.MAX_DATAGRAM_BYTES + 1];

    private long packetsSent;
    private long datagramsSent;
    private long datagramsReceived;
    private long wrongPeerDatagrams;
    private long malformedDatagrams;

    public NativeLanUdpEndpoint(
            DatagramSocket socket,
            InetSocketAddress peer,
            NativeLanPacketCodec.Binding binding,
            int timeoutMillis) throws IOException {
        this.socket = Objects.requireNonNull(socket, "socket");
        this.peer = validatePeer(peer);
        this.binding = Objects.requireNonNull(binding, "binding");
        if (timeoutMillis < 1 || timeoutMillis > 2_000) {
            throw new IllegalArgumentException("UDP timeout outside 1..=2000 ms");
        }
        socket.setSoTimeout(timeoutMillis);
    }

    public static NativeLanUdpEndpoint bind(
            InetSocketAddress local,
            InetSocketAddress peer,
            NativeLanPacketCodec.Binding binding,
            int timeoutMillis) throws IOException {
        Objects.requireNonNull(local, "local");
        if (local.isUnresolved()) {
            throw new IllegalArgumentException("local address must be an IP literal");
        }
        DatagramSocket socket = new DatagramSocket(local);
        try {
            return new NativeLanUdpEndpoint(socket, peer, binding, timeoutMillis);
        } catch (IOException | RuntimeException failure) {
            socket.close();
            throw failure;
        }
    }

    public synchronized void send(NativeLanPacketCodec.Packet packet) throws IOException {
        int fragmentCount = NativeLanPacketCodec.fragmentCount(packet.payloadLength());
        for (int fragmentIndex = 0; fragmentIndex < fragmentCount; fragmentIndex++) {
            int bytes = NativeLanPacketCodec.encodeFragment(
                    binding,
                    packet,
                    fragmentIndex,
                    sendBuffer);
            DatagramPacket datagram = new DatagramPacket(sendBuffer, bytes, peer);
            socket.send(datagram);
            datagramsSent = saturatingIncrement(datagramsSent);
        }
        packetsSent = saturatingIncrement(packetsSent);
    }

    public synchronized ReceiveOutcome receive() throws IOException {
        DatagramPacket datagram = new DatagramPacket(receiveBuffer, receiveBuffer.length);
        socket.receive(datagram);
        datagramsReceived = saturatingIncrement(datagramsReceived);
        SocketAddress sender = datagram.getSocketAddress();
        if (!peer.equals(sender)) {
            wrongPeerDatagrams = saturatingIncrement(wrongPeerDatagrams);
            return ReceiveOutcome.wrongPeer();
        }
        if (datagram.getLength() > NativeLanPacketCodec.MAX_DATAGRAM_BYTES) {
            malformedDatagrams = saturatingIncrement(malformedDatagrams);
            return ReceiveOutcome.malformed();
        }
        try {
            NativeLanPacketCodec.Fragment fragment = NativeLanPacketCodec.decodeFragment(
                    receiveBuffer,
                    datagram.getLength());
            if (!fragment.matches(binding)) {
                malformedDatagrams = saturatingIncrement(malformedDatagrams);
                return ReceiveOutcome.malformed();
            }
            return ReceiveOutcome.fragment(fragment);
        } catch (IllegalArgumentException malformed) {
            malformedDatagrams = saturatingIncrement(malformedDatagrams);
            return ReceiveOutcome.malformed();
        }
    }

    public InetSocketAddress localAddress() {
        return new InetSocketAddress(socket.getLocalAddress(), socket.getLocalPort());
    }

    public NativeLanPacketCodec.Binding binding() {
        return binding;
    }

    public synchronized Metrics metrics() {
        return new Metrics(
                packetsSent,
                datagramsSent,
                datagramsReceived,
                wrongPeerDatagrams,
                malformedDatagrams);
    }

    @Override
    public void close() {
        socket.close();
    }

    private static InetSocketAddress validatePeer(InetSocketAddress peer) {
        Objects.requireNonNull(peer, "peer");
        InetAddress address = peer.getAddress();
        if (peer.isUnresolved()
                || address == null
                || address.isAnyLocalAddress()
                || address.isMulticastAddress()
                || isLimitedBroadcast(address)
                || peer.getPort() == 0) {
            throw new IllegalArgumentException("peer must be a concrete unicast IP and port");
        }
        return peer;
    }

    private static boolean isLimitedBroadcast(InetAddress address) {
        byte[] bytes = address.getAddress();
        return bytes.length == 4
                && (bytes[0] & 0xff) == 0xff
                && (bytes[1] & 0xff) == 0xff
                && (bytes[2] & 0xff) == 0xff
                && (bytes[3] & 0xff) == 0xff;
    }

    private static long saturatingIncrement(long value) {
        return value == Long.MAX_VALUE ? value : value + 1;
    }

    public static final class Metrics {
        public final long packetsSent;
        public final long datagramsSent;
        public final long datagramsReceived;
        public final long wrongPeerDatagrams;
        public final long malformedDatagrams;

        private Metrics(
                long packetsSent,
                long datagramsSent,
                long datagramsReceived,
                long wrongPeerDatagrams,
                long malformedDatagrams) {
            this.packetsSent = packetsSent;
            this.datagramsSent = datagramsSent;
            this.datagramsReceived = datagramsReceived;
            this.wrongPeerDatagrams = wrongPeerDatagrams;
            this.malformedDatagrams = malformedDatagrams;
        }
    }

    public static final class ReceiveOutcome {
        public enum Kind {
            FRAGMENT,
            WRONG_PEER,
            MALFORMED
        }

        public final Kind kind;
        public final NativeLanPacketCodec.Fragment fragment;

        private ReceiveOutcome(Kind kind, NativeLanPacketCodec.Fragment fragment) {
            this.kind = kind;
            this.fragment = fragment;
        }

        private static ReceiveOutcome fragment(NativeLanPacketCodec.Fragment fragment) {
            return new ReceiveOutcome(Kind.FRAGMENT, fragment);
        }

        private static ReceiveOutcome wrongPeer() {
            return new ReceiveOutcome(Kind.WRONG_PEER, null);
        }

        private static ReceiveOutcome malformed() {
            return new ReceiveOutcome(Kind.MALFORMED, null);
        }
    }
}
