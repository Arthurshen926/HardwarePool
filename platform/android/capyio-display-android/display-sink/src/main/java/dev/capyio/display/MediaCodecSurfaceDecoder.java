package dev.capyio.display;

import android.media.MediaCodec;
import android.media.MediaCodecInfo;
import android.media.MediaCodecList;
import android.media.MediaFormat;
import android.view.Surface;

import java.io.IOException;
import java.nio.BufferOverflowException;
import java.nio.ByteBuffer;
import java.util.Objects;

/**
 * Synchronous worker-owned AVC decoder that renders directly to a caller-owned Surface.
 *
 * <p>This class has no callback, internal media queue, network operation or log path. Every
 * access unit is copied into a MediaCodec-owned input buffer before the call returns.</p>
 */
public final class MediaCodecSurfaceDecoder implements AutoCloseable {
    private static final String AVC_MIME = "video/avc";
    private static final long INPUT_DEQUEUE_TIMEOUT_MICROS = 10_000;
    private static final int MAX_OUTPUT_DEQUEUE_ATTEMPTS = 8;

    public enum QueueOutcome {
        BACKPRESSURE,
        DROPPED_AWAITING_KEY_FRAME,
        QUEUED,
        QUEUED_RESYNCHRONIZED,
        QUEUED_END_OF_STREAM
    }

    public static final class DecoderFailureException extends Exception {
        DecoderFailureException(String message) {
            super(message);
        }

        DecoderFailureException(String message, Throwable cause) {
            super(message, cause);
        }
    }

    private final DecoderInputGate gate = new DecoderInputGate();
    private MediaCodec codec;
    private AvcDecoderConfiguration configuration;
    private Thread ownerThread;
    private String selectedDecoderName;

    public void configure(AvcDecoderConfiguration next, Surface surface)
            throws DecoderFailureException {
        Objects.requireNonNull(next, "next");
        Objects.requireNonNull(surface, "surface");
        if (!surface.isValid()) {
            throw new IllegalArgumentException("output Surface is not valid");
        }
        gate.ensureCanConfigure(next);
        ownerThread = Thread.currentThread();

        MediaCodec created = null;
        try {
            MediaFormat format = MediaFormat.createVideoFormat(
                    AVC_MIME, next.width(), next.height());
            format.setFloat(MediaFormat.KEY_FRAME_RATE, next.frameRate());
            format.setInteger(MediaFormat.KEY_MAX_INPUT_SIZE, next.maximumAccessUnitBytes());
            format.setInteger(MediaFormat.KEY_PROFILE, mediaCodecProfile(next.profile()));
            format.setInteger(MediaFormat.KEY_LEVEL, mediaCodecLevel(next.level()));
            format.setInteger(MediaFormat.KEY_PUSH_BLANK_BUFFERS_ON_STOP, 1);
            // Android 16's media JNI requires codec-specific ByteBuffers to expose a
            // backing byte array. The configuration accessors already return copies.
            format.setByteBuffer("csd-0", ByteBuffer.wrap(next.sequenceParameterSetCopy()));
            format.setByteBuffer("csd-1", ByteBuffer.wrap(next.pictureParameterSetCopy()));

            String decoderName = new MediaCodecList(MediaCodecList.REGULAR_CODECS)
                    .findDecoderForFormat(format);
            if (decoderName == null) {
                throw new DecoderFailureException("no regular AVC decoder accepts the selected format");
            }
            created = MediaCodec.createByCodecName(decoderName);
            created.configure(format, surface, null, 0);
            created.start();

            codec = created;
            configuration = next;
            selectedDecoderName = decoderName;
            gate.configured(next);
        } catch (IOException | RuntimeException error) {
            releaseCreatedCodec(created);
            gate.failed();
            clearSessionReferences();
            throw new DecoderFailureException("configure AVC Surface decoder", error);
        } catch (DecoderFailureException error) {
            releaseCreatedCodec(created);
            gate.failed();
            clearSessionReferences();
            throw error;
        }
    }

