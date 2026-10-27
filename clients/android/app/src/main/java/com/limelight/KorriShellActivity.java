package com.limelight;

import android.annotation.SuppressLint;
import android.content.Intent;
import android.net.Uri;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.util.Log;
import android.view.ViewGroup;
import android.view.ViewParent;
import android.view.WindowManager;
import android.webkit.WebResourceRequest;
import android.webkit.WebResourceResponse;
import android.webkit.WebView;
import android.webkit.WebViewClient;

import androidx.appcompat.app.AppCompatActivity;
import androidx.activity.result.ActivityResultLauncher;
import androidx.activity.result.contract.ActivityResultContracts;
import com.limelight.identity.KorriOwnerBindingController;
import com.limelight.identity.KorriOwnerBindingHost;
import androidx.core.view.WindowCompat;
import androidx.core.view.WindowInsetsCompat;
import androidx.core.view.WindowInsetsControllerCompat;
import androidx.webkit.WebViewAssetLoader;
import androidx.webkit.WebViewCompat;
import androidx.webkit.WebViewFeature;

import com.simonwjackson.korri.korrid.KorriBrainService;
import com.simonwjackson.korri.korrid.KorridServer;

import java.io.ByteArrayInputStream;
import java.io.File;
import java.nio.charset.StandardCharsets;
import java.util.Collections;

/** The restored Artemis shell, connected through the current RPC treaty. */
public class KorriShellActivity extends AppCompatActivity {
    private static final long STARTUP_TIMEOUT_MS = 15000;
    private final Handler handler = new Handler(Looper.getMainLooper());
    private final Runnable startupDeadline = this::startupFailed;
    private WebView webView;
    private KorriRpcBinding binding;
    private boolean destroyed;
    private KorriOwnerBindingHost ownerBinding;
    private final ActivityResultLauncher<Intent> signerLauncher = registerForActivityResult(
            new ActivityResultContracts.StartActivityForResult(), result -> {
                if (ownerBinding != null) ownerBinding.onActivityResult(result.getResultCode(), result.getData());
            });

