package com.limelight;

import static org.junit.Assert.*;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;

@RunWith(RobolectricTestRunner.class)
public class KorriRpcBindingTest {
    private static final String ORIGIN = "https://appassets.androidplatform.net";
    private static final String CAPABILITY = "a".repeat(64);

    @Test public void both_methods_must_be_read_by_the_trusted_main_frame() {
        KorriRpcBinding binding = new KorriRpcBinding(ORIGIN, 12345, CAPABILITY);
        assertFalse(binding.consumed(ORIGIN, true, "korridPort"));
        assertFalse(binding.consumed("https://other.example", true, "korridCapability"));
        assertFalse(binding.consumed(ORIGIN, false, "korridCapability"));
        assertTrue(binding.consumed(ORIGIN, true, "korridCapability"));
        binding.navigationStarted();
        assertFalse(binding.consumed(ORIGIN, true, "korridCapability"));
        assertFalse(binding.consumed(ORIGIN, true, "unknown"));
        assertTrue(binding.consumed(ORIGIN, true, "korridPort"));
    }

    @Test public void bootstrap_limits_the_binding_to_the_top_frame_and_freezes_it() {
        String script = new KorriRpcBinding(ORIGIN, 12345, CAPABILITY).documentStartScript();
        assertTrue(script.contains("window !== window.top"));
        assertTrue(script.contains("location.origin !== " + org.json.JSONObject.quote(ORIGIN)));
        assertTrue(script.contains("Object.freeze("));
        assertTrue(script.contains("writable: false, configurable: false, enumerable: false"));
        assertFalse(script.contains("localStorage"));
        assertFalse(script.contains("console."));
        assertFalse(script.contains("fetch("));
    }

    @Test public void invalid_credentials_never_reach_the_script() {
        assertThrows(IllegalArgumentException.class,
                () -> new KorriRpcBinding(ORIGIN, 0, CAPABILITY));
        assertThrows(IllegalArgumentException.class,
                () -> new KorriRpcBinding(ORIGIN, 65536, CAPABILITY));
        assertThrows(IllegalArgumentException.class,
                () -> new KorriRpcBinding(ORIGIN, 12345, "invalid"));
    }
}
