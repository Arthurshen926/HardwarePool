package dev.capyio.android;

import dev.capyio.android.contract.AudioCapabilityKind;
import dev.capyio.android.contract.ActualAudioFormat;
import dev.capyio.android.contract.AudioNodeController;
import dev.capyio.android.contract.CapabilityAvailability;
import dev.capyio.android.contract.CapabilityCatalogSnapshot;
import dev.capyio.android.contract.CapabilityLifecycleController;
import dev.capyio.android.contract.CapabilityLifecycleSnapshot;
import dev.capyio.android.contract.CapabilityLifecycleState;
import dev.capyio.android.contract.CapabilityPortDirection;
import dev.capyio.android.contract.CapabilityPublicationProjection;
import dev.capyio.android.contract.CapabilitySnapshot;
import dev.capyio.android.contract.HardwarePresence;
import dev.capyio.android.contract.GamepadNodeController;
import dev.capyio.android.contract.GamepadControlsSnapshot;
import dev.capyio.android.contract.TransitionToken;
import dev.capyio.android.contract.TouchpadContactSnapshot;
import dev.capyio.android.contract.TouchpadControlsSnapshot;
import dev.capyio.android.contract.TouchpadNodeController;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;

public final class AndroidCapabilityModulesContractTest {
    private static int assertions;

    private AndroidCapabilityModulesContractTest() {}

    public static void main(String[] args) throws Exception {
        exposesTheCompleteNonDisplayMigrationInventory();
        reflectsPermissionAndAudioLifecycleState();
        reflectsGenerationBoundImuLifecycleState();
        reflectsGenerationBoundGamepadLifecycleState();
        reflectsGenerationBoundTouchpadLifecycleState();
        keepsTouchpadSourcePublishableAfterDataFailure();
        rejectsStaleSourceFramesBeforeAdapterDelivery();
        releasesUiInputsOnForegroundLoss();
        retainsServiceForConnectedDeviceLifecycle();
        projectsTheCatalogIntoSharedUiSchemaFour();
        admitsOnlyLocalAttachedSourceSharingIntent();
        projectsSelectedAndroidPublicationCatalog();
        commitsSharingIntentOnlyAfterNativeProjectionAcceptsIt();
        encodesSelectedPublicationForTheRustHost();
        permitsDeselectionAfterAnAndroidSourceGoesOffline();
        projectsAndAdmitsClosedCapabilityQuickActions();
        appliesAndProjectsClosedGamepadUiCommands();
        parsesAndProjectsClosedTouchpadUiFrames();
        reconstructsTouchpadPointerLifecycle();
        encodesControllerLabDatagram();
        sharesImuSamplingWithoutStoppingAnotherUser();
        check(HapticsStrength.amplitude(1000, 0) == 0);
        check(HapticsStrength.amplitude(1000, 100) == 255);
        check(HapticsStrength.amplitude(1000, 50) == 128);
        check(HapticsStrength.amplitude(0, 100) == 0);
        check(HapticsStrength.amplitude(1, 1) == 1);
        check(ResourceConsentMemory.allows("touchpad", 100, -1, 100));
        check(!ResourceConsentMemory.allows("haptics", 100, -1, 100));
        check(ResourceConsentMemory.allows("haptics", 100, 100, -1));
        check(!ResourceConsentMemory.allows("haptics", 101, 100, 100));
        check(!ResourceConsentMemory.allows("touchpad", -1, -1, -1));
        projectsAndAdmitsClosedAudioQuickActions();
        attachesAndAdmitsClosedCameraQuickAction();
        advertisesOnlyTheExplicitEligibleCameraProfile();
        attachesAndAdmitsHapticsSink();
        System.out.println("Android Capability module composition: PASS ("
                + assertions + " assertions)");
    }

    private static void retainsServiceForConnectedDeviceLifecycle() {
        check(AndroidForegroundOwnership.connectedDevice(true, false, null, null, null, null));
        check(AndroidForegroundOwnership.connectedDevice(false, true, null, null, null, null));
        check(!AndroidForegroundOwnership.connectedDevice(false, false, null, null, null, null));
        TouchpadNodeController touchpad = new TouchpadNodeController();
        long generation = touchpad.beginStart().generation();
        check(touchpad.completeStart(generation));
        check(touchpad.completeRemoteStop(generation));
        check(touchpad.lifecycle().state() == CapabilityLifecycleState.STOPPED);
        long retry = touchpad.beginStart().generation();
        check(!touchpad.completeRemoteStop(generation));
        check(touchpad.lifecycle().generation() == retry);
        check(touchpad.completeRemoteStop(retry));
        for (CapabilityLifecycleState owner : List.of(
                CapabilityLifecycleState.STARTING,
                CapabilityLifecycleState.ACTIVE,
                CapabilityLifecycleState.STOPPING)) {
            check(AndroidForegroundOwnership.connectedDevice(
                    owner,
                    CapabilityLifecycleState.STOPPED,
                    CapabilityLifecycleState.STOPPED,
                    CapabilityLifecycleState.STOPPED));
        }
        check(AndroidForegroundOwnership.connectedDevice(
                CapabilityLifecycleState.STOPPED,
                CapabilityLifecycleState.ACTIVE,
                CapabilityLifecycleState.STOPPED,
                CapabilityLifecycleState.STOPPED));
        check(AndroidForegroundOwnership.connectedDevice(
                CapabilityLifecycleState.STOPPED,
                CapabilityLifecycleState.STOPPED,
                CapabilityLifecycleState.ACTIVE,
                CapabilityLifecycleState.STOPPED));
        check(AndroidForegroundOwnership.connectedDevice(
                CapabilityLifecycleState.STOPPED,
                CapabilityLifecycleState.STOPPED,
                CapabilityLifecycleState.STOPPED,
                CapabilityLifecycleState.ACTIVE));
        check(!AndroidForegroundOwnership.connectedDevice(
                CapabilityLifecycleState.STOPPED,
                CapabilityLifecycleState.FAILED,
                CapabilityLifecycleState.STOPPED,
                CapabilityLifecycleState.FAILED));
    }

