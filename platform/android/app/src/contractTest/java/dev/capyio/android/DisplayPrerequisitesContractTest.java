package dev.capyio.android;

import dev.capyio.display.app.DisplayPrerequisites;
import dev.capyio.display.app.DisplayProvisioningBundleInstaller;
import dev.capyio.display.app.DisplayProvisioningProperties;
import dev.capyio.display.app.DisplaySocketOwnership;
import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.Set;
import java.util.zip.ZipEntry;
import java.util.zip.ZipOutputStream;

public final class DisplayPrerequisitesContractTest {
    public static void main(String[] args) throws Exception {
        for (boolean config : new boolean[] {false, true}) {
            for (boolean nativeClient : new boolean[] {false, true}) {
                for (String transport : new String[] {"unconfigured", "cpd1", "gamestream"}) {
                    DisplayPrerequisites state = new DisplayPrerequisites(transport, config, nativeClient);
                    boolean expected = config && !transport.equals("unconfigured")
                            && (transport.equals("cpd1") || nativeClient);
                    assert state.canOpen() == expected;
                    assert state.problemCode().isEmpty() == expected;
                }
            }
        }
        try {
            new DisplayPrerequisites("automatic", true, true);
            throw new AssertionError("implicit fallback accepted");
        } catch (IllegalArgumentException expected) { }
        assert new DisplayPrerequisites("gamestream", true, false).problemCode()
                .equals("CAPY.DISPLAY.NATIVE_CLIENT_MISSING");
        System.out.println("Display prerequisites: PASS (14 cases)");
        testProvisioning();
        testSocketOwnership();
    }

    private static void testProvisioning() throws Exception {
        assert DisplayProvisioningProperties.parse("# comment\nmode=gamestream\n"
                .getBytes(StandardCharsets.ISO_8859_1), DisplayProvisioningProperties.TRANSPORT)
                .getProperty("mode").equals("gamestream");
        for (String invalid : new String[] {"", "# no fields", "mode=", "mode=gamestream\nextra=1",
                "mode=cpd1\nmode=gamestream", "mode=cpd1\nm\\u006fde=gamestream",
                "mode=gamestream ", "mode=game\\nstream", "mode=" + "a".repeat(254),
                "#" + "a".repeat(8192)}) {
            try {
                DisplayProvisioningProperties.parse(invalid.getBytes(StandardCharsets.ISO_8859_1),
                        DisplayProvisioningProperties.TRANSPORT);
                throw new AssertionError("invalid provisioning accepted");
            } catch (IllegalArgumentException | IOException expected) { }
        }
        for (var schema : java.util.List.of(DisplayProvisioningProperties.CPD1,
                DisplayProvisioningProperties.GAMESTREAM)) {
            String text = schema.stream().sorted().map(key -> key + "=test\n")
                    .collect(java.util.stream.Collectors.joining());
            assert DisplayProvisioningProperties.parse(text.getBytes(StandardCharsets.ISO_8859_1), schema)
                    .size() == schema.size();
        }
        System.out.println("Display provisioning structure: PASS (13 cases; not TLS identity validation)");
        testProvisioningBundle();
    }

    private static void testProvisioningBundle() throws Exception {
        Path root = Files.createTempDirectory("capyio-display-bundle-");
        try {
            Map<String, byte[]> valid = bundleFiles("cpd1", DisplayProvisioningProperties.CPD1);
            assert DisplayProvisioningBundleInstaller.install(root.toFile(), zip(valid)).equals("cpd1");
            Path installed = root.resolve("capyio-display");
            assert Files.readString(installed.resolve("transport.properties")).equals("mode=cpd1\n");
            assert Files.exists(installed.resolve("client-key.pk8"));

            for (Map<String, byte[]> invalid : java.util.List.of(
                    without(valid, "ca.der"),
                    with(valid, "unexpected.txt", new byte[] {1}),
                    with(valid, "gamestream-launch.properties", properties(DisplayProvisioningProperties.GAMESTREAM)),
                    with(valid, "transport.properties", "mode=unknown\n".getBytes(StandardCharsets.ISO_8859_1)),
                    with(valid, "client-key.pk8", new byte[0]),
                    with(valid, "client-cert.der", new byte[65537]))) {
                try {
                    DisplayProvisioningBundleInstaller.install(root.toFile(), zip(invalid));
                    throw new AssertionError("invalid display bundle accepted");
                } catch (IOException | IllegalArgumentException expected) { }
                assert Files.readString(installed.resolve("transport.properties")).equals("mode=cpd1\n");
            }

            Map<String, byte[]> gameStream = bundleFiles("gamestream",
                    DisplayProvisioningProperties.GAMESTREAM);
            assert DisplayProvisioningBundleInstaller.install(root.toFile(), zip(gameStream))
                    .equals("gamestream");
            assert Files.exists(installed.resolve("gamestream-launch.properties"));
            assert !Files.exists(installed.resolve("connection.properties"));
            System.out.println("Display provisioning bundle: PASS (8 cases; app-private atomic replacement)");
        } finally {
            try (var paths = Files.walk(root)) {
                paths.sorted(java.util.Comparator.reverseOrder()).forEach(path -> {
                    try { Files.deleteIfExists(path); } catch (IOException ignored) { }
                });
            }
        }
    }

