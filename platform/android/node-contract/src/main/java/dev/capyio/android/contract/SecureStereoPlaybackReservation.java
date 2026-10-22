package dev.capyio.android.contract;

import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicReference;

/** Process-wide ownership. Uncertain cleanup quarantines playback until process exit. */
public final class SecureStereoPlaybackReservation {
    private final AtomicReference<Lease> owner = new AtomicReference<>();
    public Lease tryAcquire() {
        Lease lease = new Lease();
        return owner.compareAndSet(null, lease) ? lease : null;
    }
    public boolean isHeld() { return owner.get() != null; }
    public final class Lease {
        private final AtomicBoolean finished = new AtomicBoolean();
        private Lease() {}
        public void completeCleanup(boolean clean) {
            if (finished.compareAndSet(false, true) && clean) owner.compareAndSet(this, null);
        }
    }
}
