package dev.capyio.android;

import android.Manifest;
import android.annotation.SuppressLint;
import android.app.Activity;
import android.app.AlertDialog;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.ServiceConnection;
import android.content.pm.PackageManager;
import android.content.res.Configuration;
import android.os.Build;
import android.os.Bundle;
import android.os.Handler;
import android.os.IBinder;
import android.os.Looper;
import android.os.VibrationEffect;
import android.os.Vibrator;
import android.os.VibratorManager;
import android.webkit.JavascriptInterface;
import android.webkit.WebView;
import android.graphics.Rect;
import android.view.MotionEvent;
import android.view.View;
import android.view.WindowInsets;
import android.view.WindowInsetsController;
import android.view.WindowManager;
import android.widget.Button;
import android.widget.TextView;
import android.widget.Toast;

import dev.capyio.android.contract.ActualAudioFormat;
import dev.capyio.android.contract.AudioCapabilityKind;
import dev.capyio.android.contract.AudioCapabilitySnapshot;
import dev.capyio.android.contract.AudioCapabilityState;
import dev.capyio.android.contract.AudioNodeSnapshot;
import dev.capyio.android.contract.CapabilityLifecycleSnapshot;
import dev.capyio.android.contract.CapabilityLifecycleState;
import dev.capyio.android.contract.GamepadControlsSnapshot;
import dev.capyio.android.contract.TouchpadContactSnapshot;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.atomic.AtomicBoolean;

public final class MainActivity extends Activity {
    private static final int REQUEST_AUDIO_PERMISSIONS = 8101;
    private static final int REQUEST_CAMERA_PERMISSIONS = 8102;
    private static final int REQUEST_CONNECTED_DEVICE_NOTIFICATION = 8103;
    private static final int REQUEST_DISPLAY_PROVISIONING = 8104;
    private static final int REQUEST_PAIRING_INVITATION = 8105;
    private static final long REFRESH_INTERVAL_MILLIS = 250;
    private static final String TOUCHPAD_HAPTIC_PREFS = "touchpad_haptics";
    private static final String TOUCHPAD_HAPTIC_STRENGTH = "strength";
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
    private long pendingCameraSettingsRevision;
    private AudioNodeService.NodeBinder pendingCameraOwner;
    private AndroidCapabilityCommand.Target pendingConnectedDeviceStart;
    private boolean bound;
    private volatile boolean uiInputActive;

    private TextView nodeIdView;
    private TextView permissionView;
    private TextView speakerTransportView;
    private TextView microphoneTransportView;
    private CapabilityViews microphoneViews;
    private CapabilityViews speakerViews;
    private View nativeControls;
    private WebView sharedUi;
    private boolean sharedUiLoaded;
    private boolean touchpadImmersive;
    private AndroidPairingImportPlan pendingPairingImport;
    private AlertDialog pairingImportDialog;
    private volatile int touchpadRawDowns;
    private volatile int touchpadRawPointerDowns;
    private volatile int touchpadRawCancels;
    private volatile int touchpadRawMaxPointers;

    @Override
    public boolean dispatchTouchEvent(MotionEvent event) {
        if (touchpadImmersive) {
            int action = event.getActionMasked();
            if (action == MotionEvent.ACTION_DOWN) touchpadRawDowns++;
            else if (action == MotionEvent.ACTION_POINTER_DOWN) touchpadRawPointerDowns++;
            else if (action == MotionEvent.ACTION_CANCEL) touchpadRawCancels++;
            touchpadRawMaxPointers = Math.max(touchpadRawMaxPointers, event.getPointerCount());
        }
        return super.dispatchTouchEvent(event);
    }

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

    @Override
    public void onConfigurationChanged(Configuration configuration) {
        super.onConfigurationChanged(configuration);
        // Keep the existing WebView, input surface and service binding on rotation.
        // Release controls whose pointer coordinates belonged to the previous size.
        AudioNodeService.NodeBinder current = binder;
        if (current != null) current.releaseUiInputs();
        sharedUi.post(() -> {
            if (isDestroyed() || isFinishing()) return;
            if (touchpadImmersive) {
                sharedUi.setSystemGestureExclusionRects(List.of(
                        new Rect(0, 0, sharedUi.getWidth(), sharedUi.getHeight())));
            }
            findViewById(R.id.activity_root).requestApplyInsets();
        });
    }

