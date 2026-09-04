package com.limelight;

import android.os.SystemClock;
import android.view.KeyEvent;
import android.view.View;
import android.view.ViewGroup;
import android.webkit.WebView;

import androidx.test.core.app.ActivityScenario;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.platform.app.InstrumentationRegistry;

import org.json.JSONArray;
import org.json.JSONObject;
import org.junit.Test;
import org.junit.runner.RunWith;

import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertNotNull;
import static org.junit.Assert.assertTrue;
import static org.junit.Assert.fail;

@RunWith(AndroidJUnit4.class)
public class KorriNativeBridgeContractTest {
    private static final String BRIDGE_VERSION_ARGUMENT = "bridgeVersion";
    private static final long BRIDGE_READY_TIMEOUT_MS = 15_000;
    private static final long JAVASCRIPT_TIMEOUT_MS = 2_000;
    private static final long POLL_INTERVAL_MS = 100;

    /**
     * Deliberate runtime projection of KorriNativeBridgeSurface from
     * contracts/bridge/korri-native-bridge.ts. TypeScript interfaces are erased,
     * so this list is the test-owned guard that the hand-written Android bridge
     * still exposes every treaty member after minification. Spike-era bridge
     * extras may also exist; this test makes no claim about their absence.
     */
    private static final String[] CANONICAL_KORRI_NATIVE_METHODS = {
            "launchLocal",
            "queryStreamHosts",
            "queryStreamApps",
            "startStream",
            "korridPort",
            "korridCapability",
            "storageAccess",
            "openStorageAccessSettings",
            "backgroundNotice",
            "requestBackgroundNotice",
            "openNotificationSettings",
            "ownerBindingSnapshot",
            "startOwnerBinding",
            "systemInfo",
            "bridgeVersion"
    };

    @Test
    public void bridgeVersionMatchesTheCanonicalTreaty() throws Exception {
        int expectedBridgeVersion = expectedBridgeVersion();

        withReadyBridge((scenario, webView) -> assertEquals(String.valueOf(expectedBridgeVersion),
                evaluateJavascript(webView, "window.KorriNative.bridgeVersion()")));
    }

    @Test
    public void canonicalKorriNativeBridgeMembersAreExposed() throws Exception {
        withReadyBridge((scenario, webView) -> {
            JSONArray missing = new JSONArray(parseJavaScriptString(evaluateJavascript(webView,
                    "JSON.stringify([" + quotedMethodNames() + "]"
                            + ".filter(function(name) {"
                            + " return typeof window.KorriNative[name] !== 'function';"
                            + "}))")));

            assertEquals("Every KorriNativeBridgeSurface member should be exposed", 0,
                    missing.length());
        });
    }

    @Test
    public void safeReadOnlyBridgeMethodsReturnTreatyShapes() throws Exception {
        withReadyBridge((scenario, webView) -> {
            int port = Integer.parseInt(evaluateJavascript(webView,
                    "window.KorriNative.korridPort()"));
            assertTrue("korridPort should expose a running embedded korrid port", port > 0);

            String capability = parseJavaScriptString(evaluateJavascript(webView,
                    "window.KorriNative.korridCapability()"));
            assertFalse("korridCapability should be non-empty", capability.isEmpty());

            JSONObject storage = bridgeJson(webView, "storageAccess");
            assertTagIn(storage, "Granted", "NotRequired", "Denied", "QueryFailed");
            if ("QueryFailed".equals(storage.getString("_tag"))) {
                assertStringField(storage, "message");
            }

            JSONObject notice = bridgeJson(webView, "backgroundNotice");
            assertTagIn(notice, "Visible", "Hidden");

            JSONObject systemInfo = bridgeJson(webView, "systemInfo");
            assertTagIn(systemInfo, "SystemInfo", "Unavailable");
            if ("SystemInfo".equals(systemInfo.getString("_tag"))) {
                JSONObject payload = systemInfo.getJSONObject("payload");
                assertStringField(payload, "device");
                assertStringField(payload, "manufacturer");
                assertStringField(payload, "androidRelease");
                assertTrue("systemInfo.sdk should be positive", payload.getInt("sdk") > 0);
                assertStringField(payload, "appVersion");
            } else {
                assertStringField(systemInfo, "message");
            }
        });
    }

