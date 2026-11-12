package com.limelight;

import android.graphics.Bitmap;
import android.os.SystemClock;
import android.view.KeyEvent;
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
import java.io.FileOutputStream;
import java.io.InputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicReference;
import static org.junit.Assert.*;

/** Opt-in acceptance on an owned phone with a real aka catalog. Ends one test launch. */
@RunWith(AndroidJUnit4.class)
public class KorriGameplayOverlayDeviceTest {
    private int port;
    private String surface;
    private File evidence;
    private KorriShellActivity shellActivity;
    private String evidenceRun;

    @Test public void overlay_keeps_the_exact_game_live_and_controls_its_viewer() throws Exception {
        String title = InstrumentationRegistry.getArguments().getString("gameTitle");
        assertNotNull("Supply -e gameTitle with the real aka game's title (this test ends one launch)", title);
        evidenceRun = InstrumentationRegistry.getArguments().getString("evidenceRun");
        assertNotNull("Supply -e evidenceRun with a unique capture name", evidenceRun);
        assertTrue("Invalid evidenceRun", evidenceRun.matches("[A-Za-z0-9-]+"));
        evidence = new File(InstrumentationRegistry.getInstrumentation().getTargetContext().getCacheDir(), "overlay-376/" + evidenceRun);
        assertTrue(evidence.isDirectory() || evidence.mkdirs());
        try (ActivityScenario<KorriShellActivity> shell = ActivityScenario.launch(KorriShellActivity.class)) {
            long deadline = SystemClock.elapsedRealtime() + 120000;
            AtomicInteger address = new AtomicInteger();
            AtomicReference<String> packageId = new AtomicReference<>();
            do {
                shell.onActivity(activity -> {
                    if (activity.isShellReady() && activity.shellLibraryCount() > 0) {
                        address.set(activity.shellPort());
                        packageId.set(activity.shellSurfaceId());
                        shellActivity = activity;
                    }
                });
                if (address.get() != 0) break;
                SystemClock.sleep(100);
            } while (SystemClock.elapsedRealtime() < deadline);
            assertNotEquals("The real surface library did not become ready", 0, address.get());
            port = address.get();
            surface = packageId.get();
            JSONObject game = game(title);
            String launch = launch(game);
            snapshot("running", status());
            screenshot("running");

            for (int cycle = 0; cycle < 2; cycle++) {
                press(cycle % 2 == 0 ? KeyEvent.KEYCODE_BUTTON_MODE : KeyEvent.KEYCODE_BACK);
                awaitPresentation("gameplay-overlay");
                assertLive(launch);
                if (cycle == 1) switchSurface();
                snapshot("home-" + cycle, status());
                screenshot("home-" + cycle);
                if (cycle == 0) {
                    audio("home-audio-1");
                    record();
                    screenshot("home-moving-video");
                    audio("home-audio-2");
                }
                // StreamContainer pre-IME must deliver both edges to the package,
                // then Confirm must choose Resume rather than enter the game.
                press(KeyEvent.KEYCODE_DPAD_DOWN);
                press(KeyEvent.KEYCODE_DPAD_UP);
                screenshot("home-after-navigation-" + cycle);
                press(KeyEvent.KEYCODE_DPAD_CENTER);
                awaitPresentation("catalog");
                awaitViewer(launch, "Running");
                assertLive(launch);
            }
            screenshot("resumed");

            press(KeyEvent.KEYCODE_BUTTON_MODE);
            awaitPresentation("gameplay-overlay");
            invoke("Open Korri");
            awaitPresentation("catalog");
            awaitViewer(launch, "Backgrounded");
            assertLive(launch);
            snapshot("open-korri", status());
            screenshot("open-korri");
            assertEquals("Ok", rpc("app.session.resume", new JSONObject().put("expectedLaunchId", launch)).getString("_tag"));
            awaitViewer(launch, "Running");

            press(KeyEvent.KEYCODE_BUTTON_MODE);
            awaitPresentation("gameplay-overlay");
            assertLive(launch);
            screenshot("before-disconnect");
            invoke("Disconnect");
            awaitViewer(launch, "Stopped");
            awaitPresentation("catalog");
            assertLive(launch);
            screenshot("disconnected");
            snapshot("disconnected-streamed-session", status());
            assertEquals("Stopped", KorridServer.viewerStatus(launch));
            assertEquals("Ok", rpc("app.session.resume", new JSONObject().put("expectedLaunchId", launch)).getString("_tag"));
            awaitViewer(launch, "Running");
            assertLive(launch);
            screenshot("reconnected");

            press(KeyEvent.KEYCODE_BUTTON_MODE);
            awaitPresentation("gameplay-overlay");
            screenshot("before-end");
            invoke("End");
            awaitViewer(launch, "Stopped");
            awaitPresentation("catalog");
            JSONObject ended = status();
            if ("Err".equals(ended.getString("_tag"))) {
                String code = ended.getJSONObject("payload").getString("code");
                assertTrue("End status failed: " + code, code.equals("NoActiveSession") || code.equals("SessionCompleted"));
            } else assertEquals("Ok", ended.getString("_tag"));
            assertNull("End must remove the exact streamed session", active(ended));
            snapshot("ended", ended);
            screenshot("ended");
            android.util.Log.i("KorriOverlayProof", "PASS: two Resume cycles, Open Korri, Disconnect, reconnect, End; exact launch=" + launch);
        }
    }

