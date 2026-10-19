package dev.capyio.android;

/** Logical users of one sensor registration; called under the adapter lock. */
final class ImuSamplingUsers {
    private boolean independent;
    private boolean gamepad;

    void acquireIndependent() { independent = true; }
    void acquireGamepad() { gamepad = true; }
    void releaseIndependent() { independent = false; }
    void releaseGamepad() { gamepad = false; }
    boolean inUse() { return independent || gamepad; }
}
