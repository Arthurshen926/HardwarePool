package dev.capyio.android.contract;

/** Versioned Android-local DTO; it is not a Rust memory layout or network wire type. */
public final class AudioNodeSnapshot {
    public static final int SCHEMA_VERSION = 1;

    private final String nodeId;
    private final AudioCapabilitySnapshot microphone;
    private final AudioCapabilitySnapshot speaker;

    AudioNodeSnapshot(
            String nodeId,
            AudioCapabilitySnapshot microphone,
            AudioCapabilitySnapshot speaker) {
        if (nodeId == null || nodeId.isEmpty() || nodeId.length() > 128) {
            throw new IllegalArgumentException("node ID is outside the supported bound");
        }
        this.nodeId = nodeId;
        this.microphone = microphone;
        this.speaker = speaker;
    }

    public int schemaVersion() {
        return SCHEMA_VERSION;
    }

    public String nodeId() {
        return nodeId;
    }

    public AudioCapabilitySnapshot microphone() {
        return microphone;
    }

    public AudioCapabilitySnapshot speaker() {
        return speaker;
    }

    public AudioCapabilitySnapshot capability(AudioCapabilityKind kind) {
        return kind == AudioCapabilityKind.MICROPHONE_SOURCE ? microphone : speaker;
    }
}
