package io.capyio.camera.contract;

/** Hardware-free tests of the exact pure gate called by Android metadata/output projection. */
public final class FiniteCameraEligibilityContractTest {
    private static final CameraSourceSelection SOURCE = new CameraSourceSelection(
            "explicit-camera", CameraInventory.LensFacing.EXTERNAL, null);

    public static void main(String[] args) {
        run();
        System.out.println("FiniteCameraEligibilityContractTest passed");
    }

    public static void run() {
        selectedCameraRequiresCanonicalMetadata();
        exactEncoderFactsAreRequired();
        changedPlanIsRejectedWithoutFallback();
        actualOutputFactsAreRequired();
    }

    private static FiniteCameraEligibility.CameraDescriptor camera(Integer rotation, boolean dynamic,
            boolean codecSize, boolean previewSize, long codecMin, long previewMin, boolean fixed30) {
        return new FiniteCameraEligibility.CameraDescriptor(SOURCE.cameraId(), SOURCE.facing(),
                rotation, dynamic, codecSize, previewSize, codecMin, previewMin, fixed30);
    }

    private static FiniteCameraEligibility.CameraDescriptor validCamera() {
        return camera(0, false, true, true, 33_333_333L, 33_333_333L, true);
    }

    private static void selectedCameraRequiresCanonicalMetadata() {
        FiniteCameraEligibility.requireCamera(SOURCE, validCamera());
        reject(() -> FiniteCameraEligibility.requireCamera(null, validCamera()), "implicit camera");
        reject(() -> FiniteCameraEligibility.requireCamera(new CameraSourceSelection(
                SOURCE.cameraId(), SOURCE.facing(), 1000), validCamera()), "zoom even at 1x");
        reject(() -> FiniteCameraEligibility.requireCamera(new CameraSourceSelection(
                "different-camera", SOURCE.facing(), null), validCamera()), "changed selected camera");
        reject(() -> FiniteCameraEligibility.requireCamera(new CameraSourceSelection(
                SOURCE.cameraId(), CameraInventory.LensFacing.FRONT, null), validCamera()), "changed lens-facing metadata");
        for (Integer rotation : new Integer[] {null, -1, 90, 180, 270, 360}) {
            reject(() -> FiniteCameraEligibility.requireCamera(SOURCE,
                    camera(rotation, false, true, true, 33_333_333L, 33_333_333L, true)), "unknown/nonzero rotation");
        }
        reject(() -> FiniteCameraEligibility.requireCamera(SOURCE,
                camera(0, true, true, true, 33_333_333L, 33_333_333L, true)), "dynamic sensor orientation");
        reject(() -> FiniteCameraEligibility.requireCamera(SOURCE,
                camera(0, false, false, true, 33_333_333L, 33_333_333L, true)), "missing exact codec surface");
        reject(() -> FiniteCameraEligibility.requireCamera(SOURCE,
                camera(0, false, true, false, 33_333_333L, 33_333_333L, true)), "missing exact preview surface");
        reject(() -> FiniteCameraEligibility.requireCamera(SOURCE,
                camera(0, false, true, true, 33_333_333L, 33_333_333L, false)), "variable AE range is insufficient");
        for (long duration : new long[] {-1, 0, 33_333_334L, 66_666_666L, Long.MAX_VALUE}) {
            reject(() -> FiniteCameraEligibility.requireCamera(SOURCE,
                    camera(0, false, true, true, duration, 33_333_333L, true)), "unknown/slow codec duration");
            reject(() -> FiniteCameraEligibility.requireCamera(SOURCE,
                    camera(0, false, true, true, 33_333_333L, duration, true)), "unknown/slow preview duration");
        }
        FiniteCameraEligibility.requireCamera(SOURCE,
                camera(0, false, true, true, 16_666_666L, 16_666_666L, true));
    }

    private static FiniteCameraEligibility.EncoderDescriptor encoder(String name, int missing) {
        return new FiniteCameraEligibility.EncoderDescriptor(name, missing != 0, missing != 1,
                missing != 2, missing != 3, missing != 4, missing != 5, missing != 6);
    }

    private static void exactEncoderFactsAreRequired() {
        FiniteCameraEligibility.requireEncoder(encoder("pinned-avc", -1));
        for (int index = 0; index < 7; index++) {
            final int missing = index;
            reject(() -> FiniteCameraEligibility.requireEncoder(encoder("pinned-avc", missing)),
                    "missing encoder capability " + index);
        }
        for (String name : new String[] {"", "bad\ncodec", "x".repeat(257)}) {
            reject(() -> FiniteCameraEligibility.requireEncoder(encoder(name, -1)), "unusable codec identity");
        }
    }

    private static void changedPlanIsRejectedWithoutFallback() {
        FiniteCameraEligibility.EncoderDescriptor pinned = encoder("pinned-avc", -1);
        FiniteCameraEligibility.requireUnchanged(SOURCE, validCamera(), pinned, validCamera(), pinned);
        reject(() -> FiniteCameraEligibility.requireUnchanged(SOURCE, validCamera(), pinned,
                validCamera(), encoder("different-but-eligible-codec", -1)), "codec fallback");
        reject(() -> FiniteCameraEligibility.requireUnchanged(SOURCE, validCamera(), pinned,
                camera(0, false, true, true, 16_666_666L, 16_666_666L, true), pinned),
                "changed still-eligible camera metadata");
        reject(() -> FiniteCameraEligibility.requireUnchanged(SOURCE, validCamera(), pinned,
                validCamera(), encoder("pinned-avc", 3)), "pinned codec loses Baseline level 3.1");
    }

    private static FiniteCameraEligibility.OutputDescriptor output(int missing) {
        return new FiniteCameraEligibility.OutputDescriptor(missing == 0 ? null : "video/avc",
                missing == 1 ? null : 1280, missing == 2 ? null : 720,
                missing == 3 ? null : 30, missing == 4 ? null : 4_000_000,
                missing != 5, missing != 6, missing != 7, missing != 8,
                missing != 9, missing != 10, missing != 11, missing != 12);
    }

    private static void actualOutputFactsAreRequired() {
        FiniteCameraEligibility.requireOutput(output(-1));
        for (int index = 0; index < 13; index++) {
            final int missing = index;
            reject(() -> FiniteCameraEligibility.requireOutput(output(missing)), "missing actual output fact " + index);
        }
        reject(() -> FiniteCameraEligibility.requireOutput(new FiniteCameraEligibility.OutputDescriptor(
                "video/hevc", 1280, 720, 30, 4_000_000,
                true, true, true, true, true, true, true, true)), "wrong MIME");
        for (int[] values : new int[][] {{640, 720, 30, 4_000_000}, {1280, 480, 30, 4_000_000},
                {1280, 720, 24, 4_000_000}, {1280, 720, 30, 6_000_000}}) {
            reject(() -> FiniteCameraEligibility.requireOutput(new FiniteCameraEligibility.OutputDescriptor(
                    "video/avc", values[0], values[1], values[2], values[3],
                    true, true, true, true, true, true, true, true)), "wrong canonical scalar");
        }
    }

    private static void reject(Runnable action, String reason) {
        try {
            action.run();
            throw new AssertionError("accepted " + reason);
        } catch (IllegalArgumentException expected) {
            // A refusal is the expected fail-closed outcome, never a legacy fallback.
        }
    }
}
