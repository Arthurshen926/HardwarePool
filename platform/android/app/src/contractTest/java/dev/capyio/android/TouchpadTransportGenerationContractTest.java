package dev.capyio.android;

import java.io.IOException;
import java.net.Socket;
import java.util.List;
import java.util.concurrent.TimeUnit;

import dev.capyio.android.contract.TouchpadContactSnapshot;

public final class TouchpadTransportGenerationContractTest {
    private static final class TrackedSocket extends Socket {
        int closes;
        @Override public void close() throws IOException { closes++; super.close(); }
    }

    public static void main(String[] args) throws Exception {
        TouchpadTransportGeneration old = new TouchpadTransportGeneration(1);
        TrackedSocket socket = new TrackedSocket();
        assert old.attach(socket);
        assert old.offerFrame(List.of());
        assert old.stop(2);
        assert old.stopping() && old.framesEmpty() && socket.closes == 1;
        assert old.terminalGeneration() == 2;
        assert old.stop(1) && old.terminalGeneration() == 2;
        assert socket.closes == 1;
        TrackedSocket late = new TrackedSocket();
        assert !old.attach(late) && late.closes == 1;

        TouchpadTransportGeneration retry = new TouchpadTransportGeneration(3);
        TrackedSocket fresh = new TrackedSocket();
        assert retry.attach(fresh);
        assert !retry.stop(2) && !retry.stopping() && fresh.closes == 0;
        assert retry.offerFrame(frame(0, 10));
        for (int i = 11; i < 100; i++) assert retry.offerFrame(frame(0, i));
        assert retry.queuedFrameCount() == 1 : "same-contact motion must coalesce";
        assert retry.pollFrame(0, TimeUnit.MILLISECONDS).get(0).x() == 99;

        // Contact membership transitions are retained instead of being replaced by motion.
        assert retry.offerFrame(frame(0, 100));
        assert retry.offerFrame(List.of(
                new TouchpadContactSnapshot(0, 100, 100, 1),
                new TouchpadContactSnapshot(1, 200, 200, 1)));
        assert retry.queuedFrameCount() == 2;

        // Even pathological transition pressure remains bounded and release always wins.
        for (int i = 0; i < 20; i++) {
            assert retry.offerFrame((i & 1) == 0 ? frame(0, i) : frame(1, i));
        }
        assert retry.queuedFrameCount() <= 8;
        assert retry.offerFrame(List.of(
                new TouchpadContactSnapshot(0, 100, 100, 1),
                new TouchpadContactSnapshot(1, 200, 200, 1),
                new TouchpadContactSnapshot(2, 300, 300, 1),
                new TouchpadContactSnapshot(3, 400, 400, 1)));
        assert retry.queuedFrameCount() == 2;
        assert retry.pollFrame(0, TimeUnit.MILLISECONDS).isEmpty();
        assert retry.pollFrame(0, TimeUnit.MILLISECONDS).size() == 4;
        assert retry.offerFrame(List.of());
        assert retry.queuedFrameCount() == 1;
        assert retry.pollFrame(0, TimeUnit.MILLISECONDS).isEmpty();

        assert retry.stop(4) && fresh.closes == 1 && retry.framesEmpty();
        assert !retry.offerFrame(List.of());
        System.out.println("Touchpad transport generation: lifecycle, bounded coalescing, transition retention and fail-safe release passed");
    }

    private static List<TouchpadContactSnapshot> frame(int id, int coordinate) {
        return List.of(new TouchpadContactSnapshot(id, coordinate, coordinate, 1));
    }
}
