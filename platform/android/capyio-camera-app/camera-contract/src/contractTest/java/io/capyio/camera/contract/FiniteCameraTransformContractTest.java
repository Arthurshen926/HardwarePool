package io.capyio.camera.contract;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Locale;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicReference;

/** Hardware-free tests of the exact plan, shaders, and lifecycle used by Android. */
public final class FiniteCameraTransformContractTest {
    private static final int[] CARDINALS = {0, 90, 180, 270};

    public static void main(String[] args) throws Exception {
        run();
        if (args.length == 2 && "--export".equals(args[0])) {
            export(Path.of(args[1]));
        } else if (args.length != 0) {
            throw new IllegalArgumentException("usage: FiniteCameraTransformContractTest [--export DIRECTORY]");
        }
        System.out.println("FiniteCameraTransformContractTest passed");
    }

    public static void run() throws Exception {
        plansAreClosedAndImmutable();
        geometryFitsWithoutChangingOutput();
        producerRotationIsAppliedOnlyOnce();
        unsafeProducerTransformsAreRejected();
        partialInitializationAlwaysUnwinds();
        closeDuringEveryStageCannotResurrect();
        initializationAndCloseAreOneShot();
        callbacksCoalesceAcrossThreads();
        closeRejectsQueuedAndInFlightFrames();
        timestampsAreStrictAndOwnerBound();
        repeatedLatchIsDropOnly();
    }

    private static FiniteCameraTransform.Plan plan(int sensor, CameraInventory.LensFacing facing) {
        return FiniteCameraTransform.plan(1280, 720, sensor, facing, false);
    }

    private static void plansAreClosedAndImmutable() {
        for (CameraInventory.LensFacing facing : new CameraInventory.LensFacing[] {
                CameraInventory.LensFacing.FRONT, CameraInventory.LensFacing.BACK,
                CameraInventory.LensFacing.EXTERNAL}) {
            for (int sensor : CARDINALS) {
                FiniteCameraTransform.Plan value = plan(sensor, facing);
                check(value.equals(plan(sensor, facing)), "equal immutable plan");
                check(value.hashCode() == plan(sensor, facing).hashCode(), "consistent plan hash");
                check(value.sensorOrientationDegrees() == sensor && value.facing() == facing,
                        "metadata stays pinned");
                check(value.clockwiseRotationDegrees() == sensor, "MIRROR_MODE_NONE sensor rotation");
                check(!value.mirrored(), "front is not a mirrored preview");
                check(!value.equals(plan((sensor + 90) % 360, facing)), "changed sensor plan differs");
            }
        }
        for (Integer sensor : new Integer[] {null, -90, -1, 1, 45, 89, 91, 359, 360, 450}) {
            reject(() -> FiniteCameraTransform.plan(sensor, CameraInventory.LensFacing.BACK, false),
                    "unknown/non-cardinal sensor");
        }
        reject(() -> FiniteCameraTransform.plan(90, CameraInventory.LensFacing.BACK, true),
                "dynamic orientation");
        reject(() -> FiniteCameraTransform.plan(0, CameraInventory.LensFacing.UNKNOWN, false),
                "unknown facing");
        reject(() -> FiniteCameraTransform.plan(0, null, false), "absent facing");
        for (int[] dimensions : new int[][] {{720, 1280}, {640, 360}, {1280, 721}, {0, 720}}) {
            reject(() -> FiniteCameraTransform.plan(dimensions[0], dimensions[1], 0,
                    CameraInventory.LensFacing.BACK, false), "changed raw dimensions");
        }
        FiniteCameraTransform.Plan pinned = plan(90, CameraInventory.LensFacing.BACK);
        check(!pinned.equals(plan(90, CameraInventory.LensFacing.FRONT)), "facing stays pinned");
        float[] geometry = new float[16];
        float[] quad = new float[16];
        pinned.copyGeometryMatrix(geometry);
        geometry[0] = 500;
        pinned.copyGeometryMatrix(geometry);
        close(geometry[0], 405f / 1280, "matrix is not a mutable plan alias");
        pinned.copyInterleavedQuad(quad);
        quad[0] = 500;
        pinned.copyInterleavedQuad(quad);
        close(quad[0], -1, "quad is not a mutable shared alias");
        reject(() -> pinned.copyGeometryMatrix(new float[15]), "wrong matrix size");
        reject(() -> pinned.copyTextureMatrix(new float[17]), "oversized matrix");
        reject(() -> pinned.copyInterleavedQuad(null), "missing quad");
    }

