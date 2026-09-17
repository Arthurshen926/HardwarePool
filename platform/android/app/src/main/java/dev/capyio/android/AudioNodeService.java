package dev.capyio.android;

import android.Manifest;
import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.app.Service;
import android.content.Context;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.content.pm.ServiceInfo;
import android.hardware.SensorManager;
import android.os.Binder;
import android.os.Build;
import android.os.Handler;
import android.os.IBinder;
import android.os.Looper;
import android.os.Vibrator;

import dev.capyio.android.contract.ActualAudioFormat;
import dev.capyio.android.contract.AudioCapabilityKind;
import dev.capyio.android.contract.AudioCapabilitySnapshot;
import dev.capyio.android.contract.AudioCapabilityState;
import dev.capyio.android.contract.AudioNodeController;
import dev.capyio.android.contract.AudioNodeSnapshot;
import dev.capyio.android.contract.CapabilityCatalogSnapshot;
import dev.capyio.android.contract.CapabilityLifecycleController;
import dev.capyio.android.contract.CapabilityLifecycleSnapshot;
import dev.capyio.android.contract.CapabilityLifecycleState;
import dev.capyio.android.contract.CapabilityModuleRegistry;
import dev.capyio.android.contract.GamepadControlsSnapshot;
import dev.capyio.android.contract.GamepadNodeController;
import dev.capyio.android.contract.HardwarePresence;
import dev.capyio.android.contract.TransitionToken;
import dev.capyio.android.contract.TouchpadContactSnapshot;
import dev.capyio.android.contract.TouchpadControlsSnapshot;
import dev.capyio.android.contract.TouchpadNodeController;
import dev.capyio.android.lan.NativeLanSpeakerSessionConfig;
import dev.capyio.android.lan.NativeLanMicrophoneSessionConfig;
import io.capyio.camera.lab.UnifiedCameraNodeFacade;

/** Owns Android Node hardware resources independently of the Activity implementation. */
public final class AudioNodeService extends Service {
    private static final String ACTION_START_MICROPHONE =
            "dev.capyio.android.action.START_MICROPHONE";
    private static final String ACTION_STOP_MICROPHONE =
            "dev.capyio.android.action.STOP_MICROPHONE";
    private static final String ACTION_START_SPEAKER =
            "dev.capyio.android.action.START_SPEAKER";
    private static final String ACTION_STOP_SPEAKER =
            "dev.capyio.android.action.STOP_SPEAKER";
    private static final String ACTION_START_IMU =
            "dev.capyio.android.action.START_IMU";
    private static final String ACTION_STOP_IMU =
            "dev.capyio.android.action.STOP_IMU";
    private static final String ACTION_START_GAMEPAD =
            "dev.capyio.android.action.START_GAMEPAD";
    private static final String ACTION_STOP_GAMEPAD =
            "dev.capyio.android.action.STOP_GAMEPAD";
    private static final String ACTION_START_TOUCHPAD =
            "dev.capyio.android.action.START_TOUCHPAD";
    private static final String ACTION_STOP_TOUCHPAD =
            "dev.capyio.android.action.STOP_TOUCHPAD";
    private static final String ACTION_START_HAPTICS =
            "dev.capyio.android.action.START_HAPTICS";
    private static final String ACTION_STOP_HAPTICS =
            "dev.capyio.android.action.STOP_HAPTICS";
    private static final String ACTION_STOP_ALL =
            "dev.capyio.android.action.STOP_ALL";

    private static final String NOTIFICATION_CHANNEL_ID = "capyio-audio-node-v1";
    private static final int NOTIFICATION_ID = 4101;
    private static final String PROBLEM_FOREGROUND = "CAPY.ANDROID.FOREGROUND_DENIED";
    private static final String PROBLEM_PERMISSION = "CAPY.ANDROID.MIC_PERMISSION_DENIED";

    private final NodeBinder binder = new NodeBinder();
    private final Handler mainHandler = new Handler(Looper.getMainLooper());
    private final AudioNodeController controller = new AudioNodeController();
    private final CapabilityLifecycleController imuController =
            new CapabilityLifecycleController();
    private final GamepadNodeController gamepadController = new GamepadNodeController();
    private final TouchpadNodeController touchpadController = new TouchpadNodeController();
    private final CapabilityLifecycleController hapticsController =
            new CapabilityLifecycleController();
    private final AndroidSharingSelection sharingSelection = new AndroidSharingSelection();

