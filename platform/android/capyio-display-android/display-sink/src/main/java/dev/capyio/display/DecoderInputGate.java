package dev.capyio.display;

import java.util.Objects;
import java.util.UUID;

/** Codec-free identity, sequence and key-frame gate used by the Android adapter. */
final class DecoderInputGate {
    enum State {
        IDLE,
        AWAITING_KEY_FRAME,
        RENDERING,
        DRAINING,
        STOPPED,
        FAILED
    }

    enum Plan {
        DROP_AWAITING_KEY_FRAME,
        QUEUE,
        FLUSH_DROP_AWAITING_KEY_FRAME,
        FLUSH_QUEUE,
        QUEUE_END_OF_STREAM
    }

    private State state = State.IDLE;
    private UUID streamId;
    private long streamEpoch;
    private long lastSequence = -1;
    private long lastPresentationTimeMicros = -1;
    private boolean resynchronizing;

    State state() {
        return state;
    }

    void ensureCanConfigure(AvcDecoderConfiguration configuration) {
        Objects.requireNonNull(configuration, "configuration");
        if (state != State.IDLE && state != State.STOPPED) {
            throw new IllegalStateException("stop the decoder before reconfiguration");
        }
        if (streamId != null
                && streamId.equals(configuration.streamId())
                && configuration.streamEpoch() <= streamEpoch) {
            throw new IllegalArgumentException("reconfiguration requires a newer epoch");
        }
    }

    void configured(AvcDecoderConfiguration configuration) {
        streamId = configuration.streamId();
        streamEpoch = configuration.streamEpoch();
        lastSequence = -1;
        lastPresentationTimeMicros = -1;
        resynchronizing = false;
        state = State.AWAITING_KEY_FRAME;
    }

    Plan plan(EncodedAccessUnit unit) {
        Objects.requireNonNull(unit, "unit");
        if (state != State.AWAITING_KEY_FRAME && state != State.RENDERING) {
            throw new IllegalStateException("decoder is not accepting access units");
        }
        if (!unit.streamId().equals(streamId)) {
            throw new IllegalArgumentException("access unit belongs to another stream");
        }
        if (unit.streamEpoch() != streamEpoch) {
            throw new IllegalArgumentException("access unit belongs to another epoch");
        }
        if (unit.sequence() <= lastSequence) {
            throw new IllegalArgumentException("access unit is duplicate or late");
        }
        if (unit.endOfStream()) {
            return Plan.QUEUE_END_OF_STREAM;
        }
        boolean gap = lastSequence >= 0 && unit.sequence() != lastSequence + 1;
        boolean flush = gap || unit.discontinuity();
        if (flush) {
            return unit.keyFrame() ? Plan.FLUSH_QUEUE : Plan.FLUSH_DROP_AWAITING_KEY_FRAME;
        }
        if (state == State.AWAITING_KEY_FRAME && !unit.keyFrame()) {
            return Plan.DROP_AWAITING_KEY_FRAME;
        }
        if (lastPresentationTimeMicros >= 0
                && unit.presentationTimeMicros() <= lastPresentationTimeMicros) {
            throw new IllegalArgumentException("presentation time did not advance");
        }
        return Plan.QUEUE;
    }

    void flushed() {
        lastPresentationTimeMicros = -1;
        resynchronizing = true;
        state = State.AWAITING_KEY_FRAME;
    }

    void dropped(EncodedAccessUnit unit) {
        lastSequence = unit.sequence();
        state = State.AWAITING_KEY_FRAME;
    }

    boolean queued(EncodedAccessUnit unit) {
        lastSequence = unit.sequence();
        if (unit.endOfStream()) {
            state = State.DRAINING;
            return false;
        }
        lastPresentationTimeMicros = unit.presentationTimeMicros();
        state = State.RENDERING;
        boolean wasResynchronizing = resynchronizing;
        resynchronizing = false;
        return wasResynchronizing;
    }

    void stopped() {
        state = State.STOPPED;
        resynchronizing = false;
    }

    void failed() {
        state = State.FAILED;
        resynchronizing = false;
    }
}
