package dev.capyio.display.app;

import android.view.Surface;
import java.util.Arrays;
import java.util.Objects;
import java.util.concurrent.atomic.AtomicBoolean;

/** Owns one authenticated launch exchange and its matching native video connection. */
final class AuthenticatedGameStreamReceiver implements AutoCloseable {
    interface StatusListener {
        void onStatus(String status);
    }

    private final ProvisionedDisplayConfiguration.GameStreamLaunch provisioning;
    private final Surface surface;
    private final StatusListener statusListener;
    private final AtomicBoolean closed = new AtomicBoolean();
    private GameStreamNativeClient activeNativeClient;
    private Thread worker;

    AuthenticatedGameStreamReceiver(
            ProvisionedDisplayConfiguration.GameStreamLaunch provisioning,
            Surface surface,
            StatusListener statusListener) {
        this.provisioning = Objects.requireNonNull(provisioning, "provisioning");
        this.surface = Objects.requireNonNull(surface, "surface");
        this.statusListener = Objects.requireNonNull(statusListener, "statusListener");
    }

    synchronized void start() {
        if (worker != null || closed.get()) {
            throw new IllegalStateException("GameStream receiver already started or closed");
        }
        GameStreamNativeClient.requireAvailable();
        worker = new Thread(this::run, "CapyIO-GameStream");
        worker.start();
    }

    private void run() {
        int reconnectAttempt = 0;
        while (!closed.get()) {
            statusListener.onStatus(reconnectAttempt == 0
                    ? "authenticating GameStream launch"
                    : "reconnecting authenticated GameStream launch");
            try (GameStreamLaunchRecord launch =
                    AuthenticatedGameStreamLaunchClient.connectAndReceive(
                            provisioning.tls(), provisioning.expected())) {
                if (closed.get()) {
                    return;
                }
                byte[] key = launch.keyCopy();
                GameStreamNativeClient nativeClient = null;
                int result;
                try {
                    nativeClient = new GameStreamNativeClient(surface, statusListener::onStatus);
                    setActiveNativeClient(nativeClient);
                    if (closed.get()) {
                        nativeClient.interrupt();
                        return;
                    }
                    statusListener.onStatus("GameStream launch acknowledged");
                    result = nativeClient.run(provisioning.expected(), key, launch.keyId());
                } finally {
                    Arrays.fill(key, (byte) 0);
                    if (nativeClient != null) {
                        clearActiveNativeClient(nativeClient);
                        nativeClient.close();
                    }
                }
                if (closed.get()) {
                    return;
                }
                if (result == 0) {
                    statusListener.onStatus("GameStream receiver stopped");
                    return;
                }
                if (!DisplayReconnectPolicy.isTransientGameStreamResult(result)) {
                    statusListener.onStatus("GameStream receiver rejected (" + result + ")");
                    return;
                }
                reconnectAttempt++;
                if (!waitForRetry(reconnectAttempt, "GameStream receiver failed (" + result + ")")) {
                    return;
                }
            } catch (LinkageError error) {
                if (!closed.get()) {
                    statusListener.onStatus("GameStream client unavailable: " + describe(error));
                }
                return;
            } catch (Exception error) {
                if (closed.get()) {
                    return;
                }
                if (DisplayReconnectPolicy.isPermanentLaunchFailure(error)) {
                    statusListener.onStatus("GameStream launch rejected: " + describe(error));
                    return;
                }
                reconnectAttempt++;
                if (!waitForRetry(reconnectAttempt, "GameStream unavailable: " + describe(error))) {
                    return;
                }
            }
        }
    }

    private boolean waitForRetry(int attempt, String reason) {
        if (attempt > DisplayReconnectPolicy.MAX_ATTEMPTS) {
            statusListener.onStatus("GameStream reconnect attempts exhausted: " + reason);
            return false;
        }
        long delayMillis = DisplayReconnectPolicy.delayMillis(attempt);
        statusListener.onStatus(reason + "; retry " + attempt + "/"
                + DisplayReconnectPolicy.MAX_ATTEMPTS + " in " + delayMillis + "ms");
        try {
            Thread.sleep(delayMillis);
            return !closed.get();
        } catch (InterruptedException interrupted) {
            Thread.currentThread().interrupt();
            if (!closed.get()) {
                statusListener.onStatus("GameStream reconnect interrupted");
            }
            return false;
        }
    }

    private synchronized void setActiveNativeClient(GameStreamNativeClient client) {
        activeNativeClient = client;
        if (closed.get()) {
            client.interrupt();
        }
    }

    private synchronized void clearActiveNativeClient(GameStreamNativeClient client) {
        if (activeNativeClient == client) {
            activeNativeClient = null;
        }
    }

    private static String describe(Throwable error) {
        String message = error.getMessage();
        if (message == null || message.isBlank()) {
            return error.getClass().getSimpleName();
        }
        return message;
    }

    synchronized long presentedFrames() {
        return activeNativeClient == null ? 0 : activeNativeClient.presentedFrames();
    }

    synchronized boolean isRunning() {
        return !closed.get() && worker != null && worker.isAlive();
    }

    @Override
    public void close() {
        if (!closed.compareAndSet(false, true)) {
            return;
        }
        Thread active;
        GameStreamNativeClient nativeClient;
        synchronized (this) {
            active = worker;
            nativeClient = activeNativeClient;
        }
        if (nativeClient != null) {
            nativeClient.interrupt();
        }
        if (active != null) {
            active.interrupt();
        }
    }
}
