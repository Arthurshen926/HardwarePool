package io.capyio.camera.lab;

import android.media.MediaCodec;
import android.media.MediaCodecInfo;
import android.media.MediaFormat;
import android.os.Handler;
import android.os.HandlerThread;
import android.view.Surface;
import io.capyio.camera.contract.AvcCodecConfig;
import io.capyio.camera.contract.AvcEncoderConfig;
import io.capyio.camera.contract.BoundedAvcAccessUnitQueue;
import io.capyio.camera.contract.EncodedAvcAccessUnit;
import java.io.IOException;
import java.nio.ByteBuffer;
import java.util.Objects;
import java.util.Optional;
import java.util.concurrent.atomic.AtomicLong;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicReference;

/**
 * One-shot surface-input AVC encoder boundary.
 *
 * <p>This component owns MediaCodec and its input Surface. It has no network,
 * file or UI operations. The callback copies at most one bounded output buffer
 * into a fixed-capacity non-waiting queue and releases the codec buffer before
 * returning.</p>
 */
final class MediaCodecSurfaceEncoder implements AutoCloseable {
    record RuntimeInfo(String codecName, int requestedLatencyFrames, int actualLatencyFrames) {}

    enum State {
        NEW,
        STARTED,
        CLOSED
    }

    private static final int MAX_ERROR_CHARS = 256;
    private static final int REQUESTED_LATENCY_FRAMES = 1;

    private final Object lifecycleLock = new Object();
    private final AvcEncoderConfig config;
    private final BoundedAvcAccessUnitQueue outputQueue;
    private final AtomicReference<AvcCodecConfig> outputConfig = new AtomicReference<>();
    private final AtomicReference<String> lastError = new AtomicReference<>();
    private final AtomicLong nextSequence = new AtomicLong();
    private final AtomicLong droppedAccessUnits = new AtomicLong();
    private final AtomicInteger actualLatencyFrames = new AtomicInteger(-1);

    private State state = State.NEW;
    private HandlerThread callbackThread;
    private MediaCodec codec;
    private Surface inputSurface;
    private String codecName = "unavailable";

    MediaCodecSurfaceEncoder(AvcEncoderConfig config) {
        this.config = Objects.requireNonNull(config, "config");
        outputQueue = new BoundedAvcAccessUnitQueue(config.queueCapacity());
    }

    void start() throws IOException {
        synchronized (lifecycleLock) {
            if (state != State.NEW) {
                throw new IllegalStateException("AVC encoder is one-shot");
            }
        }

        HandlerThread thread = new HandlerThread("CapyIO-MediaCodec-AVC");
        thread.start();
        MediaCodec created = null;
        Surface createdSurface = null;
        try {
            created = MediaCodec.createEncoderByType(MediaFormat.MIMETYPE_VIDEO_AVC);
            String createdCodecName = boundCodecName(created.getName());
            created.setCallback(codecCallback, new Handler(thread.getLooper()));
            created.configure(createFormat(config), null, null, MediaCodec.CONFIGURE_FLAG_ENCODE);
            createdSurface = created.createInputSurface();

            synchronized (lifecycleLock) {
                callbackThread = thread;
                codec = created;
                inputSurface = createdSurface;
                codecName = createdCodecName;
                state = State.STARTED;
            }
            created.start();
        } catch (IOException | RuntimeException error) {
            if (createdSurface != null) {
                createdSurface.release();
            }
            if (created != null) {
                created.release();
            }
            thread.quitSafely();
            synchronized (lifecycleLock) {
                state = State.CLOSED;
            }
            throw error;
        }
    }

    State state() {
        synchronized (lifecycleLock) {
            return state;
        }
    }

    Surface inputSurface() {
        synchronized (lifecycleLock) {
            if (state != State.STARTED || inputSurface == null) {
                throw new IllegalStateException("AVC encoder is not started");
            }
            return inputSurface;
        }
    }

    Optional<EncodedAvcAccessUnit> pollAccessUnit() {
        return outputQueue.poll();
    }

    Optional<AvcCodecConfig> outputConfig() {
        return Optional.ofNullable(outputConfig.get());
    }

