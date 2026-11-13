package com.limelight;

import android.app.NativeActivity;
import android.os.Bundle;
import android.os.Build;
import android.view.WindowManager;
import android.hardware.input.InputManager;
import android.content.Context;
import java.util.ArrayList;
import java.util.concurrent.ConcurrentLinkedQueue;
import androidx.core.view.WindowCompat;
import androidx.core.view.WindowInsetsCompat;
import androidx.core.view.WindowInsetsControllerCompat;
import java.io.File;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.IOException;
import com.simonwjackson.korri.korrid.KorriBrainService;
import com.simonwjackson.korri.korrid.KorridServer;

/** Android lifecycle and in-process bootstrap for clients/shell. No browser. */
public class KorriShellActivity extends NativeActivity {
    private int port;
    private String origin;
    private volatile boolean shellReady;
    private volatile int libraryCount;
    private volatile int selectedIndex = -1;
    private String packagePath;
    private volatile String surfaceId = "";
    private volatile String surfacePage = "";
    private InputManager inputManager;
    private final ConcurrentLinkedQueue<Integer> removedInputDevices = new ConcurrentLinkedQueue<>();
    private final InputManager.InputDeviceListener inputDevices = new InputManager.InputDeviceListener() {
        @Override public void onInputDeviceAdded(int id) { }
        @Override public void onInputDeviceChanged(int id) { }
        @Override public void onInputDeviceRemoved(int id) { removedInputDevices.add(id); }
    };

    @Override protected void onCreate(Bundle savedInstanceState) {
        // NativeActivity starts android_main in super.onCreate. Publish the
        // connection first. Keep the existing origin until milestone M5.
        try {
            KorriTrustedPortalWebViewPolicy policy =
                    KorriTrustedPortalWebViewPolicy.forRuntime(BuildConfig.DEBUG, BuildConfig.PORTAL_DEV_URL);
            origin = policy.portalOrigin();
            port = KorriBrainService.ensureRunning(this, origin, getFilesDir().getAbsolutePath());
        } catch (RuntimeException error) {
            // The native screen still opens and reports that korrid is unavailable.
            port = 0;
        }
        try {
            File surfaces = new File(getCacheDir(), "surfaces");
            for (String id : new String[] { "velvet", "pico", "zone" }) {
                copyPackage("surfaces/" + id, new File(surfaces, id));
            }
            packagePath = surfaces.getAbsolutePath();
        } catch (IOException error) {
            throw new IllegalStateException("Could not unpack surfaces", error);
        }
        super.onCreate(savedInstanceState);
        inputManager = (InputManager) getSystemService(Context.INPUT_SERVICE);
        inputManager.registerInputDeviceListener(inputDevices, null);
        applyFullscreen();
    }

    // Called only in-process by Rust, never placed in an intent or URL.
    public int shellPort() { return port; }
    public String shellOrigin() { return origin; }
    public String shellCapability() { return port == 0 ? "" : KorridServer.capability(); }
    public String shellPackagePath() { return packagePath; }
    // Android's device-removal callback has no android-activity equivalent.
    // The native source drains it so disconnecting a held direction stops repeat.
    public int[] shellTakeRemovedInputDevices() {
        ArrayList<Integer> ids = new ArrayList<>();
        Integer id;
        while ((id = removedInputDevices.poll()) != null) ids.add(id);
        int[] result = new int[ids.size()];
        for (int i = 0; i < result.length; i++) result[i] = ids.get(i);
        return result;
    }

    // Readiness comes from a rendered surface frame, not just native startup.
    public void shellRendered(String id, String page, int count, int selected) {
        surfacePage = page;
        surfaceId = id;
        libraryCount = count;
        selectedIndex = selected;
        shellReady = true;
    }
    // Host-neutral Surface lifecycle. Rust releases its EGL/ANativeWindow before ack.
    public android.view.Surface shellTakeOverlaySurface() { return KorriSurfaceHost.takeSurface(); }
    public float shellOverlayDensity() { return KorriSurfaceHost.density(); }
    public boolean shellOverlayResized() { return KorriSurfaceHost.takeResized(); }
    public boolean shellOverlayClosing() { return KorriSurfaceHost.closing(); }
    public void shellOverlayReleased(boolean failed) { KorriSurfaceHost.acknowledgeRelease(failed); }
    public int[] shellTakeOverlayKeys() { return KorriSurfaceHost.takeKeys(); }
    public float[] shellTakeOverlayPointers() { return KorriSurfaceHost.takePointers(); }
    public int[] shellTakeOverlayAxes() { return KorriSurfaceHost.takeAxes(); }

    public String shellSurfacePage() { return surfacePage; }
    public String shellSurfaceId() { return surfaceId; }
    public int shellLibraryCount() { return libraryCount; }
    public int shellSelectedIndex() { return selectedIndex; }
    public boolean isShellReady() { return shellReady; }

    private void copyPackage(String asset, File target) throws IOException {
        String[] children = getAssets().list(asset);
        if (children == null) throw new IOException("Missing surface asset");
        if (children.length > 0) {
            if (!target.isDirectory() && !target.mkdirs()) throw new IOException("Surface directory");
            for (String child : children) copyPackage(asset + "/" + child, new File(target, child));
        } else {
            try (InputStream input = getAssets().open(asset);
                 FileOutputStream output = new FileOutputStream(target)) {
                byte[] buffer = new byte[16384];
                int count;
                while ((count = input.read(buffer)) != -1) output.write(buffer, 0, count);
            }
        }
    }

    private void applyFullscreen() {
        WindowCompat.setDecorFitsSystemWindows(getWindow(), false);
        if (Build.VERSION.SDK_INT >= 28) {
            WindowManager.LayoutParams attributes = getWindow().getAttributes();
            attributes.layoutInDisplayCutoutMode = Build.VERSION.SDK_INT >= 30
                    ? WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS
                    : WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;
            getWindow().setAttributes(attributes);
        }
        getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        WindowInsetsControllerCompat insets = WindowCompat.getInsetsController(getWindow(), getWindow().getDecorView());
        insets.hide(WindowInsetsCompat.Type.systemBars());
        insets.setSystemBarsBehavior(WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
    }

    @Override protected void onResume() {
        super.onResume();
        applyFullscreen();
    }

    @Override public void onWindowFocusChanged(boolean focused) {
        super.onWindowFocusChanged(focused);
        if (focused) applyFullscreen();
    }

    @Override protected void onDestroy() {
        inputManager.unregisterInputDeviceListener(inputDevices);
        super.onDestroy();
    }

    // The native input filter consumes BACK and sends semantic Back. Do not
    // let the framework finish this Activity if a Back arrives before it is ready.
    @SuppressWarnings("deprecation")
    @Override public void onBackPressed() { }

    // The foreground service, not an activity, owns korrid. In particular do
    // not stop it on rotation, Back, or while Artemis Game is in front.
}
