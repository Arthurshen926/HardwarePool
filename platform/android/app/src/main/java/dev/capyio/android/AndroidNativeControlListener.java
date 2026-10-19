package dev.capyio.android;

/** Owns one native non-blocking control listener and its bounded accept worker. */
final class AndroidNativeControlListener implements AutoCloseable {
    static {
        System.loadLibrary("capyio_android_jni");
    }

    private final AndroidNativeCatalogRuntime runtime;
    private final int boundPort;
    private final Thread worker;
    private volatile boolean closing;
    private final AndroidControlConnectionCycle cycle;
    private volatile boolean discoveryAnnouncing;
    private volatile String failure;
    private volatile long handle;
    private DialRequest pendingDial;

    private static final class DialRequest {
        final String host;
        final int port;
        final byte[] directory;
        final byte[] credential;

        DialRequest(String host, int port, byte[] directory, byte[] credential) {
            this.host = host;
            this.port = port;
            this.directory = directory.clone();
            this.credential = credential.clone();
        }
    }

    AndroidNativeControlListener(
            AndroidNativeCatalogRuntime runtime,
            String localNodeId,
            byte[] pairedDirectoryJson,
            byte[] encodedCredential) {
        this.runtime = runtime;
        handle = nativeCreate(localNodeId, pairedDirectoryJson, encodedCredential);
        if (handle == 0) {
            throw new IllegalStateException("native control listener returned an invalid handle");
        }
        // The listening socket stays bound for this owner's entire lifetime.
        // Cache before starting the worker: UI status must not acquire its
        // native Listener mutex while it waits for a peer's TLS/control I/O.
        boundPort = nativePort(handle);
        cycle = new AndroidControlConnectionCycle(new AndroidControlConnectionCycle.Connection() {
            public boolean accept() {
                DialRequest request = takePendingDial();
                if (request == null) return nativeTryAccept(handle, runtime.nativeHandle());
                try {
                    return nativeDial(handle, runtime.nativeHandle(), request.host,
                            request.port, request.directory, request.credential);
                } finally {
                    java.util.Arrays.fill(request.credential, (byte) 0);
                }
            }
            public boolean service() { return nativeServiceConnection(handle, runtime.nativeHandle()); }
            public void reset() { nativeDisconnect(handle, runtime.nativeHandle()); }
        });
        worker = new Thread(this::acceptLoop, "capyio-control-accept");
        worker.setDaemon(true);
        worker.start();
    }

    int port() {
        long current = handle;
        if (current == 0 || closing || !worker.isAlive()) {
            throw new IllegalStateException("control listener worker is unavailable");
        }
        return boundPort;
    }

    boolean connected() {
        return worker.isAlive() && !closing && cycle.connected();
    }

    String failure() {
        return failure != null ? failure : cycle.failure();
    }

    boolean discoveryAnnouncing() {
        return worker.isAlive() && !closing && discoveryAnnouncing;
    }

    synchronized boolean requestDial(
            String host, int port, byte[] directory, byte[] credential) {
        if (closing || !worker.isAlive() || cycle.connected() || pendingDial != null
                || host == null || host.isEmpty() || host.length() > 45
                || port < 1 || port > 65535 || directory == null || credential == null) {
            return false;
        }
        pendingDial = new DialRequest(host, port, directory, credential);
        failure = null;
        cycle.clearFailure();
        return true;
    }

    private synchronized DialRequest takePendingDial() {
        DialRequest request = pendingDial;
        pendingDial = null;
        return request;
    }

    private void acceptLoop() {
        try {
            long nextAnnouncementAt = 0;
            String lastFailure = null;
            while (!closing) {
                long now = android.os.SystemClock.elapsedRealtime();
                if (now >= nextAnnouncementAt) {
                    discoveryAnnouncing = nativeAnnounce(handle);
                    nextAnnouncementAt = now + 5_000;
                }
                cycle.step();
                String currentFailure = cycle.failure();
                if (currentFailure != null && !currentFailure.equals(lastFailure)) {
                    android.util.Log.w("CapyIOControl", currentFailure);
                }
                lastFailure = currentFailure;
                Thread.sleep(250);
            }
        } catch (InterruptedException interrupted) {
            Thread.currentThread().interrupt();
        } catch (RuntimeException rejected) {
            failure = rejected.getMessage();
        }
    }

    @Override
    public void close() {
        long current;
        synchronized (this) {
            if (handle == 0) return;
            closing = true;
            worker.interrupt();
            if (pendingDial != null) {
                java.util.Arrays.fill(pendingDial.credential, (byte) 0);
                pendingDial = null;
            }
            current = handle;
            handle = 0;
        }
        try {
            worker.join(6_000);
        } catch (InterruptedException interrupted) {
            Thread.currentThread().interrupt();
        }
        nativeClose(current, runtime.nativeHandle());
    }

    private static native long nativeCreate(
            String localNodeId, byte[] pairedDirectoryJson, byte[] encodedCredential);
    private static native int nativePort(long handle);
    private static native boolean nativeAnnounce(long handle);
    private static native boolean nativeTryAccept(long listenerHandle, long runtimeHandle);
    private static native boolean nativeDial(long listenerHandle, long runtimeHandle,
            String host, int port, byte[] pairedDirectoryJson, byte[] encodedCredential);
    private static native boolean nativeServiceConnection(long listenerHandle, long runtimeHandle);
    private static native void nativeDisconnect(long listenerHandle, long runtimeHandle);
    private static native void nativeClose(long listenerHandle, long runtimeHandle);
}
