package io.capyio.camera.lab;

import android.os.SystemClock;
import io.capyio.camera.contract.AvcCodecConfig;
import io.capyio.camera.contract.AvcEncoderConfig;
import io.capyio.camera.contract.AvcWireRecordEncoder;
import io.capyio.camera.contract.AvcWireSessionEncoder;
import io.capyio.camera.contract.BoundedAvcAccessUnitQueue;
import io.capyio.camera.contract.CameraTransportEndpoint;
import io.capyio.camera.contract.EncodedAvcAccessUnit;
import io.capyio.camera.contract.LoopbackConnectRetryPolicy;
import java.io.BufferedOutputStream;
import java.io.IOException;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.security.SecureRandom;
import java.util.List;
import java.util.Objects;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicLong;
import java.util.concurrent.atomic.AtomicReference;

/**
 * Explicit foreground-lab exporter for ADB reverse or a trusted LAN endpoint.
 *
 * <p>The destination has already passed the Android-free closed endpoint
 * contract. Camera/codec callbacks only perform non-waiting queue offers;
 * socket connect/write and CAVC record allocation occur on this object's
 * worker thread.</p>
 */
final class LoopbackAvcSender implements AutoCloseable {
    interface Listener {
        void onStatus(String state, long sentAccessUnits, long droppedAccessUnits);
    }

    private static final int CLOSE_JOIN_MILLIS = 500;
    private static final int QUEUE_CAPACITY = 2;
    private static final int MAX_STATUS_CHARS = 160;

    private final Listener listener;
    private final BoundedAvcAccessUnitQueue accessUnits =
            new BoundedAvcAccessUnitQueue(QUEUE_CAPACITY);
    private final AtomicReference<AvcCodecConfig> codecConfig = new AtomicReference<>();
    private final AtomicLong droppedAccessUnits = new AtomicLong();
    private final AvcWireRecordEncoder.StreamKey streamKey;
    private final AvcEncoderConfig encoderConfig;
    private final CameraTransportEndpoint endpoint;

    private volatile boolean running;
    private volatile Socket socket;
    private Thread worker;

    LoopbackAvcSender(
            CameraTransportEndpoint endpoint,
            AvcEncoderConfig encoderConfig,
            Listener listener) {
        this.listener = Objects.requireNonNull(listener, "listener");
        this.endpoint = Objects.requireNonNull(endpoint, "endpoint");
        byte[] streamId = new byte[16];
        new SecureRandom().nextBytes(streamId);
        long epoch = Math.max(1, SystemClock.elapsedRealtimeNanos());
        streamKey = new AvcWireRecordEncoder.StreamKey(streamId, epoch);
        this.encoderConfig = Objects.requireNonNull(encoderConfig, "encoderConfig");
    }

    void start() {
        if (running || worker != null) {
            throw new IllegalStateException("loopback AVC sender is one-shot");
        }
        running = true;
        worker = new Thread(this::runWorker, "CapyIO-AVC-Lab-Export");
        worker.start();
    }

    void setCodecConfig(AvcCodecConfig config) {
        codecConfig.compareAndSet(null, Objects.requireNonNull(config, "config"));
    }

    void offer(EncodedAvcAccessUnit unit) {
        if (!running) {
            droppedAccessUnits.incrementAndGet();
            return;
        }
        BoundedAvcAccessUnitQueue.OfferResult result =
                accessUnits.offer(Objects.requireNonNull(unit, "unit"));
        if (!result.accepted() || result.droppedOldest()) {
            droppedAccessUnits.incrementAndGet();
        }
    }

