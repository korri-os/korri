package com.limelight;

import com.limelight.nvstream.http.NvApp;

import org.json.JSONObject;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;

import java.io.IOException;
import java.util.Arrays;
import java.util.Collections;
import java.util.concurrent.atomic.AtomicInteger;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertSame;
import static org.junit.Assert.fail;

@RunWith(RobolectricTestRunner.class)
public class KorriMoonlightAppResolverTest {
    private static KorriMoonlightLaunchSpec spec() throws Exception {
        return KorriMoonlightLaunchSpec.parse(new JSONObject()
                .put("launchId", "0123456789abcdef0123456789abcdef")
                .put("transportId", "@korri:moonlight/moonlight")
                .put("context", new JSONObject()
                        .put("contributors", new org.json.JSONArray().put(new JSONObject()
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
                .put("integrity", "opaque")
                .toString());
    }

    @Test
    public void refreshesBeforeResolvingTheSignedPluginOwnedApp() throws Exception {
        AtomicInteger refreshes = new AtomicInteger();
        NvApp current = new NvApp("Korri Stream", "current-app", 7, false);
        KorriMoonlightAppResolver resolver = new KorriMoonlightAppResolver(() -> {
            refreshes.incrementAndGet();
            return Arrays.asList(
                    new NvApp("Desktop", "desktop", 1, false),
                    current);
        });

        assertSame(current, resolver.refreshExpected(spec()));
        assertEquals(1, refreshes.get());
    }

    @Test
    public void tagsRefreshFailureAndCurrentAppMismatch() throws Exception {
        assertFailure(
                new KorriMoonlightAppResolver(() -> {
                    throw new IOException("host app list unavailable");
                }),
                "StartFailed",
                "host app list unavailable");
        assertFailure(
                new KorriMoonlightAppResolver(() -> Collections.singletonList(
                        new NvApp("Renamed Stream", "renamed", 7, false))),
                "AppNotFound",
                "Korri Stream");
    }

    private static void assertFailure(
            KorriMoonlightAppResolver resolver,
            String reason,
            String message) throws Exception {
        try {
            resolver.refreshExpected(spec());
            fail("expected app refresh failure");
        } catch (KorriMoonlightAppResolver.Failure failure) {
            assertEquals(reason, failure.reason);
            org.junit.Assert.assertTrue(failure.getMessage().contains(message));
        }
    }
}
