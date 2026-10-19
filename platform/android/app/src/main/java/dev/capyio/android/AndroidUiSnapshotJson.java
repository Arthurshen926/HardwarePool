package dev.capyio.android;

import dev.capyio.android.contract.ActualAudioFormat;
import dev.capyio.android.contract.AudioCapabilitySnapshot;
import dev.capyio.android.contract.AudioCapabilityState;
import dev.capyio.android.contract.AudioNodeSnapshot;
import dev.capyio.android.contract.CapabilityCatalogSnapshot;
import dev.capyio.android.contract.CapabilityLifecycleSnapshot;
import dev.capyio.android.contract.CapabilityLifecycleState;
import dev.capyio.android.contract.CapabilityPortSnapshot;
import dev.capyio.android.contract.CapabilitySnapshot;
import dev.capyio.android.contract.GamepadControlsSnapshot;
import dev.capyio.android.contract.TouchpadContactSnapshot;
import dev.capyio.android.contract.TouchpadControlsSnapshot;

import java.util.List;
import java.util.Locale;
import java.util.Set;

/** Projects the bounded Android-local catalog into the shared UI schema. */
public final class AndroidUiSnapshotJson {
    private static final String NODE_NAME = "CapyIO Android";

    private AndroidUiSnapshotJson() {}

    public static String encode(
            CapabilityCatalogSnapshot catalog,
            String platformVersion,
            String projectVersion) {
        return encode(catalog, platformVersion, projectVersion, Set.of());
    }

    public static String encode(
            CapabilityCatalogSnapshot catalog,
            String platformVersion,
            String projectVersion,
            Set<String> selectedCapabilityIds) {
        return encode(catalog, platformVersion, projectVersion, selectedCapabilityIds,
                "not_configured", 0);
    }

    public static String encode(
            CapabilityCatalogSnapshot catalog,
            String platformVersion,
            String projectVersion,
            Set<String> selectedCapabilityIds,
            String pairingState,
            int pairedPeerCount) {
        return encode(catalog, platformVersion, projectVersion, selectedCapabilityIds,
                pairingState, pairedPeerCount, false, false, 0);
    }

    public static String encode(
            CapabilityCatalogSnapshot catalog,
            String platformVersion,
            String projectVersion,
            Set<String> selectedCapabilityIds,
            String pairingState,
            int pairedPeerCount,
            boolean discoveryAnnouncing,
            boolean authenticatedCatalogConnected) {
        return encode(catalog, platformVersion, projectVersion, selectedCapabilityIds,
                pairingState, pairedPeerCount, discoveryAnnouncing,
                authenticatedCatalogConnected, 0);
    }

    public static String encode(
            CapabilityCatalogSnapshot catalog,
            String platformVersion,
            String projectVersion,
            Set<String> selectedCapabilityIds,
            String pairingState,
            int pairedPeerCount,
            boolean discoveryAnnouncing,
            boolean authenticatedCatalogConnected,
            int controlListenPort) {
        if (catalog == null) {
            return encodeDisconnected(platformVersion, projectVersion);
        }
        if (selectedCapabilityIds == null) {
            throw new IllegalArgumentException("Selected Capability IDs are required");
        }
        if (!("not_configured".equals(pairingState)
                || "ready".equals(pairingState)
                || "failed".equals(pairingState))) {
            throw new IllegalArgumentException("Pairing store state is invalid");
        }
        if (pairedPeerCount < 0 || pairedPeerCount > 64
                || (!"ready".equals(pairingState) && pairedPeerCount != 0)) {
            throw new IllegalArgumentException("Paired peer count is invalid");
        }
        if (controlListenPort < 0 || controlListenPort > 65_535) {
            throw new IllegalArgumentException("Control listen port is invalid");
        }
        StringBuilder json = new StringBuilder(4096);
        json.append('{');
        appendStringField(json, "backendMode", "android_node");
        json.append(",\"schemaVersion\":5");
        appendStringField(json, "projectVersion", requireText(projectVersion, "project version"));
        appendControlPlane(json, pairingState, pairedPeerCount, discoveryAnnouncing,
                authenticatedCatalogConnected, controlListenPort);
        json.append(",\"nodes\":[{");
        appendStringField(json, "id", catalog.nodeId());
        appendStringField(json, "displayName", NODE_NAME);
        appendStringField(json, "platform", "android");
        appendStringField(json, "platformVersion", requireText(platformVersion, "platform version"));
        json.append(",\"online\":true,\"local\":true,\"capabilityCount\":")
                .append(catalog.capabilities().size()).append("}]");
        json.append(",\"capabilities\":[");
        appendCapabilities(json, catalog.nodeId(), catalog.capabilities(), selectedCapabilityIds);
        json.append(']');
        appendEmptyCollections(json);
        json.append(",\"warnings\":[");
        appendQuoted(json, "Android shared UI controls local audio, camera, IMU, gamepad, haptics and touchpad lifecycles plus bounded input state.");
        json.append(',');
        appendQuoted(json, "Sharing selection controls the authenticated catalog; hardware data Routes remain capability-specific.");
        json.append("],\"imuFixture\":null}");
        return json.toString();
    }

