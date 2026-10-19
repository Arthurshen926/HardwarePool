package dev.capyio.display.app;

import android.app.Activity;
import android.graphics.Color;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.util.Log;
import android.view.Gravity;
import android.view.SurfaceHolder;
import android.view.SurfaceView;
import android.view.View;
import android.view.ViewGroup;
import android.widget.FrameLayout;
import android.widget.TextView;
import android.widget.Button;
import android.view.WindowInsets;
import android.view.WindowInsetsController;
import android.view.WindowManager;
import dev.capyio.android.R;

/** Minimal application-owned Surface for the CapyIO virtual-monitor sink. */
public final class DisplayActivity extends Activity implements SurfaceHolder.Callback {
    private static final String TAG = "CapyIODisplay";
    private static final long METRICS_POLL_MILLIS = 500;

    private DisplaySurfaceController controller;
    private AuthenticatedDisplayTlsClient tlsClient;
    private AuthenticatedDisplayTlsClient.Configuration configuration;
    private ProvisionedDisplayConfiguration.GameStreamLaunch gameStreamLaunch;
    private AuthenticatedGameStreamReceiver gameStreamReceiver;
    private NodeDisplaySession nodeSession;
    private DisplayTransportMode transportMode;
    private FrameLayout root;
    private SurfaceView display;
    private TextView status;
    private int contentWidth;
    private int contentHeight;
    private int negotiatedRefresh;
    private int negotiatedBitrate;
    private boolean reportedPresentedFrame;
    private boolean showStatistics;
    private long previousFrames;
    private long previousSampleMillis;
    private final Handler mainHandler = new Handler(Looper.getMainLooper());
    private final Runnable gameStreamMetricsPoll = this::pollGameStreamMetrics;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        android.content.SharedPreferences viewing = getSharedPreferences("capyio-display-viewing", MODE_PRIVATE);
        showStatistics = viewing.getBoolean("showStatistics", false);
        if (viewing.getBoolean("keepAwake", true)) {
            getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        }

