package dev.capyio.android.contract;

public final class AudioNodeContractTest {
    private static int assertions;

    private AudioNodeContractTest() {}

    public static void main(String[] args) {
        declaresDirectionOnPorts();
        controlsBothCapabilitiesIndependently();
        rejectsStaleCompletions();
        isolatesFailureAndRetry();
        validatesActualFormatAndProblemBounds();
        System.out.println("Android audio node contract: PASS (" + assertions + " assertions)");
    }

    private static void declaresDirectionOnPorts() {
        check("Source".equals(AudioCapabilityKind.MICROPHONE_SOURCE.direction()));
        check("Sink".equals(AudioCapabilityKind.SPEAKER_SINK.direction()));
        check(!AudioCapabilityKind.MICROPHONE_SOURCE.portId()
                .equals(AudioCapabilityKind.SPEAKER_SINK.portId()));
        check("capyio.audio.frames/1".equals(AudioCapabilityKind.PROFILE_ID));
    }

    private static void controlsBothCapabilitiesIndependently() {
        AudioNodeController controller = new AudioNodeController();
        ActualAudioFormat microphoneFormat = new ActualAudioFormat(48_000, 1, "pcm_s16le", 480);
        ActualAudioFormat speakerFormat = new ActualAudioFormat(48_000, 2, "pcm_s16le", 480);

        TransitionToken microphoneStart = controller.beginStart(AudioCapabilityKind.MICROPHONE_SOURCE);
        TransitionToken speakerStart = controller.beginStart(AudioCapabilityKind.SPEAKER_SINK);
        check(microphoneStart.accepted());
        check(speakerStart.accepted());
        check(controller.completeStart(
                AudioCapabilityKind.MICROPHONE_SOURCE,
                microphoneStart.generation(),
                microphoneFormat));
        check(controller.completeStart(
                AudioCapabilityKind.SPEAKER_SINK,
                speakerStart.generation(),
                speakerFormat));

        controller.addFrames(
                AudioCapabilityKind.MICROPHONE_SOURCE,
                microphoneStart.generation(),
                480);
        AudioNodeSnapshot active = controller.snapshot("test-node");
        check(active.schemaVersion() == 1);
        check(active.microphone().state() == AudioCapabilityState.ACTIVE);
        check(active.speaker().state() == AudioCapabilityState.ACTIVE);
        check(active.microphone().framesProcessed() == 480);
        check(active.speaker().framesProcessed() == 0);

        TransitionToken speakerStop = controller.beginStop(AudioCapabilityKind.SPEAKER_SINK);
        check(speakerStop.accepted());
        check(controller.completeStop(AudioCapabilityKind.SPEAKER_SINK, speakerStop.generation()));
        AudioNodeSnapshot afterSpeakerStop = controller.snapshot("test-node");
        check(afterSpeakerStop.microphone().state() == AudioCapabilityState.ACTIVE);
        check(afterSpeakerStop.speaker().state() == AudioCapabilityState.STOPPED);
    }

    private static void rejectsStaleCompletions() {
        AudioNodeController controller = new AudioNodeController();
        TransitionToken start = controller.beginStart(AudioCapabilityKind.MICROPHONE_SOURCE);
        TransitionToken stop = controller.beginStop(AudioCapabilityKind.MICROPHONE_SOURCE);
        check(start.accepted());
        check(stop.accepted());
        check(!controller.completeStart(
                AudioCapabilityKind.MICROPHONE_SOURCE,
                start.generation(),
                new ActualAudioFormat(48_000, 1, "pcm_s16le", 480)));
        check(controller.completeStop(AudioCapabilityKind.MICROPHONE_SOURCE, stop.generation()));
        check(controller.snapshot("test-node").microphone().state()
                == AudioCapabilityState.STOPPED);
    }

    private static void isolatesFailureAndRetry() {
        AudioNodeController controller = new AudioNodeController();
        TransitionToken microphone = controller.beginStart(AudioCapabilityKind.MICROPHONE_SOURCE);
        TransitionToken speaker = controller.beginStart(AudioCapabilityKind.SPEAKER_SINK);
        check(controller.completeStart(
                AudioCapabilityKind.SPEAKER_SINK,
                speaker.generation(),
                new ActualAudioFormat(48_000, 2, "pcm_s16le", 480)));
        check(controller.fail(
                AudioCapabilityKind.MICROPHONE_SOURCE,
                microphone.generation(),
                "CAPY.ANDROID.MIC_START_FAILED"));

        AudioNodeSnapshot failed = controller.snapshot("test-node");
        check(failed.microphone().state() == AudioCapabilityState.FAILED);
        check(failed.speaker().state() == AudioCapabilityState.ACTIVE);
        check("CAPY.ANDROID.MIC_START_FAILED".equals(failed.microphone().problemCode()));

        TransitionToken retry = controller.beginStart(AudioCapabilityKind.MICROPHONE_SOURCE);
        check(retry.accepted());
        check(retry.generation() > microphone.generation());
        check(controller.completeStart(
                AudioCapabilityKind.MICROPHONE_SOURCE,
                retry.generation(),
                new ActualAudioFormat(48_000, 1, "pcm_s16le", 480)));
        check(controller.snapshot("test-node").speaker().state()
                == AudioCapabilityState.ACTIVE);
    }

    private static void validatesActualFormatAndProblemBounds() {
        expectFailure(() -> new ActualAudioFormat(1_000, 1, "pcm_s16le", 480));
        expectFailure(() -> new ActualAudioFormat(48_000, 0, "pcm_s16le", 480));
        expectFailure(() -> new ActualAudioFormat(48_000, 1, "PCM 16", 480));

        AudioNodeController controller = new AudioNodeController();
        TransitionToken start = controller.beginStart(AudioCapabilityKind.MICROPHONE_SOURCE);
        expectFailure(() -> controller.fail(
                AudioCapabilityKind.MICROPHONE_SOURCE,
                start.generation(),
                "contains spaces"));
        check(controller.snapshot("test-node").microphone().state()
                == AudioCapabilityState.STARTING);
    }

    private static void expectFailure(Runnable operation) {
        boolean failed = false;
        try {
            operation.run();
        } catch (IllegalArgumentException expected) {
            failed = true;
        }
        check(failed);
    }

    private static void check(boolean condition) {
        assertions++;
        if (!condition) {
            throw new AssertionError("contract assertion " + assertions + " failed");
        }
    }
}
