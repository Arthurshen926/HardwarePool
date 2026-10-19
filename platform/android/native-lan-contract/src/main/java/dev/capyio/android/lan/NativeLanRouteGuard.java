package dev.capyio.android.lan;

import java.util.Objects;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.function.LongSupplier;

/** One-shot worker lifetime, not an admission decision or transport security. */
public final class NativeLanRouteGuard {
    public final NativeLanRouteBinding route;
    private final LongSupplier wallClock;
    private final LongSupplier monotonicClock;
    private final long startedNanos;
    private final long budgetMillis;
    private final AtomicBoolean revoked = new AtomicBoolean();

    public NativeLanRouteGuard(NativeLanRouteBinding route) {
        this(route, System::currentTimeMillis, System::nanoTime);
    }

    NativeLanRouteGuard(NativeLanRouteBinding route, LongSupplier wallClock,
            LongSupplier monotonicClock) {
        this.route = Objects.requireNonNull(route, "route");
        this.wallClock = Objects.requireNonNull(wallClock, "wallClock");
        this.monotonicClock = Objects.requireNonNull(monotonicClock, "monotonicClock");
        startedNanos = monotonicClock.getAsLong();
        long now = wallClock.getAsLong();
        route.requireUnexpired(now);
        budgetMillis = route.expiresAtMs - now;
    }

    /** Control owner must revoke on disconnect, lost lease or changed Route epoch. */
    public void revoke() {
        revoked.set(true);
    }

    public boolean isLive() {
        if (revoked.get()) return false;
        long elapsed = monotonicClock.getAsLong() - startedNanos;
        long now = wallClock.getAsLong();
        if (elapsed < 0 || elapsed / 1_000_000L >= budgetMillis
                || now < 0 || now >= route.expiresAtMs) {
            revoke();
        }
        return !revoked.get();
    }
}
