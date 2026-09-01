package dev.capyio.android;

import android.Manifest;
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
import android.view.View;
import android.widget.Button;
import android.widget.TextView;

import dev.capyio.android.contract.ActualAudioFormat;
import dev.capyio.android.contract.AudioCapabilityKind;
import dev.capyio.android.contract.AudioCapabilitySnapshot;
import dev.capyio.android.contract.AudioCapabilityState;
import dev.capyio.android.contract.AudioNodeSnapshot;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.atomic.AtomicBoolean;

public final class MainActivity extends Activity {
    private static final int REQUEST_AUDIO_PERMISSIONS = 8101;
    private static final long REFRESH_INTERVAL_MILLIS = 250;

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
        }
    };

    private AudioNodeService.NodeBinder binder;
    private AudioCapabilityKind pendingStart;
    private boolean bound;

    private TextView nodeIdView;
    private TextView permissionView;
    private TextView speakerTransportView;
    private TextView microphoneTransportView;
    private CapabilityViews microphoneViews;
    private CapabilityViews speakerViews;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        setContentView(R.layout.activity_main);
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
        renderPermissionState(null);
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
    public void onRequestPermissionsResult(
            int requestCode,
            String[] permissions,
            int[] grantResults) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults);
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
