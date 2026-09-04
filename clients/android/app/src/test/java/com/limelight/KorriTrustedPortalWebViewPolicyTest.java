package com.limelight;

import android.net.Uri;
import android.webkit.JavascriptInterface;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;

import java.lang.reflect.Method;
import java.util.Arrays;
import java.util.Set;
import java.util.TreeSet;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

@RunWith(RobolectricTestRunner.class)
public class KorriTrustedPortalWebViewPolicyTest {
    private static final Set<String> TREATY_METHODS = new TreeSet<>(Arrays.asList(
            "acknowledgeGameFolderPicker",
            "backgroundNotice",
            "bridgeVersion",
            "gameFolderPickerSnapshot",
            "korridCapability",
            "korridPort",
            "launchLocal",
            "localGameAssetUrl",
            "openGameFolderPicker",
            "openNotificationSettings",
            "openOverlaySettings",
            "openStorageAccessSettings",
            "ownerBindingSnapshot",
            "queryStreamApps",
            "queryStreamHosts",
            "overlayPermission",
            "requestBackgroundNotice",
            "startOwnerBinding",
            "startStream",
            "storageAccess",
            "systemInfo"));

    private final KorriTrustedPortalWebViewPolicy policy = new KorriTrustedPortalWebViewPolicy();

    @Test
    public void javascriptBridgeExposesOnlyTheBridgeTreatyMethods() throws Exception {
        Class<?> bridge = Class.forName("com.limelight.KorriShellActivity$KorriNativeBridge");
        Set<String> exposed = new TreeSet<>();
        for (Method method : bridge.getDeclaredMethods()) {
            if (method.getAnnotation(JavascriptInterface.class) != null) {
                exposed.add(method.getName());
            }
        }

        assertEquals(TREATY_METHODS, exposed);
        assertFalse(exposed.contains("korriRpc"));
        assertFalse(exposed.contains("launchGame"));
        assertFalse(exposed.contains("getSettingsSchema"));
        assertFalse(exposed.contains("setSetting"));
    }

    @Test
    public void trustedPortalOriginIsCanonicalHttpsAssetOrigin() {
        assertEquals("https://appassets.androidplatform.net/assets/portal/index.html",
                policy.portalUrl());
        assertEquals("https://appassets.androidplatform.net", policy.portalOrigin());
        assertTrue(policy.isTrustedPortalResource(Uri.parse(
                "https://appassets.androidplatform.net:443/assets/portal/index.html")));
        assertTrue(policy.isBundledPortalAsset(Uri.parse(
                "https://appassets.androidplatform.net:443/assets/portal/index.html")));
        assertTrue(policy.isTrustedPortalResource(Uri.parse(
                "https://APPASSETS.ANDROIDPLATFORM.NET/assets/portal/chunk.js")));
        assertTrue(policy.isTrustedLocalGameAsset(Uri.parse(
                "https://appassets.androidplatform.net/game-assets/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.png")));
    }

    @Test
    public void untrustedMainFrameNavigationCannotEnterThePrivilegedWebView() {
        assertEquals(
                KorriTrustedPortalWebViewPolicy.NavigationAction.ALLOW_IN_WEBVIEW,
                policy.navigationAction(Uri.parse(
                        "https://appassets.androidplatform.net/assets/portal/index.html"), true));
        assertEquals(
                KorriTrustedPortalWebViewPolicy.NavigationAction.OPEN_EXTERNALLY,
                policy.navigationAction(Uri.parse("https://example.com/page"), true));
        assertEquals(
                KorriTrustedPortalWebViewPolicy.NavigationAction.BLOCK,
                policy.navigationAction(Uri.parse("http://127.0.0.1:43117/rpc"), true));
        assertEquals(
                KorriTrustedPortalWebViewPolicy.NavigationAction.BLOCK,
                policy.navigationAction(Uri.parse("javascript:alert(1)"), true));
    }

    @Test
    public void subframesAndResourcesMustStayOnTheTrustedAssetOrigin() {
        assertTrue(policy.isTrustedPortalResource(Uri.parse(
                "https://appassets.androidplatform.net/assets/portal/style.css")));
        assertFalse(policy.isTrustedPortalResource(Uri.parse(
                "https://appassets.androidplatform.net.evil/assets/portal/style.css")));
        assertFalse(policy.isTrustedPortalResource(Uri.parse(
                "http://appassets.androidplatform.net/assets/portal/style.css")));
        assertFalse(policy.isTrustedPortalResource(Uri.parse(
                "https://appassets.androidplatform.net:444/assets/portal/style.css")));
        assertFalse(policy.isTrustedPortalResource(Uri.parse(
                "https://appassets.androidplatform.net/assetsevil/portal/style.css")));
        assertFalse(policy.isTrustedPortalResource(Uri.parse(
                "https://example.com/assets/portal/style.css")));

        assertEquals(
                KorriTrustedPortalWebViewPolicy.NavigationAction.BLOCK,
                policy.navigationAction(Uri.parse("https://example.com/frame.html"), false));
    }

