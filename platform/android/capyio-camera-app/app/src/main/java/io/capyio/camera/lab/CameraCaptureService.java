package io.capyio.camera.lab;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.app.Service;
import android.content.Context;
import android.content.Intent;
import android.content.pm.ServiceInfo;
import android.graphics.drawable.Icon;
import android.os.Build;
import android.os.Handler;
import android.os.IBinder;
import android.os.Looper;
import android.util.Size;
import io.capyio.camera.contract.AvcQualityPreset;
import io.capyio.camera.contract.CameraInventory;
import io.capyio.camera.contract.CameraSourceSelection;
import io.capyio.camera.contract.CameraTransportEndpoint;
import io.capyio.camera.contract.EncodedAvcAccessUnit;
import io.capyio.camera.contract.FrameObservation;

/** Foreground owner of one encoder-only Camera2 session. */
public final class CameraCaptureService extends Service {
    enum Phase {
        STOPPED,
        STARTING,
        STREAMING,
        ERROR
    }

    record Snapshot(
            Phase phase,
            boolean secureTransport,
            long generation,
            int width,
            int height,
            FrameObservation.LensFacing facing,
            String sourceKey,
            AvcQualityPreset quality,
            long capturedFrames,
            long encodedAccessUnits,
            long droppedAccessUnits,
            String transportStatus,
            long sentAccessUnits,
            long exportDroppedAccessUnits,
            String encoderStatus,
            String error) {}

    private record Config(
            FrameObservation.LensFacing facing,
            CameraSourceSelection source,
            AvcQualityPreset quality,
            CameraTransportEndpoint endpoint, CameraRouteBinding route, SecureCameraBinding secure) {}

    private static final String ACTION_START =
            "io.capyio.camera.lab.action.START_CAPTURE";
    private static final String ACTION_STOP =
            "io.capyio.camera.lab.action.STOP_CAPTURE";
    static final String ACTION_STATE_CHANGED =
            "io.capyio.camera.lab.action.CAPTURE_STATE_CHANGED";
    private static final String EXTRA_FACING = "facing";
    private static final String EXTRA_QUALITY = "quality";
    private static final String EXTRA_TRANSPORT = "transport";
    private static final String EXTRA_SOURCE_ID = "source_id";
    private static final String EXTRA_SOURCE_FACING = "source_facing";
    private static final String EXTRA_SOURCE_ZOOM = "source_zoom";
    private static final String CHANNEL_ID = "capyio_camera_capture";
    private static final int NOTIFICATION_ID = 38173;

    private static volatile Snapshot latest = stoppedSnapshot();
    private record PendingSecure(long token, Config config) {}
    private static final java.util.concurrent.atomic.AtomicLong secureTokens =
            new java.util.concurrent.atomic.AtomicLong();
    private static final java.util.concurrent.atomic.AtomicReference<PendingSecure> pendingSecure =
            new java.util.concurrent.atomic.AtomicReference<>();

    static void startSecure(Context context, FrameObservation.LensFacing facing,
            CameraSourceSelection source, AvcQualityPreset quality,
            CameraRouteBinding route, SecureCameraBinding secure) {
        if (!secure.ready() || !route.isLive()) throw new IllegalStateException("secure generation not Ready");
        long token = secureTokens.incrementAndGet();
        PendingSecure pending = new PendingSecure(token,
                new Config(facing, source, quality, null, route, secure));
        PendingSecure previous = pendingSecure.getAndSet(pending);
        if (previous != null) previous.config().secure().stop();
        try {
            context.startForegroundService(new Intent(context, CameraCaptureService.class)
                    .setAction(ACTION_START).putExtra("secure_generation_token", token));
        } catch (RuntimeException failure) {
            pendingSecure.compareAndSet(pending, null);
            secure.stop();
            throw failure;
        }
    }


    private final Object lock = new Object();
    private final Handler mainHandler = new Handler(Looper.getMainLooper());
    private final Runnable announceState = this::announceLatestState;
    private Camera2Session session;
    private long generation;
    private Config activeConfig;
    private Phase phase = Phase.STOPPED;
    private int width;
    private int height;
    private FrameObservation.LensFacing activeFacing =
            FrameObservation.LensFacing.UNKNOWN;
    private String activeSource = "none";
    private long capturedFrames;
    private long encodedAccessUnits;
    private long droppedAccessUnits;
    private String transportStatus = "export stopped";
    private long sentAccessUnits;
    private long exportDroppedAccessUnits;
    private String encoderStatus = "pending";
    private String failure = "";

