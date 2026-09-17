package dev.capyio.display.app;

import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.InputStream;
import java.io.OutputStream;
import java.util.Arrays;
import java.util.Objects;

/** Transport-neutral fixed launch exchange used only after authentication. */
final class GameStreamLaunchExchange {
    private GameStreamLaunchExchange() {}

    static GameStreamLaunchRecord receiveAndAcknowledge(
            InputStream input,
            OutputStream output,
            GameStreamLaunchRecord.Expected expected) throws Exception {
        Objects.requireNonNull(input, "input");
        Objects.requireNonNull(output, "output");
        Objects.requireNonNull(expected, "expected");
        byte[] encoded = new byte[GameStreamLaunchRecord.RECORD_BYTES];
        GameStreamLaunchRecord launch = null;
        try {
            new DataInputStream(input).readFully(encoded);
            launch = GameStreamLaunchRecord.parse(encoded, expected);
            DataOutputStream dataOutput = new DataOutputStream(output);
            dataOutput.write(launch.acknowledgement());
            dataOutput.flush();
            return launch;
        } catch (Exception error) {
            if (launch != null) {
                launch.close();
            }
            throw error;
        } finally {
            Arrays.fill(encoded, (byte) 0);
        }
    }
}
