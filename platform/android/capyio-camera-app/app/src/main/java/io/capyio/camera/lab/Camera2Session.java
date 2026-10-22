package io.capyio.camera.lab;

import android.Manifest;
import android.content.Context;
import android.content.pm.PackageManager;
import android.graphics.SurfaceTexture;
import android.hardware.camera2.CameraAccessException;
import android.hardware.camera2.CameraCaptureSession;
import android.hardware.camera2.CameraCharacteristics;
import android.hardware.camera2.CameraDevice;
import android.hardware.camera2.CameraManager;
import android.hardware.camera2.CaptureRequest;
import android.hardware.camera2.CaptureResult;
import android.hardware.camera2.TotalCaptureResult;
import android.hardware.camera2.params.OutputConfiguration;
import android.hardware.camera2.params.SessionConfiguration;
import android.media.MediaCodec;
import android.os.Build;
import android.os.Handler;
import android.os.HandlerThread;
import android.os.SystemClock;
import android.util.Range;
import android.util.Size;
import android.view.Surface;
import android.view.TextureView;
import io.capyio.camera.contract.AvcEncoderConfig;
import io.capyio.camera.contract.SecureAvcSender;
import io.capyio.camera.contract.AvcQualityPreset;
import io.capyio.camera.contract.CameraFacingPolicy;
import io.capyio.camera.contract.CameraProgressWatchdog;
import io.capyio.camera.contract.CameraSourceSelection;
import io.capyio.camera.contract.CameraTransportEndpoint;
import io.capyio.camera.contract.EncodedAvcAccessUnit;
import io.capyio.camera.contract.FrameObservation;
import java.io.IOException;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import java.util.Objects;
import java.util.concurrent.Executor;

/** Owns one Camera2 device and one MediaCodec AVC input Surface, with an optional preview. */
final class Camera2Session implements AutoCloseable {
    interface Listener {
        void onStarted(Size size, FrameObservation.LensFacing facing, String sourceKey);

        void onFrame(FrameObservation observation);

        void onEncodedAccessUnit(EncodedAvcAccessUnit unit, long droppedAccessUnits);

        default void onSecureEncoded(long count) {}

        void onEncoderStatus(String codecName, int requestedLatencyFrames, int actualLatencyFrames);

        void onTransportStatus(String state, long sentAccessUnits, long droppedAccessUnits);

        void onError(String reason);

        void onClosed();
    }

    private static final int MAX_WIDTH = 1280;
    private static final int MAX_HEIGHT = 720;

    private final Context context;
    private final TextureView preview;
    private final Listener listener;
    private final FrameObservation.LensFacing preferredFacing;
    private final CameraSourceSelection preferredSource;
    private final AvcQualityPreset qualityPreset;
    private final CameraTransportEndpoint transportEndpoint;
    private final Object lock = new Object();

    private HandlerThread cameraThread;
    private Handler cameraHandler;
    private CameraDevice cameraDevice;
    private CameraCaptureSession captureSession;
    private MediaCodecSurfaceEncoder encoder;
    private FiniteCameraGpuTransform finiteTransform;
    private AvcSender sender;
    private SecureCameraBinding secureBinding;
    private long lastSecureEncoded;
    private Surface previewSurface;
    private Size captureSize;
    private boolean closed;
    private boolean closeNotified;
    private long sequence;
    private int sensorOrientationDegrees;
    private FrameObservation.LensFacing lensFacing = FrameObservation.LensFacing.UNKNOWN;
    private String sourceKey = "unknown";
    private Float targetZoomRatio;
    private int lastReportedLatencyFrames = Integer.MIN_VALUE;
    private long lastEncodedProgressMillis = -1;
    private boolean progressFailureReported;
    private final Runnable progressWatchdog = this::checkProgressWatchdog;
    private CameraRouteBinding routeBinding;

    void bindSecure(SecureCameraBinding binding) {
        if (cameraThread != null) throw new IllegalStateException("camera already started");
        secureBinding = binding;
    }

