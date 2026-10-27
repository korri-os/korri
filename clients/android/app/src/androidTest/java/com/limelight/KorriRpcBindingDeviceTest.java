package com.limelight;

import android.os.SystemClock;
import android.view.View;
import android.view.ViewGroup;
import android.webkit.WebView;
import androidx.test.core.app.ActivityScenario;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.platform.app.InstrumentationRegistry;
import org.json.JSONObject;
import org.json.JSONTokener;
import org.junit.Test;
import org.junit.runner.RunWith;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;
import static org.junit.Assert.*;

/** Runs the shipped WebView, native library and HTTP server, not a JS substitute. */
@RunWith(AndroidJUnit4.class)
public class KorriRpcBindingDeviceTest {
    @Test public void trusted_portal_consumes_private_binding_and_reaches_its_brain() throws Exception {
        try (ActivityScenario<KorriShellActivity> scenario = ActivityScenario.launch(KorriShellActivity.class)) {
            AtomicReference<WebView> page = new AtomicReference<>();
            scenario.onActivity(activity -> page.set(findWebView(activity.getWindow().getDecorView())));
            WebView web = page.get();
            assertNotNull("Shell must contain a WebView", web);
            long deadline = SystemClock.elapsedRealtime() + 15000;
            while (!"true".equals(evaluate(web, "typeof window.KorriRpc === 'object'"))) {
                assertTrue("Portal did not receive its binding", SystemClock.elapsedRealtime() < deadline);
                SystemClock.sleep(50);
            }
            assertEquals("true", evaluate(web,
                    "Object.isFrozen(window.KorriRpc) && !Object.getOwnPropertyDescriptor(window, 'KorriRpc').configurable"
                    + " && !Object.getOwnPropertyDescriptor(window, 'KorriRpc').writable"
                    + " && typeof window.KorriNative === 'undefined'"));
            String encoded = evaluate(web, "JSON.stringify({port: KorriRpc.korridPort(), capability: KorriRpc.korridCapability()})");
            JSONObject connection;
            try {
                connection = new JSONObject((String) new JSONTokener(encoded).nextValue());
            } catch (Exception error) {
                throw new AssertionError("Private binding could not be decoded");
            }
            int port = connection.getInt("port");
            String capability = connection.getString("capability");
            assertTrue("Invalid private port", port > 0 && port <= 65535);
            assertTrue("Invalid private capability", capability.matches("[0-9a-f]{64}"));
            assertEquals(401, health(port, null));
            assertEquals(200, health(port, capability));
            assertEquals("false", evaluate(web,
                    "Object.values(localStorage).some(value => value.includes(KorriRpc.korridCapability()))"));
            assertEquals("false", evaluate(web, "location.href.includes(KorriRpc.korridCapability())"));
            // A same-origin child never gets its own connection binding.
            evaluate(web, "window.child = document.createElement('iframe'); child.src='/assets/portal/index.html'; document.body.appendChild(child); true");
            SystemClock.sleep(500);
            assertEquals("false", evaluate(web, "Object.prototype.hasOwnProperty.call(child.contentWindow, 'KorriRpc')"));
        }
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

    private static String evaluate(WebView page, String script) throws Exception {
        CountDownLatch done = new CountDownLatch(1);
        AtomicReference<String> result = new AtomicReference<>();
        InstrumentationRegistry.getInstrumentation().runOnMainSync(() ->
                page.evaluateJavascript(script, value -> { result.set(value); done.countDown(); }));
        assertTrue("WebView evaluation timed out", done.await(5, TimeUnit.SECONDS));
        return result.get();
    }

    private static WebView findWebView(View view) {
        if (view instanceof WebView) return (WebView) view;
        if (view instanceof ViewGroup) {
            ViewGroup group = (ViewGroup) view;
            for (int i = 0; i < group.getChildCount(); i++) {
                WebView found = findWebView(group.getChildAt(i));
                if (found != null) return found;
            }
        }
        return null;
    }
}
