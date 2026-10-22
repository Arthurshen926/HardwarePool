package io.capyio.camera.contract;

import java.util.Arrays;
import java.util.Objects;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicInteger;

/**
 * Closed, platform-free pixel and lifetime policy for the experimental finite camera.
 * This does not rotate encoder metadata or change the negotiated finite AVC profile.
 */
public final class FiniteCameraTransform {
    public static final int INPUT_WIDTH = 1280;
    public static final int INPUT_HEIGHT = 720;
    public static final int OUTPUT_WIDTH = 1280;
    public static final int OUTPUT_HEIGHT = 720;
    public static final int MAX_PRODUCER_INSET_TEXELS = 2;
    private static final float MATRIX_EPSILON = 0.000001f;

    /** The renderer and native pixel oracle compile these exact sources. */
    public static final String VERTEX_SHADER = """
            attribute vec4 aPosition;
            attribute vec4 aTextureCoord;
            uniform mat4 uGeometryMatrix;
            uniform mat4 uTextureMatrix;
            uniform mat4 uProducerMatrix;
            varying highp vec2 vTextureCoord;
            void main() {
                gl_Position = uGeometryMatrix * aPosition;
                vTextureCoord = (uProducerMatrix * uTextureMatrix * aTextureCoord).xy;
            }
            """;

    public static final String FRAGMENT_SHADER_EXTERNAL_OES = """
            #extension GL_OES_EGL_image_external : require
            precision highp float;
            uniform samplerExternalOES sTexture;
            varying highp vec2 vTextureCoord;
            void main() {
                gl_FragColor = texture2D(sTexture, vTextureCoord);
            }
            """;

    // Triangle strip, interleaved x, y, u, v. Both axes use the OpenGL bottom-left
    // origin. Logical source top-left is (0, 1), not the first row of a CPU array.
    private static final float[] QUAD = {
        -1f, -1f, 0f, 0f,
         1f, -1f, 1f, 0f,
        -1f,  1f, 0f, 1f,
         1f,  1f, 1f, 1f
    };

    private FiniteCameraTransform() {}

    public static Plan plan(Integer sensorOrientationDegrees, CameraInventory.LensFacing facing,
            boolean dynamicSensorOrientation) {
        return plan(INPUT_WIDTH, INPUT_HEIGHT, sensorOrientationDegrees, facing,
                dynamicSensorOrientation);
    }

    public static Plan plan(int inputWidth, int inputHeight, Integer sensorOrientationDegrees,
            CameraInventory.LensFacing facing, boolean dynamicSensorOrientation) {
        if (inputWidth != INPUT_WIDTH || inputHeight != INPUT_HEIGHT) {
            throw new IllegalArgumentException("finite rotation requires an exact 1280x720 input");
        }
        if (sensorOrientationDegrees == null || !isCardinal(sensorOrientationDegrees)
                || dynamicSensorOrientation) {
            throw new IllegalArgumentException("finite rotation requires known static cardinal orientation");
        }
        if (facing == null || facing == CameraInventory.LensFacing.UNKNOWN) {
            throw new IllegalArgumentException("finite rotation requires known camera facing");
        }
        return new Plan(sensorOrientationDegrees, facing);
    }

    private static boolean isCardinal(int degrees) {
        return degrees == 0 || degrees == 90 || degrees == 180 || degrees == 270;
    }

    /**
     * Immutable natural-device-orientation plan. No display/device rotation is
     * inferred, and no front-camera mirror is introduced. The Camera2 output
     * MUST be explicitly configured with API 33 MIRROR_MODE_NONE. AOSP's
     * CameraUtils then retains the sensor's rotation flags without a front
     * reflection. SurfaceTexture's producer matrix ALREADY contains those
     * flags. Adding another sensor rotation would rotate pixels twice.
     *
     * <p>References:
     * https://android.googlesource.com/platform/frameworks/av/+/master/camera/CameraUtils.cpp
     * and https://android.googlesource.com/platform/frameworks/native/+/master/libs/gui/GLConsumerUtils.cpp
     * The front-counterclockwise preview recipe is NOT the mirror-disabled
     * buffer transform contract used here.
     */
    public static final class Plan {
        private final int sensorOrientationDegrees;
        private final CameraInventory.LensFacing facing;
        private final int clockwiseRotationDegrees;

