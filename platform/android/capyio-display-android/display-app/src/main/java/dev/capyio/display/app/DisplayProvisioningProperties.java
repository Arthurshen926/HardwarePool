package dev.capyio.display.app;

import java.io.ByteArrayInputStream;
import java.io.IOException;
import java.util.Properties;
import java.util.Set;

/** Strict, bounded lab configuration parser. It creates no trust or network access. */
public final class DisplayProvisioningProperties {
    public static final Set<String> TRANSPORT = Set.of("mode");
    public static final Set<String> CPD1 = Set.of("host", "port", "timeout_millis",
            "session_id", "route_id", "stream_id", "stream_epoch", "width", "height",
            "frame_rate_numerator", "frame_rate_denominator", "maximum_access_unit_bytes",
            "profile", "level");
    public static final Set<String> GAMESTREAM = Set.of("host", "host_ipv4", "launch_port",
            "timeout_millis", "adapter_instance_id", "display_capability_id", "session_id",
            "route_id", "stream_id", "stream_epoch", "port_base", "width", "height",
            "refresh_millihertz", "codec", "dynamic_range", "bitrate_kbps");

    private DisplayProvisioningProperties() {}

    public static Properties parse(byte[] bytes, Set<String> fields) throws IOException {
        if (bytes == null || bytes.length == 0 || bytes.length > 8192) {
            throw new IllegalArgumentException("Display properties size is invalid");
        }
        Properties values = new Properties() {
            @Override
            public synchronized Object put(Object key, Object value) {
                if (!fields.contains(key) || containsKey(key)) {
                    throw new IllegalArgumentException("Unknown or duplicate display field");
                }
                String text = (String) value;
                if (text.isBlank() || text.length() > 253 || !text.equals(text.strip())
                        || text.chars().anyMatch(Character::isISOControl)) {
                    throw new IllegalArgumentException("Invalid display field value");
                }
                return super.put(key, value);
            }
        };
        values.load(new ByteArrayInputStream(bytes));
        if (!values.keySet().equals(fields)) {
            throw new IllegalArgumentException("Missing display provisioning field");
        }
        return values;
    }
}
