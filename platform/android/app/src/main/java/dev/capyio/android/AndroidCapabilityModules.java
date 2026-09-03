package dev.capyio.android;

import dev.capyio.android.contract.AudioCapabilityKind;
import dev.capyio.android.contract.AudioCapabilitySnapshot;
import dev.capyio.android.contract.AudioCapabilityState;
import dev.capyio.android.contract.AudioNodeController;
import dev.capyio.android.contract.AudioNodeSnapshot;
import dev.capyio.android.contract.CapabilityAvailability;
import dev.capyio.android.contract.CapabilityKind;
import dev.capyio.android.contract.CapabilityModule;
import dev.capyio.android.contract.CapabilityModuleRegistry;
import dev.capyio.android.contract.CapabilityPermissionRequirement;
import dev.capyio.android.contract.CapabilityPortDirection;
import dev.capyio.android.contract.CapabilityPortSnapshot;
import dev.capyio.android.contract.CapabilitySnapshot;
import dev.capyio.android.contract.HardwarePresence;

import java.util.List;
import java.util.function.BooleanSupplier;

/** Builds the non-display module inventory owned by the Android Node service. */
final class AndroidCapabilityModules {
    private static final String MODULE_NOT_ATTACHED = "CAPY.ANDROID.MODULE_NOT_ATTACHED";
    private static final String HARDWARE_NOT_FOUND = "CAPY.ANDROID.HARDWARE_NOT_FOUND";
    private static final String HARDWARE_PROBE_FAILED = "CAPY.ANDROID.HARDWARE_PROBE_FAILED";

    private AndroidCapabilityModules() {}

    static CapabilityModuleRegistry createRegistry(
            AudioNodeController audioController,
            BooleanSupplier microphonePermissionReady,
            BooleanSupplier notificationPermissionReady,
            HardwarePresence microphonePresence,
            HardwarePresence speakerPresence,
            HardwarePresence cameraPresence,
            HardwarePresence imuPresence,
            HardwarePresence touchSurfacePresence,
            HardwarePresence vibratorPresence) {
        CapabilityModuleRegistry registry = new CapabilityModuleRegistry();
        registry.register(new AudioModule(
                audioController,
                microphonePermissionReady,
                notificationPermissionReady,
                microphonePresence,
                speakerPresence));
        registry.register(pendingModule("android.camera-module", List.of(pendingCapability(
                "android.camera",
                "Phone Camera",
                CapabilityKind.CAMERA,
                CapabilityPermissionRequirement.USER_CONFIRMATION,
                cameraPresence,
                "android.camera.source",
                "Camera Frame Source",
                CapabilityPortDirection.SOURCE,
                "capyio.video.frames/1",
                "platform-negotiated-video-frame"))));
        registry.register(pendingModule("android.controller-module", List.of(
                pendingCapability(
                        "android.imu",
                        "Phone IMU",
                        CapabilityKind.IMU,
                        CapabilityPermissionRequirement.NONE,
                        imuPresence,
                        "android.imu.source",
                        "IMU Sample Source",
                        CapabilityPortDirection.SOURCE,
                        "capyio.motion.imu-samples/1",
                        "imu-si-f32-le"),
                pendingCapability(
                        "android.gamepad",
                        "Touch Gamepad",
                        CapabilityKind.GAMEPAD,
                        CapabilityPermissionRequirement.NONE,
                        touchSurfacePresence,
                        "android.gamepad.source",
                        "Gamepad State Source",
                        CapabilityPortDirection.SOURCE,
                        "capyio.input.gamepad-state/1",
                        "gamepad-state-v1"),
                pendingCapability(
                        "android.haptics",
                        "Phone Vibration",
                        CapabilityKind.HAPTICS,
                        CapabilityPermissionRequirement.NONE,
                        vibratorPresence,
                        "android.haptics.sink",
                        "Haptics Feedback Sink",
                        CapabilityPortDirection.SINK,
                        "capyio.haptics.feedback/1",
                        "dual-rumble-v1"))));
        registry.register(pendingModule("android.touchpad-module", List.of(pendingCapability(
                "android.touchpad",
                "Phone Touchpad Surface",
                CapabilityKind.TOUCHPAD,
                CapabilityPermissionRequirement.NONE,
                touchSurfacePresence,
                "android.touchpad.source",
                "Touchpad Frame Source",
                CapabilityPortDirection.SOURCE,
                "capyio.input.touchpad-frames/1",
                "touchpad-frame-v1"))));
        return registry;
    }

    private static CapabilityModule pendingModule(
            String moduleKey,
            List<CapabilitySnapshot> capabilities) {
        return new CapabilityModule() {
            @Override
            public String moduleKey() {
                return moduleKey;
            }

            @Override
            public List<CapabilitySnapshot> capabilities() {
                return capabilities;
            }
        };
    }

