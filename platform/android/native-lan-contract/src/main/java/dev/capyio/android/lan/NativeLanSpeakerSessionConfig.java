package dev.capyio.android.lan;

import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.UnknownHostException;
import java.util.UUID;

/** Closed trusted-lab configuration for one native speaker Route epoch. */
public final class NativeLanSpeakerSessionConfig {
    public static final int SAMPLE_RATE = 48_000;
    public static final int CHANNELS = 2;
    public static final int BYTES_PER_SAMPLE = 2;
    public static final int FRAME_DURATION_MICROS = 10_000;

    public final NativeLanPacketCodec.Binding binding;
    public final InetSocketAddress localAddress;
    public final InetSocketAddress peerAddress;

    public NativeLanSpeakerSessionConfig(
            String peerIpv4,
            int localPort,
            int peerPort,
            UUID sessionId,
            UUID routeId,
            UUID streamId,
            int streamEpoch) {
        if (localPort < 1 || localPort > 65_535 || peerPort < 1 || peerPort > 65_535) {
            throw new IllegalArgumentException("speaker ports must be non-zero u16 values");
        }
        binding = new NativeLanPacketCodec.Binding(
                sessionId, routeId, streamId, streamEpoch);
        localAddress = new InetSocketAddress(ipv4Literal("0.0.0.0"), localPort);
        peerAddress = new InetSocketAddress(ipv4Literal(peerIpv4), peerPort);
        InetAddress peer = peerAddress.getAddress();
        if (peer.isAnyLocalAddress()
                || peer.isMulticastAddress()
                || "255.255.255.255".equals(peer.getHostAddress())) {
            throw new IllegalArgumentException("speaker peer must be a concrete unicast IPv4");
        }
    }

    private static InetAddress ipv4Literal(String value) {
        if (value == null || value.isEmpty() || value.length() > 15) {
            throw new IllegalArgumentException("speaker address must be an IPv4 literal");
        }
        String[] parts = value.split("\\.", -1);
        if (parts.length != 4) {
            throw new IllegalArgumentException("speaker address must be an IPv4 literal");
        }
        byte[] bytes = new byte[4];
        for (int index = 0; index < parts.length; index++) {
            String part = parts[index];
            if (part.isEmpty() || part.length() > 3) {
                throw new IllegalArgumentException("speaker address must be an IPv4 literal");
            }
            int octet = 0;
            for (int characterIndex = 0; characterIndex < part.length(); characterIndex++) {
                char character = part.charAt(characterIndex);
                if (character < '0' || character > '9') {
                    throw new IllegalArgumentException("speaker address must be an IPv4 literal");
                }
                octet = octet * 10 + character - '0';
            }
            if (octet > 255) {
                throw new IllegalArgumentException("speaker address must be an IPv4 literal");
            }
            bytes[index] = (byte) octet;
        }
        try {
            return InetAddress.getByAddress(bytes);
        } catch (UnknownHostException impossible) {
            throw new IllegalStateException("validated IPv4 length was rejected", impossible);
        }
    }
}
