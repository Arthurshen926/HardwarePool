package dev.capyio.android.lan;

import java.io.IOException;
import java.util.Objects;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.function.Consumer;

/** Owns packet-queue to UDP movement on one bounded background worker. */
public final class NativeLanSenderWorker implements AutoCloseable {
    public static final String PROBLEM_IO = "CAPY.ANDROID.LAN_SEND_IO";
    public static final String PROBLEM_ROUTE_ENDED = "CAPY.ANDROID.LAN_ROUTE_ENDED";

    private final NativeLanUdpEndpoint endpoint;
    private final NativeLanPacketQueue input;
    private final Consumer<String> failureListener;
    private final NativeLanRouteGuard routeGuard;
    private final AtomicBoolean running = new AtomicBoolean();
    private Thread worker;
    private boolean started;
    private boolean stopped;

    public NativeLanSenderWorker(
            NativeLanUdpEndpoint endpoint,
            NativeLanPacketQueue input,
            Consumer<String> failureListener) {
        this(endpoint, input, failureListener, null);
    }

    /** Null guard is reserved for the existing explicitly trusted lab path. */
    public NativeLanSenderWorker(
            NativeLanUdpEndpoint endpoint, NativeLanPacketQueue input,
            Consumer<String> failureListener, NativeLanRouteGuard routeGuard) {
        this.endpoint = Objects.requireNonNull(endpoint, "endpoint");
        this.input = Objects.requireNonNull(input, "input");
        this.failureListener = Objects.requireNonNull(failureListener, "failureListener");
        this.routeGuard = routeGuard;
        if (!endpoint.binding().matches(input.binding())) {
            throw new IllegalArgumentException("sender endpoint and queue bindings differ");
        }
        if (routeGuard != null && !endpoint.binding().matches(routeGuard.route.binding)) {
            throw new IllegalArgumentException("sender Route guard binding differs");
        }
    }

    public synchronized void start() {
        if (started || stopped) {
            throw new IllegalStateException("native LAN sender is one-shot");
        }
        started = true;
        running.set(true);
        Thread next = new Thread(this::run, "capyio-native-lan-sender");
        next.setDaemon(true);
        worker = next;
        next.start();
    }

    public void stop() {
        Thread current;
        synchronized (this) {
            current = worker;
            stopped = true;
            running.set(false);
        }
        if (routeGuard != null) routeGuard.revoke();
        endpoint.close();
        if (current != null && current != Thread.currentThread()) {
            current.interrupt();
            joinBounded(current);
        }
    }

    public boolean isRunning() {
        return running.get();
    }

    @Override
    public void close() {
        stop();
    }

    private void run() {
        try {
            while (running.get()) {
                if (!routeIsLive()) break;
                NativeLanPacketCodec.Packet packet = input.poll(50);
                if (!running.get() || !routeIsLive()) break;
                if (packet != null) {
                    endpoint.send(packet);
                }
            }
        } catch (InterruptedException interrupted) {
            Thread.currentThread().interrupt();
        } catch (IOException failure) {
            if (running.getAndSet(false)) {
                failureListener.accept(PROBLEM_IO);
            }
        } finally {
            running.set(false);
            if (routeGuard != null) routeGuard.revoke();
            endpoint.close();
            synchronized (this) {
                worker = null;
            }
        }
    }

    private boolean routeIsLive() {
        if (routeGuard == null || routeGuard.isLive()) return true;
        if (running.getAndSet(false)) failureListener.accept(PROBLEM_ROUTE_ENDED);
        return false;
    }

    private static void joinBounded(Thread thread) {
        try {
            thread.join(2_000);
        } catch (InterruptedException interrupted) {
            Thread.currentThread().interrupt();
        }
        if (thread.isAlive()) {
            throw new IllegalStateException("native LAN sender did not stop within 2 seconds");
        }
    }
}