    private CapabilityModuleRegistry capabilityRegistry;
    private MicrophoneSourceAdapter microphoneAdapter;
    private SpeakerSinkAdapter speakerAdapter;
    private ImuSourceAdapter imuAdapter;
    private ControllerRouteAdapter controllerRouteAdapter;
    private TouchpadRouteAdapter touchpadAdapter;
    private AndroidHapticsSinkAdapter hapticsAdapter;
    private AndroidNativeCatalogRuntime catalogRuntime;
    private String nativeCatalogJson;
    private String nodeId;
    private HardwarePresence touchSurfacePresence;
    private boolean foreground;
    private boolean destroying;
    private Runnable stateListener;

    public static void startMicrophone(Context context) {
        startForegroundAction(context, ACTION_START_MICROPHONE);
    }

    public static void startSpeaker(Context context) {
        startForegroundAction(context, ACTION_START_SPEAKER);
    }

    public static void stopMicrophone(Context context) {
        context.startService(new Intent(context, AudioNodeService.class)
                .setAction(ACTION_STOP_MICROPHONE));
    }

    public static void stopSpeaker(Context context) {
        context.startService(new Intent(context, AudioNodeService.class)
                .setAction(ACTION_STOP_SPEAKER));
    }

    public static void startImu(Context context) {
        startForegroundAction(context, ACTION_START_IMU);
    }

    public static void stopImu(Context context) {
        startServiceAction(context, ACTION_STOP_IMU);
    }

    public static void startGamepad(Context context) {
        startForegroundAction(context, ACTION_START_GAMEPAD);
    }

    public static void stopGamepad(Context context) {
        startServiceAction(context, ACTION_STOP_GAMEPAD);
    }

    public static void startTouchpad(Context context) {
        startForegroundAction(context, ACTION_START_TOUCHPAD);
    }

    public static void stopTouchpad(Context context) {
        startServiceAction(context, ACTION_STOP_TOUCHPAD);
    }

    public static void startHaptics(Context context) {
        startForegroundAction(context, ACTION_START_HAPTICS);
    }

    public static void stopHaptics(Context context) {
        startServiceAction(context, ACTION_STOP_HAPTICS);
    }

    private static void startServiceAction(Context context, String action) {
        context.startService(new Intent(context, AudioNodeService.class).setAction(action));
    }

    private static void startForegroundAction(Context context, String action) {
        Intent intent = new Intent(context, AudioNodeService.class).setAction(action);
        context.startForegroundService(intent);
    }

