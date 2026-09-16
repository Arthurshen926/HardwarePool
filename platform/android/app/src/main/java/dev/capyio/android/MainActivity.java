package dev.capyio.android;

import android.Manifest;
import android.annotation.SuppressLint;
import android.app.Activity;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.ServiceConnection;
import android.content.pm.PackageManager;
import android.os.Build;
import android.os.Bundle;
import android.os.Handler;
import android.os.IBinder;
import android.os.Looper;
import android.webkit.JavascriptInterface;
import android.webkit.WebView;
import android.view.View;
import android.view.WindowInsets;
import android.widget.Button;
import android.widget.TextView;

import dev.capyio.android.contract.ActualAudioFormat;
import dev.capyio.android.contract.AudioCapabilityKind;
import dev.capyio.android.contract.AudioCapabilitySnapshot;
import dev.capyio.android.contract.AudioCapabilityState;
import dev.capyio.android.contract.AudioNodeSnapshot;
import dev.capyio.android.contract.CapabilityLifecycleSnapshot;
import dev.capyio.android.contract.CapabilityLifecycleState;
import dev.capyio.android.contract.GamepadControlsSnapshot;
import dev.capyio.android.contract.TouchpadContactSnapshot;
import io.capyio.camera.lab.UnifiedCameraNodeFacade;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.atomic.AtomicBoolean;

public final class MainActivity extends Activity {
    private static final int REQUEST_AUDIO_PERMISSIONS = 8101;
    private static final int REQUEST_CAMERA_PERMISSIONS = 8102;
    private static final long REFRESH_INTERVAL_MILLIS = 250;
    private static final String SHARED_UI_URL = AndroidAssetWebViewClient.ORIGIN
            + "/capyio-ui/index.html?view=quick";

    private final Handler handler = new Handler(Looper.getMainLooper());
    private final AtomicBoolean refreshQueued = new AtomicBoolean();
    private final Runnable refreshTask = () -> {
        refreshQueued.set(false);
        renderLatest();
    };

    private final ServiceConnection serviceConnection = new ServiceConnection() {
        @Override
        public void onServiceConnected(ComponentName name, IBinder service) {
            binder = (AudioNodeService.NodeBinder) service;
            binder.setStateListener(MainActivity.this::scheduleRefresh);
            scheduleRefresh(0);
        }

        @Override
        public void onServiceDisconnected(ComponentName name) {
            if (binder != null) {
                binder.setStateListener(null);
            }
            binder = null;
            handler.removeCallbacks(refreshTask);
            refreshQueued.set(false);
            dispatchSharedUiState();
        }
    };

    private volatile AudioNodeService.NodeBinder binder;
    private AudioCapabilityKind pendingStart;
    private boolean pendingCameraStart;
    private boolean bound;

    private TextView nodeIdView;
    private TextView permissionView;
    private TextView speakerTransportView;
    private TextView microphoneTransportView;
    private CapabilityViews microphoneViews;
    private CapabilityViews speakerViews;
    private View nativeControls;
    private WebView sharedUi;
    private boolean sharedUiLoaded;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        setContentView(R.layout.activity_main);
        configureSystemBarInsets();
        nativeControls = findViewById(R.id.native_controls);
        sharedUi = findViewById(R.id.shared_ui);
        configureSharedUi();
        nodeIdView = findViewById(R.id.node_id);
        permissionView = findViewById(R.id.permission_state);
        microphoneViews = new CapabilityViews(
                findViewById(R.id.microphone_state),
                findViewById(R.id.microphone_format),
                findViewById(R.id.microphone_frames),
                findViewById(R.id.microphone_problem),
                findViewById(R.id.start_microphone),
                findViewById(R.id.stop_microphone));
        speakerViews = new CapabilityViews(
                findViewById(R.id.speaker_state),
                findViewById(R.id.speaker_format),
                findViewById(R.id.speaker_frames),
                findViewById(R.id.speaker_problem),
                findViewById(R.id.start_speaker),
                findViewById(R.id.stop_speaker));
        speakerTransportView = findViewById(R.id.speaker_transport);
        microphoneTransportView = findViewById(R.id.microphone_transport);