    @SuppressWarnings("deprecation")
    private void configureSystemBarInsets() {
        View root = findViewById(R.id.activity_root);
        root.setOnApplyWindowInsetsListener((view, windowInsets) -> {
            if (touchpadImmersive) {
                view.setPadding(0, 0, 0, 0);
                return windowInsets;
            }
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
        scheduleRefresh(0);
    }

    @Override
    protected void onStart() {
        super.onStart();
        try {
            AudioNodeService.restoreBackgroundWaiting(this);
        } catch (RuntimeException unavailable) {
            // Android may reject a foreground start; the next explicit toggle
            // will show failure rather than claiming the Node is online.
        }
        bound = bindService(
                new Intent(this, AudioNodeService.class),
                serviceConnection,
                Context.BIND_AUTO_CREATE);
    }

    @Override
    protected void onResume() {
        super.onResume();
        uiInputActive = true;
    }

    @Override
    protected void onPause() {
        uiInputActive = false;
        AudioNodeService.NodeBinder current = binder;
        if (current != null) {
            current.releaseUiInputs();
        }
        super.onPause();
    }

    @Override
    protected void onStop() {
        cancelPairingImport();
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
        cancelPairingImport();
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
                AudioNodeService.NodeBinder current = binder;
                if (current != null && current == pendingCameraOwner) {
                    current.startLocalCamera(pendingCameraSettingsRevision);
                }
                renderPermissionState(null);
            } else {
                renderPermissionState(getString(R.string.permission_denied));
            }
            pendingCameraOwner = null;
            dispatchSharedUiState();
            return;
        }
        if (requestCode == REQUEST_CONNECTED_DEVICE_NOTIFICATION) {
            AndroidCapabilityCommand.Target requested = pendingConnectedDeviceStart;
            pendingConnectedDeviceStart = null;
            if (requested != null && notificationPermissionGranted()) {
                startConnectedDeviceCapability(requested);
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

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (requestCode == REQUEST_PAIRING_INVITATION) {
            if (resultCode != RESULT_OK || data == null || data.getData() == null) {
                Toast.makeText(this, "未导入配对邀请", Toast.LENGTH_SHORT).show();
                return;
            }
            cancelPairingImport();
            try (java.io.InputStream input = getContentResolver().openInputStream(data.getData())) {
                showPairingImportPreview(AndroidPairingInvitationInstaller.prepare(this, input));
            } catch (AndroidPairingImportPlan.RejectedException rejected) {
                Toast.makeText(this, rejected.getMessage(), Toast.LENGTH_LONG).show();
            } catch (java.io.IOException | RuntimeException invalid) {
                Toast.makeText(this, "配对邀请无效或不属于本设备", Toast.LENGTH_LONG).show();
            }
            dispatchSharedUiState();
            return;
        }
        if (requestCode != REQUEST_DISPLAY_PROVISIONING) return;
        if (resultCode != RESULT_OK || data == null || data.getData() == null) {
            Toast.makeText(this, "未导入显示连接包", Toast.LENGTH_SHORT).show();
            return;
        }
        try (java.io.InputStream input = getContentResolver().openInputStream(data.getData())) {
            String mode = dev.capyio.display.app.DisplayProvisioningBundleInstaller.install(
                    getFilesDir(), input);
            Toast.makeText(this, "显示连接包已导入：" + mode, Toast.LENGTH_LONG).show();
        } catch (java.io.IOException | RuntimeException invalid) {
            Toast.makeText(this, "显示连接包导入失败，请刷新确认当前配置", Toast.LENGTH_LONG).show();
        }
        dispatchSharedUiState();
    }

    private void showPairingImportPreview(AndroidPairingImportPlan plan) {
        if (isDestroyed() || isFinishing()) {
            plan.close();
            return;
        }
        pendingPairingImport = plan;
        AndroidPairingImportPlan.Peer previous = plan.previous();
        AndroidPairingImportPlan.Peer incoming = plan.incoming();
        String message = previous == null
                ? "即将配对：" + pairingPeerLabel(incoming)
                : "当前保存：" + pairingPeerLabel(previous)
                        + "\n\n即将导入：" + pairingPeerLabel(incoming);
        if (previous != null) {
            message += "\n\n确认后将更新此节点的配对并中断现有连接。请核对 Windows 显示的配对批次。";
            if (incoming.pairedAtMs() < previous.pairedAtMs()) {
                message += "\n这份邀请比当前配对更旧，请确认它确实来自要连接的 Windows 账号。";
            }
        } else {
            message += "\n\n请确认邀请来自你要连接的设备。配对不代表允许使用全部硬件。";
        }
        pairingImportDialog = new AlertDialog.Builder(this)
                .setTitle(previous == null ? "确认导入配对" : "确认更新配对")
                .setMessage(message)
                .setNegativeButton("取消", (dialog, which) -> {})
                .setPositiveButton(previous == null ? "导入配对" : "确认更新", (dialog, which) -> {
                    if (pendingPairingImport != plan) return;
                    try {
                        AndroidPairingInvitationInstaller.confirm(this, plan);
                        boolean reloaded;
                        try { reloaded = binder != null && binder.reloadControlPairing(); }
                        catch (RuntimeException unavailable) { reloaded = false; }
                        Toast.makeText(this,
                                reloaded ? "配对已保存；请重新连接"
                                        : "配对已保存，请重新打开应用",
                                Toast.LENGTH_LONG).show();
                    } catch (AndroidPairingImportPlan.RejectedException rejected) {
                        Toast.makeText(this, rejected.getMessage(), Toast.LENGTH_LONG).show();
                    } catch (java.io.IOException | RuntimeException failed) {
                        Toast.makeText(this, "配对未能导入，请检查本机配对状态后重试", Toast.LENGTH_LONG).show();
                    } finally {
                        plan.close();
                        dispatchSharedUiState();
                    }
                })
                .create();
        pairingImportDialog.setOnDismissListener(dialog -> {
            plan.close();
            if (pendingPairingImport == plan) {
                pendingPairingImport = null;
                pairingImportDialog = null;
            }
        });
        try { pairingImportDialog.show(); }
        catch (RuntimeException unavailable) {
            cancelPairingImport();
            throw unavailable;
        }
    }

    private static String pairingPeerLabel(AndroidPairingImportPlan.Peer peer) {
        java.text.SimpleDateFormat dateFormat = new java.text.SimpleDateFormat(
                "yyyy-MM-dd HH:mm:ss.SSS 'UTC'", java.util.Locale.ROOT);
        dateFormat.setTimeZone(java.util.TimeZone.getTimeZone("UTC"));
        return peer.displayName().replace('\n', ' ').replace('\r', ' ')
                + "\n配对批次：" + dateFormat.format(new java.util.Date(peer.pairedAtMs()))
                + (peer.active() ? "" : "（当前已撤销）");
    }

    private void cancelPairingImport() {
        AndroidPairingImportPlan plan = pendingPairingImport;
        AlertDialog dialog = pairingImportDialog;
        pendingPairingImport = null;
        pairingImportDialog = null;
        if (plan != null) plan.close();
        if (dialog != null) dialog.dismiss();
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

    private void requestStartCamera(long expectedRevision) {
        AudioNodeService.NodeBinder current = binder;
        if (current == null || current.cameraSettingsRevision() != expectedRevision) return;
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
            pendingCameraSettingsRevision = expectedRevision;
            pendingCameraOwner = current;
            requestPermissions(missing.toArray(new String[0]), REQUEST_CAMERA_PERMISSIONS);
            return;
        }
        current.startLocalCamera(expectedRevision);
    }

    private boolean cameraPermissionsGranted() {
        boolean cameraGranted = checkSelfPermission(Manifest.permission.CAMERA)
                == PackageManager.PERMISSION_GRANTED;
        boolean notificationGranted = Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU
                || checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS)
                        == PackageManager.PERMISSION_GRANTED;
        return cameraGranted && notificationGranted;
    }

    private boolean notificationPermissionGranted() {
        return Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU
                || checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS)
                        == PackageManager.PERMISSION_GRANTED;
    }

