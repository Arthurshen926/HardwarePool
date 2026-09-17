package dev.capyio.display.app;

import android.content.Context;

import dev.capyio.display.AvcDecoderConfiguration;

import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.IOException;
import java.util.Arrays;
import java.util.Properties;
import java.util.UUID;

/** Loads one bounded, app-private lab provisioning bundle. */
final class ProvisionedDisplayConfiguration {
    static final String DIRECTORY = "capyio-display";
    private static final int MAX_PROPERTIES_BYTES = 8 * 1024;
    private static final int MAX_CERTIFICATE_BYTES = 64 * 1024;
    private static final int MAX_PRIVATE_KEY_BYTES = 64 * 1024;

    private ProvisionedDisplayConfiguration() {}

    record GameStreamLaunch(
            AuthenticatedGameStreamLaunchClient.Configuration tls,
            GameStreamLaunchRecord.Expected expected) {}

    static DisplayTransportMode loadTransportMode(Context context) throws IOException {
        File directory = new File(context.getFilesDir(), DIRECTORY);
        Properties values = DisplayProvisioningProperties.parse(readBounded(
                new File(directory, "transport.properties"), MAX_PROPERTIES_BYTES),
                DisplayProvisioningProperties.TRANSPORT);
        return DisplayTransportMode.parse(require(values, "mode"));
    }

    static void validateSelectedProperties(Context context, String transport) throws IOException {
        File directory = new File(context.getFilesDir(), DIRECTORY);
        boolean gameStream = "gamestream".equals(transport);
        DisplayProvisioningProperties.parse(readBounded(new File(directory,
                gameStream ? "gamestream-launch.properties" : "connection.properties"),
                MAX_PROPERTIES_BYTES), gameStream ? DisplayProvisioningProperties.GAMESTREAM
                        : DisplayProvisioningProperties.CPD1);
    }

    static AuthenticatedDisplayTlsClient.Configuration load(Context context) throws IOException {
        File directory = new File(context.getFilesDir(), DIRECTORY);
        Properties values = DisplayProvisioningProperties.parse(readBounded(
                new File(directory, "connection.properties"), MAX_PROPERTIES_BYTES),
                DisplayProvisioningProperties.CPD1);
        AuthenticatedDisplayTlsClient.Binding binding = new AuthenticatedDisplayTlsClient.Binding(
                UUID.fromString(require(values, "session_id")),
                UUID.fromString(require(values, "route_id")),
                UUID.fromString(require(values, "stream_id")),
                positiveLong(values, "stream_epoch"),
                positiveInt(values, "width"),
                positiveInt(values, "height"),
                positiveInt(values, "frame_rate_numerator"),
                positiveInt(values, "frame_rate_denominator"),
                positiveInt(values, "maximum_access_unit_bytes"),
                AvcDecoderConfiguration.Profile.valueOf(require(values, "profile")),
                AvcDecoderConfiguration.Level.valueOf(require(values, "level")));
        byte[] ca = readBounded(new File(directory, "ca.der"), MAX_CERTIFICATE_BYTES);
        byte[] certificate = readBounded(new File(directory, "client-cert.der"), MAX_CERTIFICATE_BYTES);
        byte[] key = readBounded(new File(directory, "client-key.pk8"), MAX_PRIVATE_KEY_BYTES);
        try {
            return new AuthenticatedDisplayTlsClient.Configuration(
                require(values, "host"),
                positiveInt(values, "port"),
                positiveInt(values, "timeout_millis"),
                ca, certificate, key,
                binding);
        } finally {
            Arrays.fill(key, (byte) 0);
        }
    }

