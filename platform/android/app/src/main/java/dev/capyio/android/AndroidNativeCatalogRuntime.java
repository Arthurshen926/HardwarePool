package dev.capyio.android;

/** Narrow owner for one native Rust catalog Runtime; updates preserve connections. */
public final class AndroidNativeCatalogRuntime implements AutoCloseable {
    static {
        System.loadLibrary("capyio_android_jni");
    }

    private long handle;

    public AndroidNativeCatalogRuntime(String catalogJson) {
        handle = nativeCreate(catalogJson);
        if (handle == 0) {
            throw new IllegalStateException("native catalog Runtime returned an invalid handle");
        }
    }

    public synchronized void replaceCatalog(String catalogJson) {
        requireOpen();
        if (!nativeReplace(handle, catalogJson)) {
            throw new IllegalStateException("native catalog Runtime rejected replacement");
        }
    }

    public synchronized int capabilityCount() {
        requireOpen();
        return nativeCapabilityCount(handle);
    }

    synchronized String remoteCatalogJson() {
        requireOpen();
        return new String(nativeRemoteCatalog(handle), java.nio.charset.StandardCharsets.UTF_8);
    }
    private static native byte[] nativeRemoteCatalog(long handle);

    synchronized long nativeHandle() {
        requireOpen();
        return handle;
    }

    public synchronized String touchpadRequestsJson() {
        requireOpen();
        return new String(nativeTouchpadRequests(handle), java.nio.charset.StandardCharsets.UTF_8);
    }

    synchronized String controllerReceiverJson() {
        requireOpen();
        return new String(nativeControllerReceiver(handle), java.nio.charset.StandardCharsets.UTF_8);
    }

    synchronized String controllerUse(String action, String peer, String mode, String route, boolean ready) {
        requireOpen();
        return new String(nativeControllerUse(handle, action, peer, mode, route, ready), java.nio.charset.StandardCharsets.UTF_8);
    }

    synchronized String audioUse(String action, String route, String peer, String direction, int port) {
        requireOpen();
        return new String(nativeAudioUse(handle, action, route, peer, direction, port), java.nio.charset.StandardCharsets.UTF_8);
    }

    synchronized String cameraUse(String action, String peer) {
        requireOpen();
        return new String(nativeCameraUse(handle,action,peer),java.nio.charset.StandardCharsets.UTF_8);
    }

    synchronized String displayUse(String action, String argument) {
        requireOpen(); return new String(nativeDisplayUse(handle,action,argument),java.nio.charset.StandardCharsets.UTF_8);
    }
    synchronized byte[] displayRecord(byte[] acknowledgement) {
        requireOpen(); return nativeDisplayRecord(handle,acknowledgement);
    }

    synchronized String controllerSettings(long revision, boolean motion, int strength, long requestId, String error) {
        requireOpen();
        return new String(nativeControllerSettings(handle, revision, motion, strength, requestId, error), java.nio.charset.StandardCharsets.UTF_8);
    }

    public synchronized boolean resolveTouchpadRequest(String routeId, boolean allow) {
        requireOpen();
        return nativeResolveTouchpadRequest(handle, routeId, allow);
    }

    @Override
    public synchronized void close() {
        if (handle != 0) {
            nativeClose(handle);
            handle = 0;
        }
    }

    private void requireOpen() {
        if (handle == 0) {
            throw new IllegalStateException("native catalog Runtime is closed");
        }
    }

    private static native long nativeCreate(String catalogJson);

    private static native boolean nativeReplace(long handle, String catalogJson);

    private static native int nativeCapabilityCount(long handle);
    private static native byte[] nativeTouchpadRequests(long handle);
    private static native byte[] nativeControllerReceiver(long handle);
    private static native byte[] nativeControllerUse(long handle, String action, String peer, String mode, String route, boolean ready);
    private static native byte[] nativeAudioUse(long handle, String action, String route, String peer, String direction, int port);
    private static native byte[] nativeCameraUse(long handle,String action,String peer);
    private static native byte[] nativeDisplayUse(long handle,String action,String argument);
    private static native byte[] nativeDisplayRecord(long handle,byte[] acknowledgement);
    private static native byte[] nativeControllerSettings(long handle, long revision, boolean motion, int strength, long requestId, String error);
    private static native boolean nativeResolveTouchpadRequest(long handle, String routeId, boolean allow);

    private static native void nativeClose(long handle);
}
