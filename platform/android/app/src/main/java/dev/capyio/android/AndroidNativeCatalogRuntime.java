package dev.capyio.android;

/** Narrow owner for one disconnected native Rust catalog Runtime. */
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

    public synchronized void replaceDisconnectedCatalog(String catalogJson) {
        requireOpen();
        if (!nativeReplace(handle, catalogJson)) {
            throw new IllegalStateException("native catalog Runtime rejected replacement");
        }
    }

    public synchronized int capabilityCount() {
        requireOpen();
        return nativeCapabilityCount(handle);
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

    private static native void nativeClose(long handle);
}
