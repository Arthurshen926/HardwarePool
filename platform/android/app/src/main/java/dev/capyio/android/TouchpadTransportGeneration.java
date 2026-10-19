package dev.capyio.android;

import dev.capyio.android.contract.TouchpadContactSnapshot;
import java.io.IOException;
import java.net.Socket;
import java.util.List;
import java.util.concurrent.LinkedBlockingDeque;
import java.util.concurrent.TimeUnit;

/** One worker owns one queue/socket; a stopped generation cannot affect its replacement. */
final class TouchpadTransportGeneration {
    private static final int FRAME_CAPACITY = 8;

    final long generation;
    private final LinkedBlockingDeque<List<TouchpadContactSnapshot>> frames =
            new LinkedBlockingDeque<>(FRAME_CAPACITY);
    private boolean stopping;
    private long terminalGeneration;
    private Socket socket;

    TouchpadTransportGeneration(long generation) {
        this.generation = generation;
        terminalGeneration = generation;
    }

    synchronized boolean stopping() { return stopping; }
    synchronized long terminalGeneration() { return terminalGeneration; }

    /**
     * Keeps contact-set transitions bounded while replacing redundant motion snapshots.
     * Input bursts are normal and must not fail the Route. An empty snapshot always wins
     * under pressure so a delayed consumer can never retain a stuck contact.
     */
    synchronized boolean offerFrame(List<TouchpadContactSnapshot> frame) {
        if (stopping) return false;
        List<TouchpadContactSnapshot> tail = frames.peekLast();
        if (tail != null && sameContacts(tail, frame)) {
            frames.pollLast();
            frames.offerLast(frame);
            return true;
        }
        if (frames.offerLast(frame)) return true;
        frames.clear();
        frames.offerLast(List.of());
        if (!frame.isEmpty()) frames.offerLast(frame);
        // The consumer may already have an old held-contact set. Deliver an
        // explicit release before rebuilding the latest complete snapshot.
        // Never drop a newly added finger merely because the queue was full.
        return true;
    }

    List<TouchpadContactSnapshot> pollFrame(long timeout, TimeUnit unit)
            throws InterruptedException {
        return frames.pollFirst(timeout, unit);
    }

    synchronized int queuedFrameCount() { return frames.size(); }

    synchronized boolean framesEmpty() { return frames.isEmpty(); }

    private static boolean sameContacts(
            List<TouchpadContactSnapshot> left, List<TouchpadContactSnapshot> right) {
        if (left.size() != right.size()) return false;
        for (int index = 0; index < left.size(); index++) {
            if (left.get(index).contactId() != right.get(index).contactId()) return false;
        }
        return true;
    }

    synchronized boolean attach(Socket next) throws IOException {
        if (stopping) { next.close(); return false; }
        socket = next;
        return true;
    }

    synchronized boolean stop(long expectedGeneration) {
        // The controller advances its token on Stop; an old callback must not stop a retry.
        if (expectedGeneration < generation) return false;
        terminalGeneration = Math.max(terminalGeneration, expectedGeneration);
        stopping = true;
        frames.clear();
        if (socket != null) {
            try { socket.close(); } catch (IOException ignored) { /* Peer timeout remains fail-safe. */ }
            socket = null;
        }
        return true;
    }
}
