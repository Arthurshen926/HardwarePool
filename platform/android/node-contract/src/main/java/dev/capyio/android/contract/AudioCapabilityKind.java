package dev.capyio.android.contract;

/** Direction belongs to each Port; the Android Node has no global audio role. */
public enum AudioCapabilityKind {
    MICROPHONE_SOURCE("android.microphone.source", "Source"),
    SPEAKER_SINK("android.speaker.sink", "Sink");

    public static final String PROFILE_ID = "capyio.audio.frames/1";

    private final String portId;
    private final String direction;

    AudioCapabilityKind(String portId, String direction) {
        this.portId = portId;
        this.direction = direction;
    }

    public String portId() {
        return portId;
    }

    public String direction() {
        return direction;
    }
}
