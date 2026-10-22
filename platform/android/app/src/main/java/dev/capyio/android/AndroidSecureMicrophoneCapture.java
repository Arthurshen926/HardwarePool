package dev.capyio.android;

import android.Manifest;
import android.annotation.SuppressLint;
import android.content.Context;
import android.content.pm.PackageManager;
import android.media.AudioDeviceInfo;
import android.media.AudioFormat;
import android.media.AudioRecord;
import android.media.AudioRouting;
import android.media.MediaRecorder;

import dev.capyio.android.contract.ActualAudioFormat;
import dev.capyio.android.contract.SecureMonoCaptureSession;
import dev.capyio.android.contract.SecureMonoCaptureReservation;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.util.Objects;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicLong;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.function.BooleanSupplier;
import java.util.function.Consumer;

/**
 * Service-owned finite secure mono capture, independent of the legacy UDP
 * adapter. Construction does not request permission or open a microphone.
 */
final class AndroidSecureMicrophoneCapture implements AutoCloseable {
    // Application-requested platform buffer bound; this is not a claim about
    // unobservable driver/AudioFlinger memory or real-device performance.
    static final int MAX_RECORD_BUFFER_BYTES = 19_200;
    private static final SecureMonoCaptureReservation PROCESS_CAPTURE = new SecureMonoCaptureReservation();
    private final SecureMonoCaptureSession session;
    private final AtomicBoolean startAttempted = new AtomicBoolean();
    private volatile SecureMonoCaptureReservation.Lease reservation;

    AndroidSecureMicrophoneCapture(Context context, BooleanSupplier permitted,
            long generation, SecureMonoCaptureSession.NativeSender sender) {
        session = new SecureMonoCaptureSession(generation,
                new AndroidPlatform(context, permitted, this::releaseReservation), sender);
    }

    static boolean hasUnreleasedCapture() { return PROCESS_CAPTURE.isHeld(); }

    boolean start() {
        if (!startAttempted.compareAndSet(false, true)) return false;
        reservation = PROCESS_CAPTURE.tryAcquire();
        if (reservation == null) {
            session.stop();
            return false;
        }
        boolean started = session.start();
        if (!started) {
            session.stop();
            // Stop may have completed before reservation assignment; the
            // one-shot lease makes this completion race harmless.
            SecureMonoCaptureSession.Snapshot snapshot = session.snapshot();
            if (snapshot.cleanupComplete()) releaseReservation(!snapshot.cleanupFailed());
        }
        return started;
    }

    private void releaseReservation(boolean clean) {
        SecureMonoCaptureReservation.Lease owned = reservation;
        if (owned != null) owned.completeCleanup(clean);
    }

    void stop() { session.stop(); }
    @Override public void close() { stop(); }
    SecureMonoCaptureSession.Snapshot snapshot() { return session.snapshot(); }
    boolean awaitStopped(long timeoutMillis) throws InterruptedException {
        return session.awaitStopped(timeoutMillis);
    }

    private static final class AndroidPlatform implements SecureMonoCaptureSession.Platform {
        private final Context context;
        private final BooleanSupplier permitted;
        private final Consumer<Boolean> onCleanup;

        AndroidPlatform(Context context, BooleanSupplier permitted, Consumer<Boolean> onCleanup) {
            this.context = Objects.requireNonNull(context, "context").getApplicationContext();
            this.permitted = Objects.requireNonNull(permitted, "permitted");
            this.onCleanup = Objects.requireNonNull(onCleanup, "onCleanup");
        }

        @Override public boolean permissionGranted() {
            // The caller's atomic gate represents the current visible Service
            // lifecycle and consent. Never call back into a Service monitor.
            return permitted.getAsBoolean()
                    && context.checkSelfPermission(Manifest.permission.RECORD_AUDIO)
                    == PackageManager.PERMISSION_GRANTED;
        }