    private static void geometryFitsWithoutChangingOutput() {
        AvcEncoderConfig output = AvcEncoderConfig.baseline720p30();
        for (int sensor : CARDINALS) {
            FiniteCameraTransform.Plan value = plan(sensor, CameraInventory.LensFacing.BACK);
            check(value.inputWidth() == 1280 && value.inputHeight() == 720, "raw stays 1280x720");
            check(value.outputWidth() == output.width() && value.outputHeight() == output.height(),
                    "no negotiated dimension change");
            check(output.framesPerSecond() == 30 && output.bitrateBitsPerSecond() == 4_000_000
                    && output.clockwiseRotationDegrees() == 0, "output profile stays canonical");
            float[] matrix = new float[16];
            value.copyGeometryMatrix(matrix);
            close(matrix[5], 1, "full target height");
            close(value.contentHeight(), 720, "content height");
            close(value.contentTop(), 0, "centered vertical bounds");
            if (sensor == 90 || sensor == 270) {
                close(value.contentWidth(), 405, "rotated aspect fit width");
                close(value.contentLeft(), 437.5f, "left ideal raster edge");
                close(value.contentLeft() + value.contentWidth(), 842.5f, "right ideal raster edge");
                close(1280 * (1 - matrix[0]) / 2, 437.5f, "geometry left edge");
                close(1280 * (1 + matrix[0]) / 2, 842.5f, "geometry right edge");
                close(value.contentWidth() / value.contentHeight(), 720f / 1280,
                        "no portrait stretch or center crop");
            } else {
                close(value.contentWidth(), 1280, "landscape full width");
                close(value.contentLeft(), 0, "no landscape bars");
            }
        }
    }

    private static void producerRotationIsAppliedOnlyOnce() {
        for (CameraInventory.LensFacing facing : new CameraInventory.LensFacing[] {
                CameraInventory.LensFacing.FRONT, CameraInventory.LensFacing.BACK}) {
            for (int sensor : CARDINALS) {
                FiniteCameraTransform.Plan value = plan(sensor, facing);
                float[] texture = new float[16];
                value.copyTextureMatrix(texture);
                check(Arrays.equals(texture, identity()), "no duplicate manual sensor rotation");
                for (int inset = 0; inset <= 2; inset++) {
                    float[] producer = producer(sensor, inset, inset, inset, inset);
                    float[] before = producer.clone();
                    value.requireProducerMatrix(producer);
                    check(Arrays.equals(before, producer), "validation preserves per-frame producer matrix");
                }
                // Edge insets are independently bounded, not forced to be symmetric.
                value.requireProducerMatrix(producer(sensor, 0, 2, 1, 0));
            }
        }
        check(FiniteCameraTransform.VERTEX_SHADER.contains(
                "uProducerMatrix * uTextureMatrix * aTextureCoord"), "documented matrix order");
        check(FiniteCameraTransform.FRAGMENT_SHADER_EXTERNAL_OES.contains("samplerExternalOES"),
                "production external texture sampler");
    }