    static Snapshot snapshot() {
        return latest;
    }

    static void start(
            Context context,
            FrameObservation.LensFacing facing,
            CameraSourceSelection source,
            AvcQualityPreset quality,
            CameraTransportEndpoint endpoint) {
        start(context,facing,source,quality,endpoint,null);
    }

    static void start(Context context, FrameObservation.LensFacing facing,
            CameraSourceSelection source, AvcQualityPreset quality,
            CameraTransportEndpoint endpoint, CameraRouteBinding route) {
        Intent intent = new Intent(context, CameraCaptureService.class)
                .setAction(ACTION_START)
                .putExtra(EXTRA_FACING, facing.name())
                .putExtra(EXTRA_QUALITY, quality.name())
                .putExtra(
                        EXTRA_TRANSPORT,
                        endpoint.mode() == CameraTransportEndpoint.Mode.ADB_REVERSE
                                ? ""
                                : endpoint.host());
        if (route != null) {
            intent.putExtra("route_stream",route.streamId().toString());
            intent.putExtra("route_epoch",route.epoch());
            intent.putExtra("route_expiry",route.expiresAtMs());
        }
        if (source != null) {
            intent.putExtra(EXTRA_SOURCE_ID, source.cameraId());
            intent.putExtra(EXTRA_SOURCE_FACING, source.facing().name());
            if (source.targetZoomRatioMilli() != null) {
                intent.putExtra(EXTRA_SOURCE_ZOOM, source.targetZoomRatioMilli());
            }
        }
        context.startForegroundService(intent);
    }

    static void stop(Context context) {
        PendingSecure pending = pendingSecure.getAndSet(null);
        if (pending != null) pending.config().secure().stop();
        context.stopService(new Intent(context, CameraCaptureService.class));
    }