    private void requestStartConnectedDevice(AndroidCapabilityCommand.Target target) {
        if (!notificationPermissionGranted()) {
            pendingConnectedDeviceStart = target;
            requestPermissions(
                    new String[] {Manifest.permission.POST_NOTIFICATIONS},
                    REQUEST_CONNECTED_DEVICE_NOTIFICATION);
            return;
        }
        startConnectedDeviceCapability(target);
    }

    private void startConnectedDeviceCapability(AndroidCapabilityCommand.Target target) {
        switch (target) {
            case IMU -> AudioNodeService.startImu(this);
            case GAMEPAD -> AudioNodeService.startGamepad(this);
            case TOUCHPAD -> AudioNodeService.startTouchpad(this);
            case HAPTICS -> AudioNodeService.startHaptics(this);
        }
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
        // The WebView owns its visible-panel polling. Do not render the hidden
        // legacy audio page or sustain a second 250 ms metrics loop behind it.
        if (nativeControls.getVisibility() != View.VISIBLE) {
            dispatchSharedUiState();
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
        // JavascriptInterface callbacks execute on JavaBridge, not the UI Looper.
        if (Looper.myLooper() != handler.getLooper()) {
            handler.post(this::dispatchSharedUiState);
            return;
        }
        if (isDestroyed() || isFinishing()) {
            return;
        }
        if (sharedUiLoaded && sharedUi.getVisibility() == View.VISIBLE) {
            sharedUi.evaluateJavascript(
                    "window.dispatchEvent(new Event('capyio-native-state'))",
                    null);
        }
    }

    @SuppressWarnings("deprecation")
    private void setTouchpadImmersive(boolean active) {
        if (touchpadImmersive == active || isDestroyed() || isFinishing()) {
            return;
        }
        touchpadImmersive = active;
        if (active) {
            touchpadRawDowns = 0;
            touchpadRawPointerDowns = 0;
            touchpadRawCancels = 0;
            touchpadRawMaxPointers = 0;
        }
        View root = findViewById(R.id.activity_root);
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            WindowInsetsController controller = getWindow().getInsetsController();
            if (controller != null) {
                if (active) {
                    controller.hide(WindowInsets.Type.systemBars());
                    controller.setSystemBarsBehavior(
                            WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
                } else {
                    controller.show(WindowInsets.Type.systemBars());
                }
            }
        } else if (active) {
            getWindow().getDecorView().setSystemUiVisibility(
                    View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
                            | View.SYSTEM_UI_FLAG_FULLSCREEN
                            | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
                            | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN
                            | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
                            | View.SYSTEM_UI_FLAG_LAYOUT_STABLE);
        } else {
            getWindow().getDecorView().setSystemUiVisibility(View.SYSTEM_UI_FLAG_LAYOUT_STABLE);
        }
        if (active) {
            getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
            root.setPadding(0, 0, 0, 0);
            sharedUi.post(() -> sharedUi.setSystemGestureExclusionRects(List.of(
                    new Rect(0, 0, sharedUi.getWidth(), sharedUi.getHeight()))));
        } else {
            getWindow().clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
            sharedUi.setSystemGestureExclusionRects(List.of());
            root.requestApplyInsets();
        }
    }

    @SuppressWarnings("deprecation")
    private boolean performTouchpadHaptic(String effect) {
        if (!touchpadImmersive || !("tap".equals(effect) || "drag".equals(effect))) {
            return false;
        }
        Vibrator vibrator;
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            VibratorManager manager = getSystemService(VibratorManager.class);
            vibrator = manager == null ? null : manager.getDefaultVibrator();
        } else {
            vibrator = (Vibrator) getSystemService(Context.VIBRATOR_SERVICE);
        }
        if (vibrator == null || !vibrator.hasVibrator()) {
            return false;
        }
        int strength = getTouchpadHapticStrength();
        if (strength == 0) {
            return true;
        }
        boolean drag = "drag".equals(effect);
        int amplitude = vibrator.hasAmplitudeControl()
                ? Math.max(1, Math.min(255, Math.round(strength * (drag ? 3.6f : 2.54f))))
                : VibrationEffect.DEFAULT_AMPLITUDE;
        vibrator.vibrate(VibrationEffect.createOneShot(drag ? 20L : 12L, amplitude));
        return true;
    }