    void bindRoute(CameraRouteBinding binding) {
        if (cameraThread != null) throw new IllegalStateException("camera already started");
        routeBinding = binding;
    }

    Camera2Session(
            Context context,
            TextureView preview,
            FrameObservation.LensFacing preferredFacing,
            CameraSourceSelection preferredSource,
            AvcQualityPreset qualityPreset,
            CameraTransportEndpoint transportEndpoint,
            Listener listener) {
        this.context = Objects.requireNonNull(context, "context");
        this.preview = preview;
        this.preferredFacing = Objects.requireNonNull(preferredFacing, "preferredFacing");
        this.preferredSource = preferredSource;
        this.qualityPreset = Objects.requireNonNull(qualityPreset, "qualityPreset");
        this.transportEndpoint = transportEndpoint;
        this.listener = Objects.requireNonNull(listener, "listener");
    }

    Camera2Session(
            Context context,
            FrameObservation.LensFacing preferredFacing,
            CameraSourceSelection preferredSource,
            AvcQualityPreset qualityPreset,
            CameraTransportEndpoint transportEndpoint,
            Listener listener) {
        this(
                context,
                null,
                preferredFacing,
                preferredSource,
                qualityPreset,
                transportEndpoint,
                listener);
    }

    void start() {
        synchronized (lock) {
            if (cameraThread != null || closed) {
                throw new IllegalStateException("camera session already started or closed");
            }
            cameraThread = new HandlerThread("CapyIO-Camera2");
            cameraThread.start();
            cameraHandler = new Handler(cameraThread.getLooper());
        }

        Handler handler;
        synchronized (lock) { handler = cameraHandler; }
        if (handler == null || !handler.post(() -> {
            try {
                openSelectedCamera();
            } catch (CameraAccessException | IOException | RuntimeException error) {
                fail("Unable to open Camera2: " + error.getClass().getSimpleName());
            }
        })) fail("Camera2 initialization callback was rejected");
    }

