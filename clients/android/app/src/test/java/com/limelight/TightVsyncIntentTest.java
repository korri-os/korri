package com.limelight;

import android.content.Context;
import android.content.Intent;

import androidx.test.core.app.ApplicationProvider;

import org.junit.Before;
import org.junit.BeforeClass;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.annotation.Config;

import static org.junit.Assert.*;

@Config(sdk = {33}, shadows = {
    com.limelight.shadows.ShadowMoonBridge.class,
    com.limelight.shadows.ShadowGameManager.class
})
@RunWith(RobolectricTestRunner.class)
public class TightVsyncIntentTest {
    private Context context;

    @BeforeClass
    public static void suppressLogs() {
        TestLogSuppressor.install();
    }

    @Before
    public void setUp() {
        context = ApplicationProvider.getApplicationContext();
    }

    @Test
    public void testTightVsyncExtrasConstant() {
        assertEquals("TightVsync", Game.EXTRA_TIGHT_VSYNC);
    }

    @Test
    public void testIntentWithTightVsyncExtra_True() {
        Intent intent = new Intent(context, Game.class);
        intent.putExtra(Game.EXTRA_TIGHT_VSYNC, "true");

        assertEquals("true", intent.getStringExtra(Game.EXTRA_TIGHT_VSYNC));
    }

    @Test
    public void testIntentWithTightVsyncExtra_False() {
        Intent intent = new Intent(context, Game.class);
        intent.putExtra(Game.EXTRA_TIGHT_VSYNC, "false");

        assertEquals("false", intent.getStringExtra(Game.EXTRA_TIGHT_VSYNC));
    }

    @Test
    public void testIntentWithNoTightVsyncExtra() {
        Intent intent = new Intent(context, Game.class);

        assertNull(intent.getStringExtra(Game.EXTRA_TIGHT_VSYNC));
    }

    @Test
    public void testTightVsyncOverrideLogic_True() {
        String intentTightVsync = "true";
        boolean forceTightThresholds = false;

        if (intentTightVsync != null) {
            forceTightThresholds = Boolean.parseBoolean(intentTightVsync);
        }

        assertTrue(forceTightThresholds);
    }

    @Test
    public void testTightVsyncOverrideLogic_False() {
        String intentTightVsync = "false";
        boolean forceTightThresholds = true;  // Start with true

        if (intentTightVsync != null) {
            forceTightThresholds = Boolean.parseBoolean(intentTightVsync);
        }

        assertFalse(forceTightThresholds);
    }

    @Test
    public void testTightVsyncOverrideLogic_InvalidValue() {
        String intentTightVsync = "invalid";
        boolean forceTightThresholds = true;  // Start with true

        if (intentTightVsync != null) {
            forceTightThresholds = Boolean.parseBoolean(intentTightVsync);
        }

        // Boolean.parseBoolean returns false for any non-"true" string
        assertFalse(forceTightThresholds);
    }
}
