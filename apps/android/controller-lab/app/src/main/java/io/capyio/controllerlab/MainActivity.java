package io.capyio.controllerlab;

import android.app.Activity;
import android.graphics.Color;
import android.hardware.Sensor;
import android.hardware.SensorEvent;
import android.hardware.SensorEventListener;
import android.hardware.SensorManager;
import android.os.Bundle;
import android.os.Build;
import android.text.InputFilter;
import android.text.InputType;
import android.view.View;
import android.view.Window;
import android.view.WindowInsets;
import android.view.WindowInsetsController;
import android.view.WindowManager;
import android.widget.Button;
import android.widget.EditText;
import android.widget.LinearLayout;
import android.widget.TextView;

import java.util.Locale;

public final class MainActivity extends Activity implements SensorEventListener {
    private final ControllerState controllerState = new ControllerState();
    private SensorManager sensorManager;
    private Sensor accelerometer;
    private Sensor gyroscope;
    private ControllerView controllerView;
    private EditText hostInput;
    private EditText portInput;
    private EditText tokenInput;
    private TextView statusText;
    private Button streamButton;
    private UdpControllerSender sender;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        requestWindowFeature(Window.FEATURE_NO_TITLE);
        getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        getWindow().setStatusBarColor(Color.rgb(8, 26, 20));
        getWindow().setNavigationBarColor(Color.rgb(8, 26, 20));
        buildUi();
        sensorManager = (SensorManager) getSystemService(SENSOR_SERVICE);
        accelerometer = sensorManager.getDefaultSensor(Sensor.TYPE_ACCELEROMETER);
        gyroscope = sensorManager.getDefaultSensor(Sensor.TYPE_GYROSCOPE);
        updateStatus("READY · 输入电脑地址与桌面令牌");
    }

    @Override
    protected void onResume() {
        super.onResume();
        hideSystemBars();
    }

    @Override
    protected void onPause() {
        stopStreaming();
        controllerView.cancelAllTouches();
        super.onPause();
    }

    @Override
    public void onSensorChanged(SensorEvent event) {
        if (event.sensor.getType() == Sensor.TYPE_ACCELEROMETER) {
            controllerState.setAcceleration(event.values[0], event.values[1], event.values[2], event.timestamp);
        } else if (event.sensor.getType() == Sensor.TYPE_GYROSCOPE) {
            controllerState.setAngularVelocity(event.values[0], event.values[1], event.values[2], event.timestamp);
        }
    }

    @Override
    public void onAccuracyChanged(Sensor sensor, int accuracy) {
        // The lab transports raw platform values; accuracy metadata is not promoted yet.
    }

    private void buildUi() {
        LinearLayout root = new LinearLayout(this);
        root.setOrientation(LinearLayout.VERTICAL);
        root.setBackgroundColor(Color.rgb(8, 26, 20));

        LinearLayout bar = new LinearLayout(this);
        bar.setOrientation(LinearLayout.HORIZONTAL);
        bar.setGravity(android.view.Gravity.CENTER_VERTICAL);
        int padding = dp(8);
        bar.setPadding(padding, dp(5), padding, dp(5));
        bar.setBackgroundColor(Color.rgb(13, 43, 33));

        hostInput = field("电脑 IPv4", "192.168.1.2", 15);
        hostInput.setInputType(InputType.TYPE_CLASS_PHONE);
        portInput = field("端口", "31580", 5);
        portInput.setInputType(InputType.TYPE_CLASS_NUMBER);
        tokenInput = field("桌面令牌", "", 64);
        tokenInput.setInputType(InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS);
        streamButton = new Button(this);
        streamButton.setText("开始");
        streamButton.setAllCaps(false);
        streamButton.setOnClickListener(view -> toggleStreaming());
        statusText = new TextView(this);
        statusText.setTextColor(0xff9fc4b5);
        statusText.setTextSize(11f);
        statusText.setSingleLine(true);

        bar.addView(hostInput, new LinearLayout.LayoutParams(0, dp(44), 1.35f));
        bar.addView(portInput, new LinearLayout.LayoutParams(0, dp(44), 0.58f));
        bar.addView(tokenInput, new LinearLayout.LayoutParams(0, dp(44), 1.1f));
        bar.addView(streamButton, new LinearLayout.LayoutParams(dp(78), dp(44)));
        bar.addView(statusText, new LinearLayout.LayoutParams(0, dp(44), 1.5f));

        controllerView = new ControllerView(this, controllerState, this::requestControlsFrame);
        root.addView(bar, new LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, dp(54)));
        root.addView(controllerView, new LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.MATCH_PARENT, 0, 1f));
        setContentView(root);
    }

    private EditText field(String hint, String value, int maxLength) {
        EditText field = new EditText(this);
        field.setHint(hint);
        field.setText(value);
        field.setTextColor(Color.WHITE);
        field.setHintTextColor(0xff6e9284);
        field.setTextSize(12f);
        field.setSingleLine(true);
        field.setFilters(new InputFilter[]{new InputFilter.LengthFilter(maxLength)});
        field.setPadding(dp(8), 0, dp(8), 0);
        return field;
    }

    private void toggleStreaming() {
        if (sender != null && sender.isRunning()) {
            stopStreaming();
            return;
        }
        String host = hostInput.getText().toString().trim();
        String token = tokenInput.getText().toString().trim().toLowerCase(Locale.ROOT);
        int port;
        try {
            port = Integer.parseInt(portInput.getText().toString().trim());
        } catch (NumberFormatException error) {
            updateStatus("ERROR · 端口格式错误");
            return;
        }
        if (!host.matches("(?:\\d{1,3}\\.){3}\\d{1,3}") || port < 1 || port > 65535) {
            updateStatus("ERROR · 请输入 IPv4 与有效端口");
            return;
        }
        if (!token.matches("[0-9a-fA-F]{8,64}")) {
            updateStatus("ERROR · 令牌须为 8–64 位十六进制");
            return;
        }
        sender = new UdpControllerSender(
                controllerState, host, port, token,
                message -> runOnUiThread(() -> updateStatus(message)));
        registerSensors();
        sender.start();
        streamButton.setText("停止");
        hostInput.setEnabled(false);
        portInput.setEnabled(false);
        tokenInput.setEnabled(false);
    }

    private void stopStreaming() {
        UdpControllerSender current = sender;
        sender = null;
        if (current != null) {
            current.stop();
        }
        sensorManager.unregisterListener(this);
        controllerState.resetControls();
        if (streamButton != null) {
            streamButton.setText("开始");
            hostInput.setEnabled(true);
            portInput.setEnabled(true);
            tokenInput.setEnabled(true);
            updateStatus("STOPPED · 已发送中立状态");
        }
    }

    private void registerSensors() {
        if (accelerometer != null) {
            sensorManager.registerListener(this, accelerometer, SensorManager.SENSOR_DELAY_GAME);
        }
        if (gyroscope != null) {
            sensorManager.registerListener(this, gyroscope, SensorManager.SENSOR_DELAY_GAME);
        }
        if (accelerometer == null || gyroscope == null) {
            updateStatus("WARNING · 设备缺少加速度计或陀螺仪");
        }
    }

    private void updateStatus(String value) {
        statusText.setText(value);
    }

    private void requestControlsFrame(boolean preserveEdge) {
        UdpControllerSender current = sender;
        if (current != null && current.isRunning()) {
            current.requestControlsFrame(preserveEdge);
        }
    }

    @SuppressWarnings("deprecation")
    private void hideSystemBars() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            WindowInsetsController controller = getWindow().getInsetsController();
            if (controller != null) {
                controller.hide(WindowInsets.Type.statusBars());
                controller.setSystemBarsBehavior(
                        WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
            }
        } else {
            getWindow().getDecorView().setSystemUiVisibility(
                    View.SYSTEM_UI_FLAG_FULLSCREEN
                            | View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
                            | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN);
        }
    }

    private int dp(int value) {
        return Math.round(value * getResources().getDisplayMetrics().density);
    }
}
