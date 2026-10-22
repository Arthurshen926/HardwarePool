package dev.capyio.android;

import android.Manifest;
import android.content.Context;
import android.content.pm.PackageManager;
import dev.capyio.android.contract.AudioCapabilityKind;
import dev.capyio.android.contract.AudioCapabilitySnapshot;
import dev.capyio.android.contract.AudioCapabilityState;
import dev.capyio.android.contract.AudioNodeSnapshot;
import dev.capyio.android.contract.AudioNodeController;
import dev.capyio.android.contract.SecureMonoCaptureSession;
import dev.capyio.android.contract.TransitionToken;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.function.BooleanSupplier;
import java.util.function.Consumer;
import java.util.function.Supplier;

/** Owns microphone/speaker Route jobs; called under the Node Service monitor. */
final class AndroidAudioUseModule {
    private final Context context;
    private final MicrophoneSourceAdapter microphoneAdapter;
    private final SpeakerSinkAdapter speakerAdapter;
    private final Supplier<AudioNodeSnapshot> snapshot;
    private final Consumer<AudioCapabilityKind> startCapability;
    private final Consumer<AudioCapabilityKind> stopCapability;
    private final java.util.Map<String, AudioRouteJob> jobs = new java.util.HashMap<>();
    private String usesJson = "[]";
    private final AudioNodeController controller;
    private final BooleanSupplier foregroundReady;
    private final AtomicBoolean securePermitted;
    private final BooleanSupplier secureUiVisible;
    private final AndroidMicrophoneSettings settings;
    private final Runnable stateChanged;
    private final java.util.Map<String, SecureAudioJob> secureJobs = new java.util.HashMap<>();
    private final java.util.List<SecureAudioJob> retiring = new java.util.ArrayList<>();

    AndroidAudioUseModule(Context context, MicrophoneSourceAdapter microphoneAdapter,
            SpeakerSinkAdapter speakerAdapter, Supplier<AudioNodeSnapshot> snapshot,
            Consumer<AudioCapabilityKind> startCapability, Consumer<AudioCapabilityKind> stopCapability,
            AudioNodeController controller, BooleanSupplier foregroundReady, AtomicBoolean securePermitted,
            BooleanSupplier secureUiVisible, AndroidMicrophoneSettings settings, Runnable stateChanged) {
        this.context = context;
        this.microphoneAdapter = microphoneAdapter;
        this.speakerAdapter = speakerAdapter;
        this.snapshot = snapshot;
        this.startCapability = startCapability;
        this.stopCapability = stopCapability;
        this.controller = controller;
        this.foregroundReady = foregroundReady;
        this.securePermitted = securePermitted;
        this.secureUiVisible = secureUiVisible;
        this.settings = settings;
        this.stateChanged = stateChanged;
    }

    void start(AndroidNativeCatalogRuntime runtime, String peer, String direction) {
        if (!settings.acceptsDirection(direction)) throw new IllegalArgumentException("Audio profile mismatch");
        usesJson = runtime.audioUse("start", "", peer, direction, 0);
    }

    void stop(AndroidNativeCatalogRuntime runtime, String direction) {
        usesJson = runtime.audioUse("stop", "", "", direction, 0);
        synchronize(runtime);
    }

    // The Service stops hardware controllers after releasing Route resources.
    void close() {
        securePermitted.set(false);
        for (SecureAudioJob job : new java.util.ArrayList<>(secureJobs.values())) retire(job);
        secureJobs.clear();
        for (AudioRouteJob job : jobs.values()) {
            job.close();
        }
        jobs.clear();
    }

    String publicStateJson() {
        try {
            org.json.JSONArray uses=new org.json.JSONArray(usesJson);
            java.util.Map<String,org.json.JSONObject> current=new java.util.LinkedHashMap<>();
            for(int i=0;i<uses.length();i++) {
                org.json.JSONObject item=uses.getJSONObject(i);
                item.remove("config");item.remove("secureConfig");item.remove("sessionId");item.remove("expiresAtMs");
                if ("microphone-secure".equals(item.optString("direction")) && "active".equals(item.optString("status"))) {
                    item.put("message", "安全音频已通过认证通道发送；不代表接收端已播放或听见");
                }
                String direction=item.getString("direction");
                org.json.JSONObject old=current.get(direction);
                boolean live=!"idle".equals(item.optString("status"))&&!"failed".equals(item.optString("status"));
                boolean oldLive=old!=null&&!"idle".equals(old.optString("status"))&&!"failed".equals(old.optString("status"));
                if(old==null||live||(!oldLive&&!"idle".equals(item.optString("status")))) current.put(direction,item);
            }
            return new org.json.JSONArray(current.values()).toString();
        } catch(org.json.JSONException invalid) {return "[]";}
    }

