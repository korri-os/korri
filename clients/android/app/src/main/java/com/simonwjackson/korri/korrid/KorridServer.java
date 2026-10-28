package com.simonwjackson.korri.korrid;

import android.util.Log;
import com.limelight.BuildConfig;

/**
 * JNI edge around the embedded Rust korrid server. The portal's brain
 * lives behind this boundary; Java starts/stops it and hands the trusted
 * WebView its per-server capability. Symbol names mirror services/korrid
 * src/android.rs.
 */
public final class KorridServer {
    static {
        System.loadLibrary("korrid");
    }

    private KorridServer() {}

    private static native String version();
    private static native int start(
            android.content.Context applicationContext, String allowedOrigin, String localStorageRoot, String privateStateRoot, String productRelays);
    public static native String capability();
    /** Native peer labels and addresses that Artemis may probe for Moonlight hosts. */
    public static native String moonlightHostCandidates();
    /** Public identity state only. The person private key never enters Korri. */
    public static native String identityStatus();
    /** Unsigned NIP-78 template for the selected person signer. */
    public static native String ownerBindingTemplate(long createdAt);
    /** Verifies and stores one signed public event against the exact template. */
    public static native String applyOwnerBinding(
            String unsignedTemplateJson,
            String expectedOwnerPublicKey,
            String signedEventJson);
    /** Provision this process's public Moonlight certificate through the embedded brain. */
    public static native String provisionMoonlightCertificate(
            String hostUuid, String publicClientCertificate);
    /** Returns a short-lived one-use receipt for an Android-validated game folder. */
    public static native String issueFolderSelectionReceipt(String canonicalApprovedPath);
    /** Attach or update one native signer activity; zero creates a new generation. */
    public static native long updateOwnerSigner(long generation, String snapshotJson);
    /** Consume an owner-signing effect requested through authenticated local RPC. */
    public static native String takeOwnerBindingRequest(long generation);
    public static native void detachOwnerSigner(long generation);
    public static native void stop();
    /** Semantic Leave from the native viewer; Rust applies the existing exact RPC. */
    public static native boolean leaveViewer(String expectedLaunchId);

    /** Called by Rust through the captured application class, never a browser bridge. */
    public static String networkSnapshot(android.content.Context context) {
        return KorridNetwork.snapshot(context);
    }

    /** Exact native effects called by Rust, never exposed to a WebView. */
    public static String viewerStart(String launchId, String address) throws Exception {
        return com.limelight.KorriStreamViewer.start(launchId, address);
    }
    public static String viewerStatus(String launchId) {
        return com.limelight.KorriStreamViewer.status(launchId);
    }
    public static String viewerBackground(String launchId) throws Exception {
        return com.limelight.KorriStreamViewer.background(launchId);
    }
    public static String viewerResume(String launchId) throws Exception {
        return com.limelight.KorriStreamViewer.resume(launchId);
    }
    public static String viewerStop(String launchId) throws Exception {
        return com.limelight.KorriStreamViewer.stop(launchId);
    }

    public static int startAndLog(
            android.content.Context context, String allowedOrigin, String localStorageRoot, String privateStateRoot) {
        com.limelight.KorriStreamViewer.initialize(context);
        int port = start(context.getApplicationContext(), allowedOrigin, localStorageRoot, privateStateRoot, BuildConfig.PRODUCT_RELAYS);
        Log.i("KorridServer", "brain started");
        return port;
    }
}