    private void openSelectedCamera() throws CameraAccessException, IOException {
        synchronized (lock) { if (closed) return; }
        if (context.checkSelfPermission(Manifest.permission.CAMERA) != PackageManager.PERMISSION_GRANTED) {
            fail("Camera permission is not granted");
            return;
        }

        CameraManager manager = context.getSystemService(CameraManager.class);
        FiniteCameraPreflight.Plan finitePlan = secureBinding == null ? null : secureBinding.finitePlan();
        Selection selection;
        if (finitePlan != null) {
            if (!finitePlan.source().equals(preferredSource) || qualityPreset != AvcQualityPreset.BALANCED) {
                throw new IllegalStateException("finite capture source or quality changed");
            }
            FiniteCameraPreflight.revalidate(context, finitePlan);
            selection = new Selection(finitePlan.cameraId(), finitePlan.source().key(), null,
                    new Size(finitePlan.transformPlan().inputWidth(), finitePlan.transformPlan().inputHeight()),
                    finitePlan.sensorOrientationDegrees(),
                    FrameObservation.LensFacing.valueOf(finitePlan.source().facing().name()));
        } else {
            selection = selectCamera(manager, preferredFacing, preferredSource);
        }
        sensorOrientationDegrees = selection.sensorOrientationDegrees();
        lensFacing = selection.lensFacing();
        sourceKey = selection.sourceKey();
        targetZoomRatio = selection.targetZoomRatio();

        TextureView currentPreview = preview;
        if (currentPreview != null) {
            SurfaceTexture texture = currentPreview.getSurfaceTexture();
            if (texture == null) {
                fail("Preview surface is unavailable");
                return;
            }
            texture.setDefaultBufferSize(selection.size().getWidth(), selection.size().getHeight());
            Surface createdPreview = new Surface(texture);
            synchronized (lock) {
                if (closed) {
                    createdPreview.release();
                    return;
                }
                previewSurface = createdPreview;
            }
        }
        synchronized (lock) {
            if (closed) return;
            captureSize = selection.size();
        }
        AvcEncoderConfig encoderConfig =
                finitePlan == null
                        ? configForSize(selection.size(), qualityPreset, sensorOrientationDegrees)
                        : finitePlan.encoderConfig();
        SecureAvcSender secure = null;
        AvcSender createdSender;
        if (secureBinding != null) {
            if (routeBinding == null || !routeBinding.isLive() || !secureBinding.ready()) {
                throw new IllegalStateException("secure capture requires a live Ready generation");
            }
            secure = new SecureAvcSender(secureBinding.transport(), secureBinding.generation(),
                    routeBinding.streamKey(), encoderConfig, routeBinding.expiresAtMs(),
                    listener::onTransportStatus, finitePlan != null);
            SecureAvcSender selected = secure;
            createdSender = new AvcSender() {
                public void start() { selected.start(); }
                public void setCodecConfig(io.capyio.camera.contract.AvcCodecConfig value) {
                    selected.setCodecConfig(value);
                }
                public void offer(EncodedAvcAccessUnit value) { selected.offer(value); }
                public void close() { selected.close(); }
            };
        } else {
            createdSender = new LoopbackAvcSender(Objects.requireNonNull(transportEndpoint),
                    encoderConfig, listener::onTransportStatus, routeBinding);
        }
        createdSender.start();
        MediaCodecSurfaceEncoder createdEncoder = new MediaCodecSurfaceEncoder(encoderConfig, secure, finitePlan);
        FiniteCameraGpuTransform createdTransform = null;
        try {
            if (secureBinding != null && !secureBinding.ready()) {
                throw new IllegalStateException("secure generation cancelled before encoder start");
            }
            createdEncoder.start();
            if (finitePlan != null) {
                Handler handler;
                synchronized (lock) { handler = cameraHandler; }
                createdTransform = new FiniteCameraGpuTransform(createdEncoder.inputSurface(), finitePlan,
                        secureBinding, routeBinding, Objects.requireNonNull(handler), this::fail);
            }
        } catch (IOException | RuntimeException error) {
            createdEncoder.close();
            createdSender.close();
            throw error;
        }
        synchronized (lock) {
            // Initialization and resource teardown share the camera owner. A concurrent Stop
            // marks closed immediately, then its queued cleanup takes these exact resources.
            encoder = createdEncoder;
            sender = createdSender;
            finiteTransform = createdTransform;
            if (closed) return;
        }

        if (createdTransform != null) {
            // The camera cannot open until its dedicated EGL/OES input is ready.
            // Even a zero-orientation finite camera never targets the encoder directly.
            createdTransform.start(() -> openCameraAfterPreparation(manager, selection, finitePlan));
        } else {
            openCameraAfterPreparation(manager, selection, null);
        }
    }

    private void openCameraAfterPreparation(CameraManager manager, Selection selection,
            FiniteCameraPreflight.Plan finitePlan) {
        synchronized (lock) { if (closed) return; }
        try {
            if (secureBinding != null
                    && (!secureBinding.ready() || routeBinding == null || !routeBinding.isLive())) {
                throw new IllegalStateException("secure generation cancelled before camera open");
            }
            if (finitePlan != null) {
                // Recheck original sensor/facing/input-size/transform/codec after EGL creation.
                // Never reselect, silently downgrade or restart an expired generation.
                FiniteCameraPreflight.revalidate(context, finitePlan);
            }
            synchronized (lock) { if (closed) return; }
            manager.openCamera(selection.cameraId(), executor(), deviceCallback);
        } catch (CameraAccessException | RuntimeException error) {
            fail("Unable to open prepared Camera2: " + error.getClass().getSimpleName());
        }
    }

