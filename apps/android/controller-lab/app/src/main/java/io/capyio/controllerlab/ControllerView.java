package io.capyio.controllerlab;

import android.content.Context;
import android.annotation.SuppressLint;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Paint;
import android.graphics.RectF;
import android.util.SparseArray;
import android.view.MotionEvent;
import android.view.View;

@SuppressLint("ViewConstructor")
final class ControllerView extends View {
    private static final float DEAD_ZONE = 0.12f;
    private static final int KIND_BUTTON = 1;
    private static final int KIND_DPAD = 2;
    private static final int KIND_LEFT_STICK = 3;
    private static final int KIND_RIGHT_STICK = 4;
    private static final int KIND_LEFT_TRIGGER = 5;
    private static final int KIND_RIGHT_TRIGGER = 6;

    private final ControllerState state;
    private final ControlsFrameListener controlsFrameListener;
    private final Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final SparseArray<Binding> bindings = new SparseArray<>();
    private final RectF dpad = new RectF();
    private final RectF leftTrigger = new RectF();
    private final RectF rightTrigger = new RectF();
    private final RectF leftShoulder = new RectF();
    private final RectF rightShoulder = new RectF();
    private float leftStickX;
    private float leftStickY;
    private float rightStickX;
    private float rightStickY;
    private float stickRadius;
    private float faceRadius;
    private final float[][] faceCenters = new float[4][2];
    private final float[][] systemCenters = new float[3][2];

    ControllerView(Context context, ControllerState state, ControlsFrameListener controlsFrameListener) {
        super(context);
        this.state = state;
        this.controlsFrameListener = controlsFrameListener;
        setBackgroundColor(Color.rgb(8, 26, 20));
        setFocusable(true);
    }

    @Override
    protected void onSizeChanged(int width, int height, int oldWidth, int oldHeight) {
        float unit = Math.min(width / 12.5f, height / 5.6f);
        stickRadius = unit * 0.78f;
        faceRadius = unit * 0.35f;
        leftStickX = width * 0.32f;
        leftStickY = height * 0.72f;
        rightStickX = width * 0.68f;
        rightStickY = height * 0.72f;
        float dpadSize = unit * 2.1f;
        dpad.set(width * 0.13f - dpadSize / 2f, height * 0.42f - dpadSize / 2f,
                width * 0.13f + dpadSize / 2f, height * 0.42f + dpadSize / 2f);
        float faceX = width * 0.87f;
        float faceY = height * 0.42f;
        float spacing = unit * 0.72f;
        faceCenters[0] = new float[]{faceX, faceY + spacing};
        faceCenters[1] = new float[]{faceX + spacing, faceY};
        faceCenters[2] = new float[]{faceX - spacing, faceY};
        faceCenters[3] = new float[]{faceX, faceY - spacing};
        float shoulderHeight = Math.max(42f, height * 0.14f);
        leftShoulder.set(width * 0.03f, 10f, width * 0.27f, shoulderHeight);
        rightShoulder.set(width * 0.73f, 10f, width * 0.97f, shoulderHeight);
        leftTrigger.set(width * 0.29f, 10f, width * 0.39f, shoulderHeight);
        rightTrigger.set(width * 0.61f, 10f, width * 0.71f, shoulderHeight);
        systemCenters[0] = new float[]{width * 0.44f, height * 0.39f};
        systemCenters[1] = new float[]{width * 0.56f, height * 0.39f};
        systemCenters[2] = new float[]{width * 0.50f, height * 0.29f};
    }

