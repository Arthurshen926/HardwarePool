package dev.capyio.android.contract;

/** Bounded, payload-free projection consumed by the Android Activity. */
public final class AudioCapabilitySnapshot {
    private final AudioCapabilityKind kind;
    private final AudioCapabilityState state;
    private final long generation;
    private final ActualAudioFormat actualFormat;
    private final long framesProcessed;
    private final String problemCode;

    AudioCapabilitySnapshot(
            AudioCapabilityKind kind,
            AudioCapabilityState state,
            long generation,
            ActualAudioFormat actualFormat,
            long framesProcessed,
            String problemCode) {
        this.kind = kind;
        this.state = state;
        this.generation = generation;
        this.actualFormat = actualFormat;
        this.framesProcessed = framesProcessed;
        this.problemCode = problemCode;
    }

    public AudioCapabilityKind kind() {
        return kind;
    }

    public AudioCapabilityState state() {
        return state;
    }

    public long generation() {
        return generation;
    }

    public ActualAudioFormat actualFormat() {
        return actualFormat;
    }

    public long framesProcessed() {
        return framesProcessed;
    }

    public String problemCode() {
        return problemCode;
    }
}
