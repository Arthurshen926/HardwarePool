package dev.capyio.android;

import java.util.function.BooleanSupplier;

/** Commits a Source frame before it may cross the Adapter delivery boundary. */
final class GenerationBoundSourceDelivery {
    private GenerationBoundSourceDelivery() {}

    static boolean submit(
            BooleanSupplier commit,
            BooleanSupplier deliver,
            Runnable deliveryRejected) {
        if (!commit.getAsBoolean()) {
            return false;
        }
        if (deliver.getAsBoolean()) {
            return true;
        }
        deliveryRejected.run();
        return false;
    }
}
