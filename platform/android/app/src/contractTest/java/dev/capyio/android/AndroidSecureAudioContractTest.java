package dev.capyio.android;

import dev.capyio.android.contract.*;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/** Pure metadata/admission checks. No microphone, permission, socket or native library. */
public final class AndroidSecureAudioContractTest {
    private static int assertions;
    private static final String ROUTE = "00000000-0000-4000-8000-000000000001";
    private static final String SESSION = "00000000-0000-4000-8000-000000000002";
    private static final String PEER = "00000000-0000-4000-8000-000000000003";
    public static void main(String[] args) throws Exception {
        startupRollsBackEveryFailure();
        settingsAreExplicitAndClosed();
        secureLaunchIsExactAndImmutable();
        fiveMinuteProfileIsExplicitAndImmutable();
        stalePermissionSelectionCannotStart();
        profileChangesInvalidateFakeCapture();
        transportNeverFallsBack();
        catalogPublishesOnlySelectedEligibility();
        staleLifecycleCompletionCannotReactivate();
        System.out.println("Android secure audio admission: PASS (" + assertions + " assertions)");
    }
    private static void startupRollsBackEveryFailure() {
        for (String failure : List.of("foreground", "native_throw", "native_zero", "construct", "capture_throw", "capture_false", "native_stop_throw", "capture_stop_throw")) {
            java.util.concurrent.atomic.AtomicBoolean gate = new java.util.concurrent.atomic.AtomicBoolean();
            java.util.List<String> events = new java.util.ArrayList<>();
            try {
                AndroidSecureAudioStartup.start(gate, new AndroidSecureAudioStartup.Steps<String>() {
                    @Override public boolean foregroundReady() { return !failure.equals("foreground"); }
                    @Override public long startNative() {
                        check(gate.get()); events.add("native");
                        if (failure.equals("native_throw")) throw new UnsatisfiedLinkError("fake");
                        return failure.equals("native_zero") ? 0 : 7;
                    }
                    @Override public String createCapture(long generation) {
                        check(generation == 7); events.add("construct");
                        if (failure.equals("construct")) throw new IllegalStateException("fake");
                        return "capture";
                    }
                    @Override public boolean startCapture(String capture) {
                        events.add("capture");
                        if (failure.equals("capture_throw") || failure.equals("native_stop_throw") || failure.equals("capture_stop_throw")) throw new IllegalStateException("fake");
                        return false;
                    }
                    @Override public void stopNative(long generation) {
                        check(!gate.get()); check(generation == 7); events.add("native_stop");
                        if (failure.equals("native_stop_throw")) throw new UnsatisfiedLinkError("fake stop");
                    }
                    @Override public void stopCapture(String capture) {
                        check(!gate.get()); events.add("capture_stop");
                        if (failure.equals("capture_stop_throw")) throw new IllegalStateException("fake capture stop");
                    }
                    @Override public void failed() { check(!gate.get()); events.add("failed"); }
                    @Override public void changed() { events.add("changed"); }
                });
                throw new AssertionError("Startup must fail");
            } catch (RuntimeException | LinkageError expected) { assertions++; }
            check(!gate.get());
            check(events.get(events.size() - 2).equals("failed"));
            check(events.get(events.size() - 1).equals("changed"));
            if (List.of("construct", "capture_throw", "capture_false", "native_stop_throw", "capture_stop_throw").contains(failure)) {
                check(events.contains("native_stop"));
            } else check(!events.contains("native_stop"));
            if (List.of("capture_throw", "capture_false", "native_stop_throw", "capture_stop_throw").contains(failure)) {
                check(events.indexOf("capture_stop") > events.indexOf("native_stop"));
                check(events.indexOf("failed") > events.indexOf("capture_stop"));
            } else check(!events.contains("capture_stop"));
        }
    }
    private static void settingsAreExplicitAndClosed() {
        AndroidMicrophoneSettings settings = new AndroidMicrophoneSettings();
        check(!settings.secure());
        check(settings.advertisedFormats().equals(List.of("pcm-s16le-48000-mono")));
        check(settings.acceptsDirection("microphone") && settings.acceptsDirection("speaker"));
        check(!settings.acceptsDirection("microphone-secure") && !settings.acceptsDirection("duplex"));
        for (String value : new String[] {null, "", "secure", "secure-stereo", "legacy ", "system_default"}) {
            rejects(() -> AndroidMicrophoneSettings.parse(value));
        }
        settings.select(AndroidMicrophoneSettings.parse("secure_mono_tls13_v1"));
        check(settings.secure() && settings.revision() == 1);
        check(!settings.acceptsDirection("microphone") && settings.acceptsDirection("microphone-secure"));
        check(settings.acceptsDirection("speaker"));
        check(settings.json(false, true).contains("\"eligibility\":\"eligible\""));
        check(settings.json(false, true).contains("\"permissionReady\":false"));
        check(!settings.json(true, true).contains("verified"));
        check(settings.json(true, false).contains("\"eligibility\":\"unavailable\""));
        settings.select(AndroidMicrophoneSettings.Profile.LEGACY);
        check(settings.revision() == 2 && !settings.secure());
        settings.restore("unknown-profile");
        check(!settings.secure());
    }
    private static Map<String,Object> config() {
        return new HashMap<>(Map.of("schemaVersion", 2, "transport", "pinned-tls13-mono-v1",
                "format", "pcm-s16le-48000-mono-tls13-v1", "pcmBytesPerFrame", 960,
                "sampleRateHz", 48_000, "channels", 1, "samplesPerFrame", 480));
    }
    private static AndroidSecureAudioLaunch admit(Map<String,Object> config) {
        return AndroidSecureAudioLaunch.admit(config, ROUTE, SESSION, PEER, "microphone-secure", 1000, shortSelection(), 1);
    }
    private static void secureLaunchIsExactAndImmutable() {
        rejects(() -> admit(null));
        AndroidSecureAudioLaunch launch = admit(config());
        check(launch.equals(admit(config())));
        for (String key : config().keySet()) {
            Map<String,Object> missing = config(); missing.remove(key); rejects(() -> admit(missing));
            Map<String,Object> changed = config(); changed.put(key, "unknown"); rejects(() -> admit(changed));
        }
        for (String field : List.of("host", "peerPort", "token", "grant", "credential", "generation", "streamId", "epoch", "parameters")) {
            Map<String,Object> extra = config(); extra.put(field, "forbidden"); rejects(() -> admit(extra));
        }
        for (Object value : List.of(2.0, "2", 1, -1, 3)) {
            Map<String,Object> invalid = config(); invalid.put("schemaVersion", value); rejects(() -> admit(invalid));
        }
        for (String direction : List.of("microphone", "speaker", "speaker-secure", "duplex", "")) {
            rejects(() -> AndroidSecureAudioLaunch.admit(config(), ROUTE, SESSION, PEER, direction, 1000, shortSelection(), 1));
        }
        rejects(() -> AndroidSecureAudioLaunch.admit(config(), "1-1-1-1-1", SESSION, PEER, "microphone-secure", 1000, shortSelection(), 1));
        rejects(() -> AndroidSecureAudioLaunch.admit(config(), ROUTE, SESSION, PEER, "microphone-secure", 0, shortSelection(), 1));
        check(!launch.equals(AndroidSecureAudioLaunch.admit(config(), PEER, SESSION, ROUTE, "microphone-secure", 1000, shortSelection(), 1)));
        check(!launch.equals(AndroidSecureAudioLaunch.admit(config(), ROUTE, PEER, SESSION, "microphone-secure", 1000, shortSelection(), 1)));
        check(!launch.equals(AndroidSecureAudioLaunch.admit(config(), ROUTE, SESSION, PEER, "microphone-secure", 1001, shortSelection(), 1)));
    }
    private static AndroidMicrophoneSettings.Selection shortSelection() {
        return new AndroidMicrophoneSettings.Selection(AndroidMicrophoneSettings.Profile.SECURE_MONO_TLS13_V1, 7);
    }
    private static Map<String,Object> config(AndroidMicrophoneSettings.Profile profile) {
        Map<String,Object> selected = config();
        selected.put("format", profile.format);
        return selected;
    }
    private static AndroidSecureAudioLaunch admit(AndroidMicrophoneSettings settings, long expiry) {
        return AndroidSecureAudioLaunch.admit(config(settings.profile()), ROUTE, SESSION, PEER,
                "microphone-secure", expiry, settings.selection(), 1);
    }
    private static void fiveMinuteProfileIsExplicitAndImmutable() {
        AndroidMicrophoneSettings settings = new AndroidMicrophoneSettings();
        AndroidMicrophoneSettings.Profile longer = AndroidMicrophoneSettings.Profile.SECURE_MONO_TLS13_300S_V1;
        check(AndroidMicrophoneSettings.parse("secure_mono_tls13_300s_v1") == longer);
        check(settings.profile() == AndroidMicrophoneSettings.Profile.LEGACY);
        rejects(() -> admit(settings, 300_001));
        settings.select(longer);
        check(settings.secure() && settings.revision() == 1);
        check(settings.advertisedFormats().equals(List.of(AndroidMicrophoneSettings.SECURE_300S_FORMAT)));
        check(settings.acceptsDirection("microphone-secure") && !settings.acceptsDirection("microphone"));
        check(settings.json(false, true).contains("\"captureProfile\":\"secure_mono_tls13_300s_v1\""));
        check(settings.json(false, true).contains("\"permissionReady\":false"));
        AndroidSecureAudioLaunch launch = admit(settings, 300_001);
        check(launch.profile() == longer && launch.settingsRevision() == settings.revision());
        check(launch.matches(settings));
        // The handoff is the same closed transport, never a new or legacy wire mode.
        Map<String,Object> wrongTransport = config(longer);
        wrongTransport.put("transport", "pinned-tls13-mono-300s-v1");
        rejects(() -> AndroidSecureAudioLaunch.admit(wrongTransport, ROUTE, SESSION, PEER,
                "microphone-secure", 300_001, settings.selection(), 1));
        for (String selector : List.of("pinned_tls13_mono_300s_v1", longer.format, "secure_mono_tls13_300s_v2")) {
            rejects(() -> AndroidMicrophoneSettings.parse(selector));
        }
        for (AndroidMicrophoneSettings.Profile selected : AndroidMicrophoneSettings.Profile.values()) {
            settings.select(selected);
            for (AndroidMicrophoneSettings.Profile dto : AndroidMicrophoneSettings.Profile.values()) {
                if (selected.secure() && selected == dto) continue;
                rejects(() -> AndroidSecureAudioLaunch.admit(config(dto), ROUTE, SESSION, PEER,
                        "microphone-secure", 300_001, settings.selection(), 1));
            }
        }
        settings.select(longer);
        check(!launch.matches(settings)); // Returning to the same profile is not the same approval.
        check(!launch.equals(admit(settings, 300_001)));
        AndroidSecureAudioLaunch sameRevisionDifferentProfile = new AndroidSecureAudioLaunch(
                ROUTE, SESSION, PEER, "microphone-secure", AndroidMicrophoneSettings.Profile.SECURE_MONO_TLS13_V1,
                launch.settingsRevision(), launch.expiresAtMs());
        check(!launch.equals(sameRevisionDifferentProfile));
        for (long now : new long[] {0, -1, 300_001, 300_002, Long.MAX_VALUE}) {
            rejects(() -> AndroidSecureAudioLaunch.admit(config(longer), ROUTE, SESSION, PEER,
                    "microphone-secure", 300_001, settings.selection(), now));
        }
        for (long expiry : new long[] {0, -1, 1}) rejects(() -> admit(settings, expiry));
        rejects(() -> AndroidSecureAudioLaunch.admit(config(longer), ROUTE, SESSION, PEER,
                "microphone-secure", 300_001, null, 1));
        settings.select(AndroidMicrophoneSettings.Profile.SECURE_MONO_TLS13_V1);
        // A five-minute Route grant does not turn the old profile into five-minute
        // capture. Native profile/Source lease clocks independently preserve 60s.
        check(admit(settings, 300_001).profile() == AndroidMicrophoneSettings.Profile.SECURE_MONO_TLS13_V1);
        check(!settings.advertisedFormats().contains(longer.format));
        rejects(() -> AndroidAudioRouteTransport.requireLegacy(1, "legacy-udp-v1", longer.format, "microphone"));
        settings.restore("secure_mono_tls13_300s_v1");
        check(settings.profile() == longer);
        settings.restore("unknown");
        check(settings.profile() == AndroidMicrophoneSettings.Profile.LEGACY);
    }
    private static void stalePermissionSelectionCannotStart() {
        AndroidMicrophoneSettings settings = new AndroidMicrophoneSettings();
        AndroidMicrophoneSettings.Selection pending = settings.selection();
        AndroidMicrophoneSettings.PendingPermission dialog = new AndroidMicrophoneSettings.PendingPermission();
        check(dialog.begin(pending) && dialog.awaiting());
        check(settings.canStartLegacy(pending));
        check(!settings.canStartLegacy(null));
        settings.invalidateRevision(); // Invalidation precedes publication and selection.
        check(!settings.canStartLegacy(pending));
        check(!dialog.begin(settings.selection())); // Do not relabel an older dialog as a new request.
        check(dialog.finish().equals(pending) && !dialog.awaiting());
        check(!settings.canStartLegacy(pending));
        check(dialog.finish() == null); // A duplicate callback cannot replay Start.
        check(dialog.begin(settings.selection()));
        dialog.cancel(); // Stop clears intent but retains the outstanding callback's ownership.
        check(dialog.awaiting() && !dialog.begin(settings.selection()));
        check(dialog.finish() == null && !dialog.awaiting());
        for (AndroidMicrophoneSettings.Profile selected : List.of(
                AndroidMicrophoneSettings.Profile.SECURE_MONO_TLS13_V1,
                AndroidMicrophoneSettings.Profile.SECURE_MONO_TLS13_300S_V1)) {
            settings.select(selected);
            check(dialog.begin(pending));
            check(!settings.canStartLegacy(pending));
            check(!settings.canStartLegacy(dialog.finish()));
            check(!settings.canStartLegacy(settings.selection()));
            settings.select(AndroidMicrophoneSettings.Profile.LEGACY);
            check(!settings.canStartLegacy(pending));
            check(settings.canStartLegacy(settings.selection()));
        }
        check(dialog.begin(settings.selection()));
        check(settings.canStartLegacy(dialog.finish()));
        rejects(() -> new AndroidMicrophoneSettings.Selection(null, 0));
        rejects(() -> new AndroidMicrophoneSettings.Selection(AndroidMicrophoneSettings.Profile.LEGACY, -1));
        rejects(() -> new AndroidMicrophoneSettings.Selection(AndroidMicrophoneSettings.Profile.LEGACY, Long.MAX_VALUE));
    }
    private static void profileChangesInvalidateFakeCapture() throws Exception {
        check(SecureMonoCaptureSession.RECORD_BYTES == 960);
        check(SecureMonoCaptureSession.MAX_FRAME_READS == 128);
        check(SecureMonoCaptureSession.FRAME_DEADLINE_NANOS == 250_000_000L);
        check(SecureMonoCaptureSession.MAX_REAP_MILLIS == 250);
        for (AndroidMicrophoneSettings.Profile selected : List.of(
                AndroidMicrophoneSettings.Profile.SECURE_MONO_TLS13_V1,
                AndroidMicrophoneSettings.Profile.SECURE_MONO_TLS13_300S_V1)) {
            AndroidMicrophoneSettings.Profile replacement = selected == AndroidMicrophoneSettings.Profile.SECURE_MONO_TLS13_V1
                    ? AndroidMicrophoneSettings.Profile.SECURE_MONO_TLS13_300S_V1
                    : AndroidMicrophoneSettings.Profile.SECURE_MONO_TLS13_V1;
            for (String phase : List.of("waiting", "open", "active", "revision", "permission")) {
                FakeCapture f = new FakeCapture(selected);
                Runnable invalidate = () -> {
                    f.settings.invalidateRevision(); // Revokes worker admission before fresh selection/publication.
                    check(!f.launch.matches(f.settings));
                    if (!phase.equals("revision")) f.settings.select(replacement);
                };
                if (phase.equals("waiting")) {
                    f.nativeState = SecureMonoCaptureSession.NativeState.WAITING;
                    f.onPoll = () -> {
                        invalidate.run();
                        f.nativeState = SecureMonoCaptureSession.NativeState.READY; // Late native callback.
                    };
                } else if (phase.equals("open")) {
                    f.onOpen = invalidate;
                } else {
                    f.onRead = () -> {
                        if (phase.equals("permission")) f.permission = false;
                        else if (!phase.equals("active") || f.reads == 2) {
                            if (phase.equals("active")) check(f.session.snapshot().state() == SecureMonoCaptureSession.State.ACTIVE);
                            invalidate.run();
                        }
                    };
                }
                check(f.session.start());
                awaitCapture(f.session);
                check(f.session.snapshot().state() == SecureMonoCaptureSession.State.FAILED);
                check(SecureMonoCaptureSession.PROBLEM_PERMISSION.equals(f.session.snapshot().problemCode()));
                check(f.session.snapshot().cleanupComplete() && !f.session.snapshot().cleanupFailed());
                check(f.records == (phase.equals("active") ? 1 : 0));
                check(f.opens == (phase.equals("waiting") ? 0 : 1));
                check(f.releases == f.opens && f.nativeStops == 1);
                check(f.starts == (phase.equals("waiting") || phase.equals("open") ? 0 : 1));
                if (phase.equals("permission")) {
                    f.permission = true;
                    check(f.launch.matches(f.settings));
                    check(!f.session.start()); // Permission alone cannot restart a terminal generation.
                }
                f.settings.select(selected);
                f.permission = true; // A late permission return cannot revive the old capture.
                f.nativeState = SecureMonoCaptureSession.NativeState.ACTIVE;
                check(!f.launch.matches(f.settings));
                check(!f.session.start());
                check(f.session.snapshot().state() == SecureMonoCaptureSession.State.FAILED);
                f.session.stop();
                check(f.nativeStops == 1 && f.releases == f.opens);
            }
        }
    }
    private static void awaitCapture(SecureMonoCaptureSession session) throws Exception {
        for (int attempt = 0; attempt < 20; attempt++) if (session.awaitStopped(250)) return;
        session.stop();
        throw new AssertionError("Fake capture did not finish");
    }
    /** Uses the production capture state machine, with no Android classes or real native sender. */
    private static final class FakeCapture implements SecureMonoCaptureSession.Platform,
            SecureMonoCaptureSession.Recorder, SecureMonoCaptureSession.NativeSender {
        final AndroidMicrophoneSettings settings = new AndroidMicrophoneSettings();
        final AndroidSecureAudioLaunch launch;
        final SecureMonoCaptureSession session;
        SecureMonoCaptureSession.NativeState nativeState = SecureMonoCaptureSession.NativeState.READY;
        boolean permission = true;
        boolean stopped;
        long now;
        int opens;
        int starts;
        int reads;
        int records;
        int releases;
        int nativeStops;
        Runnable onPoll = () -> {};
        Runnable onOpen = () -> {};
        Runnable onRead = () -> {};
        FakeCapture(AndroidMicrophoneSettings.Profile selected) {
            settings.select(selected);
            launch = admit(settings, 300_001);
            session = new SecureMonoCaptureSession(19, this, this);
        }
        @Override public boolean permissionGranted() { return permission && launch.matches(settings); }
        @Override public SecureMonoCaptureSession.Recorder openRecorder(SecureMonoCaptureSession.Cancellation cancellation) {
            opens++;
            onOpen.run();
            return this;
        }
        @Override public long nanoTime() { return now; }
        @Override public void awaitPoll(long nanos) { now += nanos; onPoll.run(); }
        @Override public ActualAudioFormat actualFormat() { return new ActualAudioFormat(48_000, 1, "pcm_s16le", 1_920); }
        @Override public int audioSource() { return SecureMonoCaptureSession.MICROPHONE_SOURCE; }
        @Override public void start() { starts++; }
        @Override public int routedDeviceId() { return 11; }
        @Override public long routingGeneration() { return 0; }
        @Override public int read(byte[] target, int offset, int length) {
            reads++;
            check(target.length == 960 && length == 960 && offset == 0);
            onRead.run();
            return length;
        }
        @Override public void stop() {}
        @Override public void release() { releases++; }
        @Override public SecureMonoCaptureSession.NativeState state(long generation) {
            check(generation == 19);
            return stopped ? SecureMonoCaptureSession.NativeState.STOPPED : nativeState;
        }
        @Override public boolean record(long generation, byte[] pcm960) {
            check(generation == 19 && pcm960.length == 960 && permissionGranted() && !stopped);
            records++;
            nativeState = SecureMonoCaptureSession.NativeState.ACTIVE;
            return true;
        }
        @Override public void stop(long generation) { check(generation == 19); nativeStops++; stopped = true; }
    }
    private static void transportNeverFallsBack() {
        AndroidAudioRouteTransport.requireLegacy(1, "legacy-udp-v1", "pcm-s16le-48000-mono", "microphone");
        for (String transport : new String[] {null, "", "unknown", "pinned-tls13-mono-v1"}) {
            rejects(() -> AndroidAudioRouteTransport.requireLegacy(1, transport, "pcm-s16le-48000-mono", "microphone"));
        }
        rejects(() -> AndroidAudioRouteTransport.requireLegacy(2, "legacy-udp-v1", "pcm-s16le-48000-mono", "microphone"));
        rejects(() -> AndroidAudioRouteTransport.requireLegacy(1, "legacy-udp-v1", "pcm-s16le-48000-mono-tls13-v1", "microphone"));
        rejects(() -> AndroidAudioRouteTransport.requireLegacy(1, "legacy-udp-v1", "pcm-s16le-48000-mono", "microphone-secure"));
    }
    private static CapabilityCatalogSnapshot catalog(AndroidMicrophoneSettings settings, boolean permission, HardwarePresence presence) {
        return catalogFormats(settings.advertisedFormats(), permission, presence);
    }
    private static CapabilityCatalogSnapshot catalogFormats(List<String> formats, boolean permission, HardwarePresence presence) {
        return AndroidCapabilityModules.createRegistry(new AudioNodeController(), new CapabilityLifecycleController(),
                new GamepadNodeController(), new TouchpadNodeController(), new CapabilityLifecycleController(),
                () -> permission, () -> true,
                () -> new CapabilityLifecycleSnapshot(CapabilityLifecycleState.STOPPED, 0, null), () -> true,
                () -> List.of("cavc-avc-access-unit-v1"), () -> formats, true, true,
                presence, HardwarePresence.PRESENT, HardwarePresence.PRESENT, HardwarePresence.PRESENT,
                HardwarePresence.PRESENT, HardwarePresence.PRESENT).snapshot(PEER);
    }
    private static CapabilitySnapshot microphone(CapabilityCatalogSnapshot catalog) {
        return catalog.capabilities().stream().filter(c -> c.capabilityKey().equals("android.microphone")).findFirst().orElseThrow();
    }
    private static void catalogPublishesOnlySelectedEligibility() {
        AndroidMicrophoneSettings settings = new AndroidMicrophoneSettings();
        check(microphone(catalog(settings, true, HardwarePresence.PRESENT)).ports().get(0).formats()
                .equals(List.of("pcm-s16le-48000-mono")));
        for (AndroidMicrophoneSettings.Profile selected : List.of(
                AndroidMicrophoneSettings.Profile.SECURE_MONO_TLS13_V1,
                AndroidMicrophoneSettings.Profile.SECURE_MONO_TLS13_300S_V1)) {
            settings.select(selected);
            CapabilitySnapshot mic = microphone(catalog(settings, false, HardwarePresence.PRESENT));
            check(mic.ports().size() == 1 && mic.ports().get(0).direction() == CapabilityPortDirection.SOURCE);
            check(mic.ports().get(0).formats().equals(List.of(selected.format)));
            check(mic.availability() == CapabilityAvailability.PERMISSION_REQUIRED);
            check(microphone(catalog(settings, true, HardwarePresence.ABSENT)).availability() == CapabilityAvailability.OFFLINE);
            CapabilityCatalogSnapshot eligible = catalog(settings, true, HardwarePresence.PRESENT);
            check(microphone(eligible).availability() == CapabilityAvailability.AVAILABLE);
            CapabilitySnapshot speaker = eligible.capabilities().stream().filter(c -> c.capabilityKey().equals("android.speaker")).findFirst().orElseThrow();
            check(speaker.ports().get(0).formats().equals(List.of("pcm-s16le-48000-stereo")));
            String encoded = AndroidCoreCatalogJson.encode(eligible, "test", "test");
            check(encoded.contains(selected.format));
            check(encoded.contains("\"interoperability\":\"adapter_managed\""));
        }
        for (List<String> invalid : List.of(List.<String>of(), List.of("unknown"),
                List.of(AndroidMicrophoneSettings.SECURE_FORMAT, AndroidMicrophoneSettings.SECURE_300S_FORMAT),
                List.of(AndroidMicrophoneSettings.LEGACY_FORMAT, AndroidMicrophoneSettings.SECURE_300S_FORMAT))) {
            rejects(() -> catalogFormats(invalid, true, HardwarePresence.PRESENT));
        }
    }
    private static void staleLifecycleCompletionCannotReactivate() {
        AudioNodeController controller = new AudioNodeController();
        long old = controller.beginStart(AudioCapabilityKind.MICROPHONE_SOURCE).generation();
        long stop = controller.beginStop(AudioCapabilityKind.MICROPHONE_SOURCE).generation();
        ActualAudioFormat actual = new ActualAudioFormat(48_000, 1, "pcm_s16le", 480);
        check(!controller.completeStart(AudioCapabilityKind.MICROPHONE_SOURCE, old, actual));
        check(controller.completeStop(AudioCapabilityKind.MICROPHONE_SOURCE, stop));
        long fresh = controller.beginStart(AudioCapabilityKind.MICROPHONE_SOURCE).generation();
        check(!controller.completeStart(AudioCapabilityKind.MICROPHONE_SOURCE, old, actual));
        check(!controller.completeStop(AudioCapabilityKind.MICROPHONE_SOURCE, stop));
        check(controller.completeStart(AudioCapabilityKind.MICROPHONE_SOURCE, fresh, actual));
        controller.addFrames(AudioCapabilityKind.MICROPHONE_SOURCE, old, 480);
        check(controller.snapshot("test").microphone().framesProcessed() == 0);
        long cleanup = controller.beginStop(AudioCapabilityKind.MICROPHONE_SOURCE).generation();
        check(!controller.failStop(AudioCapabilityKind.MICROPHONE_SOURCE, stop, "CAPY.CLEANUP_FAILED"));
        check(controller.failStop(AudioCapabilityKind.MICROPHONE_SOURCE, cleanup, "CAPY.CLEANUP_FAILED"));
        check(controller.snapshot("test").microphone().state() == AudioCapabilityState.FAILED);
        check(!controller.completeStop(AudioCapabilityKind.MICROPHONE_SOURCE, cleanup));
    }
    private static void rejects(Runnable action) {
        try { action.run(); } catch (IllegalArgumentException expected) { assertions++; return; }
        throw new AssertionError("Expected rejection");
    }
    private static void check(boolean condition) { assertions++; if (!condition) throw new AssertionError("assertion " + assertions); }
}