    private static void keepsTouchpadSourcePublishableAfterDataFailure() {
        TouchpadNodeController touchpad = new TouchpadNodeController();
        long generation = touchpad.beginStart().generation();
        check(touchpad.fail(generation, "CAPY.ANDROID.TOUCHPAD_RECEIVER_UNAVAILABLE"));
        CapabilityCatalogSnapshot catalog = AndroidCapabilityModules.createRegistry(
                new AudioNodeController(),
                new CapabilityLifecycleController(),
                new GamepadNodeController(),
                touchpad,
                () -> true,
                () -> true,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT).snapshot("test-node");
        CapabilitySnapshot source = capability(catalog, "android.touchpad");
        check(source.availability() == CapabilityAvailability.AVAILABLE);
        check("CAPY.ANDROID.TOUCHPAD_RECEIVER_UNAVAILABLE".equals(source.problemCode()));
        check(new AndroidSharingSelection().update(catalog, "test-node:android.touchpad", true));
        String coreCatalog = AndroidCoreCatalogJson.encode(catalog, "test-platform", "test-runtime");
        check(coreCatalog.contains("\"key\":\"android.touchpad\""));
        check(coreCatalog.contains("\"availability\":\"available\""));
        String actions = AndroidUiSnapshotJson.encodeQuickActions(
                new CapabilityLifecycleController().snapshot(),
                new CapabilityLifecycleController().snapshot(),
                touchpad.lifecycle());
        check(actions.contains("\"availableOperations\":[\"stop\"]"));
        check(actions.contains("上一次触控链路失败"));
    }

    private static void exposesTheCompleteNonDisplayMigrationInventory() {
        AudioNodeController controller = new AudioNodeController();
        CapabilityLifecycleController imuController = new CapabilityLifecycleController();
        CapabilityCatalogSnapshot snapshot = AndroidCapabilityModules.createRegistry(
                controller,
                imuController,
                new GamepadNodeController(),
                new TouchpadNodeController(),
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
                == 5);
        check(snapshot.capabilities().stream().filter(capability -> !capability.adapterAttached()).count()
                == 2);
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
        check(capability(snapshot, "android.touchpad").availability()
                == CapabilityAvailability.AVAILABLE);
        check(capability(snapshot, "android.touchpad").adapterAttached());
    }

