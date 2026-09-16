package dev.capyio.touchpad;

/** Primitive-only JNI entrypoints for the private, pre-authenticated touchpad lab stream. */
public final class NativeTouchpadRecordBridge {
    static {
        System.loadLibrary("capyio_android_jni");
    }

    private NativeTouchpadRecordBridge() {}

    public static native long nativeRecordCreate(
            String streamId,
            long streamEpoch,
            String clockDomainId,
            int widthPx,
            int heightPx,
            int widthHimetric,
            int heightHimetric,
            int maxContacts,
            boolean reportsPressure,
            long firstSequence,
            String routeId,
            String sessionId,
            String sourceNodeId,
            String sourceCapabilityId,
            String sourcePortId,
            String sinkNodeId,
            String sinkCapabilityId,
            String sinkPortId,
            long authorizationExpiresAtMs);

    public static native byte[] nativeRecordHello(long handle);

    public static native byte[] nativeRecordStart(long handle, long eventTimeNanos);

    public static native byte[] nativeRecordMotion(
            long handle,
            long eventTimeNanos,
            int action,
            int actionIndex,
            int[] pointerIds,
            int[] toolTypes,
            float[] xPx,
            float[] yPx,
            float[] pressure);

    public static native byte[] nativeRecordClose(long handle, long eventTimeNanos);

    public static native byte[] nativeRecordTakeClose(long handle);
}
