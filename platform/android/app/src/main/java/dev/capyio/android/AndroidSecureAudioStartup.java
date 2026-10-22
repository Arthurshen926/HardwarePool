package dev.capyio.android;

import java.util.concurrent.atomic.AtomicBoolean;

/** Exception-safe one-shot startup. Callbacks manage lifecycle only, never perform network I/O or joins. */
final class AndroidSecureAudioStartup {
    interface Steps<T> {
        boolean foregroundReady();
        long startNative();
        T createCapture(long generation);
        boolean startCapture(T capture);
        void stopNative(long generation);
        void stopCapture(T capture);
        void failed();
        void changed();
    }
    private AndroidSecureAudioStartup() {}
    static <T> T start(AtomicBoolean permitted, Steps<T> steps) {
        long generation = 0;
        T capture = null;
        try {
            if (!steps.foregroundReady()) throw new IllegalStateException("Visible foreground lifecycle required");
            permitted.set(true);
            generation = steps.startNative();
            if (generation <= 0) throw new IllegalStateException("Secure native startup rejected");
            capture = steps.createCapture(generation);
            if (capture == null || !steps.startCapture(capture)) throw new IllegalStateException("Secure capture startup rejected");
            steps.changed();
            return capture;
        } catch (RuntimeException | LinkageError failed) {
            permitted.set(false);
            if (generation > 0) {
                try { steps.stopNative(generation); }
                catch (RuntimeException | LinkageError unavailable) { /* Continue independent capture cancellation. */ }
            }
            if (capture != null) {
                try { steps.stopCapture(capture); }
                catch (RuntimeException | LinkageError unavailable) { /* Owner retains cleanup quarantine. */ }
            }
            try { steps.failed(); } finally { steps.changed(); }
            throw failed;
        }
    }
}
