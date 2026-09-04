package com.limelight;

import android.annotation.SuppressLint;
import android.content.ComponentName;
import android.content.Intent;
import android.content.ServiceConnection;
import android.content.SharedPreferences;
import android.content.pm.PackageManager;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.os.Environment;
import android.os.IBinder;
import android.os.Looper;
import android.os.storage.StorageManager;
import android.os.storage.StorageVolume;
import android.provider.Settings;
import android.util.Log;
import android.view.KeyEvent;
import android.view.ViewGroup;
import android.view.ViewParent;
import android.view.WindowManager;
import android.webkit.JavascriptInterface;
import android.webkit.WebResourceRequest;
import android.webkit.WebResourceResponse;
import android.webkit.WebView;
import android.webkit.WebViewClient;

import androidx.appcompat.app.AppCompatActivity;
import androidx.core.view.WindowCompat;
import androidx.core.view.WindowInsetsCompat;
import androidx.core.view.WindowInsetsControllerCompat;
import androidx.webkit.WebViewAssetLoader;

import com.limelight.computers.ComputerManagerListener;
import com.limelight.computers.ComputerManagerService;
import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.nvstream.http.NvApp;
import com.limelight.nvstream.http.NvHTTP;
import com.limelight.utils.CacheHelper;
import com.limelight.utils.ServerHelper;
import com.limelight.korri.overlay.KorriActiveLaunch;
import com.limelight.korri.overlay.KorriOverlayPermission;
import com.simonwjackson.korri.korrid.KorriBrainService;
import com.simonwjackson.korri.korrid.KorridServer;

import org.json.JSONArray;
import org.json.JSONObject;

import java.io.ByteArrayInputStream;
import java.io.File;
import java.io.StringReader;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;


/**
 * SPIKE: Korri web-shell launcher embedded in the Artemis APK.
 *
 * Proves the one-APK architecture: a WebView owns the launcher surface and a
 * narrow typed bridge (not art:// deep links) drives the orchestrated Korri
 * flow — korrid catalog -> signed Moonlight preparation -> native Game
 * activity against the plugin-owned Sunshine app selection. No trampoline hop,
 * no second launcher icon: streaming is an implementation detail of one app.
 */
public class KorriShellActivity extends AppCompatActivity {
    private static final String NOTIFICATION_PREFS = "korri-notifications";
    private static final String PREF_NOTIFICATION_PERMISSION_ASKED = "notification-permission-asked";
    private static final int REQUEST_GAME_FOLDER = 92;

    private WebView webView;
    private int korridPort = -1;
    private String korridCapability = "";
    private volatile ComputerManagerService.ComputerManagerBinder managerBinder;
    private boolean computerManagerBound;
    private volatile boolean destroyed;
    private volatile KorriMoonlightProvisioning moonlightProvisioning;
    private volatile KorriMoonlightDiscovery moonlightDiscovery;
    private volatile KorriMoonlightHostBootstrap moonlightHostBootstrap;
    private KorriGameAssetPathHandler gameAssetPathHandler;
    private final CountDownLatch binderReady = new CountDownLatch(1);
    private final KorriGameFolderPickerState gameFolderPicker = new KorriGameFolderPickerState();

    private final ServiceConnection serviceConnection = new ServiceConnection() {
        public void onServiceConnected(ComponentName className, IBinder binder) {
            final ComputerManagerService.ComputerManagerBinder localBinder =
                    (ComputerManagerService.ComputerManagerBinder) binder;
            new Thread(() -> {
                localBinder.waitForReady();
                if (destroyed) return;
                managerBinder = localBinder;
                installMoonlightDiscovery(localBinder);
                binderReady.countDown();
            }).start();
        }

        public void onServiceDisconnected(ComponentName className) {
            managerBinder = null;
            clearMoonlightDiscovery();
        }
    };

    @SuppressLint("SetJavaScriptEnabled")
    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        applyImmersiveFullscreen();

        // Embedded korrid: only this exact portal origin may present the
        // per-server capability to the localhost brain.
        final KorriTrustedPortalWebViewPolicy portalPolicy =
                KorriTrustedPortalWebViewPolicy.forRuntime(
                        BuildConfig.DEBUG,
                        BuildConfig.PORTAL_DEV_URL);
        final String portalUrl = portalPolicy.portalUrl();
        final KorriNativeBridgeLifecycle nativeBridgeLifecycle = new KorriNativeBridgeLifecycle();
        final File privateStateRoot = new File(KorriBrainService.privateStateRoot(this));
        gameAssetPathHandler = new KorriGameAssetPathHandler(privateStateRoot);
        korridPort = KorriBrainService.ensureRunning(
                this, portalPolicy.portalOrigin(), localStorageRoot());
        korridCapability = KorridServer.capability();

        computerManagerBound = bindService(new Intent(this, ComputerManagerService.class),
                serviceConnection, BIND_AUTO_CREATE);