    void synchronize(AndroidNativeCatalogRuntime runtime) {
        try {
            reapRetiring();
            usesJson = runtime.audioUse("poll", "", "", "", 0);
            org.json.JSONArray uses = new org.json.JSONArray(usesJson);
            java.util.Set<String> live = new java.util.HashSet<>();
            for (int i=0;i<uses.length();i++) if (uses.getJSONObject(i).optBoolean("shouldPrepare")) live.add(uses.getJSONObject(i).getString("routeId"));
            for (String id : new java.util.ArrayList<>(secureJobs.keySet())) {
                if (!live.contains(id)) retire(secureJobs.remove(id));
            }
            for (String id : new java.util.ArrayList<>(jobs.keySet())) {
                if (!live.contains(id)) {
                    AudioRouteJob job=jobs.remove(id);
                    job.close(); stopCapability.accept(job.kind);
                }
            }
            for (int i=0;i<uses.length();i++) {
                org.json.JSONObject use=uses.getJSONObject(i);
                if (!use.optBoolean("shouldPrepare")) continue;
                String id=use.getString("routeId");
                try {
                org.json.JSONObject config=use.getJSONObject("config");
                // Dispatch BEFORE looking up any existing legacy job or opening any socket.
                // A secure direction with a missing/wrong transport must never fall through.
                if (AndroidSecureAudioLaunch.TRANSPORT.equals(config.optString("transport"))
                        || AndroidSecureAudioLaunch.DIRECTION.equals(use.optString("direction"))) {
                    synchronizeSecure(runtime, use, config);
                    continue;
                }
                if (secureJobs.containsKey(id)) throw new IllegalArgumentException("Secure transport changed");
                AndroidAudioRouteTransport.requireLegacy(config.getInt("schemaVersion"),
                        config.getString("transport"), config.getString("format"), use.getString("direction"));
                if (!settings.acceptsDirection(use.getString("direction"))) {
                    throw new IllegalArgumentException("Audio profile changed");
                }
                boolean microphone="microphone".equals(use.getString("direction"));
                AudioCapabilityKind kind=microphone?AudioCapabilityKind.MICROPHONE_SOURCE:AudioCapabilityKind.SPEAKER_SINK;
                AudioRouteJob job=jobs.get(id);
                if (job != null && !job.binding.equals(legacyBinding(use, config))) {
                    throw new IllegalArgumentException("Legacy audio binding changed");
                }
                if (job==null) {
                    if (microphone && (!secureJobs.isEmpty() || !retiring.isEmpty()
                            || AndroidSecureMicrophoneCapture.hasUnreleasedCapture())) {
                        runtime.audioUse("busy", id,"","",0); continue;
                    }
                    AudioCapabilitySnapshot state=microphone?snapshot.get().microphone():snapshot.get().speaker();
                    if (state.state()!=AudioCapabilityState.STOPPED && state.state()!=AudioCapabilityState.FAILED) {
                        runtime.audioUse("busy", id,"","",0); continue;
                    }
                    dev.capyio.android.lan.NativeLanRouteBinding binding=new dev.capyio.android.lan.NativeLanRouteBinding(
                            java.util.UUID.fromString(use.getString("sessionId")), java.util.UUID.fromString(id),
                            java.util.UUID.fromString(config.getString("streamId")), config.getLong("epoch"), use.getLong("expiresAtMs"),System.currentTimeMillis());
                    dev.capyio.android.lan.NativeLanRouteGuard guard=new dev.capyio.android.lan.NativeLanRouteGuard(binding);
                    java.net.DatagramSocket socket=new java.net.DatagramSocket(0);
                    boolean configured=false;
                    try {
                        configured=microphone?microphoneAdapter.configureRoute(binding.microphoneBaselineConfig(config.getString("host"),socket.getLocalPort(),config.getInt("peerPort"),System.currentTimeMillis()),guard,socket)
                                :speakerAdapter.configureRoute(binding.speakerBaselineConfig(config.getString("host"),socket.getLocalPort(),config.getInt("peerPort"),System.currentTimeMillis()),guard,socket);
                        if (!configured) {guard.revoke();socket.close();continue;}
                        job=new AudioRouteJob(kind,guard,socket,legacyBinding(use, config));
                        jobs.put(id,job);
                        runtime.audioUse("prepared",id,"","",socket.getLocalPort());
                    } catch (RuntimeException | org.json.JSONException failed) {guard.revoke();socket.close();throw failed;}
                }
                if (use.optBoolean("shouldRun") && !job.started) {
                    if (microphone && context.checkSelfPermission(Manifest.permission.RECORD_AUDIO)!=PackageManager.PERMISSION_GRANTED) {
                        runtime.audioUse("failed",id,"","",0); continue;
                    }
                    job.started=true; job.startedAt=android.os.SystemClock.elapsedRealtime();
                    startCapability.accept(kind);
                }
                AudioCapabilitySnapshot state=microphone?snapshot.get().microphone():snapshot.get().speaker();
                if (job.started && state.state()==AudioCapabilityState.ACTIVE && state.framesProcessed()>0 && !"active".equals(use.optString("status"))) {
                    runtime.audioUse("active",id,"","",0);
                } else if (job.started && (state.state()==AudioCapabilityState.FAILED || state.state()==AudioCapabilityState.STOPPED
                        || (microphone && state.framesProcessed()==0 && android.os.SystemClock.elapsedRealtime()-job.startedAt>15_000))) {
                    runtime.audioUse("failed",id,"","",0);
                }
                } catch (RuntimeException | LinkageError | org.json.JSONException | java.io.IOException failed) {
                    SecureAudioJob secureJob = secureJobs.remove(id);
                    if (secureJob != null) retire(secureJob);
                    AudioRouteJob failedJob=jobs.remove(id);
                    if (failedJob!=null) {failedJob.close();stopCapability.accept(failedJob.kind);}
                    runtime.audioUse("failed",id,"","",0);
                }
            }
        } catch (RuntimeException | org.json.JSONException failed) {
            securePermitted.set(false);
            for (SecureAudioJob job : new java.util.ArrayList<>(secureJobs.values())) retire(job);
            secureJobs.clear();
            for (AudioRouteJob job:jobs.values()) {job.close();stopCapability.accept(job.kind);}
            jobs.clear();
        }
    }

