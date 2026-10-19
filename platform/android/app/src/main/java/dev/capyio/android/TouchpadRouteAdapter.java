package dev.capyio.android;

import android.os.SystemClock;

import dev.capyio.android.contract.TouchpadContactSnapshot;
import dev.capyio.touchpad.NativeTouchpadRecordBridge;

import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import java.util.concurrent.TimeUnit;

/** Owns the bounded ADB-reverse touchpad lab connection used by the unified APK. */
final class TouchpadRouteAdapter {
    static final String PROBLEM_CONNECT = "CAPY.ANDROID.TOUCHPAD_RECEIVER_UNAVAILABLE";
    static final String PROBLEM_TRANSPORT = "CAPY.ANDROID.TOUCHPAD_TRANSPORT_FAILED";
    static final String PROBLEM_BACKPRESSURE = "CAPY.ANDROID.TOUCHPAD_QUEUE_FULL";

    private static final int LAB_PORT = 61_000;
    private static final int CONNECT_TIMEOUT_MILLIS = 3_000;
    private static final int IO_TIMEOUT_MILLIS = 3_000;
    private static final int HOLD_REFRESH_MILLIS = 250;
    interface Listener {
        void onStarted(long generation);

        void onStopped(long generation);

        void onFailed(long generation, String problemCode);
    }

    private final Listener listener;
    private Thread worker;
    private TouchpadTransportGeneration active;
    private long secureJob;

    TouchpadRouteAdapter(Listener listener) {
        this.listener = listener;
    }

    synchronized void start(long nextGeneration) {
        if (active != null) {
            listener.onFailed(nextGeneration, PROBLEM_TRANSPORT);
            return;
        }
        TouchpadTransportGeneration run = new TouchpadTransportGeneration(nextGeneration);
        active = run;
        worker = new Thread(() -> run(run), "capyio-touchpad-route");
        worker.start();
    }

    synchronized void startSecure(long nextGeneration, long job) {
        if (active != null || job <= 0) {
            AndroidNativeTouchpadData.nativeStop(job);
            listener.onFailed(nextGeneration, PROBLEM_TRANSPORT);
            return;
        }
        TouchpadTransportGeneration run = new TouchpadTransportGeneration(nextGeneration);
        active = run;
        secureJob = job;
        worker = new Thread(() -> runSecure(run, job), "capyio-touchpad-tls");
        worker.start();
    }

    synchronized boolean submit(long expectedGeneration, List<TouchpadContactSnapshot> contacts) {
        if (active == null || active.stopping() || active.generation != expectedGeneration) {
            return false;
        }
        ArrayList<TouchpadContactSnapshot> sorted = new ArrayList<>(contacts);
        sorted.sort(Comparator.comparingInt(TouchpadContactSnapshot::contactId));
        List<TouchpadContactSnapshot> copy = List.copyOf(sorted);
        return active.offerFrame(copy);
    }

    synchronized void stop(long expectedGeneration) {
        if (active == null) {
            listener.onStopped(expectedGeneration);
            return;
        }
        if (!active.stop(expectedGeneration)) return;
        if (secureJob != 0) AndroidNativeTouchpadData.nativeStop(secureJob);
        Thread current = worker;
        if (current != null) {
            current.interrupt();
        }
    }

    private void runSecure(TouchpadTransportGeneration run, long job) {
        long handle = 0;
        String failure = null;
        try {
            handle = AndroidNativeTouchpadData.nativeOpen(job);
            sendSecure(job, NativeTouchpadRecordBridge.nativeRecordHello(handle));
            sendSecure(job, NativeTouchpadRecordBridge.nativeRecordStart(handle, SystemClock.elapsedRealtimeNanos()));
            if (!run.stopping()) listener.onStarted(run.generation);
            List<TouchpadContactSnapshot> previous = List.of();
            while (!run.stopping()) {
                List<TouchpadContactSnapshot> current = run.pollFrame(
                        HOLD_REFRESH_MILLIS, TimeUnit.MILLISECONDS);
                if (current == null) current = previous;
                // Empty snapshots also keep the finite data-channel watchdog live.
                for (TouchpadMotionFrame motion : TouchpadMotionFrame.sequence(previous, current)) {
                    sendSecure(job, NativeTouchpadRecordBridge.nativeRecordMotion(handle,
                            SystemClock.elapsedRealtimeNanos(), motion.action, motion.actionIndex,
                            motion.ids, motion.tools, motion.x, motion.y, motion.pressure));
                }
                previous = current;
            }
        } catch (InterruptedException stopped) {
            Thread.currentThread().interrupt();
        } catch (RuntimeException rejected) {
            failure = PROBLEM_TRANSPORT;
        } finally {
            if (handle != 0) {
                try { NativeTouchpadRecordBridge.nativeRecordTakeClose(handle); }
                catch (RuntimeException ignored) { failure = PROBLEM_TRANSPORT; }
            }
            boolean remoteStopped = AndroidNativeTouchpadData.nativeWasStopped(job);
            AndroidNativeTouchpadData.nativeClose(job);
            synchronized (this) {
                if (active == run) { active = null; worker = null; secureJob = 0; }
            }
            if (run.stopping() || remoteStopped || failure == null) listener.onStopped(run.terminalGeneration());
            else listener.onFailed(run.generation, failure);
        }
    }

