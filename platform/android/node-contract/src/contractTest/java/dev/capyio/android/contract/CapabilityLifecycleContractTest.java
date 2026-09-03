package dev.capyio.android.contract;

public final class CapabilityLifecycleContractTest {
    private static int assertions;

    private CapabilityLifecycleContractTest() {}

    public static void main(String[] args) {
        startStopAndRetryAreGenerationBound();
        invalidTransitionsAndFailuresFailClosed();
        gamepadFramesAreBoundedNeutralAndGenerationScoped();
        System.out.println("Android Capability lifecycle contract: PASS ("
                + assertions + " assertions)");
    }

    private static void startStopAndRetryAreGenerationBound() {
        CapabilityLifecycleController controller = new CapabilityLifecycleController();
        check(controller.snapshot().state() == CapabilityLifecycleState.STOPPED);
        TransitionToken start = controller.beginStart();
        check(start.accepted());
        check(controller.completeStart(start.generation()));
        check(controller.snapshot().state() == CapabilityLifecycleState.ACTIVE);

        TransitionToken stop = controller.beginStop();
        check(stop.accepted());
        check(!controller.completeStart(start.generation()));
        check(controller.completeStop(stop.generation()));
        check(controller.snapshot().state() == CapabilityLifecycleState.STOPPED);

        TransitionToken retry = controller.beginStart();
        check(retry.accepted());
        check(retry.generation() > stop.generation());
        check(!controller.completeStop(stop.generation()));
        check(controller.completeStart(retry.generation()));
    }

    private static void invalidTransitionsAndFailuresFailClosed() {
        CapabilityLifecycleController controller = new CapabilityLifecycleController();
        TransitionToken start = controller.beginStart();
        check(!controller.beginStart().accepted());
        check(!controller.fail(start.generation() + 1, "CAPY.TEST.FAILURE"));
        check(controller.fail(start.generation(), "CAPY.TEST.FAILURE"));
        check(controller.snapshot().state() == CapabilityLifecycleState.FAILED);
        check("CAPY.TEST.FAILURE".equals(controller.snapshot().problemCode()));
        check(!controller.completeStart(start.generation()));

        TransitionToken retry = controller.beginStart();
        check(retry.accepted());
        check(controller.completeStart(retry.generation()));
        check(!controller.beginStart().accepted());
    }

    private static void gamepadFramesAreBoundedNeutralAndGenerationScoped() {
        GamepadNodeController controller = new GamepadNodeController();
        TransitionToken start = controller.beginStart();
        check(start.accepted());
        check(!controller.submit(start.generation(), new GamepadControlsSnapshot(
                1, 0, 0, 1, -1, 0, 0, 0, GamepadControlsSnapshot.MAX_TRIGGER)));
        check(controller.completeStart(start.generation()));
        GamepadControlsSnapshot active = new GamepadControlsSnapshot(
                GamepadControlsSnapshot.SUPPORTED_BUTTON_MASK,
                -1,
                1,
                GamepadControlsSnapshot.MIN_AXIS,
                GamepadControlsSnapshot.MAX_AXIS,
                0,
                0,
                1,
                GamepadControlsSnapshot.MAX_TRIGGER);
        check(controller.submit(start.generation(), active));
        check(controller.controls().equals(active));

        TransitionToken stop = controller.beginStop();
        check(stop.accepted());
        check(controller.controls().equals(GamepadControlsSnapshot.neutral()));
        check(!controller.submit(start.generation(), active));
        check(controller.completeStop(stop.generation()));
        TransitionToken retry = controller.beginStart();
        check(retry.accepted());
        check(controller.completeStart(retry.generation()));
        check(!controller.submit(start.generation(), active));
        check(controller.submit(retry.generation(), active));
        check(controller.fail(retry.generation(), "CAPY.TEST.GAMEPAD_FAILED"));
        check(controller.controls().equals(GamepadControlsSnapshot.neutral()));

        rejects(() -> new GamepadControlsSnapshot(1 << 11, 0, 0, 0, 0, 0, 0, 0, 0));
        rejects(() -> new GamepadControlsSnapshot(0, 2, 0, 0, 0, 0, 0, 0, 0));
        rejects(() -> new GamepadControlsSnapshot(
                0, 0, 0, GamepadControlsSnapshot.MAX_AXIS + 1, 0, 0, 0, 0, 0));
        rejects(() -> new GamepadControlsSnapshot(
                0, 0, 0, 0, 0, 0, 0, -1, 0));
    }

    private static void rejects(Runnable operation) {
        try {
            operation.run();
            check(false);
        } catch (IllegalArgumentException expected) {
            check(true);
        }
    }

    private static void check(boolean condition) {
        assertions++;
        if (!condition) {
            throw new AssertionError("lifecycle assertion " + assertions + " failed");
        }
    }
}