    @SuppressLint("SetJavaScriptEnabled")
    @Override protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        applyImmersiveFullscreen();
        if (!WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT)
                || !WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER)) {
            startupFailed();
            return;
        }
        final KorriTrustedPortalWebViewPolicy policy =
                KorriTrustedPortalWebViewPolicy.forRuntime(BuildConfig.DEBUG, BuildConfig.PORTAL_DEV_URL);
        // The platform supplies this app-owned directory. A stream client needs
        // no shared-library scan or all-files permission to start its brain.
        final int port;
        try {
            port = KorriBrainService.ensureRunning(this, policy.portalOrigin(), getFilesDir().getAbsolutePath());
            binding = new KorriRpcBinding(policy.portalOrigin(), port, KorridServer.capability());
            ownerBinding = new KorriOwnerBindingHost(this, new KorriOwnerBindingController.NativeIdentity() {
                @Override public String status() { return KorridServer.identityStatus(); }
                @Override public String ownerBindingTemplate(long createdAt) { return KorridServer.ownerBindingTemplate(createdAt); }
                @Override public String applyOwnerBinding(String template, String owner, String event) {
                    return KorridServer.applyOwnerBinding(template, owner, event);
                }
            }, new KorriOwnerBindingHost.NativeRequests() {
                @Override public long update(long generation, String snapshot) { return KorridServer.updateOwnerSigner(generation, snapshot); }
                @Override public String take(long generation) { return KorridServer.takeOwnerBindingRequest(generation); }
                @Override public void detach(long generation) { KorridServer.detachOwnerSigner(generation); }
            }, signerLauncher::launch);
        } catch (RuntimeException error) {
            startupFailed();
            return;
        }
        final KorriGameAssetPathHandler gameAssets = new KorriGameAssetPathHandler(
                new File(KorriBrainService.privateStateRoot(this)));
        final WebViewAssetLoader loader = new WebViewAssetLoader.Builder()
                .addPathHandler("/assets/", new WebViewAssetLoader.AssetsPathHandler(this))
                .addPathHandler(KorriGameAssetPathHandler.ROUTE_PREFIX, gameAssets)
                .build();
        webView = new WebView(this);
        webView.getSettings().setJavaScriptEnabled(true);
        webView.getSettings().setDomStorageEnabled(true);
        webView.getSettings().setAllowFileAccess(false);
        webView.getSettings().setAllowContentAccess(false);
        // Preserve the restored shell's background while the portal loads.
        webView.setBackgroundColor(0xFF101018);
        WebView.setWebContentsDebuggingEnabled(BuildConfig.DEBUG);
        WebViewCompat.addWebMessageListener(webView, KorriRpcBinding.READY_LISTENER,
                Collections.singleton(policy.portalOrigin()),
                (view, message, sourceOrigin, mainFrame, reply) -> {
                    if (destroyed || view != webView) return;
                    if (binding.consumed(sourceOrigin.toString(), mainFrame, message.getData())) {
                        handler.removeCallbacks(startupDeadline);
                        Log.i("KorriPortal", "ready");
                    }
                });
        WebViewCompat.addDocumentStartJavaScript(webView, binding.documentStartScript(),
                Collections.singleton(policy.portalOrigin()));
        webView.setWebViewClient(new WebViewClient() {
            @Override public boolean shouldOverrideUrlLoading(WebView view, WebResourceRequest request) {
                return handleNavigation(request.getUrl(), request.isForMainFrame());
            }
            @Override public boolean shouldOverrideUrlLoading(WebView view, String url) {
                return handleNavigation(Uri.parse(url), true);
            }
            private boolean handleNavigation(Uri uri, boolean mainFrame) {
                KorriTrustedPortalWebViewPolicy.NavigationAction action = policy.navigationAction(uri, mainFrame);
                if (action == KorriTrustedPortalWebViewPolicy.NavigationAction.ALLOW_IN_WEBVIEW) return false;
                if (action == KorriTrustedPortalWebViewPolicy.NavigationAction.OPEN_EXTERNALLY) {
                    try { startActivity(new Intent(Intent.ACTION_VIEW, uri).addCategory(Intent.CATEGORY_BROWSABLE)); }
                    catch (RuntimeException ignored) { /* No external browser installed. */ }
                }
                return true;
            }
            @Override public WebResourceResponse shouldInterceptRequest(WebView view, WebResourceRequest request) {
                if (!"GET".equalsIgnoreCase(request.getMethod())) return null;
                Uri uri = request.getUrl();
                if (policy.isBundledPortalAsset(uri) || policy.isTrustedLocalGameAsset(uri)) {
                    WebResourceResponse response = loader.shouldInterceptRequest(uri);
                    return response != null ? response : blockedWebResource();
                }
                return policy.isTrustedPortalResource(uri) ? null : blockedWebResource();
            }
            @Override public void onPageStarted(WebView view, String url, android.graphics.Bitmap favicon) {
                binding.navigationStarted();
                handler.removeCallbacks(startupDeadline);
                handler.postDelayed(startupDeadline, STARTUP_TIMEOUT_MS);
            }
        });
        setContentView(webView);
        handler.postDelayed(startupDeadline, STARTUP_TIMEOUT_MS);
        webView.loadUrl(policy.portalUrl());
    }

    private static WebResourceResponse blockedWebResource() {
        return new WebResourceResponse("text/plain", "UTF-8", 403, "Blocked",
                Collections.emptyMap(), new ByteArrayInputStream("Blocked".getBytes(StandardCharsets.UTF_8)));
    }

    private void startupFailed() {
        Log.e("KorriPortal", "startup failed");
        finish();
    }

    private void applyImmersiveFullscreen() {
        WindowCompat.setDecorFitsSystemWindows(getWindow(), false);
        WindowInsetsControllerCompat insets = WindowCompat.getInsetsController(getWindow(), getWindow().getDecorView());
        insets.hide(WindowInsetsCompat.Type.systemBars());
        insets.setSystemBarsBehavior(WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
    }

    @Override protected void onResume() {
        super.onResume();
        if (ownerBinding != null) ownerBinding.refresh();
        applyImmersiveFullscreen();
    }

    @Override public void onWindowFocusChanged(boolean focused) {
        super.onWindowFocusChanged(focused);
        if (focused) applyImmersiveFullscreen();
    }

    @Override protected void onDestroy() {
        destroyed = true;
        if (ownerBinding != null) { ownerBinding.close(); ownerBinding = null; }
        handler.removeCallbacks(startupDeadline);
        final WebView owned = webView;
        webView = null;
        binding = null;
        if (owned != null) {
            ViewParent parent = owned.getParent();
            if (parent instanceof ViewGroup) ((ViewGroup) parent).removeView(owned);
            WebViewCompat.removeWebMessageListener(owned, KorriRpcBinding.READY_LISTENER);
            owned.stopLoading();
            owned.destroy();
        }
        if (isFinishing()) KorriBrainService.shutdown(this);
        super.onDestroy();
    }
}
