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
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.atomic.AtomicBoolean;

/** Owns the bounded ADB-reverse touchpad lab connection used by the unified APK. */
final class TouchpadRouteAdapter {
    static final String PROBLEM_CONNECT = "CAPY.ANDROID.TOUCHPAD_RECEIVER_UNAVAILABLE";
    static final String PROBLEM_TRANSPORT = "CAPY.ANDROID.TOUCHPAD_TRANSPORT_FAILED";

    private static final int LAB_PORT = 61_000;
    private static final int QUEUE_CAPACITY = 8;
    private static final int CONNECT_TIMEOUT_MILLIS = 3_000;
    private static final int IO_TIMEOUT_MILLIS = 3_000;
    interface Listener {
        void onStarted(long generation);

        void onStopped(long generation);

        void onFailed(long generation, String problemCode);
    }

    private final Listener listener;
    private final ArrayBlockingQueue<List<TouchpadContactSnapshot>> frames =
            new ArrayBlockingQueue<>(QUEUE_CAPACITY);
    private final AtomicBoolean running = new AtomicBoolean();
    private volatile Thread worker;
    private volatile long generation;
    private volatile long terminalGeneration;

    TouchpadRouteAdapter(Listener listener) {
        this.listener = listener;
    }

    synchronized void start(long nextGeneration) {
        if (!running.compareAndSet(false, true)) {
            listener.onFailed(nextGeneration, PROBLEM_TRANSPORT);
            return;
        }
        generation = nextGeneration;
        terminalGeneration = nextGeneration;
        frames.clear();
        worker = new Thread(() -> run(nextGeneration), "capyio-touchpad-route");
        worker.start();
    }

    boolean submit(long expectedGeneration, List<TouchpadContactSnapshot> contacts) {
        if (!running.get() || generation != expectedGeneration) {
            return false;
        }
        ArrayList<TouchpadContactSnapshot> sorted = new ArrayList<>(contacts);
        sorted.sort(Comparator.comparingInt(TouchpadContactSnapshot::contactId));
        List<TouchpadContactSnapshot> copy = List.copyOf(sorted);
        return frames.offer(copy);
    }

    synchronized void stop(long expectedGeneration) {
        terminalGeneration = expectedGeneration;
        running.set(false);
        Thread current = worker;
        if (current != null) {
            current.interrupt();
        }
    }

    private void run(long activeGeneration) {
        long handle = 0;
        boolean started = false;
        try (Socket socket = new Socket()) {
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
            listener.onStarted(activeGeneration);
            List<TouchpadContactSnapshot> previous = List.of();
            while (running.get() && generation == activeGeneration) {
                List<TouchpadContactSnapshot> current;
                try {
                    current = frames.take();
                } catch (InterruptedException stopped) {
                    Thread.currentThread().interrupt();
                    break;
                }
                TouchpadMotionFrame motion = TouchpadMotionFrame.between(previous, current);
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
                previous = current;
            }
            if (handle != 0) {
                byte[] cancel = NativeTouchpadRecordBridge.nativeRecordClose(
                        handle, SystemClock.elapsedRealtimeNanos());
                if (cancel != null) {
                    writeRecord(output, input, cancel);
                }
                writeRecord(output, input,
                        NativeTouchpadRecordBridge.nativeRecordTakeClose(handle));
                handle = 0;
            }
            listener.onStopped(terminalGeneration);
        } catch (IOException unavailable) {
            if (running.get()) {
                listener.onFailed(activeGeneration, started ? PROBLEM_TRANSPORT : PROBLEM_CONNECT);
            } else {
                listener.onStopped(terminalGeneration);
            }
        } catch (RuntimeException rejected) {
            if (running.get()) {
                listener.onFailed(activeGeneration, PROBLEM_TRANSPORT);
            } else {
                listener.onStopped(terminalGeneration);
            }
        } finally {
            running.compareAndSet(true, false);
            worker = null;
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
