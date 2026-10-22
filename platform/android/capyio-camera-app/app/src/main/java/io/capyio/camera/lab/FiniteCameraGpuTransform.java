package io.capyio.camera.lab;

import android.graphics.SurfaceTexture;
import android.opengl.EGL14;
import android.opengl.EGLConfig;
import android.opengl.EGLContext;
import android.opengl.EGLDisplay;
import android.opengl.EGLExt;
import android.opengl.EGLSurface;
import android.opengl.GLES11Ext;
import android.opengl.GLES20;
import android.os.Handler;
import android.os.HandlerThread;
import android.view.Surface;
import io.capyio.camera.contract.FiniteCameraTransform;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.FloatBuffer;
import java.util.Objects;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.function.Consumer;

/**
 * Finite-only Camera2 OES input -> canonical pixels -> existing AVC input Surface.
 *
 * <p>All EGL/GL/SurfaceTexture latch and release operations use one dedicated owner.
 * Camera callbacks only set a bounded latest-frame flag and enqueue at most one
 * preallocated render task. No pixel payload is copied into application memory.
 * Stop is cooperative: it invalidates the original secure generation immediately,
 * but an uninterruptible platform driver call can delay release. In that case the
 * encoder Surface is retained, never released underneath a driver still using it.</p>
 */
final class FiniteCameraGpuTransform {
    private static final long INITIALIZATION_TIMEOUT_MILLIS = 3_000L;
    private static final int EGL_RECORDABLE_ANDROID = 0x3142;
    private static final int FLOAT_BYTES = Float.BYTES;
    private static final int QUAD_STRIDE_BYTES = 4 * FLOAT_BYTES;

    private final Surface encoderSurface;
    private final FiniteCameraTransform.Plan plan;
    private final SecureCameraBinding secureBinding;
    private final CameraRouteBinding routeBinding;
    private final Handler callbackHandler;
    private final Consumer<String> failure;
    private final HandlerThread ownerThread = new HandlerThread("CapyIO-FiniteCamera-EGL");
    private final Handler ownerHandler;
    private final FiniteCameraTransform.Lifecycle lifecycle;
    private final AtomicBoolean failed = new AtomicBoolean();
    private final AtomicBoolean startRequested = new AtomicBoolean();
    private final AtomicBoolean releaseRequested = new AtomicBoolean();
    private final float[] geometryMatrix = new float[16];
    private final float[] textureMatrix = new float[16];
    private final float[] producerMatrix = new float[16];
    private final FloatBuffer quad;
    private final Runnable renderTask = this::renderLatest;
    private final Runnable releaseTask = this::releaseOnOwner;
    private final Runnable initializationTimeout = () ->
            fail("Finite camera EGL initialization timed out; driver cleanup may still be pending");
    private volatile Runnable afterRelease;
    private volatile Surface cameraSurface;

    // Only the GL owner accesses the following handles, including during partial init.
    private EGLDisplay display = EGL14.EGL_NO_DISPLAY;
    private EGLContext context = EGL14.EGL_NO_CONTEXT;
    private EGLSurface window = EGL14.EGL_NO_SURFACE;
    private EGLConfig config;
    private boolean displayInitialized;
    private boolean contextCurrent;
    private int program;
    private int externalTexture;
    private SurfaceTexture texture;
    private int positionAttribute;
    private int textureAttribute;
    private int geometryUniform;
    private int textureUniform;
    private int producerUniform;
    private int samplerUniform;

    FiniteCameraGpuTransform(Surface encoderSurface, FiniteCameraPreflight.Plan finitePlan,
            SecureCameraBinding secureBinding, CameraRouteBinding routeBinding,
            Handler callbackHandler, Consumer<String> failure) {
        this.encoderSurface = Objects.requireNonNull(encoderSurface, "encoderSurface");
        this.plan = Objects.requireNonNull(finitePlan, "finitePlan").transformPlan();
        this.secureBinding = Objects.requireNonNull(secureBinding, "secureBinding");
        this.routeBinding = Objects.requireNonNull(routeBinding, "routeBinding");
        this.callbackHandler = Objects.requireNonNull(callbackHandler, "callbackHandler");
        this.failure = Objects.requireNonNull(failure, "failure");
        if (secureBinding.finitePlan() != finitePlan) {
            throw new IllegalArgumentException("finite GPU input requires the original generation plan");
        }
        float[] vertices = new float[16];
        plan.copyInterleavedQuad(vertices);
        quad = ByteBuffer.allocateDirect(vertices.length * FLOAT_BYTES)
                .order(ByteOrder.nativeOrder()).asFloatBuffer();
        quad.put(vertices).position(0);
        plan.copyGeometryMatrix(geometryMatrix);
        plan.copyTextureMatrix(textureMatrix);
        lifecycle = new FiniteCameraTransform.Lifecycle(new FiniteCameraTransform.Backend() {
            @Override public void initialize(FiniteCameraTransform.Stage stage) {
                try { initializeStage(stage); }
                catch (RuntimeException error) {
                    fail("Finite camera EGL resource initialization failed: " + stage);
                    throw error;
                }
            }
            @Override public void release(FiniteCameraTransform.Stage stage) {
                try { releaseStage(stage); }
                catch (RuntimeException error) {
                    fail("Finite camera EGL resource release failed: " + stage);
                    throw error;
                }
            }
        });
        ownerThread.start();
        ownerHandler = new Handler(ownerThread.getLooper());
    }

