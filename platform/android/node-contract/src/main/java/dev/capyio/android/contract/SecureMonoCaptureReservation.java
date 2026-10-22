package dev.capyio.android.contract;

import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicReference;

/**
 * Process-owned admission gate, independent of any Service instance. Uncertain
 * cleanup intentionally keeps the reservation until process exit; a recreated
 * Service must not silently reuse a possibly live microphone.
 */
public final class SecureMonoCaptureReservation {
    private final AtomicReference<Lease> owner = new AtomicReference<>();

    public Lease tryAcquire() {
        Lease lease = new Lease();
        return owner.compareAndSet(null, lease) ? lease : null;
    }

    public boolean isHeld() { return owner.get() != null; }

    public final class Lease {
        private final AtomicBoolean completed = new AtomicBoolean();
        private Lease() {}

        /** One-shot: an uncertain cleanup cannot later be relabeled clean. */
        public void completeCleanup(boolean clean) {
            if (completed.compareAndSet(false, true) && clean) owner.compareAndSet(this, null);
        }
    }
}
