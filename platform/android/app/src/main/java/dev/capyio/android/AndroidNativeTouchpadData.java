package dev.capyio.android;

/** Opaque Service-owned job, never a WebView endpoint or credential API. */
final class AndroidNativeTouchpadData {
    static { System.loadLibrary("capyio_android_jni"); }
    private AndroidNativeTouchpadData() {}
    static native long nativePending(long runtimeHandle);
    static native long nativeOpen(long job);
    static native boolean nativeSend(long job, byte[] record);
    static native boolean nativeWasStopped(long job);
    static native void nativeStop(long job);
    static native void nativeClose(long job);
}
