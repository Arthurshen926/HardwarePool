package dev.capyio.android;

import android.content.Context;
import android.content.pm.PackageManager;
import android.hardware.Sensor;
import android.hardware.SensorManager;
import android.media.AudioManager;
import android.os.Vibrator;

import dev.capyio.android.contract.HardwarePresence;

import java.util.function.BooleanSupplier;

/** Read-only feature inventory. It never opens a camera, sensor, microphone, or speaker. */
final class AndroidHardwareInventory {
    private final HardwarePresence microphonePresence;
    private final HardwarePresence speakerPresence;
    private final HardwarePresence cameraPresence;
    private final HardwarePresence imuPresence;
    private final HardwarePresence touchSurfacePresence;
    private final HardwarePresence vibratorPresence;

    private AndroidHardwareInventory(
            HardwarePresence microphonePresence,
            HardwarePresence speakerPresence,
            HardwarePresence cameraPresence,
            HardwarePresence imuPresence,
            HardwarePresence touchSurfacePresence,
            HardwarePresence vibratorPresence) {
        this.microphonePresence = microphonePresence;
        this.speakerPresence = speakerPresence;
        this.cameraPresence = cameraPresence;
        this.imuPresence = imuPresence;
        this.touchSurfacePresence = touchSurfacePresence;
        this.vibratorPresence = vibratorPresence;
    }

    static AndroidHardwareInventory probe(Context context) {
        PackageManager packages = context.getPackageManager();
        SensorManager sensors = context.getSystemService(SensorManager.class);
        AudioManager audio = context.getSystemService(AudioManager.class);
        Vibrator vibrator = context.getSystemService(Vibrator.class);
        return new AndroidHardwareInventory(
                probe(() -> packages.hasSystemFeature(PackageManager.FEATURE_MICROPHONE)),
                probe(() -> audio != null
                        && audio.getDevices(AudioManager.GET_DEVICES_OUTPUTS).length > 0),
                probe(() -> packages.hasSystemFeature(PackageManager.FEATURE_CAMERA_ANY)),
                probe(() -> sensors != null
                        && sensors.getDefaultSensor(Sensor.TYPE_ACCELEROMETER) != null
                        && sensors.getDefaultSensor(Sensor.TYPE_GYROSCOPE) != null),
                probe(() -> packages.hasSystemFeature(PackageManager.FEATURE_TOUCHSCREEN)),
                probe(() -> vibrator != null && vibrator.hasVibrator()));
    }

    HardwarePresence microphonePresence() {
        return microphonePresence;
    }

    HardwarePresence speakerPresence() {
        return speakerPresence;
    }

    HardwarePresence cameraPresence() {
        return cameraPresence;
    }

    HardwarePresence imuPresence() {
        return imuPresence;
    }

    HardwarePresence touchSurfacePresence() {
        return touchSurfacePresence;
    }

    HardwarePresence vibratorPresence() {
        return vibratorPresence;
    }

    private static HardwarePresence probe(BooleanSupplier query) {
        try {
            return query.getAsBoolean() ? HardwarePresence.PRESENT : HardwarePresence.ABSENT;
        } catch (RuntimeException unavailable) {
            return HardwarePresence.UNKNOWN;
        }
    }
}