    void start(Runnable ready) {
        Objects.requireNonNull(ready, "ready");
        if (!startRequested.compareAndSet(false, true)) {
            throw new IllegalStateException("finite pixel transform is one-shot");
        }
        if (releaseRequested.get()) return;
        if (!callbackHandler.postDelayed(initializationTimeout, INITIALIZATION_TIMEOUT_MILLIS)) {
            fail("Finite camera initialization deadline could not be scheduled");
            return;
        }
        if (!ownerHandler.post(() -> {
            try {
                requireAuthority();
                if (!lifecycle.initialize()) return;
                requireAuthority();
                if (!callbackHandler.post(() -> {
                    callbackHandler.removeCallbacks(initializationTimeout);
                    if (!lifecycle.mayPresent()) return;
                    try {
                        requireAuthority();
                        ready.run();
                    } catch (RuntimeException error) {
                        fail("Finite camera initialization was cancelled or failed");
                    }
                })) fail("Finite camera readiness callback was rejected");
            } catch (Exception error) {
                fail("Finite camera EGL initialization failed: " + error.getClass().getSimpleName());
            }
        })) fail("Finite camera EGL owner rejected initialization");
    }

    Surface inputSurface() {
        Surface input = cameraSurface;
        if (!lifecycle.mayPresent() || input == null || !input.isValid()) {
            throw new IllegalStateException("finite camera texture input is unavailable");
        }
        return input;
    }

    /** Cancel admission from any thread; only the owner may release GL resources. */
    void requestStop() {
        lifecycle.requestClose();
        ownerHandler.removeCallbacks(renderTask);
        callbackHandler.removeCallbacks(initializationTimeout);
    }

    /** Called only after Camera2 producers have been stopped/closed. Never waits on a driver. */
    void closeAfterCameraStopped(Runnable completion) {
        Objects.requireNonNull(completion, "completion");
        if (!releaseRequested.compareAndSet(false, true)) return;
        afterRelease = completion;
        requestStop();
        if (!ownerHandler.post(releaseTask)) {
            // The owner is intentionally kept alive until this task; never cross-thread release.
            fail("Finite camera EGL owner rejected teardown; encoder resources are retained");
        }
    }

    private void frameAvailable(SurfaceTexture ignored) {
        if (lifecycle.onFrameAvailable() && !ownerHandler.post(renderTask)) {
            fail("Finite camera render callback was rejected");
        }
    }

