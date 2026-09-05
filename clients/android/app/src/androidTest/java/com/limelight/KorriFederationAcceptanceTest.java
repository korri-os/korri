package com.limelight;

import android.content.Context;
import android.content.Intent;
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

import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.InputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertNotEquals;
import static org.junit.Assert.assertNotNull;
import static org.junit.Assert.assertTrue;
import static org.junit.Assert.fail;

/** One ordered flow, real JNI brain, NIP-55 signer, and encrypted native peer. */
@RunWith(AndroidJUnit4.class)
public class KorriFederationAcceptanceTest {
    private static final String LABEL = "federation-acceptance";
    private int port;
    private String capability;

    @Test
    public void ownedAndroidControlsExactPeerSessionAndRecordsOnePlay() throws Exception {
        String peerKey = argument("federationDeviceKey");
        int peerPort = Integer.parseInt(argument("federationPort"));
        Context context = InstrumentationRegistry.getInstrumentation().getTargetContext();
        File peers = new File("/storage/emulated/0/korri/upstreams.json");
        assertTrue(peers.getParentFile().isDirectory() || peers.getParentFile().mkdirs());
        assertFalse("Fresh AVD must not contain peer configuration", peers.exists());
        // UpstreamHostConfig in services/korrid/src/upstreams.rs is the producer treaty.
        JSONObject peer = new JSONObject().put("label", LABEL).put("kind", "native")
                .put("baseUrl", "http://127.0.0.1:" + peerPort).put("devicePublicKey", peerKey)
                .put("moonlightAddress", "127.0.0.1:9");
        Files.write(peers.toPath(), new JSONArray().put(peer).toString().getBytes(StandardCharsets.UTF_8));
        try {
            try (ActivityScenario<KorriShellActivity> scenario = ActivityScenario.launch(KorriShellActivity.class)) {
                WebView web = readyWebView(scenario);
                assertEquals("Unowned", bridge(web, "ownerBindingSnapshot").getJSONObject("identity").getString("_tag"));
                // Invoke asynchronously: the signer temporarily pauses the WebView.
                InstrumentationRegistry.getInstrumentation().runOnMainSync(() ->
                        web.evaluateJavascript("window.KorriNative.startOwnerBinding()", null));
                SystemClock.sleep(3_000);
                long deadline = SystemClock.elapsedRealtime() + 15_000;
                JSONObject binding;
                do {
                    binding = bridge(web, "ownerBindingSnapshot");
                    if ("Owned".equals(binding.getJSONObject("identity").getString("_tag"))) break;
                    SystemClock.sleep(100);
                } while (SystemClock.elapsedRealtime() < deadline);
                assertEquals(binding.toString(), "Owned", binding.getJSONObject("identity").getString("_tag"));
                assertEquals("Approved", binding.getJSONObject("personSigner").getString("_tag"));
                assertEquals("f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9",
                        binding.getJSONObject("identity").getString("ownerPublicKey"));
                assertNotEquals(peerKey, binding.getJSONObject("identity").getString("devicePublicKey"));
                readAuthority(web);
            }
            // Bundle A peer credentials are startup snapshots. Restart the actual service,
            // not an alternate test server, then read the new bridge authority.
            String oldCapability = capability;
            stopBrain(context);
            try (ActivityScenario<KorriShellActivity> scenario = ActivityScenario.launch(KorriShellActivity.class)) {
                readAuthority(readyWebView(scenario));
                assertNotEquals("Restart must rotate local authority", oldCapability, capability);
                assertPlaintextRejected(peerPort);
                JSONObject game = onlyGame();
                JSONObject source = game.getJSONObject("source");
                assertEquals(peerKey, source.getString("devicePublicKey"));
                assertEquals(LABEL, source.getString("label"));
                assertFalse(source.getBoolean("isLocal"));
                assertEquals(0, game.getJSONObject("playStats").getInt("playCount"));
                JSONObject readiness = ok("app.source.status", new JSONObject().put("devicePublicKey", peerKey));
                assertEquals("available", readiness.getString("catalog"));
                // No certificate broker or video is required to prepare a session.
                assertEquals("disabled", readiness.getString("streamControl"));
                JSONObject prepared = ok("app.session.prepare", new JSONObject().put("gameId", game.getString("id"))
                        .put("host", game.getString("host")));
                String launchId = prepared.getString("launchId");
                assertFalse(launchId.isEmpty());
                assertActive(launchId, "running");
                freezer("freeze", launchId, "frozen", true);
                freezer("freeze", launchId, "frozen", false);
                assertActive(launchId, "frozen");
                freezer("thaw", launchId, "running", true);
                freezer("thaw", launchId, "running", false);
                assertActive(launchId, "running");
                JSONObject stale = call("app.session.stop", new JSONObject().put("expectedLaunchId", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
                assertEquals(stale.toString(), "Err", stale.getString("_tag"));
                assertEquals("StaleLaunchIdentity", stale.getJSONObject("payload").getString("code"));
                assertActive(launchId, "running");
                SystemClock.sleep(1_100);
                assertEquals("stopped", ok("app.session.stop", new JSONObject().put("expectedLaunchId", launchId)).getString("phase"));
                for (int observation = 0; observation < 3; observation++) {
                    assertEquals("stopped", ok("app.session.stop", new JSONObject().put("expectedLaunchId", launchId)).getString("phase"));
                    assertFalse(ok("app.session.status", new JSONObject()).has("active"));
                    JSONObject stats = onlyGame().getJSONObject("playStats");
                    assertEquals(1, stats.getInt("playCount"));
                    assertTrue(stats.toString(), stats.getDouble("totalPlaytimeSeconds") > 0);
                    assertFalse(stats.getString("lastPlayed").isEmpty());
                }
            }
        } finally {
            stopBrain(context);
            Files.deleteIfExists(peers.toPath());
        }
    }

    private JSONObject onlyGame() throws Exception {
        JSONObject catalog = ok("app.catalog.snapshot", new JSONObject());
        assertTrue(catalog.toString(), !catalog.has("failures") || catalog.getJSONArray("failures").length() == 0);
        JSONArray games = catalog.getJSONArray("games");
        assertEquals(catalog.toString(), 1, games.length());
        return games.getJSONObject(0);
    }

    private void assertActive(String launchId, String phase) throws Exception {
        JSONObject active = ok("app.session.status", new JSONObject()).getJSONObject("active");
        assertEquals(launchId, active.getString("launchId"));
        assertEquals(phase, active.getString("phase"));
    }

    private void freezer(String verb, String launchId, String state, boolean changed) throws Exception {
        JSONObject result = ok("app.session." + verb, new JSONObject().put("expectedLaunchId", launchId));
        assertEquals(launchId, result.getString("launchId"));
        assertEquals(state, result.getString("state"));
        assertEquals(changed, result.getBoolean("changed"));
    }

    private JSONObject ok(String tag, JSONObject payload) throws Exception {
        JSONObject outcome = call(tag, payload);
        assertEquals(tag + ": " + outcome, "Ok", outcome.getString("_tag"));
        return outcome.getJSONObject("payload");
    }

    private JSONObject call(String tag, JSONObject payload) throws Exception {
        HttpURLConnection connection = connection(port, "/rpc");
        try {
            connection.setRequestProperty("Authorization", "Bearer " + capability);
            connection.getOutputStream().write(new JSONObject().put("_tag", tag).put("payload", payload)
                    .toString().getBytes(StandardCharsets.UTF_8));
            assertEquals(tag, 200, connection.getResponseCode());
            JSONObject response = new JSONObject(readBounded(connection.getInputStream()));
            assertEquals(tag, response.getString("_tag"));
            return response.getJSONObject("outcome");
        } finally {
            connection.disconnect();
        }
    }

    private static void assertPlaintextRejected(int peerPort) throws Exception {
        HttpURLConnection connection = connection(peerPort, "/peer-rpc");
        try {
            connection.getOutputStream().write("{\"_tag\":\"app.catalog.snapshot\",\"payload\":{}}".getBytes(StandardCharsets.UTF_8));
            assertEquals("Host must reject plaintext instead of bypassing encrypted peer RPC", 400, connection.getResponseCode());
        } finally {
            connection.disconnect();
        }
    }

    private static HttpURLConnection connection(int port, String path) throws Exception {
        HttpURLConnection connection = (HttpURLConnection) new URL("http://127.0.0.1:" + port + path).openConnection();
        connection.setConnectTimeout(3_000);
        connection.setReadTimeout(15_000);
        connection.setRequestMethod("POST");
        connection.setRequestProperty("Content-Type", "application/json");
        connection.setDoOutput(true);
        return connection;
    }

    private static String readBounded(InputStream input) throws Exception {
        try (InputStream stream = input; ByteArrayOutputStream output = new ByteArrayOutputStream()) {
            byte[] buffer = new byte[4096];
            int count;
            while ((count = stream.read(buffer)) != -1) {
                assertTrue("RPC response exceeds 1 MiB", output.size() + count <= 1024 * 1024);
                output.write(buffer, 0, count);
            }
            return output.toString("UTF-8");
        }
    }

    private void readAuthority(WebView web) throws Exception {
        port = Integer.parseInt(js(web, "window.KorriNative.korridPort()"));
        capability = new JSONArray("[" + js(web, "window.KorriNative.korridCapability()") + "]").getString(0);
        assertTrue(port > 0);
        assertFalse(capability.isEmpty());
    }

    private static void stopBrain(Context context) throws Exception {
        context.stopService(new Intent().setClassName(context,
                "com.simonwjackson.korri.korrid.KorriBrainService"));
        long deadline = SystemClock.elapsedRealtime() + 10_000;
        while (SystemClock.elapsedRealtime() < deadline) {
            try {
                KorridServer.capability();
            } catch (IllegalStateException stopped) {
                assertEquals("korrid is not running", stopped.getMessage());
                return;
            }
            SystemClock.sleep(100);
        }
        fail("Embedded brain did not stop within 10 seconds");
    }

    private static WebView readyWebView(ActivityScenario<KorriShellActivity> scenario) throws Exception {
        AtomicReference<WebView> web = new AtomicReference<>();
        scenario.onActivity(activity -> web.set(findWebView(activity.findViewById(android.R.id.content))));
        assertNotNull(web.get());
        long deadline = SystemClock.elapsedRealtime() + 15_000;
        do {
            if ("true".equals(js(web.get(), "typeof window.KorriNative === 'object'"))) return web.get();
            SystemClock.sleep(100);
        } while (SystemClock.elapsedRealtime() < deadline);
        throw new AssertionError("Native bridge not ready");
    }

    private static JSONObject bridge(WebView web, String method) throws Exception {
        return new JSONObject(new JSONArray("[" + js(web, "window.KorriNative." + method + "()") + "]").getString(0));
    }

    private static String js(WebView web, String script) throws Exception {
        CountDownLatch latch = new CountDownLatch(1);
        AtomicReference<String> result = new AtomicReference<>();
        InstrumentationRegistry.getInstrumentation().runOnMainSync(() -> web.evaluateJavascript(script, value -> {
            result.set(value);
            latch.countDown();
        }));
        assertTrue("JavaScript timed out: " + script, latch.await(2, TimeUnit.SECONDS));
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

    private static String argument(String name) {
        String value = InstrumentationRegistry.getArguments().getString(name);
        assertNotNull("Missing instrumentation argument " + name, value);
        return value;
    }
}
