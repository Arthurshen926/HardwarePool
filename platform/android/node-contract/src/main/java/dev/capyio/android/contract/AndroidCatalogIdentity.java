package dev.capyio.android.contract;

import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.UUID;

/** Stable RFC 4122 UUIDv5 mapping from Android platform keys into Core identity domains. */
public final class AndroidCatalogIdentity {
    private AndroidCatalogIdentity() {}

    public static String adapterId(String nodeId, String capabilityKey) {
        CapabilitySnapshot.requireKey(capabilityKey, "Capability key");
        return namedId(nodeId, "adapter/" + capabilityKey);
    }

    public static String capabilityId(String nodeId, String capabilityKey) {
        CapabilitySnapshot.requireKey(capabilityKey, "Capability key");
        return namedId(nodeId, "capability/" + capabilityKey);
    }

    public static String portId(String nodeId, String capabilityKey, String portKey) {
        CapabilitySnapshot.requireKey(capabilityKey, "Capability key");
        CapabilitySnapshot.requireKey(portKey, "Port key");
        return namedId(nodeId, "port/" + capabilityKey + "/" + portKey);
    }

    private static String namedId(String nodeId, String name) {
        UUID namespace;
        try {
            namespace = UUID.fromString(nodeId);
        } catch (RuntimeException invalid) {
            throw new IllegalArgumentException("Node ID must be a UUID", invalid);
        }
        byte[] namespaceBytes = ByteBuffer.allocate(16)
                .putLong(namespace.getMostSignificantBits())
                .putLong(namespace.getLeastSignificantBits())
                .array();
        try {
            MessageDigest digest = MessageDigest.getInstance("SHA-1");
            digest.update(namespaceBytes);
            byte[] hash = digest.digest(name.getBytes(StandardCharsets.UTF_8));
            hash[6] = (byte) ((hash[6] & 0x0f) | 0x50);
            hash[8] = (byte) ((hash[8] & 0x3f) | 0x80);
            ByteBuffer bytes = ByteBuffer.wrap(hash);
            return new UUID(bytes.getLong(), bytes.getLong()).toString();
        } catch (NoSuchAlgorithmException impossible) {
            throw new IllegalStateException("Java runtime does not provide SHA-1", impossible);
        }
    }
}