    private final CameraDevice.StateCallback deviceCallback = new CameraDevice.StateCallback() {
        @Override
        public void onOpened(CameraDevice device) {
            if (secureBinding != null && !secureBinding.ready()) {
                device.close();
                fail("Secure generation cancelled before camera opened");
                return;
            }
            synchronized (lock) {
                if (closed) {
                    device.close();
                    return;
                }
                cameraDevice = device;
            }
            configureSession(device);
        }

        @Override
        public void onDisconnected(CameraDevice device) {
            device.close();
            fail("Camera2 device disconnected");
        }

        @Override
        public void onError(CameraDevice device, int error) {
            device.close();
            fail("Camera2 device error " + error);
        }
    };

    private void configureSession(CameraDevice device) {
        MediaCodecSurfaceEncoder currentEncoder;
        Surface display;
        FiniteCameraGpuTransform transform;
        synchronized (lock) {
            currentEncoder = encoder;
            transform = finiteTransform;
            display = previewSurface;
            if (closed || currentEncoder == null) {
                return;
            }
        }

        try {
            Surface encoded = captureInput(currentEncoder, transform);
            List<OutputConfiguration> outputs = new ArrayList<>(2);
            if (display != null) outputs.add(cameraOutput(display, transform != null));
            outputs.add(cameraOutput(encoded, transform != null));
            SessionConfiguration configuration = new SessionConfiguration(
                    SessionConfiguration.SESSION_REGULAR,
                    outputs,
                    executor(),
                    sessionCallback);
            device.createCaptureSession(configuration);
        } catch (CameraAccessException | RuntimeException error) {
            fail("Unable to configure Camera2: " + error.getClass().getSimpleName());
        }
    }

    private Surface captureInput(MediaCodecSurfaceEncoder currentEncoder,
            FiniteCameraGpuTransform transform) {
        if (secureBinding != null && secureBinding.finitePlan() != null) {
            if (transform == null) throw new IllegalStateException("finite camera transform disappeared");
            return transform.inputSurface();
        }
        if (transform != null) throw new IllegalStateException("legacy camera unexpectedly owns finite transform");
        return currentEncoder.inputSurface();
    }

    private static OutputConfiguration cameraOutput(Surface surface, boolean finite) {
        OutputConfiguration output = new OutputConfiguration(surface);
        if (finite) {
            if (Build.VERSION.SDK_INT < 33) {
                throw new IllegalStateException("explicit unmirrored Camera2 output requires API 33");
            }
            // AUTO includes front-camera reflection in the producer matrix. Never use AUTO
            // for finite capture; getTransformMatrix already includes the sensor rotation.
            output.setMirrorMode(OutputConfiguration.MIRROR_MODE_NONE);
            output.setTimestampBase(OutputConfiguration.TIMESTAMP_BASE_SENSOR);
        }
        return output;
    }

