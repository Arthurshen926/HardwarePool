package dev.capyio.android;

import android.os.SystemClock;

import dev.capyio.android.contract.GamepadControlsSnapshot;

import java.net.DatagramPacket;
import java.net.DatagramSocket;
import java.net.InetAddress;
import java.nio.charset.StandardCharsets;
import java.util.Locale;
import java.util.UUID;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.function.Supplier;

/** Bounded UDP publisher shared by the unified gamepad and standalone IMU Sources. */
final class ControllerRouteAdapter {
    private static final long FRAME_MILLIS = 8;
    private static final int MAX_PACKET_BYTES = 2_048;
    private static final int MAX_CONTROL_FRAMES = 64;

    private final Supplier<GamepadControlsSnapshot> controls;
    private final Supplier<ImuSourceAdapter.SampleSnapshot> motion;
    private final AtomicBoolean running = new AtomicBoolean();
    private final ArrayBlockingQueue<GamepadControlsSnapshot> controlFrames =
            new ArrayBlockingQueue<>(MAX_CONTROL_FRAMES);
    private volatile Endpoint endpoint;
    private volatile Thread worker;
    private volatile String problemCode;

    ControllerRouteAdapter(
            Supplier<GamepadControlsSnapshot> controls,
            Supplier<ImuSourceAdapter.SampleSnapshot> motion) {
        this.controls = controls;
        this.motion = motion;
    }

    synchronized boolean configure(String host, int port, String token) {
        if (running.get()) {
            return false;
        }
        try {
            endpoint = Endpoint.parse(host, port, token);
            problemCode = null;
            return true;
        } catch (IllegalArgumentException rejected) {
            problemCode = "CAPY.ANDROID.CONTROLLER_CONFIG_INVALID";
            return false;
        }
    }

    synchronized boolean start() {
        if (running.get()) {
            return true;
        }
        if (endpoint == null) {
            problemCode = "CAPY.ANDROID.CONTROLLER_CONFIG_REQUIRED";
            return false;
        }
        running.set(true);
        controlFrames.clear();
        worker = new Thread(this::run, "capyio-controller-route");
        worker.start();
        return true;
    }

    synchronized void stop() {
        if (!running.compareAndSet(true, false)) {
            return;
        }
        Thread current = worker;
        if (current != null) {
            current.interrupt();
        }
    }

    boolean isRunning() {
        return running.get();
    }

    boolean submitControls(GamepadControlsSnapshot snapshot) {
        return !running.get() || controlFrames.offer(snapshot);
    }

    String problemCode() {
        return problemCode;
    }

    String endpointLabel() {
        Endpoint current = endpoint;
        return current == null ? null : current.host + ":" + current.port;
    }

    private void run() {
        Endpoint target = endpoint;
        long sequence = 0;
        String session = UUID.randomUUID().toString();
        try (DatagramSocket socket = new DatagramSocket()) {
            InetAddress address = InetAddress.getByName(target.host);
            while (running.get()) {
                GamepadControlsSnapshot snapshot = controlFrames.poll();
                if (snapshot == null) {
                    snapshot = controls.get();
                }
                byte[] bytes = encode(
                        target.token,
                        session,
                        sequence++,
                        snapshot,
                        motion.get()).getBytes(StandardCharsets.UTF_8);
                if (bytes.length > MAX_PACKET_BYTES) {
                    throw new IllegalStateException("controller packet exceeds bound");
                }
                socket.send(new DatagramPacket(bytes, bytes.length, address, target.port));
                try {
                    Thread.sleep(FRAME_MILLIS);
                } catch (InterruptedException stopped) {
                    Thread.currentThread().interrupt();
                    break;
                }
            }
            for (int attempt = 0; attempt < 3; attempt++) {
                byte[] neutral = encode(
                        target.token,
                        session,
                        sequence++,
                        GamepadControlsSnapshot.neutral(),
                        motion.get()).getBytes(StandardCharsets.UTF_8);
                socket.send(new DatagramPacket(neutral, neutral.length, address, target.port));
            }
        } catch (Exception failed) {
            if (running.get()) {
                problemCode = "CAPY.ANDROID.CONTROLLER_TRANSPORT_FAILED";
            }
        } finally {
            running.set(false);
            controlFrames.clear();
            worker = null;
        }
    }

    static String encode(
            String token,
            String session,
            long sequence,
            GamepadControlsSnapshot state,
            ImuSourceAdapter.SampleSnapshot motion) {
        return ControllerPacketJson.encode(
                token,
                session,
                sequence,
                SystemClock.elapsedRealtimeNanos(),
                state,
                motion.accelerationTimestampNanos(),
                motion.accelerationX(),
                motion.accelerationY(),
                motion.accelerationZ(),
                motion.angularVelocityTimestampNanos(),
                motion.angularVelocityX(),
                motion.angularVelocityY(),
                motion.angularVelocityZ());
    }

    private record Endpoint(String host, int port, String token) {
        static Endpoint parse(String host, int port, String token) {
            String normalizedHost = host == null ? "" : host.trim();
            String normalizedToken = token == null ? "" : token.trim().toLowerCase(Locale.ROOT);
            if (!normalizedHost.matches("(?:\\d{1,3}\\.){3}\\d{1,3}")
                    || port < 1 || port > 65_535
                    || !normalizedToken.matches("[0-9a-f]{8,64}")) {
                throw new IllegalArgumentException("invalid controller endpoint");
            }
            String[] octets = normalizedHost.split("\\.");
            for (String octet : octets) {
                if (Integer.parseInt(octet) > 255) {
                    throw new IllegalArgumentException("invalid IPv4 endpoint");
                }
            }
            return new Endpoint(normalizedHost, port, normalizedToken);
        }
    }
}
