package io.capyio.camera.lab;

import io.capyio.camera.contract.SecureCameraTransport;
import java.util.Objects;

/** In-process capability, never serialized into an Intent or persisted. */
record SecureCameraBinding(SecureCameraTransport transport, long generation) {
    SecureCameraBinding {
        Objects.requireNonNull(transport, "transport");
        if (generation <= 0) throw new IllegalArgumentException("invalid secure generation");
    }
    boolean ready() { return transport.state(generation) == SecureCameraTransport.READY; }
    void stop() { transport.stop(generation); }
}
