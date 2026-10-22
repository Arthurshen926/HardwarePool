package io.capyio.camera.lab;

import android.media.MediaCodec;
import android.media.MediaCodecInfo;
import android.media.MediaFormat;
import android.os.Handler;
import android.os.HandlerThread;
import android.view.Surface;
import io.capyio.camera.contract.AvcCodecConfig;
import io.capyio.camera.contract.AvcEncoderConfig;
import io.capyio.camera.contract.SecureAvcSender;
import io.capyio.camera.contract.BoundedAvcAccessUnitQueue;
import io.capyio.camera.contract.EncodedAvcAccessUnit;
import io.capyio.camera.contract.FiniteCameraEligibility;
import java.io.IOException;
import java.nio.ByteBuffer;
import java.util.Objects;
import java.util.Optional;
import java.util.concurrent.atomic.AtomicLong;
import java.util.concurrent.atomic.AtomicBoolean;
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
    private final SecureAvcSender secureSender;
    private final FiniteCameraPreflight.Plan finitePlan;
    private final AtomicBoolean failed = new AtomicBoolean();
    // MediaCodec callbacks are serialized on callbackThread; only verified format can publish CSD.
    private boolean actualFiniteFormatVerified;
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

    MediaCodecSurfaceEncoder(AvcEncoderConfig config) { this(config, null); }

    MediaCodecSurfaceEncoder(AvcEncoderConfig config, SecureAvcSender secureSender) {
        this(config, secureSender, null);
    }

    MediaCodecSurfaceEncoder(AvcEncoderConfig config, SecureAvcSender secureSender,
            FiniteCameraPreflight.Plan finitePlan) {
        this.secureSender = secureSender;
        this.finitePlan = finitePlan;
        if (finitePlan != null && (secureSender == null || !finitePlan.encoderConfig().equals(config))) {
            throw new IllegalArgumentException("finite encoder requires its pinned secure plan");
        }
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
            created = finitePlan == null
                    ? MediaCodec.createEncoderByType(MediaFormat.MIMETYPE_VIDEO_AVC)
                    : MediaCodec.createByCodecName(finitePlan.codecName());
            String createdCodecName = boundCodecName(created.getName());
            created.setCallback(codecCallback, new Handler(thread.getLooper()));
            created.configure(finitePlan == null ? createFormat(config) : createFiniteFormat(),
                    null, null, MediaCodec.CONFIGURE_FLAG_ENCODE);
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

    long encodedAccessUnits() { return nextSequence.get(); }

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
            if (failed.get()) return;
            try {
                if (finitePlan != null) {
                    requireFiniteOutputFormat(format);
                }
                byte[] csd0 = copyFormatBuffer(format, "csd-0");
                byte[] csd1 = format.containsKey("csd-1")
                        ? copyFormatBuffer(format, "csd-1")
                        : new byte[0];
                AvcCodecConfig config = new AvcCodecConfig(csd0, csd1);
                // Finite SecureAvcSender validates SPS/PPS before publishing them to its worker.
                if (secureSender != null) secureSender.setCodecConfig(config);
                else outputConfig.set(config);
                if (finitePlan != null) actualFiniteFormatVerified = true;
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
        if (failed.get()) return;
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

        boolean codecConfig = (info.flags & MediaCodec.BUFFER_FLAG_CODEC_CONFIG) != 0;
        int finiteAllowedFlags = MediaCodec.BUFFER_FLAG_CODEC_CONFIG
                | MediaCodec.BUFFER_FLAG_KEY_FRAME | MediaCodec.BUFFER_FLAG_END_OF_STREAM;
        if (finitePlan != null && (!actualFiniteFormatVerified
                || (info.flags & ~finiteAllowedFlags) != 0
                || (codecConfig && (endOfStream || info.size == 0
                        || info.size > AvcCodecConfig.MAX_PARAMETER_SET_BYTES)))) {
            throw new IllegalArgumentException("finite AVC output precedes verified format or has invalid CSD flags/bounds");
        }

        // Reserve before allocating either payload copy. Secure overflow terminates, never drops.
        if (secureSender != null && !secureSender.reserveOutput(info.size)) {
            recordError("Secure AVC capacity exhausted or generation stopped");
            return;
        }
        boolean submitted = false;
        try {
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

            if (finitePlan != null && codecConfig) {
                // A config-only buffer is not a media AU. Validate repeated CSD, never silently
                // ignore it or consume a sequence that would create a media gap. The reservation
                // remains bounded until finally cancels it; the sender validates before update.
                secureSender.setCodecConfig(new AvcCodecConfig(payload, new byte[0]));
                return;
            }
            long sequence = nextSequence.incrementAndGet();
            EncodedAvcAccessUnit unit = new EncodedAvcAccessUnit(
                    sequence,
                    info.presentationTimeUs,
                    codecConfig,
                    (info.flags & MediaCodec.BUFFER_FLAG_KEY_FRAME) != 0,
                    endOfStream,
                    payload);
            if (secureSender != null) {
                // Finite sender validates the full access unit before any worker can see it.
                secureSender.offerReserved(unit);
                submitted = true;
                return;
            }
            BoundedAvcAccessUnitQueue.OfferResult result = outputQueue.offer(unit);
            if (!result.accepted() || result.droppedOldest()) {
                droppedAccessUnits.incrementAndGet();
            }
        } finally {
            if (secureSender != null && !submitted) secureSender.cancelReservation();
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

    static MediaFormat createFiniteFormat() {
        MediaFormat format = createFormat(AvcEncoderConfig.baseline720p30());
        format.setInteger(MediaFormat.KEY_PROFILE, MediaCodecInfo.CodecProfileLevel.AVCProfileBaseline);
        format.setInteger(MediaFormat.KEY_LEVEL, MediaCodecInfo.CodecProfileLevel.AVCLevel31);
        format.setInteger(MediaFormat.KEY_ROTATION, 0);
        return format;
    }

    private static void requireFiniteOutputFormat(MediaFormat format) {
        FiniteCameraEligibility.requireOutput(new FiniteCameraEligibility.OutputDescriptor(
                format.containsKey(MediaFormat.KEY_MIME) ? format.getString(MediaFormat.KEY_MIME) : null,
                integer(format, MediaFormat.KEY_WIDTH), integer(format, MediaFormat.KEY_HEIGHT),
                integer(format, MediaFormat.KEY_FRAME_RATE), integer(format, MediaFormat.KEY_BIT_RATE),
                equalsInteger(format, MediaFormat.KEY_PROFILE, MediaCodecInfo.CodecProfileLevel.AVCProfileBaseline),
                equalsInteger(format, MediaFormat.KEY_LEVEL, MediaCodecInfo.CodecProfileLevel.AVCLevel31),
                equalsInteger(format, MediaFormat.KEY_COLOR_FORMAT, MediaCodecInfo.CodecCapabilities.COLOR_FormatSurface),
                equalsInteger(format, MediaFormat.KEY_MAX_B_FRAMES, 0),
                equalsInteger(format, MediaFormat.KEY_COLOR_STANDARD, MediaFormat.COLOR_STANDARD_BT709),
                equalsInteger(format, MediaFormat.KEY_COLOR_RANGE, MediaFormat.COLOR_RANGE_LIMITED),
                equalsInteger(format, MediaFormat.KEY_COLOR_TRANSFER, MediaFormat.COLOR_TRANSFER_SDR_VIDEO),
                absentOrEquals(format, MediaFormat.KEY_ROTATION, 0)
                        && absentOrEquals(format, "crop-left", 0) && absentOrEquals(format, "crop-top", 0)
                        && absentOrEquals(format, "crop-right", 1279) && absentOrEquals(format, "crop-bottom", 719)
                        && !format.containsKey(MediaFormat.KEY_HDR_STATIC_INFO)));
    }

    private static Integer integer(MediaFormat format, String key) {
        return format.containsKey(key) ? format.getInteger(key) : null;
    }

    private static boolean equalsInteger(MediaFormat format, String key, int expected) {
        return Integer.valueOf(expected).equals(integer(format, key));
    }

    private static boolean absentOrEquals(MediaFormat format, String key, int expected) {
        return !format.containsKey(key) || equalsInteger(format, key, expected);
    }

    private static String boundCodecName(String value) {
        String name = value == null ? "unavailable" : value.replaceAll("[\\p{Cntrl}]", "?");
        return name.length() <= MAX_ERROR_CHARS ? name : name.substring(0, MAX_ERROR_CHARS);
    }

    private void recordError(String message) {
        failed.set(true);
        if (secureSender != null) secureSender.close();
        String bounded = message == null ? "unknown AVC encoder error" : message;
        if (bounded.length() > MAX_ERROR_CHARS) {
            bounded = bounded.substring(0, MAX_ERROR_CHARS);
        }
        lastError.compareAndSet(null, bounded);
    }

    @Override
    public void close() {
        failed.set(true);
        if (secureSender != null) secureSender.close();
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
