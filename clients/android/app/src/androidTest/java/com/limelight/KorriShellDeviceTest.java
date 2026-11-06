package com.limelight;

import android.os.SystemClock;
import android.view.View;
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
            scenario.moveToState(Lifecycle.State.CREATED);
            scenario.moveToState(Lifecycle.State.RESUMED);
            awaitReady(scenario);
            // Each replacement must release the previous EGL surface, not only
            // survive the first replacement in a fresh process.
            for (int replacement = 0; replacement < 3; replacement++) {
                scenario.recreate();
                awaitReady(scenario);
                assertEquals(200, health(port.get(), KorridServer.capability()));
            }
        }
    }

    private static void awaitReady(ActivityScenario<KorriShellActivity> scenario) {
        AtomicBoolean ready = new AtomicBoolean();
        long deadline = SystemClock.elapsedRealtime() + 30000;
        do {
            scenario.onActivity(activity -> ready.set(activity.isShellReady()));
            if (ready.get()) return;
            SystemClock.sleep(100);
        } while (SystemClock.elapsedRealtime() < deadline);
        fail("Native interpreter/QuickJS startup did not complete");
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
