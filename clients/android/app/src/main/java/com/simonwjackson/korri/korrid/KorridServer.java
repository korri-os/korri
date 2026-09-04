package com.simonwjackson.korri.korrid;

import android.content.Intent;
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
            String allowedOrigin, String localStorageRoot, String privateStateRoot);
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
    /** Tagged authorization result; a valid Moonlight launch is consumed once. */
    public static native String authorizeMoonlightLaunchSpec(String specJson);
    public static final int LOCAL_LAUNCH_REJECTED = 0;
    public static final int LOCAL_LAUNCH_PUBLISH = 1;
    public static final int LOCAL_LAUNCH_RESUME = 2;
    /** Atomically authorizes local start and attaches exact optional RetroArch authority. */
    public static native int authorizeLaunchSpec(String specJson, Intent intent);
    /** Verify the signed local handoff and publish its launch context. */
    public static native String publishLocalActiveLaunch(String specJson);
    /** Verify the signed Moonlight handoff; Java supplies its own Game component. */
    public static native String publishMoonlightActiveLaunch(
            String specJson, String applicationPackage, String gameClassName);
    /** Compare-and-clear the exact current launch. */
    public static native boolean clearActiveLaunch(String launchId);
    /** Read the current Rust snapshot; returns JSON null when idle. */
    public static native String activeLaunch();
    /** Publish strict live Moonlight values/fulfillability for the exact active launch. */
    public static native boolean publishMoonlightExecutorState(String stateJson);
    /** Compare-and-clear strict live Moonlight state for the exact launch. */
    public static native boolean clearMoonlightExecutorState(
            String launchId, String generation);
    /** Verify and consume one protected instruction for the current launch. */
    public static native String authorizePlatformInstruction(String instructionJson);
    /** Returns a short-lived one-use receipt for an Android-validated game folder. */
    public static native String issueFolderSelectionReceipt(String canonicalApprovedPath);
    public static native void stop();

    public static int startAndLog(
            String allowedOrigin, String localStorageRoot, String privateStateRoot) {
        int port = start(allowedOrigin, localStorageRoot, privateStateRoot);
        Log.i("KorridServer", version() + " listening on 127.0.0.1:" + port);
        return port;
    }
}
