package dev.capyio.android;

import dev.capyio.android.contract.TouchpadContactSnapshot;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/** Deterministically reconstructs Android pointer actions from complete shared-UI frames. */
final class TouchpadMotionFrame {
    static final int ACTION_DOWN = 0;
    static final int ACTION_UP = 1;
    static final int ACTION_MOVE = 2;
    static final int ACTION_CANCEL = 3;
    static final int ACTION_POINTER_DOWN = 5;
    static final int ACTION_POINTER_UP = 6;
    private static final int TOOL_TYPE_FINGER = 1;

    final int action;
    final int actionIndex;
    final int[] ids;
    final int[] tools;
    final float[] x;
    final float[] y;
    final float[] pressure;

    private TouchpadMotionFrame(
            int action,
            int actionIndex,
            int[] ids,
            int[] tools,
            float[] x,
            float[] y,
            float[] pressure) {
        this.action = action;
        this.actionIndex = actionIndex;
        this.ids = ids;
        this.tools = tools;
        this.x = x;
        this.y = y;
        this.pressure = pressure;
    }

    static TouchpadMotionFrame between(
            List<TouchpadContactSnapshot> previous,
            List<TouchpadContactSnapshot> current) {
        Map<Integer, TouchpadContactSnapshot> before = index(previous);
        Map<Integer, TouchpadContactSnapshot> after = index(current);
        ArrayList<Integer> added = new ArrayList<>();
        ArrayList<Integer> removed = new ArrayList<>();
        after.keySet().stream().filter(id -> !before.containsKey(id)).forEach(added::add);
        before.keySet().stream().filter(id -> !after.containsKey(id)).forEach(removed::add);
        added.sort(Integer::compareTo);
        removed.sort(Integer::compareTo);

        if (added.size() + removed.size() > 1 || (!added.isEmpty() && !removed.isEmpty())) {
            return arrays(ACTION_CANCEL, 0, List.of());
        }
        if (!added.isEmpty()) {
            int addedId = added.get(0);
            return arrays(
                    previous.isEmpty() ? ACTION_DOWN : ACTION_POINTER_DOWN,
                    indexOf(current, addedId),
                    current);
        }
        if (!removed.isEmpty()) {
            int removedId = removed.get(0);
            return arrays(
                    current.isEmpty() ? ACTION_UP : ACTION_POINTER_UP,
                    indexOf(previous, removedId),
                    previous);
        }
        return arrays(current.isEmpty() ? ACTION_CANCEL : ACTION_MOVE, 0, current);
    }

    private static Map<Integer, TouchpadContactSnapshot> index(
            List<TouchpadContactSnapshot> contacts) {
        HashMap<Integer, TouchpadContactSnapshot> indexed = new HashMap<>();
        for (TouchpadContactSnapshot contact : contacts) {
            indexed.put(contact.contactId(), contact);
        }
        return indexed;
    }

    private static int indexOf(List<TouchpadContactSnapshot> contacts, int id) {
        for (int index = 0; index < contacts.size(); index++) {
            if (contacts.get(index).contactId() == id) {
                return index;
            }
        }
        throw new IllegalArgumentException("changed touchpad contact is absent");
    }

    private static TouchpadMotionFrame arrays(
            int action, int actionIndex, List<TouchpadContactSnapshot> contacts) {
        int[] ids = new int[contacts.size()];
        int[] tools = new int[contacts.size()];
        float[] x = new float[contacts.size()];
        float[] y = new float[contacts.size()];
        float[] pressure = new float[contacts.size()];
        for (int index = 0; index < contacts.size(); index++) {
            TouchpadContactSnapshot contact = contacts.get(index);
            ids[index] = contact.contactId();
            tools[index] = TOOL_TYPE_FINGER;
            x[index] = contact.x();
            y[index] = contact.y();
            pressure[index] = (float) contact.pressure() / TouchpadContactSnapshot.MAX_PRESSURE;
        }
        return new TouchpadMotionFrame(action, actionIndex, ids, tools, x, y, pressure);
    }
}
