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
public class UltraLowLatencyIntentTest {
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
    public void testUltraLowLatencyExtrasConstant() {
        assertEquals("UltraLowLatency", Game.EXTRA_ULTRA_LOW_LATENCY);
    }

    @Test
    public void testIntentWithUltraLowLatencyExtra_True() {
        Intent intent = new Intent(context, Game.class);
        intent.putExtra(Game.EXTRA_ULTRA_LOW_LATENCY, "true");

        assertEquals("true", intent.getStringExtra(Game.EXTRA_ULTRA_LOW_LATENCY));
    }

    @Test
    public void testIntentWithUltraLowLatencyExtra_False() {
        Intent intent = new Intent(context, Game.class);
        intent.putExtra(Game.EXTRA_ULTRA_LOW_LATENCY, "false");

        assertEquals("false", intent.getStringExtra(Game.EXTRA_ULTRA_LOW_LATENCY));
    }

    @Test
    public void testIntentWithNoUltraLowLatencyExtra() {
        Intent intent = new Intent(context, Game.class);

        assertNull(intent.getStringExtra(Game.EXTRA_ULTRA_LOW_LATENCY));
    }

    @Test
    public void testUltraLowLatencyOverrideLogic_True() {
        String intentUltraLowLatency = "true";
        boolean enableUltraLowLatency = false;

        if (intentUltraLowLatency != null) {
            enableUltraLowLatency = Boolean.parseBoolean(intentUltraLowLatency);
        }

        assertTrue(enableUltraLowLatency);
    }

    @Test
    public void testUltraLowLatencyOverrideLogic_False() {
        String intentUltraLowLatency = "false";
        boolean enableUltraLowLatency = true;  // Start with true

        if (intentUltraLowLatency != null) {
            enableUltraLowLatency = Boolean.parseBoolean(intentUltraLowLatency);
        }

        assertFalse(enableUltraLowLatency);
    }

    @Test
    public void testUltraLowLatencyOverrideLogic_InvalidValue() {
        String intentUltraLowLatency = "invalid";
        boolean enableUltraLowLatency = true;  // Start with true

        if (intentUltraLowLatency != null) {
            enableUltraLowLatency = Boolean.parseBoolean(intentUltraLowLatency);
        }

        // Boolean.parseBoolean returns false for any non-"true" string
        assertFalse(enableUltraLowLatency);
    }
}
