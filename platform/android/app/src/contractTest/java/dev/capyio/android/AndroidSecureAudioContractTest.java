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
    public static void main(String[] args) {
        startupRollsBackEveryFailure();
        settingsAreExplicitAndClosed();
        secureLaunchIsExactAndImmutable();
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
        return AndroidSecureAudioLaunch.admit(config, ROUTE, SESSION, PEER, "microphone-secure", 1000);
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
            rejects(() -> AndroidSecureAudioLaunch.admit(config(), ROUTE, SESSION, PEER, direction, 1000));
        }
        rejects(() -> AndroidSecureAudioLaunch.admit(config(), "1-1-1-1-1", SESSION, PEER, "microphone-secure", 1000));
        rejects(() -> AndroidSecureAudioLaunch.admit(config(), ROUTE, SESSION, PEER, "microphone-secure", 0));
        check(!launch.equals(AndroidSecureAudioLaunch.admit(config(), PEER, SESSION, ROUTE, "microphone-secure", 1000)));
        check(!launch.equals(AndroidSecureAudioLaunch.admit(config(), ROUTE, PEER, SESSION, "microphone-secure", 1000)));
        check(!launch.equals(AndroidSecureAudioLaunch.admit(config(), ROUTE, SESSION, PEER, "microphone-secure", 1001)));
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
        return AndroidCapabilityModules.createRegistry(new AudioNodeController(), new CapabilityLifecycleController(),
                new GamepadNodeController(), new TouchpadNodeController(), new CapabilityLifecycleController(),
                () -> permission, () -> true,
                () -> new CapabilityLifecycleSnapshot(CapabilityLifecycleState.STOPPED, 0, null), () -> true,
                () -> List.of("cavc-avc-access-unit-v1"), settings::advertisedFormats, true, true,
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
        settings.select(AndroidMicrophoneSettings.Profile.SECURE_MONO_TLS13_V1);
        CapabilitySnapshot mic = microphone(catalog(settings, false, HardwarePresence.PRESENT));
        check(mic.ports().size() == 1 && mic.ports().get(0).direction() == CapabilityPortDirection.SOURCE);
        check(mic.ports().get(0).formats().equals(List.of("pcm-s16le-48000-mono-tls13-v1")));
        check(mic.availability() == CapabilityAvailability.PERMISSION_REQUIRED);
        check(microphone(catalog(settings, true, HardwarePresence.ABSENT)).availability() == CapabilityAvailability.OFFLINE);
        CapabilityCatalogSnapshot eligible = catalog(settings, true, HardwarePresence.PRESENT);
        check(microphone(eligible).availability() == CapabilityAvailability.AVAILABLE);
        CapabilitySnapshot speaker = eligible.capabilities().stream().filter(c -> c.capabilityKey().equals("android.speaker")).findFirst().orElseThrow();
        check(speaker.ports().get(0).formats().equals(List.of("pcm-s16le-48000-stereo")));
        String encoded = AndroidCoreCatalogJson.encode(eligible, "test", "test");
        check(encoded.contains("pcm-s16le-48000-mono-tls13-v1"));
        check(encoded.contains("\"interoperability\":\"adapter_managed\""));
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