    private static void unsafeProducerTransformsAreRejected() {
        for (int sensor : CARDINALS) {
            FiniteCameraTransform.Plan value = plan(sensor, CameraInventory.LensFacing.FRONT);
            reject(() -> value.requireProducerMatrix(identity()), "missing producer GL flip");
            for (int wrong : CARDINALS) {
                if (wrong != sensor) {
                    reject(() -> value.requireProducerMatrix(producer(wrong, 0, 0, 0, 0)),
                            "changed or double-applied sensor rotation");
                }
            }
            for (int index = 0; index < 16; index++) {
                for (float bad : new float[] {Float.NaN, Float.POSITIVE_INFINITY, Float.NEGATIVE_INFINITY}) {
                    float[] corrupted = producer(sensor, 0, 0, 0, 0);
                    corrupted[index] = bad;
                    reject(() -> value.requireProducerMatrix(corrupted), "non-finite producer entry");
                }
            }
            for (int index : new int[] {2, 3, 6, 7, 8, 9, 10, 11, 14, 15}) {
                float[] corrupted = producer(sensor, 0, 0, 0, 0);
                corrupted[index] += 0.1f;
                reject(() -> value.requireProducerMatrix(corrupted), "perspective/depth mutation");
            }
            float[] shear = producer(sensor, 0, 0, 0, 0);
            shear[sensor == 0 || sensor == 180 ? 1 : 0] = 0.1f;
            reject(() -> value.requireProducerMatrix(shear), "shear");
            float[] mirrored = producer(sensor, 0, 0, 0, 0);
            mirrored[12] = 1 - mirrored[12];
            mirrored[0] = -mirrored[0];
            mirrored[4] = -mirrored[4];
            reject(() -> value.requireProducerMatrix(mirrored), "front mirror");
            for (float[] badCrop : new float[][] {
                    producer(sensor, 3, 0, 0, 0), producer(sensor, 0, 3, 0, 0),
                    producer(sensor, 0, 0, 3, 0), producer(sensor, 0, 0, 0, 3),
                    producer(sensor, -1, 0, 0, 0), producer(sensor, 0, -1, 0, 0),
                    producer(sensor, 0, 0, -1, 0), producer(sensor, 0, 0, 0, -1),
                    producer(sensor, 100, 100, 100, 100)}) {
                reject(() -> value.requireProducerMatrix(badCrop), "unbounded crop or outside-buffer sample");
            }
        }
    }

    private static void partialInitializationAlwaysUnwinds() throws Exception {
        for (FiniteCameraTransform.Stage failing : FiniteCameraTransform.Stage.values()) {
            FakeBackend backend = new FakeBackend();
            backend.failInitialization = failing;
            FiniteCameraTransform.Lifecycle lifecycle = new FiniteCameraTransform.Lifecycle(backend);
            try {
                lifecycle.initialize();
                throw new AssertionError("accepted injected partial initialization failure");
            } catch (IOException expected) {
                check(expected.getMessage().equals("injected stage failure"), "original init failure retained");
            }
            check(lifecycle.state() == FiniteCameraTransform.State.CLOSED, "failure closes lifecycle");
            backend.requireReverseRelease(failing.ordinal() + 1);
            lifecycle.closeOnOwner();
            backend.requireReverseRelease(failing.ordinal() + 1);
            rejectState(lifecycle::initialize, "restart after partial initialization");
        }
        FakeBackend backend = new FakeBackend();
        backend.failInitialization = FiniteCameraTransform.Stage.CAMERA_SURFACE;
        backend.failRelease = FiniteCameraTransform.Stage.SURFACE_TEXTURE;
        FiniteCameraTransform.Lifecycle lifecycle = new FiniteCameraTransform.Lifecycle(backend);
        try {
            lifecycle.initialize();
            throw new AssertionError("accepted injected initialize and cleanup failure");
        } catch (IOException expected) {
            check(expected.getSuppressed().length == 1, "cleanup failure retained, not swallowed");
        }
        backend.requireReverseRelease(FiniteCameraTransform.Stage.values().length);
        check(lifecycle.state() == FiniteCameraTransform.State.CLOSED, "cleanup failure stays terminal");
    }

    private static void closeDuringEveryStageCannotResurrect() throws Exception {
        for (FiniteCameraTransform.Stage closing : FiniteCameraTransform.Stage.values()) {
            FakeBackend backend = new FakeBackend();
            FiniteCameraTransform.Lifecycle lifecycle = new FiniteCameraTransform.Lifecycle(backend);
            backend.afterInitialize = stage -> {
                if (stage == closing) onOtherThread(lifecycle::requestClose);
            };
            check(!lifecycle.initialize(), "close wins during initialization");
            check(lifecycle.state() == FiniteCameraTransform.State.CLOSED, "never resurrect READY");
            check(!lifecycle.onFrameAvailable() && !lifecycle.mayPresent(), "closed rejects frame admission");
            backend.requireReverseRelease(closing.ordinal() + 1);
        }
    }