        microphoneViews.start.setOnClickListener(view -> requestStart(
                AudioCapabilityKind.MICROPHONE_SOURCE));
        microphoneViews.stop.setOnClickListener(view ->
                AudioNodeService.stopMicrophone(this));
        speakerViews.start.setOnClickListener(view -> requestStart(
                AudioCapabilityKind.SPEAKER_SINK));
        speakerViews.stop.setOnClickListener(view -> AudioNodeService.stopSpeaker(this));
        findViewById(R.id.open_shared_ui).setOnClickListener(view -> showSharedUi());
        renderPermissionState(null);
        showSharedUi();
    }

    @SuppressWarnings("deprecation")
    private void configureSystemBarInsets() {
        View root = findViewById(R.id.activity_root);
        root.setOnApplyWindowInsetsListener((view, windowInsets) -> {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                android.graphics.Insets bars = windowInsets.getInsets(
                        WindowInsets.Type.systemBars() | WindowInsets.Type.displayCutout());
                android.graphics.Insets stableTop = windowInsets.getInsetsIgnoringVisibility(
                        WindowInsets.Type.statusBars() | WindowInsets.Type.displayCutout());
                view.setPadding(
                        bars.left,
                        Math.max(bars.top, stableTop.top),
                        bars.right,
                        bars.bottom);
            } else {
                view.setPadding(
                        windowInsets.getSystemWindowInsetLeft(),
                        windowInsets.getStableInsetTop(),
                        windowInsets.getSystemWindowInsetRight(),
                        windowInsets.getSystemWindowInsetBottom());
            }
            return windowInsets;
        });
        root.requestApplyInsets();
    }

    @SuppressLint("SetJavaScriptEnabled")
    @SuppressWarnings("deprecation")
    private void configureSharedUi() {
        if (BuildConfig.DEBUG) {
            WebView.setWebContentsDebuggingEnabled(true);
        }
        sharedUi.getSettings().setJavaScriptEnabled(true);
        sharedUi.getSettings().setAllowContentAccess(false);
        sharedUi.getSettings().setAllowFileAccess(false);
        sharedUi.getSettings().setAllowFileAccessFromFileURLs(false);
        sharedUi.getSettings().setAllowUniversalAccessFromFileURLs(false);
        sharedUi.getSettings().setBlockNetworkLoads(false);
        sharedUi.addJavascriptInterface(new ObserverBridge(), "CapyIOAndroid");
        sharedUi.setWebViewClient(new AndroidAssetWebViewClient(getAssets()));
    }

    private void showSharedUi() {
        nativeControls.setVisibility(View.GONE);
        sharedUi.setVisibility(View.VISIBLE);
        if (!sharedUiLoaded) {
            sharedUiLoaded = true;
            sharedUi.loadUrl(SHARED_UI_URL);
        } else {
            dispatchSharedUiState();
        }
    }

    private void hideSharedUi() {
        sharedUi.setVisibility(View.GONE);
        nativeControls.setVisibility(View.VISIBLE);
    }

    @Override
    protected void onStart() {
        super.onStart();
        bound = bindService(
                new Intent(this, AudioNodeService.class),
                serviceConnection,
                Context.BIND_AUTO_CREATE);
    }

    @Override
    protected void onStop() {
        handler.removeCallbacks(refreshTask);
        refreshQueued.set(false);
        if (binder != null) {
            binder.setStateListener(null);
            binder = null;
        }
        if (bound) {
            unbindService(serviceConnection);
            bound = false;
        }
        super.onStop();
    }

    @Override
    protected void onDestroy() {
        sharedUi.removeJavascriptInterface("CapyIOAndroid");
        sharedUi.destroy();
        super.onDestroy();
    }

    @Override
    public void onRequestPermissionsResult(
            int requestCode,
            String[] permissions,
            int[] grantResults) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults);
        if (requestCode == REQUEST_CAMERA_PERMISSIONS) {
            boolean requested = pendingCameraStart;
            pendingCameraStart = false;
            if (requested && cameraPermissionsGranted()) {
                UnifiedCameraNodeFacade.start(this);
                renderPermissionState(null);
            } else {
                renderPermissionState(getString(R.string.permission_denied));
            }
            dispatchSharedUiState();
            return;
        }
        if (requestCode != REQUEST_AUDIO_PERMISSIONS) {
            return;
        }
        AudioCapabilityKind requested = pendingStart;
        pendingStart = null;
        if (requested != null && requiredPermissionsGranted(requested)) {
            startCapability(requested);
            renderPermissionState(null);
        } else {
            renderPermissionState(getString(R.string.permission_denied));
        }
    }

    private void requestStart(AudioCapabilityKind kind) {
        List<String> missing = new ArrayList<>();
        if (kind == AudioCapabilityKind.MICROPHONE_SOURCE
                && checkSelfPermission(Manifest.permission.RECORD_AUDIO)
                        != PackageManager.PERMISSION_GRANTED) {
            missing.add(Manifest.permission.RECORD_AUDIO);
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU
                && checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS)
                        != PackageManager.PERMISSION_GRANTED) {
            missing.add(Manifest.permission.POST_NOTIFICATIONS);
        }
        if (!missing.isEmpty()) {
            pendingStart = kind;
            requestPermissions(missing.toArray(new String[0]), REQUEST_AUDIO_PERMISSIONS);
            return;
        }
        startCapability(kind);
    }

    private boolean requiredPermissionsGranted(AudioCapabilityKind kind) {
        boolean microphoneGranted = kind != AudioCapabilityKind.MICROPHONE_SOURCE
                || checkSelfPermission(Manifest.permission.RECORD_AUDIO)
                        == PackageManager.PERMISSION_GRANTED;
        boolean notificationGranted = Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU
                || checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS)
                        == PackageManager.PERMISSION_GRANTED;
        return microphoneGranted && notificationGranted;
    }

    private void requestStartCamera() {
        List<String> missing = new ArrayList<>();
        if (checkSelfPermission(Manifest.permission.CAMERA) != PackageManager.PERMISSION_GRANTED) {
            missing.add(Manifest.permission.CAMERA);
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU
                && checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS)
                        != PackageManager.PERMISSION_GRANTED) {
            missing.add(Manifest.permission.POST_NOTIFICATIONS);
        }
        if (!missing.isEmpty()) {
            pendingCameraStart = true;
            requestPermissions(missing.toArray(new String[0]), REQUEST_CAMERA_PERMISSIONS);
            return;
        }
        UnifiedCameraNodeFacade.start(this);
    }

    private boolean cameraPermissionsGranted() {
        boolean cameraGranted = checkSelfPermission(Manifest.permission.CAMERA)
                == PackageManager.PERMISSION_GRANTED;
        boolean notificationGranted = Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU
                || checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS)
                        == PackageManager.PERMISSION_GRANTED;
        return cameraGranted && notificationGranted;
    }

    private void startCapability(AudioCapabilityKind kind) {
        if (kind == AudioCapabilityKind.MICROPHONE_SOURCE) {
            AudioNodeService.startMicrophone(this);
        } else {
            AudioNodeService.startSpeaker(this);
        }
    }

    private void renderLatest() {
        if (binder == null) {
            return;
        }
        AudioNodeSnapshot snapshot = binder.snapshot();
        nodeIdView.setText(getString(R.string.node_id_format, snapshot.nodeId()));
        renderCapability(microphoneViews, snapshot.microphone());
        renderCapability(speakerViews, snapshot.speaker());
        MicrophoneSourceAdapter.TransportMetrics microphoneTransport =
                binder.microphoneTransportMetrics();
        microphoneTransportView.setText(getString(
                R.string.microphone_transport_format,
                microphoneTransport.packetsEmitted,
                microphoneTransport.packetsSent,
                microphoneTransport.datagramsSent,
                microphoneTransport.packetsDropped,
                microphoneTransport.bufferedBytes));
        SpeakerSinkAdapter.TransportMetrics transport = binder.speakerTransportMetrics();
        speakerTransportView.setText(getString(
                R.string.speaker_transport_format,
                transport.datagramsReceived,
                transport.wrongPeerDatagrams,
                transport.malformedDatagrams,
                transport.completedPackets,
                transport.partialEvictions,
                transport.fullPacketDrops + transport.fullByteDrops));
        renderPermissionState(null);
        dispatchSharedUiState();
        if (snapshot.microphone().state().ownsForegroundLifecycle()
                || snapshot.speaker().state().ownsForegroundLifecycle()) {
            scheduleRefresh();
        }
    }

    private void scheduleRefresh() {
        scheduleRefresh(REFRESH_INTERVAL_MILLIS);
    }

    private void scheduleRefresh(long delayMillis) {
        if (refreshQueued.compareAndSet(false, true)) {
            handler.postDelayed(refreshTask, delayMillis);
        }
    }

    private void renderCapability(CapabilityViews views, AudioCapabilitySnapshot capability) {
        views.state.setText(getString(R.string.state_format, capability.state().name()));
        ActualAudioFormat actualFormat = capability.actualFormat();
        if (actualFormat == null) {
            views.format.setText(R.string.format_pending);
        } else {
            views.format.setText(getString(
                    R.string.format_value,
                    actualFormat.sampleRateHz(),
                    actualFormat.channelCount(),
                    actualFormat.encoding(),
                    actualFormat.framesPerBuffer()));
        }
        views.frames.setText(getString(R.string.frames_format, capability.framesProcessed()));
        if (capability.problemCode() == null) {
            views.problem.setVisibility(View.GONE);
        } else {
            views.problem.setText(getString(R.string.problem_format, capability.problemCode()));
            views.problem.setVisibility(View.VISIBLE);
        }
        AudioCapabilityState state = capability.state();
        views.start.setEnabled(
                state == AudioCapabilityState.STOPPED || state == AudioCapabilityState.FAILED);
        views.stop.setEnabled(state != AudioCapabilityState.STOPPED);
    }

    private void renderPermissionState(String override) {
        if (override != null) {
            permissionView.setText(override);
            return;
        }
        boolean microphoneGranted = checkSelfPermission(Manifest.permission.RECORD_AUDIO)
                == PackageManager.PERMISSION_GRANTED;
        boolean notificationGranted = Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU
                || checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS)
                        == PackageManager.PERMISSION_GRANTED;
        permissionView.setText(microphoneGranted && notificationGranted
                ? R.string.permission_ready
                : R.string.permission_required);
    }

    private void dispatchSharedUiState() {
        if (sharedUiLoaded) {
            sharedUi.evaluateJavascript(
                    "window.dispatchEvent(new Event('capyio-native-state'))",
                    null);
        }
    }

    private final class ObserverBridge {
        @JavascriptInterface
        public String getDiagnosticsJson() {
            try {
                org.json.JSONObject result = new org.json.JSONObject();
                result.put("backend", "Android Node");
                result.put("version", BuildConfig.VERSION_NAME + " (" + BuildConfig.VERSION_CODE + ")");
                result.put("revision", BuildConfig.SOURCE_REVISION);
                result.put("builtAt", BuildConfig.BUILT_AT);
                org.json.JSONArray checks = new org.json.JSONArray();
                checks.put(new org.json.JSONObject()
                        .put("label", "Node 服务")
                        .put("detail", binder == null ? "界面尚未绑定服务" : "界面已绑定；不代表远端连接或数据传输成功"));
                checks.put(new org.json.JSONObject()
                        .put("label", "Android 系统")
                        .put("detail", "Android " + Build.VERSION.RELEASE));
                result.put("checks", checks);
                return result.toString();
            } catch (org.json.JSONException error) {
                return "{}";
            }
        }

        @JavascriptInterface
        public boolean openNativeDiagnostics() {
            handler.post(MainActivity.this::hideSharedUi);
            return true;
        }

        @JavascriptInterface
        public String getSnapshotJson() {
            AudioNodeService.NodeBinder current = binder;
            if (current == null) {
                return AndroidUiSnapshotJson.encodeDisconnected(
                        Build.VERSION.RELEASE,
                        BuildConfig.VERSION_NAME);
            }
            return AndroidUiSnapshotJson.encode(
                    current.capabilityCatalog(),
                    Build.VERSION.RELEASE,
                    BuildConfig.VERSION_NAME,
                    current.selectedSharedCapabilityIds());
        }

        @JavascriptInterface
        public boolean setLocalCapabilityShared(String capabilityId, boolean selected) {
            AudioNodeService.NodeBinder current = binder;
            return current != null && current.setLocalCapabilityShared(capabilityId, selected);
        }

        @JavascriptInterface
        public String getQuickActionsJson() {
            AudioNodeService.NodeBinder current = binder;
            if (current == null) {
                return "[]";
            }
            return AndroidUiSnapshotJson.encodeQuickActions(
                    current.snapshot(),
                    requiredPermissionsGranted(AudioCapabilityKind.MICROPHONE_SOURCE),
                    requiredPermissionsGranted(AudioCapabilityKind.SPEAKER_SINK),
                    current.cameraLifecycle(),
                    cameraPermissionsGranted(),
                    current.imuLifecycle(),
                    current.gamepadLifecycle(),
                    current.touchpadLifecycle(),
                    current.hapticsLifecycle());
        }

        @JavascriptInterface
        public String getAudioStateJson() {
            AudioNodeService.NodeBinder current = binder;
            if (current == null) {
                return "";
            }
            return AndroidUiSnapshotJson.encodeAudioState(
                    current.snapshot(),
                    requiredPermissionsGranted(AudioCapabilityKind.MICROPHONE_SOURCE),
                    requiredPermissionsGranted(AudioCapabilityKind.SPEAKER_SINK));
        }

        @JavascriptInterface
        public String getCameraSettingsJson() {
            return UnifiedCameraNodeFacade.settingsJson(cameraPermissionsGranted());
        }

        @JavascriptInterface
        public boolean refreshCameraSources() {
            if (!cameraPermissionsGranted()) {
                return false;
            }
            boolean accepted = UnifiedCameraNodeFacade.refreshSources(MainActivity.this);
            dispatchSharedUiState();
            return accepted;
        }

        @JavascriptInterface
        public boolean selectCameraSource(String sourceKey) {
            boolean accepted = UnifiedCameraNodeFacade.selectSource(sourceKey);
            if (accepted) {
                dispatchSharedUiState();
            }
            return accepted;
        }

        @JavascriptInterface
        public boolean selectCameraQuality(String quality) {
            boolean accepted = UnifiedCameraNodeFacade.selectQuality(quality);
            if (accepted) {
                dispatchSharedUiState();
            }
            return accepted;
        }

        @JavascriptInterface
        public String getGamepadStateJson() {
            AudioNodeService.NodeBinder current = binder;
            if (current == null) {
                return "";
            }
            return AndroidUiSnapshotJson.encodeGamepadState(
                    current.gamepadLifecycle(),
                    current.gamepadControls(),
                    current.controllerRouteEndpoint(),
                    current.controllerRouteRunning(),
                    current.controllerRouteProblemCode());
        }

        @JavascriptInterface
        public boolean configureControllerRoute(String host, int port, String token) {
            AudioNodeService.NodeBinder current = binder;
            return current != null && current.configureControllerRoute(host, port, token);
        }

        @JavascriptInterface
        public synchronized boolean updateGamepad(
                String kind, String target, int x, int y, boolean pressed) {
            AudioNodeService.NodeBinder current = binder;
            if (current == null) {
                return false;
            }
            CapabilityLifecycleSnapshot lifecycle = current.gamepadLifecycle();
            if (lifecycle.state() != CapabilityLifecycleState.ACTIVE) {
                return false;
            }
            GamepadControlsSnapshot next = AndroidGamepadUiCommand.apply(
                    kind, target, x, y, pressed, current.gamepadControls());
            return next != null && current.submitGamepad(lifecycle.generation(), next);
        }

        @JavascriptInterface
        public String getTouchpadStateJson() {
            AudioNodeService.NodeBinder current = binder;
            if (current == null) {
                return "";
            }
            return AndroidUiSnapshotJson.encodeTouchpadState(
                    current.touchpadLifecycle(), current.touchpadControls());
        }

        @JavascriptInterface
        public synchronized boolean updateTouchpad(String encodedContacts) {
            AudioNodeService.NodeBinder current = binder;
            if (current == null) {
                return false;
            }
            CapabilityLifecycleSnapshot lifecycle = current.touchpadLifecycle();
            if (lifecycle.state() != CapabilityLifecycleState.ACTIVE) {
                return false;
            }
            List<TouchpadContactSnapshot> contacts =
                    AndroidTouchpadUiFrame.parse(encodedContacts);
            return contacts != null
                    && current.submitTouchpad(lifecycle.generation(), contacts);
        }

        @JavascriptInterface
        public boolean invokeQuickAction(String actionId, String operation) {
            AudioNodeService.NodeBinder current = binder;
            if (current == null) {
                return false;
            }
            if (AndroidCameraCommand.ACTION_ID.equals(actionId)) {
                AndroidCameraCommand cameraCommand = AndroidCameraCommand.parse(
                        actionId, operation, current.cameraLifecycle().state());
                if (cameraCommand == null) {
                    return false;
                }
                handler.post(() -> {
                    if (cameraCommand.operation() == AndroidCameraCommand.Operation.STOP) {
                        UnifiedCameraNodeFacade.stop(MainActivity.this);
                    } else {
                        requestStartCamera();
                    }
                });
                return true;
            }
            AudioNodeSnapshot audio = current.snapshot();
            AudioCapabilitySnapshot audioCapability;
            if ("capyio.quick-action.android-microphone".equals(actionId)) {
                audioCapability = audio.microphone();
            } else if ("capyio.quick-action.android-speaker".equals(actionId)) {
                audioCapability = audio.speaker();
            } else {
                audioCapability = null;
            }
            if (audioCapability != null) {
                AndroidAudioCommand audioCommand = AndroidAudioCommand.parse(
                        actionId, operation, audioCapability.state());
                if (audioCommand == null) {
                    return false;
                }
                handler.post(() -> {
                    if (audioCommand.operation() == AndroidAudioCommand.Operation.STOP) {
                        if (audioCommand.target() == AudioCapabilityKind.MICROPHONE_SOURCE) {
                            AudioNodeService.stopMicrophone(MainActivity.this);
                        } else {
                            AudioNodeService.stopSpeaker(MainActivity.this);
                        }
                    } else {
                        requestStart(audioCommand.target());
                    }
                });
                return true;
            }
            CapabilityLifecycleSnapshot lifecycle;
            if ("capyio.quick-action.android-imu".equals(actionId)) {
                lifecycle = current.imuLifecycle();
            } else if ("capyio.quick-action.android-gamepad".equals(actionId)) {
                lifecycle = current.gamepadLifecycle();
            } else if ("capyio.quick-action.android-touchpad".equals(actionId)) {
                lifecycle = current.touchpadLifecycle();
            } else if ("capyio.quick-action.android-haptics".equals(actionId)) {
                lifecycle = current.hapticsLifecycle();
            } else {
                return false;
            }
            AndroidCapabilityCommand command = AndroidCapabilityCommand.parse(
                    actionId,
                    operation,
                    lifecycle.state());
            if (command == null) {
                return false;
            }
            switch (command.target()) {
                case IMU -> {
                    if (command.operation() == AndroidCapabilityCommand.Operation.STOP) {
                        current.stopImu();
                    } else {
                        current.startImu();
                    }
                }
                case GAMEPAD -> {
                    if (command.operation() == AndroidCapabilityCommand.Operation.STOP) {
                        current.stopGamepad();
                    } else {
                        current.startGamepad();
                    }
                }
                case TOUCHPAD -> {
                    if (command.operation() == AndroidCapabilityCommand.Operation.STOP) {
                        current.stopTouchpad();
                    } else {
                        current.startTouchpad();
                    }
                }
                case HAPTICS -> {
                    if (command.operation() == AndroidCapabilityCommand.Operation.STOP) {
                        current.stopHaptics();
                    } else {
                        current.startHaptics();
                    }
                }
            }
            return true;
        }
    }

    private static final class CapabilityViews {
        private final TextView state;
        private final TextView format;
        private final TextView frames;
        private final TextView problem;
        private final Button start;
        private final Button stop;

        private CapabilityViews(
                TextView state,
                TextView format,
                TextView frames,
                TextView problem,
                Button start,
                Button stop) {
            this.state = state;
            this.format = format;
            this.frames = frames;
            this.problem = problem;
            this.start = start;
            this.stop = stop;
        }
    }
}