    private int getTouchpadHapticStrength() {
        int stored = getSharedPreferences(TOUCHPAD_HAPTIC_PREFS, MODE_PRIVATE)
                .getInt(TOUCHPAD_HAPTIC_STRENGTH, 50);
        return Math.max(0, Math.min(100, stored));
    }

    private boolean setTouchpadHapticStrength(int strength) {
        if (strength < 0 || strength > 100) {
            return false;
        }
        getSharedPreferences(TOUCHPAD_HAPTIC_PREFS, MODE_PRIVATE).edit()
                .putInt(TOUCHPAD_HAPTIC_STRENGTH, strength).apply();
        return true;
    }

    private final class ObserverBridge {
        @JavascriptInterface
        public String getTouchpadInputDiagnosticsJson() {
            // Counts only; no coordinates, pointer identities or content leave the UI.
            return "{\"downs\":" + touchpadRawDowns
                    + ",\"pointerDowns\":" + touchpadRawPointerDowns
                    + ",\"cancels\":" + touchpadRawCancels
                    + ",\"maxPointers\":" + touchpadRawMaxPointers + "}";
        }

        @JavascriptInterface
        public boolean setTouchpadImmersive(boolean active) {
            handler.post(() -> MainActivity.this.setTouchpadImmersive(active));
            return true;
        }

