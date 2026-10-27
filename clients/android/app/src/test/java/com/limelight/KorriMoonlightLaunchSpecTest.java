package com.limelight;

import com.limelight.nvstream.http.NvApp;

import org.json.JSONObject;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;

import java.util.Arrays;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertNull;
import static org.junit.Assert.assertSame;
import static org.junit.Assert.fail;

@RunWith(RobolectricTestRunner.class)
public class KorriMoonlightLaunchSpecTest {
    private static JSONObject validSpec() throws Exception {
        return new JSONObject()
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
                .put("integrity", "verified and consumed by Rust before parsing");
    }

    @Test
    public void parsesTheClosedSignedMoonlightLaunchTreaty() throws Exception {
        KorriMoonlightLaunchSpec spec = KorriMoonlightLaunchSpec.parse(validSpec().toString());
        assertEquals("0123456789abcdef0123456789abcdef", spec.launchId);
        assertEquals("@korri:moonlight/moonlight", spec.transportId);
        assertEquals("artemis", spec.implementation);
        assertEquals("Korri Stream", spec.sunshineApp);
        assertEquals("host-uuid", spec.hostUuid);
        assertEquals(7, spec.appId);
    }

    @Test
    public void rejectsUnknownOrUnsupportedFieldsAfterRustAuthorization() throws Exception {
        assertInvalid(validSpec().put("hostUuid", ""));
        assertInvalid(validSpec().put("appId", 0));
        assertInvalid(validSpec().put("implementation", "other"));
        assertInvalid(validSpec().put("transportId", "@other:moonlight/moonlight"));
        assertInvalid(validSpec().put("rawHost", "must not cross"));
    }

    @Test
    public void selectsOnlyTheCachedIdWithThePluginOwnedAppName() throws Exception {
        KorriMoonlightLaunchSpec spec = KorriMoonlightLaunchSpec.parse(validSpec().toString());
        NvApp expected = new NvApp("Korri Stream", "app-uuid", 7, false);
        NvApp wrongName = new NvApp("Desktop", "desktop-uuid", 7, false);
        NvApp wrongId = new NvApp("Korri Stream", "other-uuid", 8, false);

        assertSame(expected, spec.selectExpectedApp(Arrays.asList(wrongId, expected)));
        assertNull(spec.selectExpectedApp(Arrays.asList(wrongName, wrongId)));
    }

    private static void assertInvalid(JSONObject value) throws Exception {
        try {
            KorriMoonlightLaunchSpec.parse(value.toString());
            fail("expected invalid Moonlight launch spec");
        } catch (KorriMoonlightLaunchSpec.Invalid expected) {
            assertEquals("InvalidSpec", expected.reason);
        }
    }
}
