package dev.capyio.android;

import java.io.IOException;
import java.security.MessageDigest;
import java.util.Arrays;
import java.util.ArrayList;
import java.util.List;

/** In-memory, one-use invitation transaction. Preview metadata contains no credential identifiers. */
final class AndroidPairingImportPlan implements AutoCloseable {
    record Peer(String nodeId, String displayName, long pairedAtMs, boolean active) {}
    record Entry<T>(Peer peer, T value) {}

    interface Store {
        byte[] loadDirectory() throws IOException;
        byte[] loadCredential(String handle) throws IOException;
        void putCredential(String handle, byte[] credential) throws IOException;
        void commitDirectory(byte[] directory) throws IOException;
        void removeCredential(String handle) throws IOException;
    }

    static final class RejectedException extends IOException {
        RejectedException(String message) { super(message); }
    }

    private final Peer incoming;
    private final Peer previous;
    private final byte[] originalDirectory;
    private final byte[] mergedDirectory;
    private final byte[] credential;
    private final String handle;
    private final boolean handleAlreadyReferenced;
    private boolean closed;

    AndroidPairingImportPlan(Peer incoming, Peer previous, byte[] originalDirectory,
            byte[] mergedDirectory, String handle, byte[] credential,
            boolean handleAlreadyReferenced) {
        this.incoming = incoming;
        this.previous = previous;
        this.originalDirectory = originalDirectory.clone();
        this.mergedDirectory = mergedDirectory.clone();
        this.handle = handle;
        this.credential = credential.clone();
        this.handleAlreadyReferenced = handleAlreadyReferenced;
    }

    Peer incoming() { return incoming; }
    Peer previous() { return previous; }

    /** Other records are retained; the current listener cannot select among multiple active peers. */
    static Peer validateReplacement(List<Peer> existing, Peer incoming) throws RejectedException {
        if (!incoming.active()) throw new RejectedException("邀请没有可用的配对节点。");
        Peer previous = null;
        for (Peer peer : existing) {
            if (peer.nodeId().equals(incoming.nodeId())) {
                if (previous != null) throw new RejectedException("本机配对记录无效，请先检查配对状态。");
                previous = peer;
                if (incoming.pairedAtMs() < peer.pairedAtMs()) {
                    throw new RejectedException("这份邀请早于本机已保存的配对，未导入。请在原 Windows 账号生成新的邀请，避免恢复旧证书导致连接失败。");
                }
            } else if (peer.active()) {
                throw new RejectedException("当前版本仅支持一个活动配对节点。已有其他节点的配对已保留，未导入新邀请。");
            }
        }
        return previous;
    }

    static <T> List<T> mergedRecords(List<Entry<T>> existing, Entry<T> incoming)
            throws RejectedException {
        ArrayList<Peer> peers = new ArrayList<>();
        for (Entry<T> entry : existing) peers.add(entry.peer());
        validateReplacement(peers, incoming.peer());
        ArrayList<T> merged = new ArrayList<>();
        for (Entry<T> entry : existing) {
            if (!entry.peer().nodeId().equals(incoming.peer().nodeId())) merged.add(entry.value());
        }
        merged.add(incoming.value());
        return List.copyOf(merged);
    }

    /** Called only from the native confirmation button, never while preparing the preview. */
    synchronized void commit(Store store) throws IOException {
        if (closed) throw new RejectedException("配对预览已关闭，请重新选择邀请文件。");
        boolean wroteNewCredential = false;
        try {
            if (!Arrays.equals(originalDirectory, store.loadDirectory())) {
                throw new RejectedException("配对记录已发生变化，请重新选择邀请并核对后再导入。");
            }
            if (handleAlreadyReferenced) {
                byte[] existing = store.loadCredential(handle);
                try {
                    if (!MessageDigest.isEqual(existing, credential)) {
                        throw new RejectedException("邀请与现有凭据不一致，未覆盖已有配对。");
                    }
                } finally {
                    Arrays.fill(existing, (byte) 0);
                }
            } else {
                store.putCredential(handle, credential);
                wroteNewCredential = true;
            }
            try {
                store.commitDirectory(mergedDirectory);
            } catch (IOException | RuntimeException failure) {
                // Re-importing an existing handle must never delete its live
                // credential merely because the directory write failed.
                if (wroteNewCredential) {
                    try { store.removeCredential(handle); }
                    catch (IOException rollback) { failure.addSuppressed(rollback); }
                }
                throw failure;
            }
        } finally {
            close();
        }
    }

    @Override public synchronized void close() {
        if (closed) return;
        closed = true;
        Arrays.fill(credential, (byte) 0);
        Arrays.fill(originalDirectory, (byte) 0);
        Arrays.fill(mergedDirectory, (byte) 0);
    }
}