    public static String encodeDisconnected(String platformVersion, String projectVersion) {
        StringBuilder json = new StringBuilder(768);
        json.append('{');
        appendStringField(json, "backendMode", "android_node");
        json.append(",\"schemaVersion\":5");
        appendStringField(json, "projectVersion", requireText(projectVersion, "project version"));
        appendControlPlane(json, "not_configured", 0, false, false, 0);
        json.append(",\"nodes\":[{");
        appendStringField(json, "id", "android.pending");
        appendStringField(json, "displayName", NODE_NAME);
        appendStringField(json, "platform", "android");
        appendStringField(json, "platformVersion", requireText(platformVersion, "platform version"));
        json.append(",\"online\":false,\"local\":true,\"capabilityCount\":0}]");
        json.append(",\"capabilities\":[]");
        appendEmptyCollections(json);
        json.append(",\"warnings\":[");
        appendQuoted(json, "Android Node service is not connected; the catalog is temporarily unavailable.");
        json.append("],\"imuFixture\":null}");
        return json.toString();
    }

    public static String encodeQuickActions(
            CapabilityLifecycleSnapshot imu,
            CapabilityLifecycleSnapshot gamepad,
            CapabilityLifecycleSnapshot touchpad) {
        if (imu == null || gamepad == null || touchpad == null) {
            throw new IllegalArgumentException("Capability lifecycle snapshots are required");
        }
        StringBuilder json = new StringBuilder(1536);
        json.append('[');
        appendQuickAction(
                json,
                "capyio.quick-action.android-imu",
                "IMU sensors",
                "Start or stop the service-owned accelerometer and gyroscope Adapter.",
                imu);
        json.append(',');
        appendQuickAction(
                json,
                "capyio.quick-action.android-gamepad",
                "Touch gamepad",
                "Start or stop the service-owned bounded gamepad state Adapter.",
                gamepad);
        json.append(',');
        appendQuickAction(
                json,
                "capyio.quick-action.android-touchpad",
                "Touchpad surface",
                "Start or stop the service-owned complete multi-touch state Adapter.",
                touchpad);
        json.append(']');
        return json.toString();
    }

    public static String encodeQuickActions(
            AudioNodeSnapshot audio,
            boolean microphonePermissionReady,
            boolean speakerPermissionReady,
            CapabilityLifecycleSnapshot imu,
            CapabilityLifecycleSnapshot gamepad,
            CapabilityLifecycleSnapshot touchpad) {
        return encodeQuickActions(
                audio,
                microphonePermissionReady,
                speakerPermissionReady,
                new CapabilityLifecycleSnapshot(CapabilityLifecycleState.STOPPED, 0, null),
                false,
                imu,
                gamepad,
                touchpad,
                new CapabilityLifecycleSnapshot(CapabilityLifecycleState.STOPPED, 0, null));
    }

