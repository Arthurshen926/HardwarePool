package dev.capyio.android;

import android.media.AudioAttributes;
import android.media.AudioDeviceInfo;
import android.media.AudioFormat;
import android.media.AudioRouting;
import android.media.AudioTrack;

import dev.capyio.android.contract.ActualAudioFormat;
import dev.capyio.android.contract.SecureStereoPlaybackReservation;
import dev.capyio.android.contract.SecureStereoPlaybackSession;

import java.nio.ByteOrder;
import java.util.Objects;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicLong;
import java.util.function.BooleanSupplier;
import java.util.function.Consumer;

/** Separate finite secure speaker owner. Never opens AudioTrack before native Bound. */
final class AndroidSecureStereoPlayback implements AutoCloseable {
    private static final SecureStereoPlaybackReservation PROCESS_PLAYBACK = new SecureStereoPlaybackReservation();
    private final SecureStereoPlaybackSession session;
    private final AtomicBoolean startAttempted = new AtomicBoolean();
    private volatile SecureStereoPlaybackReservation.Lease reservation;

    AndroidSecureStereoPlayback(BooleanSupplier permitted, long generation,
            SecureStereoPlaybackSession.NativeReceiver receiver) {
        session = new SecureStereoPlaybackSession(generation,
                new AndroidPlatform(permitted, this::releaseReservation), receiver);
    }
    static boolean hasUnreleasedPlayback() { return PROCESS_PLAYBACK.isHeld(); }
    boolean start() {
        if (!startAttempted.compareAndSet(false, true)) return false;
        reservation = PROCESS_PLAYBACK.tryAcquire();
        if (reservation == null) { session.stop(); return false; }
        boolean started = session.start();
        if (!started) {
            session.stop();
            SecureStereoPlaybackSession.Snapshot snapshot = session.snapshot();
            if (snapshot.cleanupComplete()) releaseReservation(!snapshot.cleanupFailed());
        }
        return started;
    }
    private void releaseReservation(boolean clean) {
        SecureStereoPlaybackReservation.Lease owned = reservation;
        if (owned != null) owned.completeCleanup(clean);
    }
    void stop() { session.stop(); }
    @Override public void close() { stop(); }
    SecureStereoPlaybackSession.Snapshot snapshot() { return session.snapshot(); }
    boolean awaitStopped(long milliseconds) throws InterruptedException { return session.awaitStopped(milliseconds); }

    private static final class AndroidPlatform implements SecureStereoPlaybackSession.Platform {
        private final BooleanSupplier permitted;
        private final Consumer<Boolean> cleanup;
        AndroidPlatform(BooleanSupplier permitted, Consumer<Boolean> cleanup) {
            this.permitted = Objects.requireNonNull(permitted);
            this.cleanup = cleanup;
        }
        @Override public boolean permitted() { return permitted.getAsBoolean(); }
        @Override public long nanoTime() { return System.nanoTime(); }
        @Override public void awaitPoll(long nanos) throws InterruptedException { TimeUnit.NANOSECONDS.sleep(nanos); }
        @Override public void cleanupComplete(boolean clean) { cleanup.accept(clean); }
        @Override public SecureStereoPlaybackSession.Track openTrack(SecureStereoPlaybackSession.Cancellation cancellation) {
            if (cancellation.cancelled()) return null;
            if (!permitted() || ByteOrder.nativeOrder() != ByteOrder.LITTLE_ENDIAN) {
                throw new IllegalStateException("visible consent and little-endian PCM16 required");
            }
            int minimum = AudioTrack.getMinBufferSize(SecureStereoPlaybackSession.SAMPLE_RATE_HZ,
                    AudioFormat.CHANNEL_OUT_STEREO, AudioFormat.ENCODING_PCM_16BIT);
            if (minimum <= 0 || minimum > SecureStereoPlaybackSession.MAX_BUFFER_BYTES) {
                throw new IllegalStateException("bounded stereo playback unavailable");
            }
            int bytes = Math.max(minimum, SecureStereoPlaybackSession.RECORD_BYTES * 4);
            bytes = ((bytes + 3) / 4) * 4;
            if (bytes > SecureStereoPlaybackSession.MAX_BUFFER_BYTES || cancellation.cancelled()) return null;
            if (!permitted()) throw new IllegalStateException("visible playback consent revoked");
            AudioTrack track = new AudioTrack.Builder()
                    .setAudioAttributes(new AudioAttributes.Builder()
                            .setUsage(AudioAttributes.USAGE_MEDIA)
                            .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC).build())
                    .setAudioFormat(new AudioFormat.Builder()
                            .setSampleRate(SecureStereoPlaybackSession.SAMPLE_RATE_HZ)
                            .setChannelMask(AudioFormat.CHANNEL_OUT_STEREO)
                            .setEncoding(AudioFormat.ENCODING_PCM_16BIT).build())
                    .setTransferMode(AudioTrack.MODE_STREAM)
                    .setBufferSizeInBytes(bytes).build();
            boolean transferred = false;
            try {
                if (cancellation.cancelled()) return null;
                AndroidTrack owned = new AndroidTrack(track);
                transferred = true;
                return owned;
            } finally {
                if (!transferred) {
                    try { track.release(); }
                    catch (RuntimeException | LinkageError failure) {
                        throw new SecureStereoPlaybackSession.CleanupFailure(failure);
                    }
                }
            }
        }
    }
    private static final class AndroidTrack implements SecureStereoPlaybackSession.Track {
        private final AudioTrack track;
        private final AtomicLong routingRevision = new AtomicLong();
        private final AudioRouting.OnRoutingChangedListener listener = ignored -> routingRevision.incrementAndGet();
        AndroidTrack(AudioTrack track) {
            this.track = track;
            track.addOnRoutingChangedListener(listener, null);
        }
        @Override public ActualAudioFormat actualFormat() {
            return new ActualAudioFormat(track.getSampleRate(), track.getChannelCount(),
                    track.getAudioFormat() == AudioFormat.ENCODING_PCM_16BIT ? "pcm_s16le" : "unsupported",
                    track.getBufferSizeInFrames());
        }
        @Override public boolean initialized() {
            return track.getState() == AudioTrack.STATE_INITIALIZED
                    && track.getFormat().getChannelMask() == AudioFormat.CHANNEL_OUT_STEREO;
        }
        @Override public int capacityFrames() { return track.getBufferCapacityInFrames(); }
        @Override public void play() { track.play(); }
        @Override public boolean playing() { return track.getPlayState() == AudioTrack.PLAYSTATE_PLAYING; }
        @Override public int routedDeviceId() {
            AudioDeviceInfo device = track.getRoutedDevice();
            return device != null && device.isSink() ? device.getId() : 0;
        }
        @Override public long routingGeneration() { return routingRevision.get(); }
        @Override public int write(byte[] pcm, int offset, int length) {
            if (pcm.length != SecureStereoPlaybackSession.RECORD_BYTES || offset != 0
                    || length <= 0 || length > pcm.length || (length & 3) != 0) {
                throw new IllegalArgumentException("bounded aligned remaining slice required");
            }
            return track.write(pcm, 0, length, AudioTrack.WRITE_NON_BLOCKING);
        }
        @Override public void pause() { if (initialized()) track.pause(); }
        @Override public void flush() { if (initialized()) track.flush(); }
        @Override public void release() {
            try { track.removeOnRoutingChangedListener(listener); }
            finally { track.release(); }
        }
    }
}
