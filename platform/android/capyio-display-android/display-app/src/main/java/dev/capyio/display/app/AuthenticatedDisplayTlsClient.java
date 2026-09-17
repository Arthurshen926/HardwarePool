package dev.capyio.display.app;

import dev.capyio.display.AvcDecoderConfiguration;
import dev.capyio.display.PrivateDisplayRecordDecoder;

import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.EOFException;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.security.GeneralSecurityException;
import java.util.Arrays;
import java.util.Objects;
import java.util.UUID;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.function.Consumer;

import javax.net.ssl.SSLHandshakeException;
import javax.net.ssl.SSLSocket;

/** TLS 1.3/mTLS receiver for one already-authorized CapyIO Display Route. */
public final class AuthenticatedDisplayTlsClient implements AutoCloseable {
    private static final byte[] MAGIC = {'C', 'P', 'Y', 'D', 'T', 'L', 'S', '2'};
    private static final int BIND_BYTES = 96;
    private static final int BIND_REQUEST = 1;
    private static final int BIND_ACK = 2;
    private static final int BIND_BODY_BYTES = 80;
    private static final int MAX_RECORD_BYTES = AvcDecoderConfiguration.MAX_ACCESS_UNIT_BYTES + 68;
    private static final int MAX_CA_BYTES = 64 * 1024;
    private static final int MAX_CLIENT_CERT_BYTES = 64 * 1024;
    private static final int MAX_CLIENT_KEY_BYTES = 64 * 1024;
    private static final int MIN_TIMEOUT_MILLIS = 100;
    private static final int MAX_TIMEOUT_MILLIS = 30_000;
    private static final String ALPN = "capyio-display/2";

    public record Binding(
            UUID sessionId,
            UUID routeId,
            UUID streamId,
            long streamEpoch,
            int width,
            int height,
            int frameRateNumerator,
            int frameRateDenominator,
            int maximumAccessUnitBytes,
            AvcDecoderConfiguration.Profile profile,
            AvcDecoderConfiguration.Level level) {
        public Binding {
            Objects.requireNonNull(sessionId, "sessionId");
            Objects.requireNonNull(routeId, "routeId");
            Objects.requireNonNull(streamId, "streamId");
            Objects.requireNonNull(profile, "profile");
            Objects.requireNonNull(level, "level");
            if (streamEpoch <= 0 || width <= 0 || height <= 0
                    || frameRateNumerator <= 0 || frameRateDenominator <= 0
                    || maximumAccessUnitBytes <= 0
                    || maximumAccessUnitBytes > AvcDecoderConfiguration.MAX_ACCESS_UNIT_BYTES) {
                throw new IllegalArgumentException("TLS display binding is outside its bounds");
            }
        }
    }

    public record Configuration(
            String host,
            int port,
            int timeoutMillis,
            byte[] privateCaDer,
            byte[] clientCertificateDer,
            byte[] clientPrivateKeyPkcs8,
            Binding binding) {
        public Configuration {
            Objects.requireNonNull(host, "host");
            Objects.requireNonNull(privateCaDer, "privateCaDer");
            Objects.requireNonNull(clientCertificateDer, "clientCertificateDer");
            Objects.requireNonNull(clientPrivateKeyPkcs8, "clientPrivateKeyPkcs8");
            Objects.requireNonNull(binding, "binding");
            if (host.isBlank() || host.length() > 253 || port <= 0 || port > 65535
                    || timeoutMillis < MIN_TIMEOUT_MILLIS || timeoutMillis > MAX_TIMEOUT_MILLIS
                    || privateCaDer.length == 0 || privateCaDer.length > MAX_CA_BYTES
                    || clientCertificateDer.length == 0
                    || clientCertificateDer.length > MAX_CLIENT_CERT_BYTES
                    || clientPrivateKeyPkcs8.length == 0
                    || clientPrivateKeyPkcs8.length > MAX_CLIENT_KEY_BYTES) {
                throw new IllegalArgumentException("TLS display client configuration is outside its bounds");
            }
            privateCaDer = Arrays.copyOf(privateCaDer, privateCaDer.length);
            clientCertificateDer = Arrays.copyOf(clientCertificateDer, clientCertificateDer.length);
            clientPrivateKeyPkcs8 = Arrays.copyOf(clientPrivateKeyPkcs8, clientPrivateKeyPkcs8.length);
        }

        @Override
        public byte[] privateCaDer() {
            return Arrays.copyOf(privateCaDer, privateCaDer.length);
        }

        @Override
        public byte[] clientCertificateDer() {
            return Arrays.copyOf(clientCertificateDer, clientCertificateDer.length);
        }

        @Override
        public byte[] clientPrivateKeyPkcs8() {
            return Arrays.copyOf(clientPrivateKeyPkcs8, clientPrivateKeyPkcs8.length);
        }
    }