    private void switchSurface() {
        String before = shellActivity.shellSurfaceId();
        android.app.Instrumentation input = InstrumentationRegistry.getInstrumentation();
        input.sendKeySync(new KeyEvent(KeyEvent.ACTION_DOWN, KeyEvent.KEYCODE_BUTTON_L1));
        input.sendKeySync(new KeyEvent(KeyEvent.ACTION_DOWN, KeyEvent.KEYCODE_BUTTON_R1));
        input.sendKeySync(new KeyEvent(KeyEvent.ACTION_UP, KeyEvent.KEYCODE_BUTTON_R1));
        input.sendKeySync(new KeyEvent(KeyEvent.ACTION_UP, KeyEvent.KEYCODE_BUTTON_L1));
        long deadline = SystemClock.elapsedRealtime() + 20000;
        while (before.equals(shellActivity.shellSurfaceId()) && SystemClock.elapsedRealtime() < deadline) SystemClock.sleep(100);
        assertNotEquals("L1+R1 did not switch the active overlay package", before, shellActivity.shellSurfaceId());
        surface = shellActivity.shellSurfaceId();
        android.util.Log.i("KorriOverlayProof", "overlay package switched: " + before + " -> " + surface);
    }

    private JSONObject game(String title) throws Exception {
        request(new JSONObject().put("_tag", "MoveCatalog")
                .put("query", new JSONObject().put("text", title).put("order", "korri"))
                .put("from", new JSONObject().put("_tag", "Start")).put("count", 50));
        long deadline = SystemClock.elapsedRealtime() + 15000;
        do {
            JSONArray games = model().getJSONObject("catalog").optJSONArray("window");
            if (games != null && games.length() == 1) return games.getJSONObject(0);
            if (games != null && games.length() > 1) fail("gameTitle matches several games; supply a more specific real title");
            SystemClock.sleep(100);
        } while (SystemClock.elapsedRealtime() < deadline);
        throw new AssertionError("The real aka game was not found in the surface catalog");
    }

    private String launch(JSONObject game) throws Exception {
        JSONObject before = status();
        if ("Err".equals(before.getString("_tag"))) {
            String code = before.getJSONObject("payload").getString("code");
            assertTrue("Cannot prove that this device is idle: " + code,
                    code.equals("NoActiveSession") || code.equals("SessionCompleted"));
        }
        assertNull("Refusing to End a pre-existing selected session; this test starts its own launches", active(before));
        JSONObject launch = new JSONObject().put("_tag", "LaunchGame").put("gameId", game.getString("id"));
        JSONArray locations = game.optJSONArray("launchLocations");
        if (locations != null) {
            String aka = null;
            for (int i = 0; i < locations.length(); i++) {
                JSONObject location = locations.getJSONObject(i);
                if (location.getString("label").equalsIgnoreCase("aka")) aka = location.getString("id");
            }
            assertNotNull("The real game has no aka launch location", aka);
            launch.put("launchLocationId", aka);
        }
        request(launch);
        long deadline = SystemClock.elapsedRealtime() + 90000;
        do {
            JSONObject active = active(status());
            if (active != null) {
                String id = active.getString("launchId");
                if ("Running".equals(KorridServer.viewerStatus(id))) return id;
            }
            SystemClock.sleep(100);
        } while (SystemClock.elapsedRealtime() < deadline);
        throw new AssertionError("korrid did not start its exact Android stream viewer");
    }

    private void invoke(String label) throws Exception {
        JSONArray controls = model().getJSONObject("presentation").getJSONArray("controls");
        for (int i = 0; i < controls.length(); i++) {
            JSONObject control = controls.getJSONObject(i);
            if (!label.equals(control.getString("label"))) continue;
            assertTrue(control.getBoolean("enabled"));
            // This is the exact request emitted by a package after its own UI
            // confirmation. Resume/input routing above is tested with real keys.
            request(new JSONObject().put("_tag", "InvokeGameplayControl").put("controlId", control.getString("id")));
            return;
        }
        throw new AssertionError("The real overlay did not offer " + label);
    }