        @JavascriptInterface
        public boolean performTouchpadHaptic(String effect) {
            return MainActivity.this.performTouchpadHaptic(effect);
        }

        @JavascriptInterface
        public int getTouchpadHapticStrength() {
            return MainActivity.this.getTouchpadHapticStrength();
        }

        @JavascriptInterface
        public boolean setTouchpadHapticStrength(int strength) {
            return MainActivity.this.setTouchpadHapticStrength(strength);
        }

        @JavascriptInterface
        public String getGamepadLayout() {
            return getSharedPreferences("capyio-gamepad-layout", MODE_PRIVATE)
                    .getString("v1", "null");
        }

        @JavascriptInterface
        public String getGamepadMotionSettings() {
            AudioNodeService.NodeBinder current = binder;
            if (current == null) return "null";
            return "{\"enabled\":" + current.gamepadMotionEnabled()
                    + ",\"available\":" + current.gamepadMotionAvailable()
                    + ",\"independentActive\":"
                    + (current.imuLifecycle().state() == dev.capyio.android.contract.CapabilityLifecycleState.ACTIVE)
                    + "}";
        }

        @JavascriptInterface
        public boolean setGamepadMotionEnabled(boolean enabled) {
            AudioNodeService.NodeBinder current = binder;
            return current != null && current.setGamepadMotionEnabled(enabled);
        }

        @JavascriptInterface
        public boolean setControllerMode(String mode) {
            AudioNodeService.NodeBinder current = binder;
            return current != null && current.setControllerMode(mode);
        }

        @JavascriptInterface
        public String getControllerUseJson() {
            AudioNodeService.NodeBinder current = binder;
            return current == null ? "null" : current.getControllerUseJson();
        }

        @JavascriptInterface
        public String getControllerPeersJson() {
            AudioNodeService.NodeBinder current = binder;
            return current == null ? "[]" : current.getControllerPeersJson();
        }

