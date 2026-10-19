package dev.capyio.android;

/** Explicit trusted-network lab endpoint; not pairing or authorization. */
public record AndroidAudioPeer(String host, int localPort, int peerPort) {
    public static String savedSettingsJson(String direction, AndroidAudioPeer peer) {
        if (!"microphone".equals(direction) && !"speaker".equals(direction)) {
            throw new IllegalArgumentException("Unknown audio direction");
        }
        return "{\"schemaVersion\":1,\"direction\":\"" + direction + "\",\"savedPeer\":"
                + (peer == null ? "null" : "{\"host\":\"" + peer.host()
                + "\",\"localPort\":" + peer.localPort() + ",\"peerPort\":" + peer.peerPort() + "}")
                + "}";
    }

    public AndroidAudioPeer {
        if (host == null || host.length() > 15 || localPort < 1 || localPort > 65535
                || peerPort < 1 || peerPort > 65535) {
            throw new IllegalArgumentException("Invalid audio endpoint");
        }
        String[] parts = host.split("\\.", -1);
        if (parts.length != 4) throw new IllegalArgumentException("IPv4 literal required");
        int[] octets = new int[4];
        for (int i = 0; i < 4; i++) {
            if (!parts[i].matches("0|[1-9][0-9]{0,2}")) {
                throw new IllegalArgumentException("Canonical IPv4 required");
            }
            octets[i] = Integer.parseInt(parts[i]);
            if (octets[i] > 255) throw new IllegalArgumentException("Invalid IPv4 octet");
        }
        boolean trustedRange = octets[0] == 10
                || (octets[0] == 172 && octets[1] >= 16 && octets[1] <= 31)
                || (octets[0] == 192 && octets[1] == 168)
                || (octets[0] == 100 && octets[1] >= 64 && octets[1] <= 127);
        if (!trustedRange) throw new IllegalArgumentException("Private or overlay IPv4 required");
    }
}
