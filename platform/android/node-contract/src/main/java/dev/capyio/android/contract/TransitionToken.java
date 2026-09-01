package dev.capyio.android.contract;

/** Generation-bound result for one requested capability transition. */
public final class TransitionToken {
    private final boolean accepted;
    private final long generation;

    TransitionToken(boolean accepted, long generation) {
        this.accepted = accepted;
        this.generation = generation;
    }

    public boolean accepted() {
        return accepted;
    }

    public long generation() {
        return generation;
    }
}
