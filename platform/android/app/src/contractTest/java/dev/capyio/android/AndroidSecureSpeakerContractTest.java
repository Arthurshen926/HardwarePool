package dev.capyio.android;

import java.util.*;
import java.nio.file.*;
import dev.capyio.android.contract.*;

/** Pure settings/catalog/dispatch checks: no Android device, JNI or sockets. */
public final class AndroidSecureSpeakerContractTest {
    private static int assertions;
    private static final String ID = "00000000-0000-4000-8000-000000000001";
    public static void main(String[] args) throws Exception {
        AndroidSpeakerSettings settings = new AndroidSpeakerSettings();
        check(!settings.secure() && settings.acceptsDirection("speaker"));
        check(!settings.acceptsDirection("speaker-secure") && !settings.acceptsDirection("microphone"));
        AndroidMicrophoneSettings mic = new AndroidMicrophoneSettings();
        var pending = new AndroidMicrophoneSettings.PendingPermission();
        pending.begin(mic.selection());
        settings.select(AndroidSpeakerSettings.Profile.SECURE_STEREO_TLS13_V1);
        check(mic.canStartLegacy(pending.finish())); // Speaker opt-in does not cancel microphone dialog.
        check(settings.secure() && settings.acceptsDirection("speaker-secure") && !settings.acceptsDirection("speaker"));
        check(settings.advertisedFormats().equals(List.of(AndroidSpeakerSettings.SECURE_FORMAT)));
        check(settings.json(true, true).contains("\"revision\":\"1\""));
        check(settings.json(true, true).contains("\"permissionReady\":true"));
        var config = config(settings);
        var launch = admit(config, settings);
        check(launch.matches(settings));
        for (String key : config.keySet()) {
            var missing = new HashMap<>(config); missing.remove(key); rejects(() -> admit(missing, settings));
            var invalid = new HashMap<>(config); invalid.put(key, "wrong"); rejects(() -> admit(invalid, settings));
        }
        for (String key : List.of("host", "peerPort", "token", "pcmBytesPerFrame", "samplesPerFrame", "source")) {
            var invalid = new HashMap<>(config); invalid.put(key, 1); rejects(() -> admit(invalid, settings));
        }
        for (Object revision : List.of(1, 1L, "01", "+1", "1.0", "2")) {
            var invalid = new HashMap<>(config); invalid.put("selectionRevision", revision); rejects(() -> admit(invalid, settings));
        }
        rejects(() -> AndroidAudioRouteTransport.requireLegacy(3, AndroidSecureSpeakerLaunch.TRANSPORT,
                AndroidSpeakerSettings.SECURE_FORMAT, "speaker-secure"));
        rejects(() -> AndroidAudioRouteTransport.requireLegacy(1, "legacy-udp-v1", AndroidSpeakerSettings.SECURE_FORMAT, "speaker"));
        settings.select(AndroidSpeakerSettings.Profile.LEGACY);
        settings.select(AndroidSpeakerSettings.Profile.SECURE_STEREO_TLS13_V1);
        check(!launch.matches(settings));
        rejects(() -> admit(config, settings));
        var registry = AndroidCapabilityModules.createRegistry(new AudioNodeController(), new CapabilityLifecycleController(),
                new GamepadNodeController(), new TouchpadNodeController(), new CapabilityLifecycleController(),
                () -> false, () -> true, () -> new CapabilityLifecycleSnapshot(CapabilityLifecycleState.STOPPED, 0, null),
                () -> false, () -> List.of("cavc-avc-access-unit-v1"), mic::advertisedFormats,
                settings::advertisedFormats, false, false, HardwarePresence.PRESENT, HardwarePresence.PRESENT,
                HardwarePresence.ABSENT, HardwarePresence.ABSENT, HardwarePresence.ABSENT, HardwarePresence.ABSENT);
        var catalog = registry.snapshot(ID);
        var speaker = catalog.capabilities().stream().filter(c -> c.capabilityKey().equals("android.speaker")).findFirst().orElseThrow();
        check(speaker.availability() == CapabilityAvailability.AVAILABLE); // RECORD_AUDIO denied does not block playback.
        check(speaker.ports().get(0).formats().equals(settings.advertisedFormats()));
        String json = AndroidCoreCatalogJson.encode(catalog, "test", "test", settings.selection());
        check(json.contains("\"speaker_selection\":{\"profile\":\"secure_stereo_tls13_v1\",\"revision\":3}"));
        check(!AndroidCoreCatalogJson.encode(catalog, "test", "test").contains("speaker_selection"));
        String service = Files.readString(Path.of("platform/android/app/src/main/java/dev/capyio/android/AudioNodeService.java"));
        String module = Files.readString(Path.of("platform/android/app/src/main/java/dev/capyio/android/AndroidAudioUseModule.java"));
        check(service.contains("setSpeakerUiVisible") && service.contains("secureSpeakerPermitted.set(false)"));
        check(module.indexOf("synchronizeStereo(runtime, use, config)") < module.indexOf("new java.net.DatagramSocket"));
        check(module.contains("approvedSpeakerRoutes.remove(id)") && module.contains("consumedSpeakerRoutes.contains(id)"));
        String stereo = module.substring(module.indexOf("private void synchronizeStereo"), module.indexOf("private void retireStereo"));
        check(!stereo.contains("Manifest.permission.RECORD_AUDIO") && !stereo.contains("controller.completeStart("));
        check(!stereo.contains("audioUse(\"active\""));
        check(module.contains("playback.acceptedBytes()") && module.contains("if (!playback.cleanupComplete()) continue;"));
        AndroidSecureSpeakerProgress stoppedBeforeConfirmation = new AndroidSecureSpeakerProgress();
        stoppedBeforeConfirmation.observe(4, new long[]{1, 0, 0}, 1920, 1);
        stoppedBeforeConfirmation.observe(3, new long[]{1, 1, 1920}, 1920, 1);
        check(stoppedBeforeConfirmation.acceptedBytes() == 0
                && stoppedBeforeConfirmation.acceptedRecords() == 0); // Stop cannot confirm a queued ACK.
        AndroidSecureSpeakerProgress progress = new AndroidSecureSpeakerProgress();
        progress.observe(4, new long[]{1, 0, 0}, 1920, 1);
        check(progress.acceptedBytes() == 0); // Play/write/ACK enqueue alone is not native acceptance.
        progress.observe(4, new long[]{1, 0, 4}, 1920, 1);
        check(progress.acceptedBytes() == 4 && progress.acceptedRecords() == 0);
        progress.observe(0, new long[]{0, 0, 0}, 1920, 1);
        check(progress.acceptedBytes() == 4); // Busy poll cannot invent progress.
        progress.observe(4, new long[]{1, 1, 1920}, 1920, 1);
        check(progress.acceptedBytes() == 1920 && progress.acceptedRecords() == 1);
        progress.observe(4, new long[]{2, 2, 3840}, 1920, 1);
        check(progress.acceptedBytes() == 1920);
        String consent = Files.readString(Path.of("platform/android/app/src/main/java/dev/capyio/android/AndroidResourceConsentModule.java"));
        check(consent.contains("remember = false") && consent.contains("AndroidSecureSpeakerConsent.requiresFresh"));
        check(consent.substring(consent.indexOf("void approveRemembered")).contains("continue;"));
        check(AndroidSecureSpeakerConsent.requiresFresh("speaker", "", true));
        check(AndroidSecureSpeakerConsent.requiresFresh("speaker", "secure_stereo_tls13_v1", false));
        check(!AndroidSecureSpeakerConsent.requiresFresh("speaker", "", false));
        check(!AndroidSecureSpeakerConsent.requiresFresh("microphone", "", true));
        check(AndroidSecureSpeakerConsent.mayApprove(settings.selection(), settings.profile().id, "3", true, true, true));
        check(!AndroidSecureSpeakerConsent.mayApprove(settings.selection(), settings.profile().id, "1", true, true, true));
        check(!AndroidSecureSpeakerConsent.mayApprove(settings.selection(), settings.profile().id, "3", false, true, true));
        check(!AndroidSecureSpeakerConsent.mayApprove(settings.selection(), settings.profile().id, "3", true, false, true));
        System.out.println("Android secure speaker settings/admission: PASS (" + assertions + " assertions)");
    }
    private static Map<String,Object> config(AndroidSpeakerSettings settings) {
        return new HashMap<>(Map.of("schemaVersion", 3, "transport", AndroidSecureSpeakerLaunch.TRANSPORT,
                "format", AndroidSpeakerSettings.SECURE_FORMAT, "pcmBytesPerRecord", 1920,
                "sampleRateHz", 48000, "channels", 2, "sampleFramesPerRecord", 480,
                "profileId", settings.profile().id, "selectionRevision", Long.toString(settings.revision())));
    }
    private static AndroidSecureSpeakerLaunch admit(Map<String,Object> config, AndroidSpeakerSettings settings) {
        return AndroidSecureSpeakerLaunch.admit(config, ID, ID, ID, "speaker-secure", 1000, settings.selection(), 1);
    }
    private static void check(boolean value) { assertions++; if (!value) throw new AssertionError("assertion " + assertions); }
    private static void rejects(Runnable action) {
        try { action.run(); throw new AssertionError("accepted invalid handoff"); }
        catch (IllegalArgumentException expected) { assertions++; }
    }
}
