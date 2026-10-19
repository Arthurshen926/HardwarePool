package dev.capyio.android;

import android.Manifest;
import android.app.Notification;
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
    private static final String ACTION_CONTROLLER_MODE = "dev.capyio.android.action.CONTROLLER_MODE";
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
    private static final String ACTION_REFRESH_PUBLICATION =
            "dev.capyio.android.action.REFRESH_PUBLICATION";
    private static final String ACTION_ENABLE_BACKGROUND_WAITING =
            "dev.capyio.android.action.ENABLE_BACKGROUND_WAITING";
    private static final String ACTION_DISABLE_BACKGROUND_WAITING =
            "dev.capyio.android.action.DISABLE_BACKGROUND_WAITING";
    private static final String NODE_PREFERENCES = "capyio-node-connectivity-v1";
    private static final String BACKGROUND_WAITING_KEY = "background-waiting-enabled";
    private volatile boolean publicationForeground;
    private volatile boolean backgroundWaitingForeground;

    private AndroidNodeNotification nodeNotification;
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
    private AndroidCatalogModule catalogModule;

    private CapabilityModuleRegistry capabilityRegistry;
    private MicrophoneSourceAdapter microphoneAdapter;
    private SpeakerSinkAdapter speakerAdapter;
    private ImuSourceAdapter imuAdapter;
    private volatile boolean gamepadMotionEnabled;
    private long controllerSettingsRevision;
    private long controllerSettingsRequestId;
    private String controllerSettingsError = "";
    private ControllerRouteAdapter controllerRouteAdapter;
    private TouchpadRouteAdapter touchpadAdapter;
    private AndroidHapticsSinkAdapter hapticsAdapter;
    private HapticsDataWorker hapticsDataWorker;
    private AndroidNativeCatalogRuntime catalogRuntime;
    private final AndroidResourceConsentModule resourceConsent = new AndroidResourceConsentModule(this);
    private long nextRememberedConsentCheck;
    // The remote settings acknowledgement must follow durable storage; rollback
    // also uses commit so a failed revision is not acknowledged as persisted.
    @android.annotation.SuppressLint("ApplySharedPref")
    private boolean applyControllerPreferences(boolean enabled, int strength) {
        if (strength < 0 || strength > 100) return false;
        if (enabled == gamepadMotionEnabled && strength == hapticsAdapter.strengthPercent()) return true;
        boolean startedMotion = enabled && !gamepadMotionEnabled
                && gamepadController.lifecycle().state() == CapabilityLifecycleState.ACTIVE;
        if (startedMotion && !imuAdapter.startForGamepad()) return false;
        int previousStrength = hapticsAdapter.strengthPercent();
        try { hapticsAdapter.setStrengthPercent(strength); }
        catch (RuntimeException unavailable) {
            if (startedMotion) imuAdapter.stopForGamepad();
            return false;
        }
        long revision = controllerSettingsRevision + 1;
        boolean saved = getSharedPreferences("capyio-gamepad", MODE_PRIVATE).edit()
                .putBoolean("motion-enabled", enabled).putInt("haptics-strength", strength)
                .putLong("settings-revision", revision).commit();
        if (!saved) {
            hapticsAdapter.setStrengthPercent(previousStrength);
            if (startedMotion) imuAdapter.stopForGamepad();
            getSharedPreferences("capyio-gamepad", MODE_PRIVATE).edit()
                    .putBoolean("motion-enabled", gamepadMotionEnabled).putInt("haptics-strength", previousStrength)
                    .putLong("settings-revision", controllerSettingsRevision).commit();
            return false;
        }
        gamepadMotionEnabled = enabled;
        if (!enabled) imuAdapter.stopForGamepad();
        controllerSettingsRevision = revision;
        // Preserve the last completed remote acknowledgement across a local edit:
        // the control worker may not have sent that result yet.
        notifyStateListener();
        return true;
    }

    // Only called on the main service poll while holding the service monitor.
    // A settings edit does not create a Route, grant vibration, or start a controller.
    private void synchronizeControllerSettings() {
        try {
            String pending = catalogRuntime.controllerSettings(controllerSettingsRevision,
                    gamepadMotionEnabled, hapticsAdapter.strengthPercent(),
                    controllerSettingsRequestId, controllerSettingsError);
            if ("null".equals(pending)) return;
            org.json.JSONObject request = new org.json.JSONObject(pending);
            long id = request.getLong("requestId");
            String error = "";
            if (request.getLong("expectedRevision") != controllerSettingsRevision) {
                error = "settings_revision_conflict";
            } else {
                boolean enabled = request.getBoolean("motionEnabled");
                int strength = request.getInt("hapticsStrength");
                // Reject the complete edit if enabling an already-running controller's
                // motion source fails, leaving the previous strength unchanged.
                if (!applyControllerPreferences(enabled, strength)) error = "settings_apply_failed";
            }
            controllerSettingsRequestId = id;
            controllerSettingsError = error;
            // Publish actual applied values; the next poll handles any subsequent request.
        } catch (org.json.JSONException invalid) {
            controllerSettingsError = "invalid_settings_request";
        }
    }

    private String authenticatedControllerReceiver = "null";
    private AndroidControllerUseModule controllerUse;
    private AndroidAudioUseModule audioUse;
    private final AndroidCameraUseModule cameraUse = new AndroidCameraUseModule(this);
    private final AndroidDisplayUseModule displayUse = new AndroidDisplayUseModule(this, this);
    private final Runnable secureTouchpadPoll = new Runnable() {
        @Override public void run() {
            synchronized (AudioNodeService.this) {
                if (destroying || catalogRuntime == null) return;
                synchronizeControllerSettings();
                controllerUse.synchronize(catalogRuntime);
                audioUse.synchronize(catalogRuntime);
                cameraUse.synchronize(catalogRuntime);
                displayUse.synchronize(catalogRuntime);
                long now = android.os.SystemClock.elapsedRealtime();
                if (now >= nextRememberedConsentCheck) {
                    nextRememberedConsentCheck = now + 1000;
                    resourceConsent.approveRemembered(catalogRuntime, control.state(), control.directory());
                    configureAuthenticatedControllerReceiver();
                }
                long hapticsJob = AndroidNativeHapticsData.nativePending(catalogRuntime.nativeHandle());
                if (hapticsJob > 0 && (hapticsDataWorker == null || !hapticsDataWorker.isAlive())) {
                    long generation = hapticsAdapter.beginRemote();
                    if (generation > 0 && refreshForegroundSafely()) {
                        hapticsDataWorker = new HapticsDataWorker(hapticsJob, generation, hapticsAdapter);
                        hapticsDataWorker.start();
                    } else {
                        if (generation > 0) hapticsAdapter.endRemote(generation);
                        AndroidNativeHapticsData.nativeClose(hapticsJob);
                    }
                }
                long job = AndroidNativeTouchpadData.nativePending(catalogRuntime.nativeHandle());
                if (job != 0) {
                    CapabilityLifecycleSnapshot lifecycle = touchpadController.lifecycle();
                    if (lifecycle.state() == CapabilityLifecycleState.ACTIVE
                            || lifecycle.state() == CapabilityLifecycleState.STARTING) {
                        // A user may have opened the old local-lab touchpad before a real
                        // authenticated Route arrived. Stop that worker, retain the pending
                        // secure listener, and let the next poll start the requested Route.
                        TransitionToken stop = touchpadController.beginStop();
                        if (stop.accepted()) touchpadAdapter.stop(stop.generation());
                    } else if (lifecycle.state() != CapabilityLifecycleState.STOPPING) {
                        TransitionToken transition = touchpadController.beginStart();
                        if (transition.accepted()
                                && touchSurfacePresence == HardwarePresence.PRESENT
                                && refreshForegroundSafely()) {
                            touchpadAdapter.startSecure(transition.generation(), job);
                        } else {
                            AndroidNativeTouchpadData.nativeStop(job);
                            if (transition.accepted()) touchpadController.fail(
                                    transition.generation(), TouchpadRouteAdapter.PROBLEM_TRANSPORT);
                        }
                    }
                    notifyStateListener();
                }
            }
            mainHandler.postDelayed(this, 100);
        }
    };
    private AndroidControlModule control;
    private AndroidFilelessPairing filelessPairing;
    private String nativeCatalogJson;
    private String nodeId;
    private HardwarePresence touchSurfacePresence;
    private boolean foreground;
    private boolean destroying;
    private Runnable stateListener;

    private void stopHapticsData() {
        if (hapticsDataWorker != null) hapticsDataWorker.stopData();
        hapticsAdapter.stop();
    }

    private void configureAuthenticatedControllerReceiver() {
        String current = catalogRuntime.controllerReceiverJson();
        if (current.equals(authenticatedControllerReceiver)) return;
        if (controllerUse.ownsRoute()) {
            // The authorized Route owner configured this exact session's endpoint.
            // A repeated advertisement must not restart its live sender.
            authenticatedControllerReceiver = current;
            return;
        }
        controllerRouteAdapter.stop();
        controllerRouteAdapter.clearEndpoint();
        authenticatedControllerReceiver = current;
        if (!"null".equals(current)) {
            try {
                org.json.JSONObject offer = new org.json.JSONObject(current);
                UnifiedCameraNodeFacade.configurePeer(offer.getString("host"));
                if (offer.getInt("port") != 0 && controllerRouteAdapter.configure(offer.getString("host"),
                        offer.getInt("port"), offer.getString("token"))) {
                    // Receiving an offer does not start sensors or touch controls.
                    // Only an already explicitly active local Source may publish.
                    startControllerRouteIfNeeded();
                }
            } catch (org.json.JSONException invalid) {
                controllerRouteAdapter.clearEndpoint();
            }
        } else {
            UnifiedCameraNodeFacade.stop(AudioNodeService.this);
            UnifiedCameraNodeFacade.configurePeer(null);
        }
        notifyStateListener();
    }

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

    static boolean backgroundWaitingPreference(Context context) {
        return context.getSharedPreferences(NODE_PREFERENCES, Context.MODE_PRIVATE)
                .getBoolean(BACKGROUND_WAITING_KEY, false);
    }

    static void restoreBackgroundWaiting(Context context) {
        if (backgroundWaitingPreference(context)) {
            startForegroundAction(context, ACTION_ENABLE_BACKGROUND_WAITING);
        } else if (!context.getSharedPreferences(NODE_PREFERENCES, Context.MODE_PRIVATE)
                .getStringSet("shared-capabilities", java.util.Set.of()).isEmpty()) {
            // Restore publication intent, not capture/input workers or Route grants.
            startForegroundAction(context, ACTION_REFRESH_PUBLICATION);
        }
    }

    private void setBackgroundWaitingPreference(boolean enabled) {
        getSharedPreferences(NODE_PREFERENCES, Context.MODE_PRIVATE)
                .edit().putBoolean(BACKGROUND_WAITING_KEY, enabled).apply();
    }

    @Override
    public void onCreate() {
        super.onCreate();
        nodeId = NodeIdentityStore.loadOrCreate(this);
        filelessPairing = new AndroidFilelessPairing(this, () -> mainHandler.post(() -> {
            boolean ready = reloadControlPairing();
            int port = 0;
            if (ready && control.listener() != null) {
                try { port = control.listener().port(); } catch (RuntimeException unavailable) { }
            }
            if (filelessPairing != null) filelessPairing.controlReady(port);
        }));
        control = new AndroidControlModule(this, nodeId);
        byte[] pendingControlCredential = control.loadCredentials();
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
        gamepadMotionEnabled = getSharedPreferences("capyio-gamepad", MODE_PRIVATE)
                .getBoolean("motion-enabled", true);
        controllerSettingsRevision = Math.max(1L, getSharedPreferences("capyio-gamepad", MODE_PRIVATE)
                .getLong("settings-revision", 1L));
        controllerRouteAdapter = new ControllerRouteAdapter(
                gamepadController::controls, imuAdapter::latestSample,
                () -> imuController.snapshot().state() == CapabilityLifecycleState.ACTIVE
                        || (gamepadMotionEnabled
                        && gamepadController.lifecycle().state() == CapabilityLifecycleState.ACTIVE));
        controllerUse = new AndroidControllerUseModule(controllerRouteAdapter,
                mode -> { if ("dsu".equals(mode)) beginImu(); else beginGamepad(); },
                () -> { endGamepad(); endImu(); }, this::notifyStateListener);
        hapticsAdapter = new AndroidHapticsSinkAdapter(
                getSystemService(Vibrator.class), hapticsController, this::postStateChanged);
        hapticsAdapter.setStrengthPercent(getSharedPreferences("capyio-gamepad", MODE_PRIVATE)
                .getInt("haptics-strength", 70));
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
        nodeNotification = new AndroidNodeNotification(this,
                new Intent(this, AudioNodeService.class).setAction(ACTION_STOP_ALL));
        nodeNotification.createChannel();
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
        audioUse = new AndroidAudioUseModule(this, microphoneAdapter, speakerAdapter,
                this::snapshot, this::startCapability, this::stopCapability);
        catalogModule = new AndroidCatalogModule(getSharedPreferences(NODE_PREFERENCES, MODE_PRIVATE));
        catalogModule.restore(capabilityCatalogSnapshot());
        publicationForeground = !catalogModule.selectedIds().isEmpty();
        nativeCatalogJson = publicationCatalogJson();
        catalogRuntime = new AndroidNativeCatalogRuntime(nativeCatalogJson);
        control.attach(catalogRuntime, pendingControlCredential);
        mainHandler.post(secureTouchpadPoll);
    }

    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        String action = intent == null ? null : intent.getAction();
        if (ACTION_REFRESH_PUBLICATION.equals(action)) {
            publicationForeground = !catalogModule.selectedIds().isEmpty();
            refreshForegroundSafely();
        } else if (ACTION_ENABLE_BACKGROUND_WAITING.equals(action)) {
            backgroundWaitingForeground = controlListenerAvailable();
            if (refreshForegroundSafely() && backgroundWaitingForeground) {
                setBackgroundWaitingPreference(true);
            } else {
                backgroundWaitingForeground = false;
                setBackgroundWaitingPreference(false);
            }
            notifyStateListener();
        } else if (ACTION_DISABLE_BACKGROUND_WAITING.equals(action)) {
            backgroundWaitingForeground = false;
            setBackgroundWaitingPreference(false);
            refreshForegroundSafely();
            notifyStateListener();
        } else if (ACTION_START_MICROPHONE.equals(action)) {
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
        } else if (ACTION_CONTROLLER_MODE.equals(action)) {
            String mode = intent.getStringExtra("controllerMode");
            if ("touch".equals(mode)) {
                endImu();
                beginGamepad();
            } else if ("motion".equals(mode)) {
                endGamepad();
                stopHapticsData();
                beginImu();
            } else if ("stopped".equals(mode)) {
                endGamepad();
                endImu();
                stopHapticsData();
                refreshForegroundSafely();
            }
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
            stopHapticsData();
            refreshForegroundSafely();
        } else if (ACTION_STOP_ALL.equals(action)) {
            publicationForeground = false;
            backgroundWaitingForeground = false;
            setBackgroundWaitingPreference(false);
            stopCapability(AudioCapabilityKind.MICROPHONE_SOURCE);
            stopCapability(AudioCapabilityKind.SPEAKER_SINK);
            endImu();
            endGamepad();
            endTouchpad();
            stopHapticsData();
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
        if (filelessPairing != null) filelessPairing.close();
        displayUse.close();
        cameraUse.close();
        audioUse.close();
        mainHandler.removeCallbacks(secureTouchpadPoll);
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
        imuAdapter.stopForGamepad();
        if (gamepad.accepted()) {
            gamepadController.completeStop(gamepad.generation());
        }
        TransitionToken touchpad = touchpadController.beginStop();
        if (touchpad.accepted()) {
            touchpadAdapter.stop(touchpad.generation());
        }
        stopHapticsData();
        controllerRouteAdapter.stop();
        control.stopListener();
        if (catalogRuntime != null) {
            catalogRuntime.close();
            catalogRuntime = null;
            nativeCatalogJson = null;
        }
        control.clearState();
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

    private synchronized boolean reloadControlPairing() {
        if (control == null || catalogRuntime == null || destroying) return false;
        boolean ready = control.reload(catalogRuntime);
        notifyStateListener();
        return ready;
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
                if (gamepadMotionEnabled && !imuAdapter.startForGamepad()) {
                    gamepadController.fail(transition.generation(), imuAdapter.problemCode());
                    refreshForegroundSafely();
                    notifyStateListener();
                    return;
                }
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
            // The IMU may keep the shared publisher alive. Drop pending touch
            // frames so the next sample uses the controller's neutral state.
            controllerRouteAdapter.discardControls();
            imuAdapter.stopForGamepad();
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
                    imuAdapter.stopForGamepad();
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
        // The initiating WebView call reads this input state directly, while
        // lifecycle changes still dispatch. A full native-state event on every
        // touch frame queued Snapshot/Catalog reads onto the UI thread and
        // delayed both rendering and the subsequent input frames.
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
        if (!touchpadController.completeStop(generation)) {
            touchpadController.completeRemoteStop(generation);
        }
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
        publicationForeground = false;
        backgroundWaitingForeground = false;
        setBackgroundWaitingPreference(false);
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
            imuAdapter.stopForGamepad();
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

        Notification notification = nodeNotification.build(microphone, speaker, connectedDevice,
                publicationForeground, backgroundWaitingForeground);
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
        startForeground(AndroidNodeNotification.ID, notification, serviceTypes);
        foreground = true;
    }

    private boolean ownsConnectedDeviceLifecycle() {
        return AndroidForegroundOwnership.connectedDevice(
                publicationForeground,
                backgroundWaitingForeground,
                imuController.snapshot().state(),
                gamepadController.lifecycle().state(),
                touchpadController.lifecycle().state(),
                hapticsController.snapshot().state());
    }

    private boolean controlListenerAvailable() {
        try {
            return control.listener() != null && control.listener().port() > 0;
        } catch (RuntimeException unavailable) {
            return false;
        }
    }

    private AudioNodeSnapshot snapshot() {
        return controller.snapshot(nodeId);
    }

    private CapabilityCatalogSnapshot capabilityCatalogSnapshot() {
        return capabilityRegistry.snapshot(nodeId);
    }

    private String publicationCatalogJson() {
        return catalogModule.publicationJson(capabilityCatalogSnapshot());
    }

    private void refreshNativeCatalog(CapabilityCatalogSnapshot catalog) {
        replaceNativePublication(catalogModule.publication(catalog));
    }

    private synchronized void replaceNativePublication(CapabilityCatalogSnapshot publication) {
        String candidate = AndroidCatalogModule.encode(publication);
        if (candidate.equals(nativeCatalogJson)) {
            return;
        }
        catalogRuntime.replaceCatalog(candidate);
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
        public String getRemoteCatalogJson(){synchronized(AudioNodeService.this){return catalogRuntime.remoteCatalogJson();}}
        public String getCameraPeersJson(){synchronized(AudioNodeService.this){return catalogRuntime.cameraUse("peers","");}}
        public String getDisplayPeersJson(){synchronized(AudioNodeService.this){return catalogRuntime.displayUse("peers","");}}
        public String getDisplayUseJson() {
            synchronized (AudioNodeService.this) {
                return displayUse.publicStateJson();
            }
        }

        public boolean startDisplayUse(String peer, int width, int height, int refresh,
                int bitrate, boolean automatic) {
            synchronized (AudioNodeService.this) {
                try {
                    displayUse.start(catalogRuntime, peer, width, height, refresh, bitrate, automatic);
                    return true;
                } catch (RuntimeException | org.json.JSONException failure) { return false; }
            }
        }

        public boolean stopDisplayUse() {
            synchronized (AudioNodeService.this) {
                try {
                    displayUse.stop(catalogRuntime);
                    return true;
                } catch (RuntimeException failure) { return false; }
            }
        }

        public String getCameraUseJson() {
            synchronized (AudioNodeService.this) {
                return cameraUse.publicStateJson();
            }
        }

        public boolean startCameraUse(String peer) {
            synchronized (AudioNodeService.this) {
                try {
                    if (!setLocalCapabilityShared(nodeId + ":android.camera", true)) return false;
                    cameraUse.start(catalogRuntime, peer);
                    return true;
                } catch (RuntimeException failed) { return false; }
            }
        }

        public boolean stopCameraUse() {
            synchronized (AudioNodeService.this) {
                try {
                    cameraUse.stop(catalogRuntime);
                    return true;
                } catch (RuntimeException failed) { return false; }
            }
        }
        public String getAudioPeersJson() { synchronized(AudioNodeService.this) { return catalogRuntime.audioUse("peers","","","",0); } }
        public String getAudioUsesJson() {
            synchronized (AudioNodeService.this) {
                return audioUse.publicStateJson();
            }
        }

        public boolean startAudioUse(String peer, String direction) {
            synchronized (AudioNodeService.this) {
                try {
                    if ("microphone".equals(direction)
                            && !setLocalCapabilityShared(nodeId + ":android.microphone", true)) return false;
                    audioUse.start(catalogRuntime, peer, direction);
                    return true;
                } catch (RuntimeException failed) { return false; }
            }
        }

        public boolean stopAudioUse(String direction) {
            synchronized (AudioNodeService.this) {
                try {
                    audioUse.stop(catalogRuntime, direction);
                    return true;
                } catch (RuntimeException failed) { return false; }
            }
        }

        public String getControllerPeersJson() {
            synchronized (AudioNodeService.this) {
                try { return catalogRuntime.controllerUse("peers", "", "", "", false); }
                catch (RuntimeException unavailable) { return "[]"; }
            }
        }
        public String getControllerUseJson() {
            synchronized (AudioNodeService.this) {
                return controllerUse.publicStateJson();
            }
        }

        public boolean startControllerUse(String peerNodeId, String mode) {
            synchronized (AudioNodeService.this) {
                try {
                    if (!setLocalCapabilityShared(nodeId + ":android.gamepad", true)) return false;
                    controllerUse.start(catalogRuntime, peerNodeId, mode);
                    return true;
                } catch (RuntimeException unavailable) { return false; }
            }
        }

        public boolean stopControllerUse() {
            synchronized (AudioNodeService.this) {
                try {
                    controllerUse.stop(catalogRuntime);
                    return true;
                } catch (RuntimeException unavailable) { return false; }
            }
        }

        public boolean setControllerMode(String mode) {
            if (!"touch".equals(mode) && !"motion".equals(mode) && !"stopped".equals(mode)) return false;
            if ("stopped".equals(mode)) stopControllerUse();
            Intent intent = new Intent(AudioNodeService.this, AudioNodeService.class)
                    .setAction(ACTION_CONTROLLER_MODE).putExtra("controllerMode", mode);
            try {
                if ("stopped".equals(mode)) startService(intent);
                else startForegroundService(intent);
                return true;
            } catch (RuntimeException unavailable) { return false; }
        }
        public String audioPeerSettingsJson(String direction) {
            synchronized (AudioNodeService.this) {
                if (!"microphone".equals(direction) && !"speaker".equals(direction)) return "";
                try {
                    return AndroidAudioPeer.savedSettingsJson(direction,
                            AndroidAudioPeerStore.load(AudioNodeService.this, direction));
                } catch (RuntimeException invalidSettings) {
                    return "";
                }
            }
        }

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
            return catalogModule.selectedIds();
        }

        public String pairingStoreState() {
            return control.state();
        }

        public int pairedPeerCount() {
            return control.peerCount();
        }

        public String pairedPeerMetadataJson() {
            synchronized (AudioNodeService.this) {
                return control.peerMetadataJson();
            }
        }

        public boolean authenticatedCatalogConnected() {
            return control.listener() != null && control.listener().connected();
        }

        public String controlConnectionFailure() {
            if (control.listener() == null) return "控制监听未就绪";
            String failure = control.listener().failure();
            return failure == null ? "" : failure.substring(0, Math.min(failure.length(), 512));
        }

        public boolean connectPairedPeerAt(String host, int port) {
            synchronized (AudioNodeService.this) {
                return control.connect(host, port);
            }
        }

        public boolean discoveryAnnouncing() {
            return control.listener() != null && control.listener().discoveryAnnouncing();
        }

        public boolean setBackgroundWaitingEnabled(boolean enabled) {
            if (enabled && !controlListenerAvailable()) return false;
            try {
                if (enabled) {
                    startForegroundAction(AudioNodeService.this, ACTION_ENABLE_BACKGROUND_WAITING);
                } else {
                    startServiceAction(AudioNodeService.this, ACTION_DISABLE_BACKGROUND_WAITING);
                }
                return true;
            } catch (RuntimeException unavailable) {
                return false;
            }
        }

        public int controlListenPort() {
            if (control.listener() == null) return 0;
            try {
                return control.listener().port();
            } catch (RuntimeException unavailable) {
                return 0;
            }
        }

        public boolean reloadControlPairing() {
            return AudioNodeService.this.reloadControlPairing();
        }

        public String enrollmentStatusJson() {
            return filelessPairing == null ? "{\"phase\":\"idle\",\"message\":\"节点服务尚未就绪\"}" : filelessPairing.status();
        }

        public String enrollmentActionJson(String request) {
            try {
                if (filelessPairing == null) throw new IllegalStateException("节点服务尚未就绪");
                return filelessPairing.action(request);
            } catch (RuntimeException error) { return AndroidFilelessPairing.errorStatus(error.getMessage()); }
        }

        public CapabilityCatalogSnapshot publicationCatalog() {
            return catalogModule.publication(
                    AudioNodeService.this.capabilityCatalogSnapshot());
        }

        public String publicationCatalogJson() {
            return AudioNodeService.this.publicationCatalogJson();
        }

        public boolean setLocalCapabilityShared(String capabilityId, boolean selected) {
            CapabilityCatalogSnapshot catalog = AudioNodeService.this.capabilityCatalogSnapshot();
            boolean accepted = catalogModule.select(
                    catalog,
                    capabilityId,
                    selected,
                    AudioNodeService.this::replaceNativePublication);
            if (accepted) {
                // A started Service, not merely a bound foreground Service, owns
                // the waiting listener when the Activity leaves the foreground.
                if (!catalogModule.selectedIds().isEmpty()) {
                    startForegroundAction(AudioNodeService.this, ACTION_REFRESH_PUBLICATION);
                } else {
                    startServiceAction(AudioNodeService.this, ACTION_REFRESH_PUBLICATION);
                }
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

        public boolean gamepadMotionEnabled() { return gamepadMotionEnabled; }

        public int hapticsStrength() { return hapticsAdapter.strengthPercent(); }
        public long hapticsExecutedFrames() { return hapticsDataWorker == null ? 0 : hapticsDataWorker.executedFrames(); }
        public boolean hapticsNetworkActive() { return hapticsDataWorker != null && hapticsDataWorker.isAlive(); }

        public boolean hapticsAvailable() { return hapticsAdapter.available(); }

        public boolean setHapticsStrength(int percent) {
            synchronized (AudioNodeService.this) {
                return applyControllerPreferences(gamepadMotionEnabled, percent);
            }
        }

        public boolean gamepadMotionAvailable() {
            return gamepadMotionEnabled
                    && gamepadController.lifecycle().state() == CapabilityLifecycleState.ACTIVE
                    && imuAdapter.latestSample().accelerationTimestampNanos() > 0;
        }

        public boolean setGamepadMotionEnabled(boolean enabled) {
            synchronized (AudioNodeService.this) {
                return applyControllerPreferences(enabled, hapticsAdapter.strengthPercent());
            }
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

        public String touchpadRequestsJson() {
            return catalogRuntime == null ? "[]" : catalogRuntime.touchpadRequestsJson();
        }

        public boolean resolveTouchpadRequest(String routeId, boolean allow) {
            return catalogRuntime != null && catalogRuntime.resolveTouchpadRequest(routeId, allow);
        }

        public boolean resolveTouchpadRequestRemembered(String routeId, boolean allow, boolean remember) {
            synchronized (AudioNodeService.this) {
                return resourceConsent.resolve(catalogRuntime, control.state(),
                        control.directory(), routeId, allow, remember);
            }
        }

        public boolean clearRememberedTouchpadPermissions() {
            synchronized (AudioNodeService.this) {
                resourceConsent.clear();
                return true;
            }
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
