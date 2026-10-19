package dev.capyio.android;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;

/** The service, not the Activity or WebView, owns network vibration execution. */
final class HapticsDataWorker extends Thread {
    private final long job;
    private final long generation;
    private final AndroidHapticsSinkAdapter sink;
    private volatile boolean stopped;
    private volatile long executedFrames;
    long executedFrames() { return executedFrames; }

    HapticsDataWorker(long job, long generation, AndroidHapticsSinkAdapter sink) {
        super("capyio-haptics-data");
        this.job = job;
        this.generation = generation;
        this.sink = sink;
    }

    void stopData() {
        stopped = true;
        AndroidNativeHapticsData.nativeClose(job);
        sink.endRemote(generation);
    }

    @Override public void run() {
        try {
            if (stopped || !AndroidNativeHapticsData.nativeOpen(job)) return;
            while (!stopped) {
                byte[] bytes = AndroidNativeHapticsData.nativeReceive(job);
                if (bytes == null || stopped) break;
                // Rust has admitted the exact Route/epoch/sequence and 24-byte record.
                ByteBuffer frame = ByteBuffer.wrap(bytes).order(ByteOrder.BIG_ENDIAN);
                AndroidHapticsCommand command;
                if (bytes[5] == 0) command = AndroidHapticsCommand.stop(generation);
                else {
                    int duration = Short.toUnsignedInt(frame.getShort(6));
                    int low = Short.toUnsignedInt(frame.getShort(16));
                    int high = Short.toUnsignedInt(frame.getShort(18));
                    command = AndroidHapticsCommand.rumble(generation,
                            (low * 1000 + 65534) / 65535,
                            (high * 1000 + 65534) / 65535, duration);
                }
                if (!sink.submit(command)) break;
                executedFrames++;
                if (!AndroidNativeHapticsData.nativeAcknowledge(job)) break;
            }
        } catch (RuntimeException unavailable) {
            stopped = true;
        } finally {
            sink.endRemote(generation);
            AndroidNativeHapticsData.nativeClose(job);
        }
    }
}
