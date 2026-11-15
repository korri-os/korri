package com.limelight;

import android.content.Context;
import android.content.Intent;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.os.SystemClock;
import android.util.Base64;
import android.util.Log;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.platform.app.InstrumentationRegistry;
import com.limelight.binding.PlatformBinding;
import com.limelight.computers.IdentityManager;
import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.nvstream.http.HostCertificateState;
import com.limelight.nvstream.http.NvApp;
import com.limelight.nvstream.http.NvHTTP;
import com.simonwjackson.korri.korrid.KorridServer;
import org.json.JSONObject;
import org.junit.Test;
import org.junit.runner.RunWith;
import java.io.ByteArrayInputStream;
import java.lang.reflect.Constructor;
import java.lang.reflect.Field;
import java.security.cert.CertificateFactory;
import java.security.cert.X509Certificate;
import java.util.UUID;
import static org.junit.Assert.*;

/**
 * Test hook of #476 slice 1: today's live resize, driven as korrid drives it,
 * against stream-cadence's sender fixture with Korri's patched Sunshine.
 *
 * <p>korrid's Portrait control ends on the client in one JNI call,
 * {@link KorridServer#viewerResize}, after korrid reads the viewer's shape
 * from {@link KorridServer#viewerSnapshot} (upstreams/live_shape.rs). Nothing
 * else reaches {@code Game.resizeLiveStream}. korrid makes that call only for
 * a streamed launch on a holder korrid, and the fixture has no korrid: no
 * launch, and no certificate-control socket to provision the app's
 * certificate. So this hook does korrid's part and nothing else:
 * <ol>
 * <li>it opens the exact viewer as {@code KorriStreamViewer.start} does after
 * {@code prepare()}, with the same Game extras. The fixture's pinned
 * certificate takes the place of korrid's provisioning: the proof pairs the
 * app's key pair with the fixture's PIN before the test runs;</li>
 * <li>it reads the shape, asks for the Portrait size (the holder's answer:
 * the owner's screen, swapped; host/live_shape.rs {@code desired}) and back,
 * through the same JNI entry, and reads the encoded size after each.</li>
 * </ol>
 * The app's code is unchanged. This class is in the test APK only. Each phase
 * holds for {@code holdMs} after its log line, so that the proof takes
 * screenshots of it.
 */
@RunWith(AndroidJUnit4.class)
public class KorriLiveResizeFixtureTest {
    private static final String TAG = "KorriResizeProof";

