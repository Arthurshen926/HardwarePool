package dev.capyio.android;

import android.app.Activity;
import android.app.Instrumentation;
import android.os.Bundle;

import java.io.File;
import java.io.FileOutputStream;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.Arrays;

/** Dependency-free device gate for the Android Keystore credential boundary. */
public final class AndroidCredentialStoreInstrumentation extends Instrumentation {
    private static final String HANDLE = "android-keystore:"
            + "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

    @Override
    public void onCreate(Bundle arguments) {
        super.onCreate(arguments);
        start();
    }

    @Override
    public void onStart() {
        Bundle result = new Bundle();
        try {
            pairingDirectoryRoundTripAndFailClosedRecovery();
            roundTripReplacementTamperAndRemoval();
            boundsAndClosedHandleShape();
            result.putString("stream", "Android pairing storage device gate: PASS\n");
            finish(Activity.RESULT_OK, result);
        } catch (Throwable failure) {
            result.putString("stream", "Android pairing storage device gate: FAIL: "
                    + failure.getClass().getSimpleName() + ": " + failure.getMessage() + "\n");
            finish(Activity.RESULT_CANCELED, result);
        }
    }

    private void pairingDirectoryRoundTripAndFailClosedRecovery() throws Exception {
        String localNodeId = "a0000000-0000-4000-8000-000000000001";
        File root = new File(getTargetContext().getNoBackupFilesDir(), "pairing-store-test-1451");
        deleteRecursively(root);
        try {
            AndroidPairingDirectoryStore store =
                    new AndroidPairingDirectoryStore(root, localNodeId);
            byte[] empty = store.load();
            requireEquals("{\"schemaVersion\":1,\"peers\":[]}",
                    new String(empty, StandardCharsets.UTF_8));
            File directory = new File(root, "pairing/peers-v1.json");
            require(!directory.exists(), "empty load must not create a directory file");

            String valid = "{\"schemaVersion\":1,\"peers\":[{"
                    + "\"nodeId\":\"b0000000-0000-4000-8000-000000000002\","
                    + "\"displayName\":\"Test tablet\","
                    + "\"serverName\":\"tablet.capyio.test\","
                    + "\"credentialHandle\":\"" + HANDLE + "\","
                    + "\"peerCertificateSha256\":\""
                    + "1111111111111111111111111111111111111111111111111111111111111111\","
                    + "\"pairedAtMs\":100,\"pairing\":{\"state\":\"active\"}}]}";
            store.commit(valid.getBytes(StandardCharsets.UTF_8));
            byte[] committed = store.load();
            requireEquals(valid, new String(committed, StandardCharsets.UTF_8));
            require(store.peerCount(committed) == 1, "paired peer count must be one");

            try {
                store.commit("{}".getBytes(StandardCharsets.UTF_8));
                throw new AssertionError("malformed replacement must fail");
            } catch (IllegalArgumentException expected) {
                requireEquals("paired-peer directory JSON is invalid", expected.getMessage());
            }
            requireEquals(valid, new String(store.load(), StandardCharsets.UTF_8));

            try (FileOutputStream output = new FileOutputStream(directory, false)) {
                output.write("{}".getBytes(StandardCharsets.UTF_8));
                output.getFD().sync();
            }
            try {
                store.load();
                throw new AssertionError("corrupt stored directory must fail closed");
            } catch (IllegalArgumentException expected) {
                requireEquals("paired-peer directory JSON is invalid", expected.getMessage());
            }
        } finally {
            deleteRecursively(root);
        }
    }

    private void roundTripReplacementTamperAndRemoval() throws Exception {
        AndroidCredentialStore store = new AndroidCredentialStore(getTargetContext());
        store.remove(HANDLE);
        try {
            byte[] first = credentialBundle("first generation");
            store.put(HANDLE, first);
            require(Arrays.equals(first, store.load(HANDLE)), "first credential must round trip");

            byte[] second = credentialBundle("second generation");
            store.put(HANDLE, second);
            require(Arrays.equals(second, store.load(HANDLE)), "second credential must round trip");

            File file = credentialFile();
            byte[] protectedBytes = Files.readAllBytes(file.toPath());
            protectedBytes[protectedBytes.length - 1] ^= 1;
            try (FileOutputStream output = new FileOutputStream(file, false)) {
                output.write(protectedBytes);
                output.getFD().sync();
            }
            try {
                store.load(HANDLE);
                throw new AssertionError("modified protected bytes must fail closed");
            } catch (AndroidCredentialStore.CredentialStoreException expected) {
                requireEquals("credential recovery failed", expected.getMessage());
            }
        } finally {
            store.remove(HANDLE);
        }
        require(!credentialFile().exists(), "credential file must be removed");
    }

    private void boundsAndClosedHandleShape() throws Exception {
        AndroidCredentialStore store = new AndroidCredentialStore(getTargetContext());
        for (String handle : Arrays.asList(
                "other:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "android-keystore:short",
                "android-keystore:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
                "android-keystore:gggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggg")) {
            try {
                store.load(handle);
                throw new AssertionError("invalid handle must be rejected");
            } catch (AndroidCredentialStore.CredentialStoreException expected) {
                requireEquals("credential handle is unsupported", expected.getMessage());
            }
        }
        try {
            store.put(HANDLE, new byte[65_537]);
            throw new AssertionError("oversized credential must be rejected");
        } catch (AndroidCredentialStore.CredentialStoreException expected) {
            requireEquals("credential size is invalid", expected.getMessage());
        }
        try {
            store.put(HANDLE, "arbitrary plaintext".getBytes(StandardCharsets.UTF_8));
            throw new AssertionError("untyped credential must be rejected");
        } catch (AndroidCredentialStore.CredentialStoreException expected) {
            requireEquals("credential protection failed", expected.getMessage());
        }
    }

    private File credentialFile() {
        return new File(getTargetContext().getNoBackupFilesDir(),
                "pairing/credentials-v1/"
                        + HANDLE.substring("android-keystore:".length()) + ".aksv1");
    }

    private static byte[] credentialBundle(String privateKey) {
        byte[] certificate = "leaf certificate".getBytes(StandardCharsets.UTF_8);
        byte[] key = privateKey.getBytes(StandardCharsets.UTF_8);
        byte[] root = "peer trust root".getBytes(StandardCharsets.UTF_8);
        ByteBuffer output = ByteBuffer.allocate(
                12 + 4 + certificate.length + 4 + key.length + 4 + root.length);
        output.put("CPYCRD01".getBytes(StandardCharsets.US_ASCII));
        output.put((byte) 1);
        output.put(new byte[3]);
        putComponent(output, certificate);
        putComponent(output, key);
        putComponent(output, root);
        return output.array();
    }

    private static void putComponent(ByteBuffer output, byte[] component) {
        output.putInt(component.length);
        output.put(component);
    }

    private static void require(boolean condition, String message) {
        if (!condition) throw new AssertionError(message);
    }

    private static void requireEquals(String expected, String actual) {
        if (!expected.equals(actual)) {
            throw new AssertionError("expected <" + expected + "> but was <" + actual + ">");
        }
    }

    private static void deleteRecursively(File target) {
        if (!target.exists()) return;
        File[] children = target.listFiles();
        if (children != null) {
            for (File child : children) deleteRecursively(child);
        }
        if (!target.delete()) {
            throw new AssertionError("could not delete test path: " + target.getName());
        }
    }
}
