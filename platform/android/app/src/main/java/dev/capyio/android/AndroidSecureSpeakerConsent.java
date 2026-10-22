package dev.capyio.android;

/** Secure speaker approval is per Route, visible and exact-revision; never remembered. */
final class AndroidSecureSpeakerConsent {
    private AndroidSecureSpeakerConsent() {}
    static boolean requiresFresh(String kind, String profileId, boolean secureSelected) {
        return "secure_stereo_tls13_v1".equals(profileId) || (secureSelected && "speaker".equals(kind));
    }
    static boolean mayApprove(AndroidSpeakerSettings.Selection selected, String profileId,
            String revision, boolean visible, boolean notificationReady, boolean hardwarePresent) {
        return selected != null && selected.profile().secure() && selected.revision() > 0
                && selected.profile().id.equals(profileId) && Long.toString(selected.revision()).equals(revision)
                && visible && notificationReady && hardwarePresent;
    }
}