        private Plan(int sensorOrientationDegrees, CameraInventory.LensFacing facing) {
            this.sensorOrientationDegrees = sensorOrientationDegrees;
            this.facing = facing;
            clockwiseRotationDegrees = sensorOrientationDegrees;
        }

        public int inputWidth() { return INPUT_WIDTH; }
        public int inputHeight() { return INPUT_HEIGHT; }
        public int outputWidth() { return OUTPUT_WIDTH; }
        public int outputHeight() { return OUTPUT_HEIGHT; }
        public int sensorOrientationDegrees() { return sensorOrientationDegrees; }
        public CameraInventory.LensFacing facing() { return facing; }
        public int clockwiseRotationDegrees() { return clockwiseRotationDegrees; }
        public boolean mirrored() { return false; }

        /** Exact aspect-fit bounds in output pixels; clear the entire target black first. */
        public float contentWidth() { return isQuarterTurn() ? 405f : OUTPUT_WIDTH; }
        public float contentHeight() { return OUTPUT_HEIGHT; }
        public float contentLeft() { return (OUTPUT_WIDTH - contentWidth()) / 2f; }
        public float contentTop() { return 0f; }

        /** Column-major GL matrix; rotating a 720x1280 image fits at 405x720. */
        public void copyGeometryMatrix(float[] destination) {
            identity(destination);
            destination[0] = isQuarterTurn() ? 405f / OUTPUT_WIDTH : 1f;
        }

        /**
         * Identity deliberately: the producer matrix already includes sensor
         * rotation. Logical UV uses a bottom-left origin. For each latched frame,
         * fetch SurfaceTexture's matrix exactly once, validate it, and apply it
         * once. Never cache it across updateTexImage calls or rotate it again.
         */
        public void copyTextureMatrix(float[] destination) {
            identity(destination);
        }

        /**
         * Admit only the pinned no-mirror sensor rotation, a tiny filter inset,
         * and GLConsumer's final vertical conversion from buffer top-left to GL
         * bottom-left. In column-vector order P = FlipV * Crop * Rotation.
         * Insets may differ on each edge but cannot exceed two raw texels.
         * Unsupported platform/vendor transforms fail instead of being guessed.
         * This validates the original producer matrix without modifying it.
         */
        public void requireProducerMatrix(float[] matrix) {
            require16(matrix);
            for (float value : matrix) {
                if (!Float.isFinite(value)) rejectProducer();
            }
            if (!near(matrix[2], 0f) || !near(matrix[3], 0f)
                    || !near(matrix[6], 0f) || !near(matrix[7], 0f)
                    || !near(matrix[8], 0f) || !near(matrix[9], 0f)
                    || !near(matrix[10], 1f) || !near(matrix[11], 0f)
                    || !near(matrix[14], 0f) || !near(matrix[15], 1f)) rejectProducer();

            final float scaleX;
            final float scaleY;
            final float insetLeft;
            final float insetBottom;
            switch (sensorOrientationDegrees) {
                case 0 -> {
                    if (!near(matrix[1], 0f) || !near(matrix[4], 0f)) rejectProducer();
                    scaleX = matrix[0];
                    scaleY = -matrix[5];
                    insetLeft = matrix[12];
                    insetBottom = 1f - matrix[13];
                }
                case 90 -> {
                    if (!near(matrix[0], 0f) || !near(matrix[5], 0f)) rejectProducer();
                    scaleX = -matrix[4];
                    scaleY = -matrix[1];
                    insetLeft = matrix[12] - scaleX;
                    insetBottom = 1f - matrix[13];
                }
                case 180 -> {
                    if (!near(matrix[1], 0f) || !near(matrix[4], 0f)) rejectProducer();
                    scaleX = -matrix[0];
                    scaleY = matrix[5];
                    insetLeft = matrix[12] - scaleX;
                    insetBottom = 1f - matrix[13] - scaleY;
                }
                case 270 -> {
                    if (!near(matrix[0], 0f) || !near(matrix[5], 0f)) rejectProducer();
                    scaleX = matrix[4];
                    scaleY = matrix[1];
                    insetLeft = matrix[12];
                    insetBottom = 1f - matrix[13] - scaleY;
                }
                default -> throw new AssertionError("validated cardinal rotation changed");
            }
            requireInsets(scaleX, insetLeft, INPUT_WIDTH);
            requireInsets(scaleY, insetBottom, INPUT_HEIGHT);
        }