    public QueueOutcome queue(EncodedAccessUnit unit) throws DecoderFailureException {
        requireOwnerThread();
        MediaCodec activeCodec = requireCodec();
        AvcDecoderConfiguration activeConfiguration = requireConfiguration();
        DecoderInputGate.Plan plan = gate.plan(unit);
        boolean flush = plan == DecoderInputGate.Plan.FLUSH_QUEUE
                || plan == DecoderInputGate.Plan.FLUSH_DROP_AWAITING_KEY_FRAME;
        boolean drop = plan == DecoderInputGate.Plan.DROP_AWAITING_KEY_FRAME
                || plan == DecoderInputGate.Plan.FLUSH_DROP_AWAITING_KEY_FRAME;

        try {
            if (flush) {
                activeCodec.flush();
                gate.flushed();
                queueCodecSpecificData(activeCodec, activeConfiguration.sequenceParameterSetCopy());
                queueCodecSpecificData(activeCodec, activeConfiguration.pictureParameterSetCopy());
            }
            if (drop) {
                gate.dropped(unit);
                return QueueOutcome.DROPPED_AWAITING_KEY_FRAME;
            }

            int inputIndex = activeCodec.dequeueInputBuffer(INPUT_DEQUEUE_TIMEOUT_MICROS);
            if (inputIndex < 0) {
                drainOutputInternal(activeCodec);
                return QueueOutcome.BACKPRESSURE;
            }
            ByteBuffer input = activeCodec.getInputBuffer(inputIndex);
            if (input == null) {
                throw new DecoderFailureException("MediaCodec returned a null input buffer");
            }
            input.clear();
            unit.copyPayloadTo(input);
            int flags = unit.endOfStream() ? MediaCodec.BUFFER_FLAG_END_OF_STREAM : 0;
            activeCodec.queueInputBuffer(
                    inputIndex,
                    0,
                    unit.payloadSize(),
                    unit.presentationTimeMicros(),
                    flags);
            boolean resynchronized = gate.queued(unit);
            drainOutputInternal(activeCodec);

            if (unit.endOfStream()) {
                return QueueOutcome.QUEUED_END_OF_STREAM;
            }
            return resynchronized
                    ? QueueOutcome.QUEUED_RESYNCHRONIZED
                    : QueueOutcome.QUEUED;
        } catch (DecoderFailureException error) {
            failAndRelease();
            throw error;
        } catch (BufferOverflowException error) {
            failAndRelease();
            throw new DecoderFailureException(
                    "selected access unit exceeds the decoder input buffer", error);
        } catch (RuntimeException error) {
            failAndRelease();
            throw new DecoderFailureException("queue AVC access unit", error);
        }
    }

    /** Releases at most eight ready output buffers to the Surface. */
    public int drainOutput() throws DecoderFailureException {
        requireOwnerThread();
        try {
            return drainOutputInternal(requireCodec());
        } catch (RuntimeException error) {
            failAndRelease();
            throw new DecoderFailureException("drain AVC decoder output", error);
        }
    }

    /** Replaces a transiently recreated application Surface without changing the stream epoch. */
    public void replaceOutputSurface(Surface surface) throws DecoderFailureException {
        Objects.requireNonNull(surface, "surface");
        requireOwnerThread();
        if (!surface.isValid()) {
            throw new IllegalArgumentException("replacement output Surface is not valid");
        }
        try {
            requireCodec().setOutputSurface(surface);
        } catch (RuntimeException error) {
            failAndRelease();
            throw new DecoderFailureException("replace AVC decoder output Surface", error);
        }
    }

    private static int drainOutputInternal(MediaCodec activeCodec) {
        MediaCodec.BufferInfo info = new MediaCodec.BufferInfo();
        int released = 0;
        for (int attempt = 0; attempt < MAX_OUTPUT_DEQUEUE_ATTEMPTS; attempt++) {
            int outputIndex = activeCodec.dequeueOutputBuffer(info, 0);
            if (outputIndex >= 0) {
                activeCodec.releaseOutputBuffer(outputIndex, info.size > 0);
                released++;
            } else if (outputIndex == MediaCodec.INFO_TRY_AGAIN_LATER) {
                break;
            } else if (outputIndex != MediaCodec.INFO_OUTPUT_FORMAT_CHANGED
                    && outputIndex != MediaCodec.INFO_OUTPUT_BUFFERS_CHANGED) {
                throw new IllegalStateException("unexpected MediaCodec output status " + outputIndex);
            }
        }
        return released;
    }