    public static String encodeQuickActions(
            AudioNodeSnapshot audio,
            boolean microphonePermissionReady,
            boolean speakerPermissionReady,
            CapabilityLifecycleSnapshot camera,
            boolean cameraPermissionReady,
            CapabilityLifecycleSnapshot imu,
            CapabilityLifecycleSnapshot gamepad,
            CapabilityLifecycleSnapshot touchpad,
            CapabilityLifecycleSnapshot haptics) {
        if (audio == null || camera == null || imu == null || gamepad == null
                || touchpad == null || haptics == null) {
            throw new IllegalArgumentException("Audio and input snapshots are required");
        }
        StringBuilder json = new StringBuilder(3584);
        json.append('[');
        appendAudioQuickAction(
                json,
                "capyio.quick-action.android-microphone",
                "Phone microphone",
                "Capture microphone audio in the existing foreground Node service.",
                audio.microphone(),
                microphonePermissionReady);
        json.append(',');
        appendAudioQuickAction(
                json,
                "capyio.quick-action.android-speaker",
                "Phone speaker",
                "Play received audio through the existing foreground Node service.",
                audio.speaker(),
                speakerPermissionReady);
        json.append(',');
        appendCameraQuickAction(json, camera, cameraPermissionReady);
        json.append(',');
        appendQuickAction(json, "capyio.quick-action.android-imu", "IMU sensors",
                "Start or stop the service-owned accelerometer and gyroscope Adapter.", imu);
        json.append(',');
        appendQuickAction(json, "capyio.quick-action.android-gamepad", "Touch gamepad",
                "Start or stop the service-owned bounded gamepad state Adapter.", gamepad);
        json.append(',');
        appendQuickAction(json, "capyio.quick-action.android-touchpad", "Touchpad surface",
                "Publish the touchpad, then accept a Windows resource request to start its secure Route.", touchpad);
        json.append(',');
        appendQuickAction(json, "capyio.quick-action.android-haptics", "Phone vibration",
                "Start the service-owned feedback Sink with a short confirmation pulse.", haptics);
        json.append(']');
        return json.toString();
    }

    private static void appendCameraQuickAction(
            StringBuilder json,
            CapabilityLifecycleSnapshot camera,
            boolean permissionReady) {
        json.append("{\"schemaVersion\":2");
        appendStringField(json, "id", "capyio.quick-action.android-camera");
        appendStringField(json, "kind", "capability_control");
        appendStringField(json, "title", "Phone camera");
        appendStringField(json, "summary", "Capture bounded Camera2 AVC in a dedicated foreground service.");
        appendStringField(json, "status", !permissionReady
                && (camera.state() == CapabilityLifecycleState.STOPPED
                || camera.state() == CapabilityLifecycleState.FAILED)
                ? "blocked"
                : actionStatus(camera.state()));
        json.append(",\"simulated\":false,\"routeId\":null,\"routeState\":null")
                .append(",\"routeEpoch\":null,\"availableOperations\":[");
        String operation = availableOperation(camera.state());
        if (operation != null) {
            appendQuoted(json, operation);
        }
        json.append(']');
        appendStringField(json, "evidenceLevel", "not_started");
        appendStringField(json, "connectionHint",
                "Camera permission and foreground capture remain Android-owned; Route transport is separate.");
        if (!permissionReady
                && (camera.state() == CapabilityLifecycleState.STOPPED
                || camera.state() == CapabilityLifecycleState.FAILED)) {
            appendStringField(json, "problemCode", "CAPY.ANDROID.PERMISSION_REQUIRED");
            appendStringField(json, "problem", "Android camera permission is required on Start.");
            json.append('}');
        } else if (camera.problemCode() != null) {
            appendStringField(json, "problemCode", camera.problemCode());
            appendStringField(json, "problem", "Android Camera Adapter reported a bounded failure.");
            json.append('}');
        } else {
            json.append(",\"problemCode\":null,\"problem\":null}");
        }
    }

    public static String encodeAudioState(
            AudioNodeSnapshot audio,
            boolean microphonePermissionReady,
            boolean speakerPermissionReady) {
        if (audio == null) {
            throw new IllegalArgumentException("Audio snapshot is required");
        }
        StringBuilder json = new StringBuilder(1024);
        json.append("{\"schemaVersion\":1");
        appendStringField(json, "nodeId", audio.nodeId());
        json.append(",\"microphone\":");
        appendAudioState(json, audio.microphone(), microphonePermissionReady);
        json.append(",\"speaker\":");
        appendAudioState(json, audio.speaker(), speakerPermissionReady);
        json.append('}');
        return json.toString();
    }

