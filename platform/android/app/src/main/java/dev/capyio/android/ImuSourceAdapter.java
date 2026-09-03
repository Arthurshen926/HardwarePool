package dev.capyio.android;

import android.hardware.Sensor;
import android.hardware.SensorEvent;
import android.hardware.SensorEventListener;
import android.hardware.SensorManager;

/** Service-owned accelerometer and gyroscope registration boundary. */
final class ImuSourceAdapter implements SensorEventListener {
    static final String PROBLEM_HARDWARE = "CAPY.ANDROID.IMU_HARDWARE_MISSING";
    static final String PROBLEM_START = "CAPY.ANDROID.IMU_START_FAILED";

    interface Listener {
        void onStarted(long generation);

        void onStopped(long generation);

        void onFailed(long generation, String problemCode);
    }

    record SampleSnapshot(
            long accelerationTimestampNanos,
            float accelerationX,
            float accelerationY,
            float accelerationZ,
            long angularVelocityTimestampNanos,
            float angularVelocityX,
            float angularVelocityY,
            float angularVelocityZ) {}

    private final SensorManager sensorManager;
    private final Sensor accelerometer;
    private final Sensor gyroscope;
    private final Listener listener;

    private volatile long activeGeneration;
    private volatile long accelerationTimestampNanos;
    private volatile float accelerationX;
    private volatile float accelerationY;
    private volatile float accelerationZ;
    private volatile long angularVelocityTimestampNanos;
    private volatile float angularVelocityX;
    private volatile float angularVelocityY;
    private volatile float angularVelocityZ;

    ImuSourceAdapter(SensorManager sensorManager, Listener listener) {
        this.sensorManager = sensorManager;
        this.listener = listener;
        Sensor accelerationCandidate = null;
        Sensor gyroscopeCandidate = null;
        try {
            if (sensorManager != null) {
                accelerationCandidate = sensorManager.getDefaultSensor(Sensor.TYPE_ACCELEROMETER);
                gyroscopeCandidate = sensorManager.getDefaultSensor(Sensor.TYPE_GYROSCOPE);
            }
        } catch (RuntimeException unavailable) {
            // Start reports the bounded hardware problem through the lifecycle listener.
        }
        accelerometer = accelerationCandidate;
        gyroscope = gyroscopeCandidate;
    }

    void start(long generation) {
        if (sensorManager == null || accelerometer == null || gyroscope == null) {
            listener.onFailed(generation, PROBLEM_HARDWARE);
            return;
        }
        if (activeGeneration != 0) {
            listener.onFailed(generation, PROBLEM_START);
            return;
        }
        try {
            boolean accelerationRegistered = sensorManager.registerListener(
                    this, accelerometer, SensorManager.SENSOR_DELAY_GAME);
            boolean gyroscopeRegistered = sensorManager.registerListener(
                    this, gyroscope, SensorManager.SENSOR_DELAY_GAME);
            if (!accelerationRegistered || !gyroscopeRegistered) {
                unregisterSafely();
                listener.onFailed(generation, PROBLEM_START);
                return;
            }
            activeGeneration = generation;
            listener.onStarted(generation);
        } catch (RuntimeException unavailable) {
            unregisterSafely();
            listener.onFailed(generation, PROBLEM_START);
        }
    }

    void stop(long generation) {
        activeGeneration = 0;
        unregisterSafely();
        listener.onStopped(generation);
    }

    SampleSnapshot latestSample() {
        return new SampleSnapshot(
                accelerationTimestampNanos,
                accelerationX,
                accelerationY,
                accelerationZ,
                angularVelocityTimestampNanos,
                angularVelocityX,
                angularVelocityY,
                angularVelocityZ);
    }

    @Override
    public void onSensorChanged(SensorEvent event) {
        if (activeGeneration == 0 || event.values.length < 3) {
            return;
        }
        if (event.sensor.getType() == Sensor.TYPE_ACCELEROMETER) {
            accelerationX = event.values[0];
            accelerationY = event.values[1];
            accelerationZ = event.values[2];
            accelerationTimestampNanos = event.timestamp;
        } else if (event.sensor.getType() == Sensor.TYPE_GYROSCOPE) {
            angularVelocityX = event.values[0];
            angularVelocityY = event.values[1];
            angularVelocityZ = event.values[2];
            angularVelocityTimestampNanos = event.timestamp;
        }
    }

    @Override
    public void onAccuracyChanged(Sensor sensor, int accuracy) {
        // Accuracy metadata will enter the typed IMU data plane in a later slice.
    }

    private void unregisterSafely() {
        try {
            if (sensorManager != null) {
                sensorManager.unregisterListener(this);
            }
        } catch (RuntimeException unavailable) {
            // Teardown remains idempotent even if the platform sensor service vanished.
        }
    }
}
