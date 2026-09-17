package dev.capyio.display.app;

import android.os.Handler;
import android.os.HandlerThread;
import android.view.Surface;

import dev.capyio.display.AvcDecoderConfiguration;
import dev.capyio.display.EncodedAccessUnit;
import dev.capyio.display.MediaCodecSurfaceDecoder;
import dev.capyio.display.PrivateDisplayRecordDecoder;

import java.util.Arrays;
import java.util.Objects;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;

/** Worker-owned bridge from authenticated complete records to an application Surface. */
public final class DisplaySurfaceController implements AutoCloseable {
    private static final int KIND_OFFSET = 5;
    private static final int KIND_CONFIGURATION = 1;
    private static final int KIND_ACCESS_UNIT = 2;
    private static final int MAX_COMPLETE_RECORD_BYTES =
            AvcDecoderConfiguration.MAX_ACCESS_UNIT_BYTES + 60;
    private static final long DELIVERY_TIMEOUT_MILLIS = 500;

    public interface Listener {
        void onStateChanged(String state);
    }

    private final HandlerThread worker = new HandlerThread("capyio-display-decoder");
    private final Handler handler;
    private final Handler callbackHandler;
    private final Listener listener;
    private final AtomicBoolean closed = new AtomicBoolean();

    private Surface surface;
    private AvcDecoderConfiguration configuration;
    private MediaCodecSurfaceDecoder decoder;

    public DisplaySurfaceController(Handler callbackHandler, Listener listener) {
        this.callbackHandler = Objects.requireNonNull(callbackHandler, "callbackHandler");
        this.listener = Objects.requireNonNull(listener, "listener");
        worker.start();
        handler = new Handler(worker.getLooper());
    }

    public void surfaceAvailable(Surface nextSurface) {
        Objects.requireNonNull(nextSurface, "nextSurface");
        requireOpen();
        handler.post(() -> {
            surface = nextSurface;
            if (decoder == null) {
                publish("surface-ready; waiting for authenticated stream configuration");
                return;
            }
            try {
                decoder.replaceOutputSurface(nextSurface);
                publish("decoder-surface-replaced");
            } catch (MediaCodecSurfaceDecoder.DecoderFailureException error) {
                stopDecoder();
                configuration = null;
                publish("display stream failed closed: replacement Surface");
            }
        });
    }

    public void surfaceDestroyed() {
        if (closed.get()) {
            return;
        }
        handler.post(() -> {
            surface = null;
            publish("surface-recreating; dropping frames until replacement");
        });
    }

    /** Drops decoder state when the authenticated transport is lost. */
    public void authenticatedTransportLost() {
        if (closed.get()) {
            return;
        }
        handler.post(() -> {
            stopDecoder();
            configuration = null;
            publish("authenticated transport lost; decoder reset");
        });
    }

    /**
     * Accepts one complete CPD1 record only after the caller's outer transport
     * authenticated its peer, authorized the Route and verified its binding.
     */
    public void submitAuthenticatedRecord(byte[] record) {
        Objects.requireNonNull(record, "record");
        requireOpen();
        if (record.length < 12 || record.length > MAX_COMPLETE_RECORD_BYTES) {
            throw new IllegalArgumentException("complete display record is outside its bound");
        }
        byte[] owned = Arrays.copyOf(record, record.length);
        CountDownLatch delivered = new CountDownLatch(1);
        AtomicReference<RuntimeException> failure = new AtomicReference<>();
        if (!handler.post(() -> {
            try {
                acceptRecord(owned);
            } catch (RuntimeException error) {
                failure.set(error);
            } finally {
                delivered.countDown();
            }
        })) {
            throw new IllegalStateException("decoder worker rejected the display record");
        }
        try {
            if (!delivered.await(DELIVERY_TIMEOUT_MILLIS, TimeUnit.MILLISECONDS)) {
                throw new IllegalStateException("decoder worker exceeded its delivery deadline");
            }
        } catch (InterruptedException error) {
            Thread.currentThread().interrupt();
            throw new IllegalStateException("display record delivery was interrupted", error);
        }
        if (failure.get() != null) {
            throw failure.get();
        }
    }

    private void acceptRecord(byte[] record) {
        try {
            int kind = Byte.toUnsignedInt(record[KIND_OFFSET]);
            if (kind == KIND_CONFIGURATION) {
                configure(PrivateDisplayRecordDecoder.decodeConfiguration(record));
            } else if (kind == KIND_ACCESS_UNIT) {
                queue(PrivateDisplayRecordDecoder.decodeAccessUnit(
                        record, requireConfiguration().maximumAccessUnitBytes()));
            } else {
                throw new IllegalArgumentException("private display record kind is unknown");
            }
        } catch (RuntimeException | MediaCodecSurfaceDecoder.DecoderFailureException error) {
            stopDecoder();
            configuration = null;
            publish("display stream failed closed: " + error.getClass().getSimpleName());
            throw new IllegalStateException("authenticated display record failed closed", error);
        }
    }

    private void configure(AvcDecoderConfiguration next)
            throws MediaCodecSurfaceDecoder.DecoderFailureException {
        if (surface == null || !surface.isValid()) {
            throw new IllegalStateException("decoder configuration arrived without a valid Surface");
        }
        stopDecoder();
        MediaCodecSurfaceDecoder created = new MediaCodecSurfaceDecoder();
        created.configure(next, surface);
        decoder = created;
        configuration = next;
        publish("decoder-ready: " + next.width() + "x" + next.height());
    }

    private void queue(EncodedAccessUnit unit)
            throws MediaCodecSurfaceDecoder.DecoderFailureException {
        if (surface == null || !surface.isValid()) {
            publish("decoder-dropped-surface-recreating");
            return;
        }
        MediaCodecSurfaceDecoder active = decoder;
        if (active == null) {
            throw new IllegalStateException("access unit arrived before decoder configuration");
        }
        MediaCodecSurfaceDecoder.QueueOutcome outcome = active.queue(unit);
        publish("decoder-" + outcome.name().toLowerCase(java.util.Locale.ROOT).replace('_', '-'));
    }

    private AvcDecoderConfiguration requireConfiguration() {
        if (configuration == null) {
            throw new IllegalStateException("access unit arrived before stream configuration");
        }
        return configuration;
    }

    private void stopDecoder() {
        MediaCodecSurfaceDecoder active = decoder;
        decoder = null;
        if (active != null) {
            try {
                active.stop();
            } catch (MediaCodecSurfaceDecoder.DecoderFailureException ignored) {
                // The caller receives the fail-closed state, never codec details.
            }
        }
    }

    private void publish(String state) {
        callbackHandler.post(() -> listener.onStateChanged(state));
    }

    private void requireOpen() {
        if (closed.get()) {
            throw new IllegalStateException("display Surface controller is closed");
        }
    }

    @Override
    public void close() {
        if (!closed.compareAndSet(false, true)) {
            return;
        }
        handler.post(() -> {
            stopDecoder();
            configuration = null;
            surface = null;
            worker.quitSafely();
        });
    }
}
