package io.capyio.camera.contract;

/**
 * Narrow, generation-bound complete-record seam. Implementations never reconnect a generation.
 * All methods return promptly without network I/O or thread joins. record reserves its bounded
 * native byte/record slot before copying the Java array and returns false on overflow, invalid
 * generation or cancellation. stop independently cancels even before start becomes Ready.
 */
public interface SecureCameraTransport {
    int PENDING = 0;
    int READY = 1;
    int FAILED = 2;
    int STOPPED = 3;

    long start(String routeId);
    int state(long generation);
    boolean record(long generation, byte[] oneCompleteRecord);
    void stop(long generation);
}
