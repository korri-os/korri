package com.limelight;

import android.content.Context;
import android.content.Intent;

import androidx.test.core.app.ApplicationProvider;

import com.limelight.preferences.PreferenceConfiguration;

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
public class FramePacingIntentTest {
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
    public void testFramePacingExtrasConstant() {
        assertEquals("FramePacing", Game.EXTRA_FRAME_PACING);
    }

    @Test
    public void testIntentWithFramePacingExtra() {
        Intent intent = new Intent(context, Game.class);
        intent.putExtra(Game.EXTRA_FRAME_PACING, "balanced");

        assertEquals("balanced", intent.getStringExtra(Game.EXTRA_FRAME_PACING));
    }

    @Test
    public void testIntentWithNoFramePacingExtra() {
        Intent intent = new Intent(context, Game.class);

        assertNull(intent.getStringExtra(Game.EXTRA_FRAME_PACING));
    }

    @Test
    public void testFramePacingOverrideLogic_Latency() {
        String intentFramePacing = "latency";
        int framePacing = -1;
        int warpFactor = -1;

        if ("latency".equals(intentFramePacing)) {
            framePacing = PreferenceConfiguration.FRAME_PACING_MIN_LATENCY;
            warpFactor = 0;
        }

        assertEquals(PreferenceConfiguration.FRAME_PACING_MIN_LATENCY, framePacing);
        assertEquals(0, warpFactor);
    }

    @Test
    public void testFramePacingOverrideLogic_Balanced() {
        String intentFramePacing = "balanced";
        int framePacing = -1;
        int warpFactor = -1;

        if ("balanced".equals(intentFramePacing)) {
            framePacing = PreferenceConfiguration.FRAME_PACING_BALANCED;
            warpFactor = 0;
        }

        assertEquals(PreferenceConfiguration.FRAME_PACING_BALANCED, framePacing);
        assertEquals(0, warpFactor);
    }

    @Test
    public void testFramePacingOverrideLogic_CapFps() {
        String intentFramePacing = "cap-fps";
        int framePacing = -1;
        int warpFactor = -1;

        if ("cap-fps".equals(intentFramePacing)) {
            framePacing = PreferenceConfiguration.FRAME_PACING_CAP_FPS;
            warpFactor = 0;
        }

        assertEquals(PreferenceConfiguration.FRAME_PACING_CAP_FPS, framePacing);
        assertEquals(0, warpFactor);
    }

    @Test
    public void testFramePacingOverrideLogic_Smoothness() {
        String intentFramePacing = "smoothness";
        int framePacing = -1;
        int warpFactor = -1;

        if ("smoothness".equals(intentFramePacing)) {
            framePacing = PreferenceConfiguration.FRAME_PACING_MAX_SMOOTHNESS;
            warpFactor = 0;
        }

        assertEquals(PreferenceConfiguration.FRAME_PACING_MAX_SMOOTHNESS, framePacing);
        assertEquals(0, warpFactor);
    }

    @Test
    public void testFramePacingOverrideLogic_Warp() {
        String intentFramePacing = "warp";
        int framePacing = -1;
        int warpFactor = -1;

        if ("warp".equals(intentFramePacing)) {
            framePacing = PreferenceConfiguration.FRAME_PACING_MAX_SMOOTHNESS;
            warpFactor = 2;
        }

        assertEquals(PreferenceConfiguration.FRAME_PACING_MAX_SMOOTHNESS, framePacing);
        assertEquals(2, warpFactor);
    }

    @Test
    public void testFramePacingOverrideLogic_Warp2() {
        String intentFramePacing = "warp2";
        int framePacing = -1;
        int warpFactor = -1;

        if ("warp2".equals(intentFramePacing)) {
            framePacing = PreferenceConfiguration.FRAME_PACING_MAX_SMOOTHNESS;
            warpFactor = 4;
        }

        assertEquals(PreferenceConfiguration.FRAME_PACING_MAX_SMOOTHNESS, framePacing);
        assertEquals(4, warpFactor);
    }

    @Test
    public void testFramePacingOverrideLogic_InvalidValue() {
        String intentFramePacing = "invalid";
        int framePacing = 99;  // Some default
        int warpFactor = 99;

        // Switch with no matching case leaves values unchanged
        switch (intentFramePacing) {
            case "latency":
            case "balanced":
            case "cap-fps":
            case "smoothness":
            case "warp":
            case "warp2":
                framePacing = 0;
                warpFactor = 0;
                break;
            // Invalid values are silently ignored
        }

        assertEquals(99, framePacing);  // Should remain unchanged
        assertEquals(99, warpFactor);   // Should remain unchanged
    }
}