    private static CapabilitySnapshot pendingCapability(
            String capabilityKey,
            String displayName,
            CapabilityKind kind,
            CapabilityPermissionRequirement permissionRequirement,
            HardwarePresence hardwarePresence,
            String portKey,
            String portDisplayName,
            CapabilityPortDirection direction,
            String profile,
            String format) {
        return new CapabilitySnapshot(
                capabilityKey,
                displayName,
                kind,
                CapabilityAvailability.OFFLINE,
                permissionRequirement,
                hardwarePresence,
                false,
                List.of(new CapabilityPortSnapshot(
                        portKey,
                        portDisplayName,
                        direction,
                        profile,
                        List.of(format))),
                inventoryProblem(hardwarePresence));
    }

    private static final class AudioModule implements CapabilityModule {
        private final AudioNodeController controller;
        private final BooleanSupplier microphonePermissionReady;
        private final BooleanSupplier notificationPermissionReady;
        private final HardwarePresence microphonePresence;
        private final HardwarePresence speakerPresence;

        private AudioModule(
                AudioNodeController controller,
                BooleanSupplier microphonePermissionReady,
                BooleanSupplier notificationPermissionReady,
                HardwarePresence microphonePresence,
                HardwarePresence speakerPresence) {
            this.controller = controller;
            this.microphonePermissionReady = microphonePermissionReady;
            this.notificationPermissionReady = notificationPermissionReady;
            this.microphonePresence = microphonePresence;
            this.speakerPresence = speakerPresence;
        }

        @Override
        public String moduleKey() {
            return "android.audio-module";
        }

        @Override
        public List<CapabilitySnapshot> capabilities() {
            AudioNodeSnapshot snapshot = controller.snapshot("capability-catalog");
            boolean notificationReady = notificationPermissionReady.getAsBoolean();
            boolean microphoneReady = microphonePermissionReady.getAsBoolean()
                    && notificationReady;
            return List.of(
                    audioCapability(snapshot.microphone(), microphoneReady, microphonePresence),
                    audioCapability(snapshot.speaker(), notificationReady, speakerPresence));
        }

        private static CapabilitySnapshot audioCapability(
                AudioCapabilitySnapshot audio,
                boolean permissionReady,
                HardwarePresence hardwarePresence) {
            boolean microphone = audio.kind() == AudioCapabilityKind.MICROPHONE_SOURCE;
            return new CapabilitySnapshot(
                    microphone ? "android.microphone" : "android.speaker",
                    microphone ? "Phone Microphone" : "Phone Speaker",
                    microphone ? CapabilityKind.MICROPHONE : CapabilityKind.SPEAKER,
                    availability(audio.state(), permissionReady, hardwarePresence),
                    microphone
                            ? CapabilityPermissionRequirement.USER_CONFIRMATION
                            : CapabilityPermissionRequirement.FOREGROUND_SERVICE,
                    hardwarePresence,
                    true,
                    List.of(new CapabilityPortSnapshot(
                            audio.kind().portId(),
                            microphone ? "Microphone Source" : "Speaker Sink",
                            microphone
                                    ? CapabilityPortDirection.SOURCE
                                    : CapabilityPortDirection.SINK,
                            AudioCapabilityKind.PROFILE_ID,
                            List.of(microphone
                                    ? "pcm-s16le-48000-mono"
                                    : "pcm-s16le-48000-stereo"))),
                    audio.problemCode() == null && hardwarePresence != HardwarePresence.PRESENT
                            ? inventoryProblem(hardwarePresence)
                            : audio.problemCode());
        }

        private static CapabilityAvailability availability(
                AudioCapabilityState state,
                boolean permissionReady,
                HardwarePresence hardwarePresence) {
            if (hardwarePresence != HardwarePresence.PRESENT) {
                return CapabilityAvailability.OFFLINE;
            }
            if (state == AudioCapabilityState.FAILED) {
                return CapabilityAvailability.OFFLINE;
            }
            if (state.ownsForegroundLifecycle()) {
                return CapabilityAvailability.BUSY;
            }
            return permissionReady
                    ? CapabilityAvailability.AVAILABLE
                    : CapabilityAvailability.PERMISSION_REQUIRED;
        }
    }

    private static String inventoryProblem(HardwarePresence presence) {
        return switch (presence) {
            case PRESENT -> MODULE_NOT_ATTACHED;
            case ABSENT -> HARDWARE_NOT_FOUND;
            case UNKNOWN -> HARDWARE_PROBE_FAILED;
        };
    }
}