    @Override
    protected void onDraw(Canvas canvas) {
        super.onDraw(canvas);
        ControllerState.Snapshot snapshot = state.snapshot();
        paint.setTextAlign(Paint.Align.CENTER);
        drawHeaderControl(canvas, leftShoulder, "L1", (snapshot.buttons() & ControllerState.LEFT_SHOULDER) != 0);
        drawHeaderControl(canvas, rightShoulder, "R1", (snapshot.buttons() & ControllerState.RIGHT_SHOULDER) != 0);
        drawTrigger(canvas, leftTrigger, "L2", snapshot.leftTrigger());
        drawTrigger(canvas, rightTrigger, "R2", snapshot.rightTrigger());
        drawDpad(canvas, snapshot.dpadX(), snapshot.dpadY());
        drawStick(canvas, leftStickX, leftStickY, snapshot.leftX(), snapshot.leftY(), "L");
        drawStick(canvas, rightStickX, rightStickY, snapshot.rightX(), snapshot.rightY(), "R");
        int[] faceBits = {ControllerState.SOUTH, ControllerState.EAST, ControllerState.WEST, ControllerState.NORTH};
        String[] faceLabels = {"A", "B", "X", "Y"};
        int[] faceColors = {0xff6de4aa, 0xffff8686, 0xff76c9ff, 0xffffcf73};
        for (int index = 0; index < faceBits.length; index++) {
            drawRoundButton(canvas, faceCenters[index][0], faceCenters[index][1], faceRadius,
                    faceLabels[index], (snapshot.buttons() & faceBits[index]) != 0, faceColors[index]);
        }
        drawRoundButton(canvas, systemCenters[0][0], systemCenters[0][1], faceRadius * 0.62f,
                "SELECT", (snapshot.buttons() & ControllerState.SELECT) != 0, 0xffd8e9e1);
        drawRoundButton(canvas, systemCenters[1][0], systemCenters[1][1], faceRadius * 0.62f,
                "START", (snapshot.buttons() & ControllerState.START) != 0, 0xffd8e9e1);
        drawRoundButton(canvas, systemCenters[2][0], systemCenters[2][1], faceRadius * 0.78f,
                "IO", (snapshot.buttons() & ControllerState.GUIDE) != 0, 0xff58d7a1);
        paint.setTextSize(Math.max(12f, getHeight() * 0.035f));
        paint.setColor(0xff719487);
        canvas.drawText("CAPYIO · TOUCH + IMU", getWidth() * 0.5f, getHeight() * 0.94f, paint);
    }

    @Override
    public boolean onTouchEvent(MotionEvent event) {
        int action = event.getActionMasked();
        int actionIndex = event.getActionIndex();
        if (action == MotionEvent.ACTION_DOWN || action == MotionEvent.ACTION_POINTER_DOWN) {
            int pointerId = event.getPointerId(actionIndex);
            Binding binding = hitTest(event.getX(actionIndex), event.getY(actionIndex));
            if (binding != null) {
                bindings.put(pointerId, binding);
                update(binding, event.getX(actionIndex), event.getY(actionIndex), true);
                controlsFrameListener.onControlsChanged(true);
            }
        } else if (action == MotionEvent.ACTION_MOVE) {
            for (int index = 0; index < event.getPointerCount(); index++) {
                Binding binding = bindings.get(event.getPointerId(index));
                if (binding != null) {
                    update(binding, event.getX(index), event.getY(index), true);
                }
            }
            controlsFrameListener.onControlsChanged(false);
        } else if (action == MotionEvent.ACTION_UP || action == MotionEvent.ACTION_POINTER_UP) {
            int pointerId = event.getPointerId(actionIndex);
            Binding binding = bindings.get(pointerId);
            if (binding != null) {
                update(binding, event.getX(actionIndex), event.getY(actionIndex), false);
                controlsFrameListener.onControlsChanged(true);
                bindings.remove(pointerId);
            }
            if (action == MotionEvent.ACTION_UP) {
                performClick();
            }
        } else if (action == MotionEvent.ACTION_CANCEL) {
            cancelAllTouches();
        }
        invalidate();
        return true;
    }

    @Override
    public boolean performClick() {
        super.performClick();
        return true;
    }

    void cancelAllTouches() {
        bindings.clear();
        state.resetControls();
        controlsFrameListener.onControlsChanged(true);
        invalidate();
    }