        @SuppressLint("MissingPermission") // Checked immediately here and on each capture iteration.
        @Override public SecureMonoCaptureSession.Recorder openRecorder(
                SecureMonoCaptureSession.Cancellation cancellation) {
            if (cancellation.cancelled()) return null;
            if (!permissionGranted()) throw new SecurityException("microphone permission unavailable");
            // AudioRecord's PCM16 byte reads are native-endian. The secure
            // format is exactly S16LE, never an implicit conversion.
            if (ByteOrder.nativeOrder() != ByteOrder.LITTLE_ENDIAN) {
                throw new IllegalStateException("little-endian PCM16 required");
            }
            int minimum = AudioRecord.getMinBufferSize(SecureMonoCaptureSession.SAMPLE_RATE_HZ,
                    AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT);
            if (minimum <= 0 || minimum > MAX_RECORD_BUFFER_BYTES) {
                throw new IllegalStateException("bounded mono format unavailable");
            }
            int bytes = Math.max(4 * SecureMonoCaptureSession.RECORD_BYTES, minimum);
            bytes = ((bytes + SecureMonoCaptureSession.RECORD_BYTES - 1)
                    / SecureMonoCaptureSession.RECORD_BYTES) * SecureMonoCaptureSession.RECORD_BYTES;
            if (bytes > MAX_RECORD_BUFFER_BYTES || cancellation.cancelled()) return null;
            if (!permissionGranted()) throw new SecurityException("microphone permission revoked");

            AudioRecord record = new AudioRecord.Builder()
                    .setAudioSource(MediaRecorder.AudioSource.MIC)
                    .setAudioFormat(new AudioFormat.Builder()
                            .setSampleRate(SecureMonoCaptureSession.SAMPLE_RATE_HZ)
                            .setChannelMask(AudioFormat.CHANNEL_IN_MONO)
                            .setEncoding(AudioFormat.ENCODING_PCM_16BIT)
                            .build())
                    .setBufferSizeInBytes(bytes)
                    .build();
            boolean handedOff = false;
            try {
                if (record.getState() != AudioRecord.STATE_INITIALIZED) {
                    throw new IllegalStateException("microphone initialization failed");
                }
                if (cancellation.cancelled()) return null;
                AndroidRecorder owned = new AndroidRecorder(record);
                handedOff = true;
                return owned;
            } finally {
                // Still worker-owned, even when Stop arrives during build().
                if (!handedOff) record.release();
            }
        }

        @Override public long nanoTime() { return System.nanoTime(); }
        @Override public void awaitPoll(long nanos) throws InterruptedException {
            TimeUnit.NANOSECONDS.sleep(nanos);
        }
        @Override public void cleanupComplete(boolean clean) { onCleanup.accept(clean); }
    }

    private static final class AndroidRecorder implements SecureMonoCaptureSession.Recorder {
        private final AudioRecord record;
        private final AtomicLong routingGeneration = new AtomicLong();
        // AudioRecord PCM16 byte[] reads are deprecated. This direct scratch
        // adds exactly 960 bytes; it is reused, never queued or retained by JNI.
        private final ByteBuffer scratch = ByteBuffer.allocateDirect(
                SecureMonoCaptureSession.RECORD_BYTES);
        private final AudioRouting.OnRoutingChangedListener routingChanged =
                ignored -> routingGeneration.incrementAndGet();
        private boolean released;

        AndroidRecorder(AudioRecord record) {
            this.record = record;
            // Any notification after the first route is pinned is terminal,
            // even if a transient switch returns to the same device ID. A
            // delayed initial notification can conservatively fail this route;
            // it cannot silently authorize a different microphone.
            record.addOnRoutingChangedListener(routingChanged, null);
        }

        @Override public ActualAudioFormat actualFormat() {
            return new ActualAudioFormat(record.getSampleRate(), record.getChannelCount(),
                    record.getAudioFormat() == AudioFormat.ENCODING_PCM_16BIT
                            ? "pcm_s16le" : "unsupported",
                    record.getBufferSizeInFrames());
        }

        @Override public int audioSource() { return record.getAudioSource(); }

        @Override public void start() {
            record.startRecording();
            if (record.getRecordingState() != AudioRecord.RECORDSTATE_RECORDING) {
                throw new IllegalStateException("microphone start failed");
            }
        }

        @Override public int routedDeviceId() {
            AudioDeviceInfo routed = record.getRoutedDevice();
            return routed != null && routed.isSource() ? routed.getId() : 0;
        }

        @Override public long routingGeneration() { return routingGeneration.get(); }

        @Override public int read(byte[] target, int offset, int length) {
            if (length <= 0 || length > scratch.capacity() || offset < 0
                    || offset > target.length - length) {
                throw new IllegalArgumentException("bounded PCM read required");
            }
            scratch.clear();
            int count = record.read(scratch, length, AudioRecord.READ_NON_BLOCKING);
            if (count > 0 && count <= length && (count & 1) == 0) {
                // AudioRecord direct reads ignore Buffer.position(); copy only
                // the reported initialized bytes, without changing endianness.
                scratch.position(0);
                scratch.get(target, offset, count);
            }
            return count;
        }

        @Override public void stop() { record.stop(); }

        @Override public void release() {
            if (released) return;
            released = true;
            try { record.removeOnRoutingChangedListener(routingChanged); }
            finally {
                try { record.release(); }
                finally {
                    scratch.clear();
                    while (scratch.hasRemaining()) scratch.put((byte) 0);
                }
            }
        }
    }
}