    static GameStreamLaunch loadGameStreamLaunch(Context context) throws IOException {
        File directory = new File(context.getFilesDir(), DIRECTORY);
        Properties values = DisplayProvisioningProperties.parse(readBounded(
                new File(directory, "gamestream-launch.properties"), MAX_PROPERTIES_BYTES),
                DisplayProvisioningProperties.GAMESTREAM);
        String host = require(values, "host");
        byte[] hostIpv4 = parseIpv4(require(values, "host_ipv4"));
        GameStreamLaunchRecord.Expected expected = new GameStreamLaunchRecord.Expected(
                UUID.fromString(require(values, "adapter_instance_id")),
                UUID.fromString(require(values, "display_capability_id")),
                UUID.fromString(require(values, "session_id")),
                UUID.fromString(require(values, "route_id")),
                UUID.fromString(require(values, "stream_id")),
                positiveLong(values, "stream_epoch"), hostIpv4,
                positiveInt(values, "port_base"), positiveInt(values, "width"),
                positiveInt(values, "height"), positiveInt(values, "refresh_millihertz"),
                positiveInt(values, "codec"), positiveInt(values, "dynamic_range"),
                positiveInt(values, "bitrate_kbps"));
        byte[] ca = readBounded(new File(directory, "ca.der"), MAX_CERTIFICATE_BYTES);
        byte[] certificate = readBounded(
                new File(directory, "client-cert.der"), MAX_CERTIFICATE_BYTES);
        byte[] key = readBounded(new File(directory, "client-key.pk8"), MAX_PRIVATE_KEY_BYTES);
        AuthenticatedGameStreamLaunchClient.Configuration tls;
        try {
            tls = new AuthenticatedGameStreamLaunchClient.Configuration(
                    host, positiveInt(values, "launch_port"),
                    positiveInt(values, "timeout_millis"), ca, certificate, key);
        } finally {
            Arrays.fill(key, (byte) 0);
        }
        return new GameStreamLaunch(tls, expected);
    }

    private static String require(Properties values, String name) {
        String value = values.getProperty(name);
        if (value == null || value.isBlank() || value.length() > 253) {
            throw new IllegalArgumentException("missing or invalid provisioning field: " + name);
        }
        return value;
    }

    private static int positiveInt(Properties values, String name) {
        int value = Integer.parseInt(require(values, name));
        if (value <= 0) {
            throw new IllegalArgumentException("provisioning field must be positive: " + name);
        }
        return value;
    }

    private static long positiveLong(Properties values, String name) {
        long value = Long.parseLong(require(values, name));
        if (value <= 0) {
            throw new IllegalArgumentException("provisioning field must be positive: " + name);
        }
        return value;
    }

    private static byte[] readBounded(File file, int maximumBytes) throws IOException {
        try (FileInputStream input = new FileInputStream(file);
                ByteArrayOutputStream output = new ByteArrayOutputStream()) {
            byte[] buffer = new byte[4096];
            int total = 0;
            int read;
            while ((read = input.read(buffer)) != -1) {
                total = Math.addExact(total, read);
                if (total > maximumBytes) {
                    throw new IOException("provisioning file exceeds its bound: " + file.getName());
                }
                output.write(buffer, 0, read);
            }
            if (total == 0) {
                throw new IOException("provisioning file is empty: " + file.getName());
            }
            return output.toByteArray();
        }
    }

    private static byte[] parseIpv4(String value) {
        String[] parts = value.split("\\.", -1);
        if (parts.length != 4) {
            throw new IllegalArgumentException("host_ipv4 must be an IPv4 literal");
        }
        byte[] address = new byte[4];
        for (int index = 0; index < parts.length; index++) {
            if (parts[index].isEmpty() || parts[index].length() > 3
                    || (parts[index].length() > 1 && parts[index].startsWith("0"))) {
                throw new IllegalArgumentException("host_ipv4 must be a canonical IPv4 literal");
            }
            int octet = Integer.parseInt(parts[index]);
            if (octet < 0 || octet > 255) {
                throw new IllegalArgumentException("host_ipv4 octet is outside 0..=255");
            }
            address[index] = (byte) octet;
        }
        return address;
    }
}
