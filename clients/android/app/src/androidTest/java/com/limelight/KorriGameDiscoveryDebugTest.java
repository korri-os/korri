package com.limelight;

import android.os.SystemClock;
import android.view.View;
import android.view.ViewGroup;
import android.webkit.WebView;

import androidx.test.core.app.ActivityScenario;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.platform.app.InstrumentationRegistry;

import com.simonwjackson.korri.korrid.KorridServer;

import org.json.JSONArray;
import org.json.JSONObject;
import org.junit.Test;
import org.junit.runner.RunWith;

import java.io.BufferedReader;
import java.io.File;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.io.OutputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertNotNull;
import static org.junit.Assert.assertTrue;
import static org.junit.Assert.fail;

@RunWith(AndroidJUnit4.class)
public class KorriGameDiscoveryDebugTest {
    private static final String ACTION_ARGUMENT = "korriDebugDiscoveryAction";
    private static final String FOLDER_PATH_ARGUMENT = "gameFolderPath";
    private static final long BRIDGE_READY_TIMEOUT_MS = 15_000;
    private static final long DISCOVERY_SETTLE_TIMEOUT_MS = 60_000;
    private static final long JAVASCRIPT_TIMEOUT_MS = 5_000;
    private static final long POLL_INTERVAL_MS = 100;

    @Test
    public void runRequestedDebugDiscoveryAction() throws Exception {
        String action = requiredArgument(ACTION_ARGUMENT);
        try (ActivityScenario<KorriShellActivity> scenario =
                     ActivityScenario.launch(KorriShellActivity.class)) {
            AtomicReference<WebView> webView = new AtomicReference<>();
            scenario.onActivity(activity -> webView.set(findWebView(
                    activity.findViewById(android.R.id.content))));
            assertNotNull("KorriShellActivity should publish its WebView", webView.get());
            waitForKorriNative(webView.get());

            if ("register".equals(action)) {
                registerFolderThroughReceipt(webView.get(), requiredArgument(FOLDER_PATH_ARGUMENT));
                return;
            }
            fail("Unsupported " + ACTION_ARGUMENT + ": " + action);
        }
    }

    private static void registerFolderThroughReceipt(WebView webView, String folderPath)
            throws Exception {
        String receipt = KorridServer.issueFolderSelectionReceipt(folderPath);
        assertNotNull("receipt should be issued for an Android-approved folder", receipt);
        assertTrue("receipt should not be empty", receipt.length() > 0);

        int port = Integer.parseInt(evaluateJavascript(webView, "window.KorriRpc.korridPort()"));
        String capability = new JSONObject("{\"value\":"
                + evaluateJavascript(webView, "window.KorriRpc.korridCapability()")
                + "}").getString("value");
        JSONObject request = new JSONObject();
        request.put("_tag", "app.discovery.registerReceipt");
        request.put("payload", new JSONObject().put("receipt", receipt));
        String response = postRpc(port, capability, request.toString());
        JSONObject envelope = new JSONObject(response);
        assertEquals("app.discovery.registerReceipt", envelope.getString("_tag"));
        assertEquals("Ok", envelope.getJSONObject("outcome").getString("_tag"));

        JSONObject snapshot = waitForDiscoveryTerminal(port, capability);
        String state = stateTag(snapshot);
        assertEquals("Discovery should finish Idle before instrumentation returns; generation="
                + snapshot.getString("generation") + " snapshot=" + snapshot,
                "Idle", state);
        assertRegisteredLocation(snapshot, folderPath);
        assertLocalGameFromFolder(port, capability, folderPath);
    }

    private static JSONObject waitForDiscoveryTerminal(int port, String capability)
            throws Exception {
        long deadline = SystemClock.elapsedRealtime() + DISCOVERY_SETTLE_TIMEOUT_MS;
        JSONObject lastSnapshot = null;
        while (SystemClock.elapsedRealtime() < deadline) {
            lastSnapshot = discoverySnapshot(port, capability);
            String state = stateTag(lastSnapshot);
            if ("Idle".equals(state) || "Problem".equals(state)) {
                return lastSnapshot;
            }
            Thread.sleep(POLL_INTERVAL_MS);
        }
        String lastGeneration = lastSnapshot == null ? "unknown" : lastSnapshot.optString("generation");
        String lastState = lastSnapshot == null ? "unknown" : stateTag(lastSnapshot);
        fail("Discovery did not settle within " + DISCOVERY_SETTLE_TIMEOUT_MS
                + "ms after registerReceipt; last generation=" + lastGeneration
                + " state=" + lastState + " snapshot=" + lastSnapshot);
        throw new AssertionError("unreachable");
    }

    private static JSONObject discoverySnapshot(int port, String capability) throws Exception {
        JSONObject request = new JSONObject();
        request.put("_tag", "app.discovery.snapshot");
        request.put("payload", new JSONObject());
        JSONObject envelope = new JSONObject(postRpc(port, capability, request.toString()));
        assertEquals("app.discovery.snapshot", envelope.getString("_tag"));
        assertEquals("Ok", envelope.getJSONObject("outcome").getString("_tag"));
        return envelope.getJSONObject("outcome").getJSONObject("payload");
    }

    private static String stateTag(JSONObject snapshot) throws Exception {
        return snapshot.getJSONObject("state").getString("_tag");
    }

