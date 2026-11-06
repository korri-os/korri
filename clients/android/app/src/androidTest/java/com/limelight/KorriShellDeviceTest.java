package com.limelight;

import android.os.SystemClock;
import android.view.View;
import android.view.MotionEvent;
import android.view.KeyEvent;
import android.view.InputDevice;
import java.util.function.Predicate;
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
import java.util.concurrent.atomic.AtomicReference;
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
            assertOwned(port.get());
            assertDragMovesPico(scenario);
            assertControllerAndRemote(scenario);
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
                assertOwned(port.get());
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

    private static void assertControllerAndRemote(ActivityScenario<KorriShellActivity> scenario) throws Exception {
        assertCursorMoves(scenario, KeyEvent.KEYCODE_DPAD_LEFT, InputDevice.SOURCE_DPAD);
        // X has Linux's Options meaning; Pico opens Find. Android Back must
        // close that surface layer through the native queue, not the Activity.
        press(KeyEvent.KEYCODE_BUTTON_X, InputDevice.SOURCE_GAMEPAD);
        awaitState(scenario, a -> a.shellSurfacePage().equals("find"), "X did not open Pico Find");
        press(KeyEvent.KEYCODE_BACK, InputDevice.SOURCE_KEYBOARD);
        awaitState(scenario, a -> a.shellSurfacePage().equals("home"), "Android Back did not close Find");
        press(KeyEvent.KEYCODE_BACK, InputDevice.SOURCE_KEYBOARD);
        SystemClock.sleep(500);
        scenario.onActivity(a -> {
            assertFalse("Back at root must not finish the native Activity", a.isFinishing());
            assertEquals("home", a.shellSurfacePage());
        });
        switchSurface();
        awaitState(scenario, a -> a.shellSurfaceId().equals("zone") && a.shellLibraryCount() > 0,
                "L1+R1 did not render Zone");
        assertCursorMoves(scenario, KeyEvent.KEYCODE_DPAD_RIGHT, InputDevice.SOURCE_GAMEPAD);
        // Remote Center and Enter are Confirm too. Zone's Options button opens
        // a non-destructive layer, and B returns to the card band.
        press(KeyEvent.KEYCODE_BUTTON_X, InputDevice.SOURCE_GAMEPAD);
        press(KeyEvent.KEYCODE_BUTTON_B, InputDevice.SOURCE_GAMEPAD);
        switchSurface();
        awaitState(scenario, a -> a.shellSurfaceId().equals("pico") && a.shellLibraryCount() > 0,
                "L1+R1 did not return to Pico");
        press(KeyEvent.KEYCODE_DPAD_CENTER, InputDevice.SOURCE_DPAD);
        awaitState(scenario, a -> a.shellSurfacePage().equals("detail"), "Remote Center did not confirm Pico card");
        press(KeyEvent.KEYCODE_BACK, InputDevice.SOURCE_KEYBOARD);
        awaitState(scenario, a -> a.shellSurfacePage().equals("home"), "Back did not close Pico detail");
        press(KeyEvent.KEYCODE_ENTER, InputDevice.SOURCE_KEYBOARD);
        awaitState(scenario, a -> a.shellSurfacePage().equals("detail"), "Enter did not confirm Pico card");
        press(KeyEvent.KEYCODE_BUTTON_B, InputDevice.SOURCE_GAMEPAD);
        awaitState(scenario, a -> a.shellSurfacePage().equals("home"), "B did not close Pico detail");
    }

    private static void assertCursorMoves(ActivityScenario<KorriShellActivity> scenario, int code, int source) throws Exception {
        AtomicInteger port = new AtomicInteger();
        AtomicReference<String> surface = new AtomicReference<>();
        scenario.onActivity(a -> {
            port.set(a.shellPort());
            surface.set(a.shellSurfaceId());
        });
        // Pico's pressed() changes state.focus; sync() reports its cart with
        // FocusGame. Observe that stable game ID through the real model, not
        // home.selected-index (the chosen cart's position in a moving window).
        String before = focusedGame(port.get(), surface.get());
        int opposite = code == KeyEvent.KEYCODE_DPAD_LEFT ? KeyEvent.KEYCODE_DPAD_RIGHT : KeyEvent.KEYCODE_DPAD_LEFT;
        for (int direction : new int[] { code, opposite, KeyEvent.KEYCODE_DPAD_DOWN,
                KeyEvent.KEYCODE_DPAD_RIGHT, KeyEvent.KEYCODE_DPAD_LEFT, KeyEvent.KEYCODE_DPAD_UP }) {
            press(direction, source);
            long deadline = SystemClock.elapsedRealtime() + 2000;
            do {
                String after = focusedGame(port.get(), surface.get());
                if (after != null && !after.equals(before)) return;
                SystemClock.sleep(100);
            } while (SystemClock.elapsedRealtime() < deadline);
        }
        fail("Injected D-pad sequence did not change the surface's reported focused game");
    }

    private static String focusedGame(int port, String surface) throws Exception {
        HttpURLConnection request = (HttpURLConnection) new URL("http://127.0.0.1:" + port + "/rpc").openConnection();
        try {
            request.setConnectTimeout(3000);
            request.setReadTimeout(3000);
            request.setRequestMethod("POST");
            request.setRequestProperty("Content-Type", "application/json");
            request.setRequestProperty("Authorization", "Bearer " + KorridServer.capability());
            request.setDoOutput(true);
            JSONObject body = new JSONObject().put("_tag", "app.surface.model")
                    .put("payload", new JSONObject().put("surfaceId", surface));
            request.getOutputStream().write(body.toString().getBytes(StandardCharsets.UTF_8));
            assertEquals(200, request.getResponseCode());
            java.io.ByteArrayOutputStream bytes = new java.io.ByteArrayOutputStream();
            try (java.io.InputStream input = request.getInputStream()) {
                byte[] buffer = new byte[4096];
                int count;
                while ((count = input.read(buffer)) != -1) bytes.write(buffer, 0, count);
            }
            JSONObject outcome = new JSONObject(bytes.toString(StandardCharsets.UTF_8.name())).getJSONObject("outcome");
            assertEquals("Ok", outcome.getString("_tag"));
            JSONObject focused = outcome.getJSONObject("payload").optJSONObject("focusedGame");
            return focused == null ? null : focused.getString("gameId");
        } finally { request.disconnect(); }
    }

    private static void awaitState(ActivityScenario<KorriShellActivity> scenario,
                                   Predicate<KorriShellActivity> condition, String failure) {
        AtomicBoolean matched = new AtomicBoolean();
        long deadline = SystemClock.elapsedRealtime() + 15000;
        do {
            scenario.onActivity(a -> matched.set(condition.test(a)));
            if (matched.get()) return;
            SystemClock.sleep(100);
        } while (SystemClock.elapsedRealtime() < deadline);
        fail(failure);
    }

    private static void switchSurface() {
        key(KeyEvent.KEYCODE_BUTTON_L1, KeyEvent.ACTION_DOWN, InputDevice.SOURCE_GAMEPAD);
        key(KeyEvent.KEYCODE_BUTTON_R1, KeyEvent.ACTION_DOWN, InputDevice.SOURCE_GAMEPAD);
        key(KeyEvent.KEYCODE_BUTTON_R1, KeyEvent.ACTION_UP, InputDevice.SOURCE_GAMEPAD);
        key(KeyEvent.KEYCODE_BUTTON_L1, KeyEvent.ACTION_UP, InputDevice.SOURCE_GAMEPAD);
    }

    private static void press(int code, int source) {
        key(code, KeyEvent.ACTION_DOWN, source);
        key(code, KeyEvent.ACTION_UP, source);
    }

    private static void key(int code, int action, int source) {
        long now = SystemClock.uptimeMillis();
        KeyEvent event = new KeyEvent(now, now, action, code, 0, 0,
                -1, 0, 0, source);
        assertTrue("Key injection failed", InstrumentationRegistry.getInstrumentation().getUiAutomation().injectInputEvent(event, true));
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

    // Check the public authenticated wire, not a JNI method unused by the
    // shipped activity (R8 can legitimately remove that Java declaration).
    private static void assertOwned(int port) throws Exception {
        HttpURLConnection request = (HttpURLConnection) new URL("http://127.0.0.1:" + port + "/rpc").openConnection();
        try {
            request.setConnectTimeout(3000);
            request.setReadTimeout(3000);
            request.setRequestMethod("POST");
            request.setRequestProperty("Content-Type", "application/json");
            request.setRequestProperty("Authorization", "Bearer " + KorridServer.capability());
            request.setDoOutput(true);
            request.getOutputStream().write("{\"_tag\":\"system.identity.status\",\"payload\":{}}".getBytes(StandardCharsets.UTF_8));
            assertEquals(200, request.getResponseCode());
            java.io.ByteArrayOutputStream bytes = new java.io.ByteArrayOutputStream();
            try (java.io.InputStream input = request.getInputStream()) {
                byte[] buffer = new byte[4096];
                int count;
                while ((count = input.read(buffer)) != -1) bytes.write(buffer, 0, count);
            }
            JSONObject outcome = new JSONObject(bytes.toString(StandardCharsets.UTF_8.name())).getJSONObject("outcome");
            assertEquals("Ok", outcome.getString("_tag"));
            assertEquals("Owned", outcome.getJSONObject("payload").getJSONObject("identity").getString("_tag"));
        } finally { request.disconnect(); }
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
