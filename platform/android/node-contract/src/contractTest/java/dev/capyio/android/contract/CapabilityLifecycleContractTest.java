package dev.capyio.android.contract;

public final class CapabilityLifecycleContractTest {
    private static int assertions;

    private CapabilityLifecycleContractTest() {}

    public static void main(String[] args) {
        startStopAndRetryAreGenerationBound();
        invalidTransitionsAndFailuresFailClosed();
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

    private static void check(boolean condition) {
        assertions++;
        if (!condition) {
            throw new AssertionError("lifecycle assertion " + assertions + " failed");
        }
    }
}