    private Binding hitTest(float x, float y) {
        if (leftShoulder.contains(x, y)) return new Binding(KIND_BUTTON, ControllerState.LEFT_SHOULDER);
        if (rightShoulder.contains(x, y)) return new Binding(KIND_BUTTON, ControllerState.RIGHT_SHOULDER);
        if (leftTrigger.contains(x, y)) return new Binding(KIND_LEFT_TRIGGER, 0);
        if (rightTrigger.contains(x, y)) return new Binding(KIND_RIGHT_TRIGGER, 0);
        int[] faceBits = {ControllerState.SOUTH, ControllerState.EAST, ControllerState.WEST, ControllerState.NORTH};
        for (int index = 0; index < faceCenters.length; index++) {
            if (distance(x, y, faceCenters[index][0], faceCenters[index][1]) <= faceRadius * 1.35f) {
                return new Binding(KIND_BUTTON, faceBits[index]);
            }
        }
        int[] systemBits = {ControllerState.SELECT, ControllerState.START, ControllerState.GUIDE};
        for (int index = 0; index < systemCenters.length; index++) {
            if (distance(x, y, systemCenters[index][0], systemCenters[index][1]) <= faceRadius) {
                return new Binding(KIND_BUTTON, systemBits[index]);
            }
        }
        if (distance(x, y, leftStickX, leftStickY) <= stickRadius * 1.25f) return new Binding(KIND_LEFT_STICK, 0);
        if (distance(x, y, rightStickX, rightStickY) <= stickRadius * 1.25f) return new Binding(KIND_RIGHT_STICK, 0);
        if (dpad.contains(x, y)) return new Binding(KIND_DPAD, 0);
        return null;
    }

    private void update(Binding binding, float x, float y, boolean active) {
        if (binding.kind == KIND_BUTTON) {
            state.setButton(binding.value, active);
        } else if (binding.kind == KIND_DPAD) {
            if (!active) {
                state.setDpad(0, 0);
            } else {
                float normalizedX = (x - dpad.centerX()) / (dpad.width() * 0.5f);
                float normalizedY = (y - dpad.centerY()) / (dpad.height() * 0.5f);
                int dx = Math.abs(normalizedX) > 0.25f ? (normalizedX > 0 ? 1 : -1) : 0;
                int dy = Math.abs(normalizedY) > 0.25f ? (normalizedY > 0 ? -1 : 1) : 0;
                state.setDpad(dx, dy);
            }
        } else if (binding.kind == KIND_LEFT_STICK || binding.kind == KIND_RIGHT_STICK) {
            if (!active) {
                state.setStick(binding.kind == KIND_LEFT_STICK, 0, 0);
            } else {
                updateStick(binding.kind == KIND_LEFT_STICK, x, y);
            }
        } else if (binding.kind == KIND_LEFT_TRIGGER || binding.kind == KIND_RIGHT_TRIGGER) {
            RectF bounds = binding.kind == KIND_LEFT_TRIGGER ? leftTrigger : rightTrigger;
            int value = active
                    ? Math.round(Math.max(0f, Math.min(1f, (bounds.bottom - y) / bounds.height())) * 65535f)
                    : 0;
            state.setTrigger(binding.kind == KIND_LEFT_TRIGGER, value);
        }
    }

    private void updateStick(boolean left, float x, float y) {
        float centerX = left ? leftStickX : rightStickX;
        float centerY = left ? leftStickY : rightStickY;
        float dx = (x - centerX) / stickRadius;
        float dy = (centerY - y) / stickRadius;
        float magnitude = (float) Math.sqrt(dx * dx + dy * dy);
        if (magnitude <= DEAD_ZONE) {
            state.setStick(left, 0, 0);
            return;
        }
        if (magnitude > 1f) {
            dx /= magnitude;
            dy /= magnitude;
            magnitude = 1f;
        }
        float scaled = (magnitude - DEAD_ZONE) / (1f - DEAD_ZONE);
        float divisor = Math.max(magnitude, 0.0001f);
        state.setStick(left,
                Math.round(dx / divisor * scaled * 32767f),
                Math.round(dy / divisor * scaled * 32767f));
    }