    private static void reflectsPermissionAndAudioLifecycleState() {
        AudioNodeController controller = new AudioNodeController();
        var registry = AndroidCapabilityModules.createRegistry(
                controller,
                new CapabilityLifecycleController(),
                new GamepadNodeController(),
                new TouchpadNodeController(),
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
                new TouchpadNodeController(),
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
                new TouchpadNodeController(),
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

    private static void reflectsGenerationBoundTouchpadLifecycleState() {
        TouchpadNodeController controller = new TouchpadNodeController();
        check(controller.controls().contacts().isEmpty());
        TransitionToken start = controller.beginStart();
        check(start.accepted());
        check(controller.completeStart(start.generation()));
        check(controller.submit(start.generation(), List.of(
                new TouchpadContactSnapshot(0, 1_000, 2_000, 512),
                new TouchpadContactSnapshot(1, 9_000, 8_000, 1_024))));
        check(controller.controls().sequence() == 1);
        check(controller.controls().contacts().size() == 2);
        TransitionToken stop = controller.beginStop();
        check(stop.accepted());
        check(controller.controls().contacts().isEmpty());
        check(!controller.submit(start.generation(), List.of(
                new TouchpadContactSnapshot(0, 1, 1, 1))));
        check(controller.completeStop(stop.generation()));
    }

    private static void rejectsStaleSourceFramesBeforeAdapterDelivery() {
        StringBuilder order = new StringBuilder();
        check(!GenerationBoundSourceDelivery.submit(
                () -> false,
                () -> {
                    order.append("deliver");
                    return true;
                },
                () -> order.append("reject")));
        check(order.isEmpty());

        check(GenerationBoundSourceDelivery.submit(
                () -> {
                    order.append("commit");
                    return true;
                },
                () -> {
                    order.append("-deliver");
                    return true;
                },
                () -> order.append("-reject")));
        check("commit-deliver".contentEquals(order));

        order.setLength(0);
        check(!GenerationBoundSourceDelivery.submit(
                () -> {
                    order.append("commit");
                    return true;
                },
                () -> false,
                () -> order.append("-reject")));
        check("commit-reject".contentEquals(order));
    }

    private static void releasesUiInputsOnForegroundLoss() {
        CapabilityLifecycleController lifecycle = new CapabilityLifecycleController();
        CapabilityLifecycleSnapshot stopped = lifecycle.snapshot();
        GamepadControlsSnapshot pressed = new GamepadControlsSnapshot(
                1, 0, 0, 0, 0, 0, 0, 0, 0);
        TouchpadControlsSnapshot contact = new TouchpadControlsSnapshot(
                1, List.of(new TouchpadContactSnapshot(0, 100, 200, 512)));
        check(AndroidUiInputRelease.gamepad(stopped, pressed) == null);
        check(AndroidUiInputRelease.touchpad(stopped, contact) == null);

        TransitionToken start = lifecycle.beginStart();
        check(lifecycle.completeStart(start.generation()));
        CapabilityLifecycleSnapshot active = lifecycle.snapshot();
        check(GamepadControlsSnapshot.neutral().equals(
                AndroidUiInputRelease.gamepad(active, pressed)));
        check(AndroidUiInputRelease.gamepad(
                active, GamepadControlsSnapshot.neutral()) == null);
        check(AndroidUiInputRelease.touchpad(active, contact).isEmpty());
        check(AndroidUiInputRelease.touchpad(
                active, TouchpadControlsSnapshot.neutral(2)) == null);
    }

    private static void projectsTheCatalogIntoSharedUiSchemaFour() {
        CapabilityCatalogSnapshot catalog = AndroidCapabilityModules.createRegistry(
                new AudioNodeController(),
                new CapabilityLifecycleController(),
                new GamepadNodeController(),
                new TouchpadNodeController(),
                () -> true,
                () -> true,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT).snapshot("test-node");

        String json = AndroidUiSnapshotJson.encode(catalog, "16", "0.4.3-dev");
        check(json.startsWith("{\"backendMode\":\"android_node\",\"schemaVersion\":5,"));
        check(json.contains("\"controlPlane\":{\"schemaVersion\":1,\"simulated\":false"));
        check(json.contains("\"discoveryState\":\"not_started\""));
        check(json.contains("\"pairingState\":\"not_configured\""));
        check(json.contains("\"transportState\":\"not_connected\""));
        check(json.contains("\"catalogExchangeState\":\"not_connected\""));
        check(json.contains("\"id\":\"test-node\",\"displayName\":\"CapyIO Android\""));
        check(json.contains("\"capabilityCount\":7"));
        check(occurrences(json, "\"inventorySource\":\"android_node_catalog\"") == 7);
        check(json.contains("\"capabilityClass\":\"microphone\""));
        check(json.contains("\"direction\":\"source\""));
        check(json.contains("\"direction\":\"sink\""));
        check(occurrences(json, "\"selectableForSharing\":true") == 5);
        check(occurrences(json, "\"selectableForSharing\":false") == 2);
        check(occurrences(json, "\"selectedForSharing\":false") == 7);
        check(json.endsWith("\"imuFixture\":null}"));
        String pairingReady = AndroidUiSnapshotJson.encode(
                catalog, "16", "0.4.3-dev", java.util.Set.of(), "ready", 0);
        check(pairingReady.contains("\"pairingState\":\"ready\""));
        check(pairingReady.contains("\"pairedPeerCount\":0"));
        String controlConnected = AndroidUiSnapshotJson.encode(
                catalog, "16", "0.4.3-dev", java.util.Set.of(), "ready", 1, true, true);
        check(controlConnected.contains("\"discoveryState\":\"listening\""));
        check(controlConnected.contains("\"transportState\":\"connected\""));
        check(controlConnected.contains("\"catalogExchangeState\":\"active\""));
        check(controlConnected.contains("\"authenticatedConnectionCount\":1"));
        String pairingFailed = AndroidUiSnapshotJson.encode(
                catalog, "16", "0.4.3-dev", java.util.Set.of(), "failed", 0);
        check(pairingFailed.contains("\"pairingState\":\"failed\""));
        check("\"a\\\"b\\\\c\\n\\ud83d\\ude00\"".equals(
                AndroidUiSnapshotJson.encodeStringForContractTest("a\"b\\c\n\ud83d\ude00")));

        String disconnected = AndroidUiSnapshotJson.encodeDisconnected("16", "0.4.3-dev");
        check(disconnected.contains("\"online\":false"));
        check(disconnected.contains("\"capabilities\":[]"));
        check(disconnected.contains("\"controlPlane\":{\"schemaVersion\":1"));
    }

    private static void admitsOnlyLocalAttachedSourceSharingIntent() {
        CapabilityCatalogSnapshot catalog = AndroidCapabilityModules.createRegistry(
                new AudioNodeController(),
                new CapabilityLifecycleController(),
                new GamepadNodeController(),
                new TouchpadNodeController(),
                () -> false,
                () -> true,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT).snapshot("test-node");
        AndroidSharingSelection selection = new AndroidSharingSelection();

        check(selection.update(catalog, "test-node:android.microphone", true));
        check(selection.update(catalog, "test-node:android.imu", true));
        check(selection.update(catalog, "test-node:android.gamepad", true));
        check(selection.update(catalog, "test-node:android.touchpad", true));
        check(selection.update(catalog, "test-node:android.speaker", true));
        check(selection.update(catalog, "test-node:android.speaker", false));
        check(selection.publicationCatalog(catalog).capabilities().stream().noneMatch(c -> c.capabilityKey().equals("android.speaker")));
        check(!selection.update(catalog, "test-node:android.haptics", true));
        check(!selection.update(catalog, "test-node:android.camera", true));
        check(!selection.update(catalog, "other-node:android.imu", true));
        check(!selection.update(catalog, "test-node:android.unknown", true));
        check(!selection.update(catalog, "x".repeat(257), true));
        check(!selection.update(catalog, null, true));
        check(selection.snapshot().size() == 4);

        String selected = AndroidUiSnapshotJson.encode(
                catalog, "16", "0.4.3-dev", selection.snapshot());
        check(occurrences(selected, "\"selectedForSharing\":true") == 4);
        check(selected.contains("\"id\":\"test-node:android.microphone\""));
        check(selection.update(catalog, "test-node:android.imu", false));
        check(selection.snapshot().size() == 3);
        check(selection.update(catalog, "test-node:android.imu", false));
        check(selection.snapshot().size() == 3);
    }

    private static void projectsSelectedAndroidPublicationCatalog() {
        CapabilityCatalogSnapshot catalog = AndroidCapabilityModules.createRegistry(
                new AudioNodeController(),
                new CapabilityLifecycleController(),
                new GamepadNodeController(),
                new TouchpadNodeController(),
                () -> true,
                () -> true,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT).snapshot("test-node");
        AndroidSharingSelection selection = new AndroidSharingSelection();
        check(selection.update(catalog, "test-node:android.microphone", true));
        check(selection.update(catalog, "test-node:android.speaker", true));
        var published = selection.publicationCatalog(catalog);

        check(published.nodeId().equals(catalog.nodeId()));
        check(published.moduleCount() == catalog.moduleCount());
        check(published.capabilities().size() == 3);
        check(capability(published, "android.microphone") != null);
        check(capability(published, "android.speaker") != null);
        check(capability(published, "android.haptics") != null);
        check(published.capabilities().stream().noneMatch(
                capability -> "android.imu".equals(capability.capabilityKey())));
        boolean rejected = false;
        try {
            CapabilityPublicationProjection.project(
                    catalog,
                    java.util.Set.of("test-node:android.speaker"));
        } catch (IllegalArgumentException expected) {
            rejected = true;
        }
        check(rejected);
    }

    private static void encodesSelectedPublicationForTheRustHost() throws Exception {
        CapabilityCatalogSnapshot catalog = AndroidCapabilityModules.createRegistry(
                new AudioNodeController(),
                new CapabilityLifecycleController(),
                new GamepadNodeController(),
                new TouchpadNodeController(),
                () -> true,
                () -> true,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT).snapshot("00000000-0000-4000-8000-00000000a001");
        AndroidSharingSelection selection = new AndroidSharingSelection();
        check(selection.update(
                catalog,
                "00000000-0000-4000-8000-00000000a001:android.speaker", true));
        check(selection.update(
                catalog,
                "00000000-0000-4000-8000-00000000a001:android.microphone",
                true));

        String json = AndroidCoreCatalogJson.encode(
                selection.publicationCatalog(catalog), "16", "0.5.2-dev");
        check(json.startsWith("{\"schema_version\":1,"));
        check(json.contains("\"node_id\":\"00000000-0000-4000-8000-00000000a001\""));
        check(json.contains("\"key\":\"android.microphone\""));
        check(json.contains("\"key\":\"android.speaker\""));
        check(json.contains("\"key\":\"android.haptics\""));
        check(!json.contains("\"key\":\"android.imu\""));
        check(json.contains("\"supported_route_modes\":[\"adapter_managed\"]"));
        check(json.contains("\"adapter_type\":\"dev.capyio.android.haptics-tls\""));
        check(json.contains("\"interoperability\":\"adapter_managed\""));
        check(json.contains("\"clock_domain\":null"));
        String fixturePath = System.getProperty("capyio.android.catalogFixture");
        check(fixturePath != null);
        check(json.equals(Files.readString(Path.of(fixturePath)).strip()));
    }

    private static void commitsSharingIntentOnlyAfterNativeProjectionAcceptsIt() {
        CapabilityCatalogSnapshot catalog = AndroidCapabilityModules.createRegistry(
                new AudioNodeController(),
                new CapabilityLifecycleController(),
                new GamepadNodeController(),
                new TouchpadNodeController(),
                () -> true,
                () -> true,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT).snapshot("test-node");
        AndroidSharingSelection selection = new AndroidSharingSelection();

        check(!selection.update(
                catalog,
                "test-node:android.microphone",
                true,
                ignored -> { throw new IllegalStateException("native rejection"); }));
        check(selection.snapshot().isEmpty());
        check(selection.update(
                catalog,
                "test-node:android.microphone",
                true,
                publication -> check(capability(publication, "android.microphone") != null)));
        check(selection.snapshot().contains("test-node:android.microphone"));
    }

    private static void permitsDeselectionAfterAnAndroidSourceGoesOffline() {
        CapabilityLifecycleController imu = new CapabilityLifecycleController();
        var registry = AndroidCapabilityModules.createRegistry(
                new AudioNodeController(),
                imu,
                new GamepadNodeController(),
                new TouchpadNodeController(),
                () -> true,
                () -> true,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT);
        AndroidSharingSelection selection = new AndroidSharingSelection();
        CapabilityCatalogSnapshot initialCatalog = registry.snapshot("test-node");

        check(selection.update(initialCatalog, "test-node:android.imu", true));
        TransitionToken start = imu.beginStart();
        check(imu.fail(start.generation(), "CAPY.ANDROID.IMU_START_FAILED"));
        CapabilityCatalogSnapshot offlineCatalog = registry.snapshot("test-node");
        check(!AndroidSharingSelection.isSelectable(capability(offlineCatalog, "android.imu")));
        check(selection.update(offlineCatalog, "test-node:android.imu", false));
        check(selection.snapshot().isEmpty());
        check(!selection.update(offlineCatalog, "test-node:android.imu", true));
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

    private static void projectsAndAdmitsClosedCapabilityQuickActions() {
        CapabilityLifecycleController imu = new CapabilityLifecycleController();
        CapabilityLifecycleController gamepad = new CapabilityLifecycleController();
        CapabilityLifecycleController touchpad = new CapabilityLifecycleController();
        String stopped = AndroidUiSnapshotJson.encodeQuickActions(
                imu.snapshot(), gamepad.snapshot(), touchpad.snapshot());
        check(occurrences(stopped, "\"kind\":\"capability_control\"") == 3);
        check(stopped.contains("capyio.quick-action.android-imu"));
        check(stopped.contains("capyio.quick-action.android-gamepad"));
        check(stopped.contains("capyio.quick-action.android-touchpad"));
        check(occurrences(stopped, "\"availableOperations\":[\"start\"]") == 2);
        check(stopped.contains("\"id\":\"capyio.quick-action.android-touchpad\""));
        check(stopped.contains("\"availableOperations\":[]"));
        check(!stopped.contains("\"routeId\":\""));

        TransitionToken imuStart = imu.beginStart();
        check(imuStart.accepted());
        String starting = AndroidUiSnapshotJson.encodeQuickActions(
                imu.snapshot(), gamepad.snapshot(), touchpad.snapshot());
        check(starting.contains("\"status\":\"starting\""));
        check(starting.contains("\"availableOperations\":[\"stop\"]"));
        check(imu.completeStart(imuStart.generation()));
        check(AndroidUiSnapshotJson.encodeQuickActions(
                imu.snapshot(), gamepad.snapshot(), touchpad.snapshot())
                .contains("\"status\":\"active\""));

        check(AndroidCapabilityCommand.parse(
                "capyio.quick-action.android-imu",
                "start",
                CapabilityLifecycleState.STOPPED).target()
                == AndroidCapabilityCommand.Target.IMU);
        check(AndroidCapabilityCommand.parse(
                "capyio.quick-action.android-gamepad",
                "stop",
                CapabilityLifecycleState.ACTIVE).target()
                == AndroidCapabilityCommand.Target.GAMEPAD);
        check(AndroidCapabilityCommand.parse(
                "capyio.quick-action.android-touchpad",
                "start",
                CapabilityLifecycleState.STOPPED).target()
                == AndroidCapabilityCommand.Target.TOUCHPAD);
        check(AndroidCapabilityCommand.parse(
                "capyio.quick-action.android-imu",
                "retry",
                CapabilityLifecycleState.FAILED).operation()
                == AndroidCapabilityCommand.Operation.RETRY);
        check(AndroidCapabilityCommand.parse(
                "capyio.quick-action.android-imu",
                "stop",
                CapabilityLifecycleState.STOPPED) == null);
        check(AndroidCapabilityCommand.parse(
                "capyio.quick-action.android-imu",
                "start",
                CapabilityLifecycleState.ACTIVE) == null);
        check(AndroidCapabilityCommand.parse(
                "capyio.quick-action.android-imu",
                "retry",
                CapabilityLifecycleState.STOPPING) == null);
        check(AndroidCapabilityCommand.parse(
                "capyio.quick-action.android-camera",
                "start",
                CapabilityLifecycleState.STOPPED) == null);
        check(AndroidCapabilityCommand.parse(
                "capyio.quick-action.android-imu",
                "start;stop",
                CapabilityLifecycleState.STOPPED) == null);
        check(AndroidCapabilityCommand.parse(null, "start", CapabilityLifecycleState.STOPPED)
                == null);
    }

    private static void appliesAndProjectsClosedGamepadUiCommands() {
        GamepadControlsSnapshot neutral = GamepadControlsSnapshot.neutral();
        GamepadControlsSnapshot south = AndroidGamepadUiCommand.apply(
                "button", "south", 0, 0, true, neutral);
        check(south != null && south.buttons() == 1);
        GamepadControlsSnapshot guide = AndroidGamepadUiCommand.apply(
                "button", "guide", 0, 0, true, south);
        check(guide != null && guide.buttons() == ((1 << 10) | 1));
        GamepadControlsSnapshot released = AndroidGamepadUiCommand.apply(
                "button", "south", 0, 0, false, guide);
        check(released != null && released.buttons() == (1 << 10));

        GamepadControlsSnapshot dpad = AndroidGamepadUiCommand.apply(
                "dpad", "", -1, 1, false, released);
        check(dpad != null && dpad.dpadX() == -1 && dpad.dpadY() == 1);
        GamepadControlsSnapshot left = AndroidGamepadUiCommand.apply(
                "stick", "left", -32_767, 32_767, false, dpad);
        check(left != null && left.leftX() == -32_767 && left.leftY() == 32_767);
        GamepadControlsSnapshot right = AndroidGamepadUiCommand.apply(
                "stick", "right", 10, -20, false, left);
        check(right != null && right.rightX() == 10 && right.rightY() == -20);
        GamepadControlsSnapshot triggers = AndroidGamepadUiCommand.apply(
                "trigger", "left", 65_535, 0, false, right);
        triggers = AndroidGamepadUiCommand.apply(
                "trigger", "right", 123, 0, false, triggers);
        check(triggers != null && triggers.leftTrigger() == 65_535
                && triggers.rightTrigger() == 123);
        check(triggers.buttons() == (1 << 10) && triggers.leftX() == -32_767);

        check(AndroidGamepadUiCommand.apply("button", "touchpad", 0, 0, true, neutral)
                == null);
        check(AndroidGamepadUiCommand.apply("button", "south", 1, 0, true, neutral)
                == null);
        check(AndroidGamepadUiCommand.apply("dpad", "", 2, 0, false, neutral) == null);
        check(AndroidGamepadUiCommand.apply("stick", "left", 32_768, 0, false, neutral)
                == null);
        check(AndroidGamepadUiCommand.apply("trigger", "left", 65_536, 0, false, neutral)
                == null);
        check(AndroidGamepadUiCommand.apply("trigger", "left", 1, 1, false, neutral)
                == null);
        check(AndroidGamepadUiCommand.apply("reset", "bad", 0, 0, false, neutral)
                == null);
        check(AndroidGamepadUiCommand.apply("reset;start", "", 0, 0, false, neutral)
                == null);
        check(AndroidGamepadUiCommand.apply(null, "", 0, 0, false, neutral) == null);
        check(AndroidGamepadUiCommand.apply("reset", "", 0, 0, false, triggers)
                .equals(neutral));

        CapabilityLifecycleController lifecycle = new CapabilityLifecycleController();
        String stopped = AndroidUiSnapshotJson.encodeGamepadState(
                lifecycle.snapshot(), neutral);
        check(stopped.contains("\"source\":\"android_touch\""));
        check(stopped.contains("\"status\":\"stopped\""));
        check(stopped.contains("\"source\":\"unavailable\""));
        TransitionToken start = lifecycle.beginStart();
        check(lifecycle.completeStart(start.generation()));
        String active = AndroidUiSnapshotJson.encodeGamepadState(
                lifecycle.snapshot(), triggers);
        check(active.contains("\"streamEpoch\":" + start.generation()));
        check(active.contains("\"pressedButtons\":[\"guide\"]"));
        check(active.contains("\"leftStick\":{\"x\":-32767,\"y\":32767}"));
        check(active.contains("\"rightTrigger\":123"));
        check(active.contains("\"endpoint\":null"));
        check(active.endsWith("}"));
    }

    private static void parsesAndProjectsClosedTouchpadUiFrames() {
        List<TouchpadContactSnapshot> contacts = AndroidTouchpadUiFrame.parse(
                "0,1000,2000,512;1,9000,8000,1024");
        check(contacts != null && contacts.size() == 2);
        check(contacts.get(0).contactId() == 0 && contacts.get(0).x() == 1_000);
        check(AndroidTouchpadUiFrame.parse("").isEmpty());
        check(AndroidTouchpadUiFrame.parse("00,1,1,1") == null);
        check(AndroidTouchpadUiFrame.parse("0,+1,1,1") == null);
        check(AndroidTouchpadUiFrame.parse("0,1,1") == null);
        check(AndroidTouchpadUiFrame.parse("0,10001,1,1") == null);
        check(AndroidTouchpadUiFrame.parse("0,1,1,1025") == null);
        check(AndroidTouchpadUiFrame.parse("0,1,1,1;0,2,2,2") == null);
        check(AndroidTouchpadUiFrame.parse(
                "0,1,1,1;1,1,1,1;2,1,1,1;3,1,1,1;4,1,1,1;5,1,1,1") == null);
        check(AndroidTouchpadUiFrame.parse("0,1,1,1;start") == null);
        check(AndroidTouchpadUiFrame.parse(null) == null);

        CapabilityLifecycleController lifecycle = new CapabilityLifecycleController();
        String stopped = AndroidUiSnapshotJson.encodeTouchpadState(
                lifecycle.snapshot(), TouchpadControlsSnapshot.neutral(0));
        check(stopped.contains("\"source\":\"android_shared_ui\""));
        check(stopped.contains("\"status\":\"idle\""));
        check(stopped.contains("\"contacts\":[]"));
        TransitionToken start = lifecycle.beginStart();
        check(lifecycle.completeStart(start.generation()));
        String active = AndroidUiSnapshotJson.encodeTouchpadState(
                lifecycle.snapshot(), new TouchpadControlsSnapshot(7, contacts));
        check(active.contains("\"streamEpoch\":" + start.generation()));
        check(active.contains("\"sequence\":7"));
        check(active.contains("\"status\":\"active\""));
        check(active.contains("{\"contactId\":0,\"x\":1000,\"y\":2000,\"pressure\":512}"));
    }

    private static void projectsAndAdmitsClosedAudioQuickActions() {
        AudioNodeController audio = new AudioNodeController();
        CapabilityLifecycleController lifecycle = new CapabilityLifecycleController();
        String stopped = AndroidUiSnapshotJson.encodeQuickActions(
                audio.snapshot("test-node"),
                false,
                true,
                lifecycle.snapshot(),
                lifecycle.snapshot(),
                lifecycle.snapshot());
        check(occurrences(stopped, "\"kind\":\"capability_control\"") == 7);
        check(stopped.contains("capyio.quick-action.android-microphone"));
        check(stopped.contains("capyio.quick-action.android-speaker"));
        check(stopped.contains("capyio.quick-action.android-camera"));
        check(stopped.contains("\"status\":\"blocked\""));
        check(stopped.contains("CAPY.ANDROID.PERMISSION_REQUIRED"));
        check(stopped.contains("\"availableOperations\":[\"start\"]"));

        TransitionToken start = audio.beginStart(AudioCapabilityKind.MICROPHONE_SOURCE);
        check(start.accepted());
        check(audio.completeStart(
                AudioCapabilityKind.MICROPHONE_SOURCE,
                start.generation(),
                new ActualAudioFormat(48_000, 1, "pcm_16bit", 480)));
        audio.addFrames(AudioCapabilityKind.MICROPHONE_SOURCE, start.generation(), 960);
        String state = AndroidUiSnapshotJson.encodeAudioState(
                audio.snapshot("test-node"), true, true);
        check(state.contains("\"nodeId\":\"test-node\""));
        check(state.contains("\"state\":\"active\""));
        check(state.contains("\"framesProcessed\":960"));
        check(state.contains("\"sampleRateHz\":48000"));
        check(state.contains("\"encoding\":\"pcm_16bit\""));

        check(AndroidAudioCommand.parse(
                "capyio.quick-action.android-microphone", "start",
                dev.capyio.android.contract.AudioCapabilityState.STOPPED).target()
                == AudioCapabilityKind.MICROPHONE_SOURCE);
        check(AndroidAudioCommand.parse(
                "capyio.quick-action.android-speaker", "stop",
                dev.capyio.android.contract.AudioCapabilityState.ACTIVE).target()
                == AudioCapabilityKind.SPEAKER_SINK);
        check(AndroidAudioCommand.parse(
                "capyio.quick-action.android-speaker", "retry",
                dev.capyio.android.contract.AudioCapabilityState.FAILED).operation()
                == AndroidAudioCommand.Operation.RETRY);
        check(AndroidAudioCommand.parse(
                "capyio.quick-action.android-speaker", "start",
                dev.capyio.android.contract.AudioCapabilityState.ACTIVE) == null);
        check(AndroidAudioCommand.parse(
                "capyio.quick-action.android-camera", "start",
                dev.capyio.android.contract.AudioCapabilityState.STOPPED) == null);
        check(AndroidAudioCommand.parse(
                "capyio.quick-action.android-speaker", "start;stop",
                dev.capyio.android.contract.AudioCapabilityState.STOPPED) == null);
    }

    private static void attachesAndAdmitsClosedCameraQuickAction() {
        CapabilityLifecycleController camera = new CapabilityLifecycleController();
        CapabilityCatalogSnapshot catalog = AndroidCapabilityModules.createRegistry(
                new AudioNodeController(),
                new CapabilityLifecycleController(),
                new GamepadNodeController(),
                new TouchpadNodeController(),
                new CapabilityLifecycleController(),
                () -> true,
                () -> true,
                camera::snapshot,
                () -> false,
                true,
                false,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT).snapshot("test-node");
        CapabilitySnapshot cameraCapability = capability(catalog, "android.camera");
        check(cameraCapability.adapterAttached());
        check(cameraCapability.availability() == CapabilityAvailability.PERMISSION_REQUIRED);
        check(AndroidSharingSelection.isSelectable(cameraCapability));

        String stopped = AndroidUiSnapshotJson.encodeQuickActions(
                new AudioNodeController().snapshot("test-node"),
                true,
                true,
                camera.snapshot(),
                false,
                new CapabilityLifecycleController().snapshot(),
                new GamepadNodeController().lifecycle(),
                new TouchpadNodeController().lifecycle(),
                new CapabilityLifecycleController().snapshot());
        check(occurrences(stopped, "\"kind\":\"capability_control\"") == 7);
        check(stopped.contains("capyio.quick-action.android-camera"));
        check(stopped.contains("CAPY.ANDROID.PERMISSION_REQUIRED"));
        check(AndroidCameraCommand.parse(
                AndroidCameraCommand.ACTION_ID,
                "start",
                CapabilityLifecycleState.STOPPED).operation()
                == AndroidCameraCommand.Operation.START);
        check(AndroidCameraCommand.parse(
                AndroidCameraCommand.ACTION_ID,
                "stop",
                CapabilityLifecycleState.ACTIVE).operation()
                == AndroidCameraCommand.Operation.STOP);
        check(AndroidCameraCommand.parse(
                AndroidCameraCommand.ACTION_ID,
                "retry",
                CapabilityLifecycleState.FAILED).operation()
                == AndroidCameraCommand.Operation.RETRY);
        check(AndroidCameraCommand.parse(
                "capyio.quick-action.android-camera-extra",
                "start",
                CapabilityLifecycleState.STOPPED) == null);
        check(AndroidCameraCommand.parse(
                AndroidCameraCommand.ACTION_ID,
                "start",
                CapabilityLifecycleState.ACTIVE) == null);
    }

    private static void advertisesOnlyTheExplicitEligibleCameraProfile() {
        var selectedFormats = new java.util.concurrent.atomic.AtomicReference<List<String>>(List.of());
        var registry = AndroidCapabilityModules.createRegistry(
                new AudioNodeController(), new CapabilityLifecycleController(),
                new GamepadNodeController(), new TouchpadNodeController(),
                new CapabilityLifecycleController(), () -> true, () -> true,
                new CapabilityLifecycleController()::snapshot, () -> true, selectedFormats::get,
                true, false, HardwarePresence.PRESENT, HardwarePresence.PRESENT,
                HardwarePresence.PRESENT, HardwarePresence.PRESENT,
                HardwarePresence.PRESENT, HardwarePresence.PRESENT);
        CapabilitySnapshot unavailable = capability(registry.snapshot("test-node"), "android.camera");
        check(unavailable.availability() == CapabilityAvailability.OFFLINE);
        check(!AndroidSharingSelection.isSelectable(unavailable));
        check(unavailable.ports().get(0).formats().equals(List.of("platform-negotiated-video-frame")));
        var selection = new AndroidSharingSelection();
        check(!selection.update(registry.snapshot("test-node"), "test-node:android.camera", true));
        check(selection.publicationCatalog(registry.snapshot("test-node")).capabilities().stream()
                .noneMatch(item -> item.capabilityKey().equals("android.camera")));
        selectedFormats.set(List.of("cavc-avc-finite-720p30-v1"));
        CapabilitySnapshot finite = capability(registry.snapshot("test-node"), "android.camera");
        check(finite.ports().get(0).formats().equals(List.of("cavc-avc-finite-720p30-v1")));
        selectedFormats.set(List.of()); // Preflight/source/quality invalidation never falls back.
        unavailable = capability(registry.snapshot("test-node"), "android.camera");
        check(unavailable.availability() == CapabilityAvailability.OFFLINE);
        check(!AndroidSharingSelection.isSelectable(unavailable));
        check(unavailable.ports().get(0).formats().equals(List.of("platform-negotiated-video-frame")));
        selectedFormats.set(List.of("cavc-avc-access-unit-v1"));
        check(capability(registry.snapshot("test-node"), "android.camera").ports().get(0)
                .formats().equals(List.of("cavc-avc-access-unit-v1")));
        for (List<String> invalid : List.of(List.of("unknown"),
                List.of("cavc-avc-access-unit-v1", "cavc-avc-finite-720p30-v1"))) {
            selectedFormats.set(invalid);
            try {
                registry.snapshot("test-node");
                throw new AssertionError("invalid camera format selection accepted");
            } catch (IllegalArgumentException expected) { check(true); }
        }
    }

    private static void attachesAndAdmitsHapticsSink() {
        CapabilityLifecycleController haptics = new CapabilityLifecycleController();
        CapabilityCatalogSnapshot catalog = AndroidCapabilityModules.createRegistry(
                new AudioNodeController(),
                new CapabilityLifecycleController(),
                new GamepadNodeController(),
                new TouchpadNodeController(),
                haptics,
                () -> true,
                () -> true,
                () -> new dev.capyio.android.contract.CapabilityLifecycleSnapshot(
                        CapabilityLifecycleState.STOPPED, 0, null),
                () -> true,
                true,
                true,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT,
                HardwarePresence.PRESENT).snapshot("test-node");
        CapabilitySnapshot sink = capability(catalog, "android.haptics");
        check(sink.adapterAttached());
        check(sink.availability() == CapabilityAvailability.AVAILABLE);
        check(sink.ports().get(0).direction() == CapabilityPortDirection.SINK);
        check(!AndroidSharingSelection.isSelectable(sink));

        check(AndroidCapabilityCommand.parse(
                "capyio.quick-action.android-haptics", "start",
                CapabilityLifecycleState.STOPPED).target()
                == AndroidCapabilityCommand.Target.HAPTICS);
        check(AndroidCapabilityCommand.parse(
                "capyio.quick-action.android-haptics", "stop",
                CapabilityLifecycleState.ACTIVE).operation()
                == AndroidCapabilityCommand.Operation.STOP);
        check(AndroidCapabilityCommand.parse(
                "capyio.quick-action.android-haptics", "start",
                CapabilityLifecycleState.ACTIVE) == null);

        AndroidHapticsCommand rumble = AndroidHapticsCommand.rumble(4, 250, 800, 120);
        check(rumble.generation() == 4 && rumble.highFrequency() == 800);
        check(AndroidHapticsCommand.stop(4).stop());
        boolean rejected = false;
        try {
            AndroidHapticsCommand.rumble(4, 0, 0, 120);
        } catch (IllegalArgumentException expected) {
            rejected = true;
        }
        check(rejected);
        rejected = false;
        try {
            AndroidHapticsCommand.rumble(4, 1, 1, 10_001);
        } catch (IllegalArgumentException expected) {
            rejected = true;
        }
        check(rejected);
    }

    private static void reconstructsTouchpadPointerLifecycle() {
        TouchpadContactSnapshot first = new TouchpadContactSnapshot(0, 100, 200, 512);
        TouchpadContactSnapshot second = new TouchpadContactSnapshot(1, 300, 400, 1024);

        TouchpadMotionFrame down = TouchpadMotionFrame.between(List.of(), List.of(first));
        check(down.action == TouchpadMotionFrame.ACTION_DOWN);
        check(down.actionIndex == 0 && down.ids.length == 1 && down.ids[0] == 0);
        check(down.pressure[0] == 0.5f);

        TouchpadMotionFrame pointerDown =
                TouchpadMotionFrame.between(List.of(first), List.of(first, second));
        check(pointerDown.action == TouchpadMotionFrame.ACTION_POINTER_DOWN);
        check(pointerDown.actionIndex == 1 && pointerDown.ids.length == 2);

        TouchpadMotionFrame move = TouchpadMotionFrame.between(
                List.of(first, second),
                List.of(new TouchpadContactSnapshot(0, 101, 202, 512), second));
        check(move.action == TouchpadMotionFrame.ACTION_MOVE);

        TouchpadMotionFrame pointerUp =
                TouchpadMotionFrame.between(List.of(first, second), List.of(second));
        check(pointerUp.action == TouchpadMotionFrame.ACTION_POINTER_UP);
        check(pointerUp.actionIndex == 0 && pointerUp.ids.length == 2);

        TouchpadMotionFrame up = TouchpadMotionFrame.between(List.of(second), List.of());
        check(up.action == TouchpadMotionFrame.ACTION_UP && up.ids.length == 1);

        TouchpadMotionFrame cancel =
                TouchpadMotionFrame.between(List.of(first, second), List.of());
        check(cancel.action == TouchpadMotionFrame.ACTION_CANCEL && cancel.ids.length == 0);

        TouchpadContactSnapshot third = new TouchpadContactSnapshot(2, 500, 600, 512);
        TouchpadContactSnapshot fourth = new TouchpadContactSnapshot(3, 700, 800, 512);
        List<TouchpadMotionFrame> simultaneous = TouchpadMotionFrame.sequence(
                List.of(), List.of(first, second, third, fourth));
        check(simultaneous.size() == 4);
        check(simultaneous.get(0).action == TouchpadMotionFrame.ACTION_DOWN);
        for (int index = 1; index < simultaneous.size(); index++) {
            check(simultaneous.get(index).action == TouchpadMotionFrame.ACTION_POINTER_DOWN);
            check(simultaneous.get(index).actionIndex == index);
            check(simultaneous.get(index).ids.length == index + 1);
        }
        List<TouchpadMotionFrame> replacement = TouchpadMotionFrame.sequence(
                List.of(first, second), List.of(second, third));
        check(replacement.size() == 3);
        check(replacement.get(0).action == TouchpadMotionFrame.ACTION_CANCEL);
        check(replacement.get(1).action == TouchpadMotionFrame.ACTION_DOWN);
        check(replacement.get(2).action == TouchpadMotionFrame.ACTION_POINTER_DOWN);
        List<TouchpadMotionFrame> simultaneousRelease = TouchpadMotionFrame.sequence(
                List.of(first, second, third, fourth), List.of());
        check(simultaneousRelease.size() == 1
                && simultaneousRelease.get(0).action == TouchpadMotionFrame.ACTION_CANCEL);
    }

    private static void encodesControllerLabDatagram() {
        GamepadControlsSnapshot controls = new GamepadControlsSnapshot(
                1, -1, 1, -12000, 9000, 0, 32767, 123, 456);
        String packet = ControllerPacketJson.encode(
                "c0decafe", "session-1", 7, 99, controls,
                90, 0.0f, 1.0f, 9.7f,
                95, 0.1f, 0.2f, 0.3f);
        check(packet.startsWith("{\"version\":2,"));
        check(packet.contains("\"motionAvailable\":true,\"gamepadMotionEnabled\":true"));
        check(packet.contains("\"sequence\":7,\"timestampNanos\":99"));
        check(packet.contains("\"buttons\":1,\"dpadX\":-1,\"dpadY\":1"));
        check(packet.contains("\"leftX\":-12000,\"leftY\":9000"));
        check(packet.contains("\"acceleration\":[0.0,1.0,9.7]"));
        check(packet.endsWith("\"angularVelocityTimestampNanos\":95}"));
        String absent = ControllerPacketJson.encode("c0decafe", "session-1", 8, 100,
                controls, 0, 0, 0, 0, 0, 0, 0, 0, false);
        check(absent.contains("\"motionAvailable\":false,\"gamepadMotionEnabled\":false"));
        check(absent.contains("\"accelerationTimestampNanos\":0"));
        check(absent.endsWith("\"angularVelocityTimestampNanos\":0}"));
    }

    private static void sharesImuSamplingWithoutStoppingAnotherUser() {
        ImuSamplingUsers users = new ImuSamplingUsers();
        check(!users.inUse());
        users.acquireGamepad(); users.acquireIndependent();
        users.releaseGamepad(); check(users.inUse());
        users.releaseIndependent(); check(!users.inUse());
        users.acquireIndependent(); users.acquireGamepad();
        users.releaseIndependent(); check(users.inUse());
        users.acquireGamepad(); users.releaseGamepad(); check(!users.inUse());
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
