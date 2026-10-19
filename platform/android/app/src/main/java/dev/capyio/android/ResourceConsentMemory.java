package dev.capyio.android;

/** Pair generation plus exact resource scope; old grants only covered touchpad. */
final class ResourceConsentMemory {
    private ResourceConsentMemory() {}

    static boolean allows(String kind, long pairingGeneration, long resourceGrant, long legacyGrant) {
        return pairingGeneration >= 0 && (resourceGrant == pairingGeneration
                || ("touchpad".equals(kind) && legacyGrant == pairingGeneration));
    }
}