    @Override
    public void onCreate() {
        super.onCreate();
        NotificationManager notifications = getSystemService(NotificationManager.class);
        NotificationChannel channel = new NotificationChannel(
                CHANNEL_ID,
                getString(R.string.camera_notification_channel),
                NotificationManager.IMPORTANCE_LOW);
        channel.setDescription(getString(R.string.camera_notification_channel_description));
        notifications.createNotificationChannel(channel);
    }

    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        if (intent == null || !ACTION_START.equals(intent.getAction())) {
            stopCapture(false, "");
            return START_NOT_STICKY;
        }
        Config requested;
        try {
            requested = configFrom(intent);
        } catch (IllegalArgumentException | IllegalStateException rejected) {
            if (intent.hasExtra("secure_generation_token")) {
                // A late delivery from an obsolete in-process generation cannot stop a newer one.
                synchronized (lock) {
                    if (session == null) stopSelf(startId);
                }
                return START_NOT_STICKY;
            }
            stopCapture(true, "Invalid capture request: " + rejected.getClass().getSimpleName());
            return START_NOT_STICKY;
        }
        try {
            enterForeground(notification(getString(R.string.camera_notification_starting)));
        } catch (SecurityException | IllegalStateException error) {
            stopCapture(true, "Open CapyIO in the foreground and allow camera access, then retry sharing.");
            return START_NOT_STICKY;
        }
        try {
            startCapture(requested);
        } catch (IllegalArgumentException | IllegalStateException error) {
            stopCapture(true, "Invalid capture request: " + error.getClass().getSimpleName());
        }
        return START_NOT_STICKY;
    }

    @Override
    public void onDestroy() {
        boolean retainFailure;
        String reason;
        synchronized (lock) {
            retainFailure = phase == Phase.ERROR;
            reason = failure;
        }
        mainHandler.removeCallbacksAndMessages(null);
        stopCapture(retainFailure, reason);
        mainHandler.removeCallbacks(announceState);
        announceLatestState();
        super.onDestroy();
    }

    @Override
    public IBinder onBind(Intent intent) {
        return null;
    }

    private void startCapture(Config config) {
        Camera2Session previous;
        long currentGeneration;
        synchronized (lock) {
            generation++;
            currentGeneration = generation;
            previous = session;
            session = null;
            activeConfig = config;
            resetMetricsLocked();
            phase = Phase.STARTING;
            publishLocked();
        }
        if (previous != null) {
            previous.close();
        }

        Camera2Session created = new Camera2Session(
                this,
                config.facing(),
                config.source(),
                config.quality(),
                config.endpoint(),
                listenerFor(currentGeneration));
        created.bindRoute(config.route());
        created.bindSecure(config.secure());
        synchronized (lock) {
            if (generation != currentGeneration || phase != Phase.STARTING) {
                created.close();
                return;
            }
            session = created;
        }
        created.start();
    }

    private Camera2Session.Listener listenerFor(long listenerGeneration) {
        return new Camera2Session.Listener() {
            @Override
            public void onStarted(
                    Size size,
                    FrameObservation.LensFacing facing,
                    String sourceKey) {
                synchronized (lock) {
                    if (generation != listenerGeneration) {
                        return;
                    }
                    width = size.getWidth();
                    height = size.getHeight();
                    activeFacing = facing;
                    activeSource = sourceKey;
                    phase = Phase.STREAMING;
                    publishLocked();
                }
                updateNotification(getString(R.string.camera_notification_streaming));
            }

            @Override
            public void onFrame(FrameObservation observation) {
                if (observation.sequence() == 1 || observation.sequence() % 30 == 0) {
                    synchronized (lock) {
                        if (generation == listenerGeneration) {
                            capturedFrames = observation.sequence();
                            publishLocked();
                        }
                    }
                }
            }

            @Override
            public void onEncodedAccessUnit(EncodedAvcAccessUnit unit, long dropped) {
                if (unit.sequence() == 1 || unit.keyFrame() || unit.sequence() % 30 == 0) {
                    synchronized (lock) {
                        if (generation == listenerGeneration) {
                            encodedAccessUnits = unit.sequence();
                            droppedAccessUnits = dropped;
                            publishLocked();
                        }
                    }
                }
            }

            @Override
            public void onSecureEncoded(long count) {
                synchronized (lock) {
                    if (generation == listenerGeneration) {
                        encodedAccessUnits = count;
                        publishLocked();
                    }
                }
            }

            @Override
            public void onEncoderStatus(
                    String codecName,
                    int requestedLatencyFrames,
                    int actualLatencyFrames) {
                synchronized (lock) {
                    if (generation == listenerGeneration) {
                        encoderStatus = codecName
                                + "; latency frames requested/actual: "
                                + requestedLatencyFrames
                                + "/"
                                + (actualLatencyFrames >= 0
                                        ? Integer.toString(actualLatencyFrames)
                                        : "unreported");
                        publishLocked();
                    }
                }
            }

            @Override
            public void onTransportStatus(String state, long sent, long dropped) {
                synchronized (lock) {
                    if (generation == listenerGeneration) {
                        transportStatus = state;
                        sentAccessUnits = sent;
                        exportDroppedAccessUnits = dropped;
                        publishLocked();
                    }
                }
            }

            @Override
            public void onError(String reason) {
                mainHandler.post(() -> failCapture(listenerGeneration, reason));
            }

            @Override
            public void onClosed() {
                mainHandler.post(() -> sessionClosed(listenerGeneration));
            }
        };
    }

    private void failCapture(long listenerGeneration, String reason) {
        synchronized (lock) {
            if (generation != listenerGeneration) {
                return;
            }
        }
        stopCapture(true, reason);
    }

    private void sessionClosed(long listenerGeneration) {
        synchronized (lock) {
            if (generation != listenerGeneration || session == null) {
                return;
            }
        }
        stopCapture(true, "Camera session closed unexpectedly");
    }

    private void stopCapture(boolean retainFailure, String reason) {
        Camera2Session current;
        synchronized (lock) {
            generation++;
            current = session;
            session = null;
            if (retainFailure) {
                phase = Phase.ERROR;
                failure = reason;
            } else {
                phase = Phase.STOPPED;
                activeConfig = null;
                resetMetricsLocked();
            }
            publishLocked();
        }
        if (current != null) {
            current.close();
        }
        stopForeground(STOP_FOREGROUND_REMOVE);
        stopSelf();
    }

    private void resetMetricsLocked() {
        width = 0;
        height = 0;
        activeFacing = FrameObservation.LensFacing.UNKNOWN;
        activeSource = "none";
        capturedFrames = 0;
        encodedAccessUnits = 0;
        droppedAccessUnits = 0;
        transportStatus = "export stopped";
        sentAccessUnits = 0;
        exportDroppedAccessUnits = 0;
        encoderStatus = "pending";
        failure = "";
    }

    private void publishLocked() {
        AvcQualityPreset quality = activeConfig == null
                ? AvcQualityPreset.BALANCED
                : activeConfig.quality();
        latest = new Snapshot(
                phase,
                activeConfig != null && activeConfig.secure() != null,
                generation,
                width,
                height,
                activeFacing,
                activeSource,
                quality,
                capturedFrames,
                encodedAccessUnits,
                droppedAccessUnits,
                transportStatus,
                sentAccessUnits,
                exportDroppedAccessUnits,
                encoderStatus,
                failure);
        mainHandler.removeCallbacks(announceState);
        mainHandler.post(announceState);
    }

    private void announceLatestState() {
        sendBroadcast(new Intent(ACTION_STATE_CHANGED).setPackage(getPackageName()));
    }

    private void updateNotification(String text) {
        enterForeground(notification(text));
    }

    private void enterForeground(Notification notification) {
        if (Build.VERSION.SDK_INT >= 30) {
            startForeground(
                    NOTIFICATION_ID,
                    notification,
                    ServiceInfo.FOREGROUND_SERVICE_TYPE_CAMERA);
        } else {
            startForeground(NOTIFICATION_ID, notification);
        }
    }

    private Notification notification(String text) {
        Intent open = new Intent(this, MainActivity.class)
                .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP);
        Intent packageLaunch = getPackageManager().getLaunchIntentForPackage(getPackageName());
        if (packageLaunch != null) {
            open = packageLaunch.addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP);
        }
        PendingIntent contentIntent = PendingIntent.getActivity(
                this,
                0,
                open,
                PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
        Intent stop = new Intent(this, CameraCaptureService.class).setAction(ACTION_STOP);
        PendingIntent stopIntent = PendingIntent.getService(
                this,
                1,
                stop,
                PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
        return new Notification.Builder(this, CHANNEL_ID)
                .setSmallIcon(R.drawable.ic_camera)
                .setContentTitle(getString(R.string.app_name))
                .setContentText(text)
                .setOngoing(true)
                .setOnlyAlertOnce(true)
                .setContentIntent(contentIntent)
                .addAction(new Notification.Action.Builder(
                        Icon.createWithResource(this, R.drawable.ic_camera),
                        getString(R.string.stop_camera),
                        stopIntent).build())
                .build();
    }

    private static Config configFrom(Intent intent) {
        if (intent.hasExtra("secure_generation_token")) {
            long token = intent.getLongExtra("secure_generation_token", 0);
            PendingSecure pending = pendingSecure.get();
            if (pending == null || pending.token() != token
                    || !pendingSecure.compareAndSet(pending, null)
                    || !pending.config().secure().ready() || !pending.config().route().isLive()) {
                throw new IllegalStateException("stale secure capture request");
            }
            return pending.config();
        }
        FrameObservation.LensFacing facing = FrameObservation.LensFacing.valueOf(
                requiredExtra(intent, EXTRA_FACING));
        AvcQualityPreset quality = AvcQualityPreset.valueOf(
                requiredExtra(intent, EXTRA_QUALITY));
        CameraTransportEndpoint endpoint = CameraTransportEndpoint.fromUserInput(
                requiredExtra(intent, EXTRA_TRANSPORT));
        String sourceId = intent.getStringExtra(EXTRA_SOURCE_ID);
        CameraSourceSelection source = null;
        if (sourceId != null) {
            CameraInventory.LensFacing sourceFacing = CameraInventory.LensFacing.valueOf(
                    requiredExtra(intent, EXTRA_SOURCE_FACING));
            Integer zoom = intent.hasExtra(EXTRA_SOURCE_ZOOM)
                    ? intent.getIntExtra(EXTRA_SOURCE_ZOOM, 0)
                    : null;
            source = new CameraSourceSelection(sourceId, sourceFacing, zoom);
        }
        CameraRouteBinding route = intent.hasExtra("route_stream") ? new CameraRouteBinding(
                requiredExtra(intent,"route_stream"),intent.getLongExtra("route_epoch",0),intent.getLongExtra("route_expiry",0)) : null;
        return new Config(facing, source, quality, endpoint, route, null);
    }

    private static String requiredExtra(Intent intent, String name) {
        String value = intent.getStringExtra(name);
        if (value == null) {
            throw new IllegalArgumentException("missing " + name);
        }
        return value;
    }

    private static Snapshot stoppedSnapshot() {
        return new Snapshot(
                Phase.STOPPED,
                false,
                0,
                0,
                0,
                FrameObservation.LensFacing.UNKNOWN,
                "none",
                AvcQualityPreset.BALANCED,
                0,
                0,
                0,
                "export stopped",
                0,
                0,
                "pending",
                "");
    }
}