    private static void queueCodecSpecificData(MediaCodec activeCodec, byte[] bytes)
            throws DecoderFailureException {
        int inputIndex = activeCodec.dequeueInputBuffer(INPUT_DEQUEUE_TIMEOUT_MICROS);
        if (inputIndex < 0) {
            throw new DecoderFailureException(
                    "decoder did not provide an input buffer for codec-specific data");
        }
        ByteBuffer input = activeCodec.getInputBuffer(inputIndex);
        if (input == null || input.capacity() < bytes.length) {
            throw new DecoderFailureException("decoder codec-config input buffer is unavailable or short");
        }
        input.clear();
        input.put(bytes);
        activeCodec.queueInputBuffer(
                inputIndex, 0, bytes.length, 0, MediaCodec.BUFFER_FLAG_CODEC_CONFIG);
    }

    public void stop() throws DecoderFailureException {
        if (gate.state() == DecoderInputGate.State.STOPPED) {
            return;
        }
        if (gate.state() == DecoderInputGate.State.IDLE
                || gate.state() == DecoderInputGate.State.FAILED) {
            gate.stopped();
            clearSessionReferences();
            return;
        }
        requireOwnerThread();
        MediaCodec activeCodec = codec;
        try {
            if (activeCodec != null) {
                activeCodec.stop();
                activeCodec.release();
            }
            gate.stopped();
            clearSessionReferences();
        } catch (RuntimeException error) {
            releaseCreatedCodec(activeCodec);
            gate.failed();
            clearSessionReferences();
            throw new DecoderFailureException("stop AVC Surface decoder", error);
        }
    }

    @Override
    public void close() throws DecoderFailureException {
        stop();
    }

    public String state() {
        return gate.state().name().toLowerCase(java.util.Locale.ROOT);
    }

    public String selectedDecoderName() {
        return selectedDecoderName;
    }

    public long retainedPayloadBytes() {
        return 0;
    }

    private MediaCodec requireCodec() {
        if (codec == null) {
            throw new IllegalStateException("decoder is not configured");
        }
        return codec;
    }

    private AvcDecoderConfiguration requireConfiguration() {
        if (configuration == null) {
            throw new IllegalStateException("decoder configuration is unavailable");
        }
        return configuration;
    }

    private void requireOwnerThread() {
        if (ownerThread == null || ownerThread != Thread.currentThread()) {
            throw new IllegalStateException("MediaCodec decoder must remain on its owner worker thread");
        }
    }

    private void failAndRelease() {
        MediaCodec activeCodec = codec;
        if (activeCodec != null) {
            try {
                activeCodec.stop();
            } catch (RuntimeException ignored) {
                // Preserve the original decoder failure.
            }
            try {
                activeCodec.release();
            } catch (RuntimeException ignored) {
                // Preserve the original decoder failure.
            }
        }
        gate.failed();
        clearSessionReferences();
    }

    private static void releaseCreatedCodec(MediaCodec created) {
        if (created == null) {
            return;
        }
        try {
            created.release();
        } catch (RuntimeException ignored) {
            // Preserve the construction failure.
        }
    }

    private void clearSessionReferences() {
        codec = null;
        configuration = null;
        selectedDecoderName = null;
        ownerThread = null;
    }

    private static int mediaCodecProfile(AvcDecoderConfiguration.Profile profile) {
        return switch (profile) {
            case BASELINE -> MediaCodecInfo.CodecProfileLevel.AVCProfileBaseline;
            case MAIN -> MediaCodecInfo.CodecProfileLevel.AVCProfileMain;
            case HIGH -> MediaCodecInfo.CodecProfileLevel.AVCProfileHigh;
        };
    }

    private static int mediaCodecLevel(AvcDecoderConfiguration.Level level) {
        return switch (level) {
            case L3_1 -> MediaCodecInfo.CodecProfileLevel.AVCLevel31;
            case L4 -> MediaCodecInfo.CodecProfileLevel.AVCLevel4;
            case L4_1 -> MediaCodecInfo.CodecProfileLevel.AVCLevel41;
            case L4_2 -> MediaCodecInfo.CodecProfileLevel.AVCLevel42;
            case L5 -> MediaCodecInfo.CodecProfileLevel.AVCLevel5;
            case L5_1 -> MediaCodecInfo.CodecProfileLevel.AVCLevel51;
            case L5_2 -> MediaCodecInfo.CodecProfileLevel.AVCLevel52;
        };
    }
}
