package dev.capyio.android;

import dev.capyio.android.contract.CapabilityCatalogSnapshot;
import dev.capyio.android.contract.CapabilityPortSnapshot;
import dev.capyio.android.contract.CapabilitySnapshot;

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
        appendQuoted(json, "Android shared UI is a local read-only observer in A01C2.");
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
