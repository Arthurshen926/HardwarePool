package dev.capyio.display.app;

import android.media.MediaCodec;
import android.media.MediaCodecInfo;
import android.media.MediaCodecList;
import android.media.MediaFormat;
import android.os.Build;
import android.view.Surface;
import java.io.IOException;
import java.nio.ByteBuffer;
import java.util.Objects;

/** Bounded input submission with independent Surface-output draining. */
final class GameStreamMediaCodecDecoder implements AutoCloseable {
    static final int DR_OK = 0;
    static final int DR_NEED_IDR = -1;
    private static final int BUFFER_TYPE_PICDATA = 0;
    private static final long INPUT_TIMEOUT_MICROS = 2_000;

    private final Surface surface;
    private final MediaCodec.BufferInfo outputInfo = new MediaCodec.BufferInfo();
    private MediaCodec codec;
    private boolean started;
    private long presentedFrames;
    private Thread outputThread;

    GameStreamMediaCodecDecoder(Surface surface) {
        this.surface = Objects.requireNonNull(surface, "surface");
    }

    synchronized int setup(int videoFormat, int width, int height, int refreshRate, int flags) {
        if (codec != null || width <= 0 || height <= 0 || refreshRate <= 0 || flags != 0) {
            return DR_NEED_IDR;
        }
        String mime = mimeFor(videoFormat);
        if (mime == null) {
            return DR_NEED_IDR;
        }
        MediaFormat format = MediaFormat.createVideoFormat(mime, width, height);
        format.setInteger(MediaFormat.KEY_FRAME_RATE, normalizedRefreshRate(refreshRate));
        format.setInteger(MediaFormat.KEY_PRIORITY, 0);
        if (Build.VERSION.SDK_INT >= 30) {
            format.setInteger(MediaFormat.KEY_LOW_LATENCY, 1);
        }
        MediaCodec created = null;
        try {
            String name = new MediaCodecList(MediaCodecList.REGULAR_CODECS)
                    .findDecoderForFormat(format);
            if (name == null) {
                return DR_NEED_IDR;
            }
            created = MediaCodec.createByCodecName(name);
            created.configure(format, surface, null, 0);
            codec = created;
            return DR_OK;
        } catch (IOException | RuntimeException error) {
            if (created != null) {
                created.release();
            }
            codec = null;
            return DR_NEED_IDR;
        }
    }

    synchronized void start() {
        if (codec == null || started) {
            return;
        }
        try {
            codec.start();
            started = true;
            outputThread = new Thread(this::drainLoop, "capyio-display-output");
            outputThread.setDaemon(true);
            outputThread.start();
        } catch (RuntimeException error) {
            releaseCodec();
        }
    }

    synchronized int submit(byte[] bytes, int length, int bufferType,
            int frameNumber, int frameType, long receiveTimeUs, long enqueueTimeUs,
            long presentationTimeUs) {
        if (!started || codec == null || bytes == null || length <= 0 || length > bytes.length
                || frameNumber < 0 || frameType < 0 || receiveTimeUs < 0 || enqueueTimeUs < 0
                || presentationTimeUs < 0) {
            return DR_NEED_IDR;
        }
        try {
            int inputIndex = codec.dequeueInputBuffer(INPUT_TIMEOUT_MICROS);
            if (inputIndex < 0) {
                return DR_NEED_IDR;
            }
            ByteBuffer input = codec.getInputBuffer(inputIndex);
            if (input == null || input.capacity() < length) {
                return DR_NEED_IDR;
            }
            input.clear();
            input.put(bytes, 0, length);
            int codecFlags = bufferType == BUFFER_TYPE_PICDATA
                    ? 0 : MediaCodec.BUFFER_FLAG_CODEC_CONFIG;
            codec.queueInputBuffer(inputIndex, 0, length, presentationTimeUs, codecFlags);
            return DR_OK;
        } catch (RuntimeException error) {
            releaseCodec();
            return DR_NEED_IDR;
        }
    }

    synchronized long presentedFrames() {
        return presentedFrames;
    }

    synchronized void stop() {
        stopOutputThread();
        if (!started || codec == null) {
            return;
        }
        try {
            codec.stop();
        } catch (RuntimeException ignored) {
            // Cleanup remains mandatory after a decoder failure.
        }
        started = false;
    }

    @Override
    public synchronized void close() {
        releaseCodec();
    }

    private int drainOutput(MediaCodec active) {
        int presented = 0;
        // A bounded batch releases the lifecycle lock even on a busy decoder.
        for (int batch = 0; batch < 16; batch++) {
            int outputIndex = active.dequeueOutputBuffer(outputInfo, 0);
            if (outputIndex >= 0) {
                boolean videoFrame = (outputInfo.flags & MediaCodec.BUFFER_FLAG_CODEC_CONFIG) == 0
                        && (outputInfo.flags & MediaCodec.BUFFER_FLAG_END_OF_STREAM) == 0;
                active.releaseOutputBuffer(outputIndex, videoFrame);
                if (videoFrame) {
                    presented++;
                }
            } else if (outputIndex == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED
                    || outputIndex == MediaCodec.INFO_OUTPUT_BUFFERS_CHANGED) {
                continue;
            } else {
                return presented;
            }
        }
        return presented;
    }

    private void drainLoop() {
        while (!Thread.currentThread().isInterrupted()) {
            synchronized (this) {
                if (!started || codec == null || Thread.currentThread() != outputThread) return;
                try {
                    presentedFrames = Math.addExact(presentedFrames, drainOutput(codec));
                } catch (RuntimeException failed) {
                    releaseCodec();
                    return;
                }
            }
            try { Thread.sleep(4); }
            catch (InterruptedException stopped) { Thread.currentThread().interrupt(); return; }
        }
    }

    private void stopOutputThread() {
        Thread old = outputThread;
        outputThread = null;
        if (old != null) old.interrupt();
        // No join while holding the decoder monitor. The old worker checks its
        // identity before touching the codec, including after a new Start.
    }

    private void releaseCodec() {
        stopOutputThread();
        MediaCodec active = codec;
        codec = null;
        if (active == null) {
            started = false;
            return;
        }
        if (started) {
            try {
                active.stop();
            } catch (RuntimeException ignored) {
                // Release still follows.
            }
        }
        started = false;
        active.release();
    }

    private static String mimeFor(int format) {
        if ((format & 0x000f) != 0) {
            return MediaFormat.MIMETYPE_VIDEO_AVC;
        }
        if ((format & 0x0f00) != 0) {
            return MediaFormat.MIMETYPE_VIDEO_HEVC;
        }
        if ((format & 0xf000) != 0) {
            return MediaFormat.MIMETYPE_VIDEO_AV1;
        }
        return null;
    }

    private static int normalizedRefreshRate(int refreshRate) {
        return refreshRate > 1_000 ? Math.max(1, Math.round(refreshRate / 1_000.0f)) : refreshRate;
    }
}