        @JavascriptInterface
        public String getAudioPeersJson() { AudioNodeService.NodeBinder current=binder;return current==null?"[]":current.getAudioPeersJson(); }
        @JavascriptInterface
        public String getCameraPeersJson() { AudioNodeService.NodeBinder current=binder;return current==null?"[]":current.getCameraPeersJson(); }
        @JavascriptInterface
        public String getCameraUseJson() { AudioNodeService.NodeBinder current=binder;return current==null?"{\"status\":\"idle\"}":current.getCameraUseJson(); }
        @JavascriptInterface
        public String getDisplayPeersJson() {AudioNodeService.NodeBinder current=binder;return current==null?"[]":current.getDisplayPeersJson();}
        @JavascriptInterface
        public String getDisplayUseJson() {AudioNodeService.NodeBinder current=binder;return current==null?"null":current.getDisplayUseJson();}
        @JavascriptInterface
        public boolean startDisplayUse(String peer,int width,int height,int refreshMillihertz,int bitrateKbps,boolean automatic) {AudioNodeService.NodeBinder current=binder;return current!=null&&current.startDisplayUse(peer,width,height,refreshMillihertz,bitrateKbps,automatic);}
        @JavascriptInterface
        public boolean stopDisplayUse() {AudioNodeService.NodeBinder current=binder;return current!=null&&current.stopDisplayUse();}
        @JavascriptInterface
        public boolean startCameraUse(String peer) {AudioNodeService.NodeBinder current=binder;return current!=null&&current.startCameraUse(peer);}
        @JavascriptInterface
        public boolean stopCameraUse() {AudioNodeService.NodeBinder current=binder;return current!=null&&current.stopCameraUse();}
        @JavascriptInterface
        public String getAudioUsesJson() { AudioNodeService.NodeBinder current=binder;return current==null?"[]":current.getAudioUsesJson(); }
        @JavascriptInterface
        public boolean startAudioUse(String peer,String direction) { AudioNodeService.NodeBinder current=binder;return current!=null&&current.startAudioUse(peer,direction); }
        @JavascriptInterface
        public boolean stopAudioUse(String direction) { AudioNodeService.NodeBinder current=binder;return current!=null&&current.stopAudioUse(direction); }

        @JavascriptInterface
        public boolean startControllerUse(String peerNodeId, String mode) {
            AudioNodeService.NodeBinder current = binder;
            return current != null && current.startControllerUse(peerNodeId, mode);
        }

        @JavascriptInterface
        public boolean stopControllerUse() {
            AudioNodeService.NodeBinder current = binder;
            return current != null && current.stopControllerUse();
        }

        @JavascriptInterface
        public String getHapticsSettings() {
            AudioNodeService.NodeBinder current = binder;
            if (current == null) return "null";
            return "{\"strength\":" + current.hapticsStrength()
                    + ",\"available\":" + current.hapticsAvailable()
                    + ",\"executedFrames\":" + current.hapticsExecutedFrames()
                    + ",\"networkActive\":" + current.hapticsNetworkActive() + "}";
        }

        @JavascriptInterface
        public boolean setHapticsStrength(int percent) {
            AudioNodeService.NodeBinder current = binder;
            return current != null && current.setHapticsStrength(percent);
        }

        @JavascriptInterface
        public boolean saveGamepadLayout(String layout) {
            if (layout == null || layout.length() > 2048) return false;
            // A single private presentation preference, never a path or command.
            return getSharedPreferences("capyio-gamepad-layout", MODE_PRIVATE)
                    .edit().putString("v1", layout).commit();
        }

        @JavascriptInterface
        public String getDisplayStateJson() {
            return dev.capyio.display.app.DisplayNodeFacade.stateJson(MainActivity.this);
        }

        @JavascriptInterface
        public boolean configureDisplayViewing(boolean fullscreen, boolean keepAwake, boolean showStatistics) {
            return getSharedPreferences("capyio-display-viewing", MODE_PRIVATE).edit()
                    .putBoolean("fullscreen", fullscreen).putBoolean("keepAwake", keepAwake)
                    .putBoolean("showStatistics", showStatistics).commit();
        }

        @JavascriptInterface
        public boolean openDisplayReceiver() {
            if (!dev.capyio.display.app.DisplayNodeFacade.inspect(MainActivity.this).canOpen()) return false;
            handler.post(() -> {
                if (!isDestroyed() && !isFinishing()) {
                    dev.capyio.display.app.DisplayNodeFacade.open(MainActivity.this);
                }
            });
            return true;
        }

        @JavascriptInterface
        public boolean importDisplayProvisioning() {
            handler.post(() -> {
                if (isDestroyed() || isFinishing()) return;
                Intent picker = new Intent(Intent.ACTION_OPEN_DOCUMENT)
                        .addCategory(Intent.CATEGORY_OPENABLE)
                        .setType("application/zip");
                try {
                    startActivityForResult(picker, REQUEST_DISPLAY_PROVISIONING);
                } catch (RuntimeException unavailable) {
                    Toast.makeText(MainActivity.this, "系统文件选择器不可用", Toast.LENGTH_LONG).show();
                }
            });
            return true;
        }

