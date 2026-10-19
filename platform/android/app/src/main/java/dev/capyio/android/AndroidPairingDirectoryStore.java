package dev.capyio.android;

import android.content.Context;
import android.util.AtomicFile;

import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileNotFoundException;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.UUID;

/** Atomic non-secret paired-directory storage; schema authority remains in Rust. */
final class AndroidPairingDirectoryStore {
    private static final int MAX_DIRECTORY_BYTES = 65_536;
    private static final byte[] EMPTY_DIRECTORY =
            "{\"schemaVersion\":1,\"peers\":[]}".getBytes(StandardCharsets.UTF_8);

    private final String localNodeId;
    private final AtomicFile directoryFile;

    AndroidPairingDirectoryStore(Context context, String localNodeId) {
        this(context.getNoBackupFilesDir(), localNodeId);
    }

    AndroidPairingDirectoryStore(File noBackupRoot, String localNodeId) {
        this(noBackupRoot, localNodeId, false);
    }

    AndroidPairingDirectoryStore(File noBackupRoot, String localNodeId, boolean pendingEnrollment) {
        this.localNodeId = UUID.fromString(localNodeId).toString();
        this.directoryFile = new AtomicFile(new File(
                noBackupRoot, pendingEnrollment
                    ? "pairing/pending-enrollment-" + this.localNodeId + ".json"
                    : "pairing/peers-v1.json"));
    }

    byte[] load() throws IOException {
        final byte[] stored;
        try {
            stored = readBounded(directoryFile);
        } catch (FileNotFoundException absent) {
            return AndroidNativePairingDirectory.canonicalize(localNodeId, EMPTY_DIRECTORY);
        }
        return AndroidNativePairingDirectory.canonicalize(localNodeId, stored);
    }

    int peerCount(byte[] canonicalDirectoryJson) {
        return AndroidNativePairingDirectory.peerCount(localNodeId, canonicalDirectoryJson);
    }

    String[] activeCredentialHandles(byte[] canonicalDirectoryJson) {
        return AndroidNativePairingDirectory.activeCredentialHandles(
                localNodeId, canonicalDirectoryJson);
    }

    void commit(byte[] directoryJson) throws IOException {
        if (directoryJson == null || directoryJson.length > MAX_DIRECTORY_BYTES) {
            throw new IllegalArgumentException("paired directory exceeds its bound");
        }
        byte[] canonical = AndroidNativePairingDirectory.canonicalize(localNodeId, directoryJson);
        File parent = directoryFile.getBaseFile().getParentFile();
        if (parent == null || (!parent.isDirectory() && !parent.mkdirs())) {
            throw new IOException("pairing directory is unavailable");
        }
        FileOutputStream output = null;
        try {
            output = directoryFile.startWrite();
            output.write(canonical);
            output.flush();
            output.getFD().sync();
            directoryFile.finishWrite(output);
        } catch (IOException error) {
            if (output != null) directoryFile.failWrite(output);
            throw error;
        }
    }

    private static byte[] readBounded(AtomicFile file) throws IOException {
        try (FileInputStream input = file.openRead();
             ByteArrayOutputStream output = new ByteArrayOutputStream()) {
            byte[] buffer = new byte[4096];
            int total = 0;
            while (true) {
                int read = input.read(buffer);
                if (read == -1) break;
                total += read;
                if (total > MAX_DIRECTORY_BYTES) {
                    throw new IOException("paired directory exceeds its bound");
                }
                output.write(buffer, 0, read);
            }
            return output.toByteArray();
        }
    }
}