    /** Invalidate both transport generations before changing the Source or any settings. */
    void invalidateMicrophone(AndroidNativeCatalogRuntime runtime) {
        securePermitted.set(false);
        try {
            invalidateSecureMicrophone(runtime);
        } finally {
            try { runtime.audioUse("stop", "", "", "microphone", 0); }
            finally { closeLegacyMicrophone(); }
        }
    }

    void invalidateSecureMicrophone(AndroidNativeCatalogRuntime runtime) {
        securePermitted.set(false);
        try { usesJson = runtime.audioUse("stop", "", "", "microphone-secure", 0); }
        finally {
            for (SecureAudioJob job : new java.util.ArrayList<>(secureJobs.values())) retire(job);
            secureJobs.clear();
        }
    }

    private void closeLegacyMicrophone() {
        for (String id : new java.util.ArrayList<>(jobs.keySet())) {
            AudioRouteJob job = jobs.get(id);
            if (job.kind == AudioCapabilityKind.MICROPHONE_SOURCE) {
                jobs.remove(id); job.close(); stopCapability.accept(job.kind);
            }
        }
    }

    boolean ownsSecureMicrophone() { return !secureJobs.isEmpty() || !retiring.isEmpty(); }

    private void synchronizeSecure(AndroidNativeCatalogRuntime runtime, org.json.JSONObject use,
            org.json.JSONObject config) throws org.json.JSONException {
        java.util.Map<String, Object> fields = new java.util.HashMap<>();
        java.util.Iterator<String> names = config.keys();
        while (names.hasNext()) { String key = names.next(); fields.put(key, config.get(key)); }
        AndroidSecureAudioLaunch launch = AndroidSecureAudioLaunch.admit(fields, use.getString("routeId"),
                use.getString("sessionId"), use.getString("peerNodeId"), use.getString("direction"),
                strictLong(use, "expiresAtMs"));
        String id = launch.routeId();
        if (!settings.secure() || jobs.containsKey(id) || launch.expiresAtMs() <= System.currentTimeMillis()) {
            throw new IllegalArgumentException("Secure microphone selection or binding changed");
        }
        SecureAudioJob job = secureJobs.get(id);
        if (job != null && !job.launch.equals(launch)) {
            throw new IllegalArgumentException("Immutable secure audio binding changed");
        }
        if (!(use.get("shouldRun") instanceof Boolean)) throw new IllegalArgumentException("Invalid secure run state");
        if (!use.getBoolean("shouldRun")) {
            if (job != null) retire(secureJobs.remove(id));
            return;
        }
        if (context.checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) {
            throw new IllegalArgumentException("Microphone permission required");
        }
        if (job == null) {
            if (!retiring.isEmpty() || !secureJobs.isEmpty() || AndroidSecureMicrophoneCapture.hasUnreleasedCapture()) {
                throw new IllegalStateException("Microphone cleanup pending");
            }
            TransitionToken transition = controller.beginStart(AudioCapabilityKind.MICROPHONE_SOURCE);
            if (!transition.accepted()) { runtime.audioUse("busy", id, "", "", 0); return; }
            job = AndroidSecureAudioStartup.start(securePermitted, new AndroidSecureAudioStartup.Steps<SecureAudioJob>() {
                @Override public boolean foregroundReady() { return AndroidAudioUseModule.this.foregroundReady.getAsBoolean(); }
                @Override public long startNative() { return runtime.audioSecureStart(id); }
                @Override public SecureAudioJob createCapture(long generation) {
                    SecureMonoCaptureSession.NativeSender sender = new SecureMonoCaptureSession.NativeSender() {
                        @Override public SecureMonoCaptureSession.NativeState state(long value) {
                            return switch (runtime.audioSecureState(value)) {
                                case 0 -> SecureMonoCaptureSession.NativeState.WAITING;
                                case 1 -> SecureMonoCaptureSession.NativeState.READY;
                                case 4 -> SecureMonoCaptureSession.NativeState.ACTIVE;
                                case 3 -> SecureMonoCaptureSession.NativeState.STOPPED;
                                default -> SecureMonoCaptureSession.NativeState.FAILED;
                            };
                        }
                        @Override public boolean record(long value, byte[] pcm960) { return runtime.audioSecureRecord(value, pcm960); }
                        @Override public void stop(long value) { runtime.audioSecureStop(value); }
                    };
                    AndroidSecureMicrophoneCapture capture = new AndroidSecureMicrophoneCapture(
                            context, () -> securePermitted.get() && secureUiVisible.getAsBoolean(), generation, sender);
                    SecureAudioJob created = new SecureAudioJob(launch, transition.generation(), generation, runtime, capture);
                    secureJobs.put(id, created); // Retain cleanup ownership before any worker can start.
                    return created;
                }
                @Override public boolean startCapture(SecureAudioJob created) { return created.capture.start(); }
                @Override public void stopNative(long generation) { runtime.audioSecureStop(generation); }
                @Override public void stopCapture(SecureAudioJob created) { created.capture.stop(); }
                @Override public void failed() {
                    controller.fail(AudioCapabilityKind.MICROPHONE_SOURCE, transition.generation(), "CAPY.ANDROID.MIC_SECURE_START_FAILED");
                }
                @Override public void changed() { stateChanged.run(); }
            });
        }
        SecureMonoCaptureSession.Snapshot capture = job.capture.snapshot();
        if (capture.state() == SecureMonoCaptureSession.State.FAILED
                || capture.state() == SecureMonoCaptureSession.State.STOPPED) {
            throw new IllegalStateException("Secure microphone capture ended");
        }
        // Native ACTIVE means an authenticated media write succeeded, not just enqueue or AudioRecord start.
        if (capture.state() == SecureMonoCaptureSession.State.ACTIVE
                && runtime.audioSecureState(job.generation) == 4 && capture.actualFormat() != null) {
            if (!"active".equals(use.optString("status"))) {
                runtime.audioUse("active", id, "", "", 0);
            }
            controller.completeStart(AudioCapabilityKind.MICROPHONE_SOURCE, job.lifecycleGeneration, capture.actualFormat());
            long frames = Math.multiplyExact(capture.acceptedFrames(), 480L);
            if (frames > job.reportedFrames) {
                controller.addFrames(AudioCapabilityKind.MICROPHONE_SOURCE, job.lifecycleGeneration, frames - job.reportedFrames);
                job.reportedFrames = frames;
                stateChanged.run();
            }
        }
    }

