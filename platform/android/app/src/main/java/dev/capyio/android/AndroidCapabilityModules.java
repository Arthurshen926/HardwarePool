package dev.capyio.android;

import dev.capyio.android.contract.AudioCapabilityKind;
import dev.capyio.android.contract.AudioCapabilitySnapshot;
import dev.capyio.android.contract.AudioCapabilityState;
import dev.capyio.android.contract.AudioNodeController;
import dev.capyio.android.contract.AudioNodeSnapshot;
import dev.capyio.android.contract.CapabilityAvailability;
import dev.capyio.android.contract.CapabilityKind;
import dev.capyio.android.contract.CapabilityLifecycleController;
import dev.capyio.android.contract.CapabilityLifecycleSnapshot;
import dev.capyio.android.contract.CapabilityLifecycleState;
import dev.capyio.android.contract.CapabilityModule;
import dev.capyio.android.contract.CapabilityModuleRegistry;
import dev.capyio.android.contract.CapabilityPermissionRequirement;
import dev.capyio.android.contract.CapabilityPortDirection;
import dev.capyio.android.contract.CapabilityPortSnapshot;
import dev.capyio.android.contract.CapabilitySnapshot;
import dev.capyio.android.contract.GamepadNodeController;
import dev.capyio.android.contract.HardwarePresence;
import dev.capyio.android.contract.TouchpadNodeController;

import java.util.List;
import java.util.function.BooleanSupplier;
import java.util.function.Supplier;

/** Builds the non-display module inventory owned by the Android Node service. */
final class AndroidCapabilityModules {
    private static final String MODULE_NOT_ATTACHED = "CAPY.ANDROID.MODULE_NOT_ATTACHED";
    private static final String HARDWARE_NOT_FOUND = "CAPY.ANDROID.HARDWARE_NOT_FOUND";
    private static final String HARDWARE_PROBE_FAILED = "CAPY.ANDROID.HARDWARE_PROBE_FAILED";

    private AndroidCapabilityModules() {}

    static CapabilityModuleRegistry createRegistry(
            AudioNodeController audioController,
            CapabilityLifecycleController imuController,
            GamepadNodeController gamepadController,
            TouchpadNodeController touchpadController,
            BooleanSupplier microphonePermissionReady,
            BooleanSupplier notificationPermissionReady,
            HardwarePresence microphonePresence,
            HardwarePresence speakerPresence,
            HardwarePresence cameraPresence,
            HardwarePresence imuPresence,
            HardwarePresence touchSurfacePresence,
            HardwarePresence vibratorPresence) {
        return createRegistry(
                audioController,
                imuController,
                gamepadController,
                touchpadController,
                new CapabilityLifecycleController(),
                microphonePermissionReady,
                notificationPermissionReady,
                () -> new CapabilityLifecycleSnapshot(
                        CapabilityLifecycleState.STOPPED, 0, null),
                () -> false,
                false,
                false,
                microphonePresence,
                speakerPresence,
                cameraPresence,
                imuPresence,
                touchSurfacePresence,
                vibratorPresence);
    }

    static CapabilityModuleRegistry createRegistry(
            AudioNodeController audioController,
            CapabilityLifecycleController imuController,
            GamepadNodeController gamepadController,
            TouchpadNodeController touchpadController,
            CapabilityLifecycleController hapticsController,
            BooleanSupplier microphonePermissionReady,
            BooleanSupplier notificationPermissionReady,
            Supplier<CapabilityLifecycleSnapshot> cameraLifecycle,
            BooleanSupplier cameraPermissionReady,
            boolean cameraAdapterAttached,
            boolean hapticsAdapterAttached,
            HardwarePresence microphonePresence,
            HardwarePresence speakerPresence,
            HardwarePresence cameraPresence,
            HardwarePresence imuPresence,
            HardwarePresence touchSurfacePresence,
            HardwarePresence vibratorPresence) {
        return createRegistry(audioController, imuController, gamepadController, touchpadController,
                hapticsController, microphonePermissionReady, notificationPermissionReady,
                cameraLifecycle, cameraPermissionReady,
                () -> List.of("cavc-avc-access-unit-v1"), cameraAdapterAttached, hapticsAdapterAttached,
                microphonePresence, speakerPresence, cameraPresence, imuPresence,
                touchSurfacePresence, vibratorPresence);
    }