        public void copyInterleavedQuad(float[] destination) {
            require16(destination);
            System.arraycopy(QUAD, 0, destination, 0, QUAD.length);
        }

        private boolean isQuarterTurn() {
            return clockwiseRotationDegrees == 90 || clockwiseRotationDegrees == 270;
        }

        @Override public boolean equals(Object other) {
            return other instanceof Plan plan
                    && sensorOrientationDegrees == plan.sensorOrientationDegrees && facing == plan.facing;
        }

        @Override public int hashCode() { return 31 * sensorOrientationDegrees + facing.hashCode(); }
    }

    private static void require16(float[] values) {
        if (values == null || values.length != 16) {
            throw new IllegalArgumentException("finite transform requires exactly sixteen floats");
        }
    }

    private static void identity(float[] destination) {
        require16(destination);
        Arrays.fill(destination, 0f);
        destination[0] = destination[5] = destination[10] = destination[15] = 1f;
    }

    private static boolean near(float actual, float expected) {
        return Math.abs(actual - expected) <= MATRIX_EPSILON;
    }

    private static void requireInsets(float scale, float firstInset, int dimension) {
        float limit = (float) MAX_PRODUCER_INSET_TEXELS / dimension;
        float secondInset = 1f - firstInset - scale;
        if (scale <= 0f || scale > 1f + MATRIX_EPSILON
                || firstInset < -MATRIX_EPSILON || firstInset > limit + MATRIX_EPSILON
                || secondInset < -MATRIX_EPSILON || secondInset > limit + MATRIX_EPSILON) {
            rejectProducer();
        }
    }

    private static void rejectProducer() {
        throw new IllegalArgumentException("finite producer transform differs from the pinned no-mirror rotation");
    }

    /** Explicit initialization order and exact reverse cleanup order. */
    public enum Stage {
        EGL_DISPLAY, EGL_CONTEXT, EGL_SURFACE, GL_PROGRAM, EXTERNAL_TEXTURE,
        SURFACE_TEXTURE, CAMERA_SURFACE
    }

    public enum State { NEW, INITIALIZING, READY, CLOSING, CLOSED }

    /**
     * Injected platform operations. A stage is registered before initialize, so
     * release MUST tolerate a partially initialized stage and absent handles.
     * Resources belong only to this instance, never to the encoder or preview.
     */
    public interface Backend {
        void initialize(Stage stage) throws Exception;
        void release(Stage stage);
    }

    /**
     * Production owner state shared with deterministic fault/race tests.
     * EGL, initialization, timestamp admission and cleanup run on one owner.
     * Camera notifications and close requests may originate on other threads.
     * The one-bit latest-frame slot never queues frame objects or grows a list.
     */
    public static final class Lifecycle {
        private static final int EMPTY = 0;
        private static final int PENDING = 1;
        private static final int CLOSED = 2;
        private static final Stage[] STAGES = Stage.values();
        private final Backend backend;
        private final AtomicBoolean initializationAttempted = new AtomicBoolean();
        private final AtomicInteger frameSlot = new AtomicInteger(EMPTY);
        private volatile State state = State.NEW;
        private Thread owner;
        private int attemptedStages;
        private long lastTimestampNs = -1;
        private long lastTimestampUs = -1;

