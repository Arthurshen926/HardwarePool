package dev.capyio.android.contract;

import java.util.List;

public final class CapabilityCatalogContractTest {
    private static int assertions;

    private CapabilityCatalogContractTest() {}

    public static void main(String[] args) {
        preservesPortDirectionAndModuleOrder();
        rejectsDuplicateAndUnboundedState();
        distinguishesUnattachedModulesFromAvailableHardware();
        derivesStableCrossLanguageCatalogIdentities();
        System.out.println("Android Capability catalog contract: PASS ("
                + assertions + " assertions)");
    }

    private static void preservesPortDirectionAndModuleOrder() {
        CapabilityModuleRegistry registry = new CapabilityModuleRegistry();
        registry.register(module("android.audio", capability(
                "android.microphone",
                CapabilityKind.MICROPHONE,
                CapabilityAvailability.AVAILABLE,
                true,
                CapabilityPortDirection.SOURCE)));
        registry.register(module("android.input", capability(
                "android.touchpad",
                CapabilityKind.TOUCHPAD,
                CapabilityAvailability.OFFLINE,
                false,
                CapabilityPortDirection.SOURCE)));

        CapabilityCatalogSnapshot snapshot = registry.snapshot("test-node");
        check(snapshot.schemaVersion() == 1);
        check(snapshot.moduleCount() == 2);
        check(snapshot.capabilities().size() == 2);
        check("android.microphone".equals(snapshot.capabilities().get(0).capabilityKey()));
        check(snapshot.capabilities().get(0).ports().get(0).direction()
                == CapabilityPortDirection.SOURCE);
        expectFailure(() -> snapshot.capabilities().add(snapshot.capabilities().get(0)));
    }

    private static void rejectsDuplicateAndUnboundedState() {
        CapabilityModuleRegistry registry = new CapabilityModuleRegistry();
        CapabilityModule audio = module("android.audio", capability(
                "android.microphone",
                CapabilityKind.MICROPHONE,
                CapabilityAvailability.AVAILABLE,
                true,
                CapabilityPortDirection.SOURCE));
        registry.register(audio);
        expectFailure(() -> registry.register(audio));
        expectFailure(() -> new CapabilitySnapshot(
                "android.camera",
                "Camera",
                CapabilityKind.CAMERA,
                CapabilityAvailability.AVAILABLE,
                CapabilityPermissionRequirement.USER_CONFIRMATION,
                HardwarePresence.PRESENT,
                false,
                List.of(port("android.camera.source", CapabilityPortDirection.SOURCE)),
                null));
        expectFailure(() -> new CapabilityPortSnapshot(
                "Bad Key",
                "Port",
                CapabilityPortDirection.SOURCE,
                "capyio.video.frames/1",
                List.of("video-frame-v1")));
        expectFailure(() -> new CapabilitySnapshot(
                "android.imu",
                "IMU",
                CapabilityKind.IMU,
                CapabilityAvailability.AVAILABLE,
                CapabilityPermissionRequirement.NONE,
                HardwarePresence.ABSENT,
                true,
                List.of(port("android.imu.source", CapabilityPortDirection.SOURCE)),
                null));
        expectFailure(() -> new CapabilitySnapshot(
                "android.camera",
                "Camera",
                CapabilityKind.CAMERA,
                CapabilityAvailability.PERMISSION_REQUIRED,
                CapabilityPermissionRequirement.USER_CONFIRMATION,
                HardwarePresence.UNKNOWN,
                true,
                List.of(port("android.camera.source", CapabilityPortDirection.SOURCE)),
                null));
        expectFailure(() -> new CapabilityPortSnapshot(
                "android.camera.source",
                "Port",
                CapabilityPortDirection.SOURCE,
                "capyio.video.frames/01",
                List.of("video-frame-v1")));
    }

    private static void distinguishesUnattachedModulesFromAvailableHardware() {
        CapabilitySnapshot pending = capability(
                "android.camera",
                CapabilityKind.CAMERA,
                CapabilityAvailability.OFFLINE,
                false,
                CapabilityPortDirection.SOURCE);
        check(!pending.adapterAttached());
        check(pending.availability() == CapabilityAvailability.OFFLINE);
        check(pending.hardwarePresence() == HardwarePresence.PRESENT);
        check("CAPY.ANDROID.MODULE_NOT_ATTACHED".equals(pending.problemCode()));
    }

    private static void derivesStableCrossLanguageCatalogIdentities() {
        String nodeId = "00000000-0000-4000-8000-00000000a001";
        check("b6cfd6b8-f99c-5fe4-8b55-cba830f956d0".equals(
                AndroidCatalogIdentity.adapterId(nodeId, "android.imu")));
        check("ecaefa6d-6e4e-577e-9ef3-73760663dba7".equals(
                AndroidCatalogIdentity.capabilityId(nodeId, "android.imu")));
        check("44a9ac57-5493-5083-9bae-6cb997420e4a".equals(
                AndroidCatalogIdentity.portId(
                        nodeId, "android.imu", "android.imu.source")));
        check(!AndroidCatalogIdentity.adapterId(nodeId, "android.imu").equals(
                AndroidCatalogIdentity.capabilityId(nodeId, "android.imu")));
        expectFailure(() -> AndroidCatalogIdentity.capabilityId("not-a-uuid", "android.imu"));
        expectFailure(() -> AndroidCatalogIdentity.portId(nodeId, "android.imu", "Bad Port"));
    }

    private static CapabilityModule module(String moduleKey, CapabilitySnapshot capability) {
        return new CapabilityModule() {
            @Override
            public String moduleKey() {
                return moduleKey;
            }

            @Override
            public List<CapabilitySnapshot> capabilities() {
                return List.of(capability);
            }
        };
    }

    private static CapabilitySnapshot capability(
            String key,
            CapabilityKind kind,
            CapabilityAvailability availability,
            boolean attached,
            CapabilityPortDirection direction) {
        return new CapabilitySnapshot(
                key,
                key,
                kind,
                availability,
                CapabilityPermissionRequirement.NONE,
                HardwarePresence.PRESENT,
                attached,
                List.of(port(key + ".port", direction)),
                attached ? null : "CAPY.ANDROID.MODULE_NOT_ATTACHED");
    }

    private static CapabilityPortSnapshot port(
            String key,
            CapabilityPortDirection direction) {
        return new CapabilityPortSnapshot(
                key,
                key,
                direction,
                "capyio.input.touchpad-frames/1",
                List.of("touchpad-frame-v1"));
    }

    private static void expectFailure(Runnable operation) {
        boolean failed = false;
        try {
            operation.run();
        } catch (RuntimeException expected) {
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
