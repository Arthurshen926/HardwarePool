package dev.capyio.android;

import dev.capyio.android.contract.AudioCapabilityKind;
import dev.capyio.android.contract.AudioNodeController;
import dev.capyio.android.contract.CapabilityAvailability;
import dev.capyio.android.contract.CapabilityCatalogSnapshot;
import dev.capyio.android.contract.CapabilityLifecycleController;
import dev.capyio.android.contract.CapabilityLifecycleState;
import dev.capyio.android.contract.CapabilityPortDirection;
import dev.capyio.android.contract.CapabilitySnapshot;
import dev.capyio.android.contract.HardwarePresence;
import dev.capyio.android.contract.GamepadNodeController;
import dev.capyio.android.contract.TransitionToken;

public final class AndroidCapabilityModulesContractTest {
    private static int assertions;

    private AndroidCapabilityModulesContractTest() {}

    public static void main(String[] args) {
        exposesTheCompleteNonDisplayMigrationInventory();
        reflectsPermissionAndAudioLifecycleState();
        reflectsGenerationBoundImuLifecycleState();
        reflectsGenerationBoundGamepadLifecycleState();
        projectsTheCatalogIntoSharedUiSchemaFour();
        System.out.println("Android Capability module composition: PASS ("
                + assertions + " assertions)");
    }

    private static void exposesTheCompleteNonDisplayMigrationInventory() {
        AudioNodeController controller = new AudioNodeController();
        CapabilityLifecycleController imuController = new CapabilityLifecycleController();
        CapabilityCatalogSnapshot snapshot = AndroidCapabilityModules.createRegistry(
                controller,
                imuController,
                new GamepadNodeController(),
                () -> true,
                () -> true,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT).snapshot("test-node");

        check(snapshot.moduleCount() == 4);
        check(snapshot.capabilities().size() == 7);
        check(snapshot.capabilities().stream().filter(CapabilitySnapshot::adapterAttached).count()
                == 4);
        check(snapshot.capabilities().stream().filter(capability -> !capability.adapterAttached()).count()
                == 3);
        check(capability(snapshot, "android.microphone").ports().get(0).direction()
                == CapabilityPortDirection.SOURCE);
        check(capability(snapshot, "android.speaker").ports().get(0).direction()
                == CapabilityPortDirection.SINK);
        check(capability(snapshot, "android.gamepad").ports().get(0).direction()
                == CapabilityPortDirection.SOURCE);
        check(capability(snapshot, "android.haptics").ports().get(0).direction()
                == CapabilityPortDirection.SINK);
        check(capability(snapshot, "android.camera").availability()
                == CapabilityAvailability.OFFLINE);
        check(capability(snapshot, "android.camera").hardwarePresence()
                == HardwarePresence.PRESENT);
        check(capability(snapshot, "android.imu").availability()
                == CapabilityAvailability.AVAILABLE);
        check("CAPY.ANDROID.MODULE_NOT_ATTACHED".equals(
                capability(snapshot, "android.touchpad").problemCode()));
    }

    private static void reflectsPermissionAndAudioLifecycleState() {
        AudioNodeController controller = new AudioNodeController();
        var registry = AndroidCapabilityModules.createRegistry(
                controller,
                new CapabilityLifecycleController(),
                new GamepadNodeController(),
                () -> false,
                () -> true,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.ABSENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.UNKNOWN);
        CapabilityCatalogSnapshot stopped = registry.snapshot("test-node");
        check(capability(stopped, "android.microphone").availability()
                == CapabilityAvailability.PERMISSION_REQUIRED);
        check(capability(stopped, "android.speaker").availability()
                == CapabilityAvailability.AVAILABLE);
        check(capability(stopped, "android.camera").hardwarePresence()
                == HardwarePresence.ABSENT);
        check("CAPY.ANDROID.HARDWARE_PROBE_FAILED".equals(
                capability(stopped, "android.haptics").problemCode()));
        check(capability(stopped, "android.haptics").hardwarePresence()
                == HardwarePresence.UNKNOWN);

        TransitionToken start = controller.beginStart(AudioCapabilityKind.SPEAKER_SINK);
        check(start.accepted());
        CapabilityCatalogSnapshot starting = registry.snapshot("test-node");
        check(capability(starting, "android.speaker").availability()
                == CapabilityAvailability.BUSY);
        check(capability(starting, "android.microphone").availability()
                == CapabilityAvailability.PERMISSION_REQUIRED);
    }