    Optional<AvcCodecConfig> takeOutputConfig() {
        return Optional.ofNullable(outputConfig.getAndSet(null));
    }

    Optional<String> lastError() {
        return Optional.ofNullable(lastError.get());
    }

    Optional<String> takeLastError() {
        return Optional.ofNullable(lastError.getAndSet(null));
    }

    long droppedAccessUnits() {
        return droppedAccessUnits.get();
    }

    RuntimeInfo runtimeInfo() {
        synchronized (lifecycleLock) {
            return new RuntimeInfo(codecName, REQUESTED_LATENCY_FRAMES, actualLatencyFrames.get());
        }
    }

    void signalEndOfInputStream() {
        MediaCodec current;
        synchronized (lifecycleLock) {
            if (state != State.STARTED || codec == null) {
                throw new IllegalStateException("AVC encoder is not started");
            }
            current = codec;
        }
        current.signalEndOfInputStream();
    }

    private final MediaCodec.Callback codecCallback = new MediaCodec.Callback() {
        @Override
        public void onInputBufferAvailable(MediaCodec codec, int index) {
            recordError("Surface-input AVC encoder unexpectedly requested an input buffer");
        }

        @Override
        public void onOutputBufferAvailable(
                MediaCodec codec,
                int index,
                MediaCodec.BufferInfo info) {
            try {
                copyOutput(codec, index, info);
            } catch (RuntimeException error) {
                recordError("Invalid AVC output: " + error.getClass().getSimpleName());
            } finally {
                try {
                    codec.releaseOutputBuffer(index, false);
                } catch (RuntimeException error) {
                    recordError("Unable to release AVC output: " + error.getClass().getSimpleName());
                }
            }
        }

        @Override
        public void onError(MediaCodec codec, MediaCodec.CodecException error) {
            recordError("MediaCodec AVC error: " + error.getDiagnosticInfo());
        }

        @Override
        public void onOutputFormatChanged(MediaCodec codec, MediaFormat format) {
            try {
                byte[] csd0 = copyFormatBuffer(format, "csd-0");
                byte[] csd1 = format.containsKey("csd-1")
                        ? copyFormatBuffer(format, "csd-1")
                        : new byte[0];
                outputConfig.set(new AvcCodecConfig(csd0, csd1));
                int actual = format.containsKey(MediaFormat.KEY_LATENCY)
                        ? format.getInteger(MediaFormat.KEY_LATENCY)
                        : -1;
                actualLatencyFrames.set(actual >= 0 && actual <= AvcEncoderConfig.MAX_FRAMES_PER_SECOND
                        ? actual
                        : -1);
            } catch (RuntimeException error) {
                recordError("Invalid AVC output format: " + error.getClass().getSimpleName());
            }
        }
    };

    private void copyOutput(MediaCodec codec, int index, MediaCodec.BufferInfo info) {
        if ((info.flags & MediaCodec.BUFFER_FLAG_PARTIAL_FRAME) != 0) {
            throw new IllegalArgumentException("partial AVC output is unsupported");
        }
        boolean endOfStream = (info.flags & MediaCodec.BUFFER_FLAG_END_OF_STREAM) != 0;
        if (info.size == 0 && !endOfStream) {
            return;
        }
        if (info.offset < 0
                || info.size < 0
                || info.size > EncodedAvcAccessUnit.MAX_PAYLOAD_BYTES) {
            throw new IllegalArgumentException("AVC output bounds are invalid");
        }

        byte[] payload = new byte[info.size];
        if (info.size > 0) {
            ByteBuffer output = codec.getOutputBuffer(index);
            if (output == null || info.offset > output.capacity() - info.size) {
                throw new IllegalArgumentException("AVC output buffer is unavailable or short");
            }
            ByteBuffer view = output.duplicate();
            view.clear();
            view.position(info.offset);
            view.limit(info.offset + info.size);
            view.get(payload);
        }

        long sequence = nextSequence.incrementAndGet();
        EncodedAvcAccessUnit unit = new EncodedAvcAccessUnit(
                sequence,
                info.presentationTimeUs,
                (info.flags & MediaCodec.BUFFER_FLAG_CODEC_CONFIG) != 0,
                (info.flags & MediaCodec.BUFFER_FLAG_KEY_FRAME) != 0,
                endOfStream,
                payload);
        BoundedAvcAccessUnitQueue.OfferResult result = outputQueue.offer(unit);
        if (!result.accepted() || result.droppedOldest()) {
            droppedAccessUnits.incrementAndGet();
        }
    }

