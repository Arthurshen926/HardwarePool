package dev.capyio.android;

/** Per-connection failure isolation. A rejected peer must not kill the listener. */
final class AndroidControlConnectionCycle {
    interface Connection {
        boolean accept();
        boolean service();
        void reset();
    }

    private final Connection connection;
    private volatile boolean connected;
    private volatile String failure;

    AndroidControlConnectionCycle(Connection connection) { this.connection = connection; }
    boolean connected() { return connected; }
    String failure() { return failure; }
    void clearFailure() { failure = null; }

    void step() {
        try {
            if (!connected) {
                connected = connection.accept();
                if (connected) failure = null;
            }
            if (connected && !connection.service()) {
                connected = false;
                connection.reset();
            }
        } catch (RuntimeException rejected) {
            connected = false;
            String message = rejected.getMessage();
            failure = message == null ? "control connection rejected"
                    : message.substring(0, Math.min(message.length(), 512));
            // Reset failure is fatal; do not advertise a poisoned listener as live.
            connection.reset();
        }
    }
}
