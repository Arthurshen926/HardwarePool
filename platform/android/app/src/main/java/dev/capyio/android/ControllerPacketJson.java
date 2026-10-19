package dev.capyio.android;

import dev.capyio.android.contract.GamepadControlsSnapshot;

/** Version 2 separates controls from unavailable or disabled motion. */
final class ControllerPacketJson {
    private ControllerPacketJson() {}

    static String encode(
            String token,
            String session,
            long sequence,
            long timestampNanos,
            GamepadControlsSnapshot state,
            long accelerationTimestampNanos,
            float accelerationX,
            float accelerationY,
            float accelerationZ,
            long angularVelocityTimestampNanos,
            float angularVelocityX,
            float angularVelocityY,
            float angularVelocityZ) {
        return encode(token, session, sequence, timestampNanos, state,
                accelerationTimestampNanos, accelerationX, accelerationY, accelerationZ,
                angularVelocityTimestampNanos, angularVelocityX, angularVelocityY, angularVelocityZ, true);
    }

    static String encode(String token, String session, long sequence, long timestampNanos,
            GamepadControlsSnapshot state, long accelerationTimestampNanos,
            float accelerationX, float accelerationY, float accelerationZ,
            long angularVelocityTimestampNanos, float angularVelocityX,
            float angularVelocityY, float angularVelocityZ, boolean gamepadMotionEnabled) {
        boolean motionAvailable = accelerationTimestampNanos > 0 && angularVelocityTimestampNanos > 0;
        return new StringBuilder(640)
                .append('{')
                .append("\"version\":2,")
                .append("\"motionAvailable\":").append(motionAvailable).append(',')
                .append("\"gamepadMotionEnabled\":").append(gamepadMotionEnabled).append(',')
                .append("\"token\":\"").append(token).append("\",")
                .append("\"session\":\"").append(session).append("\",")
                .append("\"sequence\":").append(sequence).append(',')
                .append("\"timestampNanos\":").append(Math.max(1, timestampNanos)).append(',')
                .append("\"buttons\":").append(state.buttons()).append(',')
                .append("\"dpadX\":").append(state.dpadX()).append(',')
                .append("\"dpadY\":").append(state.dpadY()).append(',')
                .append("\"leftX\":").append(state.leftX()).append(',')
                .append("\"leftY\":").append(state.leftY()).append(',')
                .append("\"rightX\":").append(state.rightX()).append(',')
                .append("\"rightY\":").append(state.rightY()).append(',')
                .append("\"leftTrigger\":").append(state.leftTrigger()).append(',')
                .append("\"rightTrigger\":").append(state.rightTrigger()).append(',')
                .append("\"acceleration\":[")
                .append(Float.toString(accelerationX)).append(',')
                .append(Float.toString(accelerationY)).append(',')
                .append(Float.toString(accelerationZ)).append("],")
                .append("\"angularVelocity\":[")
                .append(Float.toString(angularVelocityX)).append(',')
                .append(Float.toString(angularVelocityY)).append(',')
                .append(Float.toString(angularVelocityZ)).append("],")
                .append("\"accelerationTimestampNanos\":")
                .append(accelerationTimestampNanos).append(',')
                .append("\"angularVelocityTimestampNanos\":")
                .append(angularVelocityTimestampNanos)
                .append('}')
                .toString();
    }
}
