package dev.capyio.android.contract;

import java.util.EnumMap;
import java.util.Map;

/**
 * Hardware-free lifecycle authority used by the Android service.
 *
 * <p>Every completion is generation-bound, so a late platform callback cannot
 * reactivate a capability after Stop or Retry. Microphone and speaker slots are
 * mutated independently.</p>
 */
public final class AudioNodeController {
    private static final int MAX_PROBLEM_CODE_LENGTH = 96;

    private final Map<AudioCapabilityKind, Slot> slots =
            new EnumMap<>(AudioCapabilityKind.class);

    public AudioNodeController() {
        for (AudioCapabilityKind kind : AudioCapabilityKind.values()) {
            slots.put(kind, new Slot());
        }
    }

    public synchronized TransitionToken beginStart(AudioCapabilityKind kind) {
        Slot slot = slot(kind);
        if (slot.state != AudioCapabilityState.STOPPED
                && slot.state != AudioCapabilityState.FAILED) {
            return new TransitionToken(false, slot.generation);
        }
        slot.generation = nextGeneration(slot.generation);
        slot.state = AudioCapabilityState.STARTING;
        slot.actualFormat = null;
        slot.framesProcessed = 0;
        slot.problemCode = null;
        return new TransitionToken(true, slot.generation);
    }

    public synchronized boolean completeStart(
            AudioCapabilityKind kind,
            long generation,
            ActualAudioFormat actualFormat) {
        if (actualFormat == null) {
            throw new IllegalArgumentException("actual format is required");
        }
        Slot slot = slot(kind);
        if (slot.generation != generation || slot.state != AudioCapabilityState.STARTING) {
            return false;
        }
        slot.actualFormat = actualFormat;
        slot.state = AudioCapabilityState.ACTIVE;
        return true;
    }

    public synchronized TransitionToken beginStop(AudioCapabilityKind kind) {
        Slot slot = slot(kind);
        if (slot.state == AudioCapabilityState.STOPPED
                || slot.state == AudioCapabilityState.STOPPING) {
            return new TransitionToken(false, slot.generation);
        }
        slot.generation = nextGeneration(slot.generation);
        slot.state = AudioCapabilityState.STOPPING;
        slot.problemCode = null;
        return new TransitionToken(true, slot.generation);
    }

    public synchronized boolean completeStop(AudioCapabilityKind kind, long generation) {
        Slot slot = slot(kind);
        if (slot.generation != generation || slot.state != AudioCapabilityState.STOPPING) {
            return false;
        }
        slot.state = AudioCapabilityState.STOPPED;
        slot.actualFormat = null;
        slot.framesProcessed = 0;
        slot.problemCode = null;
        return true;
    }

    public synchronized boolean fail(
            AudioCapabilityKind kind,
            long generation,
            String problemCode) {
        validateProblemCode(problemCode);
        Slot slot = slot(kind);
        if (slot.generation != generation
                || (slot.state != AudioCapabilityState.STARTING
                        && slot.state != AudioCapabilityState.ACTIVE)) {
            return false;
        }
        slot.state = AudioCapabilityState.FAILED;
        slot.problemCode = problemCode;
        return true;
    }

    public synchronized void addFrames(
            AudioCapabilityKind kind,
            long generation,
            long frames) {
        if (frames < 1) {
            throw new IllegalArgumentException("frame increment must be positive");
        }
        Slot slot = slot(kind);
        if (slot.generation != generation || slot.state != AudioCapabilityState.ACTIVE) {
            return;
        }
        if (Long.MAX_VALUE - slot.framesProcessed < frames) {
            slot.framesProcessed = Long.MAX_VALUE;
        } else {
            slot.framesProcessed += frames;
        }
    }

    public synchronized AudioNodeSnapshot snapshot(String nodeId) {
        return new AudioNodeSnapshot(
                nodeId,
                snapshotSlot(AudioCapabilityKind.MICROPHONE_SOURCE),
                snapshotSlot(AudioCapabilityKind.SPEAKER_SINK));
    }

    private AudioCapabilitySnapshot snapshotSlot(AudioCapabilityKind kind) {
        Slot slot = slot(kind);
        return new AudioCapabilitySnapshot(
                kind,
                slot.state,
                slot.generation,
                slot.actualFormat,
                slot.framesProcessed,
                slot.problemCode);
    }

    private Slot slot(AudioCapabilityKind kind) {
        if (kind == null) {
            throw new IllegalArgumentException("capability kind is required");
        }
        return slots.get(kind);
    }

    private static long nextGeneration(long generation) {
        if (generation == Long.MAX_VALUE) {
            throw new IllegalStateException("capability generation exhausted");
        }
        return generation + 1;
    }

    private static void validateProblemCode(String value) {
        if (value == null || value.isEmpty() || value.length() > MAX_PROBLEM_CODE_LENGTH) {
            throw new IllegalArgumentException("problem code is outside the supported bound");
        }
        for (int index = 0; index < value.length(); index++) {
            char character = value.charAt(index);
            boolean valid = (character >= 'A' && character <= 'Z')
                    || (character >= '0' && character <= '9')
                    || character == '.'
                    || character == '_';
            if (!valid) {
                throw new IllegalArgumentException("problem code must be canonical ASCII");
            }
        }
    }

    private static final class Slot {
        private AudioCapabilityState state = AudioCapabilityState.STOPPED;
        private long generation;
        private ActualAudioFormat actualFormat;
        private long framesProcessed;
        private String problemCode;
    }
}