    @Test
    public void configurableNip55SignerCreatesAVerifiedOwnerBinding() throws Exception {
        withReadyBridge((scenario, webView) -> {
            JSONObject initial = bridgeJson(webView, "ownerBindingSnapshot");
            assertEquals("Unowned", initial.getJSONObject("identity").getString("_tag"));
            assertStringField(initial, "deviceFingerprint");
            assertStringField(initial, "requestedAction");
            assertStringField(initial, "bindingUri");

            invokeJavascriptNoResult(webView, "window.KorriNative.startOwnerBinding()");

            // The configurable signer answers immediately. Do not poll the
            // paused WebView while Android owns the signer screen.
            Thread.sleep(3_000L);
            JSONObject current = bridgeJson(webView, "ownerBindingSnapshot");
            assertEquals(current.toString(), "Owned",
                    current.getJSONObject("identity").getString("_tag"));
            assertEquals("Approved",
                    current.getJSONObject("personSigner").getString("_tag"));
        });
    }

    @Test
    public void activityKeyEventsReachTheWebViewAsSemanticInput() throws Exception {
        withReadyBridge((scenario, webView) -> {
            installSemanticInputReceiver(webView);
            try {
                assertSemanticEvent(scenario, webView, KeyEvent.KEYCODE_DPAD_UP,
                        "direction", "direction", "up");
                assertSemanticEvent(scenario, webView, KeyEvent.KEYCODE_DPAD_DOWN,
                        "direction", "direction", "down");
                assertSemanticEvent(scenario, webView, KeyEvent.KEYCODE_DPAD_LEFT,
                        "direction", "direction", "left");
                assertSemanticEvent(scenario, webView, KeyEvent.KEYCODE_DPAD_RIGHT,
                        "direction", "direction", "right");
                assertSemanticEvent(scenario, webView, KeyEvent.KEYCODE_DPAD_CENTER,
                        "confirm", null, null);
                assertSemanticEvent(scenario, webView, KeyEvent.KEYCODE_BUTTON_A,
                        "confirm", null, null);
                assertSemanticEvent(scenario, webView, KeyEvent.KEYCODE_BACK,
                        "back", null, null);
                assertSemanticEvent(scenario, webView, KeyEvent.KEYCODE_BUTTON_B,
                        "back", null, null);
                assertSemanticEvent(scenario, webView, KeyEvent.KEYCODE_MENU,
                        "menu", null, null);
                assertSemanticEvent(scenario, webView, KeyEvent.KEYCODE_BUTTON_START,
                        "menu", null, null);
                assertSemanticEvent(scenario, webView, KeyEvent.KEYCODE_BUTTON_SELECT,
                        "options", null, null);
            } finally {
                restoreSemanticInputReceiver(webView);
            }
        });
    }

    @Test
    public void semanticInputIgnoresKeyUpAndUnmappedKeys() throws Exception {
        withReadyBridge((scenario, webView) -> {
            installSemanticInputReceiver(webView);
            try {
                dispatchKeyEvent(scenario, KeyEvent.ACTION_DOWN, KeyEvent.KEYCODE_DPAD_UP);
                dispatchKeyEvent(scenario, KeyEvent.ACTION_UP, KeyEvent.KEYCODE_DPAD_UP);
                JSONObject event = waitForSemanticEvent(webView);
                assertEquals("direction", event.getString("type"));
                assertEquals("up", event.getString("direction"));
                assertEquals("gamepad", event.getString("source"));
                assertNoAdditionalSemanticEvents(webView);

                dispatchKeyEvent(scenario, KeyEvent.ACTION_DOWN, KeyEvent.KEYCODE_VOLUME_UP);
                assertNoAdditionalSemanticEvents(webView);
            } finally {
                restoreSemanticInputReceiver(webView);
            }
        });
    }

    private static void withReadyBridge(ReadyBridgeAssertion assertion) throws Exception {
        try (ActivityScenario<KorriShellActivity> scenario =
                     ActivityScenario.launch(KorriShellActivity.class)) {
            AtomicReference<WebView> webView = new AtomicReference<>();
            scenario.onActivity(activity -> {
                View content = activity.findViewById(android.R.id.content);
                webView.set(findWebView(content));
            });

            assertNotNull("KorriShellActivity should publish its WebView in the view hierarchy",
                    webView.get());

            waitForKorriNative(webView.get());
            assertion.run(scenario, webView.get());
        }
    }

    private interface ReadyBridgeAssertion {
        void run(ActivityScenario<KorriShellActivity> scenario, WebView webView) throws Exception;
    }

