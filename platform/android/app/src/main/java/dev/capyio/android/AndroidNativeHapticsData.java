package dev.capyio.android;

/** Private service bridge: no endpoint, credentials or motor commands from WebView. */
final class AndroidNativeHapticsData {
    private AndroidNativeHapticsData() {}
    static native long nativePending(long runtime);
    static native boolean nativeOpen(long job);
    static native byte[] nativeReceive(long job);
    static native boolean nativeAcknowledge(long job);
    static native void nativeClose(long job);
}