    private void renderLatest() {
        if (!lifecycle.beginFrame()) return;
        try {
            requireAuthority();
            requireCurrentContext();
            texture.updateTexImage();
            texture.getTransformMatrix(producerMatrix);
            requireAuthority();
            if (!lifecycle.mayPresent()) return;
            plan.requireProducerMatrix(producerMatrix);
            long producerTimestampNs = texture.getTimestamp();
            // A delayed callback may refer to the buffer updateTexImage already consumed.
            // Dropping that exact repeat does not present, count progress, or refresh authority.
            if (lifecycle.isRepeatedTimestampNs(producerTimestampNs)) return;
            lifecycle.acceptTimestampNs(producerTimestampNs);
            // The producer matrix is sampled on every latched buffer, never cached across frames.
            requireAuthority();
            if (!lifecycle.mayPresent()) return;
            GLES20.glViewport(0, 0, plan.outputWidth(), plan.outputHeight());
            GLES20.glDisable(GLES20.GL_SCISSOR_TEST);
            GLES20.glDisable(GLES20.GL_BLEND);
            GLES20.glClearColor(0f, 0f, 0f, 1f);
            GLES20.glClear(GLES20.GL_COLOR_BUFFER_BIT);
            GLES20.glUseProgram(program);
            GLES20.glActiveTexture(GLES20.GL_TEXTURE0);
            GLES20.glBindTexture(GLES11Ext.GL_TEXTURE_EXTERNAL_OES, externalTexture);
            GLES20.glUniform1i(samplerUniform, 0);
            GLES20.glUniformMatrix4fv(geometryUniform, 1, false, geometryMatrix, 0);
            GLES20.glUniformMatrix4fv(textureUniform, 1, false, textureMatrix, 0);
            GLES20.glUniformMatrix4fv(producerUniform, 1, false, producerMatrix, 0);
            quad.position(0);
            GLES20.glVertexAttribPointer(positionAttribute, 2, GLES20.GL_FLOAT, false,
                    QUAD_STRIDE_BYTES, quad);
            quad.position(2);
            GLES20.glVertexAttribPointer(textureAttribute, 2, GLES20.GL_FLOAT, false,
                    QUAD_STRIDE_BYTES, quad);
            GLES20.glEnableVertexAttribArray(positionAttribute);
            GLES20.glEnableVertexAttribArray(textureAttribute);
            GLES20.glDrawArrays(GLES20.GL_TRIANGLE_STRIP, 0, 4);
            checkGl("finite draw");
            requireAuthority();
            if (!lifecycle.mayPresent()) return;
            // Keep producer time intact. Never substitute, rebase, increment or regenerate PTS.
            requireEgl(EGLExt.eglPresentationTimeANDROID(display, window, producerTimestampNs),
                    "finite presentation timestamp");
            requireAuthority();
            if (!lifecycle.mayPresent()) return;
            requireEgl(EGL14.eglSwapBuffers(display, window), "finite encoder swap");
            requireAuthority();
        } catch (RuntimeException error) {
            fail("Finite camera pixel transform failed: " + error.getClass().getSimpleName());
        }
    }

    private void requireAuthority() {
        if (!routeBinding.isLive() || !secureBinding.ready()) {
            throw new IllegalStateException("original finite camera generation is no longer live");
        }
    }

