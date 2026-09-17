package dev.capyio.display.app;

import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.nio.file.Files;
import java.util.Arrays;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.Set;
import java.util.zip.ZipEntry;
import java.util.zip.ZipInputStream;

/** Installs one closed, bounded display connection bundle into app-private storage. */
public final class DisplayProvisioningBundleInstaller {
    private static final String TARGET = "capyio-display";
    private static final String PENDING = "capyio-display.pending";
    private static final String BACKUP = "capyio-display.backup";
    private static final Set<String> MANAGED_NAMES = Set.of(
            "transport.properties", "connection.properties", "gamestream-launch.properties",
            "ca.der", "client-cert.der", "client-key.pk8");
    private static final Set<String> COMMON_NAMES = Set.of(
            "transport.properties", "ca.der", "client-cert.der", "client-key.pk8");
    private static final int MAX_ARCHIVE_ENTRIES = 5;
    private static final int MAX_TOTAL_BYTES = 224 * 1024;

    private DisplayProvisioningBundleInstaller() {}

    public static String install(File filesDirectory, InputStream source) throws IOException {
        if (filesDirectory == null || !filesDirectory.isDirectory() || source == null) {
            throw new IllegalArgumentException("Display provisioning destination is unavailable");
        }
        Map<String, byte[]> files = read(source);
        try {
            String mode = parseAndValidate(files);
            File pending = child(filesDirectory, PENDING);
            File target = child(filesDirectory, TARGET);
            File backup = child(filesDirectory, BACKUP);
            removeManagedDirectory(pending, true);
            removeManagedDirectory(backup, true);
            if (!pending.mkdir()) throw new IOException("Cannot create display provisioning staging directory");
            try {
                for (Map.Entry<String, byte[]> entry : files.entrySet()) {
                    writeSynced(child(pending, entry.getKey()), entry.getValue());
                }
                if (target.exists()) {
                    validateManagedDirectory(target);
                    if (!target.renameTo(backup)) throw new IOException("Cannot preserve the previous display configuration");
                }
                if (!pending.renameTo(target)) {
                    if (backup.exists() && !backup.renameTo(target)) {
                        throw new IOException("Cannot install or restore the display configuration");
                    }
                    throw new IOException("Cannot install the display configuration");
                }
                removeManagedDirectory(backup, true);
                return mode;
            } catch (IOException | RuntimeException failure) {
                removeManagedDirectory(pending, true);
                throw failure;
            }
        } finally {
            files.values().forEach(bytes -> Arrays.fill(bytes, (byte) 0));
        }
    }

    static Map<String, byte[]> read(InputStream source) throws IOException {
        Map<String, byte[]> files = new LinkedHashMap<>();
        int total = 0;
        boolean complete = false;
        try (ZipInputStream archive = new ZipInputStream(source)) {
            ZipEntry entry;
            while ((entry = archive.getNextEntry()) != null) {
                String name = entry.getName();
                if (entry.isDirectory() || !MANAGED_NAMES.contains(name) || files.containsKey(name)
                        || files.size() >= MAX_ARCHIVE_ENTRIES) {
                    throw new IOException("Display bundle contains an unknown or duplicate entry");
                }
                int maximum = name.endsWith(".properties") ? 8192 : 65536;
                byte[] bytes = readBounded(archive, maximum);
                total = Math.addExact(total, bytes.length);
                if (total > MAX_TOTAL_BYTES) throw new IOException("Display bundle exceeds its total bound");
                files.put(name, bytes);
                archive.closeEntry();
            }
            complete = true;
            return files;
        } finally {
            if (!complete) files.values().forEach(bytes -> Arrays.fill(bytes, (byte) 0));
        }
    }

    private static String parseAndValidate(Map<String, byte[]> files) throws IOException {
        if (!files.keySet().containsAll(COMMON_NAMES)) {
            throw new IOException("Display bundle is missing a credential or transport file");
        }
        var transport = DisplayProvisioningProperties.parse(files.get("transport.properties"),
                DisplayProvisioningProperties.TRANSPORT);
        String mode = transport.getProperty("mode");
        String selected;
        Set<String> schema;
        if ("cpd1".equals(mode)) {
            selected = "connection.properties";
            schema = DisplayProvisioningProperties.CPD1;
        } else if ("gamestream".equals(mode)) {
            selected = "gamestream-launch.properties";
            schema = DisplayProvisioningProperties.GAMESTREAM;
        } else {
            throw new IOException("Display bundle selects an unsupported transport");
        }
        Set<String> expected = new java.util.HashSet<>(COMMON_NAMES);
        expected.add(selected);
        if (!files.keySet().equals(expected)) {
            throw new IOException("Display bundle does not exactly match its selected transport");
        }
        DisplayProvisioningProperties.parse(files.get(selected), schema);
        return mode;
    }

    private static byte[] readBounded(InputStream input, int maximum) throws IOException {
        ByteArrayOutputStream output = new ByteArrayOutputStream();
        byte[] buffer = new byte[4096];
        int total = 0;
        int read;
        while ((read = input.read(buffer)) != -1) {
            total = Math.addExact(total, read);
            if (total > maximum) throw new IOException("Display bundle entry exceeds its bound");
            output.write(buffer, 0, read);
        }
        if (total == 0) throw new IOException("Display bundle entry is empty");
        return output.toByteArray();
    }

    private static void writeSynced(File file, byte[] bytes) throws IOException {
        try (FileOutputStream output = new FileOutputStream(file)) {
            output.write(bytes);
            output.getFD().sync();
        }
    }

    private static File child(File parent, String name) throws IOException {
        File child = new File(parent, name);
        String parentPath = parent.getCanonicalPath() + File.separator;
        if (!child.getCanonicalPath().startsWith(parentPath)) {
            throw new IOException("Display provisioning path escaped app-private storage");
        }
        return child;
    }

    private static void validateManagedDirectory(File directory) throws IOException {
        if (Files.isSymbolicLink(directory.toPath()) || !directory.isDirectory()) {
            throw new IOException("Display provisioning target is not a private directory");
        }
        File[] children = directory.listFiles();
        if (children == null) throw new IOException("Cannot inspect display provisioning directory");
        for (File child : children) {
            if (!MANAGED_NAMES.contains(child.getName()) || Files.isSymbolicLink(child.toPath())
                    || !child.isFile()) {
                throw new IOException("Display provisioning directory contains unmanaged content");
            }
        }
    }

    private static void removeManagedDirectory(File directory, boolean absentAllowed) throws IOException {
        if (!directory.exists()) {
            if (absentAllowed) return;
            throw new IOException("Display provisioning directory is missing");
        }
        validateManagedDirectory(directory);
        File[] children = directory.listFiles();
        if (children == null) throw new IOException("Cannot inspect display provisioning directory");
        for (File child : children) {
            if (!child.delete()) throw new IOException("Cannot remove stale display provisioning file");
        }
        if (!directory.delete()) throw new IOException("Cannot remove stale display provisioning directory");
    }
}
