package dev.capyio.android;

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

/** Projects the bounded Android-local catalog into the shared UI schema. */
public final class AndroidUiSnapshotJson {
    private static final String NODE_NAME = "CapyIO Android";

    private AndroidUiSnapshotJson() {}

    public static String encode(
            CapabilityCatalogSnapshot catalog,
            String platformVersion,
            String projectVersion) {
        if (catalog == null) {
            return encodeDisconnected(platformVersion, projectVersion);
        }
        StringBuilder json = new StringBuilder(4096);
        json.append('{');
        appendStringField(json, "backendMode", "android_observer");
        json.append(",\"schemaVersion\":4");
        appendStringField(json, "projectVersion", requireText(projectVersion, "project version"));
        json.append(",\"nodes\":[{");
        appendStringField(json, "id", catalog.nodeId());
        appendStringField(json, "displayName", NODE_NAME);
        appendStringField(json, "platform", "android");
        appendStringField(json, "platformVersion", requireText(platformVersion, "platform version"));
        json.append(",\"online\":true,\"local\":true,\"capabilityCount\":")
                .append(catalog.capabilities().size()).append("}]");
        json.append(",\"capabilities\":[");
        appendCapabilities(json, catalog.nodeId(), catalog.capabilities());
        json.append(']');
        appendEmptyCollections(json);
        json.append(",\"warnings\":[");
        appendQuoted(json, "Android shared UI controls local IMU, gamepad and touchpad lifecycles and input state.");
        json.append(',');
        appendQuoted(json, "Sharing selection, Routes, and remote catalog exchange are not attached yet.");
        json.append("],\"imuFixture\":null}");
        return json.toString();
    }

    public static String encodeDisconnected(String platformVersion, String projectVersion) {
        StringBuilder json = new StringBuilder(768);
        json.append('{');
        appendStringField(json, "backendMode", "android_observer");
        json.append(",\"schemaVersion\":4");
        appendStringField(json, "projectVersion", requireText(projectVersion, "project version"));
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

    public static String encodeGamepadState(
            CapabilityLifecycleSnapshot lifecycle,
            GamepadControlsSnapshot controls) {
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
        appendStringField(json, "status", gamepadStatus(lifecycle.state()));
        if (lifecycle.state() == CapabilityLifecycleState.ACTIVE) {
            appendStringField(json, "endpoint", "local://shared-ui");
        } else {
            json.append(",\"endpoint\":null");
        }
        json.append(",\"lanHostHint\":null,\"pairingToken\":null")
                .append(",\"peerConnected\":false,\"acceptedPackets\":0")
                .append(",\"rejectedPackets\":0,\"replayedPackets\":0")
                .append(",\"peerTimeouts\":0,\"projectionQueueFull\":0")
                .append(",\"packetAgeMillis\":null,\"remoteSequence\":null");
        appendStringField(json, "lastEvent", "android.lifecycle." + lower(lifecycle.state()));
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
        String operation = availableOperation(lifecycle.state());
        if (operation != null) {
            appendQuoted(json, operation);
        }
        json.append(']');
        appendStringField(json, "evidenceLevel", "not_started");
        appendStringField(
                json,
                "connectionHint",
                "Local Android Capability lifecycle only; sharing and Routes remain separate.");
        if (lifecycle.problemCode() == null) {
            json.append(",\"problemCode\":null,\"problem\":null}");
        } else {
            appendStringField(json, "problemCode", lifecycle.problemCode());
            appendStringField(json, "problem", "Android Adapter reported a bounded failure.");
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
            List<CapabilitySnapshot> capabilities) {
        for (int capabilityIndex = 0; capabilityIndex < capabilities.size(); capabilityIndex++) {
            if (capabilityIndex > 0) {
                json.append(',');
            }
            CapabilitySnapshot capability = capabilities.get(capabilityIndex);
            json.append('{');
            appendStringField(json, "id", nodeId + ":" + capability.capabilityKey());
            appendStringField(json, "nodeId", nodeId);
            appendStringField(json, "nodeName", NODE_NAME);
            appendStringField(json, "displayName", capability.displayName());
            appendStringField(json, "capabilityClass", lower(capability.kind()));
            json.append(",\"local\":true");
            appendStringField(json, "availability", lower(capability.availability()));
            appendStringField(json, "permissionRequirement", lower(capability.permissionRequirement()));
            appendStringField(json, "inventorySource", "android_node_catalog");
            json.append(",\"simulated\":false,\"selectableForSharing\":false")
                    .append(",\"selectedForSharing\":false,\"ports\":[");
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
