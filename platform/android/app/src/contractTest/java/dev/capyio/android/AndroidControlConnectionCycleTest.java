package dev.capyio.android;

public final class AndroidControlConnectionCycleTest {
    public static void main(String[] args) {
        final class Fake implements AndroidControlConnectionCycle.Connection {
            int accepts, resets;
            boolean serviceThrows, eof, resetThrows;
            public boolean accept() {
                if (++accepts <= 2) throw new IllegalStateException("rejected handshake");
                return true;
            }
            public boolean service() {
                if (serviceThrows) throw new IllegalStateException("dispatch failed");
                return !eof;
            }
            public void reset() {
                resets++;
                if (resetThrows) throw new IllegalStateException("cleanup failed");
            }
        }
        Fake fake = new Fake();
        AndroidControlConnectionCycle cycle = new AndroidControlConnectionCycle(fake);
        cycle.step(); cycle.step();
        assert !cycle.connected() && fake.resets == 2;
        assert "rejected handshake".equals(cycle.failure());
        cycle.clearFailure();
        assert cycle.failure() == null;
        cycle.step();
        assert cycle.connected() && cycle.failure() == null;
        fake.serviceThrows = true;
        cycle.step();
        assert !cycle.connected() && fake.resets == 3;
        fake.serviceThrows = false;
        cycle.step();
        assert cycle.connected() && fake.accepts == 4;
        fake.eof = true;
        cycle.step();
        assert !cycle.connected() && fake.resets == 4;
        fake.eof = false;
        cycle.step();
        assert cycle.connected() && fake.accepts == 5;
        fake.serviceThrows = true; fake.resetThrows = true;
        boolean fatal = false;
        try { cycle.step(); } catch (IllegalStateException expected) { fatal = true; }
        assert fatal && !cycle.connected();
        System.out.println("Android control recovery: PASS (reject, retry, dispatch, EOF, fatal cleanup)");
    }
}
