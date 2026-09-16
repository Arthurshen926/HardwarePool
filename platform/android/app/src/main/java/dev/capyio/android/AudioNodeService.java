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
    private AndroidHapticsSinkAdapter hapticsAdapter;
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
                        notifyStateListener();
                    }

                    @Override
                    public void onStopped(long generation) {
                        imuController.completeStop(generation);
                        notifyStateListener();
                    }

                    @Override
                    public void onFailed(long generation, String problemCode) {
                        imuController.fail(generation, problemCode);
                        notifyStateListener();
                    }
                });
        hapticsAdapter = new AndroidHapticsSinkAdapter(
                getSystemService(Vibrator.class), hapticsController, this::notifyStateListener);
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
        } else if (ACTION_STOP_ALL.equals(action)) {
            stopCapability(AudioCapabilityKind.MICROPHONE_SOURCE);
            stopCapability(AudioCapabilityKind.SPEAKER_SINK);
            stopImu();
            stopGamepad();
            stopTouchpad();
            hapticsAdapter.stop();
            UnifiedCameraNodeFacade.stop(this);
        }
        return START_NOT_STICKY;
    }

    @Override
    public IBinder onBind(Intent intent) {
        return binder;
    }

    @Override
    public void onDestroy() {
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
            touchpadController.completeStop(touchpad.generation());
        }
        hapticsAdapter.stop();
        stateListener = null;
        super.onDestroy();
    }

    private void startCapability(AudioCapabilityKind kind) {
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

    private void stopCapability(AudioCapabilityKind kind) {
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

    private void startImu() {
        TransitionToken transition = imuController.beginStart();
        if (transition.accepted()) {
            imuAdapter.start(transition.generation());
        }
        notifyStateListener();
    }

    private void stopImu() {
        TransitionToken transition = imuController.beginStop();
        if (transition.accepted()) {
            imuAdapter.stop(transition.generation());
        }
        notifyStateListener();
    }

    private void startGamepad() {
        TransitionToken transition = gamepadController.beginStart();
        if (transition.accepted()) {
            if (touchSurfacePresence == HardwarePresence.PRESENT) {
                gamepadController.completeStart(transition.generation());
            } else {
                gamepadController.fail(
                        transition.generation(), "CAPY.ANDROID.GAMEPAD_HARDWARE_MISSING");
            }
        }
        notifyStateListener();
    }

    private void stopGamepad() {
        TransitionToken transition = gamepadController.beginStop();
        if (transition.accepted()) {
            gamepadController.completeStop(transition.generation());
        }
        notifyStateListener();
    }

    private boolean submitGamepad(long generation, GamepadControlsSnapshot controls) {
        boolean accepted = gamepadController.submit(generation, controls);
        if (accepted) {
            notifyStateListener();
        }
        return accepted;
    }

    private void startTouchpad() {
        TransitionToken transition = touchpadController.beginStart();
        if (transition.accepted()) {
            if (touchSurfacePresence == HardwarePresence.PRESENT) {
                touchpadController.completeStart(transition.generation());
            } else {
                touchpadController.fail(
                        transition.generation(), "CAPY.ANDROID.TOUCHPAD_HARDWARE_MISSING");
            }
        }
        notifyStateListener();
    }

    private void stopTouchpad() {
        TransitionToken transition = touchpadController.beginStop();
        if (transition.accepted()) {
            touchpadController.completeStop(transition.generation());
        }
        notifyStateListener();
    }

    private boolean submitTouchpad(
            long generation, java.util.List<TouchpadContactSnapshot> contacts) {
        boolean accepted = touchpadController.submit(generation, contacts);
        if (accepted) {
            notifyStateListener();
        }
        return accepted;
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

    private void refreshForegroundSafely() {
        try {
            refreshForeground();
        } catch (RuntimeException denied) {
            failForegroundOwnedCapabilities();
        }
    }

    private void failForegroundOwnedCapabilities() {
        AudioNodeSnapshot snapshot = snapshot();
        failForegroundCapability(snapshot.microphone());
        failForegroundCapability(snapshot.speaker());
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
        if (!microphone && !speaker) {
            if (foreground) {
                stopForeground(STOP_FOREGROUND_REMOVE);
                foreground = false;
            }
            stopSelf();
            return;
        }

        Notification notification = buildNotification(microphone, speaker);
        int serviceTypes = 0;
        if (microphone && Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            serviceTypes |= ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE;
        }
        if (speaker) {
            serviceTypes |= ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK;
        }
        startForeground(NOTIFICATION_ID, notification, serviceTypes);
        foreground = true;
    }

    private Notification buildNotification(boolean microphone, boolean speaker) {
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

        int detail = microphone && speaker
                ? R.string.notification_both
                : microphone
                        ? R.string.notification_microphone
                        : R.string.notification_speaker;
        return new Notification.Builder(this, NOTIFICATION_CHANNEL_ID)
                .setSmallIcon(R.drawable.ic_capyio)
                .setContentTitle(getString(R.string.notification_title))
                .setContentText(getString(detail))
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
        Runnable listener = stateListener;
        if (listener != null) {
            listener.run();
        }
    }

    public final class NodeBinder extends Binder {
        public AudioNodeSnapshot snapshot() {
            return AudioNodeService.this.snapshot();
        }

        public CapabilityCatalogSnapshot capabilityCatalog() {
            return AudioNodeService.this.capabilityCatalogSnapshot();
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
            return AndroidCoreCatalogJson.encode(
                    publicationCatalog(), Build.VERSION.RELEASE, BuildConfig.VERSION_NAME);
        }

        public boolean setLocalCapabilityShared(String capabilityId, boolean selected) {
            boolean accepted = sharingSelection.update(
                    AudioNodeService.this.capabilityCatalogSnapshot(), capabilityId, selected);
            if (accepted) {
                notifyStateListener();
            }
            return accepted;
        }

        public CapabilityLifecycleSnapshot imuLifecycle() {
            return imuController.snapshot();
        }

        public void startImu() {
            mainHandler.post(AudioNodeService.this::startImu);
        }

        public void stopImu() {
            mainHandler.post(AudioNodeService.this::stopImu);
        }

        public CapabilityLifecycleSnapshot gamepadLifecycle() {
            return gamepadController.lifecycle();
        }

        public GamepadControlsSnapshot gamepadControls() {
            return gamepadController.controls();
        }

        public void startGamepad() {
            mainHandler.post(AudioNodeService.this::startGamepad);
        }

        public boolean submitGamepad(long generation, GamepadControlsSnapshot controls) {
            if (controls == null) {
                throw new IllegalArgumentException("gamepad controls are required");
            }
            return AudioNodeService.this.submitGamepad(generation, controls);
        }

        public void stopGamepad() {
            mainHandler.post(AudioNodeService.this::stopGamepad);
        }

        public CapabilityLifecycleSnapshot touchpadLifecycle() {
            return touchpadController.lifecycle();
        }

        public TouchpadControlsSnapshot touchpadControls() {
            return touchpadController.controls();
        }

        public void startTouchpad() {
            mainHandler.post(AudioNodeService.this::startTouchpad);
        }

        public boolean submitTouchpad(
                long generation, java.util.List<TouchpadContactSnapshot> contacts) {
            if (contacts == null) {
                throw new IllegalArgumentException("touchpad contacts are required");
            }
            return AudioNodeService.this.submitTouchpad(generation, contacts);
        }

        public void stopTouchpad() {
            mainHandler.post(AudioNodeService.this::stopTouchpad);
        }

        public CapabilityLifecycleSnapshot hapticsLifecycle() {
            return hapticsController.snapshot();
        }

        public void startHaptics() {
            mainHandler.post(hapticsAdapter::start);
        }

        public void stopHaptics() {
            mainHandler.post(hapticsAdapter::stop);
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