    private static void assertRegisteredLocation(JSONObject snapshot, String folderPath)
            throws Exception {
        String expectedPath = new File(folderPath).getCanonicalPath();
        JSONArray locations = snapshot.getJSONArray("locations");
        for (int index = 0; index < locations.length(); index++) {
            String label = locations.getJSONObject(index).getString("label");
            if (folderPath.equals(label) || expectedPath.equals(new File(label).getCanonicalPath())) {
                return;
            }
        }
        fail("Discovery Idle snapshot generation=" + snapshot.getString("generation")
                + " did not include registered folder " + expectedPath
                + " in locations=" + locations);
    }

    private static void assertLocalGameFromFolder(int port, String capability, String folderPath)
            throws Exception {
        List<String> expectedTitles = fixtureGameTitles(folderPath);
        assertTrue("Selected fixture folder should contain at least one .gba game: " + folderPath,
                expectedTitles.size() > 0);

        JSONObject request = new JSONObject();
        request.put("_tag", "app.local-games.list");
        request.put("payload", new JSONObject());
        JSONObject envelope = new JSONObject(postRpc(port, capability, request.toString()));
        assertEquals("app.local-games.list", envelope.getString("_tag"));
        assertEquals("Ok", envelope.getJSONObject("outcome").getString("_tag"));
        JSONArray games = envelope.getJSONObject("outcome").getJSONObject("payload")
                .getJSONArray("games");
        for (int gameIndex = 0; gameIndex < games.length(); gameIndex++) {
            String title = games.getJSONObject(gameIndex).getString("title");
            if (expectedTitles.contains(title)) {
                return;
            }
        }
        fail("No local game from selected fixture folder " + folderPath
                + " was visible through app.local-games.list; expected titles=" + expectedTitles
                + " response=" + envelope);
    }

    private static List<String> fixtureGameTitles(String folderPath) {
        List<String> titles = new ArrayList<>();
        File[] files = new File(folderPath).listFiles();
        if (files == null) {
            return titles;
        }
        for (File file : files) {
            String name = file.getName();
            if (file.isFile() && name.toLowerCase(Locale.ROOT).endsWith(".gba")) {
                titles.add(name.substring(0, name.length() - ".gba".length()));
            }
        }
        return titles;
    }

    private static String postRpc(int port, String capability, String body) throws Exception {
        HttpURLConnection connection = (HttpURLConnection)
                new URL("http://127.0.0.1:" + port + "/rpc").openConnection();
        connection.setRequestMethod("POST");
        connection.setConnectTimeout(2_000);
        connection.setReadTimeout(10_000);
        connection.setDoOutput(true);
        connection.setRequestProperty("content-type", "application/json");
        connection.setRequestProperty("authorization", "Bearer " + capability);
        byte[] bytes = body.getBytes(StandardCharsets.UTF_8);
        connection.setFixedLengthStreamingMode(bytes.length);
        try (OutputStream output = connection.getOutputStream()) {
            output.write(bytes);
        }
        int code = connection.getResponseCode();
        InputStream stream = code >= 200 && code < 300
                ? connection.getInputStream()
                : connection.getErrorStream();
        String response = readFully(stream);
        assertEquals("RPC HTTP status for " + body + " response=" + response, 200, code);
        return response;
    }

    private static String readFully(InputStream stream) throws Exception {
        StringBuilder builder = new StringBuilder();
        try (BufferedReader reader = new BufferedReader(
                new InputStreamReader(stream, StandardCharsets.UTF_8))) {
            String line;
            while ((line = reader.readLine()) != null) {
                builder.append(line);
            }
        }
        return builder.toString();
    }

    private static String requiredArgument(String name) {
        String value = InstrumentationRegistry.getArguments().getString(name);
        assertNotNull("Missing instrumentation argument '" + name + "'", value);
        assertTrue("Instrumentation argument '" + name + "' must not be empty",
                value.length() > 0);
        return value;
    }

    private static void waitForKorriNative(WebView webView) throws Exception {
        long deadline = SystemClock.elapsedRealtime() + BRIDGE_READY_TIMEOUT_MS;
        String lastResult = null;
        while (SystemClock.elapsedRealtime() < deadline) {
            lastResult = evaluateJavascript(webView,
                    "typeof window.KorriNative === 'object' "
                            + "&& typeof window.KorriRpc === 'object' "
                            + "&& typeof window.KorriRpc.korridPort === 'function' "
                            + "&& typeof window.KorriRpc.korridCapability === 'function' "
                            + "&& typeof window.KorriNative.launchLocal === 'function'");
            if ("true".equals(lastResult)) {
                return;
            }
            Thread.sleep(POLL_INTERVAL_MS);
        }
        fail("window.KorriNative debug discovery bridge was not ready within "
                + BRIDGE_READY_TIMEOUT_MS + "ms; last readiness result was " + lastResult);
    }

    private static String evaluateJavascript(WebView webView, String script) throws Exception {
        return evaluateJavascript(webView, script, script);
    }

    private static String evaluateJavascript(WebView webView, String script, String description)
            throws Exception {
        CountDownLatch callback = new CountDownLatch(1);
        AtomicReference<String> result = new AtomicReference<>();

        InstrumentationRegistry.getInstrumentation().runOnMainSync(
                () -> webView.evaluateJavascript(script, value -> {
                    result.set(value);
                    callback.countDown();
                }));

        assertTrue("Timed out waiting for JavaScript result: " + description,
                callback.await(JAVASCRIPT_TIMEOUT_MS, TimeUnit.MILLISECONDS));
        return result.get();
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
