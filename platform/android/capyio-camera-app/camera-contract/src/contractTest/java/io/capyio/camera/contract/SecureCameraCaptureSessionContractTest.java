package io.capyio.camera.contract;

/** Exercises the same orchestration gate used by AndroidCameraUseModule without hardware. */
public final class SecureCameraCaptureSessionContractTest {
    public static void run() {
        waitsForReadyAndAuthorization();
        stopBeforeReadyAndStaleGenerationStayStopped();
        permissionAndExpiryAreRequired();
        failureAndRevocationNeverRetry();
    }

    private static SecureCameraCaptureSession gate(FakeTransport transport, FakeCapture capture) {
        return new SecureCameraCaptureSession(transport, "route-1", System.currentTimeMillis() + 10_000, capture);
    }

    private static void waitsForReadyAndAuthorization() {
        FakeTransport transport = new FakeTransport();
        FakeCapture capture = new FakeCapture();
        SecureCameraCaptureSession gate = gate(transport, capture);
        check(gate.start(), "native preparation starts");
        gate.poll(true);
        check(capture.starts == 0 && !gate.capturing(), "Pending never captures");
        transport.state = SecureCameraTransport.READY;
        gate.poll(false);
        check(capture.starts == 0, "Ready without route authorization never captures");
        gate.poll(true);
        gate.poll(true);
        check(gate.capturing() && capture.starts == 1 && capture.generation == 7, "capture starts exactly once on Ready");
        check(!gate.start() && transport.starts == 1, "one-shot native start");
        gate.close();
        check(capture.stops == 1 && transport.stops == 1, "capture and native stop together");
    }

    private static void stopBeforeReadyAndStaleGenerationStayStopped() {
        FakeTransport transport = new FakeTransport();
        FakeCapture capture = new FakeCapture();
        SecureCameraCaptureSession gate = gate(transport, capture);
        gate.start();
        gate.close();
        transport.state = SecureCameraTransport.READY;
        gate.poll(true);
        check(gate.terminal() && capture.starts == 0 && !gate.start(), "late Ready after Stop ignored");
        gate = gate(transport, capture);
        gate.start();
        transport.currentGeneration = 8; // New generation is Ready, old one is not.
        gate.poll(true);
        check(gate.terminal() && capture.starts == 0, "stale generation Ready cannot start capture");
    }

    private static void permissionAndExpiryAreRequired() {
        FakeTransport transport = new FakeTransport();
        FakeCapture capture = new FakeCapture();
        capture.permission = false;
        SecureCameraCaptureSession gate = gate(transport, capture);
        check(!gate.start() && gate.terminal() && transport.starts == 0, "no permission never prepares native");
        capture.permission = true;
        gate = new SecureCameraCaptureSession(transport, "route-1", System.currentTimeMillis() - 1, capture);
        check(!gate.start() && gate.terminal(), "expired grant cannot start");
        gate = gate(transport, capture);
        transport.state = SecureCameraTransport.READY;
        gate.start();
        gate.poll(true);
        capture.permission = false;
        gate.poll(true);
        check(gate.terminal() && capture.stops == 1, "permission revocation stops active capture");
    }

    private static void failureAndRevocationNeverRetry() {
        FakeTransport transport = new FakeTransport();
        FakeCapture capture = new FakeCapture();
        SecureCameraCaptureSession gate = gate(transport, capture);
        gate.start();
        transport.state = SecureCameraTransport.FAILED;
        gate.poll(true);
        transport.state = SecureCameraTransport.READY;
        gate.poll(true);
        check(gate.terminal() && capture.starts == 0 && transport.starts == 1, "failure never restarts");
        gate = gate(transport, capture);
        gate.start();
        gate.poll(true);
        gate.poll(false);
        check(gate.terminal() && capture.stops == 1, "route run revocation stops capture");
        capture.acceptStart = false;
        gate = gate(transport, capture);
        transport.state = SecureCameraTransport.READY;
        gate.start();
        gate.poll(true);
        check(gate.terminal() && capture.stops == 2, "partial failed capture start cleaned up");
    }

    private static void check(boolean value, String reason) {
        if (!value) throw new AssertionError(reason);
    }

    private static final class FakeCapture implements SecureCameraCaptureSession.Capture {
        boolean permission = true;
        boolean acceptStart = true;
        int starts;
        int stops;
        long generation;
        public boolean permissionGranted() { return permission; }
        public boolean start(long generation) { starts++; this.generation = generation; return acceptStart; }
        public void stop() { stops++; }
    }

    private static final class FakeTransport implements SecureCameraTransport {
        int state = PENDING;
        long currentGeneration = 7;
        int starts;
        int stops;
        public long start(String routeId) { starts++; return currentGeneration; }
        public int state(long generation) { return generation == currentGeneration ? state : STOPPED; }
        public boolean record(long generation, byte[] record) { throw new AssertionError("gate sends no records"); }
        public void stop(long generation) { stops++; if (generation == currentGeneration) state = STOPPED; }
    }
}
