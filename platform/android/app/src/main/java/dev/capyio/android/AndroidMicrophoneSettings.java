package dev.capyio.android;

import java.util.List;

/** Closed local selection. Eligibility is not evidence that real capture succeeds. */
final class AndroidMicrophoneSettings {
    static final String LEGACY_FORMAT = "pcm-s16le-48000-mono";
    static final String SECURE_FORMAT = "pcm-s16le-48000-mono-tls13-v1";
    static final String SECURE_300S_FORMAT = "pcm-s16le-48000-mono-tls13-300s-v1";
    enum Profile {
        LEGACY("legacy", LEGACY_FORMAT),
        SECURE_MONO_TLS13_V1("secure_mono_tls13_v1", SECURE_FORMAT),
        SECURE_MONO_TLS13_300S_V1("secure_mono_tls13_300s_v1", SECURE_300S_FORMAT);
        final String id;
        final String format;
        Profile(String id, String format) { this.id = id; this.format = format; }
        boolean secure() { return this == SECURE_MONO_TLS13_V1 || this == SECURE_MONO_TLS13_300S_V1; }
    }
    record Selection(Profile profile, long revision) {
        Selection {
            if (profile == null || revision < 0 || revision > 9_007_199_254_740_991L) {
                throw new IllegalArgumentException("Invalid microphone selection revision");
            }
        }
    }
    /** One outstanding platform dialog; cancellation does not reuse its callback. */
    static final class PendingPermission {
        private boolean awaiting;
        private Selection selected;
        boolean awaiting() { return awaiting; }
        boolean begin(Selection selection) {
            if (awaiting) return false;
            awaiting = true;
            selected = selection;
            return true;
        }
        void cancel() { selected = null; }
        Selection finish() {
            Selection original = selected;
            selected = null;
            awaiting = false;
            return original;
        }
    }
    // The capture worker reads this without acquiring the Service monitor.
    // Publish the profile and its revision atomically, never as two mutable fields.
    private volatile Selection selection = new Selection(Profile.LEGACY, 0);

    static Profile parse(String value) {
        for (Profile candidate : Profile.values()) if (candidate.id.equals(value)) return candidate;
        throw new IllegalArgumentException("Unknown microphone capture profile");
    }
    Selection selection() { return selection; }
    Profile profile() { return selection.profile(); }
    long revision() { return selection.revision(); }
    List<String> advertisedFormats() { return List.of(selection.profile().format); }
    boolean secure() { return selection.profile().secure(); }
    boolean isCurrent(Profile expectedProfile, long expectedRevision) {
        Selection current = selection;
        return current.profile() == expectedProfile && current.revision() == expectedRevision;
    }
    boolean canStartLegacy(Selection expected) {
        return expected != null && expected.profile() == Profile.LEGACY
                && isCurrent(expected.profile(), expected.revision());
    }
    void restore(String value) {
        Profile restored;
        try { restored = parse(value); } catch (IllegalArgumentException invalid) { restored = Profile.LEGACY; }
        // Used during Service initialization, before any publication/capture.
        // Still give every restore a fresh revision, never revive a stale snapshot.
        select(restored);
    }
    void invalidateRevision() {
        Selection current = selection;
        if (current.revision() >= 9_007_199_254_740_991L) throw new IllegalArgumentException("Microphone revision exhausted");
        selection = new Selection(current.profile(), current.revision() + 1);
    }
    // The owner must stop native jobs/capture and unpublish before calling this method.
    void select(Profile next) {
        if (next == null) throw new IllegalArgumentException("Invalid microphone selection");
        Selection current = selection;
        if (current.revision() >= 9_007_199_254_740_991L) throw new IllegalArgumentException("Microphone revision exhausted");
        selection = new Selection(next, current.revision() + 1);
    }
    boolean acceptsDirection(String direction) {
        return "speaker".equals(direction)
                || (secure() ? "microphone-secure" : "microphone").equals(direction);
    }
    String json(boolean permissionReady, boolean hardwarePresent) {
        Selection current = selection;
        String eligibility = !current.profile().secure() ? "not_requested" : hardwarePresent ? "eligible" : "unavailable";
        return "{\"schemaVersion\":1,\"revision\":" + current.revision()
                + ",\"captureProfile\":\"" + current.profile().id + "\",\"source\":\"system_default\""
                + ",\"eligibility\":\"" + eligibility + "\",\"permissionReady\":" + permissionReady + "}";
    }
}
