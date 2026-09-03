package dev.capyio.android;

import dev.capyio.android.contract.AudioCapabilityKind;
import dev.capyio.android.contract.AudioNodeController;
import dev.capyio.android.contract.CapabilityAvailability;
import dev.capyio.android.contract.CapabilityCatalogSnapshot;
import dev.capyio.android.contract.CapabilityPortDirection;
import dev.capyio.android.contract.CapabilitySnapshot;
import dev.capyio.android.contract.TransitionToken;

public final class AndroidCapabilityModulesContractTest {
    private static int assertions;

    private AndroidCapabilityModulesContractTest() {}

    public static void main(String[] args) {
        exposesTheCompleteNonDisplayMigrationInventory();
        reflectsPermissionAndAudioLifecycleState();
        System.out.println("Android Capability module composition: PASS ("
                + assertions + " assertions)");
    }

    private static void exposesTheCompleteNonDisplayMigrationInventory() {
        AudioNodeController controller = new AudioNodeController();
        CapabilityCatalogSnapshot snapshot = AndroidCapabilityModules.createRegistry(
                controller,
                () -> true,
                () -> true).snapshot("test-node");

        check(snapshot.moduleCount() == 4);
        check(snapshot.capabilities().size() == 7);
        check(snapshot.capabilities().stream().filter(CapabilitySnapshot::adapterAttached).count()
                == 2);
        check(snapshot.capabilities().stream().filter(capability -> !capability.adapterAttached()).count()
                == 5);
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
        check("CAPY.ANDROID.MODULE_NOT_ATTACHED".equals(
                capability(snapshot, "android.touchpad").problemCode()));
    }

    private static void reflectsPermissionAndAudioLifecycleState() {
        AudioNodeController controller = new AudioNodeController();
        var registry = AndroidCapabilityModules.createRegistry(
                controller,
                () -> false,
                () -> true);
        CapabilityCatalogSnapshot stopped = registry.snapshot("test-node");
        check(capability(stopped, "android.microphone").availability()
                == CapabilityAvailability.PERMISSION_REQUIRED);
        check(capability(stopped, "android.speaker").availability()
                == CapabilityAvailability.AVAILABLE);

        TransitionToken start = controller.beginStart(AudioCapabilityKind.SPEAKER_SINK);
        check(start.accepted());
        CapabilityCatalogSnapshot starting = registry.snapshot("test-node");
        check(capability(starting, "android.speaker").availability()
                == CapabilityAvailability.BUSY);
        check(capability(starting, "android.microphone").availability()
                == CapabilityAvailability.PERMISSION_REQUIRED);
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