    private final Configuration configuration;
    private final DisplaySurfaceController surfaceController;
    private final Consumer<String> status;
    private final AtomicBoolean closed = new AtomicBoolean();
    private volatile SSLSocket socket;
    private Thread receiverThread;

    public AuthenticatedDisplayTlsClient(
            Configuration configuration,
            DisplaySurfaceController surfaceController,
            Consumer<String> status) {
        this.configuration = Objects.requireNonNull(configuration, "configuration");
        this.surfaceController = Objects.requireNonNull(surfaceController, "surfaceController");
        this.status = Objects.requireNonNull(status, "status");
    }

    public synchronized void start() {
        if (receiverThread != null) {
            throw new IllegalStateException("TLS display receiver already started");
        }
        if (closed.get()) {
            throw new IllegalStateException("TLS display receiver is closed");
        }
        receiverThread = new Thread(this::receive, "capyio-display-tls");
        receiverThread.start();
    }

    private void receive() {
        int reconnectAttempt = 0;
        while (!closed.get()) {
            status.accept(reconnectAttempt == 0
                    ? "connecting to authenticated display source"
                    : "reconnecting to authenticated display source");
            try {
                receiveOneConnection();
                if (closed.get()) {
                    return;
                }
                throw new EOFException("display source disconnected");
            } catch (Exception error) {
                socket = null;
                if (closed.get()) {
                    return;
                }
                surfaceController.authenticatedTransportLost();
                if (isPermanentFailure(error)) {
                    status.accept("display connection rejected: " + describe(error));
                    close();
                    return;
                }
                reconnectAttempt++;
                if (reconnectAttempt > DisplayReconnectPolicy.MAX_ATTEMPTS) {
                    status.accept("display reconnect attempts exhausted: " + describe(error));
                    close();
                    return;
                }
                long delayMillis = DisplayReconnectPolicy.delayMillis(reconnectAttempt);
                status.accept("display source unavailable; retry " + reconnectAttempt
                        + "/" + DisplayReconnectPolicy.MAX_ATTEMPTS + " in "
                        + delayMillis + "ms");
                try {
                    Thread.sleep(delayMillis);
                } catch (InterruptedException interrupted) {
                    Thread.currentThread().interrupt();
                    if (!closed.get()) {
                        status.accept("display reconnect interrupted");
                        close();
                    }
                    return;
                }
            }
        }
    }

    private void receiveOneConnection() throws Exception {
        try (SSLSocket active = connect()) {
            socket = active;
            try {
                DataOutputStream output = new DataOutputStream(active.getOutputStream());
                DataInputStream input = new DataInputStream(active.getInputStream());
                output.write(encodeBinding(configuration.binding(), BIND_REQUEST));
                output.flush();
                byte[] acknowledgement = new byte[BIND_BYTES];
                input.readFully(acknowledgement);
                if (!Arrays.equals(
                        acknowledgement, encodeBinding(configuration.binding(), BIND_ACK))) {
                    throw new SecurityException(
                            "TLS display Route acknowledgement is unauthorized");
                }
                // The timeout bounds connect, handshake, and Route binding. After binding,
                // a static desktop may legitimately produce no records; close() interrupts
                // this dedicated receiver thread by closing the socket.
                active.setSoTimeout(0);
                status.accept("authenticated TLS 1.3 display stream connected");

                while (!closed.get()) {
                    int length = input.readInt();
                    if (DisplayTlsOuterFrame.isHeartbeat(length, MAX_RECORD_BYTES)) {
                        output.writeInt(DisplayTlsOuterFrame.HEARTBEAT_LENGTH);
                        output.flush();
                        continue;
                    }
                    byte[] record = new byte[length];
                    input.readFully(record);
                    try {
                        verifyRecordBinding(record);
                        surfaceController.submitAuthenticatedRecord(record);
                    } catch (RuntimeException error) {
                        throw new SecurityException(
                                "authenticated display record was rejected", error);
                    }
                }
            } finally {
                if (socket == active) {
                    socket = null;
                }
            }
        }
    }

