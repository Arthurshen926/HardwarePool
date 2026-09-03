package io.capyio.camera.contract;

import java.util.Arrays;
import java.util.Objects;

/**
 * Closed destination contract for the foreground camera lab exporter.
 *
 * <p>An empty user value preserves the ADB-reverse loopback lab. A non-empty
 * value must be a strict IPv4 literal inside an RFC1918, link-local, or
 * 100.64.0.0/10 shared-address range. Host names, public addresses, wildcard
 * binds, multicast and IPv6 are deliberately not representable.</p>
 */
public final class CameraTransportEndpoint {
    public enum Mode {
        ADB_REVERSE,
        TRUSTED_LAN
    }

    public static final int PORT = 38_173;
    private static final int IPV4_BYTES = 4;
    private static final int MAX_IPV4_LITERAL_CHARS = 15;
    private static final int MAX_USER_INPUT_CHARS = 32;
    private static final byte[] LOOPBACK = new byte[] {127, 0, 0, 1};

    private final Mode mode;
    private final byte[] address;
    private final String host;

    private CameraTransportEndpoint(Mode mode, byte[] address, String host) {
        this.mode = Objects.requireNonNull(mode, "mode");
        this.address = address.clone();
        this.host = Objects.requireNonNull(host, "host");
    }

    public static CameraTransportEndpoint adbReverse() {
        return new CameraTransportEndpoint(Mode.ADB_REVERSE, LOOPBACK, "127.0.0.1");
    }

    public static CameraTransportEndpoint fromUserInput(String value) {
        if (value == null) {
            throw new IllegalArgumentException("camera transport address is missing");
        }
        if (value.length() > MAX_USER_INPUT_CHARS) {
            throw new IllegalArgumentException("camera transport input is too long");
        }
        String literal = value.trim();
        if (literal.isEmpty()) {
            return adbReverse();
        }
        if (literal.length() > MAX_IPV4_LITERAL_CHARS) {
            throw new IllegalArgumentException("camera transport address is too long");
        }
        byte[] address = parseStrictIpv4(literal);
        if (!isTrustedLanAddress(address)) {
            throw new IllegalArgumentException(
                    "camera transport address is outside trusted lab ranges");
        }
        return new CameraTransportEndpoint(
                Mode.TRUSTED_LAN,
                address,
                normalizedIpv4(address));
    }

    public Mode mode() {
        return mode;
    }

    public String host() {
        return host;
    }

    public int port() {
        return PORT;
    }

    public byte[] addressBytes() {
        return address.clone();
    }

    public String modeLabel() {
        return mode == Mode.ADB_REVERSE ? "ADB reverse" : "trusted LAN lab";
    }

    @Override
    public boolean equals(Object other) {
        return other instanceof CameraTransportEndpoint endpoint
                && mode == endpoint.mode
                && Arrays.equals(address, endpoint.address);
    }

    @Override
    public int hashCode() {
        return 31 * mode.hashCode() + Arrays.hashCode(address);
    }

    @Override
    public String toString() {
        return mode + "(" + host + ":" + PORT + ")";
    }

    private static byte[] parseStrictIpv4(String literal) {
        byte[] parsed = new byte[IPV4_BYTES];
        int segment = 0;
        int value = 0;
        int digits = 0;
        int segmentStart = 0;
        for (int index = 0; index <= literal.length(); index++) {
            char character = index == literal.length() ? '.' : literal.charAt(index);
            if (character == '.') {
                if (segment >= IPV4_BYTES || digits == 0 || value > 255) {
                    throw new IllegalArgumentException("invalid IPv4 literal");
                }
                if (digits > 1 && literal.charAt(segmentStart) == '0') {
                    throw new IllegalArgumentException("IPv4 segments must be canonical decimal");
                }
                parsed[segment++] = (byte) value;
                value = 0;
                digits = 0;
                segmentStart = index + 1;
                continue;
            }
            if (character < '0' || character > '9' || digits == 3) {
                throw new IllegalArgumentException("invalid IPv4 literal");
            }
            value = value * 10 + character - '0';
            digits++;
        }
        if (segment != IPV4_BYTES) {
            throw new IllegalArgumentException("IPv4 literal must contain four segments");
        }
        return parsed;
    }

    private static boolean isTrustedLanAddress(byte[] address) {
        int first = Byte.toUnsignedInt(address[0]);
        int second = Byte.toUnsignedInt(address[1]);
        boolean privateAddress = first == 10
                || (first == 172 && second >= 16 && second <= 31)
                || (first == 192 && second == 168);
        boolean sharedAddress = first == 100 && second >= 64 && second <= 127;
        boolean linkLocal = first == 169 && second == 254;
        return privateAddress || sharedAddress || linkLocal;
    }

    private static String normalizedIpv4(byte[] address) {
        return Byte.toUnsignedInt(address[0])
                + "."
                + Byte.toUnsignedInt(address[1])
                + "."
                + Byte.toUnsignedInt(address[2])
                + "."
                + Byte.toUnsignedInt(address[3]);
    }
}