    private static void sendSecure(long job, byte[] record) {
        if (!AndroidNativeTouchpadData.nativeSend(job, record)) {
            throw new IllegalStateException("secure touchpad data connection stopped");
        }
    }

    private void run(TouchpadTransportGeneration run) {
        long activeGeneration = run.generation;
        long handle = 0;
        boolean started = false;
        String failure = null;
        try (Socket socket = new Socket()) {
            if (!run.attach(socket)) return;
            socket.setTcpNoDelay(true);
            socket.setSoTimeout(IO_TIMEOUT_MILLIS);
            socket.connect(new InetSocketAddress("127.0.0.1", LAB_PORT), CONNECT_TIMEOUT_MILLIS);
            DataInputStream input = new DataInputStream(socket.getInputStream());
            DataOutputStream output = new DataOutputStream(socket.getOutputStream());
            handle = createSession();
            writeRecord(output, input, NativeTouchpadRecordBridge.nativeRecordHello(handle));
            writeRecord(output, input, NativeTouchpadRecordBridge.nativeRecordStart(
                    handle, SystemClock.elapsedRealtimeNanos()));
            started = true;
            if (!run.stopping()) listener.onStarted(activeGeneration);
            List<TouchpadContactSnapshot> previous = List.of();
            while (!run.stopping()) {
                List<TouchpadContactSnapshot> current;
                try {
                    current = run.pollFrame(HOLD_REFRESH_MILLIS, TimeUnit.MILLISECONDS);
                } catch (InterruptedException stopped) {
                    Thread.currentThread().interrupt();
                    break;
                }
                if (current == null) {
                    if (previous.isEmpty()) continue;
                    // Complete held-contact snapshots keep the receiver's dead-peer watchdog live.
                    current = previous;
                }
                for (TouchpadMotionFrame motion : TouchpadMotionFrame.sequence(previous, current)) {
                    byte[] record = NativeTouchpadRecordBridge.nativeRecordMotion(
                            handle,
                            SystemClock.elapsedRealtimeNanos(),
                            motion.action,
                            motion.actionIndex,
                            motion.ids,
                            motion.tools,
                            motion.x,
                            motion.y,
                            motion.pressure);
                    writeRecord(output, input, record);
                }
                previous = current;
            }
            if (handle != 0 && !run.stopping()) {
                byte[] cancel = NativeTouchpadRecordBridge.nativeRecordClose(
                        handle, SystemClock.elapsedRealtimeNanos());
                if (cancel != null) {
                    writeRecord(output, input, cancel);
                }
                byte[] close = NativeTouchpadRecordBridge.nativeRecordTakeClose(handle);
                handle = 0;
                writeRecord(output, input, close);
            }
        } catch (IOException unavailable) {
            failure = started ? PROBLEM_TRANSPORT : PROBLEM_CONNECT;
        } catch (RuntimeException rejected) {
            failure = PROBLEM_TRANSPORT;
        } finally {
            if (handle != 0) {
                // JNI records are thread-local: release on this worker even after a failed write.
                try { NativeTouchpadRecordBridge.nativeRecordTakeClose(handle); }
                catch (RuntimeException cleanupFailed) { failure = PROBLEM_TRANSPORT; }
            }
            synchronized (this) {
                if (active == run) { active = null; worker = null; }
            }
            // Release ownership before notifying: retry from a callback must not be clobbered.
            if (run.stopping() || failure == null) listener.onStopped(run.terminalGeneration());
            else listener.onFailed(activeGeneration, failure);
        }
    }

    private static long createSession() {
        return NativeTouchpadRecordBridge.nativeRecordCreate(
                "00000000-0000-4000-8000-00000000f109",
                1,
                "android.elapsed-realtime-nanos",
                TouchpadContactSnapshot.MAX_COORDINATE,
                TouchpadContactSnapshot.MAX_COORDINATE,
                12_000,
                7_000,
                5,
                true,
                0,
                "00000000-0000-4000-8000-00000000f101",
                "00000000-0000-4000-8000-00000000f102",
                "00000000-0000-4000-8000-00000000f103",
                "00000000-0000-4000-8000-00000000f104",
                "00000000-0000-4000-8000-00000000f105",
                "00000000-0000-4000-8000-00000000f106",
                "00000000-0000-4000-8000-00000000f107",
                "00000000-0000-4000-8000-00000000f108",
                -1);
    }

    private static void writeRecord(
            DataOutputStream output, DataInputStream input, byte[] record) throws IOException {
        output.write(record);
        output.flush();
        if (record.length > 5 && record[5] == 2) {
            byte[] acknowledgement = new byte[24];
            input.readFully(acknowledgement);
            validateAcknowledgement(record, acknowledgement);
        }
    }

    private static void validateAcknowledgement(byte[] record, byte[] acknowledgement)
            throws IOException {
        if (acknowledgement.length != 24
                || acknowledgement[0] != 'C'
                || acknowledgement[1] != 'P'
                || acknowledgement[2] != 'T'
                || acknowledgement[3] != 'R'
                || acknowledgement[5] != 3) {
            throw new IOException("invalid touchpad acknowledgement");
        }
        for (int index = 0; index < 8; index++) {
            if (record[16 + index] != acknowledgement[16 + index]) {
                throw new IOException("touchpad acknowledgement sequence mismatch");
            }
        }
    }

}