    private final CameraCaptureSession.StateCallback sessionCallback =
            new CameraCaptureSession.StateCallback() {
                @Override
                public void onConfigured(CameraCaptureSession session) {
                    CameraDevice device;
                    MediaCodecSurfaceEncoder currentEncoder;
                    FiniteCameraGpuTransform transform;
                    Surface display;
                    Size size;
                    synchronized (lock) {
                        if (closed) {
                            session.close();
                            return;
                        }
                        captureSession = session;
                        device = cameraDevice;
                        currentEncoder = encoder;
                        transform = finiteTransform;
                        display = previewSurface;
                        size = captureSize;
                    }
                    if (device == null || currentEncoder == null || size == null) {
                        fail("Camera2 resources disappeared during configuration");
                        return;
                    }

                    try {
                        if (secureBinding != null && !secureBinding.ready()) {
                            session.close();
                            fail("Secure generation cancelled before capture start");
                            return;
                        }
                        Surface encoded = captureInput(currentEncoder, transform);
                        CaptureRequest.Builder request =
                                device.createCaptureRequest(CameraDevice.TEMPLATE_PREVIEW);
                        if (display != null) {
                            request.addTarget(display);
                        }
                        request.addTarget(encoded);
                        if (secureBinding != null && secureBinding.finitePlan() != null) {
                            request.set(CaptureRequest.CONTROL_AE_MODE, CaptureRequest.CONTROL_AE_MODE_ON);
                            request.set(CaptureRequest.CONTROL_AE_TARGET_FPS_RANGE, new Range<>(30, 30));
                            if (Build.VERSION.SDK_INT < 33 || transform == null) {
                                throw new IllegalStateException("finite pixel normalization is unavailable");
                            }
                            request.set(CaptureRequest.SCALER_ROTATE_AND_CROP,
                                    CaptureRequest.SCALER_ROTATE_AND_CROP_NONE);
                        }
                        Float selectedZoomRatio = targetZoomRatio;
                        if (selectedZoomRatio != null && Build.VERSION.SDK_INT >= 30) {
                            request.set(CaptureRequest.CONTROL_ZOOM_RATIO, selectedZoomRatio);
                        }
                        session.setRepeatingRequest(request.build(), captureCallback, cameraHandler);
                        synchronized (lock) {
                            lastEncodedProgressMillis = SystemClock.elapsedRealtime();
                        }
                        scheduleProgressWatchdog();
                        listener.onStarted(size, lensFacing, sourceKey);
                    } catch (CameraAccessException | RuntimeException error) {
                        fail("Unable to start Camera2 stream: " + error.getClass().getSimpleName());
                    }
                }

                @Override
                public void onConfigureFailed(CameraCaptureSession session) {
                    session.close();
                    fail("Camera2 session configuration failed");
                }
            };

    private final CameraCaptureSession.CaptureCallback captureCallback =
            new CameraCaptureSession.CaptureCallback() {
                @Override
                public void onCaptureCompleted(
                        CameraCaptureSession session,
                        CaptureRequest request,
                        TotalCaptureResult result) {
                    Long timestamp = result.get(CaptureResult.SENSOR_TIMESTAMP);
                    Size size;
                    synchronized (lock) {
                        if (closed) return;
                        size = captureSize;
                    }
                    if (secureBinding != null && secureBinding.finitePlan() != null) {
                        // CaptureResult and texture notifications are not atomically paired here.
                        // This is an additional fail-closed check after metadata arrives. The
                        // latched producer matrix is independently checked before every GPU draw.
                        Integer rotateAndCrop = Build.VERSION.SDK_INT >= 31
                                ? result.get(CaptureResult.SCALER_ROTATE_AND_CROP) : null;
                        if (rotateAndCrop == null || rotateAndCrop != CaptureResult.SCALER_ROTATE_AND_CROP_NONE
                                || !secureBinding.ready() || routeBinding == null || !routeBinding.isLive()) {
                            fail("Finite camera actual transform or original generation is invalid");
                            return;
                        }
                    }
                    if (timestamp == null || timestamp < 0 || size == null) {
                        fail("Camera2 result is missing a valid sensor timestamp or size");
                        return;
                    }
                    listener.onFrame(new FrameObservation(
                            size.getWidth(),
                            size.getHeight(),
                            timestamp,
                            ++sequence,
                            sensorOrientationDegrees,
                            lensFacing));
                    drainEncoder();
                }
            };

