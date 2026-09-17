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
    private DisplayTransportMode transportMode;
    private FrameLayout root;
    private SurfaceView display;
    private TextView status;
    private int contentWidth;
    private int contentHeight;
    private boolean reportedPresentedFrame;
    private final Handler mainHandler = new Handler(Looper.getMainLooper());
    private final Runnable gameStreamMetricsPoll = this::pollGameStreamMetrics;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);

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
        setContentView(root);

        controller = new DisplaySurfaceController(
                mainHandler, next -> status.setText(next));
        try {
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
        if (transportMode == DisplayTransportMode.GAMESTREAM && gameStreamLaunch != null) {
            if (gameStreamReceiver != null) {
                return;
            }
            try {
                gameStreamReceiver = new AuthenticatedGameStreamReceiver(
                        gameStreamLaunch,
                        holder.getSurface(),
                        next -> runOnUiThread(() -> status.setText(next)));
                gameStreamReceiver.start();
                mainHandler.removeCallbacks(gameStreamMetricsPoll);
                mainHandler.postDelayed(gameStreamMetricsPoll, METRICS_POLL_MILLIS);
            } catch (Exception | LinkageError error) {
                gameStreamReceiver = null;
                status.setText(getString(R.string.display_gamestream_unavailable, error.getMessage()));
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
    protected void onDestroy() {
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
        if (presentedFrames > 0) {
            String metrics = "GameStream video connected; frames presented: " + presentedFrames;
            status.setText(metrics);
            if (!reportedPresentedFrame) {
                reportedPresentedFrame = true;
                Log.i(TAG, metrics);
            }
        }
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
    }
}