        public Lifecycle(Backend backend) { this.backend = Objects.requireNonNull(backend, "backend"); }

        /** Returns false if close won before or during initialization. Never retries. */
        public boolean initialize() throws Exception {
            if (!initializationAttempted.compareAndSet(false, true) || state != State.NEW) {
                throw new IllegalStateException("finite renderer initialization is one-shot");
            }
            owner = Thread.currentThread();
            state = State.INITIALIZING;
            try {
                for (Stage stage : STAGES) {
                    if (frameSlot.get() == CLOSED) {
                        closeOnOwner();
                        return false;
                    }
                    attemptedStages++;
                    backend.initialize(stage);
                }
                state = State.READY;
                if (frameSlot.get() == CLOSED) {
                    closeOnOwner();
                    return false;
                }
                return true;
            } catch (Exception | Error failure) {
                try {
                    closeOnOwner();
                } catch (RuntimeException | Error cleanupFailure) {
                    failure.addSuppressed(cleanupFailure);
                }
                throw failure;
            }
        }

        /** True authorizes posting the same preallocated render runnable once. */
        public boolean onFrameAvailable() {
            return state == State.READY && frameSlot.compareAndSet(EMPTY, PENDING);
        }

        /** Owner consumes the latest available frame, not one object per callback. */
        public boolean beginFrame() {
            requireOwner();
            return state == State.READY && frameSlot.compareAndSet(PENDING, EMPTY);
        }

        /**
         * A delayed callback may latch a buffer already consumed by a previous
         * latest-frame update. This is a drop-only observation, not a new frame:
         * it changes no timestamp, progress, or lifetime state. The caller must
         * neither present it nor refresh the existing no-progress watchdog.
         */
        public boolean isRepeatedTimestampNs(long timestampNs) {
            requireOwner();
            return mayPresent() && timestampNs > 0 && timestampNs == lastTimestampNs;
        }

        /**
         * Preserve the source nanoseconds for eglPresentationTimeANDROID. Also
         * reject collisions after MediaCodec's nanosecond-to-microsecond loss.
         */
        public long acceptTimestampNs(long timestampNs) {
            requireOwner();
            if (!mayPresent()) throw new IllegalStateException("finite renderer is closed");
            long timestampUs = timestampNs / 1_000L;
            if (timestampNs <= 0 || timestampNs <= lastTimestampNs || timestampUs <= lastTimestampUs) {
                throw new IllegalArgumentException("finite frame timestamps must increase at codec precision");
            }
            lastTimestampNs = timestampNs;
            lastTimestampUs = timestampUs;
            return timestampNs;
        }

        /** Recheck immediately before presentation; a close request revokes admission. */
        public boolean mayPresent() { return state == State.READY && frameSlot.get() != CLOSED; }
        public State state() { return state; }

        /** Thread-safe revocation only. The caller must schedule closeOnOwner. */
        public boolean requestClose() { return frameSlot.getAndSet(CLOSED) != CLOSED; }

        /** Idempotent owner cleanup; every attempted stage is released even after a failure. */
        public void closeOnOwner() {
            if (owner == null) owner = Thread.currentThread();
            requireOwner();
            requestClose();
            if (state == State.CLOSED) return;
            state = State.CLOSING;
            Throwable firstFailure = null;
            try {
                while (attemptedStages > 0) {
                    Stage stage = STAGES[--attemptedStages];
                    try {
                        backend.release(stage);
                    } catch (RuntimeException | Error failure) {
                        if (firstFailure == null) firstFailure = failure;
                        else firstFailure.addSuppressed(failure);
                    }
                }
            } finally {
                state = State.CLOSED;
            }
            if (firstFailure != null) {
                throw new IllegalStateException("finite renderer resource cleanup failed", firstFailure);
            }
        }

        private void requireOwner() {
            if (owner != Thread.currentThread()) {
                throw new IllegalStateException("finite renderer operation must run on its serialized owner");
            }
        }
    }
}