    private void drainEncoder() {
        MediaCodecSurfaceEncoder current;
        AvcSender currentSender;
        synchronized (lock) {
            current = encoder;
            currentSender = sender;
        }
        if (current == null || currentSender == null) {
            return;
        }
        current.takeOutputConfig().ifPresent(currentSender::setCodecConfig);
        if (secureBinding != null) {
            long count = current.encodedAccessUnits();
            if (count != lastSecureEncoded) {
                lastSecureEncoded = count;
                synchronized (lock) { lastEncodedProgressMillis = SystemClock.elapsedRealtime(); }
                listener.onSecureEncoded(count);
            }
            if (!secureBinding.ready() || routeBinding == null || !routeBinding.isLive()) {
                fail("Secure camera generation stopped or expired");
                return;
            }
        }
        MediaCodecSurfaceEncoder.RuntimeInfo runtimeInfo = current.runtimeInfo();
        if (runtimeInfo.actualLatencyFrames() != lastReportedLatencyFrames) {
            lastReportedLatencyFrames = runtimeInfo.actualLatencyFrames();
            listener.onEncoderStatus(
                    runtimeInfo.codecName(),
                    runtimeInfo.requestedLatencyFrames(),
                    runtimeInfo.actualLatencyFrames());
        }
        for (int index = 0; index < AvcEncoderConfig.MAX_QUEUE_CAPACITY; index++) {
            java.util.Optional<EncodedAvcAccessUnit> unit = current.pollAccessUnit();
            if (unit.isEmpty()) {
                break;
            }
            EncodedAvcAccessUnit accessUnit = unit.orElseThrow();
            currentSender.offer(accessUnit);
            synchronized (lock) {
                lastEncodedProgressMillis = SystemClock.elapsedRealtime();
            }
            listener.onEncodedAccessUnit(accessUnit, current.droppedAccessUnits());
        }
        current.takeLastError().ifPresent(this::fail);
    }

    private void scheduleProgressWatchdog() {
        Handler handler;
        synchronized (lock) {
            if (closed || progressFailureReported || lastEncodedProgressMillis < 0) {
                return;
            }
            handler = cameraHandler;
        }
        if (handler == null
                || !handler.postDelayed(
                        progressWatchdog,
                        CameraProgressWatchdog.CHECK_INTERVAL_MILLIS)) {
            fail("Camera progress watchdog could not be scheduled");
        }
    }

    private void checkProgressWatchdog() {
        long lastProgress;
        synchronized (lock) {
            if (closed || progressFailureReported) {
                return;
            }
            lastProgress = lastEncodedProgressMillis;
        }
        long now = SystemClock.elapsedRealtime();
        if (lastProgress >= 0 && CameraProgressWatchdog.isExpired(now, lastProgress)) {
            synchronized (lock) {
                if (closed || progressFailureReported) {
                    return;
                }
                progressFailureReported = true;
            }
            fail("Camera stream stalled: no encoded progress for "
                    + CameraProgressWatchdog.STALL_TIMEOUT_MILLIS
                    + " ms");
            return;
        }
        scheduleProgressWatchdog();
    }

    private Executor executor() {
        Handler handler;
        synchronized (lock) {
            handler = cameraHandler;
        }
        if (handler == null) {
            throw new IllegalStateException("camera thread is unavailable");
        }
        return command -> {
            if (!handler.post(command)) {
                // Late device/session callbacks must still close their newly delivered handles.
                // Never discard onOpened after Stop just because its owner Looper has quit.
                synchronized (lock) {
                    if (!closed) fail("Camera2 callback thread stopped");
                }
                command.run();
            }
        };
    }

    private static Selection selectCamera(
            CameraManager manager,
            FrameObservation.LensFacing preferredFacing,
            CameraSourceSelection preferredSource) throws CameraAccessException {
        if (preferredSource != null) {
            CameraCharacteristics logical =
                    manager.getCameraCharacteristics(preferredSource.cameraId());
            return selectionFor(
                    preferredSource.cameraId(),
                    preferredSource.targetZoomRatioMilli(),
                    logical,
                    logical);
        }
        List<Selection> candidates = new ArrayList<>();
        for (String cameraId : manager.getCameraIdList()) {
            CameraCharacteristics characteristics = manager.getCameraCharacteristics(cameraId);
            try {
                candidates.add(selectionFor(
                        cameraId, null, characteristics, characteristics));
            } catch (IllegalStateException unavailable) {
                // A directly openable metadata entry without shared outputs is not a stream candidate.
            }
        }
        if (candidates.isEmpty()) {
            throw new IllegalStateException("no shared Camera2 preview/MediaCodec size is available");
        }
        List<FrameObservation.LensFacing> availableFacings =
                new ArrayList<>(candidates.size());
        for (Selection candidate : candidates) {
            availableFacings.add(candidate.lensFacing());
        }
        FrameObservation.LensFacing selectedFacing =
                CameraFacingPolicy.select(preferredFacing, availableFacings);
        for (Selection candidate : candidates) {
            if (candidate.lensFacing() == selectedFacing) {
                return candidate;
            }
        }
        throw new IllegalStateException("selected camera facing disappeared");
    }

