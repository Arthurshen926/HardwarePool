package dev.capyio.display.app;

import java.util.Objects;
import java.util.Arrays;

import javax.net.ssl.SSLSocket;

/** One-shot authenticated receiver for GameStream launch material. */
public final class AuthenticatedGameStreamLaunchClient {
    private static final String ALPN = "capyio-gamestream-launch/1";
    private static final int MAX_CREDENTIAL_BYTES = 64 * 1024;

    public record Configuration(
            String host,
            int port,
            int timeoutMillis,
            byte[] privateCaDer,
            byte[] clientCertificateDer,
            byte[] clientPrivateKeyPkcs8) {
        public Configuration {
            Objects.requireNonNull(host, "host");
            Objects.requireNonNull(privateCaDer, "privateCaDer");
            Objects.requireNonNull(clientCertificateDer, "clientCertificateDer");
            Objects.requireNonNull(clientPrivateKeyPkcs8, "clientPrivateKeyPkcs8");
            if (host.isBlank() || host.length() > 253 || port <= 0 || port > 65535
                    || timeoutMillis < 100 || timeoutMillis > 30000
                    || invalidCredential(privateCaDer) || invalidCredential(clientCertificateDer)
                    || invalidCredential(clientPrivateKeyPkcs8)) {
                throw new IllegalArgumentException("GameStream launch TLS configuration is invalid");
            }
            privateCaDer = Arrays.copyOf(privateCaDer, privateCaDer.length);
            clientCertificateDer = Arrays.copyOf(clientCertificateDer, clientCertificateDer.length);
            clientPrivateKeyPkcs8 = Arrays.copyOf(clientPrivateKeyPkcs8,
                    clientPrivateKeyPkcs8.length);
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

    private AuthenticatedGameStreamLaunchClient() {}

    public static GameStreamLaunchRecord connectAndReceive(
            Configuration configuration,
            GameStreamLaunchRecord.Expected expected) throws Exception {
        Objects.requireNonNull(configuration, "configuration");
        Objects.requireNonNull(expected, "expected");
        try (SSLSocket socket = MutualTls13Socket.connect(
                configuration.host(), configuration.port(), configuration.timeoutMillis(),
                configuration.privateCaDer(), configuration.clientCertificateDer(),
                configuration.clientPrivateKeyPkcs8(), ALPN)) {
            return GameStreamLaunchExchange.receiveAndAcknowledge(
                    socket.getInputStream(), socket.getOutputStream(), expected);
        }
    }

    private static boolean invalidCredential(byte[] value) {
        return value.length == 0 || value.length > MAX_CREDENTIAL_BYTES;
    }
}
