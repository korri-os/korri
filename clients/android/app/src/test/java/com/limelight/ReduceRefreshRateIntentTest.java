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
public class ReduceRefreshRateIntentTest {
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
    public void testReduceRefreshRateExtrasConstant() {
        assertEquals("ReduceRefreshRate", Game.EXTRA_REDUCE_REFRESH_RATE);
    }

    @Test
    public void testIntentWithReduceRefreshRateExtra_True() {
        Intent intent = new Intent(context, Game.class);
        intent.putExtra(Game.EXTRA_REDUCE_REFRESH_RATE, "true");

        assertEquals("true", intent.getStringExtra(Game.EXTRA_REDUCE_REFRESH_RATE));
    }

    @Test
    public void testIntentWithReduceRefreshRateExtra_False() {
        Intent intent = new Intent(context, Game.class);
        intent.putExtra(Game.EXTRA_REDUCE_REFRESH_RATE, "false");

        assertEquals("false", intent.getStringExtra(Game.EXTRA_REDUCE_REFRESH_RATE));
    }

    @Test
    public void testIntentWithNoReduceRefreshRateExtra() {
        Intent intent = new Intent(context, Game.class);

        assertNull(intent.getStringExtra(Game.EXTRA_REDUCE_REFRESH_RATE));
    }

    @Test
    public void testReduceRefreshRateOverrideLogic_True() {
        String intentReduceRefreshRate = "true";
        boolean reduceRefreshRate = false;

        if (intentReduceRefreshRate != null) {
            reduceRefreshRate = Boolean.parseBoolean(intentReduceRefreshRate);
        }

        assertTrue(reduceRefreshRate);
    }

    @Test
    public void testReduceRefreshRateOverrideLogic_False() {
        String intentReduceRefreshRate = "false";
        boolean reduceRefreshRate = true;  // Start with true

        if (intentReduceRefreshRate != null) {
            reduceRefreshRate = Boolean.parseBoolean(intentReduceRefreshRate);
        }

        assertFalse(reduceRefreshRate);
    }

    @Test
    public void testReduceRefreshRateOverrideLogic_InvalidValue() {
        String intentReduceRefreshRate = "invalid";
        boolean reduceRefreshRate = true;  // Start with true

        if (intentReduceRefreshRate != null) {
            reduceRefreshRate = Boolean.parseBoolean(intentReduceRefreshRate);
        }

        // Boolean.parseBoolean returns false for any non-"true" string
        assertFalse(reduceRefreshRate);
    }
}
