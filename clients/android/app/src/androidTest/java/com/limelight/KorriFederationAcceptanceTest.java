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
    private int controlPort;
    private String controlToken;

    @Test
    public void relayDiscoveryRememberedPeersAndExactSession() throws Exception {
        String peerKey = argument("federationDeviceKey");
        int peerPort = Integer.parseInt(argument("federationPort"));
        controlPort = Integer.parseInt(argument("federationControlPort"));
        controlToken = argument("federationControlToken");
        int relayPort = Integer.parseInt(argument("federationRelayPort"));
        int unavailablePort = Integer.parseInt(argument("federationUnavailablePort"));
        Context context = InstrumentationRegistry.getInstrumentation().getTargetContext();
        File peers = new File("/storage/emulated/0/korri/upstreams.json");
        assertTrue(peers.getParentFile().isDirectory() || peers.getParentFile().mkdirs());
        assertFalse("Fresh AVD must not contain peer configuration", peers.exists());
        File config = new File(peers.getParentFile(), "device.yaml");
        assertFalse("Fresh AVD must not contain settings", config.exists());
        // Existing HostPayload.relays in config/settings.rs. No static peer file.
        JSONArray relays = new JSONArray().put("ws://127.0.0.1:" + unavailablePort)
                .put("ws://127.0.0.1:" + relayPort);
        Files.write(config.toPath(), ("host:\n  relays: " + relays + "\n").getBytes(StandardCharsets.UTF_8));
        String androidKey;
        try {
            try (ActivityScenario<KorriShellActivity> scenario = ActivityScenario.launch(KorriShellActivity.class)) {
                WebView web = readyWebView(scenario);
                assertEquals("Unowned", bridge(web, "ownerBindingSnapshot").getJSONObject("identity").getString("_tag"));
                readAuthority(web);
                int originalPort = port;
                String originalCapability = capability;
                assertEquals(0, ok("app.peer.list", new JSONObject()).getJSONArray("peers").length());
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
                androidKey = binding.getJSONObject("identity").getString("devicePublicKey");
                assertNotEquals(peerKey, androidKey);
                readAuthority(web);
                assertEquals("Binding must retain the running listener", originalPort, port);
                assertEquals("Binding must retain the running capability", originalCapability, capability);
                ok("app.peer.list", new JSONObject()); // Original authority works immediately.
                JSONObject discovered = awaitPeer(peerKey);
                String initialState = discovered.getString("state");
                assertTrue(discovered.toString(), initialState.equals("loading") || initialState.equals("ready") || initialState.equals("failed"));
                // The actual portal is also a catalog consumer. It can race this
                // observation; only a native operation (not discovery) changes loading.
                android.util.Log.i("FederationAcceptance", "first roster state=" + initialState);
                awaitRelayEvidence(peerKey, androidKey);
                assertFalse("Discovery must not create a static file", peers.exists());
                assertPlaintextRejected(peerPort);
                JSONObject game = awaitGame();
                assertPeer(peerKey, "ready", 0);
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
                assertFalse(peers.exists());
                assertTrue(fixture("/relay/stop").getBoolean("relayStopped"));
            }
            String oldCapability = capability;
            stopBrain(context);
            try (ActivityScenario<KorriShellActivity> scenario = ActivityScenario.launch(KorriShellActivity.class)) {
                WebView web = readyWebView(scenario);
                readAuthority(web);
                assertNotEquals("Intentional restart rotates local authority", oldCapability, capability);
                JSONObject identity = bridge(web, "ownerBindingSnapshot").getJSONObject("identity");
                assertEquals("Owned", identity.getString("_tag"));
                assertEquals("Same private root must retain the device", androidKey, identity.getString("devicePublicKey"));
                assertFalse(peers.exists());
                assertTrue(fixture("/evidence").getBoolean("relayStopped"));
                awaitPeer(peerKey);
                JSONObject rememberedGame = awaitGame();
                assertEquals(1, rememberedGame.getJSONObject("playStats").getInt("playCount"));
                long readyTime = assertPeer(peerKey, "ready", 0);
                android.util.Log.i("FederationAcceptance", "remembered catalog ready after Android restart with relay stopped");
                fixture("/host/stop");
                // With the only host offline, the catalog contract returns Err;
                // partial-failure snapshots require at least one successful host.
                JSONObject offline = call("app.catalog.snapshot", new JSONObject());
                assertEquals(offline.toString(), "Err", offline.getString("_tag"));
                assertEquals("UpstreamUnreachable", offline.getJSONObject("payload").getString("code"));
                assertFalse(offline.getJSONObject("payload").has("games"));
                long failedTime = assertPeer(peerKey, "failed", readyTime);
                android.util.Log.i("FederationAcceptance", "host offline: generic failed state");
                JSONObject restarted = fixture("/host/start");
                assertEquals(peerPort, restarted.getInt("hostPort"));
                assertEquals(peerKey, restarted.getString("hostKey"));
                assertEquals(1, awaitGame().getJSONObject("playStats").getInt("playCount"));
                assertPeer(peerKey, "ready", failedTime);
                assertFalse(peers.exists());
                android.util.Log.i("FederationAcceptance", "same host identity/port/state recovered ready; one completed play");
            }
        } finally {
            stopBrain(context);
            Files.deleteIfExists(config.toPath());
        }
    }

    private JSONObject awaitPeer(String key) throws Exception {
        long deadline = SystemClock.elapsedRealtime() + 150_000;
        do {
            JSONArray entries = ok("app.peer.list", new JSONObject()).getJSONArray("peers");
            for (int i = 0; i < entries.length(); i++) {
                JSONObject entry = entries.getJSONObject(i);
                if (key.equals(entry.getString("devicePublicKey"))) return entry;
            }
            SystemClock.sleep(250);
        } while (SystemClock.elapsedRealtime() < deadline);
        throw new AssertionError("Host key did not enter the actual owner roster");
    }

    private long assertPeer(String key, String state, long previousTime) throws Exception {
        JSONObject entry = awaitPeer(key);
        assertEquals(entry.toString(), LABEL, entry.getString("label"));
        assertEquals(entry.toString(), state, entry.getString("state"));
        long updated = entry.getLong("updatedAt");
        assertTrue(entry.toString(), updated >= previousTime && updated > 1_700_000_000L && updated < 10_000_000_000L);
        if ("failed".equals(state)) assertEquals("Authenticated peer operation failed", entry.getString("lastError"));
        else assertFalse(entry.toString(), entry.has("lastError"));
        return updated;
    }

    private void awaitRelayEvidence(String hostKey, String androidKey) throws Exception {
        long deadline = SystemClock.elapsedRealtime() + 150_000;
        do {
            JSONArray events = fixture("/evidence").getJSONArray("events");
            boolean hostOwner = false, androidOwner = false, endpoint = false;
            for (int i = 0; i < events.length(); i++) {
                JSONObject event = events.getJSONObject(i);
                assertEquals(30078, event.getInt("kind"));
                String address = eventTag(event, "d");
                if (address.startsWith("org.korri.device-owner:")) {
                    assertEquals("f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9", event.getString("pubkey"));
                    assertEquals("", event.getString("content"));
                    hostOwner |= address.equals("org.korri.device-owner:" + hostKey);
                    androidOwner |= address.equals("org.korri.device-owner:" + androidKey);
                } else {
                    assertTrue(address, address.startsWith("org.korri.endpoint:"));
                    assertFalse(event.getString("content").startsWith("{"));
                    endpoint |= hostKey.equals(event.getString("pubkey")) && androidKey.equals(eventTag(event, "p"));
                }
            }
            if (hostOwner && androidOwner && endpoint) {
                android.util.Log.i("FederationAcceptance", "two owner statements and recipient-encrypted endpoint; one unavailable relay plus one working");
                return;
            }
            SystemClock.sleep(250);
        } while (SystemClock.elapsedRealtime() < deadline);
        throw new AssertionError("Relay did not contain both owners and the host endpoint for Android");
    }

    private static String eventTag(JSONObject event, String name) throws Exception {
        JSONArray tags = event.getJSONArray("tags");
        for (int i = 0; i < tags.length(); i++) {
            JSONArray tag = tags.getJSONArray(i);
            if (name.equals(tag.getString(0))) return tag.getString(1);
        }
        return "";
    }

    private JSONObject fixture(String route) throws Exception {
        HttpURLConnection connection = connection(controlPort, route);
        try {
            connection.setRequestProperty("Authorization", "Bearer " + controlToken);
            connection.getOutputStream().write("{}".getBytes(StandardCharsets.UTF_8));
            assertEquals(route, 200, connection.getResponseCode());
            return new JSONObject(readBounded(connection.getInputStream()));
        } finally { connection.disconnect(); }
    }

    private JSONObject awaitGame() throws Exception {
        long deadline = SystemClock.elapsedRealtime() + 150_000;
        JSONObject outcome;
        do {
            outcome = call("app.catalog.snapshot", new JSONObject());
            if ("Err".equals(outcome.getString("_tag"))) {
                // Relay publication precedes Android's next discovery read. No
                // eligible endpoint is a bounded waiting state, not a ready catalog.
                assertEquals(outcome.toString(), "UpstreamUnreachable",
                        outcome.getJSONObject("payload").getString("code"));
            } else {
                assertEquals(outcome.toString(), "Ok", outcome.getString("_tag"));
                JSONObject catalog = outcome.getJSONObject("payload");
                if (catalog.getJSONArray("games").length() == 1 && (!catalog.has("failures") || catalog.getJSONArray("failures").length() == 0)) return catalog.getJSONArray("games").getJSONObject(0);
            }
            SystemClock.sleep(250);
        } while (SystemClock.elapsedRealtime() < deadline);
        throw new AssertionError("Catalog did not become ready: " + outcome);
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
        port = Integer.parseInt(js(web, "window.KorriRpc.korridPort()"));
        capability = new JSONArray("[" + js(web, "window.KorriRpc.korridCapability()") + "]").getString(0);
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
