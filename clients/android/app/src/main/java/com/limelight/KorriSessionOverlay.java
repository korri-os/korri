package com.limelight;

import android.annotation.SuppressLint;
import android.view.ViewGroup;
import android.webkit.JavascriptInterface;
import android.webkit.WebResourceRequest;
import android.webkit.WebResourceResponse;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.widget.FrameLayout;

import androidx.webkit.WebViewAssetLoader;

import com.limelight.nvstream.jni.MoonBridge;

import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

import java.util.ArrayList;
import java.util.List;
import java.util.Locale;

/**
 * Korri session lifecycle overlay: a portal-origin WebView covering the
 * stream surface from Activity start until the connection-established
 * signal, so Korri-initiated streams never show native connecting UI.
 *
 * Mirrors the treaty in contracts/bridge/korri-native-bridge.ts
 * (KorriSessionBridgeSurface, StreamLifecycleEvent, session-screen entry).
 * When the two sides disagree, the contracts file wins.
 *
 * Contract shape is pull-then-push: every published event is retained in
 * an ordered log that `lifecycleSnapshot()` replays, so the overlay's JS
 * can boot after stages have already fired (or after the stream ended)
 * without losing the story.
 */
public class KorriSessionOverlay {

    private final Game game;
    private final List<String> eventLog = new ArrayList<>();
    private WebView webView;

    public KorriSessionOverlay(Game game) {
        this.game = game;
    }

