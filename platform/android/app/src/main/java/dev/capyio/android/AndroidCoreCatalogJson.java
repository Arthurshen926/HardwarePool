package dev.capyio.android;

import dev.capyio.android.contract.CapabilityCatalogSnapshot;
import dev.capyio.android.contract.CapabilityPortSnapshot;
import dev.capyio.android.contract.CapabilitySnapshot;

import java.util.Locale;

/** Encodes the closed schema-v1 handoff consumed by the Rust Android host. */
public final class AndroidCoreCatalogJson {
    private static final int SCHEMA_VERSION = 1;
    private static final String NODE_NAME = "CapyIO Android";
    private static final String ADAPTER_VERSION = "1";

    private AndroidCoreCatalogJson() {}

    public static String encode(
            CapabilityCatalogSnapshot catalog,
            String platformVersion,
            String runtimeVersion) {
        return encode(catalog, platformVersion, runtimeVersion, null);
    }

    static String encode(CapabilityCatalogSnapshot catalog, String platformVersion,
            String runtimeVersion, AndroidSpeakerSettings.Selection speakerSelection) {
        if (catalog == null || platformVersion == null || runtimeVersion == null) {
            throw new IllegalArgumentException("catalog and version fields are required");
        }

        StringBuilder json = new StringBuilder(4096);
        json.append("{\"schema_version\":").append(SCHEMA_VERSION);
        appendStringField(json, "node_id", catalog.nodeId());
        appendStringField(json, "display_name", NODE_NAME);
        appendStringField(json, "platform_version", platformVersion);
        appendStringField(json, "runtime_version", runtimeVersion);
        json.append(",\"capabilities\":[");
        for (int index = 0; index < catalog.capabilities().size(); index++) {
            if (index > 0) {
                json.append(',');
            }
            appendCapability(json, catalog.capabilities().get(index), speakerSelection);
        }
        return json.append("]}").toString();
    }

    private static void appendCapability(StringBuilder json, CapabilitySnapshot capability, AndroidSpeakerSettings.Selection speakerSelection) {
        CatalogPolicy policy = policyFor(capability.capabilityKey());
        json.append('{');
        appendStringField(json, "key", capability.capabilityKey());
        appendStringField(json, "display_name", capability.displayName());
        appendStringField(json, "class", lower(capability.kind()));
        appendStringField(json, "availability", lower(capability.availability()));
        appendStringField(
                json, "permission_requirement", lower(capability.permissionRequirement()));
        if ("android.speaker".equals(capability.capabilityKey()) && speakerSelection != null) {
            json.append(",\"speaker_selection\":{\"profile\":");
            appendQuoted(json, speakerSelection.profile().id);
            json.append(",\"revision\":").append(speakerSelection.revision()).append('}');
        }
        json.append(",\"adapter_attached\":").append(capability.adapterAttached());
        appendStringField(json, "adapter_type", policy.adapterType());
        appendStringField(json, "adapter_version", ADAPTER_VERSION);
        json.append(",\"supported_route_modes\":[");
        appendQuoted(json, policy.routeMode());
        json.append("],\"ports\":[");
        for (int index = 0; index < capability.ports().size(); index++) {
            if (index > 0) {
                json.append(',');
            }
            appendPort(json, capability.ports().get(index), policy);
        }
        json.append("]}");
    }

    private static void appendPort(
            StringBuilder json, CapabilityPortSnapshot port, CatalogPolicy policy) {
        json.append('{');
        appendStringField(json, "key", port.portKey());
        appendStringField(json, "display_name", port.displayName());
        appendStringField(json, "direction", lower(port.direction()));
        appendStringField(json, "profile", port.profile());
        json.append(",\"formats\":[");
        for (int index = 0; index < port.formats().size(); index++) {
            if (index > 0) {
                json.append(',');
            }
            appendQuoted(json, port.formats().get(index));
        }
        json.append("],\"qos_modes\":[");
        appendQuoted(json, policy.qosMode());
        json.append(']');
        if (policy.clockDomain() == null) {
            json.append(",\"clock_domain\":null");
        } else {
            appendStringField(json, "clock_domain", policy.clockDomain());
        }
        appendStringField(json, "interoperability", policy.interoperability());
        json.append('}');
    }

    private static CatalogPolicy policyFor(String capabilityKey) {
        return switch (capabilityKey) {
            case "android.microphone", "android.speaker" -> new CatalogPolicy(
                    "dev.capyio.android.native-audio",
                    "adapter_managed",
                    "interactive",
                    "android.audio.frame_position",
                    "adapter_managed");
            case "android.camera" -> new CatalogPolicy(
                    "dev.capyio.android.camera2-avc",
                    "adapter_managed",
                    "interactive",
                    "android.camera.sensor_timestamp",
                    "adapter_managed");
            case "android.touchpad" -> new CatalogPolicy(
                    "dev.capyio.android.touchpad-private",
                    "adapter_managed",
                    "interactive",
                    "android.uptime_nanos",
                    "adapter_managed");
            case "android.imu" -> new CatalogPolicy(
                    "dev.capyio.android.imu-local",
                    "external_protocol",
                    "measurement",
                    "android.sensor.elapsed_realtime",
                    "standard_port");
            case "android.gamepad" -> new CatalogPolicy(
                    "dev.capyio.android.gamepad-local",
                    "adapter_managed",
                    "interactive",
                    "android.uptime_nanos",
                    "adapter_managed");
            case "android.haptics" -> new CatalogPolicy(
                    "dev.capyio.android.haptics-tls",
                    "adapter_managed",
                    "interactive",
                    null,
                    "adapter_managed");
            default -> throw new IllegalArgumentException(
                    "unsupported Android capability policy: " + capabilityKey);
        };
    }

    private static void appendStringField(StringBuilder json, String name, String value) {
        if (json.charAt(json.length() - 1) != '{') {
            json.append(',');
        }
        appendQuoted(json, name);
        json.append(':');
        appendQuoted(json, value);
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
                        String hex = Integer.toHexString(character);
                        json.append("0".repeat(4 - hex.length())).append(hex);
                    } else {
                        json.append(character);
                    }
                }
            }
        }
        json.append('"');
    }

    private static String lower(Enum<?> value) {
        return value.name().toLowerCase(Locale.ROOT);
    }

    private record CatalogPolicy(
            String adapterType,
            String routeMode,
            String qosMode,
            String clockDomain,
            String interoperability) {}
}
