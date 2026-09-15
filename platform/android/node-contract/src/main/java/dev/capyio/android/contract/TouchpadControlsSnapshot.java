package dev.capyio.android.contract;

import java.util.HashSet;
import java.util.List;

/** Complete bounded normalized touchpad state owned by the Android Node service. */
public record TouchpadControlsSnapshot(long sequence, List<TouchpadContactSnapshot> contacts) {
    public static final int MAX_CONTACTS = 5;

    public TouchpadControlsSnapshot {
        if (sequence < 0) {
            throw new IllegalArgumentException("touchpad sequence must not be negative");
        }
        if (contacts == null || contacts.size() > MAX_CONTACTS) {
            throw new IllegalArgumentException("touchpad contacts exceed the supported bound");
        }
        contacts = List.copyOf(contacts);
        HashSet<Integer> ids = new HashSet<>();
        for (TouchpadContactSnapshot contact : contacts) {
            if (contact == null || !ids.add(contact.contactId())) {
                throw new IllegalArgumentException("touchpad contact IDs must be present and unique");
            }
        }
    }

    public static TouchpadControlsSnapshot neutral(long sequence) {
        return new TouchpadControlsSnapshot(sequence, List.of());
    }
}
