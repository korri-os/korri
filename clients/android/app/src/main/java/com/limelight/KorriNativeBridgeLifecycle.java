package com.limelight;

import android.net.Uri;

/**
 * Owns the trusted WebView timing invariant for KorriNative and KorriRpc.
 * Android exposes a newly-added interface only after a future navigation, so
 * the trusted portal's initial interface must survive its load callbacks.
 */
final class KorriNativeBridgeLifecycle {
    static final String BRIDGE_NAME = "KorriNative";
    // Treaty: contracts/bridge/korri-rpc-bridge.ts.
    static final String RPC_BRIDGE_NAME = "KorriRpc";

    interface Operations {
        void addJavascriptInterface(String name);

        void removeJavascriptInterface(String name);
    }

    void removeJavascriptInterfaces(Operations operations) {
        operations.removeJavascriptInterface(BRIDGE_NAME);
        operations.removeJavascriptInterface(RPC_BRIDGE_NAME);
    }

    void installBeforeInitialLoad(
            Uri uri,
            KorriTrustedPortalWebViewPolicy portalPolicy,
            Operations operations) {
        if (portalPolicy.isTrustedPortalResource(uri)) {
            operations.addJavascriptInterface(BRIDGE_NAME);
            operations.addJavascriptInterface(RPC_BRIDGE_NAME);
        }
    }

    void onMainFramePageStarted(
            Uri uri,
            KorriTrustedPortalWebViewPolicy portalPolicy,
            Operations operations) {
        if (!portalPolicy.isTrustedPortalResource(uri)) {
            removeJavascriptInterfaces(operations);
        }
    }

    void onMainFramePageFinished() {
        // The already-injected bridge is intentionally preserved. Removing or
        // re-adding it here would make the current trusted document miss it.
    }
}
