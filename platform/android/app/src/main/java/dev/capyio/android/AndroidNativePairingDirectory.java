package dev.capyio.android;

import java.nio.charset.StandardCharsets;

/** Rust-owned validation and canonicalization for paired-directory schema 1. */
final class AndroidNativePairingDirectory {
    static {
        System.loadLibrary("capyio_android_jni");
    }

    private AndroidNativePairingDirectory() {}

    static byte[] canonicalize(String localNodeId, byte[] directoryJson) {
        if (localNodeId == null || directoryJson == null) {
            throw new IllegalArgumentException("paired directory arguments must not be null");
        }
        byte[] result = nativeCanonicalize(localNodeId, directoryJson);
        if (result == null) {
            throw new IllegalStateException("native paired-directory validation returned null");
        }
        return result;
    }

    static int peerCount(String localNodeId, byte[] directoryJson) {
        if (localNodeId == null || directoryJson == null) {
            throw new IllegalArgumentException("paired directory arguments must not be null");
        }
        int result = nativePeerCount(localNodeId, directoryJson);
        if (result < 0) {
            throw new IllegalStateException("native paired-directory count returned invalid value");
        }
        return result;
    }

    static String[] activeCredentialHandles(String localNodeId, byte[] directoryJson) {
        if (localNodeId == null || directoryJson == null) {
            throw new IllegalArgumentException("paired directory arguments must not be null");
        }
        byte[] result = nativeActiveCredentialHandles(localNodeId, directoryJson);
        if (result == null) {
            throw new IllegalStateException("native active-credential projection returned null");
        }
        if (result.length == 0) return new String[0];
        return new String(result, StandardCharsets.UTF_8).split("\\n", -1);
    }

    static byte[] invitationDirectory(String localNodeId, byte[] invitationBytes) {
        byte[] result = nativeInvitationDirectory(localNodeId, invitationBytes);
        if (result == null) throw new IllegalStateException("native invitation directory is null");
        return result;
    }

    static byte[] invitationCredential(String localNodeId, byte[] invitationBytes) {
        byte[] result = nativeInvitationCredential(localNodeId, invitationBytes);
        if (result == null) throw new IllegalStateException("native invitation credential is null");
        return result;
    }

    private static native byte[] nativeCanonicalize(String localNodeId, byte[] directoryJson);

    private static native int nativePeerCount(String localNodeId, byte[] directoryJson);

    private static native byte[] nativeActiveCredentialHandles(
            String localNodeId, byte[] directoryJson);

    private static native byte[] nativeInvitationDirectory(
            String localNodeId, byte[] invitationBytes);

    private static native byte[] nativeInvitationCredential(
            String localNodeId, byte[] invitationBytes);
}