    private static void initializationAndCloseAreOneShot() throws Exception {
        FakeBackend before = new FakeBackend();
        FiniteCameraTransform.Lifecycle preclosed = new FiniteCameraTransform.Lifecycle(before);
        check(preclosed.requestClose(), "first pre-init close");
        check(!preclosed.initialize(), "pre-init close prevents allocation");
        check(before.initialized.isEmpty() && before.released.isEmpty(), "no resources after pre-init close");
        check(!preclosed.requestClose(), "duplicate request close is harmless");
        preclosed.closeOnOwner();
        rejectState(preclosed::initialize, "restart preclosed lifecycle");

        FakeBackend backend = new FakeBackend();
        FiniteCameraTransform.Lifecycle value = new FiniteCameraTransform.Lifecycle(backend);
        check(!value.onFrameAvailable(), "cannot queue before READY");
        check(value.initialize(), "successful init");
        rejectState(value::initialize, "duplicate initialization");
        value.closeOnOwner();
        value.closeOnOwner();
        backend.requireReverseRelease(FiniteCameraTransform.Stage.values().length);
        rejectState(value::initialize, "closed instance cannot restart");
    }

    private static void callbacksCoalesceAcrossThreads() throws Exception {
        FiniteCameraTransform.Lifecycle value = new FiniteCameraTransform.Lifecycle(new FakeBackend());
        value.initialize();
        AtomicInteger scheduled = new AtomicInteger();
        CountDownLatch start = new CountDownLatch(1);
        Thread[] callbacks = new Thread[4];
        for (int i = 0; i < callbacks.length; i++) {
            callbacks[i] = new Thread(() -> {
                try {
                    start.await();
                } catch (InterruptedException failure) {
                    throw new AssertionError(failure);
                }
                for (int frame = 0; frame < 10_000; frame++) {
                    if (value.onFrameAvailable()) scheduled.incrementAndGet();
                }
            }, "finite-callback-test-" + i);
            callbacks[i].start();
        }
        start.countDown();
        for (Thread callback : callbacks) callback.join();
        check(scheduled.get() == 1, "40,000 callbacks retain one render task");
        check(value.beginFrame() && !value.beginFrame(), "consume one latest-frame slot");
        check(value.onFrameAvailable() && !value.onFrameAvailable(), "slot is reusable after consume");
        value.closeOnOwner();
        check(!value.beginFrame(), "queued render after close is a no-op");
    }

    private static void closeRejectsQueuedAndInFlightFrames() throws Exception {
        for (boolean inFlight : new boolean[] {false, true}) {
            FakeBackend backend = new FakeBackend();
            FiniteCameraTransform.Lifecycle value = new FiniteCameraTransform.Lifecycle(backend);
            value.initialize();
            check(value.onFrameAvailable(), "one frame queued");
            if (inFlight) {
                check(value.beginFrame(), "render begins");
                check(value.acceptTimestampNs(1_000_000) == 1_000_000, "original presentation ns");
            }
            onOtherThread(value::requestClose);
            check(!value.onFrameAvailable() && !value.beginFrame(), "close rejects queued and late work");
            check(!value.mayPresent(), "close wins before the presentation recheck");
            rejectState(() -> value.acceptTimestampNs(2_000_000), "PTS after close request");
            value.closeOnOwner();
            backend.requireReverseRelease(FiniteCameraTransform.Stage.values().length);
        }
    }

    private static void timestampsAreStrictAndOwnerBound() throws Exception {
        FiniteCameraTransform.Lifecycle value = new FiniteCameraTransform.Lifecycle(new FakeBackend());
        value.initialize();
        for (long timestamp : new long[] {1_000_001, 34_333_334, 67_666_667, 1_000_000_000}) {
            check(value.acceptTimestampNs(timestamp) == timestamp, "preserve source ns exactly");
        }
        for (long invalid : new long[] {-1, 0, 1, 999_999_999, 1_000_000_000, 1_000_000_001}) {
            reject(() -> value.acceptTimestampNs(invalid), "negative/duplicate/regressing/quantized PTS");
        }
        onOtherThread(() -> rejectState(value::beginFrame, "render off GL owner"));
        onOtherThread(() -> rejectState(() -> value.acceptTimestampNs(2_000_000_000), "PTS off GL owner"));
        onOtherThread(() -> rejectState(value::closeOnOwner, "resource cleanup off GL owner"));
        value.closeOnOwner();
    }

