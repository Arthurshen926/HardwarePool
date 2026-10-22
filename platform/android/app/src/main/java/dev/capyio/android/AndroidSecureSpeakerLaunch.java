package dev.capyio.android;

import java.util.Map;
import java.util.Set;
import java.util.UUID;

/** Exact payload-free native handoff; authority and transport remain inside Rust. */
record AndroidSecureSpeakerLaunch(String routeId, String sessionId, String peerNodeId,
        String direction, AndroidSpeakerSettings.Profile profile, long settingsRevision, long expiresAtMs) {
    static final String TRANSPORT = "pinned-tls13-stereo-v1";
    static final String DIRECTION = "speaker-secure";
    private static final Set<String> FIELDS = Set.of("schemaVersion", "transport", "format",
            "pcmBytesPerRecord", "sampleRateHz", "channels", "sampleFramesPerRecord", "profileId", "selectionRevision");

    AndroidSecureSpeakerLaunch {
        uuid(routeId); uuid(sessionId); uuid(peerNodeId);
        if (!DIRECTION.equals(direction) || profile == null || !profile.secure()
                || settingsRevision <= 0 || settingsRevision > 9_007_199_254_740_991L || expiresAtMs <= 0) {
            throw new IllegalArgumentException("Invalid secure speaker binding");
        }
    }
    static AndroidSecureSpeakerLaunch admit(Map<String, Object> config, String routeId,
            String sessionId, String peerNodeId, String direction, long expiresAtMs,
            AndroidSpeakerSettings.Selection selection, long nowMs) {
        if (selection == null || !selection.profile().secure()
                || nowMs <= 0 || expiresAtMs <= nowMs
                || config == null || !config.keySet().equals(FIELDS)
                || !integer(config.get("schemaVersion"), 3)
                || !TRANSPORT.equals(config.get("transport"))
                || !selection.profile().format.equals(config.get("format"))
                || !selection.profile().id.equals(config.get("profileId"))
                || !Long.toString(selection.revision()).equals(config.get("selectionRevision"))
                || !integer(config.get("pcmBytesPerRecord"), 1920)
                || !integer(config.get("sampleRateHz"), 48_000)
                || !integer(config.get("channels"), 2)
                || !integer(config.get("sampleFramesPerRecord"), 480)) {
            throw new IllegalArgumentException("Invalid secure stereo audio handoff");
        }
        // expiresAtMs is the original Route grant, not the playback duration.
        // Native profile/Source lease clocks may end playback sooner; never
        // infer a profile, extend authority, or allocate from this timestamp.
        return new AndroidSecureSpeakerLaunch(routeId, sessionId, peerNodeId, direction,
                selection.profile(), selection.revision(), expiresAtMs);
    }
    boolean matches(AndroidSpeakerSettings settings) {
        return settings.isCurrent(profile, settingsRevision);
    }
    private static boolean integer(Object value, int expected) {
        return (value instanceof Integer || value instanceof Long) && ((Number) value).longValue() == expected;
    }
    private static void uuid(String value) {
        if (value == null || value.length() != 36 || !UUID.fromString(value).toString().equals(value)) {
            throw new IllegalArgumentException("Noncanonical secure speaker identity");
        }
    }
}
