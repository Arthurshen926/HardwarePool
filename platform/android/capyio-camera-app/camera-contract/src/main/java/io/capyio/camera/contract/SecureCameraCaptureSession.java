package io.capyio.camera.contract;

import java.util.Objects;
import java.util.concurrent.TimeUnit;

/**
 * One-shot route-to-capture gate, shared by the Android service and plain-JVM tests.
 * Native Ready permits capture, never receiver Active. All callbacks must be nonblocking;
 * no callback may perform network I/O or join a cleanup worker under this gate's monitor.
 */
public final class SecureCameraCaptureSession implements AutoCloseable {
    /** Real Camera2 capture remains legacy-only until canonical finite capture is implemented. */
    public static boolean supportsCaptureFormat(String format) {
        return "cavc-avc-access-unit-v1".equals(format);
    }

    public interface Capture {
        boolean permissionGranted();
        boolean start(long generation);
        void stop();
    }

    private final SecureCameraTransport transport;
    private final String routeId;
    private final long expiry;
    private final long createdNanos;
    private final long lifetimeNanos;
    private final Capture capture;
    private boolean attempted;
    private boolean terminal;
    private boolean captureAttempted;
    private boolean capturing;
    private long generation;

    public SecureCameraCaptureSession(SecureCameraTransport transport, String routeId,
            long expiry, Capture capture) {
        this.transport = Objects.requireNonNull(transport, "transport");
        this.routeId = Objects.requireNonNull(routeId, "routeId");
        if (routeId.isEmpty() || routeId.length() > 256 || expiry <= 0) {
            throw new IllegalArgumentException("invalid secure capture route");
        }
        this.expiry = expiry;
        this.capture = Objects.requireNonNull(capture, "capture");
        createdNanos = System.nanoTime();
        lifetimeNanos = TimeUnit.MILLISECONDS.toNanos(Math.max(0, expiry - System.currentTimeMillis()));
    }

    /** Starts only native preparation. Camera capture is deferred to an authorized Ready poll. */
    public synchronized boolean start() {
        if (attempted || terminal) return false;
        attempted = true;
        try {
            if (!live() || !capture.permissionGranted()) {
                close();
                return false;
            }
            generation = transport.start(routeId);
            if (generation <= 0) {
                close();
                return false;
            }
            return true;
        } catch (RuntimeException failed) {
            close();
            return false;
        }
    }

    /** shouldRun is the current authenticated route authorization, not local send progress. */
    public synchronized void poll(boolean shouldRun) {
        if (terminal || !attempted) return;
        try {
            if (!live() || !capture.permissionGranted()) {
                close();
                return;
            }
            int state = transport.state(generation);
            if (state == SecureCameraTransport.PENDING && !captureAttempted) return;
            if (state != SecureCameraTransport.READY || (capturing && !shouldRun)) {
                close();
                return;
            }
            if (!captureAttempted && shouldRun) {
                captureAttempted = true;
                capturing = capture.start(generation);
                if (!capturing) close();
            }
        } catch (RuntimeException failed) {
            close();
        }
    }

    private boolean live() {
        return System.currentTimeMillis() < expiry
                && System.nanoTime() - createdNanos < lifetimeNanos;
    }

    public synchronized boolean terminal() { return terminal; }
    public synchronized long generation() { return generation; }
    public synchronized boolean capturing() { return capturing; }

    /** Idempotent and no join: invalidates this gate before either cleanup callback. */
    @Override public synchronized void close() {
        if (terminal) return;
        terminal = true;
        capturing = false;
        try {
            if (generation > 0) transport.stop(generation);
        } finally {
            if (captureAttempted) capture.stop();
        }
    }
}