        webView = new WebView(this);
        webView.getSettings().setJavaScriptEnabled(true);
        webView.getSettings().setDomStorageEnabled(true);
        webView.setBackgroundColor(0xFF101018);
        // file:// cannot serve the portal: WebView blocks ES modules and
        // stylesheets there (CORS, origin null). The asset loader maps
        // https://appassets.androidplatform.net/assets/** onto the APK's
        // assets with a real https origin.
        final WebViewAssetLoader assetLoader = new WebViewAssetLoader.Builder()
                .addPathHandler("/assets/", new WebViewAssetLoader.AssetsPathHandler(this))
                .addPathHandler(
                        KorriGameAssetPathHandler.ROUTE_PREFIX,
                        gameAssetPathHandler)
                .build();
        webView.setWebViewClient(new WebViewClient() {
            @Override
            public boolean shouldOverrideUrlLoading(WebView view, WebResourceRequest request) {
                return handleNavigation(request.getUrl(), request.isForMainFrame());
            }

            @Override
            public boolean shouldOverrideUrlLoading(WebView view, String url) {
                return handleNavigation(Uri.parse(url), true);
            }

            @Override
            public WebResourceResponse shouldInterceptRequest(
                    WebView view, WebResourceRequest request) {
                if (!"GET".equalsIgnoreCase(request.getMethod())) {
                    // The trusted portal talks to its localhost brain with
                    // capability-bearing POST requests. Untrusted documents
                    // cannot load in this WebView, so do not mistake RPC for
                    // script/image/subframe resource loading.
                    return null;
                }
                Uri uri = request.getUrl();
                if (portalPolicy.isBundledPortalAsset(uri)
                        || portalPolicy.isTrustedLocalGameAsset(uri)) {
                    WebResourceResponse response = assetLoader.shouldInterceptRequest(uri);
                    return response != null ? response : blockedWebResource();
                }
                if (portalPolicy.isTrustedPortalResource(uri)) {
                    return null;
                }
                return blockedWebResource();
            }

            @Override
            public void onPageStarted(WebView view, String url, android.graphics.Bitmap favicon) {
                nativeBridgeLifecycle.onMainFramePageStarted(
                        Uri.parse(url),
                        portalPolicy,
                        new KorriNativeBridgeOperations(view));
            }

            @Override
            public void onPageFinished(WebView view, String url) {
                nativeBridgeLifecycle.onMainFramePageFinished();
                view.evaluateJavascript("document.title", title ->
                        Log.i("KorriPortal", "title=" + title));
            }

            private boolean handleNavigation(Uri uri, boolean mainFrame) {
                KorriTrustedPortalWebViewPolicy.NavigationAction action =
                        portalPolicy.navigationAction(uri, mainFrame);
                if (action == KorriTrustedPortalWebViewPolicy.NavigationAction.ALLOW_IN_WEBVIEW) {
                    return false;
                }
                if (action == KorriTrustedPortalWebViewPolicy.NavigationAction.OPEN_EXTERNALLY) {
                    try {
                        startActivity(new Intent(Intent.ACTION_VIEW, uri));
                    } catch (Exception error) {
                        Log.w("KorriPortal", "blocked external navigation: " + uri, error);
                    }
                }
                return true;
            }
        });
        if (BuildConfig.DEBUG) {
            WebView.setWebContentsDebuggingEnabled(true);
        }