    private void retire(SecureAudioJob job) {
        if (job == null) return;
        securePermitted.set(false);
        try { job.runtime.audioSecureStop(job.generation); }
        catch (RuntimeException | LinkageError unavailable) { /* The atomic gate remains revoked. */ }
        try { job.capture.stop(); } // Signals only; never join under the service monitor.
        finally {
            TransitionToken stop = controller.beginStop(AudioCapabilityKind.MICROPHONE_SOURCE);
            job.stopGeneration = stop.generation();
            retiring.add(job);
            stateChanged.run();
        }
    }

    private void reapRetiring() {
        for (SecureAudioJob job : new java.util.ArrayList<>(retiring)) {
            if (!job.capture.snapshot().cleanupComplete()) continue;
            if (job.capture.snapshot().cleanupFailed()) {
                if (controller.failStop(AudioCapabilityKind.MICROPHONE_SOURCE, job.stopGeneration,
                        "CAPY.ANDROID.SECURE_MIC_CLEANUP_FAILED")) stateChanged.run();
                continue; // Retain quarantine even though the cleanup attempt ended.
            }
            retiring.remove(job);
            controller.completeStop(AudioCapabilityKind.MICROPHONE_SOURCE, job.stopGeneration);
            stateChanged.run();
        }
    }

    private static long strictLong(org.json.JSONObject value, String key) throws org.json.JSONException {
        Object field = value.get(key);
        if (!(field instanceof Integer || field instanceof Long)) throw new IllegalArgumentException("Invalid secure integer");
        return ((Number) field).longValue();
    }