    private static void appendAudioState(
            StringBuilder json,
            AudioCapabilitySnapshot audio,
            boolean permissionReady) {
        json.append('{');
        appendStringField(json, "state", lower(audio.state()));
        json.append(",\"generation\":").append(audio.generation())
                .append(",\"framesProcessed\":").append(audio.framesProcessed())
                .append(",\"permissionReady\":").append(permissionReady)
                .append(",\"actualFormat\":");
        ActualAudioFormat format = audio.actualFormat();
        if (format == null) {
            json.append("null");
        } else {
            json.append("{\"sampleRateHz\":").append(format.sampleRateHz())
                    .append(",\"channelCount\":").append(format.channelCount());
            appendStringField(json, "encoding", format.encoding());
            json.append(",\"framesPerBuffer\":").append(format.framesPerBuffer())
                    .append('}');
        }
        if (audio.problemCode() == null) {
            json.append(",\"problemCode\":null}");
        } else {
            appendStringField(json, "problemCode", audio.problemCode());
            json.append('}');
        }
    }

    private static void appendAudioQuickAction(
            StringBuilder json,
            String id,
            String title,
            String summary,
            AudioCapabilitySnapshot audio,
            boolean permissionReady) {
        json.append("{\"schemaVersion\":2");
        appendStringField(json, "id", id);
        appendStringField(json, "kind", "capability_control");
        appendStringField(json, "title", title);
        appendStringField(json, "summary", summary);
        appendStringField(json, "status", audioActionStatus(audio.state(), permissionReady));
        json.append(",\"simulated\":false,\"routeId\":null,\"routeState\":null")
                .append(",\"routeEpoch\":null,\"availableOperations\":[");
        String operation = audioAvailableOperation(audio.state());
        if (operation != null) {
            appendQuoted(json, operation);
        }
        json.append(']');
        appendStringField(json, "evidenceLevel", "not_started");
        appendStringField(json, "connectionHint",
                "Local Android audio lifecycle; permission and foreground state remain platform-owned.");
        if (!permissionReady
                && (audio.state() == AudioCapabilityState.STOPPED
                || audio.state() == AudioCapabilityState.FAILED)) {
            appendStringField(json, "problemCode", "CAPY.ANDROID.PERMISSION_REQUIRED");
            appendStringField(json, "problem", "Android system permission is required on Start.");
            json.append('}');
        } else if (audio.problemCode() != null) {
            appendStringField(json, "problemCode", audio.problemCode());
            appendStringField(json, "problem", "Android audio Adapter reported a bounded failure.");
            json.append('}');
        } else {
            json.append(",\"problemCode\":null,\"problem\":null}");
        }
    }

    private static String audioActionStatus(
            AudioCapabilityState state, boolean permissionReady) {
        if (!permissionReady && (state == AudioCapabilityState.STOPPED
                || state == AudioCapabilityState.FAILED)) {
            return "blocked";
        }
        return switch (state) {
            case STOPPED -> "idle";
            case STARTING -> "starting";
            case ACTIVE -> "active";
            case STOPPING -> "stopping";
            case FAILED -> "failed";
        };
    }

    private static String audioAvailableOperation(AudioCapabilityState state) {
        return switch (state) {
            case STOPPED -> "start";
            case STARTING, ACTIVE -> "stop";
            case STOPPING -> null;
            case FAILED -> "retry";
        };
    }

    public static String encodeGamepadState(
            CapabilityLifecycleSnapshot lifecycle,
            GamepadControlsSnapshot controls) {
        return encodeGamepadState(lifecycle, controls, null, false, null);
    }

