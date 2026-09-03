package dev.capyio.android.contract;

/** Generation-bound observer snapshot for one non-audio platform Adapter. */
public record CapabilityLifecycleSnapshot(
        CapabilityLifecycleState state,
        long generation,
        String problemCode) {
    public CapabilityLifecycleSnapshot {
        if (state == null || generation < 0) {
            throw new IllegalArgumentException("Capability lifecycle snapshot is invalid");
        }
    }
}
