package com.limelight;

import android.os.SystemClock;
import android.view.View;
import android.view.MotionEvent;
import android.view.WindowManager;
import android.content.pm.ActivityInfo;
import android.os.Build;
import androidx.test.platform.app.InstrumentationRegistry;
import org.json.JSONObject;
import android.view.ViewGroup;
import android.webkit.WebView;
import androidx.lifecycle.Lifecycle;
import androidx.test.core.app.ActivityScenario;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import com.simonwjackson.korri.korrid.KorridServer;
import org.junit.Test;
import org.junit.runner.RunWith;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicInteger;
import static org.junit.Assert.*;

/** The shipped NativeActivity, interpreter, QuickJS and authenticated RPC. */
@RunWith(AndroidJUnit4.class)
public class KorriShellDeviceTest {
    @Test public void native_shell_survives_background_and_recreation() throws Exception {
        try (ActivityScenario<KorriShellActivity> scenario = ActivityScenario.launch(KorriShellActivity.class)) {
            awaitReady(scenario);
            AtomicInteger port = new AtomicInteger();
            scenario.onActivity(activity -> {
                assertFalse("Launch path must not contain a browser", hasWebView(activity.getWindow().getDecorView()));
                assertNull("No bootstrap in intent", activity.getIntent().getExtras());
                port.set(activity.shellPort());
            });
            assertEquals(401, health(port.get(), null));
            assertEquals(200, health(port.get(), KorridServer.capability()));
            assertEquals("Owned", new JSONObject(KorridServer.identityStatus()).getString("_tag"));
            assertDragMovesPico(scenario);
            scenario.onActivity(activity -> activity.setRequestedOrientation(ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE));
            SystemClock.sleep(1500);
            awaitReady(scenario);
            assertFullscreen(scenario);
            scenario.onActivity(activity -> activity.setRequestedOrientation(ActivityInfo.SCREEN_ORIENTATION_PORTRAIT));
            SystemClock.sleep(1500);
            awaitReady(scenario);
            assertFullscreen(scenario);
            scenario.moveToState(Lifecycle.State.CREATED);
            scenario.moveToState(Lifecycle.State.RESUMED);
            awaitReady(scenario);
            // Each replacement must release the previous EGL surface, not only
            // survive the first replacement in a fresh process.
            for (int replacement = 0; replacement < 3; replacement++) {
                scenario.recreate();
                awaitReady(scenario);
                assertEquals(200, health(port.get(), KorridServer.capability()));
                assertEquals("Owned", new JSONObject(KorridServer.identityStatus()).getString("_tag"));
            }
        }
    }

    private static void awaitReady(ActivityScenario<KorriShellActivity> scenario) {
        AtomicBoolean ready = new AtomicBoolean();
        long deadline = SystemClock.elapsedRealtime() + 120000;
        do {
            scenario.onActivity(activity -> ready.set(activity.isShellReady() && activity.shellLibraryCount() > 0));
            if (ready.get()) {
                scenario.onActivity(activity -> android.util.Log.i("KorriShellDeviceTest",
                        "PICO_RENDERED count=" + activity.shellLibraryCount() + " selected=" + activity.shellSelectedIndex()));
                return;
            }
            SystemClock.sleep(100);
        } while (SystemClock.elapsedRealtime() < deadline);
        fail("Pico did not render a non-empty real peer library");
    }

    private static void assertFullscreen(ActivityScenario<KorriShellActivity> scenario) {
        scenario.onActivity(activity -> {
            if (Build.VERSION.SDK_INT >= 30) {
                assertEquals(WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS,
                        activity.getWindow().getAttributes().layoutInDisplayCutoutMode);
                assertFalse(activity.getWindow().getDecorView().getRootWindowInsets()
                        .isVisible(android.view.WindowInsets.Type.systemBars()));
            }
        });
    }

    private static void assertDragMovesPico(ActivityScenario<KorriShellActivity> scenario) {
        AtomicInteger initial = new AtomicInteger();
        AtomicInteger width = new AtomicInteger();
        AtomicInteger height = new AtomicInteger();
        scenario.onActivity(activity -> {
            initial.set(activity.shellSelectedIndex());
            width.set(activity.getWindow().getDecorView().getWidth());
            height.set(activity.getWindow().getDecorView().getHeight());
        });
        // Real native input queue, not a call to the gesture mapper. Pico's
        // shelf scrolls horizontally and its grid scrolls vertically.
        swipe(width.get() * .8f, height.get() * .7f, width.get() * .2f, height.get() * .7f);
        swipe(width.get() * .5f, height.get() * .8f, width.get() * .5f, height.get() * .2f);
        AtomicBoolean moved = new AtomicBoolean();
        long deadline = SystemClock.elapsedRealtime() + 5000;
        do {
            scenario.onActivity(activity -> moved.set(activity.shellSelectedIndex() != initial.get()));
            if (moved.get()) return;
            SystemClock.sleep(100);
        } while (SystemClock.elapsedRealtime() < deadline);
        fail("Finger drag did not move Pico's library cursor");
    }

    private static void swipe(float x0, float y0, float x1, float y1) {
        long down = SystemClock.uptimeMillis();
        for (int i = 0; i <= 20; i++) {
            int action = i == 0 ? MotionEvent.ACTION_DOWN : i == 20 ? MotionEvent.ACTION_UP : MotionEvent.ACTION_MOVE;
            MotionEvent event = MotionEvent.obtain(down, SystemClock.uptimeMillis(), action,
                    x0 + (x1 - x0) * i / 20f, y0 + (y1 - y0) * i / 20f, 0);
            event.setSource(android.view.InputDevice.SOURCE_TOUCHSCREEN);
            try {
                assertTrue(InstrumentationRegistry.getInstrumentation().getUiAutomation().injectInputEvent(event, true));
            } finally { event.recycle(); }
            SystemClock.sleep(20);
        }
    }

    private static boolean hasWebView(View view) {
        if (view instanceof WebView) return true;
        if (view instanceof ViewGroup) {
            ViewGroup group = (ViewGroup) view;
            for (int i = 0; i < group.getChildCount(); i++) {
                if (hasWebView(group.getChildAt(i))) return true;
            }
        }
        return false;
    }

    private static int health(int port, String capability) throws Exception {
        HttpURLConnection request = (HttpURLConnection) new URL("http://127.0.0.1:" + port + "/rpc").openConnection();
        try {
            request.setConnectTimeout(3000);
            request.setReadTimeout(3000);
            request.setRequestMethod("POST");
            request.setRequestProperty("Content-Type", "application/json");
            if (capability != null) request.setRequestProperty("Authorization", "Bearer " + capability);
            request.setDoOutput(true);
            request.getOutputStream().write("{\"_tag\":\"system.health\",\"payload\":{}}".getBytes(StandardCharsets.UTF_8));
            return request.getResponseCode();
        } catch (java.io.IOException error) {
            throw new AssertionError("Local RPC request failed");
        } finally { request.disconnect(); }
    }

}