    public static String encodeGamepadState(
            CapabilityLifecycleSnapshot lifecycle,
            GamepadControlsSnapshot controls,
            String controllerEndpoint,
            boolean controllerRouteRunning,
            String controllerProblemCode) {
        if (lifecycle == null || controls == null) {
            throw new IllegalArgumentException("Gamepad snapshots are required");
        }
        StringBuilder json = new StringBuilder(2048);
        json.append("{\"schemaVersion\":1,\"source\":\"android_touch\"")
                .append(",\"simulated\":false")
                .append(",\"profile\":\"capyio.input.gamepad-state/1\"")
                .append(",\"streamEpoch\":").append(lifecycle.generation())
                .append(",\"sequence\":null,\"sourceTimestampNanos\":null")
                .append(",\"pressedButtons\":[");
        appendPressedButtons(json, controls.buttons());
        json.append("]")
                .append(",\"dpad\":{\"x\":").append(controls.dpadX())
                .append(",\"y\":").append(controls.dpadY()).append('}')
                .append(",\"leftStick\":{\"x\":").append(controls.leftX())
                .append(",\"y\":").append(controls.leftY()).append('}')
                .append(",\"rightStick\":{\"x\":").append(controls.rightX())
                .append(",\"y\":").append(controls.rightY()).append('}')
                .append(",\"leftTrigger\":").append(controls.leftTrigger())
                .append(",\"rightTrigger\":").append(controls.rightTrigger());
        appendStringField(json, "lastUpdate", "android.lifecycle."
                + lower(lifecycle.state()));
        json.append(",\"dsuProjection\":{\"supported\":false,\"status\":\"unsupported\"")
                .append(",\"endpoint\":null,\"mode\":\"motion_only\"")
                .append(",\"lastSubmit\":\"not_attached\",\"controlsSubmitted\":0")
                .append(",\"controlsAccepted\":0,\"controlsQueueFull\":0")
                .append(",\"controlsNeutralResets\":0,\"activeSubscribers\":0")
                .append(",\"padPacketsSent\":0,\"packetSendErrors\":0}")
                .append(",\"windowsProjection\":{\"supported\":false")
                .append(",\"status\":\"unsupported\",\"controllerKind\":\"dualshock4\"")
                .append(",\"deviceIdentity\":\"Not attached on Android\"")
                .append(",\"viiperEndpoint\":null,\"usbipEndpoint\":null")
                .append(",\"viiperReady\":false,\"usbipReady\":false")
                .append(",\"xinputAvailable\":false,\"xinputReady\":false")
                .append(",\"exportCount\":0,\"busId\":null,\"ownedUsbipPort\":null")
                .append(",\"inputPackets\":0,\"nonNeutralPackets\":0")
                .append(",\"ds4RejectedPackets\":0,\"xinputPackets\":0")
                .append(",\"inputOfflineEvents\":0,\"lastRemoteSequence\":null")
                .append(",\"lastEvent\":\"not_attached\",\"problemCode\":null")
                .append(",\"problem\":null}")
                .append(",\"androidInput\":{\"supported\":true");
        appendStringField(json, "status", controllerRouteRunning
                ? "connected"
                : controllerProblemCode != null ? "failed" : gamepadStatus(lifecycle.state()));
        if (controllerEndpoint != null) {
            appendStringField(json, "endpoint", "udp://" + controllerEndpoint);
        } else {
            json.append(",\"endpoint\":null");
        }
        json.append(",\"lanHostHint\":null,\"pairingToken\":null")
                .append(",\"peerConnected\":false,\"acceptedPackets\":0")
                .append(",\"rejectedPackets\":0,\"replayedPackets\":0")
                .append(",\"peerTimeouts\":0,\"projectionQueueFull\":0")
                .append(",\"packetAgeMillis\":null,\"remoteSequence\":null");
        appendStringField(json, "lastEvent", controllerRouteRunning
                ? "android.controller.streaming"
                : controllerProblemCode != null
                        ? controllerProblemCode
                        : "android.lifecycle." + lower(lifecycle.state()));
        json.append("},\"motion\":{\"source\":\"unavailable\"")
                .append(",\"sourceTimestampNanos\":null")
                .append(",\"acceleration\":[0,0,0],\"angularVelocity\":[0,0,0]}")
                .append('}');
        return json.toString();
    }

    public static String encodeTouchpadState(
            CapabilityLifecycleSnapshot lifecycle,
            TouchpadControlsSnapshot controls) {
        if (lifecycle == null || controls == null) {
            throw new IllegalArgumentException("Touchpad snapshots are required");
        }
        StringBuilder json = new StringBuilder(768);
        json.append("{\"schemaVersion\":1")
                .append(",\"source\":\"android_shared_ui\"")
                .append(",\"profile\":\"capyio.input.touchpad-frames/1\"")
                .append(",\"streamEpoch\":").append(lifecycle.generation())
                .append(",\"sequence\":").append(controls.sequence());
        appendStringField(json, "status", actionStatus(lifecycle.state()));
        json.append(",\"contacts\":[");
        for (int index = 0; index < controls.contacts().size(); index++) {
            if (index > 0) {
                json.append(',');
            }
            TouchpadContactSnapshot contact = controls.contacts().get(index);
            json.append("{\"contactId\":").append(contact.contactId())
                    .append(",\"x\":").append(contact.x())
                    .append(",\"y\":").append(contact.y())
                    .append(",\"pressure\":").append(contact.pressure()).append('}');
        }
        json.append("]}");
        return json.toString();
    }

