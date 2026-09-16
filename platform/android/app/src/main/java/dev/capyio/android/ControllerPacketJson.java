package dev.capyio.android;

import dev.capyio.android.contract.GamepadControlsSnapshot;

/** Pure encoder for the existing bounded Android Controller Lab v1 datagram. */
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
        return new StringBuilder(640)
                .append('{')
                .append("\"version\":1,")
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
                .append(Math.max(1, accelerationTimestampNanos)).append(',')
                .append("\"angularVelocityTimestampNanos\":")
                .append(Math.max(1, angularVelocityTimestampNanos))
                .append('}')
                .toString();
    }
}
