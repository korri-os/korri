package com.simonwjackson.korri.korrid;

import android.util.Log;

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
            android.content.Context applicationContext, String allowedOrigin, String localStorageRoot, String privateStateRoot);
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
    public static native void stop();

    public static int startAndLog(
            android.content.Context context, String allowedOrigin, String localStorageRoot, String privateStateRoot) {
        int port = start(context.getApplicationContext(), allowedOrigin, localStorageRoot, privateStateRoot);
        Log.i("KorridServer", "brain started");
        return port;
    }
}
