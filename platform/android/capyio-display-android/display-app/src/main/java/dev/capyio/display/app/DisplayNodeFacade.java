package dev.capyio.display.app;

import android.content.Context;
import android.content.Intent;
import java.io.File;
import java.io.IOException;
import java.util.Locale;
import org.json.JSONException;
import org.json.JSONObject;

/** Narrow same-APK entrypoint. No credentials or arbitrary paths cross the UI bridge. */
public final class DisplayNodeFacade {
    private DisplayNodeFacade() {}

    public static DisplayPrerequisites inspect(Context context) {
        boolean nativeAvailable;
        try {
            GameStreamNativeClient.requireAvailable();
            nativeAvailable = true;
        } catch (LinkageError unavailable) {
            nativeAvailable = false;
        }
        String transport;
        try {
            transport = ProvisionedDisplayConfiguration.loadTransportMode(context)
                    .name().toLowerCase(Locale.ROOT);
        } catch (IOException | RuntimeException invalid) {
            return new DisplayPrerequisites("unconfigured", false, nativeAvailable);
        }
        File directory = new File(context.getFilesDir(), ProvisionedDisplayConfiguration.DIRECTORY);
        boolean present = boundedFile(directory, "ca.der", 65536)
                && boundedFile(directory, "client-cert.der", 65536)
                && boundedFile(directory, "client-key.pk8", 65536)
                && boundedFile(directory, "gamestream".equals(transport)
                        ? "gamestream-launch.properties" : "connection.properties", 8192);
        if (present) {
            try {
                ProvisionedDisplayConfiguration.validateSelectedProperties(context, transport);
            } catch (IOException | RuntimeException invalid) {
                present = false;
            }
        }
        return new DisplayPrerequisites(transport, present, nativeAvailable);
    }

    private static boolean boundedFile(File directory, String name, long maximum) {
        File file = new File(directory, name);
        return file.isFile() && file.canRead() && file.length() > 0 && file.length() <= maximum;
    }

    public static String stateJson(Context context) {
        DisplayPrerequisites state = inspect(context);
        try {
            return new JSONObject().put("schemaVersion", 1)
                    .put("transport", state.transport())
                    .put("configurationPresent", state.configurationPresent())
                    .put("nativeClientAvailable", state.nativeClientAvailable())
                    .put("canOpen", state.canOpen())
                    .put("problemCode", state.problemCode()).toString();
        } catch (JSONException impossible) {
            throw new IllegalStateException("Display status serialization failed", impossible);
        }
    }

    public static boolean open(Context context) {
        if (!inspect(context).canOpen()) return false;
        // Only this fixed, unexported Activity is reachable. It revalidates its own configuration.
        context.startActivity(new Intent(context, DisplayActivity.class));
        return true;
    }
}
