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
import android.os.Binder;
import android.os.Build;
import android.os.Handler;
import android.os.IBinder;
import android.os.Looper;

import dev.capyio.android.contract.ActualAudioFormat;
import dev.capyio.android.contract.AudioCapabilityKind;
import dev.capyio.android.contract.AudioCapabilitySnapshot;
import dev.capyio.android.contract.AudioNodeController;
import dev.capyio.android.contract.AudioNodeSnapshot;
import dev.capyio.android.contract.TransitionToken;

/** Owns Android audio resources independently of the Activity lifecycle. */
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

    private MicrophoneSourceAdapter microphoneAdapter;
    private SpeakerSinkAdapter speakerAdapter;
    private String nodeId;
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
        createNotificationChannel();
        microphoneAdapter = new MicrophoneSourceAdapter(
                listenerFor(AudioCapabilityKind.MICROPHONE_SOURCE));
        speakerAdapter = new SpeakerSinkAdapter(
                listenerFor(AudioCapabilityKind.SPEAKER_SINK));
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
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            int serviceTypes = 0;
            if (microphone && Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                serviceTypes |= ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE;
            }
            if (speaker) {
                serviceTypes |= ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK;
            }
            startForeground(NOTIFICATION_ID, notification, serviceTypes);
        } else {
            startForeground(NOTIFICATION_ID, notification);
        }
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

        public int speakerUnderrunCount() {
            return speakerAdapter.underrunCount();
        }

        public void setStateListener(Runnable listener) {
            stateListener = listener;
        }
    }
}
