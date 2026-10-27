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
public class LowLatencyFrameBalanceIntentTest {
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
    public void testLowLatencyFrameBalanceExtrasConstant() {
        assertEquals("LowLatencyFrameBalance", Game.EXTRA_LOW_LATENCY_FRAME_BALANCE);
    }

    @Test
    public void testIntentWithLowLatencyFrameBalanceExtra_True() {
        Intent intent = new Intent(context, Game.class);
        intent.putExtra(Game.EXTRA_LOW_LATENCY_FRAME_BALANCE, "true");

        assertEquals("true", intent.getStringExtra(Game.EXTRA_LOW_LATENCY_FRAME_BALANCE));
    }

    @Test
    public void testIntentWithLowLatencyFrameBalanceExtra_False() {
        Intent intent = new Intent(context, Game.class);
        intent.putExtra(Game.EXTRA_LOW_LATENCY_FRAME_BALANCE, "false");

        assertEquals("false", intent.getStringExtra(Game.EXTRA_LOW_LATENCY_FRAME_BALANCE));
    }

    @Test
    public void testIntentWithNoLowLatencyFrameBalanceExtra() {
        Intent intent = new Intent(context, Game.class);

        assertNull(intent.getStringExtra(Game.EXTRA_LOW_LATENCY_FRAME_BALANCE));
    }

    @Test
    public void testLowLatencyFrameBalanceOverrideLogic_True() {
        String intentLfr = "true";
        boolean preferLowerDelays = false;

        if (intentLfr != null) {
            preferLowerDelays = Boolean.parseBoolean(intentLfr);
        }

        assertTrue(preferLowerDelays);
    }

    @Test
    public void testLowLatencyFrameBalanceOverrideLogic_False() {
        String intentLfr = "false";
        boolean preferLowerDelays = true;  // Start with true

        if (intentLfr != null) {
            preferLowerDelays = Boolean.parseBoolean(intentLfr);
        }

        assertFalse(preferLowerDelays);
    }

    @Test
    public void testLowLatencyFrameBalanceOverrideLogic_InvalidValue() {
        String intentLfr = "invalid";
        boolean preferLowerDelays = true;  // Start with true

        if (intentLfr != null) {
            preferLowerDelays = Boolean.parseBoolean(intentLfr);
        }

        // Boolean.parseBoolean returns false for any non-"true" string
        assertFalse(preferLowerDelays);
    }
}