    @Test public void portrait_and_back_resize_the_live_stream() throws Exception {
        Bundle arguments = InstrumentationRegistry.getArguments();
        String host = required(arguments, "host");
        int port = Integer.parseInt(required(arguments, "port"));
        String application = required(arguments, "application");
        byte[] serverCertificate = Base64.decode(required(arguments, "serverCertificate"), Base64.DEFAULT);
        long hold = Long.parseLong(arguments.getString("holdMs", "8000"));
        Context context = InstrumentationRegistry.getInstrumentation().getTargetContext();
        KorriStreamViewer.initialize(context);

        X509Certificate pinned = (X509Certificate) CertificateFactory.getInstance("X.509")
                .generateCertificate(new ByteArrayInputStream(serverCertificate));
        ComputerDetails.AddressTuple tuple = new ComputerDetails.AddressTuple(host, port);
        String uniqueId = new IdentityManager(context).getUniqueId();
        ComputerDetails computer = new NvHTTP(tuple, 0, uniqueId, null,
                PlatformBinding.getCryptoProvider(context)).getComputerDetails(true);
        NvHTTP trusted = new NvHTTP(tuple, computer.httpsPort, uniqueId, pinned,
                PlatformBinding.getCryptoProvider(context));
        assertEquals("The app's key pair is not paired with the fixture's Sunshine",
                HostCertificateState.ACCEPTED, trusted.getPairState());
        NvApp selected = null;
        for (NvApp app : trusted.getAppList()) {
            if (application.equals(app.getAppName())) selected = app;
        }
        assertNotNull("The fixture's Sunshine has no application " + application, selected);
        String[] screen = KorridServer.viewerDisplay().split("x");
        assertEquals("The viewer reports no screen", 2, screen.length);

        // The extras of KorriStreamViewer.prepare, and korrid's launch display:
        // the viewer's own screen (viewerDisplay), landscape.
        Intent intent = new Intent(context, Game.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                .putExtra(Game.EXTRA_HOST, tuple.address)
                .putExtra(Game.EXTRA_PORT, tuple.port)
                .putExtra(Game.EXTRA_HTTPS_PORT, computer.httpsPort)
                .putExtra(Game.EXTRA_APP_NAME, selected.getAppName())
                .putExtra(Game.EXTRA_APP_UUID, selected.getAppUUID())
                .putExtra(Game.EXTRA_APP_ID, selected.getAppId())
                .putExtra(Game.EXTRA_APP_HDR, selected.isHdrSupported())
                .putExtra(Game.EXTRA_UNIQUEID, uniqueId)
                .putExtra(Game.EXTRA_PC_UUID, computer.uuid)
                .putExtra(Game.EXTRA_PC_NAME, computer.name)
                .putExtra(Game.EXTRA_SERVER_CERT, pinned.getEncoded())
                .putExtra(Game.EXTRA_WIDTH, Integer.parseInt(screen[0]))
                .putExtra(Game.EXTRA_HEIGHT, Integer.parseInt(screen[1]));
        String launch = UUID.randomUUID().toString().replace("-", "");
        openExactViewer(context, launch, intent);
        try {
            JSONObject shape = readyShape(launch);
            int width = shape.getJSONObject("screen").getInt("width");
            int height = shape.getJSONObject("screen").getInt("height");
            phase("streaming", shape, hold);

            assertEquals("Applied", KorridServer.viewerResize(launch, shape.getString("connection"),
                    String.valueOf(height), String.valueOf(width)));
            phase("portrait", encoded(launch, height, width), hold);

            shape = readyShape(launch);
            assertEquals("Applied", KorridServer.viewerResize(launch, shape.getString("connection"),
                    String.valueOf(width), String.valueOf(height)));
            phase("landscape", encoded(launch, width, height), hold);
        } finally {
            Log.i(TAG, "stop: " + KorridServer.viewerStop(launch));
        }
    }

    /**
     * KorriStreamViewer.start after prepare(): a new exact session, its intent,
     * the Game activity started on the main thread, and Running awaited. The
     * session is private to the viewer, so the hook reaches it by reflection.
     */
    private static void openExactViewer(Context context, String launch, Intent intent) throws Exception {
        Object host = field(KorriStreamViewer.class, "instance").get(null);
        assertNotNull("KorriStreamViewer is not initialized", host);
        Class<?> sessionClass = Class.forName("com.limelight.KorriStreamViewer$Session");
        Constructor<?> make = sessionClass.getDeclaredConstructor(String.class);
        make.setAccessible(true);
        Object session = make.newInstance(launch);
        KorriStreamViewerState state = (KorriStreamViewerState) field(sessionClass, "state").get(session);
        intent.putExtra(Game.EXTRA_KORRI_LAUNCH_ID, launch)
                .putExtra(KorriStreamViewer.EXTRA_GENERATION, state.generation);
        field(sessionClass, "intent").set(session, intent);
        field(KorriStreamViewer.class, "session").set(host, session);
        Log.i(TAG, "open: launch " + launch + " generation " + state.generation);
        new Handler(Looper.getMainLooper()).post(() -> context.startActivity(new Intent(intent)));
        assertTrue("The exact viewer did not report Running",
                state.await(KorriStreamViewerState.Phase.RUNNING, 45_000));
    }

    /** korrid's read before a shape change: the snapshot's shape, once it is ready. */
    private static JSONObject readyShape(String launch) throws Exception {
        long deadline = SystemClock.elapsedRealtime() + 30_000;
        JSONObject last = null;
        while (SystemClock.elapsedRealtime() < deadline) {
            JSONObject snapshot = new JSONObject(KorridServer.viewerSnapshot(launch));
            last = snapshot.optJSONObject("shape");
            if (last != null && last.optBoolean("ready")) return last;
            SystemClock.sleep(200);
        }
        throw new AssertionError("The viewer's shape did not become ready: " + last);
    }

    private static JSONObject encoded(String launch, int width, int height) throws Exception {
        JSONObject shape = new JSONObject(KorridServer.viewerSnapshot(launch)).getJSONObject("shape");
        JSONObject size = shape.getJSONObject("encoded");
        assertEquals("encoded width", width, size.getInt("width"));
        assertEquals("encoded height", height, size.getInt("height"));
        return shape;
    }

    private static void phase(String name, JSONObject shape, long hold) {
        Log.i(TAG, "phase=" + name + " shape=" + shape);
        SystemClock.sleep(hold);
    }

    private static Field field(Class<?> owner, String name) throws NoSuchFieldException {
        Field found = owner.getDeclaredField(name);
        found.setAccessible(true);
        return found;
    }

    private static String required(Bundle arguments, String name) {
        String value = arguments.getString(name);
        assertNotNull("Supply -e " + name, value);
        return value;
    }
}