    private static Selection selectionFor(
            String cameraId,
            Integer targetZoomRatioMilli,
            CameraCharacteristics logicalCharacteristics,
            CameraCharacteristics streamCharacteristics) {
        android.hardware.camera2.params.StreamConfigurationMap map =
                streamCharacteristics.get(CameraCharacteristics.SCALER_STREAM_CONFIGURATION_MAP);
        if (map == null) {
            throw new IllegalStateException("selected camera has no stream configuration map");
        }
        Size[] codecSizes = map.getOutputSizes(MediaCodec.class);
        Size[] previewSizes = map.getOutputSizes(SurfaceTexture.class);
        if (codecSizes == null
                || codecSizes.length == 0
                || previewSizes == null
                || previewSizes.length == 0) {
            throw new IllegalStateException("selected camera has no shared output sizes");
        }
        Integer androidFacing = logicalCharacteristics.get(CameraCharacteristics.LENS_FACING);
        Integer orientation = logicalCharacteristics.get(CameraCharacteristics.SENSOR_ORIENTATION);
        String key = targetZoomRatioMilli == null
                ? cameraId + "@auto"
                : cameraId + "@" + String.format(
                        java.util.Locale.ROOT,
                        "%.3fx",
                        targetZoomRatioMilli / 1_000.0f);
        return new Selection(
                cameraId,
                key,
                targetZoomRatioMilli == null ? null : targetZoomRatioMilli / 1_000.0f,
                chooseCommonSize(codecSizes, previewSizes),
                normalizeOrientation(orientation),
                mapFacing(androidFacing));
    }

    private static Size chooseCommonSize(Size[] codecSizes, Size[] previewSizes) {
        Comparator<Size> byArea = Comparator.comparingLong(
                size -> (long) size.getWidth() * size.getHeight());
        Size bounded = null;
        Size smallest = null;
        for (Size size : codecSizes) {
            if ((size.getWidth() & 1) != 0 || (size.getHeight() & 1) != 0) {
                continue;
            }
            if (!containsSize(previewSizes, size)) {
                continue;
            }
            if (smallest == null || byArea.compare(size, smallest) < 0) {
                smallest = size;
            }
            if (size.getWidth() <= MAX_WIDTH
                    && size.getHeight() <= MAX_HEIGHT
                    && (bounded == null || byArea.compare(size, bounded) > 0)) {
                bounded = size;
            }
        }
        if (bounded != null) {
            return bounded;
        }
        if (smallest != null) {
            return smallest;
        }
        throw new IllegalStateException("no even shared Camera2/MediaCodec size is available");
    }

    private static boolean containsSize(Size[] sizes, Size expected) {
        for (Size size : sizes) {
            if (size.equals(expected)) {
                return true;
            }
        }
        return false;
    }

    private static AvcEncoderConfig configForSize(
            Size size, AvcQualityPreset qualityPreset, int clockwiseRotationDegrees) {
        return new AvcEncoderConfig(
                size.getWidth(),
                size.getHeight(),
                30,
                qualityPreset.bitrateForDimensions(size.getWidth(), size.getHeight()),
                clockwiseRotationDegrees,
                1,
                2);
    }

    private static int normalizeOrientation(Integer orientation) {
        if (orientation == null) {
            return 0;
        }
        return switch (orientation) {
            case 0, 90, 180, 270 -> orientation;
            default -> 0;
        };
    }

