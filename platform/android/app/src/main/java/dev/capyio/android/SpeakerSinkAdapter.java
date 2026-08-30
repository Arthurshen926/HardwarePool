package dev.capyio.android;

import android.media.AudioAttributes;
import android.media.AudioFormat;
import android.media.AudioTrack;

import dev.capyio.android.contract.ActualAudioFormat;

/** Android speaker Sink. The native transport will own its bounded writer in 001D. */
final class SpeakerSinkAdapter implements AudioPlatformAdapter {
    static final String PROBLEM_BUSY = "CAPY.ANDROID.SPEAKER_BUSY";
    static final String PROBLEM_UNSUPPORTED = "CAPY.ANDROID.SPEAKER_FORMAT_UNSUPPORTED";
    static final String PROBLEM_START = "CAPY.ANDROID.SPEAKER_START_FAILED";

    private static final int REQUESTED_SAMPLE_RATE = 48_000;
    private static final int REQUESTED_CHANNEL_MASK = AudioFormat.CHANNEL_OUT_STEREO;
    private static final int REQUESTED_ENCODING = AudioFormat.ENCODING_PCM_16BIT;
    private static final int MAX_TRACK_BUFFER_BYTES = 1024 * 1024;

    private final Listener listener;
    private AudioTrack track;

    SpeakerSinkAdapter(Listener listener) {
        this.listener = listener;
    }

    @Override
    public synchronized void start(long generation) {
        if (track != null) {
            listener.onFailed(generation, PROBLEM_BUSY);
            return;
        }

        AudioTrack candidate = null;
        try {
            int minimumBytes = AudioTrack.getMinBufferSize(
                    REQUESTED_SAMPLE_RATE,
                    REQUESTED_CHANNEL_MASK,
                    REQUESTED_ENCODING);
            if (minimumBytes <= 0 || minimumBytes > MAX_TRACK_BUFFER_BYTES) {
                listener.onFailed(generation, PROBLEM_UNSUPPORTED);
                return;
            }
            candidate = new AudioTrack.Builder()
                    .setAudioAttributes(new AudioAttributes.Builder()
                            .setUsage(AudioAttributes.USAGE_MEDIA)
                            .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
                            .build())
                    .setAudioFormat(new AudioFormat.Builder()
                            .setSampleRate(REQUESTED_SAMPLE_RATE)
                            .setChannelMask(REQUESTED_CHANNEL_MASK)
                            .setEncoding(REQUESTED_ENCODING)
                            .build())
                    .setTransferMode(AudioTrack.MODE_STREAM)
                    .setBufferSizeInBytes(minimumBytes)
                    .setPerformanceMode(AudioTrack.PERFORMANCE_MODE_LOW_LATENCY)
                    .build();
            if (candidate.getState() != AudioTrack.STATE_INITIALIZED) {
                listener.onFailed(generation, PROBLEM_START);
                candidate.release();
                return;
            }

            AudioFormat platformFormat = candidate.getFormat();
            int bytesPerFrame = AndroidAudioFormats.bytesPerFrame(
                    platformFormat.getEncoding(),
                    platformFormat.getChannelCount());
            ActualAudioFormat actualFormat = new ActualAudioFormat(
                    platformFormat.getSampleRate(),
                    platformFormat.getChannelCount(),
                    AndroidAudioFormats.canonicalEncoding(platformFormat.getEncoding()),
                    Math.max(1, minimumBytes / bytesPerFrame));

            candidate.play();
            if (candidate.getPlayState() != AudioTrack.PLAYSTATE_PLAYING) {
                listener.onFailed(generation, PROBLEM_START);
                candidate.release();
                return;
            }
            track = candidate;
            listener.onStarted(generation, actualFormat);
        } catch (IllegalArgumentException unsupported) {
            if (candidate != null) {
                candidate.release();
            }
            listener.onFailed(generation, PROBLEM_UNSUPPORTED);
        } catch (IllegalStateException startFailure) {
            if (candidate != null) {
                candidate.release();
            }
            listener.onFailed(generation, PROBLEM_START);
        }
    }

    @Override
    public synchronized void stop(long generation) {
        AudioTrack current = track;
        track = null;
        if (current != null) {
            try {
                if (current.getPlayState() == AudioTrack.PLAYSTATE_PLAYING) {
                    current.pause();
                }
                current.flush();
                current.stop();
            } catch (IllegalStateException ignored) {
                // Release is still safe and mandatory after a route/device change.
            } finally {
                current.release();
            }
        }
        listener.onStopped(generation);
    }

    synchronized int underrunCount() {
        return track == null ? 0 : track.getUnderrunCount();
    }
}
