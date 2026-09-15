package dev.capyio.android;

import dev.capyio.android.contract.TouchpadContactSnapshot;
import dev.capyio.android.contract.TouchpadControlsSnapshot;

import java.util.ArrayList;
import java.util.List;

/** Parses the closed complete-contact grammar used by the packaged shared UI. */
public final class AndroidTouchpadUiFrame {
    private static final int MAX_ENCODED_LENGTH = 160;

    private AndroidTouchpadUiFrame() {}

    public static List<TouchpadContactSnapshot> parse(String encoded) {
        if (encoded == null || encoded.length() > MAX_ENCODED_LENGTH) {
            return null;
        }
        if (encoded.isEmpty()) {
            return List.of();
        }
        String[] contactFields = encoded.split(";", -1);
        if (contactFields.length > TouchpadControlsSnapshot.MAX_CONTACTS) {
            return null;
        }
        ArrayList<TouchpadContactSnapshot> contacts = new ArrayList<>(contactFields.length);
        try {
            for (String contactField : contactFields) {
                String[] values = contactField.split(",", -1);
                if (values.length != 4) {
                    return null;
                }
                for (String value : values) {
                    if (!isCanonicalUnsignedInteger(value)) {
                        return null;
                    }
                }
                contacts.add(new TouchpadContactSnapshot(
                        Integer.parseInt(values[0]),
                        Integer.parseInt(values[1]),
                        Integer.parseInt(values[2]),
                        Integer.parseInt(values[3])));
            }
            return new TouchpadControlsSnapshot(0, contacts).contacts();
        } catch (IllegalArgumentException ignored) {
            return null;
        }
    }

    private static boolean isCanonicalUnsignedInteger(String value) {
        if (value.isEmpty() || value.length() > 5
                || (value.length() > 1 && value.charAt(0) == '0')) {
            return false;
        }
        for (int index = 0; index < value.length(); index++) {
            char character = value.charAt(index);
            if (character < '0' || character > '9') {
                return false;
            }
        }
        return true;
    }
}