    private static FrameObservation.LensFacing mapFacing(Integer facing) {
        if (facing == null) {
            return FrameObservation.LensFacing.UNKNOWN;
        }
        return switch (facing) {
            case CameraCharacteristics.LENS_FACING_FRONT -> FrameObservation.LensFacing.FRONT;
            case CameraCharacteristics.LENS_FACING_BACK -> FrameObservation.LensFacing.BACK;
            case CameraCharacteristics.LENS_FACING_EXTERNAL -> FrameObservation.LensFacing.EXTERNAL;
            default -> FrameObservation.LensFacing.UNKNOWN;
        };
    }

    private void fail(String reason) {
        synchronized (lock) {
            if (closed) {
                return;
            }
        }
        // Service cleanup is posted to its main handler; stop secure writes synchronously first.
        if (secureBinding != null) secureBinding.stop();
        listener.onError(reason);
    }

    @Override
    public void close() {
        Handler handler;
        FiniteCameraGpuTransform transform;
        synchronized (lock) {
            if (closed) return;
            closed = true;
            handler = cameraHandler;
            transform = finiteTransform;
        }
        // Admission is cancelled synchronously even if a platform driver delays cleanup.
        if (transform != null) transform.requestStop();
        try {
            if (secureBinding != null) secureBinding.stop();
        } finally {
            if (handler == null || !handler.post(this::closeResourcesOnCameraOwner)) {
                closeResourcesOnCameraOwner();
            }
        }
    }

    private void closeResourcesOnCameraOwner() {
        CameraCaptureSession session;
        CameraDevice device;
        MediaCodecSurfaceEncoder currentEncoder;
        FiniteCameraGpuTransform transform;
        AvcSender currentSender;
        Surface display;
        HandlerThread thread;
        Handler handler;
        synchronized (lock) {
            session = captureSession;
            captureSession = null;
            device = cameraDevice;
            cameraDevice = null;
            currentEncoder = encoder;
            encoder = null;
            transform = finiteTransform;
            finiteTransform = null;
            currentSender = sender;
            sender = null;
            display = previewSurface;
            previewSurface = null;
            captureSize = null;
            thread = cameraThread;
            handler = cameraHandler;
            cameraThread = null;
            cameraHandler = null;
        }

        Runnable finish = () -> {
            try {
                if (currentEncoder != null) currentEncoder.close();
            } finally {
                try {
                    if (display != null) display.release();
                } finally {
                    if (thread != null) thread.quitSafely();
                    notifyClosedOnce();
                }
            }
        };
        boolean teardownFailed = false;
        try {
            // Stop native sends before any potentially blocking platform teardown operation.
            if (secureBinding != null) {
                try { secureBinding.stop(); }
                catch (RuntimeException error) { teardownFailed = true; }
            }
            if (handler != null) handler.removeCallbacks(progressWatchdog);
            if (session != null) {
                try { session.stopRepeating(); }
                catch (CameraAccessException | RuntimeException error) { teardownFailed = true; }
                try { session.abortCaptures(); }
                catch (CameraAccessException | RuntimeException error) { teardownFailed = true; }
                try { session.close(); }
                catch (RuntimeException error) { teardownFailed = true; }
            }
            if (device != null) {
                try { device.close(); }
                catch (RuntimeException error) { teardownFailed = true; }
            }
        } finally {
            try {
                if (currentSender != null) currentSender.close();
            } finally {
                if (transform != null) {
                    // No timeout can safely release an encoder still in use by a stuck EGL driver.
                    // Cooperative cleanup is bounded in queued work, not driver wall-clock time.
                    transform.closeAfterCameraStopped(finish);
                } else {
                    finish.run();
                }
            }
        }
        if (teardownFailed) listener.onError("Camera2 producer teardown reported a platform failure");
    }

    private void notifyClosedOnce() {
        synchronized (lock) {
            if (closeNotified) {
                return;
            }
            closeNotified = true;
        }
        listener.onClosed();
    }

    private record Selection(
            String cameraId,
            String sourceKey,
            Float targetZoomRatio,
            Size size,
            int sensorOrientationDegrees,
            FrameObservation.LensFacing lensFacing) {}
}
