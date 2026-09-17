package dev.capyio.display.app;

import java.net.Socket;

/** Transfer socket ownership only after successful configuration and handshake. */
public final class DisplaySocketOwnership {
    private DisplaySocketOwnership() {}

    @FunctionalInterface
    public interface Initializer<T extends Socket> { void run(T socket) throws Exception; }

    public static <T extends Socket> T initialize(T socket, Initializer<T> initializer)
            throws Exception {
        try {
            initializer.run(socket);
            return socket;
        } catch (Exception | Error failure) {
            try {
                socket.close();
            } catch (Exception cleanup) {
                failure.addSuppressed(cleanup);
            }
            throw failure;
        }
    }
}