    private static void repeatedLatchIsDropOnly() throws Exception {
        FiniteCameraTransform.Lifecycle value = new FiniteCameraTransform.Lifecycle(new FakeBackend());
        value.initialize();
        check(!value.isRepeatedTimestampNs(0), "zero timestamp cannot bypass admission");
        check(!value.isRepeatedTimestampNs(1_000_001), "first latch is a new frame");
        value.onFrameAvailable();
        check(value.beginFrame(), "first frame callback");
        value.acceptTimestampNs(1_000_001);
        for (int callback = 0; callback < 10_000; callback++) {
            // A previously queued callback arrives after updateTexImage already
            // consumed its buffer. The production caller skips presentation.
            check(value.onFrameAvailable() && value.beginFrame(), "late callback consumes bounded slot");
            check(value.isRepeatedTimestampNs(1_000_001), "exact repeated latch is drop-only");
        }
        reject(() -> value.acceptTimestampNs(1_000_001), "repeated latch cannot be presented");
        check(!value.isRepeatedTimestampNs(1_000_002), "distinct ns is not a duplicate callback");
        reject(() -> value.acceptTimestampNs(1_000_002), "distinct ns sharing encoded microsecond");
        value.acceptTimestampNs(34_333_334);
        check(!value.isRepeatedTimestampNs(1_000_001), "previous buffer is regression, not repeat");
        reject(() -> value.acceptTimestampNs(1_000_001), "old buffer cannot masquerade as repeat");
        check(value.isRepeatedTimestampNs(34_333_334), "repeats did not shift the accepted timeline");
        value.requestClose();
        check(!value.isRepeatedTimestampNs(34_333_334), "close revokes even drop-only observations");
        value.closeOnOwner();
    }

    /**
     * Synthetic platform producer matrices, not an Android-device result.
     * AOSP GLConsumer applies column-vector P = FlipV * Crop * Rotation.
     * The native oracle must independently derive expected pixels, never reuse
     * this helper or the exported matrices to calculate its expected image.
     */
    private static float[] producer(int sensor, float left, float right, float bottom, float top) {
        float sx = 1 - (left + right) / 1280f;
        float sy = 1 - (bottom + top) / 720f;
        float tx = left / 1280f;
        float ty = bottom / 720f;
        float[] value = identity();
        value[0] = value[1] = value[4] = value[5] = 0;
        switch (sensor) {
            case 0 -> { value[0] = sx; value[5] = -sy; value[12] = tx; value[13] = 1 - ty; }
            case 90 -> { value[4] = -sx; value[1] = -sy; value[12] = tx + sx; value[13] = 1 - ty; }
            case 180 -> { value[0] = -sx; value[5] = sy; value[12] = tx + sx; value[13] = 1 - ty - sy; }
            case 270 -> { value[4] = sx; value[1] = sy; value[12] = tx; value[13] = 1 - ty - sy; }
            default -> throw new AssertionError("test fixture rotation");
        }
        return value;
    }

    private static float[] identity() {
        float[] value = new float[16];
        value[0] = value[5] = value[10] = value[15] = 1;
        return value;
    }

