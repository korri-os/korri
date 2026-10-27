package com.limelight;

import android.annotation.SuppressLint;
import android.graphics.Color;
import android.view.KeyEvent;
import android.view.MotionEvent;
import android.view.View;
import android.view.ViewGroup;
import android.webkit.JavascriptInterface;
import android.webkit.WebView;
import android.widget.FrameLayout;

import com.limelight.korri.overlay.KorriOverlayHostExclusion;

/**
 * Web-rendered in-game overlay: a transparent WebView floated above the
 * stream surface, replacing the native AlertDialog menu.
 *
 * The web page owns all presentation; this class owns lifecycle, input
 * gating (nothing leaks to the stream while the overlay is up), and a
 * narrow action bridge. Certificate material and protocol state never enter
 * JavaScript.
 */
public class KorriGameOverlay implements KorriOverlayHostExclusion.LegacyHost {

    private final Game game;
    private WebView webView;
    private boolean showing;
    private boolean destroyed;

    // Edge-detection state for translating gamepad hats/sticks to dpad nav.
    private int lastNavX = 0;
    private int lastNavY = 0;

    public KorriGameOverlay(Game game) {
        this.game = game;
    }

    public boolean isShowing() {
        return showing;
    }

    public boolean isDestroyed() {
        return destroyed;
    }

    @Override
    public boolean isVisible() {
        return showing;
    }

    public void show() {
        if (showing || destroyed) return;
        ensureWebView();
        showing = true;
        webView.setVisibility(View.VISIBLE);
        webView.requestFocus();
        webView.evaluateJavascript(
                "window.dispatchEvent(new Event('korri-overlay-shown'))", null);
    }

    public void hide() {
        if (!showing) return;
        showing = false;
        if (webView != null) {
            webView.setVisibility(View.GONE);
        }
        View streamContainer = game.findViewById(R.id.streamContainer);
        if (streamContainer != null) {
            streamContainer.requestFocus();
        }
    }

    @Override
    public void closeAndDestroy() {
        if (destroyed) return;
        hide();
        destroyed = true;
        if (webView != null) {
            webView.stopLoading();
            ViewGroup parent = (ViewGroup) webView.getParent();
            if (parent != null) parent.removeView(webView);
            webView.destroy();
            webView = null;
        }
    }

