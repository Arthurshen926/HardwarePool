package dev.capyio.display.app;

import android.view.Surface;
import java.util.Objects;

/** Narrow JNI owner for the display-only Moonlight Common C connection. */
final class GameStreamNativeClient {
    interface StatusListener {
        void onStatus(String status);
    }

    static {
        System.loadLibrary("capyio-gamestream-client");
    }

    /** Triggers library resolution before acknowledging any remote launch. */
    static void requireAvailable() {
        // Class initialization performs the load; LinkageError fails closed.
    }

    private final GameStreamMediaCodecDecoder decoder;
    private final StatusListener statusListener;

    GameStreamNativeClient(Surface surface, StatusListener statusListener) {
        decoder = new GameStreamMediaCodecDecoder(Objects.requireNonNull(surface, "surface"));
        this.statusListener = Objects.requireNonNull(statusListener, "statusListener");
    }

    int run(GameStreamLaunchRecord.Expected expected, byte[] key, int keyId) {
        byte[] host = expected.hostIpv4();
        String hostText = (host[0] & 0xff) + "." + (host[1] & 0xff) + "."
                + (host[2] & 0xff) + "." + (host[3] & 0xff);
        return nativeRun(hostText, expected.portBase(), expected.width(), expected.height(),
                expected.refreshMillihertz(), expected.codec(), expected.dynamicRange(),
                expected.bitrateKbps(), key, keyId);
    }

    void interrupt() {
        nativeInterrupt();
    }

    void close() {
        decoder.close();
    }

    long presentedFrames() {
        return decoder.presentedFrames();
    }

    @SuppressWarnings("unused") // Called from JNI.
    private void onNativeStage(String stage) {
        statusListener.onStatus(stage);
    }

    @SuppressWarnings("unused") // Called from JNI.
    private void onNativeConnected() {
        statusListener.onStatus("GameStream video connected");
    }

    @SuppressWarnings("unused") // Called from JNI.
    private void onNativeTerminated(int errorCode) {
        statusListener.onStatus("GameStream terminated (" + errorCode + ")");
    }

    @SuppressWarnings("unused") // Called from JNI.
    private int onNativeVideoSetup(int format, int width, int height, int refreshRate, int flags) {
        return decoder.setup(format, width, height, refreshRate, flags);
    }

    @SuppressWarnings("unused") // Called from JNI.
    private void onNativeVideoStart() {
        decoder.start();
    }

    @SuppressWarnings("unused") // Called from JNI.
    private void onNativeVideoStop() {
        decoder.stop();
    }

    @SuppressWarnings("unused") // Called from JNI.
    private void onNativeVideoCleanup() {
        decoder.close();
    }

    @SuppressWarnings("unused") // Called from JNI.
    private int onNativeVideoUnit(byte[] bytes, int length, int bufferType,
            int frameNumber, int frameType, long receiveTimeUs, long enqueueTimeUs,
            long presentationTimeUs) {
        return decoder.submit(bytes, length, bufferType, frameNumber, frameType,
                receiveTimeUs, enqueueTimeUs, presentationTimeUs);
    }

    private native int nativeRun(String host, int portBase, int width, int height,
            int refreshMillihertz, int codec, int dynamicRange, int bitrateKbps,
            byte[] key, int keyId);

    private native void nativeInterrupt();
}