    private static void appendPressedButtons(StringBuilder json, int buttons) {
        String[] names = {"south", "east", "west", "north", "left_shoulder",
                "right_shoulder", "left_stick", "right_stick", "select", "start", "guide"};
        boolean first = true;
        for (int bit = 0; bit < names.length; bit++) {
            if ((buttons & (1 << bit)) == 0) {
                continue;
            }
            if (!first) {
                json.append(',');
            }
            appendQuoted(json, names[bit]);
            first = false;
        }
    }

    private static String gamepadStatus(CapabilityLifecycleState state) {
        return switch (state) {
            case STOPPED, STOPPING -> "stopped";
            case STARTING -> "listening";
            case ACTIVE -> "connected";
            case FAILED -> "failed";
        };
    }

    private static void appendQuickAction(
            StringBuilder json,
            String id,
            String title,
            String summary,
            CapabilityLifecycleSnapshot lifecycle) {
        json.append("{\"schemaVersion\":2");
        appendStringField(json, "id", id);
        appendStringField(json, "kind", "capability_control");
        appendStringField(json, "title", title);
        appendStringField(json, "summary", summary);
        appendStringField(json, "status", actionStatus(lifecycle.state()));
        json.append(",\"simulated\":false,\"routeId\":null,\"routeState\":null")
                .append(",\"routeEpoch\":null,\"availableOperations\":[");
        boolean touchpad = "capyio.quick-action.android-touchpad".equals(id);
        String operation = touchpad
                ? switch (lifecycle.state()) {
                    case STARTING, ACTIVE, FAILED -> "stop";
                    case STOPPED, STOPPING -> null;
                }
                : availableOperation(lifecycle.state());
        if (operation != null) {
            appendQuoted(json, operation);
        }
        json.append(']');
        appendStringField(json, "evidenceLevel", "not_started");
        appendStringField(json, "connectionHint", touchpad
                ? "勾选本机触控板共享后，从 Windows 的远程资源申请使用；此处不启动旧 ADB 反向接收器。"
                : "Local Android Capability lifecycle only; sharing and Routes remain separate.");
        if (lifecycle.problemCode() == null) {
            json.append(",\"problemCode\":null,\"problem\":null}");
        } else {
            appendStringField(json, "problemCode", lifecycle.problemCode());
            appendStringField(json, "problem", touchpad
                    ? "上一次触控链路失败。可清除错误，再从 Windows 重新申请；本机屏幕仍可共享。"
                    : "CAPY.ANDROID.CONTROLLER_QUEUE_FULL".equals(lifecycle.problemCode())
                            ? "手柄输入发送队列已满，已停止输入。请重新启动手柄；如再次出现，请保留此错误码。"
                            : "Android Adapter reported a bounded failure.");
            json.append('}');
        }
    }

    private static String actionStatus(CapabilityLifecycleState state) {
        return switch (state) {
            case STOPPED -> "idle";
            case STARTING -> "starting";
            case ACTIVE -> "active";
            case STOPPING -> "stopping";
            case FAILED -> "failed";
        };
    }

    private static String availableOperation(CapabilityLifecycleState state) {
        return switch (state) {
            case STOPPED -> "start";
            case STARTING, ACTIVE -> "stop";
            case STOPPING -> null;
            case FAILED -> "retry";
        };
    }

    private static void appendCapabilities(
            StringBuilder json,
            String nodeId,
            List<CapabilitySnapshot> capabilities,
            Set<String> selectedCapabilityIds) {
        for (int capabilityIndex = 0; capabilityIndex < capabilities.size(); capabilityIndex++) {
            if (capabilityIndex > 0) {
                json.append(',');
            }
            CapabilitySnapshot capability = capabilities.get(capabilityIndex);
            String capabilityId = nodeId + ":" + capability.capabilityKey();
            json.append('{');
            appendStringField(json, "id", capabilityId);
            appendStringField(json, "nodeId", nodeId);
            appendStringField(json, "nodeName", NODE_NAME);
            appendStringField(json, "displayName", capability.displayName());
            appendStringField(json, "capabilityClass", lower(capability.kind()));
            json.append(",\"local\":true");
            appendStringField(json, "availability", lower(capability.availability()));
            appendStringField(json, "permissionRequirement", lower(capability.permissionRequirement()));
            appendStringField(json, "inventorySource", "android_node_catalog");
            json.append(",\"simulated\":false,\"selectableForSharing\":")
                    .append(AndroidSharingSelection.isSelectable(capability))
                    .append(",\"selectedForSharing\":")
                    .append(selectedCapabilityIds.contains(capabilityId))
                    .append(",\"ports\":[");
            appendPorts(json, nodeId, capability);
            json.append("]}");
        }
    }