    private void assertLive(String launch) throws Exception {
        JSONObject active = active(status());
        assertNotNull("The streamed launch disappeared", active);
        assertEquals(launch, active.getString("launchId"));
        assertEquals("running", active.getString("phase"));
    }
    private JSONObject active(JSONObject outcome) {
        JSONObject payload = outcome.optJSONObject("payload");
        return payload == null ? null : payload.optJSONObject("active");
    }
    private JSONObject status() throws Exception { return rpc("app.session.status", new JSONObject()); }
    private JSONObject model() throws Exception {
        JSONObject outcome = rpc("app.surface.model", new JSONObject().put("surfaceId", surface));
        assertEquals("Ok", outcome.getString("_tag"));
        return outcome.getJSONObject("payload");
    }
    private void request(JSONObject request) throws Exception {
        assertEquals("Ok", rpc("app.surface.request", new JSONObject().put("surfaceId", surface).put("request", request)).getString("_tag"));
    }
    private void awaitPresentation(String kind) throws Exception {
        long deadline = SystemClock.elapsedRealtime() + 20000;
        do {
            if (kind.equals(model().getJSONObject("presentation").getString("kind"))) return;
            SystemClock.sleep(100);
        } while (SystemClock.elapsedRealtime() < deadline);
        fail("The surface did not show " + kind);
    }
    private void awaitViewer(String launch, String expected) throws Exception {
        long deadline = SystemClock.elapsedRealtime() + 60000;
        do {
            if (expected.equals(KorridServer.viewerStatus(launch))) return;
            SystemClock.sleep(100);
        } while (SystemClock.elapsedRealtime() < deadline);
        fail("The exact viewer did not become " + expected);
    }
    private void press(int code) {
        InstrumentationRegistry.getInstrumentation().sendKeyDownUpSync(code);
        SystemClock.sleep(250);
    }
    private JSONObject rpc(String method, JSONObject payload) throws Exception {
        HttpURLConnection connection = (HttpURLConnection) new URL("http://127.0.0.1:" + port + "/rpc").openConnection();
        try {
            connection.setConnectTimeout(3000);
            connection.setReadTimeout(70000);
            connection.setRequestMethod("POST");
            connection.setRequestProperty("Content-Type", "application/json");
            connection.setRequestProperty("Authorization", "Bearer " + KorridServer.capability());
            connection.setDoOutput(true);
            connection.getOutputStream().write(new JSONObject().put("_tag", method).put("payload", payload).toString().getBytes(StandardCharsets.UTF_8));
            assertEquals("Local RPC failed", 200, connection.getResponseCode());
            return new JSONObject(new String(read(connection.getInputStream()), StandardCharsets.UTF_8)).getJSONObject("outcome");
        } finally { connection.disconnect(); }
    }
    private byte[] read(InputStream input) throws Exception {
        try (InputStream owned = input; ByteArrayOutputStream bytes = new ByteArrayOutputStream()) {
            byte[] buffer = new byte[4096];
            int count;
            while ((count = owned.read(buffer)) != -1) bytes.write(buffer, 0, count);
            return bytes.toByteArray();
        }
    }
    private void snapshot(String name, JSONObject status) throws Exception {
        try (FileOutputStream output = new FileOutputStream(new File(evidence, name + ".json"))) {
            output.write(status.toString(2).getBytes(StandardCharsets.UTF_8));
        }
        android.util.Log.i("KorriOverlayProof", name + ": " + status);
    }
    private void screenshot(String name) throws Exception {
        Bitmap bitmap = InstrumentationRegistry.getInstrumentation().getUiAutomation().takeScreenshot();
        assertNotNull("Device screenshot failed", bitmap);
        try (FileOutputStream output = new FileOutputStream(new File(evidence, name + ".png"))) {
            assertTrue(bitmap.compress(Bitmap.CompressFormat.PNG, 100, output));
        } finally { bitmap.recycle(); }
    }
    private void record() throws Exception {
        android.os.ParcelFileDescriptor descriptor = InstrumentationRegistry.getInstrumentation().getUiAutomation()
                .executeShellCommand("screenrecord --time-limit 6 /sdcard/Download/korri-376-" + evidenceRun + ".mp4");
        try (InputStream input = new android.os.ParcelFileDescriptor.AutoCloseInputStream(descriptor)) {
            read(input);
        }
    }

    private void audio(String name) throws Exception {
        android.os.ParcelFileDescriptor descriptor = InstrumentationRegistry.getInstrumentation().getUiAutomation()
                .executeShellCommand("dumpsys media.audio_flinger");
        try (InputStream input = new android.os.ParcelFileDescriptor.AutoCloseInputStream(descriptor);
             FileOutputStream output = new FileOutputStream(new File(evidence, name + ".txt"))) {
            output.write(read(input));
        }
    }
}