    private static void reflectsGenerationBoundImuLifecycleState() {
        CapabilityLifecycleController imuController = new CapabilityLifecycleController();
        var registry = AndroidCapabilityModules.createRegistry(
                new AudioNodeController(),
                imuController,
                new GamepadNodeController(),
                () -> true,
                () -> true,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT);

        TransitionToken start = imuController.beginStart();
        check(start.accepted());
        check(capability(registry.snapshot("test-node"), "android.imu").availability()
                == CapabilityAvailability.BUSY);
        check(imuController.completeStart(start.generation()));
        check(imuController.snapshot().state() == CapabilityLifecycleState.ACTIVE);

        TransitionToken stop = imuController.beginStop();
        check(stop.accepted());
        check(!imuController.completeStart(start.generation()));
        check(imuController.completeStop(stop.generation()));
        check(capability(registry.snapshot("test-node"), "android.imu").availability()
                == CapabilityAvailability.AVAILABLE);

        TransitionToken retry = imuController.beginStart();
        check(retry.accepted());
        check(imuController.fail(retry.generation(), "CAPY.ANDROID.IMU_START_FAILED"));
        CapabilitySnapshot failed = capability(registry.snapshot("test-node"), "android.imu");
        check(failed.availability() == CapabilityAvailability.OFFLINE);
        check("CAPY.ANDROID.IMU_START_FAILED".equals(failed.problemCode()));
    }

    private static void reflectsGenerationBoundGamepadLifecycleState() {
        GamepadNodeController gamepadController = new GamepadNodeController();
        var registry = AndroidCapabilityModules.createRegistry(
                new AudioNodeController(),
                new CapabilityLifecycleController(),
                gamepadController,
                () -> true,
                () -> true,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT);

        check(capability(registry.snapshot("test-node"), "android.gamepad").availability()
                == CapabilityAvailability.AVAILABLE);
        TransitionToken start = gamepadController.beginStart();
        check(start.accepted());
        check(gamepadController.completeStart(start.generation()));
        check(capability(registry.snapshot("test-node"), "android.gamepad").availability()
                == CapabilityAvailability.BUSY);
        TransitionToken stop = gamepadController.beginStop();
        check(stop.accepted());
        check(gamepadController.completeStop(stop.generation()));
        check(capability(registry.snapshot("test-node"), "android.gamepad").availability()
                == CapabilityAvailability.AVAILABLE);
    }

    private static void projectsTheCatalogIntoSharedUiSchemaFour() {
        CapabilityCatalogSnapshot catalog = AndroidCapabilityModules.createRegistry(
                new AudioNodeController(),
                new CapabilityLifecycleController(),
                new GamepadNodeController(),
                () -> true,
                () -> true,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT).snapshot("test-node");

        String json = AndroidUiSnapshotJson.encode(catalog, "16", "0.4.3-dev");
        check(json.startsWith("{\"backendMode\":\"android_observer\",\"schemaVersion\":4,"));
        check(json.contains("\"id\":\"test-node\",\"displayName\":\"CapyIO Android\""));
        check(json.contains("\"capabilityCount\":7"));
        check(occurrences(json, "\"inventorySource\":\"android_node_catalog\"") == 7);
        check(json.contains("\"capabilityClass\":\"microphone\""));
        check(json.contains("\"direction\":\"source\""));
        check(json.contains("\"direction\":\"sink\""));
        check(json.contains("\"selectableForSharing\":false"));
        check(json.endsWith("\"imuFixture\":null}"));
        check("\"a\\\"b\\\\c\\n\\ud83d\\ude00\"".equals(
                AndroidUiSnapshotJson.encodeStringForContractTest("a\"b\\c\n\ud83d\ude00")));

        String disconnected = AndroidUiSnapshotJson.encodeDisconnected("16", "0.4.3-dev");
        check(disconnected.contains("\"online\":false"));
        check(disconnected.contains("\"capabilities\":[]"));
    }

    private static int occurrences(String value, String target) {
        int count = 0;
        int offset = 0;
        while ((offset = value.indexOf(target, offset)) >= 0) {
            count++;
            offset += target.length();
        }
        return count;
    }

    private static CapabilitySnapshot capability(
            CapabilityCatalogSnapshot snapshot,
            String capabilityKey) {
        return snapshot.capabilities().stream()
                .filter(capability -> capabilityKey.equals(capability.capabilityKey()))
                .findFirst()
                .orElseThrow(() -> new AssertionError("missing Capability " + capabilityKey));
    }

    private static void check(boolean condition) {
        assertions++;
        if (!condition) {
            throw new AssertionError("composition assertion " + assertions + " failed");
        }
    }
}
