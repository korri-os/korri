package com.limelight;

import android.app.NativeActivity;
import android.os.Bundle;
import com.simonwjackson.korri.korrid.KorriBrainService;
import com.simonwjackson.korri.korrid.KorridServer;

/** Android lifecycle and in-process bootstrap for clients/shell. No browser. */
public class KorriShellActivity extends NativeActivity {
    private int port;
    private volatile boolean shellReady;

    @Override protected void onCreate(Bundle savedInstanceState) {
        // NativeActivity starts android_main in super.onCreate. Publish the
        // connection first. Keep the existing origin until milestone M5.
        try {
            KorriTrustedPortalWebViewPolicy policy =
                    KorriTrustedPortalWebViewPolicy.forRuntime(BuildConfig.DEBUG, BuildConfig.PORTAL_DEV_URL);
            port = KorriBrainService.ensureRunning(this, policy.portalOrigin(), getFilesDir().getAbsolutePath());
        } catch (RuntimeException error) {
            // The native screen still opens and reports that korrid is unavailable.
            port = 0;
        }
        super.onCreate(savedInstanceState);
    }

    // Called only in-process by Rust, never placed in an intent or URL.
    public int shellPort() { return port; }
    public String shellCapability() { return port == 0 ? "" : KorridServer.capability(); }
    public String shellVersion() { return BuildConfig.VERSION_NAME; }
    public void shellStarted() { shellReady = true; }
    public boolean isShellReady() { return shellReady; }

    // The foreground service, not an activity, owns korrid. In particular do
    // not stop it on rotation, Back, or while Artemis Game is in front.
}