    private void initializeStage(FiniteCameraTransform.Stage stage) {
        requireAuthority();
        switch (stage) {
            case EGL_DISPLAY -> {
                if (!encoderSurface.isValid()) throw new IllegalStateException("encoder Surface is invalid");
                display = EGL14.eglGetDisplay(EGL14.EGL_DEFAULT_DISPLAY);
                if (display == EGL14.EGL_NO_DISPLAY) throw new IllegalStateException("EGL display unavailable");
                int[] versions = new int[2];
                requireEgl(EGL14.eglInitialize(display, versions, 0, versions, 1), "EGL initialize");
                displayInitialized = true;
                String extensions = EGL14.eglQueryString(display, EGL14.EGL_EXTENSIONS);
                if (extensions == null || !(" " + extensions + " ").contains(" EGL_ANDROID_presentation_time ")) {
                    throw new IllegalStateException("EGL producer timestamp extension unavailable");
                }
                int[] attributes = {EGL14.EGL_RED_SIZE, 8, EGL14.EGL_GREEN_SIZE, 8,
                        EGL14.EGL_BLUE_SIZE, 8, EGL14.EGL_ALPHA_SIZE, 8,
                        EGL14.EGL_SURFACE_TYPE, EGL14.EGL_WINDOW_BIT,
                        EGL14.EGL_RENDERABLE_TYPE, EGL14.EGL_OPENGL_ES2_BIT,
                        EGL_RECORDABLE_ANDROID, 1, EGL14.EGL_NONE};
                EGLConfig[] configs = new EGLConfig[1];
                int[] count = new int[1];
                requireEgl(EGL14.eglChooseConfig(display, attributes, 0, configs, 0, 1, count, 0),
                        "recordable EGL configuration");
                if (count[0] != 1 || configs[0] == null) {
                    throw new IllegalStateException("recordable EGL2 configuration unavailable");
                }
                config = configs[0];
            }
            case EGL_CONTEXT -> {
                context = EGL14.eglCreateContext(display, config, EGL14.EGL_NO_CONTEXT,
                        new int[] {EGL14.EGL_CONTEXT_CLIENT_VERSION, 2, EGL14.EGL_NONE}, 0);
                if (context == null || context == EGL14.EGL_NO_CONTEXT) {
                    throw new IllegalStateException("EGL2 context creation failed");
                }
            }
            case EGL_SURFACE -> {
                window = EGL14.eglCreateWindowSurface(display, config, encoderSurface,
                        new int[] {EGL14.EGL_NONE}, 0);
                if (window == null || window == EGL14.EGL_NO_SURFACE) {
                    throw new IllegalStateException("encoder EGL window creation failed");
                }
                requireEgl(EGL14.eglMakeCurrent(display, window, window, context), "EGL make current");
                contextCurrent = true;
                int[] value = new int[1];
                requireEgl(EGL14.eglQuerySurface(display, window, EGL14.EGL_WIDTH, value, 0), "EGL width");
                if (value[0] != plan.outputWidth()) throw new IllegalStateException("encoder width changed");
                requireEgl(EGL14.eglQuerySurface(display, window, EGL14.EGL_HEIGHT, value, 0), "EGL height");
                if (value[0] != plan.outputHeight()) throw new IllegalStateException("encoder height changed");
            }
            case GL_PROGRAM -> createProgram();
            case EXTERNAL_TEXTURE -> {
                int[] textures = new int[1];
                GLES20.glGenTextures(1, textures, 0);
                externalTexture = textures[0];
                if (externalTexture == 0) throw new IllegalStateException("OES texture creation failed");
                GLES20.glBindTexture(GLES11Ext.GL_TEXTURE_EXTERNAL_OES, externalTexture);
                GLES20.glTexParameteri(GLES11Ext.GL_TEXTURE_EXTERNAL_OES,
                        GLES20.GL_TEXTURE_MIN_FILTER, GLES20.GL_LINEAR);
                GLES20.glTexParameteri(GLES11Ext.GL_TEXTURE_EXTERNAL_OES,
                        GLES20.GL_TEXTURE_MAG_FILTER, GLES20.GL_LINEAR);
                GLES20.glTexParameteri(GLES11Ext.GL_TEXTURE_EXTERNAL_OES,
                        GLES20.GL_TEXTURE_WRAP_S, GLES20.GL_CLAMP_TO_EDGE);
                GLES20.glTexParameteri(GLES11Ext.GL_TEXTURE_EXTERNAL_OES,
                        GLES20.GL_TEXTURE_WRAP_T, GLES20.GL_CLAMP_TO_EDGE);
                checkGl("OES texture setup");
            }
            case SURFACE_TEXTURE -> {
                texture = new SurfaceTexture(externalTexture);
                texture.setDefaultBufferSize(plan.inputWidth(), plan.inputHeight());
                texture.setOnFrameAvailableListener(this::frameAvailable, callbackHandler);
            }
            case CAMERA_SURFACE -> {
                cameraSurface = new Surface(texture);
                if (!cameraSurface.isValid()) throw new IllegalStateException("camera Surface is invalid");
            }
        }
        requireAuthority();
    }

    private void createProgram() {
        requireCurrentContext();
        int vertex = 0;
        int fragment = 0;
        try {
            vertex = compileShader(GLES20.GL_VERTEX_SHADER, FiniteCameraTransform.VERTEX_SHADER);
            fragment = compileShader(GLES20.GL_FRAGMENT_SHADER, FiniteCameraTransform.FRAGMENT_SHADER_EXTERNAL_OES);
            program = GLES20.glCreateProgram();
            if (program == 0) throw new IllegalStateException("GL program creation failed");
            GLES20.glAttachShader(program, vertex);
            GLES20.glAttachShader(program, fragment);
            GLES20.glLinkProgram(program);
            int[] result = new int[1];
            GLES20.glGetProgramiv(program, GLES20.GL_LINK_STATUS, result, 0);
            if (result[0] == 0) throw new IllegalStateException("finite GL program link failed");
            positionAttribute = requireLocation(GLES20.glGetAttribLocation(program, "aPosition"));
            textureAttribute = requireLocation(GLES20.glGetAttribLocation(program, "aTextureCoord"));
            geometryUniform = requireLocation(GLES20.glGetUniformLocation(program, "uGeometryMatrix"));
            textureUniform = requireLocation(GLES20.glGetUniformLocation(program, "uTextureMatrix"));
            producerUniform = requireLocation(GLES20.glGetUniformLocation(program, "uProducerMatrix"));
            samplerUniform = requireLocation(GLES20.glGetUniformLocation(program, "sTexture"));
            checkGl("finite program setup");
        } finally {
            if (vertex != 0) GLES20.glDeleteShader(vertex);
            if (fragment != 0) GLES20.glDeleteShader(fragment);
        }
    }