    private static byte[] copyFormatBuffer(MediaFormat format, String key) {
        ByteBuffer source = format.getByteBuffer(key);
        if (source == null || source.remaining() == 0) {
            throw new IllegalArgumentException("missing " + key);
        }
        if (source.remaining() > AvcCodecConfig.MAX_PARAMETER_SET_BYTES) {
            throw new IllegalArgumentException("oversized " + key);
        }
        ByteBuffer view = source.duplicate();
        byte[] copy = new byte[view.remaining()];
        view.get(copy);
        return copy;
    }

    private static MediaFormat createFormat(AvcEncoderConfig config) {
        MediaFormat format = MediaFormat.createVideoFormat(
                MediaFormat.MIMETYPE_VIDEO_AVC, config.width(), config.height());
        format.setInteger(
                MediaFormat.KEY_COLOR_FORMAT,
                MediaCodecInfo.CodecCapabilities.COLOR_FormatSurface);
        format.setInteger(MediaFormat.KEY_BIT_RATE, config.bitrateBitsPerSecond());
        format.setInteger(MediaFormat.KEY_FRAME_RATE, config.framesPerSecond());
        format.setInteger(MediaFormat.KEY_I_FRAME_INTERVAL, config.keyFrameIntervalSeconds());
        format.setInteger(MediaFormat.KEY_MAX_B_FRAMES, 0);
        format.setInteger(MediaFormat.KEY_LATENCY, REQUESTED_LATENCY_FRAMES);
        format.setInteger(MediaFormat.KEY_PRIORITY, 0);
        format.setFloat(MediaFormat.KEY_MAX_FPS_TO_ENCODER, config.framesPerSecond());
        format.setInteger(MediaFormat.KEY_COLOR_STANDARD, MediaFormat.COLOR_STANDARD_BT709);
        format.setInteger(MediaFormat.KEY_COLOR_RANGE, MediaFormat.COLOR_RANGE_LIMITED);
        format.setInteger(MediaFormat.KEY_COLOR_TRANSFER, MediaFormat.COLOR_TRANSFER_SDR_VIDEO);
        return format;
    }

    private static String boundCodecName(String value) {
        String name = value == null ? "unavailable" : value.replaceAll("[\\p{Cntrl}]", "?");
        return name.length() <= MAX_ERROR_CHARS ? name : name.substring(0, MAX_ERROR_CHARS);
    }

    private void recordError(String message) {
        String bounded = message == null ? "unknown AVC encoder error" : message;
        if (bounded.length() > MAX_ERROR_CHARS) {
            bounded = bounded.substring(0, MAX_ERROR_CHARS);
        }
        lastError.compareAndSet(null, bounded);
    }

    @Override
    public void close() {
        MediaCodec currentCodec;
        Surface currentSurface;
        HandlerThread currentThread;
        synchronized (lifecycleLock) {
            if (state == State.CLOSED) {
                return;
            }
            state = State.CLOSED;
            currentCodec = codec;
            codec = null;
            currentSurface = inputSurface;
            inputSurface = null;
            currentThread = callbackThread;
            callbackThread = null;
        }

        if (currentCodec != null) {
            try {
                currentCodec.stop();
            } catch (RuntimeException error) {
                recordError("Unable to stop AVC encoder: " + error.getClass().getSimpleName());
            } finally {
                try {
                    currentCodec.release();
                } catch (RuntimeException error) {
                    recordError("Unable to release AVC encoder: " + error.getClass().getSimpleName());
                }
            }
        }
        if (currentSurface != null) {
            currentSurface.release();
        }
        if (currentThread != null) {
            currentThread.quitSafely();
        }
    }
}