    /**
     * Attach the overlay above the stream surface. No-op when already
     * attached. Re-attach after reveal() is sanctioned: the event log
     * survives, so a post-reveal termination can boot a fresh overlay that
     * renders the failure from its snapshot pull. UI thread only.
     */
    @SuppressLint("SetJavaScriptEnabled")
    public void attach() {
        if (webView != null) return;
        webView = new WebView(game);
        webView.getSettings().setJavaScriptEnabled(true);
        webView.getSettings().setDomStorageEnabled(true);
        webView.setBackgroundColor(0xFF101018);
        webView.setElevation(9000f);
        // Same asset-loader arrangement as KorriShellActivity: the bundled
        // portal needs a real https origin for ES modules, and loopback
        // korrid is exempt from mixed-content blocking. Deliberately
        // duplicated rather than extracted — the setup is a handful of
        // lines and the two hosts differ in everything around it.
        final WebViewAssetLoader assetLoader = new WebViewAssetLoader.Builder()
                .addPathHandler("/assets/", new WebViewAssetLoader.AssetsPathHandler(game))
                .build();
        webView.setWebViewClient(new WebViewClient() {
            @Override
            public WebResourceResponse shouldInterceptRequest(
                    WebView view, WebResourceRequest request) {
                return assetLoader.shouldInterceptRequest(request.getUrl());
            }
        });
        webView.addJavascriptInterface(new SessionBridge(), "KorriSession");
        ViewGroup root = game.findViewById(android.R.id.content);
        root.addView(webView, new FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));
        webView.loadUrl(sessionUrl());
    }

    /**
     * Frames are imminent (or the Activity is going away): remove and
     * destroy the overlay WebView so it costs nothing during streaming.
     * UI thread only.
     */
    public void reveal() {
        if (webView == null) return;
        ViewGroup parent = (ViewGroup) webView.getParent();
        if (parent != null) {
            parent.removeView(webView);
        }
        webView.destroy();
        webView = null;
    }

    /**
     * Record a treaty StreamLifecycleEvent and push it to the overlay's JS
     * when one is attached. Safe from any thread.
     */
    public void publish(JSONObject event) {
        final String json = event.toString();
        synchronized (eventLog) {
            eventLog.add(json);
        }
        game.runOnUiThread(() -> {
            if (webView != null) {
                webView.evaluateJavascript(
                        "window.__korriSessionEvent && window.__korriSessionEvent("
                                + JSONObject.quote(json) + ")", null);
            }
        });
    }

    private String sessionUrl() {
        String devUrl = BuildConfig.PORTAL_DEV_URL;
        String base = (BuildConfig.DEBUG && devUrl != null && !devUrl.isEmpty())
                ? devUrl
                : "https://appassets.androidplatform.net/assets/portal/index.html";
        // Treaty session-screen entry: SESSION_SCREEN_PARAM=SESSION_SCREEN_VALUE.
        return base + (base.contains("?") ? "&" : "?") + "screen=session";
    }

    // --- Treaty event vocabulary (StreamLifecycleEvent) -----------------

    /** Map a raw Moonlight stage string onto the treaty's StreamStageId. */
    public static String semanticStage(String rawStage, String appName) {
        if (rawStage == null) return "initializing";
        if (rawStage.equals(appName)) return "launching-app";
        String stage = rawStage.toLowerCase(Locale.ROOT);
        // "audio stream initialization" fires BEFORE the RTSP handshake in
        // moonlight-common-c's stage order, so it belongs to the pre-
        // handshake phase; the remaining stream init/establishment stages
        // come after it.
        if (stage.contains("platform") || stage.contains("name resolution")
                || stage.contains("audio stream initialization")) {
            return "initializing";
        }
        if (stage.contains("rtsp")) return "handshaking";
        return "establishing-streams";
    }

    /** Map a stage failure onto the treaty's StreamFailureReason. */
    public static String failureReason(
            String rawStage, String appName, int errorCode, boolean portsBlocked) {
        if (portsBlocked || errorCode == -408) return "HostUnreachable";
        if (errorCode == 403) return "PermissionDenied";
        if (rawStage != null && rawStage.equals(appName)) return "AppLaunchFailed";
        if (rawStage != null && rawStage.toLowerCase(Locale.ROOT).contains("video")) {
            return "DecoderInitFailed";
        }
        return "Unknown";
    }

    /** Map a termination error code onto the treaty's StreamFailureReason. */
    public static String terminationReason(int errorCode) {
        switch (errorCode) {
            case MoonBridge.ML_ERROR_NO_VIDEO_TRAFFIC:
            case MoonBridge.ML_ERROR_NO_VIDEO_FRAME:
                return "NoVideoTraffic";
            default:
                return "ConnectionLost";
        }
    }

    public static JSONObject stageStartingEvent(String rawStage, String appName) {
        return stageEvent("stage-starting", rawStage, appName);
    }

    public static JSONObject stageCompleteEvent(String rawStage, String appName) {
        return stageEvent("stage-complete", rawStage, appName);
    }

    private static JSONObject stageEvent(String type, String rawStage, String appName) {
        try {
            return new JSONObject()
                    .put("type", type)
                    .put("stage", semanticStage(rawStage, appName))
                    .put("detail", rawStage);
        } catch (JSONException e) {
            throw new IllegalStateException(e);
        }
    }

    public static JSONObject connectedEvent() {
        try {
            return new JSONObject().put("type", "connected");
        } catch (JSONException e) {
            throw new IllegalStateException(e);
        }
    }

    /** Failure before Moonlight can emit a native connection stage. */
    public static JSONObject decoderUnsupportedEvent() {
        try {
            return new JSONObject()
                    .put("type", "failed")
                    .put("reason", "DecoderInitFailed")
                    .put("stage", "initializing")
                    .put("detail", "hardware accelerated H.264 is unavailable");
        } catch (JSONException e) {
            throw new IllegalStateException(e);
        }
    }

    public static JSONObject failedEvent(
            String rawStage, String appName, int errorCode, boolean portsBlocked) {
        try {
            return new JSONObject()
                    .put("type", "failed")
                    .put("reason", failureReason(rawStage, appName, errorCode, portsBlocked))
                    .put("stage", semanticStage(rawStage, appName))
                    .put("errorCode", errorCode)
                    .put("detail", rawStage);
        } catch (JSONException e) {
            throw new IllegalStateException(e);
        }
    }

    public static JSONObject terminatedEvent(boolean graceful, int errorCode) {
        try {
            return new JSONObject()
                    .put("type", "terminated")
                    .put("graceful", graceful)
                    .put("reason", graceful ? "Unknown" : terminationReason(errorCode))
                    .put("errorCode", errorCode);
        } catch (JSONException e) {
            throw new IllegalStateException(e);
        }
    }

    /** Mirrors KorriSessionBridgeSurface in the treaty. */
    private class SessionBridge {

        /** JSON-encoded StreamLifecycleSnapshot: the event log so far. */
        @JavascriptInterface
        public String lifecycleSnapshot() {
            try {
                JSONArray events = new JSONArray();
                synchronized (eventLog) {
                    for (String json : eventLog) {
                        events.put(new JSONObject(json));
                    }
                }
                return new JSONObject().put("events", events).toString();
            } catch (JSONException e) {
                return "{\"events\":[]}";
            }
        }

        /** User acknowledged a failure: back to the portal Activity. */
        @JavascriptInterface
        public void exitToPortal() {
            game.runOnUiThread(game::finish);
        }
    }
}
