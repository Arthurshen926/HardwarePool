package dev.capyio.android;

import android.content.Context;
import android.security.keystore.KeyGenParameterSpec;
import android.security.keystore.KeyProperties;
import android.util.AtomicFile;

import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.nio.ByteBuffer;
import java.security.GeneralSecurityException;
import java.security.KeyStore;
import java.util.Arrays;

import javax.crypto.Cipher;
import javax.crypto.KeyGenerator;
import javax.crypto.SecretKey;
import javax.crypto.spec.GCMParameterSpec;

/** App-private, Android-Keystore-backed storage for bounded control credentials. */
final class AndroidCredentialStore {
    private static final String HANDLE_PREFIX = "android-keystore:";
    private static final String KEY_ALIAS_PREFIX = "capyio-control-v1-";
    private static final String KEY_STORE = "AndroidKeyStore";
    private static final String TRANSFORMATION = "AES/GCM/NoPadding";
    private static final byte[] MAGIC = {'C', 'A', 'K', '1'};
    private static final int VERSION = 1;
    private static final int IV_BYTES = 12;
    private static final int TAG_BITS = 128;
    private static final int MAX_PLAINTEXT_BYTES = 65_536;
    private static final int MAX_PROTECTED_BYTES = MAX_PLAINTEXT_BYTES + 128;

    private final File credentialDirectory;

    AndroidCredentialStore(Context context) {
        credentialDirectory = new File(
                context.getNoBackupFilesDir(), "pairing/credentials-v1");
    }

    void put(String handle, byte[] plaintext) throws CredentialStoreException {
        String name = parseHandle(handle);
        if (plaintext == null || plaintext.length == 0
                || plaintext.length > MAX_PLAINTEXT_BYTES) {
            throw new CredentialStoreException("credential size is invalid");
        }
        try {
            AndroidNativeControlCredential.requireValid(plaintext);
            SecretKey key = loadOrCreateKey(name);
            Cipher cipher = Cipher.getInstance(TRANSFORMATION);
            cipher.init(Cipher.ENCRYPT_MODE, key);
            byte[] iv = cipher.getIV();
            if (iv == null || iv.length != IV_BYTES) {
                throw new GeneralSecurityException("unexpected GCM IV size");
            }
            byte[] ciphertext = cipher.doFinal(plaintext);
            ByteBuffer envelope = ByteBuffer.allocate(
                    MAGIC.length + 1 + 1 + Integer.BYTES + iv.length + ciphertext.length);
            envelope.put(MAGIC);
            envelope.put((byte) VERSION);
            envelope.put((byte) iv.length);
            envelope.putInt(ciphertext.length);
            envelope.put(iv);
            envelope.put(ciphertext);
            writeAtomically(credentialFile(name), envelope.array());
        } catch (GeneralSecurityException | IOException | IllegalArgumentException error) {
            throw new CredentialStoreException("credential protection failed", error);
        }
    }

    boolean contains(String handle) throws CredentialStoreException {
        try (FileInputStream input = new AtomicFile(credentialFile(parseHandle(handle))).openRead()) {
            return true;
        } catch (java.io.FileNotFoundException absent) {
            return false;
        } catch (IOException error) {
            throw new CredentialStoreException("credential presence check failed", error);
        }
    }

    byte[] load(String handle) throws CredentialStoreException {
        String name = parseHandle(handle);
        try {
            byte[] envelope = readBounded(new AtomicFile(credentialFile(name)));
            ByteBuffer input = ByteBuffer.wrap(envelope);
            byte[] magic = new byte[MAGIC.length];
            if (input.remaining() < MAGIC.length + 1 + 1 + Integer.BYTES) {
                throw new IOException("credential envelope is truncated");
            }
            input.get(magic);
            int version = Byte.toUnsignedInt(input.get());
            int ivLength = Byte.toUnsignedInt(input.get());
            int ciphertextLength = input.getInt();
            if (!Arrays.equals(magic, MAGIC) || version != VERSION || ivLength != IV_BYTES
                    || ciphertextLength < TAG_BITS / 8
                    || ciphertextLength > MAX_PLAINTEXT_BYTES + TAG_BITS / 8
                    || input.remaining() != ivLength + ciphertextLength) {
                throw new IOException("credential envelope is invalid");
            }
            byte[] iv = new byte[ivLength];
            byte[] ciphertext = new byte[ciphertextLength];
            input.get(iv);
            input.get(ciphertext);

            SecretKey key = loadKey(name);
            Cipher cipher = Cipher.getInstance(TRANSFORMATION);
            cipher.init(Cipher.DECRYPT_MODE, key, new GCMParameterSpec(TAG_BITS, iv));
            byte[] plaintext = cipher.doFinal(ciphertext);
            if (plaintext.length == 0 || plaintext.length > MAX_PLAINTEXT_BYTES) {
                throw new GeneralSecurityException("decrypted credential size is invalid");
            }
            // Reject legacy or otherwise non-CapyIO plaintext even when its
            // Keystore envelope is authentic.
            AndroidNativeControlCredential.requireValid(plaintext);
            return plaintext;
        } catch (GeneralSecurityException | IOException | IllegalArgumentException error) {
            throw new CredentialStoreException("credential recovery failed", error);
        }
    }