    private static Map<String, byte[]> bundleFiles(String mode, Set<String> schema) {
        Map<String, byte[]> files = new LinkedHashMap<>();
        files.put("transport.properties", ("mode=" + mode + "\n")
                .getBytes(StandardCharsets.ISO_8859_1));
        files.put(mode.equals("cpd1") ? "connection.properties" : "gamestream-launch.properties",
                properties(schema));
        files.put("ca.der", new byte[] {1});
        files.put("client-cert.der", new byte[] {2});
        files.put("client-key.pk8", new byte[] {3});
        return files;
    }

    private static byte[] properties(Set<String> schema) {
        return schema.stream().sorted().map(key -> key + "=test\n")
                .collect(java.util.stream.Collectors.joining())
                .getBytes(StandardCharsets.ISO_8859_1);
    }

    private static Map<String, byte[]> without(Map<String, byte[]> source, String name) {
        Map<String, byte[]> copy = new LinkedHashMap<>(source);
        copy.remove(name);
        return copy;
    }

    private static Map<String, byte[]> with(Map<String, byte[]> source, String name, byte[] value) {
        Map<String, byte[]> copy = new LinkedHashMap<>(source);
        copy.put(name, value);
        return copy;
    }

    private static ByteArrayInputStream zip(Map<String, byte[]> files) throws IOException {
        ByteArrayOutputStream bytes = new ByteArrayOutputStream();
        try (ZipOutputStream output = new ZipOutputStream(bytes)) {
            for (Map.Entry<String, byte[]> file : files.entrySet()) {
                output.putNextEntry(new ZipEntry(file.getKey()));
                output.write(file.getValue());
                output.closeEntry();
            }
        }
        return new ByteArrayInputStream(bytes.toByteArray());
    }

    private static final class FakeSocket extends Socket {
        int closes;
        boolean failClose;
        @Override public synchronized void close() throws IOException {
            closes++;
            if (failClose) throw new IOException("synthetic cleanup failure");
        }
    }

    private static void testSocketOwnership() throws Exception {
        FakeSocket success = new FakeSocket();
        assert DisplaySocketOwnership.initialize(success, socket -> { }) == success;
        assert success.closes == 0;
        success.close();
        for (Throwable failure : new Throwable[] {new IOException("connect failed"),
                new SecurityException("ALPN rejected"), new AssertionError("provider failed")}) {
            FakeSocket socket = new FakeSocket();
            try {
                DisplaySocketOwnership.initialize(socket, value -> {
                    if (failure instanceof Error error) throw error;
                    throw (Exception) failure;
                });
                throw new AssertionError("failure ignored");
            } catch (Exception | Error actual) { assert actual == failure; }
            assert socket.closes == 1;
        }
        FakeSocket badClose = new FakeSocket();
        badClose.failClose = true;
        IOException original = new IOException("handshake failed");
        try {
            DisplaySocketOwnership.initialize(badClose, socket -> { throw original; });
            throw new AssertionError("failure ignored");
        } catch (IOException actual) {
            assert actual == original;
            assert actual.getSuppressed().length == 1;
            assert badClose.closes == 1;
        }
        System.out.println("Display socket ownership: PASS (5 cases; no network connection)");
    }
}
