package dev.capyio.android;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;
import java.util.List;

public final class AndroidPairingImportPlanTest {
    private static final AndroidPairingImportPlan.Peer OLD =
            new AndroidPairingImportPlan.Peer("node-a", "Windows old", 100, true);
    private static final AndroidPairingImportPlan.Peer NEW =
            new AndroidPairingImportPlan.Peer("node-a", "Windows new", 200, true);
    private static final AndroidPairingImportPlan.Peer REVOKED =
            new AndroidPairingImportPlan.Peer("node-b", "Other revoked", 50, false);

    public static void main(String[] args) throws Exception {
        previewAndCancelNeverWrite();
        confirmationMergesOnlyTheNamedPeer();
        secondActivePeerIsRejectedBeforeMutation();
        oldInvitationCannotRollBackCurrentPairing();
        stalePreviewCannotOverwriteNewState();
        newCredentialRollsBackButReimportNeverDeletesExistingCredential();
        existingHandleCannotOverwriteDifferentCredential();
        System.out.println("Android pairing import: PASS (preview, cancel, merge, conflict, stale state, rollback, replay)");
    }

    private static void previewAndCancelNeverWrite() throws Exception {
        FakeStore store = new FakeStore();
        AndroidPairingImportPlan plan = plan(false);
        assert plan.previous().pairedAtMs() == 100 && plan.incoming().pairedAtMs() == 200;
        assert store.writes == 0 && store.removes == 0;
        plan.close();
        expectRejected(() -> plan.commit(store));
        assert store.writes == 0 && store.removes == 0;
        assert Arrays.equals(store.directory, bytes("old directory"));
    }

    private static void confirmationMergesOnlyTheNamedPeer() throws Exception {
        assert AndroidPairingImportPlan.validateReplacement(List.of(OLD, REVOKED), NEW) == OLD;
        List<String> merged = AndroidPairingImportPlan.mergedRecords(
                List.of(new AndroidPairingImportPlan.Entry<>(OLD, "old-a-record"),
                        new AndroidPairingImportPlan.Entry<>(REVOKED, "unchanged-b-record")),
                new AndroidPairingImportPlan.Entry<>(NEW, "new-a-record"));
        assert merged.equals(List.of("unchanged-b-record", "new-a-record"));
        assert AndroidPairingImportPlan.mergedRecords(
                List.of(new AndroidPairingImportPlan.Entry<>(REVOKED, "unchanged-b-record")),
                new AndroidPairingImportPlan.Entry<>(NEW, "new-a-record")).equals(merged);
        FakeStore store = new FakeStore();
        AndroidPairingImportPlan plan = plan(false);
        plan.commit(store);
        assert store.writes == 2 && store.removes == 0;
        assert Arrays.equals(store.directory, bytes("merged directory"));
        assert Arrays.equals(store.credential, bytes("new credential"));
        expectRejected(() -> plan.commit(store));
        assert store.writes == 2;
    }

    private static void secondActivePeerIsRejectedBeforeMutation() throws Exception {
        AndroidPairingImportPlan.Peer other = new AndroidPairingImportPlan.Peer(
                "node-c", "Other active", 80, true);
        expectRejected(() -> AndroidPairingImportPlan.validateReplacement(List.of(other), NEW));
        expectRejected(() -> AndroidPairingImportPlan.validateReplacement(List.of(OLD, other), NEW));
        expectRejected(() -> AndroidPairingImportPlan.validateReplacement(List.of(), REVOKED));
    }

    private static void oldInvitationCannotRollBackCurrentPairing() throws Exception {
        expectRejected(() -> AndroidPairingImportPlan.validateReplacement(List.of(NEW), OLD));
        expectRejected(() -> AndroidPairingImportPlan.mergedRecords(
                List.of(new AndroidPairingImportPlan.Entry<>(NEW, "current")),
                new AndroidPairingImportPlan.Entry<>(OLD, "old")));
        assert AndroidPairingImportPlan.validateReplacement(List.of(NEW), NEW) == NEW;
    }

    private static void stalePreviewCannotOverwriteNewState() throws Exception {
        FakeStore store = new FakeStore();
        AndroidPairingImportPlan plan = plan(false);
        store.directory = bytes("changed while preview was visible");
        expectRejected(() -> plan.commit(store));
        assert store.writes == 0 && store.removes == 0;
    }

    private static void newCredentialRollsBackButReimportNeverDeletesExistingCredential() throws Exception {
        FakeStore fresh = new FakeStore();
        fresh.failCommit = true;
        try { plan(false).commit(fresh); throw new AssertionError("commit must fail"); }
        catch (IOException expected) { assert fresh.removes == 1; }
        assert Arrays.equals(fresh.directory, bytes("old directory"));

        FakeStore reused = new FakeStore();
        reused.failCommit = true;
        reused.credential = bytes("new credential");
        try { plan(true).commit(reused); throw new AssertionError("commit must fail"); }
        catch (IOException expected) { assert reused.removes == 0 && reused.writes == 0; }
        assert Arrays.equals(reused.credential, bytes("new credential"));
        assert Arrays.equals(reused.directory, bytes("old directory"));
    }

    private static void existingHandleCannotOverwriteDifferentCredential() throws Exception {
        FakeStore store = new FakeStore();
        store.credential = bytes("existing different credential");
        expectRejected(() -> plan(true).commit(store));
        assert store.writes == 0 && store.removes == 0;
        assert Arrays.equals(store.credential, bytes("existing different credential"));
    }

    private static AndroidPairingImportPlan plan(boolean referenced) {
        return new AndroidPairingImportPlan(NEW, OLD, bytes("old directory"),
                bytes("merged directory"), "synthetic-handle", bytes("new credential"), referenced);
    }

    private static byte[] bytes(String value) { return value.getBytes(StandardCharsets.UTF_8); }
    private interface Action { void run() throws Exception; }
    private static void expectRejected(Action action) throws Exception {
        try { action.run(); throw new AssertionError("operation must reject"); }
        catch (AndroidPairingImportPlan.RejectedException expected) { /* Expected fail-closed path. */ }
    }

    private static final class FakeStore implements AndroidPairingImportPlan.Store {
        byte[] directory = bytes("old directory");
        byte[] credential;
        boolean failCommit;
        int writes;
        int removes;

        public byte[] loadDirectory() { return directory.clone(); }
        public byte[] loadCredential(String handle) throws IOException {
            if (credential == null) throw new IOException("missing synthetic credential");
            return credential.clone();
        }
        public void putCredential(String handle, byte[] value) { credential = value.clone(); writes++; }
        public void commitDirectory(byte[] value) throws IOException {
            if (failCommit) throw new IOException("synthetic write failure");
            directory = value.clone(); writes++;
        }
        public void removeCredential(String handle) { credential = null; removes++; }
    }
}