        @JavascriptInterface
        public boolean importPairingInvitation() {
            handler.post(() -> {
                if (isDestroyed() || isFinishing()) return;
                Intent picker = new Intent(Intent.ACTION_OPEN_DOCUMENT)
                        .addCategory(Intent.CATEGORY_OPENABLE)
                        .setType("application/octet-stream");
                try {
                    startActivityForResult(picker, REQUEST_PAIRING_INVITATION);
                } catch (RuntimeException unavailable) {
                    Toast.makeText(MainActivity.this, "系统文件选择器不可用", Toast.LENGTH_LONG).show();
                }
            });
            return true;
        }

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
                    current.selectedSharedCapabilityIds(),
                    current.pairingStoreState(),
                    current.pairedPeerCount(),
                    current.discoveryAnnouncing(),
                    current.authenticatedCatalogConnected(),
                    current.controlListenPort());
        }

        @JavascriptInterface
        public String getRemoteCatalogJson() {
            AudioNodeService.NodeBinder current = binder;
            return current == null ? "{\"nodes\":[],\"capabilities\":[]}" : current.getRemoteCatalogJson();
        }

        @JavascriptInterface
        public String getResourceRequestsJson() {
            AudioNodeService.NodeBinder current = binder;
            return current == null ? "[]" : current.touchpadRequestsJson();
        }

        @JavascriptInterface
        public String getPairedPeersJson() {
            AudioNodeService.NodeBinder current = binder;
            return current == null ? "[]" : current.pairedPeerMetadataJson();
        }

        @JavascriptInterface
        public String getEnrollmentStatusJson() {
            AudioNodeService.NodeBinder current = binder;
            return current == null ? "{\"phase\":\"idle\",\"message\":\"节点服务尚未就绪\"}" : current.enrollmentStatusJson();
        }

        @JavascriptInterface
        public String getEnrollmentAddressesJson() {
            org.json.JSONArray addresses = new org.json.JSONArray();
            try {
                java.util.Enumeration<java.net.NetworkInterface> interfaces = java.net.NetworkInterface.getNetworkInterfaces();
                while (interfaces != null && interfaces.hasMoreElements() && addresses.length() < 32) {
                    java.net.NetworkInterface item = interfaces.nextElement();
                    if (!item.isUp()) continue;
                    java.util.Enumeration<java.net.InetAddress> values = item.getInetAddresses();
                    while (values.hasMoreElements() && addresses.length() < 32) {
                        java.net.InetAddress address = values.nextElement();
                        if (address instanceof java.net.Inet4Address && !address.isLoopbackAddress()) addresses.put(address.getHostAddress());
                    }
                }
            } catch (java.net.SocketException unavailable) { }
            return addresses.toString();
        }

        @JavascriptInterface
        public String enrollmentActionJson(String request) {
            AudioNodeService.NodeBinder current = binder;
            return current == null ? AndroidFilelessPairing.errorStatus("节点服务尚未就绪") : current.enrollmentActionJson(request);
        }

        @JavascriptInterface
        public boolean connectPairedPeerAt(String host, int port) {
            AudioNodeService.NodeBinder current = binder;
            return current != null && current.connectPairedPeerAt(host, port);
        }

        @JavascriptInterface
        public String getControlConnectionFailure() {
            AudioNodeService.NodeBinder current = binder;
            return current == null ? "节点服务未就绪" : current.controlConnectionFailure();
        }

        @JavascriptInterface
        public boolean resolveResourceRequest(String routeId, boolean allow) {
            AudioNodeService.NodeBinder current = binder;
            return current != null && routeId != null && routeId.length() == 36
                    && current.resolveTouchpadRequest(routeId, allow);
        }

        @JavascriptInterface
        public boolean resolveResourceRequestRemembered(String routeId, boolean allow, boolean remember) {
            AudioNodeService.NodeBinder current = binder;
            return current != null && routeId != null && routeId.length() == 36
                    && current.resolveTouchpadRequestRemembered(routeId, allow, remember);
        }

        @JavascriptInterface
        public boolean clearRememberedResourcePermissions() {
            AudioNodeService.NodeBinder current = binder;
            return current != null && current.clearRememberedTouchpadPermissions();
        }

        @JavascriptInterface
        public boolean setLocalCapabilityShared(String capabilityId, boolean selected) {
            AudioNodeService.NodeBinder current = binder;
            return current != null && current.setLocalCapabilityShared(capabilityId, selected);
        }

        @JavascriptInterface
        public boolean getBackgroundWaitingEnabled() {
            return AudioNodeService.backgroundWaitingPreference(MainActivity.this);
        }

        @JavascriptInterface
        public boolean setBackgroundWaitingEnabled(boolean enabled) {
            AudioNodeService.NodeBinder current = binder;
            return current != null && current.setBackgroundWaitingEnabled(enabled);
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
        public boolean configureAudioPeer(String direction, String host, int localPort, int peerPort,
                boolean trustedLabConfirmed) {
            AudioNodeService.NodeBinder current = binder;
            return current != null && current.configureAudioPeer(
                    direction, host, localPort, peerPort, trustedLabConfirmed);
        }

        @JavascriptInterface
        public String getAudioPeerSettingsJson(String direction) {
            AudioNodeService.NodeBinder current = binder;
            return current == null ? "" : current.audioPeerSettingsJson(direction);
        }

        @JavascriptInterface
        public String getCameraSettingsJson() {
            AudioNodeService.NodeBinder current = binder;
            return current == null ? "" : current.getCameraSettingsJson(cameraPermissionsGranted());
        }

        @JavascriptInterface
        public boolean refreshCameraSources() {
            if (!cameraPermissionsGranted()) {
                return false;
            }
            AudioNodeService.NodeBinder current = binder;
            boolean accepted = current != null && current.updateCameraSettings("refresh", "");
            dispatchSharedUiState();
            return accepted;
        }

        @JavascriptInterface
        public boolean selectCameraSource(String sourceKey) {
            AudioNodeService.NodeBinder current = binder;
            boolean accepted = current != null && current.updateCameraSettings("source", sourceKey);
            if (accepted) {
                dispatchSharedUiState();
            }
            return accepted;
        }

        @JavascriptInterface
        public boolean selectCameraQuality(String quality) {
            AudioNodeService.NodeBinder current = binder;
            boolean accepted = current != null && current.updateCameraSettings("quality", quality);
            if (accepted) {
                dispatchSharedUiState();
            }
            return accepted;
        }

        @JavascriptInterface
        public boolean selectCameraProfile(String profile) {
            AudioNodeService.NodeBinder current = binder;
            boolean accepted = current != null && current.updateCameraSettings("profile", profile);
            if (accepted) dispatchSharedUiState();
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
            if (current == null || !uiInputActive) {
                return false;
            }
            CapabilityLifecycleSnapshot lifecycle = current.gamepadLifecycle();
            if (lifecycle.state() != CapabilityLifecycleState.ACTIVE) {
                // Stopping the Route already neutralized controls. A view-unmount
                // reset is idempotent, not a failed attempt to send new input.
                return "reset".equals(kind);
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
            if (current == null || !uiInputActive) {
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
                long expectedRevision = current.cameraSettingsRevision();
                handler.post(() -> {
                    if (cameraCommand.operation() == AndroidCameraCommand.Operation.STOP) {
                        pendingCameraStart = false;
                        current.stopCameraUse();
                    } else if (binder == current) {
                        requestStartCamera(expectedRevision);
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
            if ("capyio.quick-action.android-touchpad".equals(actionId)) {
                if (!"stop".equals(operation)
                        || (lifecycle.state() != CapabilityLifecycleState.ACTIVE
                        && lifecycle.state() != CapabilityLifecycleState.STARTING
                        && lifecycle.state() != CapabilityLifecycleState.FAILED)) {
                    return false;
                }
                current.stopTouchpad();
                return true;
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
                        handler.post(() -> requestStartConnectedDevice(command.target()));
                    }
                }
                case GAMEPAD -> {
                    if (command.operation() == AndroidCapabilityCommand.Operation.STOP) {
                        current.stopGamepad();
                    } else {
                        handler.post(() -> requestStartConnectedDevice(command.target()));
                    }
                }
                case TOUCHPAD -> { return false; }
                case HAPTICS -> {
                    if (command.operation() == AndroidCapabilityCommand.Operation.STOP) {
                        current.stopHaptics();
                    } else {
                        handler.post(() -> requestStartConnectedDevice(command.target()));
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
