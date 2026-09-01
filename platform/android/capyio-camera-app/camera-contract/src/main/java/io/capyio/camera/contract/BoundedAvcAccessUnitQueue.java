package io.capyio.camera.contract;

import java.util.Objects;
import java.util.Optional;
import java.util.concurrent.locks.ReentrantLock;

/**
 * Fixed-capacity single-producer/single-consumer queue with drop-oldest policy.
 *
 * <p>MediaCodec callbacks use only {@code tryLock}; they never wait for the
 * consumer. A full queue drops its oldest item. Lock contention drops the
 * incoming item and both outcomes are observable.</p>
 */
public final class BoundedAvcAccessUnitQueue {
    public record OfferResult(boolean accepted, boolean droppedOldest) {}

    private final EncodedAvcAccessUnit[] slots;
    private final ReentrantLock lock = new ReentrantLock();
    private int head;
    private int size;

    public BoundedAvcAccessUnitQueue(int capacity) {
        if (capacity <= 0 || capacity > AvcEncoderConfig.MAX_QUEUE_CAPACITY) {
            throw new IllegalArgumentException("queue capacity is outside the bootstrap bound");
        }
        slots = new EncodedAvcAccessUnit[capacity];
    }

    public int capacity() {
        return slots.length;
    }

    public OfferResult offer(EncodedAvcAccessUnit unit) {
        Objects.requireNonNull(unit, "unit");
        if (!lock.tryLock()) {
            return new OfferResult(false, false);
        }
        try {
            boolean dropped = size == slots.length;
            if (dropped) {
                slots[head] = null;
                head = (head + 1) % slots.length;
                size--;
            }
            int tail = (head + size) % slots.length;
            slots[tail] = unit;
            size++;
            return new OfferResult(true, dropped);
        } finally {
            lock.unlock();
        }
    }

    public Optional<EncodedAvcAccessUnit> poll() {
        if (!lock.tryLock()) {
            return Optional.empty();
        }
        try {
            if (size == 0) {
                return Optional.empty();
            }
            EncodedAvcAccessUnit unit = slots[head];
            slots[head] = null;
            head = (head + 1) % slots.length;
            size--;
            return Optional.of(unit);
        } finally {
            lock.unlock();
        }
    }
}
