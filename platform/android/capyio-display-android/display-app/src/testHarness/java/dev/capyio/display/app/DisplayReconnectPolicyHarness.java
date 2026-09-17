package dev.capyio.display.app;

import java.io.IOException;
import java.security.GeneralSecurityException;
import javax.net.ssl.SSLHandshakeException;

public final class DisplayReconnectPolicyHarness {
    private DisplayReconnectPolicyHarness() {}

    public static void main(String[] args) {
        long[] expected = {250, 500, 1_000, 2_000, 4_000, 4_000, 4_000, 4_000};
        for (int index = 0; index < expected.length; index++) {
            long actual = DisplayReconnectPolicy.delayMillis(index + 1);
            if (actual != expected[index]) {
                throw new AssertionError("attempt " + (index + 1) + ": expected "
                        + expected[index] + "ms, got " + actual + "ms");
            }
        }
        reject(0);
        reject(DisplayReconnectPolicy.MAX_ATTEMPTS + 1);

        requireTransient(110);
        requireTransient(-100);
        requireTransient(-101);
        requireTransient(-102);
        requireTerminal(0);
        requireTerminal(-1);
        requireTerminal(-103);
        requireTerminal(-104);

        requirePermanent(new SecurityException("binding"));
        requirePermanent(new GeneralSecurityException("certificate"));
        requirePermanent(new IOException("outer", new SSLHandshakeException("TLS")));
        requireTemporary(new IOException("timeout"));
    }

    private static void reject(int attempt) {
        try {
            DisplayReconnectPolicy.delayMillis(attempt);
            throw new AssertionError("out-of-range retry attempt was accepted: " + attempt);
        } catch (IllegalArgumentException expected) {
            // Expected.
        }
    }

    private static void requireTransient(int result) {
        if (!DisplayReconnectPolicy.isTransientGameStreamResult(result)) {
            throw new AssertionError("transient GameStream result rejected: " + result);
        }
    }

    private static void requireTerminal(int result) {
        if (DisplayReconnectPolicy.isTransientGameStreamResult(result)) {
            throw new AssertionError("terminal GameStream result retried: " + result);
        }
    }

    private static void requirePermanent(Throwable error) {
        if (!DisplayReconnectPolicy.isPermanentLaunchFailure(error)) {
            throw new AssertionError("permanent launch failure retried: " + error);
        }
    }

    private static void requireTemporary(Throwable error) {
        if (DisplayReconnectPolicy.isPermanentLaunchFailure(error)) {
            throw new AssertionError("temporary launch failure rejected: " + error);
        }
    }
}
