package io.capyio.controllerlab;

import android.os.SystemClock;

import java.net.DatagramPacket;
import java.net.DatagramSocket;
import java.net.InetAddress;
import java.nio.charset.StandardCharsets;
import java.util.Locale;
import java.util.UUID;
import java.util.ArrayDeque;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.function.Consumer;

final class UdpControllerSender {
    private static final long FRAME_MILLIS = 8L;
    private static final int MAX_URGENT_FRAMES = 64;
    private final ControllerState state;
    private final String host;
    private final int port;
    private final String token;
    private final String session = UUID.randomUUID().toString();
    private final Consumer<String> status;
    private final AtomicBoolean running = new AtomicBoolean(false);
    private final Object frameSignal = new Object();
    private final ArrayDeque<ControllerState.Snapshot> urgentFrames = new ArrayDeque<>();
    private boolean controlsDirty;
    private Thread worker;

    UdpControllerSender(
            ControllerState state,
            String host,
            int port,
            String token,
            Consumer<String> status) {
        this.state = state;
        this.host = host;
        this.port = port;
        this.token = token.toLowerCase(Locale.ROOT);
        this.status = status;
    }

    void start() {
        if (!running.compareAndSet(false, true)) {
            return;
        }
        worker = new Thread(this::run, "capyio-controller-udp");
        worker.start();
    }

    void stop() {
        if (!running.compareAndSet(true, false)) {
            return;
        }
        Thread current = worker;
        if (current != null) {
            current.interrupt();
            try {
                current.join(1_000L);
            } catch (InterruptedException interrupted) {
                Thread.currentThread().interrupt();
            }
        }
        worker = null;
    }

    boolean isRunning() {
        return running.get();
    }

    /**
     * Wakes the sender for touch movement and preserves button/down/up snapshots.
     * Edge snapshots must not be reduced to the latest state: a quick tap can
     * otherwise begin and end between two periodic IMU frames.
     */
    void requestControlsFrame(boolean preserveEdge) {
        synchronized (frameSignal) {
            if (preserveEdge) {
                if (urgentFrames.size() == MAX_URGENT_FRAMES) {
                    urgentFrames.removeFirst();
                }
                urgentFrames.addLast(state.snapshot());
            } else {
                controlsDirty = true;
            }
            frameSignal.notifyAll();
        }
    }

    private void run() {
        long sequence = 0L;
        try (DatagramSocket socket = new DatagramSocket()) {
            InetAddress address = InetAddress.getByName(host);
            status.accept("STREAMING · " + address.getHostAddress() + ":" + port);
            while (running.get()) {
                ControllerState.Snapshot snapshot;
                synchronized (frameSignal) {
                    snapshot = urgentFrames.pollFirst();
                    if (snapshot == null) {
                        snapshot = state.snapshot();
                    }
                    controlsDirty = false;
                }
                send(socket, address, sequence++, snapshot);
                synchronized (frameSignal) {
                    if (urgentFrames.isEmpty() && !controlsDirty && running.get()) {
                        try {
                            frameSignal.wait(FRAME_MILLIS);
                        } catch (InterruptedException interrupted) {
                            if (running.get()) {
                                Thread.currentThread().interrupt();
                            }
                        }
                    }
                }
            }
            state.resetControls();
            for (int attempt = 0; attempt < 3; attempt++) {
                send(socket, address, sequence++, state.snapshot());
            }
        } catch (Exception error) {
            status.accept("ERROR · " + error.getClass().getSimpleName());
        } finally {
            running.set(false);
        }
    }

    private void send(
            DatagramSocket socket,
            InetAddress address,
            long sequence,
            ControllerState.Snapshot snapshot) throws Exception {
        long timestamp = Math.max(1L, SystemClock.elapsedRealtimeNanos());
        String json = encode(snapshot, sequence, timestamp);
        byte[] bytes = json.getBytes(StandardCharsets.UTF_8);
        if (bytes.length > 2_048) {
            throw new IllegalStateException("controller packet exceeds 2048 bytes");
        }
        socket.send(new DatagramPacket(bytes, bytes.length, address, port));
    }

    private String encode(ControllerState.Snapshot state, long sequence, long timestamp) {
        return new StringBuilder(512)
                .append('{')
                .append("\"version\":1,")
                .append("\"token\":\"").append(token).append("\",")
                .append("\"session\":\"").append(session).append("\",")
                .append("\"sequence\":").append(sequence).append(',')
                .append("\"timestampNanos\":").append(timestamp).append(',')
                .append("\"buttons\":").append(state.buttons()).append(',')
                .append("\"dpadX\":").append(state.dpadX()).append(',')
                .append("\"dpadY\":").append(state.dpadY()).append(',')
                .append("\"leftX\":").append(state.leftX()).append(',')
                .append("\"leftY\":").append(state.leftY()).append(',')
                .append("\"rightX\":").append(state.rightX()).append(',')
                .append("\"rightY\":").append(state.rightY()).append(',')
                .append("\"leftTrigger\":").append(state.leftTrigger()).append(',')
                .append("\"rightTrigger\":").append(state.rightTrigger()).append(',')
                .append("\"acceleration\":").append(vector(state.acceleration())).append(',')
                .append("\"angularVelocity\":").append(vector(state.angularVelocity())).append(',')
                .append("\"accelerationTimestampNanos\":")
                .append(Math.max(1L, state.accelerationTimestampNanos())).append(',')
                .append("\"angularVelocityTimestampNanos\":")
                .append(Math.max(1L, state.angularVelocityTimestampNanos()))
                .append('}')
                .toString();
    }

    private static String vector(float[] values) {
        return new StringBuilder(64)
                .append('[')
                .append(Float.toString(values[0])).append(',')
                .append(Float.toString(values[1])).append(',')
                .append(Float.toString(values[2]))
                .append(']')
                .toString();
    }
}