    private record LegacyBinding(String session, String peer, String direction, long expires,
            String stream, long epoch, String host, int peerPort) {}
    private static LegacyBinding legacyBinding(org.json.JSONObject use, org.json.JSONObject config)
            throws org.json.JSONException {
        return new LegacyBinding(use.getString("sessionId"), use.getString("peerNodeId"),
                use.getString("direction"), use.getLong("expiresAtMs"), config.getString("streamId"),
                config.getLong("epoch"), config.getString("host"), config.getInt("peerPort"));
    }
    private static final class SecureAudioJob {
        final AndroidSecureAudioLaunch launch;
        final long lifecycleGeneration;
        final long generation;
        final AndroidNativeCatalogRuntime runtime;
        final AndroidSecureMicrophoneCapture capture;
        long reportedFrames;
        long stopGeneration;
        SecureAudioJob(AndroidSecureAudioLaunch launch, long lifecycleGeneration, long generation,
                AndroidNativeCatalogRuntime runtime, AndroidSecureMicrophoneCapture capture) {
            this.launch = launch; this.lifecycleGeneration = lifecycleGeneration; this.generation = generation;
            this.runtime = runtime; this.capture = capture;
        }
    }

    private static final class AudioRouteJob {
        final AudioCapabilityKind kind;
        final dev.capyio.android.lan.NativeLanRouteGuard guard;
        final java.net.DatagramSocket socket;
        final LegacyBinding binding;
        boolean started;
        long startedAt;
        AudioRouteJob(AudioCapabilityKind kind, dev.capyio.android.lan.NativeLanRouteGuard guard, java.net.DatagramSocket socket, LegacyBinding binding) {
            this.kind = kind;
            this.guard = guard;
            this.socket = socket;
            this.binding = binding;
        }

        void close() {
            guard.revoke();
            socket.close();
        }
    }
}