    private static void appendPorts(
            StringBuilder json,
            String nodeId,
            CapabilitySnapshot capability) {
        List<CapabilityPortSnapshot> ports = capability.ports();
        for (int portIndex = 0; portIndex < ports.size(); portIndex++) {
            if (portIndex > 0) {
                json.append(',');
            }
            CapabilityPortSnapshot port = ports.get(portIndex);
            json.append('{');
            appendStringField(json, "id", nodeId + ":" + capability.capabilityKey()
                    + ":" + port.portKey());
            appendStringField(json, "displayName", port.displayName());
            appendStringField(json, "direction", lower(port.direction()));
            appendStringField(json, "profile", port.profile());
            json.append(",\"formats\":[");
            for (int formatIndex = 0; formatIndex < port.formats().size(); formatIndex++) {
                if (formatIndex > 0) {
                    json.append(',');
                }
                appendQuoted(json, port.formats().get(formatIndex));
            }
            json.append("]}");
        }
    }

    private static void appendEmptyCollections(StringBuilder json) {
        json.append(",\"routes\":[],\"adapters\":[],\"events\":[]");
    }

    private static void appendControlPlane(
            StringBuilder json,
            String pairingState,
            int pairedPeerCount,
            boolean discoveryAnnouncing,
            boolean authenticatedCatalogConnected,
            int controlListenPort) {
        json.append(",\"controlPlane\":{")
                .append("\"schemaVersion\":1")
                .append(",\"simulated\":false")
                .append(",\"discoveryState\":\"")
                .append(discoveryAnnouncing ? "listening" : "not_started")
                .append('"')
                .append(",\"pairingState\":");
        appendQuoted(json, pairingState);
        json.append(",\"transportState\":\"")
                .append(authenticatedCatalogConnected ? "connected" : "not_connected")
                .append('"')
                .append(",\"catalogExchangeState\":\"")
                .append(authenticatedCatalogConnected ? "active" : "not_connected")
                .append('"')
                .append(",\"discoveredCandidateCount\":0")
                .append(",\"pairedPeerCount\":").append(pairedPeerCount)
                .append(",\"authenticatedConnectionCount\":")
                .append(authenticatedCatalogConnected ? 1 : 0)
                .append(",\"listenPort\":");
        if (controlListenPort == 0) json.append("null");
        else json.append(controlListenPort);
        json.append('}');
    }

    private static void appendStringField(StringBuilder json, String name, String value) {
        if (json.charAt(json.length() - 1) != '{') {
            json.append(',');
        }
        appendQuoted(json, name);
        json.append(':');
        appendQuoted(json, value);
    }

    static String encodeStringForContractTest(String value) {
        StringBuilder json = new StringBuilder();
        appendQuoted(json, value);
        return json.toString();
    }

    private static void appendQuoted(StringBuilder json, String value) {
        json.append('"');
        for (int index = 0; index < value.length(); index++) {
            char character = value.charAt(index);
            switch (character) {
                case '"' -> json.append("\\\"");
                case '\\' -> json.append("\\\\");
                case '\b' -> json.append("\\b");
                case '\f' -> json.append("\\f");
                case '\n' -> json.append("\\n");
                case '\r' -> json.append("\\r");
                case '\t' -> json.append("\\t");
                default -> {
                    if (character < 0x20 || Character.isSurrogate(character)) {
                        json.append("\\u");
                        appendHex4(json, character);
                    } else {
                        json.append(character);
                    }
                }
            }
        }
        json.append('"');
    }

    private static void appendHex4(StringBuilder json, char value) {
        char[] digits = "0123456789abcdef".toCharArray();
        json.append(digits[(value >>> 12) & 0x0f]);
        json.append(digits[(value >>> 8) & 0x0f]);
        json.append(digits[(value >>> 4) & 0x0f]);
        json.append(digits[value & 0x0f]);
    }

    private static String lower(Enum<?> value) {
        return value.name().toLowerCase(Locale.ROOT);
    }

    private static String requireText(String value, String field) {
        if (value == null || value.isBlank() || value.length() > 128) {
            throw new IllegalArgumentException(field + " is outside the supported bound");
        }
        return value;
    }
}
