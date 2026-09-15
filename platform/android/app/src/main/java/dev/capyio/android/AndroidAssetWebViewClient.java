package dev.capyio.android;

import android.content.res.AssetManager;
import android.net.Uri;
import android.webkit.WebResourceRequest;
import android.webkit.WebResourceResponse;
import android.webkit.WebView;
import android.webkit.WebViewClient;

import java.io.ByteArrayInputStream;
import java.io.IOException;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.util.Map;

/** Same-origin APK asset loader that fails closed for every non-CapyIO request. */
final class AndroidAssetWebViewClient extends WebViewClient {
    static final String ORIGIN = "https://appassets.androidplatform.net";
    private static final String PATH_PREFIX = "/capyio-ui/";
    private static final int MAX_PATH_LENGTH = 240;

    private final AssetManager assets;

    AndroidAssetWebViewClient(AssetManager assets) {
        this.assets = assets;
    }

    @Override
    public boolean shouldOverrideUrlLoading(WebView view, WebResourceRequest request) {
        return !isAllowed(request.getUrl());
    }

    @Override
    public WebResourceResponse shouldInterceptRequest(
            WebView view,
            WebResourceRequest request) {
        Uri uri = request.getUrl();
        if (!isAllowed(uri)) {
            return denied(403, "Forbidden");
        }
        String assetPath = uri.getPath().substring(1);
        try {
            InputStream stream = assets.open(assetPath, AssetManager.ACCESS_STREAMING);
            return new WebResourceResponse(mimeType(assetPath), "UTF-8", stream);
        } catch (IOException missing) {
            return denied(404, "Not Found");
        }
    }

    static boolean isAllowed(Uri uri) {
        if (uri == null
                || !"https".equals(uri.getScheme())
                || !"appassets.androidplatform.net".equals(uri.getHost())
                || uri.getPort() != -1) {
            return false;
        }
        String path = uri.getPath();
        return path != null
                && path.length() <= MAX_PATH_LENGTH
                && path.startsWith(PATH_PREFIX)
                && !path.contains("..")
                && !path.contains("\\")
                && !path.contains("//");
    }

    private static String mimeType(String path) {
        if (path.endsWith(".html")) return "text/html";
        if (path.endsWith(".js")) return "text/javascript";
        if (path.endsWith(".css")) return "text/css";
        if (path.endsWith(".json")) return "application/json";
        if (path.endsWith(".svg")) return "image/svg+xml";
        if (path.endsWith(".png")) return "image/png";
        if (path.endsWith(".woff2")) return "font/woff2";
        return "application/octet-stream";
    }

    private static WebResourceResponse denied(int statusCode, String reason) {
        byte[] body = reason.getBytes(StandardCharsets.UTF_8);
        return new WebResourceResponse(
                "text/plain",
                "UTF-8",
                statusCode,
                reason,
                Map.of("Cache-Control", "no-store"),
                new ByteArrayInputStream(body));
    }
}