    void remove(String handle) throws CredentialStoreException {
        String name = parseHandle(handle);
        try {
            KeyStore keyStore = openKeyStore();
            keyStore.deleteEntry(keyAlias(name));
            AtomicFile file = new AtomicFile(credentialFile(name));
            file.delete();
        } catch (GeneralSecurityException error) {
            throw new CredentialStoreException("credential removal failed", error);
        }
    }

    private SecretKey loadOrCreateKey(String name) throws GeneralSecurityException {
        KeyStore keyStore = openKeyStore();
        java.security.Key existing = keyStore.getKey(keyAlias(name), null);
        if (existing instanceof SecretKey secretKey) return secretKey;
        if (existing != null) throw new GeneralSecurityException("credential key type is invalid");

        KeyGenerator generator = KeyGenerator.getInstance(
                KeyProperties.KEY_ALGORITHM_AES, KEY_STORE);
        generator.init(new KeyGenParameterSpec.Builder(
                keyAlias(name), KeyProperties.PURPOSE_ENCRYPT | KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setRandomizedEncryptionRequired(true)
                .setKeySize(256)
                .build());
        return generator.generateKey();
    }

    private SecretKey loadKey(String name) throws GeneralSecurityException {
        java.security.Key key = openKeyStore().getKey(keyAlias(name), null);
        if (!(key instanceof SecretKey secretKey)) {
            throw new GeneralSecurityException("credential key is unavailable");
        }
        return secretKey;
    }

    private static KeyStore openKeyStore() throws GeneralSecurityException {
        KeyStore keyStore = KeyStore.getInstance(KEY_STORE);
        try {
            keyStore.load(null);
        } catch (IOException error) {
            throw new GeneralSecurityException("Android Keystore could not be loaded", error);
        }
        return keyStore;
    }

    private static void writeAtomically(File target, byte[] bytes) throws IOException {
        File parent = target.getParentFile();
        if (parent == null || (!parent.isDirectory() && !parent.mkdirs())) {
            throw new IOException("credential directory is unavailable");
        }
        AtomicFile atomicFile = new AtomicFile(target);
        FileOutputStream output = null;
        try {
            output = atomicFile.startWrite();
            output.write(bytes);
            output.flush();
            output.getFD().sync();
            atomicFile.finishWrite(output);
        } catch (IOException error) {
            if (output != null) atomicFile.failWrite(output);
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
                if (total > MAX_PROTECTED_BYTES) {
                    throw new IOException("credential envelope exceeds its bound");
                }
                output.write(buffer, 0, read);
            }
            return output.toByteArray();
        }
    }

    private File credentialFile(String name) {
        return new File(credentialDirectory, name + ".aksv1");
    }

    private static String keyAlias(String name) {
        return KEY_ALIAS_PREFIX + name;
    }

    private static String parseHandle(String handle) throws CredentialStoreException {
        if (handle == null || !handle.startsWith(HANDLE_PREFIX)) {
            throw new CredentialStoreException("credential handle is unsupported");
        }
        String name = handle.substring(HANDLE_PREFIX.length());
        if (name.length() != 64) {
            throw new CredentialStoreException("credential handle is unsupported");
        }
        for (int index = 0; index < name.length(); index++) {
            char value = name.charAt(index);
            if (!((value >= '0' && value <= '9') || (value >= 'a' && value <= 'f'))) {
                throw new CredentialStoreException("credential handle is unsupported");
            }
        }
        return name;
    }

    static final class CredentialStoreException extends Exception {
        CredentialStoreException(String message) {
            super(message);
        }

        CredentialStoreException(String message, Throwable cause) {
            super(message, cause);
        }
    }
}