    @Test
    public void releaseRuntimeIgnoresDebugPortalOverride() {
        KorriTrustedPortalWebViewPolicy releasePolicy =
                KorriTrustedPortalWebViewPolicy.forRuntime(false, "http://192.0.2.10:5173/");

        assertEquals(KorriTrustedPortalWebViewPolicy.TRUSTED_PORTAL_URL,
                releasePolicy.portalUrl());
        assertEquals("https://appassets.androidplatform.net", releasePolicy.portalOrigin());
        assertFalse(releasePolicy.isTrustedPortalResource(Uri.parse("http://192.0.2.10:5173/")));
        assertEquals(
                KorriTrustedPortalWebViewPolicy.NavigationAction.OPEN_EXTERNALLY,
                releasePolicy.navigationAction(Uri.parse("http://192.0.2.10:5173/"), true));
    }

    @Test
    public void debugRuntimeTrustsOnlyTheConfiguredNetworkPortalOrigin() {
        KorriTrustedPortalWebViewPolicy debugPolicy =
                KorriTrustedPortalWebViewPolicy.forRuntime(true, "http://192.0.2.10:5173/portal/");

        assertEquals("http://192.0.2.10:5173/portal/", debugPolicy.portalUrl());
        assertEquals("http://192.0.2.10:5173", debugPolicy.portalOrigin());
        assertTrue(debugPolicy.isTrustedPortalResource(Uri.parse("http://192.0.2.10:5173/src/main.ts")));
        assertTrue(debugPolicy.isTrustedPortalResource(Uri.parse("http://192.0.2.10:5173/assets/app.css")));
        assertFalse("dev resources are fetched by WebView, not the appassets loader",
                debugPolicy.isBundledPortalAsset(Uri.parse("http://192.0.2.10:5173/assets/app.css")));
        assertFalse(debugPolicy.isTrustedPortalResource(Uri.parse("http://192.0.2.10:5174/src/main.ts")));
        assertFalse(debugPolicy.isTrustedPortalResource(Uri.parse("http://192.0.2.10.evil:5173/src/main.ts")));
        assertFalse(debugPolicy.isTrustedPortalResource(Uri.parse("https://192.0.2.10:5173/src/main.ts")));
        assertEquals(
                KorriTrustedPortalWebViewPolicy.NavigationAction.ALLOW_IN_WEBVIEW,
                debugPolicy.navigationAction(Uri.parse("http://192.0.2.10:5173/portal/"), true));
    }

    @Test
    public void localGameAssetsAllowOnlyOneExactUnencodedContentAddressedPath() {
        String good = "https://appassets.androidplatform.net/game-assets/"
                + "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.png";
        assertTrue(policy.isTrustedLocalGameAsset(Uri.parse(good)));

        KorriTrustedPortalWebViewPolicy debugPolicy =
                KorriTrustedPortalWebViewPolicy.forRuntime(true, "http://192.0.2.10:5173/");
        assertTrue(debugPolicy.isTrustedLocalGameAsset(Uri.parse(good)));
        assertFalse(debugPolicy.isTrustedLocalGameAsset(Uri.parse(
                "http://192.0.2.10:5173/game-assets/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.png")));

        for (String url : Arrays.asList(
                good + "?v=1",
                good + "#fragment",
                "https://appassets.androidplatform.net/game-assets/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.gif",
                "https://appassets.androidplatform.net/game-assets/AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA.png",
                "https://appassets.androidplatform.net/game-assets/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.png",
                "https://appassets.androidplatform.net/game-assets/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.png/",
                "https://appassets.androidplatform.net/game-assets/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.png/extra",
                "https://appassets.androidplatform.net/game-assets//aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.png",
                "https://appassets.androidplatform.net/game-assets/../aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.png",
                "https://appassets.androidplatform.net/game-assets/%61aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.png",
                "https://appassets.androidplatform.net/assets/portal/index.html",
                "https://example.com/game-assets/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.png")) {
            assertFalse(url, policy.isTrustedLocalGameAsset(Uri.parse(url)));
        }
    }
}
