package dev.capyio.android;

import android.media.AudioFormat;
import android.media.AudioRecord;
import android.media.MediaRecorder;

import dev.capyio.android.contract.ActualAudioFormat;

import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicLong;

/** Android microphone Source. Captured bytes are deliberately discarded in 001C. */
final class MicrophoneSourceAdapter implements AudioPlatformAdapter {
    static final String PROBLEM_BUSY = "CAPY.ANDROID.MIC_BUSY";
    static final String PROBLEM_PERMISSION = "CAPY.ANDROID.MIC_PERMISSION_DENIED";
    static final String PROBLEM_UNSUPPORTED = "CAPY.ANDROID.MIC_FORMAT_UNSUPPORTED";
    static final String PROBLEM_START = "CAPY.ANDROID.MIC_START_FAILED";
    static final String PROBLEM_READ = "CAPY.ANDROID.MIC_READ_FAILED";

    private static final int REQUESTED_SAMPLE_RATE = 48_000;
    private static final int REQUESTED_CHANNEL_MASK = AudioFormat.CHANNEL_IN_MONO;
    private static final int REQUESTED_ENCODING = AudioFormat.ENCODING_PCM_16BIT;
    private static final int TEN_MILLISECONDS_FRAMES = REQUESTED_SAMPLE_RATE / 100;
    private static final int MAX_WORK_BUFFER_BYTES = 256 * 1024;
    private static final int MAX_RECORD_BUFFER_BYTES = 512 * 1024;

    private final Listener listener;
    private final AtomicBoolean running = new AtomicBoolean();
    private final AtomicLong completionGeneration = new AtomicLong();

    private volatile AudioRecord recorder;
    private volatile Thread worker;

    MicrophoneSourceAdapter(Listener listener) {
        this.listener = listener;
    }

    @Override
    public synchronized void start(long generation) {
        if (worker != null) {
            listener.onFailed(generation, PROBLEM_BUSY);
            return;
        }
        completionGeneration.set(generation);
        running.set(true);
        Thread nextWorker = new Thread(() -> capture(generation), "capyio-microphone-source");
        nextWorker.setDaemon(true);
        worker = nextWorker;
        nextWorker.start();
    }

    @Override
    public void stop(long generation) {
        completionGeneration.set(generation);
        running.set(false);
        AudioRecord current = recorder;
        if (current != null) {
            try {
                current.stop();
            } catch (IllegalStateException ignored) {
                // The capture thread still owns release and the stopped completion.
            }
        } else if (worker == null) {
            listener.onStopped(generation);
        }
    }

    private void capture(long startGeneration) {
        boolean failed = false;
        AudioRecord localRecorder = null;
        try {
            int minimumBytes = AudioRecord.getMinBufferSize(
                    REQUESTED_SAMPLE_RATE,
                    REQUESTED_CHANNEL_MASK,
                    REQUESTED_ENCODING);
            int requestedChunkBytes = TEN_MILLISECONDS_FRAMES * 2;
            int workBufferBytes = Math.max(minimumBytes, requestedChunkBytes);
            if (minimumBytes <= 0 || workBufferBytes > MAX_WORK_BUFFER_BYTES) {
                failed = true;
                listener.onFailed(startGeneration, PROBLEM_UNSUPPORTED);
                return;
            }
            int recordBufferBytes = Math.min(workBufferBytes * 2, MAX_RECORD_BUFFER_BYTES);
            localRecorder = new AudioRecord.Builder()
                    .setAudioSource(MediaRecorder.AudioSource.DEFAULT)
                    .setAudioFormat(new AudioFormat.Builder()
                            .setSampleRate(REQUESTED_SAMPLE_RATE)
                            .setChannelMask(REQUESTED_CHANNEL_MASK)
                            .setEncoding(REQUESTED_ENCODING)
                            .build())
                    .setBufferSizeInBytes(recordBufferBytes)
                    .build();
            recorder = localRecorder;
            if (localRecorder.getState() != AudioRecord.STATE_INITIALIZED) {
                failed = true;
                listener.onFailed(startGeneration, PROBLEM_START);
                return;
            }

            AudioFormat platformFormat = localRecorder.getFormat();
            int bytesPerFrame = AndroidAudioFormats.bytesPerFrame(
                    platformFormat.getEncoding(),
                    platformFormat.getChannelCount());
            byte[] workBuffer = new byte[workBufferBytes];
            ActualAudioFormat actualFormat = new ActualAudioFormat(
                    platformFormat.getSampleRate(),
                    platformFormat.getChannelCount(),
                    AndroidAudioFormats.canonicalEncoding(platformFormat.getEncoding()),
                    Math.max(1, workBuffer.length / bytesPerFrame));

            localRecorder.startRecording();
            if (localRecorder.getRecordingState() != AudioRecord.RECORDSTATE_RECORDING) {
                failed = true;
                listener.onFailed(startGeneration, PROBLEM_START);
                return;
            }
            listener.onStarted(startGeneration, actualFormat);

            while (running.get()) {
                int bytesRead = localRecorder.read(
                        workBuffer,
                        0,
                        workBuffer.length,
                        AudioRecord.READ_BLOCKING);
                if (bytesRead > 0) {
                    listener.onFrames(startGeneration, bytesRead / bytesPerFrame);
                } else if (bytesRead < 0 && running.get()) {
                    failed = true;
                    listener.onFailed(startGeneration, PROBLEM_READ);
                    break;
                }
            }
        } catch (SecurityException denied) {
            failed = true;
            listener.onFailed(startGeneration, PROBLEM_PERMISSION);
        } catch (IllegalArgumentException unsupported) {
            failed = true;
            listener.onFailed(startGeneration, PROBLEM_UNSUPPORTED);
        } catch (IllegalStateException startFailure) {
            failed = true;
            listener.onFailed(startGeneration, PROBLEM_START);
        } finally {
            running.set(false);
            if (localRecorder != null) {
                try {
                    if (localRecorder.getRecordingState() == AudioRecord.RECORDSTATE_RECORDING) {
                        localRecorder.stop();
                    }
                } catch (IllegalStateException ignored) {
                    // Release remains mandatory after an asynchronous device failure.
                }
                localRecorder.release();
            }
            synchronized (this) {
                recorder = null;
                worker = null;
            }
            if (!failed) {
                listener.onStopped(completionGeneration.get());
            }
        }
    }
}
