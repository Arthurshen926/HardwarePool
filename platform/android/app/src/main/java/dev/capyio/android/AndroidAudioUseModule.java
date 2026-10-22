package dev.capyio.android;

import android.Manifest;
import android.content.Context;
import android.content.pm.PackageManager;
import dev.capyio.android.contract.AudioCapabilityKind;
import dev.capyio.android.contract.AudioCapabilitySnapshot;
import dev.capyio.android.contract.AudioCapabilityState;
import dev.capyio.android.contract.AudioNodeSnapshot;
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

    AndroidAudioUseModule(Context context, MicrophoneSourceAdapter microphoneAdapter,
            SpeakerSinkAdapter speakerAdapter, Supplier<AudioNodeSnapshot> snapshot,
            Consumer<AudioCapabilityKind> startCapability, Consumer<AudioCapabilityKind> stopCapability) {
        this.context = context;
        this.microphoneAdapter = microphoneAdapter;
        this.speakerAdapter = speakerAdapter;
        this.snapshot = snapshot;
        this.startCapability = startCapability;
        this.stopCapability = stopCapability;
    }

    void start(AndroidNativeCatalogRuntime runtime, String peer, String direction) {
        usesJson = runtime.audioUse("start", "", peer, direction, 0);
    }

    void stop(AndroidNativeCatalogRuntime runtime, String direction) {
        usesJson = runtime.audioUse("stop", "", "", direction, 0);
        synchronize(runtime);
    }

    // The Service stops hardware controllers after releasing Route resources.
    void close() {
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
                item.remove("config");item.remove("sessionId");item.remove("expiresAtMs");
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
            usesJson = runtime.audioUse("poll", "", "", "", 0);
            org.json.JSONArray uses = new org.json.JSONArray(usesJson);
            java.util.Set<String> live = new java.util.HashSet<>();
            for (int i=0;i<uses.length();i++) if (uses.getJSONObject(i).optBoolean("shouldPrepare")) live.add(uses.getJSONObject(i).getString("routeId"));
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
                // This consumer owns only the legacy lab DatagramSocket path.
                // Do not infer transport from a port or silently downgrade TLS.
                AndroidAudioRouteTransport.requireLegacy(config.getInt("schemaVersion"),
                        config.getString("transport"), config.getString("format"), use.getString("direction"));
                boolean microphone="microphone".equals(use.getString("direction"));
                AudioCapabilityKind kind=microphone?AudioCapabilityKind.MICROPHONE_SOURCE:AudioCapabilityKind.SPEAKER_SINK;
                AudioRouteJob job=jobs.get(id);
                if (job==null) {
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
                        job=new AudioRouteJob(kind,guard,socket);
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
                } catch (RuntimeException | org.json.JSONException | java.io.IOException failed) {
                    AudioRouteJob failedJob=jobs.remove(id);
                    if (failedJob!=null) {failedJob.close();stopCapability.accept(failedJob.kind);}
                    runtime.audioUse("failed",id,"","",0);
                }
            }
        } catch (RuntimeException | org.json.JSONException failed) {
            for (AudioRouteJob job:jobs.values()) {job.close();stopCapability.accept(job.kind);}
            jobs.clear();
        }
    }

    private static final class AudioRouteJob {
        final AudioCapabilityKind kind;
        final dev.capyio.android.lan.NativeLanRouteGuard guard;
        final java.net.DatagramSocket socket;
        boolean started;
        long startedAt;
        AudioRouteJob(AudioCapabilityKind kind, dev.capyio.android.lan.NativeLanRouteGuard guard, java.net.DatagramSocket socket) {
            this.kind = kind;
            this.guard = guard;
            this.socket = socket;
        }

        void close() {
            guard.revoke();
            socket.close();
        }
    }
}
