package io.capyio.camera.lab;

import android.Manifest;
import android.annotation.SuppressLint;
import android.app.Activity;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.content.pm.PackageManager;
import android.content.res.Configuration;
import android.graphics.Color;
import android.hardware.camera2.CameraAccessException;
import android.os.Bundle;
import android.os.Build;
import android.os.Handler;
import android.os.Looper;
import android.text.InputFilter;
import android.text.InputType;
import android.view.Gravity;
import android.view.ViewGroup;
import android.view.WindowManager;
import android.widget.Button;
import android.widget.EditText;
import android.widget.LinearLayout;
import android.widget.TextView;
import io.capyio.camera.contract.AvcQualityPreset;
import io.capyio.camera.contract.CameraFacingPolicy;
import io.capyio.camera.contract.CameraSourceSelection;
import io.capyio.camera.contract.CameraTransportEndpoint;
import io.capyio.camera.contract.FrameObservation;

/** User-visible controller for the service-owned Camera2 stream. */
public final class MainActivity extends Activity {
    private static final int CAMERA_PERMISSION_REQUEST = 1001;
    private static final long SERVICE_POLL_MILLIS = 250;

    private final Handler mainHandler = new Handler(Looper.getMainLooper());
    private final Runnable servicePoll = this::pollService;
    private final BroadcastReceiver serviceStateReceiver = new BroadcastReceiver() {
        @Override
        public void onReceive(Context context, Intent intent) {
            consumeServiceSnapshot();
        }
    };
    private TextView status;
    private Button toggle;
    private Button switchCamera;
    private Button switchQuality;
    private Button switchSource;
    private Button inspectCameras;
    private EditText transportHost;
    private boolean activityVisible;
    private boolean permissionPending;
    private boolean restartAfterStop;
    private FrameObservation.LensFacing preferredFacing = FrameObservation.LensFacing.BACK;
    private AvcQualityPreset preferredQuality = AvcQualityPreset.BALANCED;
    private CameraSourceSelection preferredSource;
    private CameraTransportEndpoint preferredTransport = CameraTransportEndpoint.adbReverse();
    private CameraCaptureService.Phase lastPhase = CameraCaptureService.Phase.STOPPED;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        getWindow().addFlags(WindowManager.LayoutParams.FLAG_SECURE);
        createUi();
        renderStatus(getString(R.string.camera_stopped));
    }

    @Override
    protected void onResume() {
        super.onResume();
        activityVisible = true;
        mainHandler.removeCallbacks(servicePoll);
        mainHandler.post(servicePoll);
    }

    @Override
    @SuppressLint("UnspecifiedRegisterReceiverFlag")
    protected void onStart() {
        super.onStart();
        IntentFilter stateFilter =
                new IntentFilter(CameraCaptureService.ACTION_STATE_CHANGED);
        if (Build.VERSION.SDK_INT >= 33) {
            registerReceiver(serviceStateReceiver, stateFilter, RECEIVER_NOT_EXPORTED);
        } else {
            // API 29-32 has no receiver-export flag; the sender fixes this package explicitly.
            registerReceiver(serviceStateReceiver, stateFilter);
        }
        mainHandler.post(servicePoll);
    }

    @Override
    protected void onPause() {
        activityVisible = false;
        super.onPause();
    }

    @Override
    protected void onStop() {
        mainHandler.removeCallbacks(servicePoll);
        unregisterReceiver(serviceStateReceiver);
        super.onStop();
    }

    @Override
    protected void onDestroy() {
        mainHandler.removeCallbacksAndMessages(null);
        super.onDestroy();
    }

    @Override
    public void onConfigurationChanged(Configuration newConfig) {
        super.onConfigurationChanged(newConfig);
        updateControls(CameraCaptureService.snapshot());
    }

    private void createUi() {
        LinearLayout root = new LinearLayout(this);
        root.setOrientation(LinearLayout.VERTICAL);
        root.setPadding(dp(16), dp(16), dp(16), dp(16));
        root.setBackgroundColor(Color.BLACK);

        status = new TextView(this);
        status.setTextColor(Color.WHITE);
        status.setTextSize(16);
        status.setGravity(Gravity.START);
        root.addView(status, new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT));

        transportHost = new EditText(this);
        transportHost.setSingleLine(true);
        transportHost.setFilters(new InputFilter[] {new InputFilter.LengthFilter(32)});
        transportHost.setInputType(
                InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_URI);
        transportHost.setTextColor(Color.WHITE);
        transportHost.setHintTextColor(Color.LTGRAY);
        transportHost.setHint(R.string.camera_transport_host_hint);
        root.addView(transportHost, new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT));

        TextView ownershipNote = new TextView(this);
        ownershipNote.setText(R.string.camera_service_ownership_note);
        ownershipNote.setTextColor(Color.LTGRAY);
        ownershipNote.setGravity(Gravity.CENTER);
        LinearLayout.LayoutParams noteParams = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                0,
                1f);
        noteParams.setMargins(0, dp(16), 0, dp(16));
        root.addView(ownershipNote, noteParams);

        switchCamera = new Button(this);
        switchCamera.setOnClickListener(view -> onSwitchCameraPressed());
        root.addView(switchCamera, buttonLayout());

        switchQuality = new Button(this);
        switchQuality.setOnClickListener(view -> onSwitchQualityPressed());
        root.addView(switchQuality, buttonLayout());

        switchSource = new Button(this);
        switchSource.setOnClickListener(view -> onSwitchSourcePressed());
        root.addView(switchSource, buttonLayout());

        inspectCameras = new Button(this);
        inspectCameras.setText(R.string.inspect_camera_capabilities);
        inspectCameras.setOnClickListener(view -> onInspectCamerasPressed());
        root.addView(inspectCameras, buttonLayout());

        toggle = new Button(this);
        toggle.setText(R.string.start_camera);
        toggle.setOnClickListener(view -> onTogglePressed());
        root.addView(toggle, buttonLayout());

        setContentView(root);
        updateSwitchButton();
        updateQualityButton();
        updateSourceButton();
    }

    private LinearLayout.LayoutParams buttonLayout() {
        return new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT);
    }

    private void onInspectCamerasPressed() {
        if (checkSelfPermission(Manifest.permission.CAMERA) != PackageManager.PERMISSION_GRANTED) {
            renderStatus(getString(R.string.camera_inventory_permission_required));
            return;
        }
        try {
            renderStatus(CameraInventoryCollector.collect(this).toJson());
        } catch (CameraAccessException | RuntimeException error) {
            renderStatus(getString(
                    R.string.camera_inventory_failed,
                    error.getClass().getSimpleName()));
        }
    }

    private void onSwitchCameraPressed() {
        preferredFacing = CameraFacingPolicy.toggle(preferredFacing);
        preferredSource = null;
        updateSwitchButton();
        updateSourceButton();
        restartIfRunning();
    }

    private void onSwitchQualityPressed() {
        preferredQuality = preferredQuality.next();
        updateQualityButton();
        restartIfRunning();
    }

    private void onSwitchSourcePressed() {
        if (checkSelfPermission(Manifest.permission.CAMERA) != PackageManager.PERMISSION_GRANTED) {
            renderStatus(getString(R.string.camera_inventory_permission_required));
            return;
        }
        try {
            preferredSource = CameraSourceSelection.next(
                    preferredSource,
                    CameraSourceSelection.enumerate(CameraInventoryCollector.collect(this)));
            if (preferredSource != null) {
                preferredFacing = mapFacing(preferredSource.facing());
            }
            updateSourceButton();
            updateSwitchButton();
            restartIfRunning();
        } catch (CameraAccessException | RuntimeException error) {
            renderStatus(getString(
                    R.string.camera_inventory_failed,
                    error.getClass().getSimpleName()));
        }
    }

    private void restartIfRunning() {
        CameraCaptureService.Phase phase = CameraCaptureService.snapshot().phase();
        if (phase == CameraCaptureService.Phase.STARTING
                || phase == CameraCaptureService.Phase.STREAMING) {
            restartAfterStop = true;
            CameraCaptureService.stop(this);
            renderStatus(getString(R.string.camera_restarting));
            return;
        }
        renderStatus(getString(R.string.camera_settings_selected));
    }

    private void onTogglePressed() {
        CameraCaptureService.Phase phase = CameraCaptureService.snapshot().phase();
        if (permissionPending
                || phase == CameraCaptureService.Phase.STARTING
                || phase == CameraCaptureService.Phase.STREAMING) {
            permissionPending = false;
            restartAfterStop = false;
            CameraCaptureService.stop(this);
            updateControls(CameraCaptureService.snapshot());
            return;
        }
        startWithPermission();
    }

    private void startWithPermission() {
        if (checkSelfPermission(Manifest.permission.CAMERA) != PackageManager.PERMISSION_GRANTED) {
            permissionPending = true;
            requestPermissions(
                    new String[] {Manifest.permission.CAMERA},
                    CAMERA_PERMISSION_REQUEST);
            updateControls(CameraCaptureService.snapshot());
            return;
        }
        try {
            preferredTransport = CameraTransportEndpoint.fromUserInput(
                    transportHost.getText().toString());
        } catch (IllegalArgumentException error) {
            renderStatus(getString(R.string.camera_transport_host_rejected));
            return;
        }
        CameraCaptureService.start(
                this,
                preferredFacing,
                preferredSource,
                preferredQuality,
                preferredTransport);
        renderStatus(getString(R.string.opening_camera));
    }

    @Override
    public void onRequestPermissionsResult(
            int requestCode,
            String[] permissions,
            int[] grantResults) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults);
        if (requestCode != CAMERA_PERMISSION_REQUEST) {
            return;
        }
        permissionPending = false;
        boolean granted = grantResults.length == 1
                && grantResults[0] == PackageManager.PERMISSION_GRANTED;
        if (granted) {
            startWithPermission();
        } else {
            renderStatus(getString(R.string.permission_denied));
            updateControls(CameraCaptureService.snapshot());
        }
    }

    private void pollService() {
        consumeServiceSnapshot();
        if (activityVisible) {
            mainHandler.postDelayed(servicePoll, SERVICE_POLL_MILLIS);
        }
    }

    private void consumeServiceSnapshot() {
        CameraCaptureService.Snapshot snapshot = CameraCaptureService.snapshot();
        if (snapshot.phase() == CameraCaptureService.Phase.STOPPED
                && lastPhase != CameraCaptureService.Phase.STOPPED
                && restartAfterStop) {
            restartAfterStop = false;
            startWithPermission();
            snapshot = CameraCaptureService.snapshot();
        }
        lastPhase = snapshot.phase();
        renderSnapshot(snapshot);
        updateControls(snapshot);
    }

    private void renderSnapshot(CameraCaptureService.Snapshot snapshot) {
        switch (snapshot.phase()) {
            case STOPPED -> {
                if (!restartAfterStop) {
                    renderStatus(getString(R.string.camera_stopped));
                }
            }
            case STARTING -> renderStatus(getString(R.string.opening_camera));
            case ERROR -> renderStatus(snapshot.error());
            case STREAMING -> renderStatus(getString(
                    R.string.streaming_status,
                    snapshot.width(),
                    snapshot.height(),
                    snapshot.facing(),
                    snapshot.sourceKey(),
                    snapshot.quality(),
                    snapshot.quality().bitrateForDimensions(
                                    snapshot.width(), snapshot.height())
                            / 1_000_000.0,
                    snapshot.capturedFrames(),
                    snapshot.encodedAccessUnits(),
                    snapshot.droppedAccessUnits(),
                    snapshot.transportStatus(),
                    snapshot.sentAccessUnits(),
                    snapshot.exportDroppedAccessUnits(),
                    snapshot.encoderStatus()));
        }
    }

    private void updateControls(CameraCaptureService.Snapshot snapshot) {
        boolean running = permissionPending
                || snapshot.phase() == CameraCaptureService.Phase.STARTING
                || snapshot.phase() == CameraCaptureService.Phase.STREAMING;
        toggle.setText(running ? R.string.stop_camera : R.string.start_camera);
        switchCamera.setEnabled(!permissionPending);
        switchQuality.setEnabled(!permissionPending);
        switchSource.setEnabled(!permissionPending);
        inspectCameras.setEnabled(!permissionPending);
        transportHost.setEnabled(!running);
        updateSwitchButton();
        updateQualityButton();
        updateSourceButton();
    }

    private void updateSwitchButton() {
        switchCamera.setText(preferredFacing == FrameObservation.LensFacing.BACK
                ? R.string.use_front_camera
                : R.string.use_back_camera);
    }

    private void updateQualityButton() {
        switchQuality.setText(getString(R.string.cycle_camera_quality, preferredQuality));
    }

    private void updateSourceButton() {
        switchSource.setText(preferredSource == null
                ? getString(R.string.camera_source_automatic)
                : getString(R.string.camera_source_cycle, sourceDescription(preferredSource)));
    }

    private String sourceDescription(CameraSourceSelection source) {
        String selectionMode = source.targetZoomRatioMilli() == null
                ? getString(R.string.camera_source_zoom_automatic)
                : getString(
                        R.string.camera_source_zoom_target,
                        source.targetZoomRatioMilli() / 1_000.0);
        return getString(
                R.string.camera_source_description,
                source.cameraId(),
                source.facing(),
                selectionMode);
    }

    private static FrameObservation.LensFacing mapFacing(
            io.capyio.camera.contract.CameraInventory.LensFacing facing) {
        return switch (facing) {
            case FRONT -> FrameObservation.LensFacing.FRONT;
            case BACK -> FrameObservation.LensFacing.BACK;
            case EXTERNAL -> FrameObservation.LensFacing.EXTERNAL;
            case UNKNOWN -> FrameObservation.LensFacing.UNKNOWN;
        };
    }

    private void renderStatus(String text) {
        status.setText(text);
    }

    private int dp(int value) {
        return Math.round(value * getResources().getDisplayMetrics().density);
    }
}
