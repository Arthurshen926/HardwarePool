package dev.capyio.display.app;

import java.io.ByteArrayInputStream;
import java.net.InetSocketAddress;
import java.security.KeyFactory;
import java.security.KeyStore;
import java.security.PrivateKey;
import java.security.SecureRandom;
import java.security.cert.Certificate;
import java.security.cert.CertificateFactory;
import java.security.spec.PKCS8EncodedKeySpec;
import java.util.Arrays;

import javax.net.ssl.KeyManagerFactory;
import javax.net.ssl.SSLContext;
import javax.net.ssl.SSLParameters;
import javax.net.ssl.SSLSocket;
import javax.net.ssl.TrustManagerFactory;

/** Shared internal TLS 1.3/mTLS connector for the display Adapter. */
final class MutualTls13Socket {
    private MutualTls13Socket() {}

    static SSLSocket connect(
            String host,
            int port,
            int timeoutMillis,
            byte[] privateCaDer,
            byte[] clientCertificateDer,
            byte[] privateKeyBytes,
            String alpn) throws Exception {
        try {
            CertificateFactory certificates = CertificateFactory.getInstance("X.509");
            Certificate clientCertificate = certificates.generateCertificate(
                    new ByteArrayInputStream(clientCertificateDer));
            PrivateKey clientPrivateKey = KeyFactory.getInstance("EC").generatePrivate(
                    new PKCS8EncodedKeySpec(privateKeyBytes));
            KeyStore identity = KeyStore.getInstance(KeyStore.getDefaultType());
            identity.load(null);
            identity.setKeyEntry(
                    "capyio-display-client", clientPrivateKey, new char[0],
                    new Certificate[] {clientCertificate});
            KeyManagerFactory keyManagers = KeyManagerFactory.getInstance(
                    KeyManagerFactory.getDefaultAlgorithm());
            keyManagers.init(identity, new char[0]);

            Certificate ca = certificates.generateCertificate(
                    new ByteArrayInputStream(privateCaDer));
            KeyStore roots = KeyStore.getInstance(KeyStore.getDefaultType());
            roots.load(null);
            roots.setCertificateEntry("capyio-display-ca", ca);
            TrustManagerFactory trustManagers = TrustManagerFactory.getInstance(
                    TrustManagerFactory.getDefaultAlgorithm());
            trustManagers.init(roots);

            SSLContext context = SSLContext.getInstance("TLSv1.3");
            context.init(keyManagers.getKeyManagers(), trustManagers.getTrustManagers(),
                    new SecureRandom());
            return DisplaySocketOwnership.initialize(
                    (SSLSocket) context.getSocketFactory().createSocket(), active -> {
                        active.setEnabledProtocols(new String[] {"TLSv1.3"});
                        active.setSoTimeout(timeoutMillis);
                        SSLParameters parameters = active.getSSLParameters();
                        parameters.setEndpointIdentificationAlgorithm("HTTPS");
                        parameters.setApplicationProtocols(new String[] {alpn});
                        active.setSSLParameters(parameters);
                        active.connect(new InetSocketAddress(host, port), timeoutMillis);
                        active.startHandshake();
                        if (!"TLSv1.3".equals(active.getSession().getProtocol())
                                || !alpn.equals(active.getApplicationProtocol())) {
                            throw new SecurityException("TLS display protocol or ALPN was downgraded");
                        }
                    });
        } finally {
            Arrays.fill(privateKeyBytes, (byte) 0);
        }
    }
}