    private static int compileShader(int type, String source) {
        int shader = GLES20.glCreateShader(type);
        if (shader == 0) throw new IllegalStateException("finite shader creation failed");
        try {
            GLES20.glShaderSource(shader, source);
            GLES20.glCompileShader(shader);
            int[] result = new int[1];
            GLES20.glGetShaderiv(shader, GLES20.GL_COMPILE_STATUS, result, 0);
            if (result[0] == 0) throw new IllegalStateException("finite shader compilation failed");
            return shader;
        } catch (RuntimeException error) {
            GLES20.glDeleteShader(shader);
            throw error;
        }
    }

    private static int requireLocation(int value) {
        if (value < 0) throw new IllegalStateException("finite shader input is missing");
        return value;
    }

    private void requireCurrentContext() {
        if (!contextCurrent || !context.equals(EGL14.eglGetCurrentContext())) {
            throw new IllegalStateException("finite EGL context is not current on its owner");
        }
    }

    private static void requireEgl(boolean success, String operation) {
        if (!success) throw new IllegalStateException(operation + " failed");
    }

    private static void checkGl(String operation) {
        if (GLES20.glGetError() != GLES20.GL_NO_ERROR) {
            throw new IllegalStateException(operation + " failed");
        }
    }

    private void releaseStage(FiniteCameraTransform.Stage stage) {
        switch (stage) {
            case CAMERA_SURFACE -> {
                Surface input = cameraSurface;
                cameraSurface = null;
                if (input != null) input.release();
            }
            case SURFACE_TEXTURE -> {
                SurfaceTexture input = texture;
                texture = null;
                if (input != null) {
                    try { input.setOnFrameAvailableListener(null); }
                    finally { input.release(); }
                }
            }
            case EXTERNAL_TEXTURE -> {
                if (externalTexture != 0 && contextCurrent) {
                    GLES20.glDeleteTextures(1, new int[] {externalTexture}, 0);
                    checkGl("OES texture release");
                }
                externalTexture = 0;
            }
            case GL_PROGRAM -> {
                if (program != 0 && contextCurrent) {
                    GLES20.glDeleteProgram(program);
                    checkGl("finite program release");
                }
                program = 0;
            }
            case EGL_SURFACE -> {
                if (display != EGL14.EGL_NO_DISPLAY) {
                    boolean unbound = EGL14.eglMakeCurrent(display, EGL14.EGL_NO_SURFACE,
                            EGL14.EGL_NO_SURFACE, EGL14.EGL_NO_CONTEXT);
                    contextCurrent = false;
                    boolean destroyed = window == EGL14.EGL_NO_SURFACE || window == null
                            || EGL14.eglDestroySurface(display, window);
                    window = EGL14.EGL_NO_SURFACE;
                    requireEgl(unbound && destroyed, "finite EGL window release");
                }
            }
            case EGL_CONTEXT -> {
                if (context != EGL14.EGL_NO_CONTEXT && context != null) {
                    boolean destroyed = EGL14.eglDestroyContext(display, context);
                    context = EGL14.EGL_NO_CONTEXT;
                    requireEgl(destroyed, "finite EGL context release");
                }
            }
            case EGL_DISPLAY -> {
                boolean terminated = !displayInitialized || EGL14.eglTerminate(display);
                displayInitialized = false;
                display = EGL14.EGL_NO_DISPLAY;
                config = null;
                boolean released = EGL14.eglReleaseThread();
                requireEgl(terminated && released, "finite EGL display release");
            }
        }
    }

    private void releaseOnOwner() {
        try {
            lifecycle.closeOnOwner();
        } catch (RuntimeException error) {
            fail("Finite camera EGL teardown failed: " + error.getClass().getSimpleName());
        } finally {
            // A return from the owner release, not a timeout, permits encoder Surface release.
            try {
                Runnable completion = afterRelease;
                afterRelease = null;
                if (completion != null) completion.run();
            } finally {
                ownerThread.quitSafely();
            }
        }
    }

    private void fail(String reason) {
        requestStop();
        // Native cancellation is synchronous, before the service's posted failure cleanup.
        // An unexpected JNI failure must not kill the GL owner before it can release resources.
        String reported = reason;
        try { secureBinding.stop(); }
        catch (RuntimeException error) { reported += "; native generation cancellation failed"; }
        if (failed.compareAndSet(false, true)) failure.accept(reported);
    }
}