    private static void export(Path directory) throws IOException {
        Files.createDirectories(directory);
        Files.writeString(directory.resolve("vertex.glsl"), FiniteCameraTransform.VERTEX_SHADER,
                StandardCharsets.UTF_8);
        Files.writeString(directory.resolve("fragment.glsl"), FiniteCameraTransform.FRAGMENT_SHADER_EXTERNAL_OES,
                StandardCharsets.UTF_8);
        float[] quad = new float[16];
        plan(0, CameraInventory.LensFacing.BACK).copyInterleavedQuad(quad);
        StringBuilder vertices = new StringBuilder();
        appendFloats(vertices, quad);
        Files.writeString(directory.resolve("vertices.txt"), vertices.append('\n'), StandardCharsets.UTF_8);
        StringBuilder cases = new StringBuilder(
                "# name facing sensor rotation producer geometry[16] texture[16] producer[16]\n");
        for (CameraInventory.LensFacing facing : new CameraInventory.LensFacing[] {
                CameraInventory.LensFacing.BACK, CameraInventory.LensFacing.FRONT}) {
            for (int sensor : CARDINALS) {
                FiniteCameraTransform.Plan value = plan(sensor, facing);
                for (int inset = 0; inset <= 2; inset++) {
                    String variant = inset == 0 ? "raw" : "inset" + inset;
                    String lens = facing.name().toLowerCase(Locale.ROOT);
                    cases.append(lens).append('_').append(sensor).append('_').append(variant)
                            .append(' ').append(lens).append(' ').append(sensor).append(' ')
                            .append(value.clockwiseRotationDegrees()).append(' ').append(variant);
                    float[] geometry = new float[16];
                    float[] texture = new float[16];
                    float[] producer = producer(sensor, inset, inset, inset, inset);
                    value.copyGeometryMatrix(geometry);
                    value.copyTextureMatrix(texture);
                    value.requireProducerMatrix(producer);
                    appendFloats(cases, geometry);
                    appendFloats(cases, texture);
                    appendFloats(cases, producer);
                    cases.append('\n');
                }
            }
        }
        Files.writeString(directory.resolve("cases.tsv"), cases, StandardCharsets.UTF_8);
    }

    private static void appendFloats(StringBuilder target, float[] values) {
        for (float value : values) target.append(' ').append(Float.toString(value));
    }

    private interface CheckedAction { void run() throws Exception; }
    private interface StageHook { void after(FiniteCameraTransform.Stage stage) throws Exception; }

    private static final class FakeBackend implements FiniteCameraTransform.Backend {
        final List<FiniteCameraTransform.Stage> initialized = new ArrayList<>();
        final List<FiniteCameraTransform.Stage> released = new ArrayList<>();
        FiniteCameraTransform.Stage failInitialization;
        FiniteCameraTransform.Stage failRelease;
        StageHook afterInitialize = stage -> {};

        @Override public void initialize(FiniteCameraTransform.Stage stage) throws Exception {
            initialized.add(stage);
            if (stage == failInitialization) throw new IOException("injected stage failure");
            afterInitialize.after(stage);
        }

        @Override public void release(FiniteCameraTransform.Stage stage) {
            released.add(stage);
            if (stage == failRelease) throw new IllegalStateException("injected cleanup failure");
        }

        void requireReverseRelease(int attemptedCount) {
            check(initialized.size() == attemptedCount, "only attempted init stages recorded");
            check(released.size() == attemptedCount, "every attempted stage released exactly once");
            for (int i = 0; i < attemptedCount; i++) {
                check(released.get(i) == initialized.get(attemptedCount - i - 1), "reverse resource order");
            }
        }
    }

    private static void onOtherThread(CheckedAction action) throws Exception {
        AtomicReference<Throwable> failure = new AtomicReference<>();
        Thread thread = new Thread(() -> {
            try { action.run(); }
            catch (Throwable error) { failure.set(error); }
        }, "finite-race-test");
        thread.start();
        thread.join();
        if (failure.get() != null) throw new AssertionError("other thread failed", failure.get());
    }

    private static void reject(Runnable action, String reason) {
        try { action.run(); }
        catch (IllegalArgumentException expected) { return; }
        throw new AssertionError("accepted " + reason);
    }

    private static void rejectState(CheckedAction action, String reason) {
        try { action.run(); }
        catch (IllegalStateException expected) { return; }
        catch (Exception failure) { throw new AssertionError("unexpected exception for " + reason, failure); }
        throw new AssertionError("accepted " + reason);
    }

    private static void close(float actual, float expected, String reason) {
        if (Math.abs(actual - expected) > 0.00001f) {
            throw new AssertionError(reason + ": " + actual + " != " + expected);
        }
    }

    private static void check(boolean condition, String reason) {
        if (!condition) throw new AssertionError(reason);
    }
}