    private void runWorker() {
        long sent = 0;
        IOException lastFailure = null;
        try {
            for (int attempt = 1;
                    running && LoopbackConnectRetryPolicy.mayAttempt(attempt);
                    attempt++) {
                notifyStatus(
                        "waiting for "
                                + endpoint.modeLabel()
                                + " "
                                + attempt
                                + "/"
                                + LoopbackConnectRetryPolicy.MAX_ATTEMPTS,
                        sent);
                try {
                    sent = runConnection(sent);
                    break;
                } catch (IOException error) {
                    lastFailure = error;
                    if (!running
                            || !LoopbackConnectRetryPolicy.shouldRetryAfterFailure(attempt)) {
                        break;
                    }
                    notifyStatus(endpoint.modeLabel() + " interrupted; retrying", sent);
                    TimeUnit.MILLISECONDS.sleep(LoopbackConnectRetryPolicy.RETRY_DELAY_MILLIS);
                }
            }
            if (running && lastFailure != null) {
                notifyStatus("export stopped: " + lastFailure.getClass().getSimpleName(), sent);
            }
        } catch (InterruptedException error) {
            Thread.currentThread().interrupt();
        } catch (RuntimeException error) {
            if (running) {
                notifyStatus("export stopped: " + error.getClass().getSimpleName(), sent);
            }
        } finally {
            socket = null;
            running = false;
            notifyStatus("export stopped", sent);
        }
    }

    private long runConnection(long sent) throws IOException, InterruptedException {
        try (Socket connected = new Socket()) {
            socket = connected;
            connected.connect(
                    new InetSocketAddress(
                            InetAddress.getByAddress(endpoint.addressBytes()),
                            endpoint.port()),
                    LoopbackConnectRetryPolicy.CONNECT_TIMEOUT_MILLIS);
            connected.setTcpNoDelay(true);
            notifyStatus(endpoint.modeLabel() + " connected", sent);
            try (BufferedOutputStream output =
                    new BufferedOutputStream(connected.getOutputStream(), 64 * 1024)) {
                AvcWireSessionEncoder sessionEncoder =
                        new AvcWireSessionEncoder(streamKey, encoderConfig);
                AvcCodecConfig appliedConfig = null;
                while (running) {
                    AvcCodecConfig latestConfig = codecConfig.get();
                    if (latestConfig != null && latestConfig != appliedConfig) {
                        sessionEncoder.setCodecConfig(latestConfig);
                        appliedConfig = latestConfig;
                    }
                    java.util.Optional<EncodedAvcAccessUnit> queued = accessUnits.poll();
                    if (queued.isEmpty()) {
                        TimeUnit.MILLISECONDS.sleep(1);
                        continue;
                    }
                    EncodedAvcAccessUnit unit = queued.orElseThrow();
                    List<byte[]> records = sessionEncoder.encode(unit);
                    for (byte[] record : records) {
                        output.write(record);
                    }
                    if (!records.isEmpty()) {
                        output.flush();
                        if (!unit.codecConfig() && !unit.endOfStream()) {
                            sent++;
                        }
                        if (sent == 1 || unit.keyFrame() || sent % 30 == 0) {
                            notifyStatus("sending through " + endpoint.modeLabel(), sent);
                        }
                    }
                }
            }
            return sent;
        } finally {
            socket = null;
        }
    }

    private void notifyStatus(String state, long sent) {
        String bounded = state;
        if (bounded.length() > MAX_STATUS_CHARS) {
            bounded = bounded.substring(0, MAX_STATUS_CHARS);
        }
        listener.onStatus(bounded, sent, droppedAccessUnits.get());
    }

    @Override
    public void close() {
        running = false;
        Socket currentSocket = socket;
        if (currentSocket != null) {
            try {
                currentSocket.close();
            } catch (IOException ignored) {
                // The socket is already on a terminal cleanup path.
            }
        }
        Thread currentWorker = worker;
        worker = null;
        if (currentWorker != null) {
            currentWorker.interrupt();
            try {
                currentWorker.join(CLOSE_JOIN_MILLIS);
            } catch (InterruptedException error) {
                Thread.currentThread().interrupt();
            }
        }
        for (int index = 0; index < QUEUE_CAPACITY; index++) {
            if (accessUnits.poll().isEmpty()) {
                break;
            }
        }
    }
}
