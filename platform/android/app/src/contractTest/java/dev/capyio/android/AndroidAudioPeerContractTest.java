package dev.capyio.android;

public final class AndroidAudioPeerContractTest {
    public static void main(String[] args) {
        for (String host : new String[] {"10.0.0.1", "172.16.0.1", "172.31.255.254", "192.168.1.2", "100.64.0.1", "100.127.255.254"}) {
            assert new AndroidAudioPeer(host, 1, 65535).host().equals(host);
        }
        for (String host : new String[] {"8.8.8.8", "127.0.0.1", "0.0.0.0", "255.255.255.255", "224.1.2.3", "100.63.0.1", "100.128.0.1", "172.32.0.1", "192.168.001.1", "192.168.1.256", "localhost", "192.168.1.1:1", " 10.0.0.1", "10.0.0.1\n", "::1", ""}) {
            rejects(() -> new AndroidAudioPeer(host, 40010, 40011));
        }
        rejects(() -> new AndroidAudioPeer(null, 1, 2));
        for (int port : new int[] {-1, 0, 65536}) {
            rejects(() -> new AndroidAudioPeer("10.0.0.1", port, 2));
            rejects(() -> new AndroidAudioPeer("10.0.0.1", 1, port));
        }
        System.out.println("Android audio peer contract: PASS (29 cases)");
    }
    private static void rejects(Runnable action) {
        try { action.run(); } catch (IllegalArgumentException expected) { return; }
        throw new AssertionError("Unsafe endpoint accepted");
    }
}
