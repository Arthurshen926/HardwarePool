package dev.capyio.android;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.content.Context;
import android.content.Intent;

/** Renders Node status; foreground ownership remains with AudioNodeService. */
final class AndroidNodeNotification {
    static final int ID = 4101;
    private static final String CHANNEL_ID = "capyio-audio-node-v1";
    private final Context context;
    private final Intent stopIntent;

    AndroidNodeNotification(Context context, Intent stopIntent) {
        this.context = context;
        this.stopIntent = stopIntent;
    }

    Notification build(
            boolean microphone, boolean speaker, boolean connectedDevice,
            boolean publicationForeground, boolean backgroundWaitingForeground) {
        Intent openIntent = new Intent(context, MainActivity.class)
                .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP);
        PendingIntent openPendingIntent = PendingIntent.getActivity(
                context,
                0,
                openIntent,
                PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
        PendingIntent stopPendingIntent = PendingIntent.getService(
                context,
                1,
                stopIntent,
                PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);

        java.util.List<String> details = new java.util.ArrayList<>(3);
        if (microphone) {
            details.add(context.getString(R.string.notification_microphone));
        }
        if (speaker) {
            details.add(context.getString(R.string.notification_speaker));
        }
        if (connectedDevice) {
            details.add(publicationForeground ? "资源已共享 · 等待或保持设备连接"
                    : backgroundWaitingForeground ? "节点在线 · 等待已配对设备"
                    : context.getString(R.string.notification_connected_device));
        }
        return new Notification.Builder(context, CHANNEL_ID)
                .setSmallIcon(R.drawable.ic_capyio)
                .setContentTitle(context.getString(R.string.notification_title))
                .setContentText(String.join(" · ", details))
                .setContentIntent(openPendingIntent)
                .setOngoing(true)
                .setOnlyAlertOnce(true)
                .setCategory(Notification.CATEGORY_SERVICE)
                .addAction(new Notification.Action.Builder(
                        null,
                        context.getString(R.string.notification_stop_all),
                        stopPendingIntent).build())
                .build();
    }

    void createChannel() {
        NotificationChannel channel = new NotificationChannel(
                CHANNEL_ID,
                context.getString(R.string.notification_channel_name),
                NotificationManager.IMPORTANCE_LOW);
        channel.setDescription(context.getString(R.string.notification_channel_description));
        NotificationManager manager = context.getSystemService(NotificationManager.class);
        manager.createNotificationChannel(channel);
    }

}