        nativeBridgeLifecycle.installBeforeInitialLoad(
                Uri.parse(portalUrl),
                portalPolicy,
                new KorriNativeBridgeOperations(webView));
        webView.loadUrl(portalUrl);
        setContentView(webView);
    }

    /**
     * Korri owns the whole screen, so the status and navigation bars are not
     * part of its surface. Hiding them also hands their insets back to the
     * portal, which otherwise lays out inside a shorter window.
     *
     * Android restores the bars after dialogs, pickers, permission prompts, and
     * returns from a game, so this runs again on every resume and focus gain
     * rather than once at startup.
     */
    private void applyImmersiveFullscreen() {
        WindowCompat.setDecorFitsSystemWindows(getWindow(), false);
        WindowInsetsControllerCompat insetsController =
                WindowCompat.getInsetsController(getWindow(), getWindow().getDecorView());
        // Transient bars on swipe: a stuck launcher must still expose the
        // system bars without a keyboard or a reboot.
        insetsController.setSystemBarsBehavior(
                WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
        insetsController.hide(WindowInsetsCompat.Type.systemBars());
    }

    /** The portal ships as bundled assets (built by `nix run .#portal-bundle`). */

    /** True when the user can actually see Korri's background notice. */
    private boolean notificationsAllowed() {
        return androidx.core.app.NotificationManagerCompat.from(this).areNotificationsEnabled();
    }

    private boolean notificationPermissionAsked() {
        return notificationPreferences().getBoolean(PREF_NOTIFICATION_PERMISSION_ASKED, false);
    }

    private void markNotificationPermissionAsked() {
        notificationPreferences()
                .edit()
                .putBoolean(PREF_NOTIFICATION_PERMISSION_ASKED, true)
                .apply();
    }

    private SharedPreferences notificationPreferences() {
        return getSharedPreferences(NOTIFICATION_PREFS, MODE_PRIVATE);
    }


    private static WebResourceResponse blockedWebResource() {
        return new WebResourceResponse(
                "text/plain",
                "UTF-8",
                new ByteArrayInputStream(new byte[0]));
    }

    private final class KorriNativeBridgeOperations
            implements KorriNativeBridgeLifecycle.Operations {
        private final WebView view;

        private KorriNativeBridgeOperations(WebView view) {
            this.view = view;
        }

        @Override
        public void addJavascriptInterface() {
            view.addJavascriptInterface(
                    new KorriNativeBridge(),
                    KorriNativeBridgeLifecycle.BRIDGE_NAME);
        }

        @Override
        public void removeJavascriptInterface() {
            view.removeJavascriptInterface(KorriNativeBridgeLifecycle.BRIDGE_NAME);
        }
    }

    /** Android supplies storage location; korrid owns everything beneath it. */
    private static String localStorageRoot() {
        return new File(Environment.getExternalStorageDirectory(), "korri")
                .getAbsolutePath();
    }

    /**
     * Semantic input seam — the WebView never sees a key code.
     *
     * Hardware truth (key codes, controller quirks) stays here: navigation-
     * relevant keys are translated into the semantic vocabulary defined in
     * contracts/bridge/korri-native-bridge.ts (BridgeInputEvent) and pushed
     * to the portal via window.__korriInput. Everything else falls through
     * to normal dispatch.
     */
    @Override
    public boolean dispatchKeyEvent(KeyEvent event) {
        String semanticJson = toSemanticInputJson(event.getKeyCode());
        if (semanticJson == null) {
            return super.dispatchKeyEvent(event);
        }
        if (event.getAction() == KeyEvent.ACTION_DOWN && webView != null) {
            webView.evaluateJavascript(
                    "window.__korriInput && window.__korriInput('"
                            + semanticJson + "')", null);
        }
        return true;
    }

    private static String toSemanticInputJson(int keyCode) {
        switch (keyCode) {
            case KeyEvent.KEYCODE_DPAD_UP:
                return "{\"type\":\"direction\",\"direction\":\"up\",\"source\":\"gamepad\"}";
            case KeyEvent.KEYCODE_DPAD_DOWN:
                return "{\"type\":\"direction\",\"direction\":\"down\",\"source\":\"gamepad\"}";
            case KeyEvent.KEYCODE_DPAD_LEFT:
                return "{\"type\":\"direction\",\"direction\":\"left\",\"source\":\"gamepad\"}";
            case KeyEvent.KEYCODE_DPAD_RIGHT:
                return "{\"type\":\"direction\",\"direction\":\"right\",\"source\":\"gamepad\"}";
            case KeyEvent.KEYCODE_DPAD_CENTER:
            case KeyEvent.KEYCODE_BUTTON_A:
                return "{\"type\":\"confirm\",\"source\":\"gamepad\"}";
            case KeyEvent.KEYCODE_BUTTON_B:
            case KeyEvent.KEYCODE_BACK:
                return "{\"type\":\"back\",\"source\":\"gamepad\"}";
            case KeyEvent.KEYCODE_BUTTON_START:
            case KeyEvent.KEYCODE_MENU:
                return "{\"type\":\"menu\",\"source\":\"gamepad\"}";
            case KeyEvent.KEYCODE_BUTTON_SELECT:
                return "{\"type\":\"options\",\"source\":\"gamepad\"}";
            default:
                return null;
        }
    }

    @Override
    protected void onDestroy() {
        if (Looper.myLooper() != Looper.getMainLooper()) {
            throw new IllegalStateException("Korri Shell destruction must run on the main thread");
        }

        // A finished Activity must not leave a second trusted portal target in
        // the process. Detach before destroy, revoke the Shell-only authority,
        // and clear our field before WebView teardown can re-enter lifecycle
        // code. The foreground brain remains owned by KorriBrainService.
        destroyed = true;
        clearMoonlightDiscovery();

        final WebView ownedWebView = webView;
        webView = null;
        if (ownedWebView != null) {
            final ViewParent parent = ownedWebView.getParent();
            if (parent instanceof ViewGroup) {
                ((ViewGroup) parent).removeView(ownedWebView);
            }
            ownedWebView.removeJavascriptInterface("KorriNative");
            ownedWebView.stopLoading();
            ownedWebView.destroy();
        }

        // Binding ownership begins when bindService() succeeds, not when its
        // asynchronous binder callback happens to complete.
        if (computerManagerBound) {
            computerManagerBound = false;
            unbindService(serviceConnection);
        }
        managerBinder = null;
        super.onDestroy();
    }

    @Override
    protected void onResume() {
        super.onResume();
        // The launch may remain live, but Korri itself being foreground is
        // never a gameplay-overlay target. Returning from a stream, Android
        // picker, or settings lets the web surface refresh its state.
        KorriBrainService.setOverlayArmed(false);
        applyImmersiveFullscreen();
        KorriMoonlightHostBootstrap bootstrap = moonlightHostBootstrap;
        if (bootstrap != null) bootstrap.start();
        if (webView != null) {
            webView.evaluateJavascript(
                    "window.dispatchEvent(new Event('korri-shell-resumed'))", null);
        }
    }

    @Override
    public void onWindowFocusChanged(boolean hasFocus) {
        super.onWindowFocusChanged(hasFocus);
        // Focus returns after the bars have already been shown by the system.
        // Re-hiding here is what keeps the launcher edge to edge over time.
        if (hasFocus) {
            applyImmersiveFullscreen();
        }
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (requestCode != REQUEST_GAME_FOLDER) return;
        if (resultCode != RESULT_OK || data == null || data.getData() == null) {
            gameFolderPicker.cancelled();
            return;
        }
        if (!hasAllFilesStorageAccess()) {
            gameFolderPicker.problem(
                    "StorageAccessDenied",
                    "Grant Korri file access, then choose a game folder");
            return;
        }
        KorriExternalStorageTreeResolver.Result result =
                KorriExternalStorageTreeResolver.resolve(
                        data.getData().toString(), externalStorageVolumes());
        if (!result.isOk()) {
            gameFolderPicker.problem(result.code, result.message);
            return;
        }
        try {
            String receipt = KorridServer.issueFolderSelectionReceipt(
                    result.canonicalDirectory.getPath());
            if (receipt == null || receipt.isEmpty()) {
                gameFolderPicker.problem(
                        "FolderSelectionReceiptUnavailable",
                        "Korri could not approve the selected folder");
                return;
            }
            gameFolderPicker.selected(receipt);
        } catch (Throwable error) {
            gameFolderPicker.problem(
                    "FolderSelectionReceiptUnavailable",
                    error.getMessage() != null
                            ? error.getMessage()
                            : "Korri could not approve the selected folder");
        }
    }

    @Override
    protected void onStop() {
        super.onStop();
        // Backgrounding is only an arming transition. It does not claim that
        // the active process or stream ended.
        if (KorriBrainService.activeLaunch() != null) {
            KorriBrainService.setOverlayArmed(true);
        }
    }

    private boolean hasAllFilesStorageAccess() {
        return Build.VERSION.SDK_INT < Build.VERSION_CODES.R
                || Environment.isExternalStorageManager();
    }

    private List<KorriExternalStorageTreeResolver.Volume> externalStorageVolumes() {
        List<KorriExternalStorageTreeResolver.Volume> volumes = new ArrayList<>();
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) {
            volumes.add(new KorriExternalStorageTreeResolver.Volume(
                    "primary", Environment.getExternalStorageDirectory()));
            return volumes;
        }
        StorageManager storageManager = getSystemService(StorageManager.class);
        if (storageManager == null) return volumes;
        for (StorageVolume volume : storageManager.getStorageVolumes()) {
            File directory = volume.getDirectory();
            if (directory == null) continue;
            String id = volume.isPrimary() ? "primary" : volume.getUuid();
            if (id == null || id.isEmpty()) continue;
            volumes.add(new KorriExternalStorageTreeResolver.Volume(id, directory));
        }
        return volumes;
    }

    private interface ActivityStart {
        void run() throws Exception;
    }

    /**
     * Narrow spike contract between the Korri web surface and the Android
     * runtime. Deals in Korri-shaped concepts (hosts, apps, launch requests),
     * never in raw intent extras or certificate material.
     */
    private class KorriNativeBridge {

        // --- Treaty surface: contracts/bridge/korri-native-bridge.ts ---
        // These methods mirror KorriNativeBridgeSurface. When the two sides
        // disagree, the contracts file wins.

        @JavascriptInterface
        public int bridgeVersion() {
            // Mirrors BRIDGE_VERSION in contracts/bridge/korri-native-bridge.ts.
            return 18;
        }

        @JavascriptInterface
        public String systemInfo() {
            try {
                String appVersion = getPackageManager()
                        .getPackageInfo(getPackageName(), 0).versionName;
                JSONObject payload = new JSONObject();
                payload.put("device", Build.MODEL);
                payload.put("manufacturer", Build.MANUFACTURER);
                payload.put("androidRelease", Build.VERSION.RELEASE);
                payload.put("sdk", Build.VERSION.SDK_INT);
                payload.put("appVersion", appVersion == null ? "Unknown" : appVersion);
                JSONObject result = new JSONObject();
                result.put("_tag", "SystemInfo");
                result.put("payload", payload);
                return result.toString();
            } catch (Exception error) {
                return "{\"_tag\":\"Unavailable\",\"message\":"
                        + JSONObject.quote(error.toString()) + "}";
            }
        }

        /**
         * Whether the user can see Korri running in the background. Reports
         * what is visible, not whether the brain is up: Android lets the
         * notice be hidden while the service keeps running.
         */
        @JavascriptInterface
        public String backgroundNotice() {
            return notificationsAllowed()
                    ? "{\"_tag\":\"Visible\"}"
                    : "{\"_tag\":\"Hidden\"}";
        }

        /**
         * Ask Android for permission to show the notice. Android stops
         * prompting once the user has declined twice, so a refusal to prompt
         * is reported plainly rather than as a denial.
         */
        @JavascriptInterface
        public String requestBackgroundNotice() {
            if (Build.VERSION.SDK_INT < 33) {
                // Older Android grants this at install time.
                return "{\"_tag\":\"Granted\"}";
            }
            if (notificationsAllowed()) {
                return "{\"_tag\":\"Granted\"}";
            }
            boolean rationale = shouldShowRequestPermissionRationale(
                    android.Manifest.permission.POST_NOTIFICATIONS);
            if (KorriNotificationPermissionPrompt.decision(
                    notificationPermissionAsked(), rationale)
                    == KorriNotificationPermissionPrompt.Decision.UNPROMPTED) {
                return "{\"_tag\":\"Unprompted\"}";
            }
            try {
                startActivityOnUiThread(
                        () -> requestPermissions(
                                new String[] {android.Manifest.permission.POST_NOTIFICATIONS}, 91),
                        "notification permission request timed out");
                markNotificationPermissionAsked();
            } catch (Exception error) {
                return "{\"_tag\":\"Unprompted\"}";
            }
            // The dialog is asynchronous; the portal re-reads backgroundNotice()
            // on korri-shell-resumed, exactly as it does for file access.
            return "{\"_tag\":\"Prompted\"}";
        }

        /**
         * Take the user to the system screen. Hiding the notice is the user's
         * to do -- an app may not hide its own -- so turning it off always
         * ends up here.
         */
        @JavascriptInterface
        public String openNotificationSettings() {
            try {
                Intent intent = new Intent(
                        android.provider.Settings.ACTION_APP_NOTIFICATION_SETTINGS);
                intent.putExtra(android.provider.Settings.EXTRA_APP_PACKAGE, getPackageName());
                startActivityOnUiThread(intent, "notification settings start timed out");
                return "{\"_tag\":\"Opened\"}";
            } catch (Exception error) {
                return "{\"_tag\":\"Unavailable\",\"message\":"
                        + JSONObject.quote(String.valueOf(error.getMessage())) + "}";
            }
        }

        /**
         * Whether Korri may use the user-visible storage its settings, plugins
         * and local-game files live in. Below Android 11 the concept does not
         * exist, so nothing needs granting.
         */
        @JavascriptInterface
        public String storageAccess() {
            try {
                if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) {
                    return "{\"_tag\":\"NotRequired\"}";
                }
                return Environment.isExternalStorageManager()
                        ? "{\"_tag\":\"Granted\"}"
                        : "{\"_tag\":\"Denied\"}";
            } catch (Throwable error) {
                return "{\"_tag\":\"QueryFailed\",\"message\":"
                        + JSONObject.quote(String.valueOf(error.getMessage())) + "}";
            }
        }

        /**
         * Take the user to the system screen where file access is granted. The
         * shell cannot grant it; returning `Opened` means only that the screen
         * was shown.
         */
        @JavascriptInterface
        public String openStorageAccessSettings() {
            if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) {
                return "{\"_tag\":\"Unavailable\",\"message\":"
                        + JSONObject.quote("This version of Android grants file access at install time")
                        + "}";
            }
            try {
                requestAllFilesAccess();
                return "{\"_tag\":\"Opened\"}";
            } catch (Exception error) {
                return "{\"_tag\":\"Unavailable\",\"message\":"
                        + JSONObject.quote(String.valueOf(error.getMessage())) + "}";
            }
        }

        /** Actual accessibility grant state; the launch path never depends on it. */
        @JavascriptInterface
        public String overlayPermission() {
            return KorriOverlayPermission.stateJson(
                    KorriOverlayPermission.state(KorriShellActivity.this));
        }

        /** Opens Settings only. The grant remains a later observed state. */
        @JavascriptInterface
        public String openOverlaySettings() {
            return KorriOverlayPermission.openSettings(KorriShellActivity.this);
        }

        /** Open Android's asynchronous game-folder picker. */
        @JavascriptInterface
        public String openGameFolderPicker() {
            String opened = gameFolderPicker.choose();
            try {
                if ("Busy".equals(new JSONObject(opened).getString("_tag"))) return opened;
            } catch (Exception ignored) {
                // Fall through and let the start attempt report the real failure.
            }
            if (!hasAllFilesStorageAccess()) {
                try {
                    requestAllFilesAccess();
                } catch (Exception ignored) {
                    // The snapshot below still tells the portal what is wrong.
                }
                gameFolderPicker.problem(
                        "StorageAccessDenied",
                        "Grant Korri file access, then choose a game folder");
                return KorriGameFolderPickerState.unavailableJson(
                        "Grant Korri file access, then choose a game folder");
            }
            try {
                Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT_TREE);
                intent.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION
                        | Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION
                        | Intent.FLAG_GRANT_PREFIX_URI_PERMISSION);
                startActivityOnUiThread(
                        () -> startActivityForResult(intent, REQUEST_GAME_FOLDER),
                        "game folder picker start timed out");
                return opened;
            } catch (Exception error) {
                String message = error.getMessage() != null
                        ? error.getMessage()
                        : "game folder picker is unavailable";
                gameFolderPicker.problem("FolderSelectionUnavailable", message);
                return KorriGameFolderPickerState.unavailableJson(message);
            }
        }

        /** Re-readable result of Android's asynchronous game-folder picker. */
        @JavascriptInterface
        public String gameFolderPickerSnapshot() {
            return gameFolderPicker.snapshotJson();
        }

        /** Acknowledge a definitive picker result so it cannot be applied twice. */
        @JavascriptInterface
        public String acknowledgeGameFolderPicker(String generation) {
            return gameFolderPicker.acknowledgeJson(generation);
        }

        /** Resolve one opaque local cover asset id to its trusted WebView URL. */
        @JavascriptInterface
        public String localGameAssetUrl(String assetId) {
            String url = KorriGameAssetPathHandler.trustedUrlForAssetId(assetId);
            if (url == null || gameAssetPathHandler == null
                    || gameAssetPathHandler.resolveKnownBlob(assetId) == null) {
                return "{\"_tag\":\"Absent\"}";
            }
            try {
                JSONObject result = new JSONObject();
                result.put("_tag", "Resolved");
                result.put("url", url);
                return result.toString();
            } catch (Exception error) {
                return "{\"_tag\":\"Absent\"}";
            }
        }

        /** Port of the embedded korrid server, or -1 when it is not running. */
        @JavascriptInterface
        public int korridPort() {
            return korridPort;
        }

        /** Per-server bearer capability for the localhost korrid RPC. */
        @JavascriptInterface
        public String korridCapability() {
            return korridCapability;
        }

        /** JSON-encoded LaunchLocalResult. */
        @JavascriptInterface
        public String launchLocal(String specJson) {
            final KorriLocalLaunchSpec.Parsed spec;
            try {
                spec = KorriLocalLaunchSpec.parse(
                        specJson,
                        new File(localStorageRoot()),
                        this::containsKnownStorageVolume);
            } catch (KorriLocalLaunchSpec.Invalid error) {
                return launchFailed(error.reason, error.getMessage());
            }

            // Validate package availability before any external-storage write.
            final Intent intent;
            if (spec.isAndroidApp) {
                Intent launch = getPackageManager()
                        .getLaunchIntentForPackage(spec.component.getPackageName());
                if (launch == null) {
                    return launchFailed("NotInstalled", "local launcher is not installed");
                }
                intent = launch;
            } else {
                intent = spec.intent();
                try {
                    getPackageManager().getActivityInfo(intent.getComponent(), 0);
                } catch (PackageManager.NameNotFoundException error) {
                    return launchFailed("NotInstalled", "local launcher is not installed");
                }
            }
            KorriLocalLaunchSpec.applyTaskPolicy(spec, intent);

            boolean hasProvisioning = !spec.directories.isEmpty() || !spec.files.isEmpty();
            if (hasProvisioning
                    && !KorriLocalLaunchSpec.supportsStorageProvisioning(Build.VERSION.SDK_INT)) {
                return launchFailed(
                        "ProvisionFailed",
                        "Local game storage provisioning requires Android 11 or newer");
            }
            boolean hasAllFilesAccess = Build.VERSION.SDK_INT < Build.VERSION_CODES.R
                    || Environment.isExternalStorageManager();
            if (KorriLocalLaunchSpec.requiresStorageGrant(
                    spec,
                    Build.VERSION.SDK_INT,
                    hasAllFilesAccess)) {
                try {
                    requestAllFilesAccess();
                } catch (Exception error) {
                    return launchFailed(
                            "ProvisionFailed",
                            "Unable to open Android all files access settings");
                }
                return launchFailed(
                        "ProvisionFailed",
                        "Grant Korri all files access, then return and retry");
            }
            try {
                for (String directory : spec.directories) {
                    provisionDirectory(directory);
                }
                for (KorriLocalLaunchSpec.FileSpec file : spec.files) {
                    provisionFile(file.path, file.content);
                }
            } catch (Exception error) {
                return launchFailed("ProvisionFailed",
                        error.getMessage() != null
                                ? error.getMessage()
                                : "local file provisioning failed");
            }

            int authorization = KorridServer.authorizeLaunchSpec(specJson, intent);
            if (authorization == KorridServer.LOCAL_LAUNCH_REJECTED) {
                return launchFailed("InvalidSpec",
                        "local launch instruction failed authorization");
            }
            try {
                startActivityOnUiThread(intent, "local launcher start timed out");
            } catch (InterruptedException error) {
                Thread.currentThread().interrupt();
                return launchFailed("StartFailed", "local launcher start interrupted");
            } catch (Exception error) {
                return launchFailed("StartFailed",
                        error.getMessage() != null ? error.getMessage() : "start failed");
            }
            if (authorization == KorridServer.LOCAL_LAUNCH_PUBLISH) {
                final KorriActiveLaunch active;
                try {
                    // Record only after Android accepted a fresh exact signed launch.
                    active = KorriBrainService.publishLocalActiveLaunch(spec, specJson);
                } catch (Exception publishError) {
                    return launchFailed("StartFailed", "local launch context could not be recorded");
                }
                try {
                    // This action-bound event contains identity only. Never add the
                    // signed spec, control token, RPC capability, or control port.
                    Log.i(KorriLocalLaunchLifecycle.TAG,
                            KorriLocalLaunchLifecycle.published(
                                    active.launchId(),
                                    active.gameId(),
                                    spec.component.getPackageName(),
                                    spec.launcherId));
                } catch (IllegalArgumentException metadataError) {
                    Log.e(KorriLocalLaunchLifecycle.TAG,
                            "local launch publication metadata was invalid");
                }
            }
            return "{\"_tag\":\"Launched\"}";
        }

        private void requestAllFilesAccess() throws Exception {
            startActivityOnUiThread(() -> {
                try {
                    startActivity(new Intent(
                            Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION,
                            Uri.parse("package:" + getPackageName())));
                } catch (Exception appSettingsError) {
                    startActivity(new Intent(
                            Settings.ACTION_MANAGE_ALL_FILES_ACCESS_PERMISSION));
                }
            }, "all files access settings timed out");
        }

        private void startActivityOnUiThread(Intent intent, String timeoutMessage)
                throws Exception {
            startActivityOnUiThread(() -> startActivity(intent), timeoutMessage);
        }

        private void startActivityOnUiThread(ActivityStart start, String timeoutMessage)
                throws Exception {
            if (Looper.myLooper() == Looper.getMainLooper()) {
                start.run();
                return;
            }

            KorriUiStartGate.run(
                    KorriShellActivity.this::runOnUiThread,
                    start::run,
                    5,
                    TimeUnit.SECONDS,
                    timeoutMessage);
        }

        private boolean containsKnownStorageVolume(String canonicalPath) throws Exception {
            if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) {
                return KorriLocalLaunchSpec.containsCanonicalPath(
                        Environment.getExternalStorageDirectory(), canonicalPath);
            }
            StorageManager storageManager = getSystemService(StorageManager.class);
            if (storageManager == null) {
                return false;
            }
            for (StorageVolume volume : storageManager.getStorageVolumes()) {
                File directory = volume.getDirectory();
                if (directory == null) {
                    continue;
                }
                File root = directory.getCanonicalFile();
                if (canonicalPath.equals(root.getPath())
                        || canonicalPath.startsWith(root.getPath() + File.separator)) {
                    return true;
                }
            }
            return false;
        }

        private void provisionDirectory(String targetPath) throws Exception {
            File korriRoot = new File(localStorageRoot()).getCanonicalFile();
            File directory = new File(targetPath).getCanonicalFile();
            String rootPrefix = korriRoot.getPath() + File.separator;
            if (!directory.getPath().startsWith(rootPrefix)) {
                throw new IllegalArgumentException("provision path is outside Korri storage");
            }
            if (!directory.isDirectory() && !directory.mkdirs()) {
                throw new IllegalStateException("cannot create provisioned directory");
            }
        }

        /** Write a validated korrid-provided file at the Android storage edge. */
        private void provisionFile(String targetPath, String content) throws Exception {
            File korriRoot = new File(localStorageRoot()).getCanonicalFile();
            File target = new File(targetPath).getCanonicalFile();
            String rootPrefix = korriRoot.getPath() + File.separator;
            if (!target.getPath().startsWith(rootPrefix)) {
                throw new IllegalArgumentException("provision path is outside Korri storage");
            }
            File parent = target.getParentFile();
            if (parent == null || (!parent.isDirectory() && !parent.mkdirs())) {
                throw new IllegalStateException("cannot create provisioned file directory");
            }
            KorriAtomicFile.write(target, content.getBytes(StandardCharsets.UTF_8));
        }

        private String launchFailed(String reason, String message) {
            try {
                JSONObject failed = new JSONObject();
                failed.put("_tag", "LaunchFailed");
                failed.put("reason", reason);
                failed.put("message", message);
                return failed.toString();
            } catch (Exception e) {
                return "{\"_tag\":\"LaunchFailed\",\"reason\":\"" + reason
                        + "\",\"message\":\"\"}";
            }
        }

        /** JSON-encoded QueryStreamHostsResult. */
        @JavascriptInterface
        public String queryStreamHosts() {
            try {
                JSONArray items = new JSONArray();
                com.limelight.computers.ComputerDatabaseManager db =
                        new com.limelight.computers.ComputerDatabaseManager(KorriShellActivity.this);
                try {
                    List<ComputerDetails> computers = db.getAllComputers();
                    for (ComputerDetails details : computers) {
                        JSONObject host = new JSONObject();
                        host.put("uuid", details.uuid);
                        host.put("name", details.name);
                        items.put(host);
                    }
                } finally {
                    db.close();
                }
                JSONObject ok = new JSONObject();
                ok.put("_tag", "StreamHosts");
                ok.put("items", items);
                return ok.toString();
            } catch (Exception e) {
                return queryFailed(e);
            }
        }

        /** JSON-encoded QueryStreamAppsResult. */
        @JavascriptInterface
        public String queryStreamApps(String hostUuid) {
            try {
                // Return only current local cache state on the JavaScript bridge
                // thread. Provision, attestation, and HTTPS refresh run on the
                // bounded Android discovery executor and become visible on a
                // later portal poll.
                KorriMoonlightDiscovery discovery = moonlightDiscovery;
                List<NvApp> current = discovery == null
                        ? cachedAppList(getApplicationContext(), hostUuid)
                        : discovery.query(hostUuid);
                JSONArray items = new JSONArray();
                for (NvApp app : current) {
                    JSONObject entry = new JSONObject();
                    entry.put("id", app.getAppId());
                    entry.put("name", app.getAppName());
                    items.put(entry);
                }
                JSONObject ok = new JSONObject();
                ok.put("_tag", "StreamApps");
                ok.put("items", items);
                return ok.toString();
            } catch (Exception e) {
                return queryFailed(e);
            }
        }

        /**
         * JSON-encoded StartStreamResult. The only callable startup surface
         * accepts korrid's signed, one-use Moonlight launch instruction.
         */
        @JavascriptInterface
        public String startStream(String specJson) {
            try {
                ComputerManagerService.ComputerManagerBinder binder = awaitBinder(10);
                if (binder == null) {
                    return streamFailed("StartFailed", "computer manager not ready");
                }
                moonlightFlow(
                        binder,
                        (authorizedSpecJson, spec, app, computer) -> {
                            final Intent intent = ServerHelper.createStartIntent(
                                    KorriShellActivity.this, app, computer, binder);
                            // Korri-initiated: the stream Activity narrates its lifecycle
                            // through the web overlay instead of the native spinner.
                            intent.putExtra(Game.EXTRA_KORRI_SESSION, true);
                            intent.putExtra(Game.EXTRA_KORRI_LAUNCH_ID, spec.launchId);
                            startActivityOnUiThread(
                                    intent, "Moonlight Activity start timed out");
                            if (Game.instance != null) {
                                KorriBrainService.claimActiveLaunch(spec.launchId, Game.instance);
                            }
                        }, this).startStream(specJson);
                return "{\"_tag\":\"StreamStarted\"}";
            } catch (KorriMoonlightShellFlow.Failure error) {
                return streamFailed(error.reason, error.getMessage());
            } catch (Exception e) {
                return streamFailed("StartFailed",
                        e.getMessage() != null ? e.getMessage() : "start failed");
            }
        }

        private String queryFailed(Exception e) {
            try {
                JSONObject failed = new JSONObject();
                failed.put("_tag", "QueryFailed");
                failed.put("message",
                        e.getMessage() != null ? e.getMessage() : "query failed");
                return failed.toString();
            } catch (Exception inner) {
                return "{\"_tag\":\"QueryFailed\",\"message\":\"query failed\"}";
            }
        }

        private String streamFailed(String reason, String message) {
            try {
                JSONObject failed = new JSONObject();
                failed.put("_tag", "StreamFailed");
                failed.put("reason", reason);
                failed.put("message", message);
                return failed.toString();
            } catch (Exception e) {
                return "{\"_tag\":\"StreamFailed\",\"reason\":\"" + reason
                        + "\",\"message\":\"\"}";
            }
        }


    }

    private synchronized void clearMoonlightDiscovery() {
        KorriMoonlightDiscovery ownedDiscovery = moonlightDiscovery;
        KorriMoonlightHostBootstrap ownedBootstrap = moonlightHostBootstrap;
        moonlightDiscovery = null;
        moonlightHostBootstrap = null;
        moonlightProvisioning = null;
        if (ownedDiscovery != null) ownedDiscovery.close();
        if (ownedBootstrap != null) ownedBootstrap.close();
    }

    private synchronized void installMoonlightDiscovery(
            ComputerManagerService.ComputerManagerBinder binder) {
        if (destroyed || managerBinder != binder) return;
        KorriMoonlightDiscovery previousDiscovery = moonlightDiscovery;
        KorriMoonlightHostBootstrap previousBootstrap = moonlightHostBootstrap;
        if (previousDiscovery != null) previousDiscovery.close();
        if (previousBootstrap != null) previousBootstrap.close();
        android.content.Context application = getApplicationContext();
        java.lang.ref.WeakReference<KorriShellActivity> shell =
                new java.lang.ref.WeakReference<>(this);
        KorriMoonlightProvisioning provisioning = KorriMoonlightProvisioning.artemis(
                application, binder);
        moonlightProvisioning = provisioning;
        moonlightDiscovery = new KorriMoonlightDiscovery(
                hostUuid -> cachedAppList(application, hostUuid),
                (hostUuid, cached) -> {
                    ComputerManagerService.ComputerManagerBinder.MoonlightHostSnapshot snapshot =
                            binder.snapshotMoonlightHost(hostUuid);
                    return snapshot != null
                            && (snapshot.computer.serverCert == null
                            || !hasCachedAppList(application, hostUuid));
                },
                (hostUuid, guard) -> provisioning.repairAndLoadApps(hostUuid, guard),
                hostUuid -> {
                    KorriShellActivity activity = shell.get();
                    if (activity != null) activity.notifyStreamAppsChanged();
                });
        moonlightHostBootstrap = new KorriMoonlightHostBootstrap(
                () -> KorriMoonlightHostBootstrap.decodeCandidates(
                        KorridServer.moonlightHostCandidates()),
                candidate -> {
                    ComputerDetails details = new ComputerDetails();
                    details.name = candidate.label;
                    details.manualAddress = candidate.manualAddress;
                    return binder.addComputerBlocking(details);
                },
                () -> {
                    KorriShellActivity activity = shell.get();
                    if (activity != null) activity.notifyStreamAppsChanged();
                });
        moonlightHostBootstrap.start();
    }

    private void notifyStreamAppsChanged() {
        runOnUiThread(() -> {
            if (destroyed || webView == null) return;
            webView.evaluateJavascript(
                    "window.dispatchEvent(new Event('korri-stream-apps-changed'))", null);
        });
    }

    private KorriMoonlightShellFlow moonlightFlow(
            ComputerManagerService.ComputerManagerBinder binder,
            KorriMoonlightShellFlow.GameStarter gameStarter,
            Object launchOwner) {
        KorriMoonlightProvisioning provisioning = moonlightProvisioning;
        if (provisioning == null) {
            provisioning = KorriMoonlightProvisioning.artemis(
                    getApplicationContext(), binder);
            moonlightProvisioning = provisioning;
        }
        KorriMoonlightProvisioning exactProvisioning = provisioning;
        return new KorriMoonlightShellFlow(
                KorridServer::authorizeMoonlightLaunchSpec,
                hostUuid -> awaitOnlineComputer(binder, hostUuid, 12),
                exactProvisioning::repairAndLoadApps,
                (specJson, spec) -> {
                    KorriActiveLaunch launch =
                            KorriBrainService.reserveMoonlightActiveLaunch(
                                    launchOwner,
                                    specJson,
                                    spec.launchId,
                                    getPackageName(),
                                    Game.class.getName());
                    if (launch == null) return null;
                    return () -> KorriBrainService.clearActiveLaunch(
                            launchOwner, spec.launchId);
                },
                gameStarter);
    }

    private ComputerManagerService.ComputerManagerBinder awaitBinder(int seconds) {
        try {
            binderReady.await(seconds, TimeUnit.SECONDS);
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
        }
        return managerBinder;
    }

    /**
     * Return the computer once it is ONLINE with an active address, polling
     * briefly when needed. This replaces the trampoline's visible spinner
     * screen with an in-shell wait.
     */
    private ComputerDetails awaitOnlineComputer(
            ComputerManagerService.ComputerManagerBinder binder,
            String hostUuid,
            int timeoutSeconds) {
        ComputerDetails existing = binder.getComputer(hostUuid);
        if (existing != null
                && existing.state == ComputerDetails.State.ONLINE
                && existing.activeAddress != null) {
            return existing;
        }

        binder.invalidateStateForComputer(hostUuid);
        final CountDownLatch online = new CountDownLatch(1);
        final ComputerDetails[] resolved = new ComputerDetails[1];
        binder.startPolling(new ComputerManagerListener() {
            @Override
            public void notifyComputerUpdated(ComputerDetails details) {
                if (!details.uuid.equalsIgnoreCase(hostUuid)) return;
                if (details.state == ComputerDetails.State.ONLINE
                        && details.activeAddress != null) {
                    resolved[0] = details;
                    online.countDown();
                } else if (details.state == ComputerDetails.State.OFFLINE) {
                    online.countDown();
                }
            }
        });
        try {
            online.await(timeoutSeconds, TimeUnit.SECONDS);
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
        } finally {
            binder.stopPolling();
        }
        return resolved[0];
    }

    private static boolean hasCachedAppList(
            android.content.Context context, String hostUuid) {
        return CacheHelper.cacheFileExists(context.getCacheDir(), "applist", hostUuid);
    }

    private static List<NvApp> cachedAppList(
            android.content.Context context, String hostUuid) throws Exception {
        if (!hasCachedAppList(context, hostUuid)) {
            return java.util.Collections.emptyList();
        }
        String rawAppList = CacheHelper.readInputStreamToString(
                CacheHelper.openCacheFileForInput(
                        context.getCacheDir(), "applist", hostUuid));
        if (rawAppList.isEmpty()) return java.util.Collections.emptyList();
        return NvHTTP.getAppListByReader(new StringReader(rawAppList));
    }
}