    @SuppressLint("SetJavaScriptEnabled")
    private void ensureWebView() {
        if (webView != null) return;
        webView = new WebView(game);
        webView.getSettings().setJavaScriptEnabled(true);
        webView.setBackgroundColor(Color.TRANSPARENT);
        webView.setVisibility(View.GONE);
        webView.setElevation(10000f);
        webView.setFocusable(true);
        webView.setFocusableInTouchMode(true);
        // The overlay WebView takes focus while showing, so hardware keys
        // dispatch here first and Chromium consumes gamepad buttons before
        // they can bubble to Game's overlay gate. A View OnKeyListener runs
        // ahead of the WebView's internal handling, so close/toggle keys
        // work regardless. Dpad/enter fall through for native focus nav.
        webView.setOnKeyListener((v, keyCode, event) -> {
            switch (keyCode) {
                case KeyEvent.KEYCODE_DPAD_UP:
                case KeyEvent.KEYCODE_DPAD_DOWN:
                case KeyEvent.KEYCODE_DPAD_LEFT:
                case KeyEvent.KEYCODE_DPAD_RIGHT:
                case KeyEvent.KEYCODE_DPAD_CENTER:
                case KeyEvent.KEYCODE_ENTER:
                    return false; // native WebView focus navigation
                default:
                    return handleKeyEvent(event);
            }
        });
        webView.addJavascriptInterface(new OverlayBridge(), "KorriOverlayNative");
        ViewGroup root = game.findViewById(android.R.id.content);
        root.addView(webView, new FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));
        webView.loadUrl("file:///android_asset/korri-shell/overlay.html");
    }

    /**
     * Called from Game's key handlers while showing. Returns true when the
     * event was consumed (i.e. must not reach the stream).
     */
    public boolean handleKeyEvent(KeyEvent event) {
        int code = event.getKeyCode();
        // System keys stay with the system.
        if (code == KeyEvent.KEYCODE_VOLUME_UP
                || code == KeyEvent.KEYCODE_VOLUME_DOWN
                || code == KeyEvent.KEYCODE_VOLUME_MUTE
                || code == KeyEvent.KEYCODE_POWER) {
            return false;
        }
        switch (code) {
            case KeyEvent.KEYCODE_BACK:
            case KeyEvent.KEYCODE_BUTTON_B:
            case KeyEvent.KEYCODE_BUTTON_START:
            case KeyEvent.KEYCODE_BUTTON_MODE: // Guide toggles the overlay closed
                if (event.getAction() == KeyEvent.ACTION_UP) {
                    hide();
                }
                return true;
            case KeyEvent.KEYCODE_BUTTON_A:
                forward(new KeyEvent(event.getAction(), KeyEvent.KEYCODE_DPAD_CENTER));
                return true;
            case KeyEvent.KEYCODE_DPAD_UP:
            case KeyEvent.KEYCODE_DPAD_DOWN:
            case KeyEvent.KEYCODE_DPAD_LEFT:
            case KeyEvent.KEYCODE_DPAD_RIGHT:
            case KeyEvent.KEYCODE_DPAD_CENTER:
            case KeyEvent.KEYCODE_ENTER:
                forward(event);
                return true;
            default:
                // Swallow everything else so the stream never sees it.
                return true;
        }
    }

    /**
     * Called from Game's controller motion path while showing. Translates
     * dpad hats and the left stick into dpad key taps for web navigation.
     */
    public boolean handleMotionEvent(MotionEvent event) {
        float x = event.getAxisValue(MotionEvent.AXIS_HAT_X);
        float y = event.getAxisValue(MotionEvent.AXIS_HAT_Y);
        if (Math.abs(x) < 0.5f && Math.abs(y) < 0.5f) {
            x = event.getAxisValue(MotionEvent.AXIS_X);
            y = event.getAxisValue(MotionEvent.AXIS_Y);
        }
        int navX = x > 0.5f ? 1 : (x < -0.5f ? -1 : 0);
        int navY = y > 0.5f ? 1 : (y < -0.5f ? -1 : 0);
        if (navX != lastNavX && navX != 0) {
            tap(navX > 0 ? KeyEvent.KEYCODE_DPAD_RIGHT : KeyEvent.KEYCODE_DPAD_LEFT);
        }
        if (navY != lastNavY && navY != 0) {
            tap(navY > 0 ? KeyEvent.KEYCODE_DPAD_DOWN : KeyEvent.KEYCODE_DPAD_UP);
        }
        lastNavX = navX;
        lastNavY = navY;
        return true; // consume; nothing reaches the stream
    }

    private void tap(int keyCode) {
        forward(new KeyEvent(KeyEvent.ACTION_DOWN, keyCode));
        forward(new KeyEvent(KeyEvent.ACTION_UP, keyCode));
    }

    private void forward(KeyEvent event) {
        if (webView != null) {
            webView.dispatchKeyEvent(event);
        }
    }

    /** Narrow action surface; presentation stays in the web page. */
    private class OverlayBridge {

        @JavascriptInterface
        public void resume() {
            game.runOnUiThread(KorriGameOverlay.this::hide);
        }

        @JavascriptInterface
        public void disconnect() {
            game.runOnUiThread(() -> {
                hide();
                game.disconnect();
            });
        }

        @JavascriptInterface
        public void quitSession() {
            game.runOnUiThread(() -> {
                hide();
                game.quit();
            });
        }

        @JavascriptInterface
        public void toggleKeyboard() {
            game.runOnUiThread(() -> {
                hide();
                game.toggleKeyboard();
            });
        }

        @JavascriptInterface
        public boolean isFillMode() {
            return game.isFillModeEnabled();
        }

        @JavascriptInterface
        public boolean isZoomMode() {
            return game.isZoomModeEnabled();
        }

        @JavascriptInterface
        public void toggleZoomMode() {
            game.runOnUiThread(() -> {
                hide();
                game.toggleZoomMode();
            });
        }

        @JavascriptInterface
        public void rotateScreen() {
            game.runOnUiThread(() -> {
                hide();
                game.rotateScreen();
            });
        }

        @JavascriptInterface
        public void toggleHud() {
            game.runOnUiThread(() -> {
                hide();
                game.toggleHUD();
            });
        }

        @JavascriptInterface
        public void toggleFloatingButton() {
            game.runOnUiThread(() -> {
                hide();
                game.toggleFloatingButtonVisibility();
            });
        }

        @JavascriptInterface
        public void toggleFullKeyboard() {
            game.runOnUiThread(() -> {
                hide();
                game.toggleFullKeyboard();
            });
        }

        @JavascriptInterface
        public void toggleKeyboardController() {
            game.runOnUiThread(() -> {
                hide();
                game.toggleKeyboardController();
            });
        }

        @JavascriptInterface
        public void switchTouchSensitivity() {
            game.runOnUiThread(() -> {
                hide();
                game.switchTouchSensitivity();
            });
        }

        /** Mouse modes as JSON: [{index,label}...] plus the local-cursor toggle. */
        @JavascriptInterface
        public String getMouseModes() {
            try {
                org.json.JSONArray modes = new org.json.JSONArray();
                String[] labels = game.getMouseModeLabels();
                for (int i = 0; i < labels.length; i++) {
                    modes.put(new org.json.JSONObject()
                            .put("index", i).put("label", labels[i]));
                }
                modes.put(new org.json.JSONObject()
                        .put("index", -1)
                        .put("label", game.getString(R.string.toggle_local_mouse_cursor)));
                return modes.toString();
            } catch (org.json.JSONException e) {
                return "[]";
            }
        }

        @JavascriptInterface
        public void setMouseMode(int index) {
            game.runOnUiThread(() -> {
                hide();
                game.applyMouseModeFromOverlay(index);
            });
        }

        @JavascriptInterface
        public void toggleFillMode() {
            game.runOnUiThread(game::toggleFillMode);
        }

        // Settings reuse the exact same theme-free contract as the shell.
        @JavascriptInterface
        public String getSettingsValues() {
            return KorriSettingsBridge.valuesJson(game);
        }

        @JavascriptInterface
        public String setSetting(String key, String jsonValue) {
            String result = KorriSettingsBridge.applySetting(game, key, jsonValue);
            game.runOnUiThread(game::applyLivePrefs);
            return result;
        }
    }
}