    private static int expectedBridgeVersion() {
        String value = InstrumentationRegistry.getArguments()
                .getString(BRIDGE_VERSION_ARGUMENT);
        assertNotNull("Missing instrumentation argument '" + BRIDGE_VERSION_ARGUMENT
                        + "'. Supply the canonical BRIDGE_VERSION from "
                        + "clients/android/test/bridge-contract-version.ts.",
                value);
        try {
            return Integer.parseInt(value);
        } catch (NumberFormatException error) {
            fail("Instrumentation argument '" + BRIDGE_VERSION_ARGUMENT
                    + "' must be an integer, got: " + value);
            throw error;
        }
    }

    private static void waitForKorriNative(WebView webView) throws Exception {
        long deadline = SystemClock.elapsedRealtime() + BRIDGE_READY_TIMEOUT_MS;
        String lastResult = null;
        while (SystemClock.elapsedRealtime() < deadline) {
            lastResult = tryEvaluateJavascript(webView,
                    "typeof window.KorriNative === 'object' "
                            + "&& typeof window.KorriNative.bridgeVersion === 'function'");
            if ("true".equals(lastResult)) {
                return;
            }
            Thread.sleep(POLL_INTERVAL_MS);
        }
        fail("window.KorriNative.bridgeVersion was not exposed within "
                + BRIDGE_READY_TIMEOUT_MS + "ms; last readiness result was " + lastResult);
    }

    private static void invokeJavascriptNoResult(WebView webView, String script) throws Exception {
        CountDownLatch posted = new CountDownLatch(1);
        webView.post(() -> {
            webView.evaluateJavascript(script, null);
            posted.countDown();
        });
        assertTrue("JavaScript invocation should reach the WebView",
                posted.await(JAVASCRIPT_TIMEOUT_MS, TimeUnit.MILLISECONDS));
    }

    private static JSONObject bridgeJson(WebView webView, String methodName) throws Exception {
        return new JSONObject(parseJavaScriptString(evaluateJavascript(webView,
                "window.KorriNative." + methodName + "()")));
    }

    private static void assertTagIn(JSONObject value, String... allowedTags) throws Exception {
        String tag = value.getString("_tag");
        for (String allowed : allowedTags) {
            if (allowed.equals(tag)) {
                return;
            }
        }
        fail("Unexpected bridge result tag for " + value.names() + ": " + tag);
    }

    private static void assertStringField(JSONObject value, String field) throws Exception {
        assertTrue("Expected string field: " + field, value.has(field));
        assertFalse("Expected non-empty string field: " + field,
                value.getString(field).isEmpty());
    }

    private static void assertSemanticEvent(
            ActivityScenario<KorriShellActivity> scenario,
            WebView webView,
            int keyCode,
            String expectedType,
            String optionalField,
            String optionalValue) throws Exception {
        dispatchKeyEvent(scenario, KeyEvent.ACTION_DOWN, keyCode);
        JSONObject event = waitForSemanticEvent(webView);
        assertEquals(expectedType, event.getString("type"));
        assertEquals("gamepad", event.getString("source"));
        if (optionalField != null) {
            assertEquals(optionalValue, event.getString(optionalField));
        }
        assertNoAdditionalSemanticEvents(webView);
    }

    private static void installSemanticInputReceiver(WebView webView) throws Exception {
        waitForPortalSemanticInputReceiver(webView);
        assertEquals("true", evaluateJavascript(webView,
                "(function() {"
                        + "window.__korriInputOriginalDescriptorForTest = "
                        + "  Object.getOwnPropertyDescriptor(window, '__korriInput');"
                        + "window.__korriInputOriginalValueForTest = window.__korriInput;"
                        + "window.__korriInputEventsForTest = [];"
                        + "window.__korriInputReceiverForTest = function(json) {"
                        + "  window.__korriInputEventsForTest.push(JSON.parse(json));"
                        + "};"
                        + "Object.defineProperty(window, '__korriInput', {"
                        + "  configurable: true,"
                        + "  get: function() { return window.__korriInputReceiverForTest; },"
                        + "  set: function(value) { window.__korriInputOriginalValueForTest = value; }"
                        + "});"
                        + "return window.__korriInput === window.__korriInputReceiverForTest;"
                        + "})()"));
    }

