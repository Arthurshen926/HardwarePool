package dev.capyio.android;

import android.content.Context;
import android.os.Build;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;
import java.util.concurrent.Executors;
import java.util.concurrent.ScheduledExecutorService;
import java.util.concurrent.TimeUnit;
import org.json.JSONObject;

/** Service-owned pairing; credentials cross JNI only to the native secure-store installer. */
final class AndroidFilelessPairing implements AutoCloseable {
    static { System.loadLibrary("capyio_android_jni"); }
    private final Context context;
    private final Runnable reload;
    private final ScheduledExecutorService worker = Executors.newSingleThreadScheduledExecutor();
    private volatile String status = "{\"phase\":\"idle\",\"message\":\"\"}";
    private volatile boolean active;
    private boolean closed;

    AndroidFilelessPairing(Context context, Runnable reload) {
        this.context = context.getApplicationContext(); this.reload = reload;
        worker.scheduleWithFixedDelay(this::poll, 0, 500, TimeUnit.MILLISECONDS);
    }
    String status() {
        if (!active) {
            try {
                String pending = new AndroidEnrollmentJournal(context).summary();
                if (pending != null) return pending;
            } catch (Exception error) { return errorStatus(error.getMessage()); }
        }
        return status;
    }
    // Main-thread callback after the native listener has actually bound its socket.
    void controlReady(int port) { nativeControlReady(port); }
    synchronized String action(String request) {
        if (closed) throw new IllegalStateException("节点服务已关闭");
        try {
            String action = new JSONObject(request).getString("action");
            AndroidEnrollmentJournal journal = new AndroidEnrollmentJournal(context);
            if (action.equals("recover")) {
                if (active) throw new IllegalStateException("请先结束当前配对，再恢复记录");
                if (!journal.present()) throw new IllegalStateException("没有待恢复的配对记录");
                JSONObject recovered = new JSONObject(journal.summary());
                journal.commit(); reload.run();
                status = recovered.put("phase", "recovered").put("message", "本机已使用原证书恢复配对；对端如有恢复记录也需恢复，再正常连接。未确认对端已完成。").toString();
                return status;
            }
            if ((action.equals("welcome") || action.equals("join")) && journal.present()) {
                throw new IllegalStateException("请先恢复已确认的配对，不要生成另一套证书");
            }
            String id = NodeIdentityStore.loadOrCreate(context);
            byte[] directory = new AndroidPairingDirectoryStore(context, id).load();
            status = new String(nativeAction(id, Build.MODEL, directory, request), StandardCharsets.UTF_8);
            active = true;
            return status;
        } catch (Exception error) { throw new IllegalStateException(error.getMessage(), error); }
    }
    private synchronized void poll() {
        if (closed || !active) return;
        try {
            status = new String(nativePoll(this), StandardCharsets.UTF_8);
            String phase = new JSONObject(status).getString("phase");
            if (phase.equals("complete") || phase.equals("failed") || phase.equals("cancelled")
                    || (phase.equals("idle") && !new JSONObject(status).optString("discoveryState").equals("scanning"))) active = false;
        } catch (Exception error) {
            try { action("{\"action\":\"cancel\"}"); } catch (Exception ignored) { }
            active = false;
            status = errorStatus(error.getMessage());
        }
    }
    // Called synchronously by nativePoll only after BOTH users approved pinned TLS identities.
    public boolean stageCandidate(byte[] bytes) {
        try { new AndroidEnrollmentJournal(context).stage(bytes); return true; }
        catch (Exception error) { return false; }
        finally { Arrays.fill(bytes, (byte) 0); }
    }
    public boolean commitCandidate() {
        try {
            AndroidEnrollmentJournal journal = new AndroidEnrollmentJournal(context);
            if (!journal.present()) return false;
            journal.commit(); reload.run(); return true;
        } catch (Exception error) { return false; }
    }
    @Override public synchronized void close() {
        if (closed) return;
        try { action("{\"action\":\"cancel\"}"); } catch (Exception ignored) { }
        closed = true; active = false; worker.shutdownNow();
    }
    static String errorStatus(String message) {
        try { return new JSONObject().put("phase", "failed").put("message", message == null ? "配对操作失败" : message).toString(); }
        catch (Exception impossible) { return "{\"phase\":\"failed\",\"message\":\"配对操作失败\"}"; }
    }
    private static native byte[] nativeAction(String nodeId, String name, byte[] directory, String request);
    private static native byte[] nativePoll(AndroidFilelessPairing owner);
    private static native void nativeControlReady(int port);
}