    @Override
    public void onCreate() {
        super.onCreate();
        nodeId = NodeIdentityStore.loadOrCreate(this);
        AndroidHardwareInventory inventory = AndroidHardwareInventory.probe(this);
        touchSurfacePresence = inventory.touchSurfacePresence();
        capabilityRegistry = AndroidCapabilityModules.createRegistry(
                controller,
                imuController,
                gamepadController,
                touchpadController,
                hapticsController,
                () -> checkSelfPermission(Manifest.permission.RECORD_AUDIO)
                        == PackageManager.PERMISSION_GRANTED,
                () -> Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU
                        || checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS)
                                == PackageManager.PERMISSION_GRANTED,
                AudioNodeService::cameraLifecycleSnapshot,
                () -> checkSelfPermission(Manifest.permission.CAMERA)
                        == PackageManager.PERMISSION_GRANTED,
                true,
                true,
                inventory.microphonePresence(),
                inventory.speakerPresence(),
                inventory.cameraPresence(),
                inventory.imuPresence(),
                inventory.touchSurfacePresence(),
                inventory.vibratorPresence());
        imuAdapter = new ImuSourceAdapter(
                getSystemService(SensorManager.class),
                new ImuSourceAdapter.Listener() {
                    @Override
                    public void onStarted(long generation) {
                        imuController.completeStart(generation);
                        startControllerRouteIfNeeded();
                        notifyStateListener();
                    }

                    @Override
                    public void onStopped(long generation) {
                        imuController.completeStop(generation);
                        stopControllerRouteIfUnused();
                        notifyStateListener();
                    }

                    @Override
                    public void onFailed(long generation, String problemCode) {
                        imuController.fail(generation, problemCode);
                        stopControllerRouteIfUnused();
                        notifyStateListener();
                    }
                });
        controllerRouteAdapter = new ControllerRouteAdapter(
                gamepadController::controls, imuAdapter::latestSample);
        hapticsAdapter = new AndroidHapticsSinkAdapter(
                getSystemService(Vibrator.class), hapticsController, this::postStateChanged);
        touchpadAdapter = new TouchpadRouteAdapter(new TouchpadRouteAdapter.Listener() {
            @Override
            public void onStarted(long generation) {
                completeTouchpadStart(generation);
            }

            @Override
            public void onStopped(long generation) {
                completeTouchpadStop(generation);
            }

            @Override
            public void onFailed(long generation, String problemCode) {
                failTouchpad(generation, problemCode);
            }
        });
        createNotificationChannel();
        NativeLanMicrophoneSessionConfig microphoneConfig;
        try {
            microphoneConfig = NativeMicrophoneConfigLoader.load(this);
        } catch (RuntimeException invalidConfig) {
            microphoneConfig = null;
        }
        microphoneAdapter = new MicrophoneSourceAdapter(
                listenerFor(AudioCapabilityKind.MICROPHONE_SOURCE), microphoneConfig);
        NativeLanSpeakerSessionConfig speakerConfig;
        try {
            speakerConfig = NativeSpeakerConfigLoader.load(this);
        } catch (RuntimeException invalidConfig) {
            speakerConfig = null;
        }
        speakerAdapter = new SpeakerSinkAdapter(
                listenerFor(AudioCapabilityKind.SPEAKER_SINK), speakerConfig);
        nativeCatalogJson = publicationCatalogJson();
        catalogRuntime = new AndroidNativeCatalogRuntime(nativeCatalogJson);
    }

    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        String action = intent == null ? null : intent.getAction();
        if (ACTION_START_MICROPHONE.equals(action)) {
            if (checkSelfPermission(Manifest.permission.RECORD_AUDIO)
                    != PackageManager.PERMISSION_GRANTED) {
                recordPermissionFailure();
                stopSelf(startId);
            } else {
                startCapability(AudioCapabilityKind.MICROPHONE_SOURCE);
            }
        } else if (ACTION_STOP_MICROPHONE.equals(action)) {
            stopCapability(AudioCapabilityKind.MICROPHONE_SOURCE);
        } else if (ACTION_START_SPEAKER.equals(action)) {
            startCapability(AudioCapabilityKind.SPEAKER_SINK);
        } else if (ACTION_STOP_SPEAKER.equals(action)) {
            stopCapability(AudioCapabilityKind.SPEAKER_SINK);
        } else if (ACTION_START_IMU.equals(action)) {
            beginImu();
        } else if (ACTION_STOP_IMU.equals(action)) {
            endImu();
        } else if (ACTION_START_GAMEPAD.equals(action)) {
            beginGamepad();
        } else if (ACTION_STOP_GAMEPAD.equals(action)) {
            endGamepad();
        } else if (ACTION_START_TOUCHPAD.equals(action)) {
            beginTouchpad();
        } else if (ACTION_STOP_TOUCHPAD.equals(action)) {
            endTouchpad();
        } else if (ACTION_START_HAPTICS.equals(action)) {
            hapticsAdapter.start();
            refreshForegroundSafely();
        } else if (ACTION_STOP_HAPTICS.equals(action)) {
            hapticsAdapter.stop();
            refreshForegroundSafely();
        } else if (ACTION_STOP_ALL.equals(action)) {
            stopCapability(AudioCapabilityKind.MICROPHONE_SOURCE);
            stopCapability(AudioCapabilityKind.SPEAKER_SINK);
            endImu();
            endGamepad();
            endTouchpad();
            hapticsAdapter.stop();
            UnifiedCameraNodeFacade.stop(this);
            refreshForegroundSafely();
        }
        return START_NOT_STICKY;
    }

    @Override
    public IBinder onBind(Intent intent) {
        return binder;
    }

    @Override
    public synchronized void onDestroy() {
        destroying = true;
        TransitionToken microphone = controller.beginStop(AudioCapabilityKind.MICROPHONE_SOURCE);
        if (microphone.accepted()) {
            microphoneAdapter.stop(microphone.generation());
        }
        TransitionToken speaker = controller.beginStop(AudioCapabilityKind.SPEAKER_SINK);
        if (speaker.accepted()) {
            speakerAdapter.stop(speaker.generation());
        }
        TransitionToken imu = imuController.beginStop();
        if (imu.accepted()) {
            imuAdapter.stop(imu.generation());
        }
        TransitionToken gamepad = gamepadController.beginStop();
        if (gamepad.accepted()) {
            gamepadController.completeStop(gamepad.generation());
        }
        TransitionToken touchpad = touchpadController.beginStop();
        if (touchpad.accepted()) {
            touchpadAdapter.stop(touchpad.generation());
        }
        hapticsAdapter.stop();
        controllerRouteAdapter.stop();
        if (catalogRuntime != null) {
            catalogRuntime.close();
            catalogRuntime = null;
            nativeCatalogJson = null;
        }
        stateListener = null;
        super.onDestroy();
    }

    private synchronized void startCapability(AudioCapabilityKind kind) {
        TransitionToken transition = controller.beginStart(kind);
        if (!transition.accepted()) {
            notifyStateListener();
            return;
        }
        try {
            refreshForeground();
        } catch (RuntimeException denied) {
            controller.fail(kind, transition.generation(), PROBLEM_FOREGROUND);
            notifyStateListener();
            stopSelf();
            return;
        }
        adapter(kind).start(transition.generation());
        notifyStateListener();
    }

    private synchronized void stopCapability(AudioCapabilityKind kind) {
        TransitionToken transition = controller.beginStop(kind);
        if (!transition.accepted()) {
            notifyStateListener();
            return;
        }
        refreshForegroundSafely();
        adapter(kind).stop(transition.generation());
        notifyStateListener();
    }

    private AudioPlatformAdapter adapter(AudioCapabilityKind kind) {
        return kind == AudioCapabilityKind.MICROPHONE_SOURCE
                ? microphoneAdapter
                : speakerAdapter;
    }

    private void beginImu() {
        TransitionToken transition = imuController.beginStart();
        if (transition.accepted()) {
            if (!refreshForegroundSafely()) {
                return;
            }
            imuAdapter.start(transition.generation());
        }
        notifyStateListener();
    }

    private void endImu() {
        TransitionToken transition = imuController.beginStop();
        if (transition.accepted()) {
            imuAdapter.stop(transition.generation());
        }
        refreshForegroundSafely();
        notifyStateListener();
    }

    private synchronized void beginGamepad() {
        TransitionToken transition = gamepadController.beginStart();
        if (transition.accepted()) {
            if (!refreshForegroundSafely()) {
                return;
            }
            if (touchSurfacePresence == HardwarePresence.PRESENT) {
                gamepadController.completeStart(transition.generation());
                startControllerRouteIfNeeded();
            } else {
                gamepadController.fail(
                        transition.generation(), "CAPY.ANDROID.GAMEPAD_HARDWARE_MISSING");
            }
        }
        refreshForegroundSafely();
        notifyStateListener();
    }

    private synchronized void endGamepad() {
        TransitionToken transition = gamepadController.beginStop();
        if (transition.accepted()) {
            gamepadController.completeStop(transition.generation());
            stopControllerRouteIfUnused();
        }
        refreshForegroundSafely();
        notifyStateListener();
    }

    private synchronized boolean submitGamepad(
            long generation, GamepadControlsSnapshot controls) {
        boolean accepted = GenerationBoundSourceDelivery.submit(
                () -> gamepadController.submit(generation, controls),
                () -> controllerRouteAdapter.submitControls(controls),
                () -> {
                    gamepadController.fail(
                            generation, ControllerRouteAdapter.PROBLEM_BACKPRESSURE);
                    controllerRouteAdapter.discardControls();
                    stopControllerRouteIfUnused();
                    notifyStateListener();
                });
        if (accepted) {
            notifyPresentationListener();
        }
        return accepted;
    }

    private void startControllerRouteIfNeeded() {
        if (imuController.snapshot().state() == CapabilityLifecycleState.ACTIVE
                || gamepadController.lifecycle().state() == CapabilityLifecycleState.ACTIVE) {
            controllerRouteAdapter.start();
        }
    }

    private void stopControllerRouteIfUnused() {
        if (imuController.snapshot().state() != CapabilityLifecycleState.ACTIVE
                && gamepadController.lifecycle().state() != CapabilityLifecycleState.ACTIVE) {
            controllerRouteAdapter.stop();
        }
    }

    private synchronized void beginTouchpad() {
        TransitionToken transition = touchpadController.beginStart();
        if (transition.accepted()) {
            if (!refreshForegroundSafely()) {
                return;
            }
            if (touchSurfacePresence == HardwarePresence.PRESENT) {
                touchpadAdapter.start(transition.generation());
            } else {
                touchpadController.fail(
                        transition.generation(), "CAPY.ANDROID.TOUCHPAD_HARDWARE_MISSING");
            }
        }
        refreshForegroundSafely();
        notifyStateListener();
    }

    private synchronized void endTouchpad() {
        TransitionToken transition = touchpadController.beginStop();
        if (transition.accepted()) {
            touchpadAdapter.stop(transition.generation());
        }
        refreshForegroundSafely();
        notifyStateListener();
    }

    private synchronized boolean submitTouchpad(
            long generation, java.util.List<TouchpadContactSnapshot> contacts) {
        boolean accepted = GenerationBoundSourceDelivery.submit(
                () -> touchpadController.submit(generation, contacts),
                () -> touchpadAdapter.submit(generation, contacts),
                () -> {
                    touchpadController.fail(
                            generation, TouchpadRouteAdapter.PROBLEM_BACKPRESSURE);
                    touchpadAdapter.stop(generation);
                    notifyStateListener();
                });
        if (accepted) {
            notifyPresentationListener();
        }
        return accepted;
    }

    private synchronized void releaseUiInputs() {
        CapabilityLifecycleSnapshot gamepadLifecycle = gamepadController.lifecycle();
        GamepadControlsSnapshot gamepadRelease = AndroidUiInputRelease.gamepad(
                gamepadLifecycle, gamepadController.controls());
        if (gamepadRelease != null) {
            submitGamepad(gamepadLifecycle.generation(), gamepadRelease);
        }

        CapabilityLifecycleSnapshot touchpadLifecycle = touchpadController.lifecycle();
        java.util.List<TouchpadContactSnapshot> touchpadRelease =
                AndroidUiInputRelease.touchpad(
                        touchpadLifecycle, touchpadController.controls());
        if (touchpadRelease != null) {
            submitTouchpad(touchpadLifecycle.generation(), touchpadRelease);
        }
    }

    private synchronized void completeTouchpadStart(long generation) {
        touchpadController.completeStart(generation);
        postStateChanged();
    }

    private synchronized void completeTouchpadStop(long generation) {
        touchpadController.completeStop(generation);
        postStateChanged();
    }

    private synchronized void failTouchpad(long generation, String problemCode) {
        touchpadController.fail(generation, problemCode);
        postStateChanged();
    }

    private AudioPlatformAdapter.Listener listenerFor(AudioCapabilityKind kind) {
        return new AudioPlatformAdapter.Listener() {
            @Override
            public void onStarted(long generation, ActualAudioFormat actualFormat) {
                controller.completeStart(kind, generation, actualFormat);
                postStateChanged();
            }

            @Override
            public void onFrames(long generation, long frames) {
                controller.addFrames(kind, generation, frames);
            }

            @Override
            public void onStopped(long generation) {
                controller.completeStop(kind, generation);
                postStateChanged();
            }

            @Override
            public void onFailed(long generation, String problemCode) {
                controller.fail(kind, generation, problemCode);
                postStateChanged();
            }
        };
    }

    private void recordPermissionFailure() {
        TransitionToken transition = controller.beginStart(AudioCapabilityKind.MICROPHONE_SOURCE);
        if (transition.accepted()) {
            controller.fail(
                    AudioCapabilityKind.MICROPHONE_SOURCE,
                    transition.generation(),
                    PROBLEM_PERMISSION);
        }
        notifyStateListener();
    }

    private void postStateChanged() {
        mainHandler.post(() -> {
            if (!destroying) {
                refreshForegroundSafely();
                notifyStateListener();
            }
        });
    }

    private boolean refreshForegroundSafely() {
        try {
            refreshForeground();
            return true;
        } catch (RuntimeException denied) {
            failForegroundOwnedCapabilities();
            return false;
        }
    }

    private void failForegroundOwnedCapabilities() {
        AudioNodeSnapshot snapshot = snapshot();
        failForegroundCapability(snapshot.microphone());
        failForegroundCapability(snapshot.speaker());
        CapabilityLifecycleSnapshot imu = imuController.snapshot();
        if (imu.state().ownsAdapterLifecycle()) {
            imuController.fail(imu.generation(), PROBLEM_FOREGROUND);
            imuAdapter.stop(imu.generation());
        }
        CapabilityLifecycleSnapshot gamepad = gamepadController.lifecycle();
        if (gamepad.state().ownsAdapterLifecycle()) {
            gamepadController.fail(gamepad.generation(), PROBLEM_FOREGROUND);
            controllerRouteAdapter.discardControls();
            stopControllerRouteIfUnused();
        }
        CapabilityLifecycleSnapshot touchpad = touchpadController.lifecycle();
        if (touchpad.state().ownsAdapterLifecycle()) {
            touchpadController.fail(touchpad.generation(), PROBLEM_FOREGROUND);
            touchpadAdapter.stop(touchpad.generation());
        }
        hapticsAdapter.failForeground(PROBLEM_FOREGROUND);
        if (foreground) {
            stopForeground(STOP_FOREGROUND_REMOVE);
            foreground = false;
        }
        stopSelf();
        notifyStateListener();
    }

    private void failForegroundCapability(AudioCapabilitySnapshot capability) {
        if (capability.state().ownsForegroundLifecycle()) {
            controller.fail(capability.kind(), capability.generation(), PROBLEM_FOREGROUND);
            adapter(capability.kind()).stop(capability.generation());
        }
    }

    private void refreshForeground() {
        AudioNodeSnapshot snapshot = snapshot();
        boolean microphone = snapshot.microphone().state().ownsForegroundLifecycle();
        boolean speaker = snapshot.speaker().state().ownsForegroundLifecycle();
        boolean connectedDevice = ownsConnectedDeviceLifecycle();
        if (!microphone && !speaker && !connectedDevice) {
            if (foreground) {
                stopForeground(STOP_FOREGROUND_REMOVE);
                foreground = false;
            }
            stopSelf();
            return;
        }

        Notification notification = buildNotification(microphone, speaker, connectedDevice);
        int serviceTypes = 0;
        if (microphone && Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            serviceTypes |= ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE;
        }
        if (speaker) {
            serviceTypes |= ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK;
        }
        if (connectedDevice) {
            serviceTypes |= ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE;
        }
        startForeground(NOTIFICATION_ID, notification, serviceTypes);
        foreground = true;
    }

    private boolean ownsConnectedDeviceLifecycle() {
        return AndroidForegroundOwnership.connectedDevice(
                imuController.snapshot().state(),
                gamepadController.lifecycle().state(),
                touchpadController.lifecycle().state(),
                hapticsController.snapshot().state());
    }

    private Notification buildNotification(
            boolean microphone, boolean speaker, boolean connectedDevice) {
        Intent openIntent = new Intent(this, MainActivity.class)
                .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP);
        PendingIntent openPendingIntent = PendingIntent.getActivity(
                this,
                0,
                openIntent,
                PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
        PendingIntent stopPendingIntent = PendingIntent.getService(
                this,
                1,
                new Intent(this, AudioNodeService.class).setAction(ACTION_STOP_ALL),
                PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);

        java.util.List<String> details = new java.util.ArrayList<>(3);
        if (microphone) {
            details.add(getString(R.string.notification_microphone));
        }
        if (speaker) {
            details.add(getString(R.string.notification_speaker));
        }
        if (connectedDevice) {
            details.add(getString(R.string.notification_connected_device));
        }
        return new Notification.Builder(this, NOTIFICATION_CHANNEL_ID)
                .setSmallIcon(R.drawable.ic_capyio)
                .setContentTitle(getString(R.string.notification_title))
                .setContentText(String.join(" · ", details))
                .setContentIntent(openPendingIntent)
                .setOngoing(true)
                .setOnlyAlertOnce(true)
                .setCategory(Notification.CATEGORY_SERVICE)
                .addAction(new Notification.Action.Builder(
                        null,
                        getString(R.string.notification_stop_all),
                        stopPendingIntent).build())
                .build();
    }

    private void createNotificationChannel() {
        NotificationChannel channel = new NotificationChannel(
                NOTIFICATION_CHANNEL_ID,
                getString(R.string.notification_channel_name),
                NotificationManager.IMPORTANCE_LOW);
        channel.setDescription(getString(R.string.notification_channel_description));
        NotificationManager manager = getSystemService(NotificationManager.class);
        manager.createNotificationChannel(channel);
    }

    private AudioNodeSnapshot snapshot() {
        return controller.snapshot(nodeId);
    }

    private CapabilityCatalogSnapshot capabilityCatalogSnapshot() {
        return capabilityRegistry.snapshot(nodeId);
    }

    private String publicationCatalogJson() {
        return encodeCoreCatalog(
                sharingSelection.publicationCatalog(capabilityCatalogSnapshot()));
    }

    private static String encodeCoreCatalog(CapabilityCatalogSnapshot publication) {
        return AndroidCoreCatalogJson.encode(
                publication, Build.VERSION.RELEASE, BuildConfig.VERSION_NAME);
    }

    private void refreshNativeCatalog(CapabilityCatalogSnapshot catalog) {
        replaceNativePublication(sharingSelection.publicationCatalog(catalog));
    }

    private synchronized void replaceNativePublication(CapabilityCatalogSnapshot publication) {
        String candidate = encodeCoreCatalog(publication);
        if (candidate.equals(nativeCatalogJson)) {
            return;
        }
        catalogRuntime.replaceDisconnectedCatalog(candidate);
        nativeCatalogJson = candidate;
    }

    private static CapabilityLifecycleSnapshot cameraLifecycleSnapshot() {
        UnifiedCameraNodeFacade.Snapshot camera = UnifiedCameraNodeFacade.snapshot();
        CapabilityLifecycleState state = switch (camera.state()) {
            case "stopped" -> CapabilityLifecycleState.STOPPED;
            case "starting" -> CapabilityLifecycleState.STARTING;
            case "active" -> CapabilityLifecycleState.ACTIVE;
            case "failed" -> CapabilityLifecycleState.FAILED;
            default -> throw new IllegalStateException("Unknown camera lifecycle state");
        };
        return new CapabilityLifecycleSnapshot(state, camera.generation(), camera.problemCode());
    }

    private void notifyStateListener() {
        if (catalogRuntime != null) {
            try {
                refreshNativeCatalog(capabilityCatalogSnapshot());
            } catch (RuntimeException rejected) {
                // Keep the last Core-valid native snapshot until local intent becomes valid again.
            }
        }
        notifyPresentationListener();
    }

    private void notifyPresentationListener() {
        Runnable listener = stateListener;
        if (listener != null) {
            listener.run();
        }
    }

    public final class NodeBinder extends Binder {
        public boolean configureAudioPeer(String direction, String host, int localPort, int peerPort,
                boolean trustedLabConfirmed) {
            synchronized (AudioNodeService.this) {
                if (!trustedLabConfirmed) return false;
                AudioNodeSnapshot audio = snapshot();
                AudioCapabilityState state;
                if ("microphone".equals(direction)) state = audio.microphone().state();
                else if ("speaker".equals(direction)) state = audio.speaker().state();
                else return false;
                if (state != AudioCapabilityState.STOPPED && state != AudioCapabilityState.FAILED) return false;
                try {
                    AndroidAudioPeer peer = new AndroidAudioPeer(host, localPort, peerPort);
                    Runnable persist = () -> AndroidAudioPeerStore.save(AudioNodeService.this, direction, peer);
                    if ("microphone".equals(direction)) {
                        NativeLanSpeakerSessionConfig other = NativeSpeakerConfigLoader.load(AudioNodeService.this);
                        if (other != null && other.localAddress.getPort() == localPort) return false;
                        return microphoneAdapter.configure(NativeMicrophoneConfigLoader.fromPeer(peer), persist);
                    }
                    if ("speaker".equals(direction)) {
                        NativeLanMicrophoneSessionConfig other = NativeMicrophoneConfigLoader.load(AudioNodeService.this);
                        if (other != null && other.localAddress.getPort() == localPort) return false;
                        return speakerAdapter.configure(NativeSpeakerConfigLoader.fromPeer(peer), persist);
                    }
                    return false;
                } catch (RuntimeException rejected) {
                    return false;
                }
            }
        }

        public AudioNodeSnapshot snapshot() {
            return AudioNodeService.this.snapshot();
        }

        public CapabilityCatalogSnapshot capabilityCatalog() {
            CapabilityCatalogSnapshot catalog = AudioNodeService.this.capabilityCatalogSnapshot();
            try {
                refreshNativeCatalog(catalog);
            } catch (RuntimeException rejected) {
                // Read remains available while native state keeps its last valid snapshot.
            }
            return catalog;
        }

        public CapabilityLifecycleSnapshot cameraLifecycle() {
            return AudioNodeService.cameraLifecycleSnapshot();
        }

        public java.util.Set<String> selectedSharedCapabilityIds() {
            return sharingSelection.snapshot();
        }

        public CapabilityCatalogSnapshot publicationCatalog() {
            return sharingSelection.publicationCatalog(
                    AudioNodeService.this.capabilityCatalogSnapshot());
        }

        public String publicationCatalogJson() {
            return AudioNodeService.this.publicationCatalogJson();
        }

        public boolean setLocalCapabilityShared(String capabilityId, boolean selected) {
            CapabilityCatalogSnapshot catalog = AudioNodeService.this.capabilityCatalogSnapshot();
            boolean accepted = sharingSelection.update(
                    catalog,
                    capabilityId,
                    selected,
                    AudioNodeService.this::replaceNativePublication);
            if (accepted) {
                notifyStateListener();
            }
            return accepted;
        }

        public CapabilityLifecycleSnapshot imuLifecycle() {
            return imuController.snapshot();
        }

        public void startImu() {
            AudioNodeService.startImu(AudioNodeService.this);
        }

        public void stopImu() {
            AudioNodeService.stopImu(AudioNodeService.this);
        }

        public CapabilityLifecycleSnapshot gamepadLifecycle() {
            return gamepadController.lifecycle();
        }

        public GamepadControlsSnapshot gamepadControls() {
            return gamepadController.controls();
        }

        public void startGamepad() {
            AudioNodeService.startGamepad(AudioNodeService.this);
        }

        public boolean configureControllerRoute(String host, int port, String token) {
            boolean accepted = controllerRouteAdapter.configure(host, port, token);
            if (accepted) {
                startControllerRouteIfNeeded();
            }
            return accepted;
        }

        public String controllerRouteEndpoint() {
            return controllerRouteAdapter.endpointLabel();
        }

        public boolean controllerRouteRunning() {
            return controllerRouteAdapter.isRunning();
        }

        public String controllerRouteProblemCode() {
            return controllerRouteAdapter.problemCode();
        }

        public boolean submitGamepad(long generation, GamepadControlsSnapshot controls) {
            if (controls == null) {
                throw new IllegalArgumentException("gamepad controls are required");
            }
            return AudioNodeService.this.submitGamepad(generation, controls);
        }

        public void stopGamepad() {
            AudioNodeService.stopGamepad(AudioNodeService.this);
        }

        public CapabilityLifecycleSnapshot touchpadLifecycle() {
            return touchpadController.lifecycle();
        }

        public TouchpadControlsSnapshot touchpadControls() {
            return touchpadController.controls();
        }

        public void startTouchpad() {
            AudioNodeService.startTouchpad(AudioNodeService.this);
        }

        public boolean submitTouchpad(
                long generation, java.util.List<TouchpadContactSnapshot> contacts) {
            if (contacts == null) {
                throw new IllegalArgumentException("touchpad contacts are required");
            }
            return AudioNodeService.this.submitTouchpad(generation, contacts);
        }

        public void stopTouchpad() {
            AudioNodeService.stopTouchpad(AudioNodeService.this);
        }

        public void releaseUiInputs() {
            AudioNodeService.this.releaseUiInputs();
        }

        public CapabilityLifecycleSnapshot hapticsLifecycle() {
            return hapticsController.snapshot();
        }

        public void startHaptics() {
            AudioNodeService.startHaptics(AudioNodeService.this);
        }

        public void stopHaptics() {
            AudioNodeService.stopHaptics(AudioNodeService.this);
        }

        public boolean submitHaptics(
                long generation, int lowFrequency, int highFrequency, int durationMillis) {
            try {
                return hapticsAdapter.submit(AndroidHapticsCommand.rumble(
                        generation, lowFrequency, highFrequency, durationMillis));
            } catch (IllegalArgumentException rejected) {
                return false;
            }
        }

        public boolean cancelHaptics(long generation) {
            try {
                return hapticsAdapter.submit(AndroidHapticsCommand.stop(generation));
            } catch (IllegalArgumentException rejected) {
                return false;
            }
        }

        public int speakerUnderrunCount() {
            return speakerAdapter.underrunCount();
        }

        public SpeakerSinkAdapter.TransportMetrics speakerTransportMetrics() {
            return speakerAdapter.transportMetrics();
        }

        public MicrophoneSourceAdapter.TransportMetrics microphoneTransportMetrics() {
            return microphoneAdapter.transportMetrics();
        }

        public void setStateListener(Runnable listener) {
            stateListener = listener;
        }
    }
}