        root = new FrameLayout(this);
        root.setBackgroundColor(Color.BLACK);
        display = new SurfaceView(this);
        display.setVisibility(View.INVISIBLE);
        display.getHolder().addCallback(this);
        FrameLayout.LayoutParams displayLayout = new FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT);
        displayLayout.gravity = Gravity.CENTER;
        root.addView(display, displayLayout);
        root.addOnLayoutChangeListener((view, left, top, right, bottom,
                oldLeft, oldTop, oldRight, oldBottom) -> updateDisplayBounds());

        status = new TextView(this);
        status.setTextColor(Color.WHITE);
        status.setBackgroundColor(0x88000000);
        status.setPadding(24, 16, 24, 16);
        status.setText(R.string.display_waiting_surface);
        FrameLayout.LayoutParams statusLayout = new FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        statusLayout.gravity = Gravity.BOTTOM | Gravity.START;
        root.addView(status, statusLayout);
        Button exit = new Button(this);
        exit.setText("退出接收");
        exit.setContentDescription("退出画面接收；Windows 扩展屏需在发送端停止");
        exit.setOnClickListener(view -> finish());
        FrameLayout.LayoutParams exitLayout = new FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        exitLayout.gravity = Gravity.TOP | Gravity.END;
        root.addView(exit, exitLayout);
        setContentView(root);
        if (viewing.getBoolean("fullscreen", true) && android.os.Build.VERSION.SDK_INT >= 30) {
            WindowInsetsController insets = getWindow().getInsetsController();
            if (insets != null) {
                insets.setSystemBarsBehavior(WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
                insets.hide(WindowInsets.Type.systemBars());
            }
        }

        controller = new DisplaySurfaceController(
                mainHandler, next -> status.setText(next));
        try {
            String operation = getIntent().getStringExtra("nodeDisplayOperation");
            if(operation!=null) {
                nodeSession=NodeDisplaySession.attach(operation,()->runOnUiThread(this::finish));
                if(nodeSession==null)throw new IllegalStateException("display session is no longer active");
                exit.setText("停止共享");
                exit.setContentDescription("停止双方屏幕共享并恢复 Windows 显示器");
                transportMode=DisplayTransportMode.GAMESTREAM;
                contentWidth=nodeSession.expected.width(); contentHeight=nodeSession.expected.height();
                negotiatedRefresh=nodeSession.expected.refreshMillihertz();
                negotiatedBitrate=nodeSession.expected.bitrateKbps();
                updateDisplayBounds(); startTransportIfReady(display.getHolder()); return;
            }
            transportMode = ProvisionedDisplayConfiguration.loadTransportMode(this);
            if (transportMode == DisplayTransportMode.CPD1) {
                configuration = ProvisionedDisplayConfiguration.load(this);
                contentWidth = configuration.binding().width();
                contentHeight = configuration.binding().height();
                status.setText(R.string.display_cpd1_selected);
            } else {
                gameStreamLaunch = ProvisionedDisplayConfiguration.loadGameStreamLaunch(this);
                contentWidth = gameStreamLaunch.expected().width();
                contentHeight = gameStreamLaunch.expected().height();
                negotiatedRefresh=gameStreamLaunch.expected().refreshMillihertz();
                negotiatedBitrate=gameStreamLaunch.expected().bitrateKbps();
                status.setText(R.string.display_gamestream_selected);
            }
            updateDisplayBounds();
            startTransportIfReady(display.getHolder());
        } catch (Exception error) {
            status.setText(getString(R.string.display_provisioning_unavailable, error.getMessage()));
        }
    }

    @Override
    public void surfaceCreated(SurfaceHolder holder) {
        controller.surfaceAvailable(holder.getSurface());
        startTransportIfReady(holder);
    }

    private void startTransportIfReady(SurfaceHolder holder) {
        if (holder == null || holder.getSurface() == null || !holder.getSurface().isValid()) {
            return;
        }
        if (tlsClient != null) {
            return;
        }
        if (transportMode == DisplayTransportMode.GAMESTREAM && (gameStreamLaunch != null || nodeSession != null)) {
            if (gameStreamReceiver != null) {
                return;
            }
            try {
                gameStreamReceiver = nodeSession != null ? new AuthenticatedGameStreamReceiver(
                        nodeSession, holder.getSurface(),
                        next -> runOnUiThread(() -> { status.setVisibility(View.VISIBLE); status.setText(next); })) : new AuthenticatedGameStreamReceiver(
                        gameStreamLaunch,
                        holder.getSurface(),
                        next -> runOnUiThread(() -> { status.setVisibility(View.VISIBLE); status.setText(next); }));
                gameStreamReceiver.start();
                mainHandler.removeCallbacks(gameStreamMetricsPoll);
                mainHandler.postDelayed(gameStreamMetricsPoll, METRICS_POLL_MILLIS);
            } catch (Exception | LinkageError error) {
                if(gameStreamReceiver!=null)gameStreamReceiver.close();
                gameStreamReceiver = null;
                status.setText(getString(R.string.display_gamestream_unavailable, error.getMessage()));
                if(nodeSession!=null)nodeSession.close("receiver_failed_"+error.getClass().getSimpleName());
            }
            return;
        }
        if (transportMode != DisplayTransportMode.CPD1 || configuration == null) {
            return;
        }
        try {
            tlsClient = new AuthenticatedDisplayTlsClient(
                    configuration,
                    controller,
                    next -> runOnUiThread(() -> status.setText(next)));
            tlsClient.start();
        } catch (Exception error) {
            status.setText(getString(R.string.display_client_unavailable, error.getMessage()));
        }
    }

    @Override
    public void surfaceChanged(SurfaceHolder holder, int format, int width, int height) {
        // The Surface is fitted to the negotiated video aspect ratio; no Route mode change is implied.
    }

    @Override
    public void surfaceDestroyed(SurfaceHolder holder) {
        mainHandler.removeCallbacks(gameStreamMetricsPoll);
        if (gameStreamReceiver != null) {
            gameStreamReceiver.close();
            gameStreamReceiver = null;
        }
        controller.surfaceDestroyed();
    }

    @Override
    protected void onStop() {
        // A display Surface is visible-use only. Home/lock stops this Route,
        // not the Node service or concurrent audio/camera/input capabilities.
        if(nodeSession!=null&&!isChangingConfigurations()) {
            if(gameStreamReceiver!=null)gameStreamReceiver.close();
            nodeSession.close("");
        }
        super.onStop();
    }

    @Override
    protected void onDestroy() {
        if(nodeSession!=null)nodeSession.close("");
        mainHandler.removeCallbacks(gameStreamMetricsPoll);
        if (tlsClient != null) {
            tlsClient.close();
        }
        if (gameStreamReceiver != null) {
            gameStreamReceiver.close();
        }
        controller.close();
        super.onDestroy();
    }

    private void pollGameStreamMetrics() {
        AuthenticatedGameStreamReceiver receiver = gameStreamReceiver;
        if (receiver == null || !receiver.isRunning()) {
            return;
        }
        long presentedFrames = receiver.presentedFrames();
        long now = android.os.SystemClock.elapsedRealtime();
        if (presentedFrames > 0) {
            if(nodeSession!=null)nodeSession.presented(presentedFrames);
            long elapsed = now - previousSampleMillis;
            String metrics = DisplayReceiverPresentation.metrics(contentWidth,contentHeight,
                    negotiatedRefresh,negotiatedBitrate,presentedFrames,presentedFrames-previousFrames,
                    previousSampleMillis>0?elapsed:0);
            status.setText(metrics);
            status.setVisibility(showStatistics ? View.VISIBLE : View.GONE);
            if (!reportedPresentedFrame) {
                reportedPresentedFrame = true;
                Log.i(TAG, metrics);
            }
        }
        previousFrames = presentedFrames;
        previousSampleMillis = now;
        mainHandler.postDelayed(gameStreamMetricsPoll, METRICS_POLL_MILLIS);
    }

    private void updateDisplayBounds() {
        if (root == null || display == null || contentWidth <= 0 || contentHeight <= 0
                || root.getWidth() <= 0 || root.getHeight() <= 0) {
            return;
        }
        AspectRatioFit.Size fitted = AspectRatioFit.inside(
                root.getWidth(), root.getHeight(), contentWidth, contentHeight);
        FrameLayout.LayoutParams current = (FrameLayout.LayoutParams) display.getLayoutParams();
        if (current.width == fitted.width() && current.height == fitted.height()
                && current.gravity == Gravity.CENTER) {
            if (display.getVisibility() != View.VISIBLE) {
                display.setVisibility(View.VISIBLE);
            }
            return;
        }
        current.width = fitted.width();
        current.height = fitted.height();
        current.gravity = Gravity.CENTER;
        display.setLayoutParams(current);
        // Do not require a second layout callback to create the Surface.
        // An initially INVISIBLE SurfaceView has no displayable Surface.
        display.setVisibility(View.VISIBLE);
    }
}