    static CapabilityModuleRegistry createRegistry(
            AudioNodeController audioController,
            CapabilityLifecycleController imuController,
            GamepadNodeController gamepadController,
            TouchpadNodeController touchpadController,
            CapabilityLifecycleController hapticsController,
            BooleanSupplier microphonePermissionReady,
            BooleanSupplier notificationPermissionReady,
            Supplier<CapabilityLifecycleSnapshot> cameraLifecycle,
            BooleanSupplier cameraPermissionReady,
            Supplier<List<String>> cameraFormats,
            boolean cameraAdapterAttached,
            boolean hapticsAdapterAttached,
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
        registry.register(new CapabilityModule() {
            @Override
            public String moduleKey() {
                return "android.camera-module";
            }

            @Override
            public List<CapabilitySnapshot> capabilities() {
                if (!cameraAdapterAttached) {
                    return List.of(pendingCapability(
                            "android.camera",
                            "Phone Camera",
                            CapabilityKind.CAMERA,
                            CapabilityPermissionRequirement.USER_CONFIRMATION,
                            cameraPresence,
                            "android.camera.source",
                            "Camera Frame Source",
                            CapabilityPortDirection.SOURCE,
                            "capyio.video.frames/1",
                            "platform-negotiated-video-frame"));
                }
                List<String> formats = cameraFormats.get();
                if (!formats.isEmpty() && !formats.equals(List.of("cavc-avc-access-unit-v1"))
                        && !formats.equals(List.of("cavc-avc-finite-720p30-v1"))) {
                    throw new IllegalArgumentException("camera catalog requires one exact selected profile");
                }
                return List.of(cameraCapability(
                        cameraLifecycle.get(), cameraPermissionReady.getAsBoolean(), cameraPresence, formats));
            }
        });
        registry.register(controllerModule(
                imuController,
                gamepadController,
                hapticsController,
                imuPresence,
                touchSurfacePresence,
                vibratorPresence,
                hapticsAdapterAttached));
        registry.register(new CapabilityModule() {
            @Override
            public String moduleKey() {
                return "android.touchpad-module";
            }

            @Override
            public List<CapabilitySnapshot> capabilities() {
                return List.of(touchpadCapability(
                        touchpadController.lifecycle(), touchSurfacePresence));
            }
        });
        return registry;
    }

    private static CapabilitySnapshot cameraCapability(
            CapabilityLifecycleSnapshot lifecycle,
            boolean permissionReady,
            HardwarePresence hardwarePresence,
            List<String> formats) {
        CapabilityAvailability availability;
        String problemCode = lifecycle.problemCode();
        if (hardwarePresence != HardwarePresence.PRESENT) {
            availability = CapabilityAvailability.OFFLINE;
            problemCode = inventoryProblem(hardwarePresence);
        } else if (formats.isEmpty()) {
            // Keep the required local inventory entry, but do not make it shareable or
            // describe unsupported capture as either a finite or legacy AVC Source.
            availability = CapabilityAvailability.OFFLINE;
            problemCode = "CAPY.ANDROID.CAMERA_FINITE_PREFLIGHT_REQUIRED";
        } else if (lifecycle.state() == CapabilityLifecycleState.FAILED) {
            availability = CapabilityAvailability.OFFLINE;
        } else if (lifecycle.state().ownsAdapterLifecycle()) {
            availability = CapabilityAvailability.BUSY;
        } else {
            availability = permissionReady
                    ? CapabilityAvailability.AVAILABLE
                    : CapabilityAvailability.PERMISSION_REQUIRED;
        }
        return new CapabilitySnapshot(
                "android.camera",
                "Phone Camera",
                CapabilityKind.CAMERA,
                availability,
                CapabilityPermissionRequirement.USER_CONFIRMATION,
                hardwarePresence,
                true,
                List.of(new CapabilityPortSnapshot(
                        "android.camera.source",
                        "Camera Frame Source",
                        CapabilityPortDirection.SOURCE,
                        "capyio.video.frames/1",
                        formats.isEmpty() ? List.of("platform-negotiated-video-frame") : formats)),
                problemCode);
    }

    private static CapabilityModule controllerModule(
            CapabilityLifecycleController imuController,
            GamepadNodeController gamepadController,
            CapabilityLifecycleController hapticsController,
            HardwarePresence imuPresence,
            HardwarePresence touchSurfacePresence,
            HardwarePresence vibratorPresence,
            boolean hapticsAdapterAttached) {
        return new CapabilityModule() {
            @Override
            public String moduleKey() {
                return "android.controller-module";
            }

            @Override
            public List<CapabilitySnapshot> capabilities() {
                return List.of(
                        imuCapability(imuController.snapshot(), imuPresence),
                        gamepadCapability(gamepadController.lifecycle(), touchSurfacePresence),
                        hapticsAdapterAttached
                                ? hapticsCapability(hapticsController.snapshot(), vibratorPresence)
                                : pendingCapability(
                                        "android.haptics",
                                        "Phone Vibration",
                                        CapabilityKind.HAPTICS,
                                        CapabilityPermissionRequirement.NONE,
                                        vibratorPresence,
                                        "android.haptics.sink",
                                        "Haptics Feedback Sink",
                                        CapabilityPortDirection.SINK,
                                        "capyio.haptics.feedback/1",
                                        "dual-rumble-v1"));
            }
        };
    }