    private static boolean isPermanentFailure(Throwable error) {
        Throwable current = error;
        for (int depth = 0; current != null && depth < 6; depth++) {
            if (current instanceof SecurityException
                    || current instanceof GeneralSecurityException
                    || current instanceof SSLHandshakeException) {
                return true;
            }
            current = current.getCause();
        }
        return false;
    }

    private SSLSocket connect() throws Exception {
        return MutualTls13Socket.connect(
                configuration.host(), configuration.port(), configuration.timeoutMillis(),
                configuration.privateCaDer(), configuration.clientCertificateDer(),
                configuration.clientPrivateKeyPkcs8(), ALPN);
    }

    private void verifyRecordBinding(byte[] record) {
        if (record.length < 6) {
            throw new IllegalArgumentException("TLS display record is truncated");
        }
        if (Byte.toUnsignedInt(record[5]) != 1) {
            return;
        }
        AvcDecoderConfiguration decoded = PrivateDisplayRecordDecoder.decodeConfiguration(record);
        Binding expected = configuration.binding();
        if (!decoded.streamId().equals(expected.streamId())
                || decoded.streamEpoch() != expected.streamEpoch()
                || decoded.width() != expected.width()
                || decoded.height() != expected.height()
                || decoded.frameRateNumerator() != expected.frameRateNumerator()
                || decoded.frameRateDenominator() != expected.frameRateDenominator()
                || decoded.maximumAccessUnitBytes() != expected.maximumAccessUnitBytes()
                || decoded.profile() != expected.profile()
                || decoded.level() != expected.level()) {
            throw new SecurityException("TLS display configuration differs from Route binding");
        }
    }

    private static byte[] encodeBinding(Binding binding, int kind) {
        ByteBuffer bytes = ByteBuffer.allocate(BIND_BYTES).order(ByteOrder.BIG_ENDIAN);
        bytes.put(MAGIC);
        bytes.put((byte) 2);
        bytes.put((byte) kind);
        bytes.putShort((short) 0);
        bytes.putInt(BIND_BODY_BYTES);
        putUuid(bytes, binding.sessionId());
        putUuid(bytes, binding.routeId());
        putUuid(bytes, binding.streamId());
        bytes.putLong(binding.streamEpoch());
        bytes.putInt(binding.width());
        bytes.putInt(binding.height());
        bytes.putInt(binding.frameRateNumerator());
        bytes.putInt(binding.frameRateDenominator());
        bytes.putInt(binding.maximumAccessUnitBytes());
        bytes.put((byte) 1);
        bytes.put((byte) (binding.profile().ordinal() + 1));
        bytes.put((byte) (binding.level().ordinal() + 1));
        bytes.put((byte) 0);
        return bytes.array();
    }

    private static void putUuid(ByteBuffer bytes, UUID value) {
        bytes.putLong(value.getMostSignificantBits());
        bytes.putLong(value.getLeastSignificantBits());
    }

    private static String describe(Throwable error) {
        StringBuilder message = new StringBuilder();
        Throwable current = error;
        for (int depth = 0; current != null && depth < 5 && message.length() < 480; depth++) {
            if (depth > 0) {
                message.append(" <- ");
            }
            message.append(current.getClass().getSimpleName());
            if (current.getMessage() != null) {
                message.append(": ").append(current.getMessage());
            }
            current = current.getCause();
        }
        return message.toString();
    }

    @Override
    public synchronized void close() {
        if (!closed.compareAndSet(false, true)) {
            return;
        }
        SSLSocket active = socket;
        if (active != null) {
            try {
                active.close();
            } catch (Exception ignored) {
                // Shutdown remains idempotent.
            }
        }
        Thread thread = receiverThread;
        if (thread != null) {
            thread.interrupt();
        }
        Arrays.fill(configuration.clientPrivateKeyPkcs8, (byte) 0);
    }
}
