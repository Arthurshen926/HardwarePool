package dev.capyio.android;

import java.util.List;

/** Closed local selection. Eligibility is not evidence that real capture succeeds. */
final class AndroidMicrophoneSettings {
    static final String LEGACY_FORMAT = "pcm-s16le-48000-mono";
    static final String SECURE_FORMAT = "pcm-s16le-48000-mono-tls13-v1";
    enum Profile {
        LEGACY("legacy", LEGACY_FORMAT),
        SECURE_MONO_TLS13_V1("secure_mono_tls13_v1", SECURE_FORMAT);
        final String id;
        final String format;
        Profile(String id, String format) { this.id = id; this.format = format; }
    }
    private Profile profile = Profile.LEGACY;
    private long revision;

    static Profile parse(String value) {
        for (Profile candidate : Profile.values()) if (candidate.id.equals(value)) return candidate;
        throw new IllegalArgumentException("Unknown microphone capture profile");
    }
    Profile profile() { return profile; }
    long revision() { return revision; }
    List<String> advertisedFormats() { return List.of(profile.format); }
    boolean secure() { return profile == Profile.SECURE_MONO_TLS13_V1; }
    void restore(String value) {
        try { profile = parse(value); } catch (IllegalArgumentException invalid) { profile = Profile.LEGACY; }
    }
    void invalidateRevision() {
        if (revision >= 9_007_199_254_740_991L) throw new IllegalArgumentException("Microphone revision exhausted");
        revision++;
    }
    // The owner must stop native jobs/capture and unpublish before calling this method.
    void select(Profile next) {
        if (next == null) throw new IllegalArgumentException("Invalid microphone selection");
        invalidateRevision();
        profile = next;
    }
    boolean acceptsDirection(String direction) {
        return "speaker".equals(direction)
                || (secure() ? "microphone-secure" : "microphone").equals(direction);
    }
    String json(boolean permissionReady, boolean hardwarePresent) {
        String eligibility = !secure() ? "not_requested" : hardwarePresent ? "eligible" : "unavailable";
        return "{\"schemaVersion\":1,\"revision\":" + revision
                + ",\"captureProfile\":\"" + profile.id + "\",\"source\":\"system_default\""
                + ",\"eligibility\":\"" + eligibility + "\",\"permissionReady\":" + permissionReady + "}";
    }
}
