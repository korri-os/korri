package com.limelight;

import android.content.Context;

import androidx.test.core.app.ApplicationProvider;

import com.limelight.binding.crypto.AndroidCryptoProvider;
import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.nvstream.http.NvApp;

import org.json.JSONArray;
import org.json.JSONObject;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;

import java.security.cert.X509Certificate;
import java.util.Collections;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicInteger;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertSame;
import static org.junit.Assert.assertTrue;
import static org.junit.Assert.fail;

@RunWith(RobolectricTestRunner.class)
public class KorriMoonlightShellFlowTest {
    private static ComputerDetails computer() {
        ComputerDetails value = new ComputerDetails();
        value.uuid = "host-uuid";
        value.name = "Zao";
        value.state = ComputerDetails.State.ONLINE;
        return value;
    }

    private static String validSpec() throws Exception {
        return new JSONObject()
                .put("launchId", "0123456789abcdef0123456789abcdef")
                .put("transportId", "@korri:moonlight/moonlight")
                .put("context", new JSONObject()
                        .put("contributors", new JSONArray().put(new JSONObject()
                                .put("kind", "transport")
                                .put("id", "@korri:moonlight/moonlight")))
                        .put("executor", new JSONObject()
                                .put("id", "android-moonlight")
                                .put("available", false))
                        .put("foreground", new JSONObject().put("kind", "artemis-game")))
                .put("implementation", "artemis")
                .put("sunshineApp", "Korri Stream")
                .put("hostUuid", "host-uuid")
                .put("appId", 7)
                .put("integrity", "verified before Java parsing")
                .toString();
    }

    @Test
    public void serializedSecondStartIsRejectedWhileFirstLaunchRemainsActive() throws Exception {
        AtomicBoolean active = new AtomicBoolean();
        AtomicInteger starts = new AtomicInteger();
        KorriMoonlightShellFlow flow = flow(
                (specJson, spec) -> {
                    if (!active.compareAndSet(false, true)) return null;
                    return () -> active.set(false);
                },
                () -> starts.incrementAndGet());

        flow.startStream(validSpec());
        assertTrue(active.get());
        assertEquals(1, starts.get());
        try {
            flow.startStream(validSpec());
            fail("expected active launch rejection");
        } catch (KorriMoonlightShellFlow.Failure expected) {
            assertEquals("StartInProgress", expected.reason);
        }
        assertEquals(1, starts.get());
    }

    @Test
    public void publicationFailureNeverStartsActivity() throws Exception {
        AtomicInteger starts = new AtomicInteger();
        KorriMoonlightShellFlow flow = flow(
                (specJson, spec) -> {
                    throw new IllegalStateException("publication unavailable");
                },
                starts::incrementAndGet);
        try {
            flow.startStream(validSpec());
            fail("expected publication failure");
        } catch (KorriMoonlightShellFlow.Failure expected) {
            assertEquals("StartFailed", expected.reason);
        }
        assertEquals(0, starts.get());
    }

    @Test
    public void activityStartFailureRollsBackExactReservation() throws Exception {
        AtomicBoolean active = new AtomicBoolean();
        KorriMoonlightShellFlow flow = flow(
                (specJson, spec) -> {
                    active.set(true);
                    return () -> active.set(false);
                },
                () -> {
                    throw new IllegalStateException("activity failed");
                });
        try {
            flow.startStream(validSpec());
            fail("expected activity failure");
        } catch (KorriMoonlightShellFlow.Failure expected) {
            assertEquals("StartFailed", expected.reason);
        }
        assertFalse(active.get());
    }

    @Test
    public void exactCommittedComputerReachesGameIntent() throws Exception {
        ComputerDetails exact = computer();
        NvApp app = new NvApp("Korri Stream", "current", 7, false);
        X509Certificate certificate = certificate();
        exact.serverCert = certificate;
        final ComputerDetails[] started = new ComputerDetails[1];
        KorriMoonlightShellFlow flow = new KorriMoonlightShellFlow(
                spec -> "Authorized",
                hostUuid -> new ComputerDetails(exact),
                hostUuid -> new KorriMoonlightProvisioning.Provisioned(
                        exact, certificate, "<apps/>", Collections.singletonList(app)),
                (specJson, spec) -> () -> {},
                (specJson, spec, selected, computer) -> {
                    assertSame(app, selected);
                    started[0] = computer;
                });

        flow.startStream(validSpec());
        assertSame(exact, started[0]);
    }

    private interface StartAction {
        void run() throws Exception;
    }

    private static KorriMoonlightShellFlow flow(
            KorriMoonlightShellFlow.ActiveLaunchReservation reservation,
            StartAction start) {
        ComputerDetails exact = computer();
        NvApp app = new NvApp("Korri Stream", "current", 7, false);
        X509Certificate certificate = certificate();
        exact.serverCert = certificate;
        return new KorriMoonlightShellFlow(
                spec -> "Authorized",
                hostUuid -> exact,
                hostUuid -> new KorriMoonlightProvisioning.Provisioned(
                        exact, certificate, "<apps/>", Collections.singletonList(app)),
                reservation,
                (specJson, spec, selected, host) -> start.run());
    }

    private static X509Certificate certificate() {
        Context context = ApplicationProvider.getApplicationContext();
        return new AndroidCryptoProvider(context).getClientCertificate();
    }
}