    private static CapabilitySnapshot hapticsCapability(
            CapabilityLifecycleSnapshot lifecycle,
            HardwarePresence hardwarePresence) {
        CapabilityAvailability availability;
        String problemCode = lifecycle.problemCode();
        if (hardwarePresence != HardwarePresence.PRESENT) {
            availability = CapabilityAvailability.OFFLINE;
            problemCode = inventoryProblem(hardwarePresence);
        } else if (lifecycle.state() == CapabilityLifecycleState.FAILED) {
            availability = CapabilityAvailability.OFFLINE;
        } else {
            availability = lifecycle.state().ownsAdapterLifecycle()
                    ? CapabilityAvailability.BUSY
                    : CapabilityAvailability.AVAILABLE;
        }
        return new CapabilitySnapshot(
                "android.haptics",
                "Phone Vibration",
                  CapabilityKind.HAPTICS,
                  availability,
                  CapabilityPermissionRequirement.USER_CONFIRMATION,
                hardwarePresence,
                true,
                List.of(new CapabilityPortSnapshot(
                        "android.haptics.sink",
                        "Haptics Feedback Sink",
                        CapabilityPortDirection.SINK,
                        "capyio.haptics.feedback/1",
                        List.of("dual-rumble-v1"))),
                problemCode);
    }

    private static CapabilitySnapshot imuCapability(
            CapabilityLifecycleSnapshot lifecycle,
            HardwarePresence hardwarePresence) {
        CapabilityAvailability availability;
        String problemCode = null;
        if (hardwarePresence != HardwarePresence.PRESENT) {
            availability = CapabilityAvailability.OFFLINE;
            problemCode = inventoryProblem(hardwarePresence);
        } else if (lifecycle.state() == CapabilityLifecycleState.FAILED) {
            availability = CapabilityAvailability.OFFLINE;
            problemCode = lifecycle.problemCode();
        } else {
            availability = lifecycle.state().ownsAdapterLifecycle()
                    ? CapabilityAvailability.BUSY
                    : CapabilityAvailability.AVAILABLE;
        }
        return new CapabilitySnapshot(
                "android.imu",
                "Phone IMU",
                CapabilityKind.IMU,
                availability,
                CapabilityPermissionRequirement.NONE,
                hardwarePresence,
                true,
                List.of(new CapabilityPortSnapshot(
                        "android.imu.source",
                        "IMU Sample Source",
                        CapabilityPortDirection.SOURCE,
                        "capyio.motion.imu-samples/1",
                        List.of("imu-si-f32-le"))),
                problemCode);
    }

    private static CapabilitySnapshot gamepadCapability(
            CapabilityLifecycleSnapshot lifecycle,
            HardwarePresence hardwarePresence) {
        CapabilityAvailability availability;
        String problemCode = null;
        if (hardwarePresence != HardwarePresence.PRESENT) {
            availability = CapabilityAvailability.OFFLINE;
            problemCode = inventoryProblem(hardwarePresence);
        } else if (lifecycle.state() == CapabilityLifecycleState.FAILED) {
            availability = CapabilityAvailability.OFFLINE;
            problemCode = lifecycle.problemCode();
        } else {
            availability = lifecycle.state().ownsAdapterLifecycle()
                    ? CapabilityAvailability.BUSY
                    : CapabilityAvailability.AVAILABLE;
        }
        return new CapabilitySnapshot(
                "android.gamepad",
                "Touch Gamepad",
                CapabilityKind.GAMEPAD,
                availability,
                CapabilityPermissionRequirement.NONE,
                hardwarePresence,
                true,
                List.of(new CapabilityPortSnapshot(
                        "android.gamepad.source",
                        "Gamepad State Source",
                        CapabilityPortDirection.SOURCE,
                        "capyio.input.gamepad-state/1",
                        List.of("gamepad-state-v1"))),
                problemCode);
    }

    private static CapabilitySnapshot touchpadCapability(
            CapabilityLifecycleSnapshot lifecycle,
            HardwarePresence hardwarePresence) {
        CapabilityAvailability availability;
        String problemCode = null;
        if (hardwarePresence != HardwarePresence.PRESENT) {
            availability = CapabilityAvailability.OFFLINE;
            problemCode = inventoryProblem(hardwarePresence);
        } else if (lifecycle.state() == CapabilityLifecycleState.FAILED) {
            // A failed data Route does not remove the local touchscreen.
            // Keep the Source publishable so a fresh authorized Route can retry.
            availability = CapabilityAvailability.AVAILABLE;
            problemCode = lifecycle.problemCode();
        } else {
            availability = lifecycle.state().ownsAdapterLifecycle()
                    ? CapabilityAvailability.BUSY
                    : CapabilityAvailability.AVAILABLE;
        }
        return new CapabilitySnapshot(
                "android.touchpad",
                "Phone Touchpad Surface",
                CapabilityKind.TOUCHPAD,
                availability,
                CapabilityPermissionRequirement.NONE,
                hardwarePresence,
                true,
                List.of(new CapabilityPortSnapshot(
                        "android.touchpad.source",
                        "Touchpad Frame Source",
                        CapabilityPortDirection.SOURCE,
                        "capyio.input.touchpad-frames/1",
                        List.of("touchpad-frame-v1"))),
                problemCode);
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