    private void drawHeaderControl(Canvas canvas, RectF bounds, String label, boolean pressed) {
        paint.setColor(pressed ? 0xff58d7a1 : 0xff233c33);
        canvas.drawRoundRect(bounds, 18f, 18f, paint);
        drawLabel(canvas, label, bounds.centerX(), bounds.centerY(), pressed ? 0xff082219 : 0xffd8e9e1, bounds.height() * 0.38f);
    }

    private void drawTrigger(Canvas canvas, RectF bounds, String label, int value) {
        paint.setColor(0xff183128);
        canvas.drawRoundRect(bounds, 14f, 14f, paint);
        float ratio = value / 65535f;
        paint.setColor(0xff58d7a1);
        canvas.drawRoundRect(new RectF(bounds.left, bounds.bottom - bounds.height() * ratio, bounds.right, bounds.bottom), 14f, 14f, paint);
        drawLabel(canvas, label, bounds.centerX(), bounds.centerY(), 0xffeaf8f1, bounds.height() * 0.34f);
    }

    private void drawDpad(Canvas canvas, int dx, int dy) {
        float third = dpad.width() / 3f;
        RectF horizontal = new RectF(dpad.left, dpad.top + third, dpad.right, dpad.bottom - third);
        RectF vertical = new RectF(dpad.left + third, dpad.top, dpad.right - third, dpad.bottom);
        paint.setColor(0xff233c33);
        canvas.drawRoundRect(horizontal, 12f, 12f, paint);
        canvas.drawRoundRect(vertical, 12f, 12f, paint);
        paint.setColor(0xff58d7a1);
        if (dx != 0 || dy != 0) {
            float cx = dpad.centerX() + dx * third;
            float cy = dpad.centerY() - dy * third;
            canvas.drawCircle(cx, cy, third * 0.36f, paint);
        }
    }

    private void drawStick(Canvas canvas, float cx, float cy, int axisX, int axisY, String label) {
        paint.setStyle(Paint.Style.STROKE);
        paint.setStrokeWidth(3f);
        paint.setColor(0xff345348);
        canvas.drawCircle(cx, cy, stickRadius, paint);
        paint.setStyle(Paint.Style.FILL);
        float knobX = cx + axisX / 32767f * stickRadius * 0.63f;
        float knobY = cy - axisY / 32767f * stickRadius * 0.63f;
        paint.setColor(0xff2b493e);
        canvas.drawCircle(knobX, knobY, stickRadius * 0.43f, paint);
        drawLabel(canvas, label, knobX, knobY, 0xff9ec5b5, stickRadius * 0.3f);
    }

    private void drawRoundButton(Canvas canvas, float cx, float cy, float radius, String label, boolean pressed, int color) {
        paint.setColor(pressed ? color : 0xff263e35);
        canvas.drawCircle(cx, cy, radius, paint);
        drawLabel(canvas, label, cx, cy, pressed ? 0xff092019 : color, Math.min(radius * 0.72f, 25f));
    }

    private void drawLabel(Canvas canvas, String label, float x, float y, int color, float size) {
        paint.setColor(color);
        paint.setTextSize(Math.max(10f, size));
        paint.setTypeface(android.graphics.Typeface.DEFAULT_BOLD);
        Paint.FontMetrics metrics = paint.getFontMetrics();
        canvas.drawText(label, x, y - (metrics.ascent + metrics.descent) / 2f, paint);
    }

    private static float distance(float x, float y, float cx, float cy) {
        return (float) Math.hypot(x - cx, y - cy);
    }

    private static final class Binding {
        final int kind;
        final int value;

        Binding(int kind, int value) {
            this.kind = kind;
            this.value = value;
        }
    }

    interface ControlsFrameListener {
        void onControlsChanged(boolean preserveEdge);
    }
}