    private static void waitForPortalSemanticInputReceiver(WebView webView) throws Exception {
        long deadline = SystemClock.elapsedRealtime() + BRIDGE_READY_TIMEOUT_MS;
        String lastResult = null;
        while (SystemClock.elapsedRealtime() < deadline) {
            lastResult = tryEvaluateJavascript(webView,
                    "typeof window.__korriInput === 'function'");
            if ("true".equals(lastResult)) {
                return;
            }
            Thread.sleep(POLL_INTERVAL_MS);
        }
        fail("window.__korriInput was not registered within "
                + BRIDGE_READY_TIMEOUT_MS + "ms; last readiness result was " + lastResult);
    }

    private static void restoreSemanticInputReceiver(WebView webView) throws Exception {
        assertEquals("true", evaluateJavascript(webView,
                "(function() {"
                        + "var descriptor = window.__korriInputOriginalDescriptorForTest;"
                        + "var value = window.__korriInputOriginalValueForTest;"
                        + "delete window.__korriInput;"
                        + "if (descriptor) {"
                        + "  Object.defineProperty(window, '__korriInput', descriptor);"
                        + "} else if (typeof value === 'undefined') {"
                        + "  delete window.__korriInput;"
                        + "} else {"
                        + "  window.__korriInput = value;"
                        + "}"
                        + "delete window.__korriInputOriginalDescriptorForTest;"
                        + "delete window.__korriInputOriginalValueForTest;"
                        + "delete window.__korriInputReceiverForTest;"
                        + "delete window.__korriInputEventsForTest;"
                        + "return true;"
                        + "})()"));
    }

    private static void dispatchKeyEvent(
            ActivityScenario<KorriShellActivity> scenario,
            int action,
            int keyCode) {
        scenario.onActivity(activity -> activity.dispatchKeyEvent(new KeyEvent(action, keyCode)));
    }

    private static JSONObject waitForSemanticEvent(WebView webView) throws Exception {
        long deadline = SystemClock.elapsedRealtime() + JAVASCRIPT_TIMEOUT_MS;
        while (SystemClock.elapsedRealtime() < deadline) {
            int count = Integer.parseInt(evaluateJavascript(webView,
                    "window.__korriInputEventsForTest.length"));
            if (count > 0) {
                return new JSONObject(parseJavaScriptString(evaluateJavascript(webView,
                        "JSON.stringify(window.__korriInputEventsForTest.shift())")));
            }
            Thread.sleep(POLL_INTERVAL_MS);
        }
        fail("Timed out waiting for semantic input event");
        throw new AssertionError("unreachable");
    }

    private static void assertNoAdditionalSemanticEvents(WebView webView) throws Exception {
        // evaluateJavascript calls are ordered on this WebView. This no-op callback
        // is therefore a deterministic barrier behind the key dispatch's script.
        evaluateJavascript(webView, "void 0");
        assertEquals("0", evaluateJavascript(webView,
                "window.__korriInputEventsForTest.length"));
    }

    private static String evaluateJavascript(WebView webView, String script) throws Exception {
        String result = tryEvaluateJavascript(webView, script);
        assertNotNull("Timed out waiting for JavaScript result: " + script, result);
        return result;
    }

    private static String tryEvaluateJavascript(WebView webView, String script) throws Exception {
        CountDownLatch callback = new CountDownLatch(1);
        AtomicReference<String> result = new AtomicReference<>();

        InstrumentationRegistry.getInstrumentation().runOnMainSync(
                () -> webView.evaluateJavascript(script, value -> {
                    result.set(value);
                    callback.countDown();
                }));

        if (!callback.await(JAVASCRIPT_TIMEOUT_MS, TimeUnit.MILLISECONDS)) {
            return null;
        }
        return result.get();
    }

    private static String parseJavaScriptString(String value) throws Exception {
        return new JSONArray("[" + value + "]").getString(0);
    }

    private static String quotedMethodNames() {
        StringBuilder names = new StringBuilder();
        for (int index = 0; index < CANONICAL_KORRI_NATIVE_METHODS.length; index++) {
            if (index > 0) {
                names.append(',');
            }
            names.append(JSONObject.quote(CANONICAL_KORRI_NATIVE_METHODS[index]));
        }
        return names.toString();
    }

    private static WebView findWebView(View view) {
        if (view instanceof WebView) {
            return (WebView) view;
        }
        if (view instanceof ViewGroup) {
            ViewGroup group = (ViewGroup) view;
            for (int index = 0; index < group.getChildCount(); index++) {
                WebView child = findWebView(group.getChildAt(index));
                if (child != null) {
                    return child;
                }
            }
        }
        return null;
    }
}
