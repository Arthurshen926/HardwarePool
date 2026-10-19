package dev.capyio.android;

/** Rust-owned validation for the secret control credential bundle. */
final class AndroidNativeControlCredential {
    static {
        System.loadLibrary("capyio_android_jni");
    }

    private AndroidNativeControlCredential() {}

    static void requireValid(byte[] encodedBundle) {
        if (encodedBundle == null) {
            throw new IllegalArgumentException("control credential bytes must not be null");
        }
        if (!nativeValidate(encodedBundle)) {
            throw new IllegalStateException("native control credential validation returned false");
        }
    }

    private static native boolean nativeValidate(byte[] encodedBundle);
}
